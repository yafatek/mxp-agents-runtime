//! Durable episodic memory journal implementations.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::MemoryResult;
use crate::record::MemoryRecord;

/// Trait implemented by durable journals.
#[async_trait]
pub trait Journal: Send + Sync {
    /// Appends a record to the journal.
    async fn append(&self, record: &MemoryRecord) -> MemoryResult<()>;

    /// Returns the most recent `limit` records, ordered oldest to newest.
    async fn tail(&self, limit: usize) -> MemoryResult<Vec<MemoryRecord>>;

    /// Clears the journal contents.
    async fn clear(&self) -> MemoryResult<()>;
}

/// File-backed journal writing newline-delimited JSON entries.
pub struct FileJournal {
    path: PathBuf,
    file: Mutex<tokio::fs::File>,
}

impl FileJournal {
    /// Opens (or creates) a journal file at the provided path.
    ///
    /// # Errors
    ///
    /// Propagates I/O and serialization errors encountered while preparing the
    /// file.
    pub async fn open(path: impl Into<PathBuf>) -> MemoryResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .await?;

        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    /// Returns the underlying path of the journal file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl Journal for FileJournal {
    async fn append(&self, record: &MemoryRecord) -> MemoryResult<()> {
        let line = serde_json::to_vec(record)?;
        let mut guard = self.file.lock().await;
        guard.write_all(&line).await?;
        guard.write_u8(b'\n').await?;
        guard.flush().await?;
        Ok(())
    }

    async fn tail(&self, limit: usize) -> MemoryResult<Vec<MemoryRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let data = fs::read(&self.path).await?;
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let mut records = Vec::new();
        for chunk in data
            .split(|byte| *byte == b'\n')
            .filter(|chunk| !chunk.is_empty())
        {
            let record: MemoryRecord = serde_json::from_slice(chunk)?;
            records.push(record);
        }

        if records.len() <= limit {
            return Ok(records);
        }

        let skip = records.len() - limit;
        Ok(records.into_iter().skip(skip).collect())
    }

    async fn clear(&self) -> MemoryResult<()> {
        let mut guard = self.file.lock().await;
        guard.rewind().await?;
        guard.set_len(0).await?;
        guard.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use uuid::Uuid;

    use crate::record::MemoryChannel;

    fn temp_path() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("memory-journal-{}.log", Uuid::new_v4()));
        path
    }

    #[tokio::test]
    async fn append_and_tail_roundtrip() {
        let path = temp_path();
        let journal = FileJournal::open(&path).await.unwrap();

        for content in ["one", "two", "three"] {
            let record = crate::record::MemoryRecord::builder(
                MemoryChannel::Input,
                Bytes::from_static(content.as_bytes()),
            )
            .build()
            .unwrap();
            journal.append(&record).await.unwrap();
        }

        let tail = journal.tail(2).await.unwrap();
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].payload(), &Bytes::from_static(b"two"));
        assert_eq!(tail[1].payload(), &Bytes::from_static(b"three"));

        journal.clear().await.unwrap();
        let empty = journal.tail(10).await.unwrap();
        assert!(empty.is_empty());

        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }
}
