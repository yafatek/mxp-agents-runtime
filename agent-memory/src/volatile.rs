//! In-memory volatile store backed by a bounded ring buffer.

use std::collections::VecDeque;
use std::num::NonZeroUsize;

use tokio::sync::RwLock;

use crate::record::MemoryRecord;

/// Configuration for the volatile memory buffer.
#[derive(Debug, Clone, Copy)]
pub struct VolatileConfig {
    capacity: NonZeroUsize,
    max_total_bytes: Option<NonZeroUsize>,
}

impl VolatileConfig {
    /// Creates a configuration with the provided capacity.
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            max_total_bytes: None,
        }
    }

    /// Sets the optional total byte ceiling for the buffer.
    #[must_use]
    pub fn with_max_total_bytes(mut self, max_total_bytes: NonZeroUsize) -> Self {
        self.max_total_bytes = Some(max_total_bytes);
        self
    }

    /// Returns the configured capacity.
    #[must_use]
    pub const fn capacity(self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the maximum total bytes, if configured.
    #[must_use]
    pub const fn max_total_bytes(self) -> Option<NonZeroUsize> {
        self.max_total_bytes
    }
}

impl Default for VolatileConfig {
    fn default() -> Self {
        Self {
            capacity: NonZeroUsize::new(256).expect("non-zero"),
            max_total_bytes: None,
        }
    }
}

#[derive(Debug, Default)]
struct VolatileInner {
    entries: VecDeque<MemoryRecord>,
    total_bytes: usize,
}

/// Volatile memory ring retaining the most recent records.
#[derive(Debug)]
pub struct VolatileMemory {
    config: VolatileConfig,
    inner: RwLock<VolatileInner>,
}

impl VolatileMemory {
    /// Creates a new buffer using the supplied configuration.
    #[must_use]
    pub fn new(config: VolatileConfig) -> Self {
        Self {
            config,
            inner: RwLock::new(VolatileInner {
                entries: VecDeque::with_capacity(config.capacity().get()),
                total_bytes: 0,
            }),
        }
    }

    /// Inserts a record, evicting the oldest entries if capacity constraints are exceeded.
    pub async fn push(&self, record: MemoryRecord) {
        let mut guard = self.inner.write().await;
        guard.total_bytes += record.payload().len();
        guard.entries.push_back(record);

        while guard.entries.len() > self.config.capacity().get() {
            if let Some(evicted) = guard.entries.pop_front() {
                guard.total_bytes = guard.total_bytes.saturating_sub(evicted.payload().len());
            }
        }

        if let Some(limit) = self.config.max_total_bytes() {
            let limit = limit.get();
            while guard.total_bytes > limit && guard.entries.len() > 1 {
                if let Some(evicted) = guard.entries.pop_front() {
                    guard.total_bytes = guard.total_bytes.saturating_sub(evicted.payload().len());
                }
            }
        }
    }

    /// Returns the most recent records up to the requested limit.
    #[must_use]
    pub async fn recent(&self, limit: usize) -> Vec<MemoryRecord> {
        let guard = self.inner.read().await;
        let take = limit.min(guard.entries.len());
        guard
            .entries
            .iter()
            .rev()
            .take(take)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Returns statistics about the buffer utilisation.
    #[must_use]
    pub async fn stats(&self) -> VolatileStats {
        let guard = self.inner.read().await;
        VolatileStats {
            entries: guard.entries.len(),
            total_bytes: guard.total_bytes,
            capacity: self.config.capacity().get(),
            max_total_bytes: self.config.max_total_bytes().map(NonZeroUsize::get),
        }
    }
}

/// Snapshot describing utilisation of the volatile buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolatileStats {
    /// Entries currently stored in the buffer.
    pub entries: usize,
    /// Accumulated payload bytes currently retained.
    pub total_bytes: usize,
    /// Maximum number of entries permitted.
    pub capacity: usize,
    /// Optional total byte limit when configured.
    pub max_total_bytes: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn respects_capacity() {
        let config = VolatileConfig::new(NonZeroUsize::new(2).unwrap());
        let memory = VolatileMemory::new(config);

        memory
            .push(
                MemoryRecord::builder(
                    crate::record::MemoryChannel::Input,
                    Bytes::from_static(b"one"),
                )
                .build()
                .unwrap(),
            )
            .await;
        memory
            .push(
                MemoryRecord::builder(
                    crate::record::MemoryChannel::Input,
                    Bytes::from_static(b"two"),
                )
                .build()
                .unwrap(),
            )
            .await;
        memory
            .push(
                MemoryRecord::builder(
                    crate::record::MemoryChannel::Input,
                    Bytes::from_static(b"three"),
                )
                .build()
                .unwrap(),
            )
            .await;

        let recent = memory.recent(10).await;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].payload(), &Bytes::from_static(b"two"));
        assert_eq!(recent[1].payload(), &Bytes::from_static(b"three"));
    }

    #[tokio::test]
    async fn respects_total_byte_limit() {
        let config = VolatileConfig::new(NonZeroUsize::new(10).unwrap())
            .with_max_total_bytes(NonZeroUsize::new(8).unwrap());
        let memory = VolatileMemory::new(config);

        for value in [b"aaaa", b"bbbb", b"cccc"] {
            memory
                .push(
                    MemoryRecord::builder(
                        crate::record::MemoryChannel::Input,
                        Bytes::copy_from_slice(value),
                    )
                    .build()
                    .unwrap(),
                )
                .await;
        }

        let stats = memory.stats().await;
        assert!(stats.total_bytes <= 8 || stats.entries == 1);
    }
}
