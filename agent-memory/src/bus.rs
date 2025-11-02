//! Coordinates volatile memory, journal persistence, and vector store indexing.

use std::sync::Arc;

use serde_json::Value;

use crate::journal::Journal;
use crate::record::MemoryRecord;
use crate::vector_store_api::{VectorMatch, VectorPoint, VectorQuery, VectorStoreClient};
use crate::volatile::{VolatileConfig, VolatileMemory, VolatileStats};
use crate::{MemoryError, MemoryResult};

/// Builder for [`MemoryBus`] instances.
pub struct MemoryBusBuilder {
    volatile_config: VolatileConfig,
    journal: Option<Arc<dyn Journal>>,
    vector_store: Option<Arc<dyn VectorStoreClient>>,
}

impl MemoryBusBuilder {
    /// Starts a new builder using the supplied volatile config.
    #[must_use]
    pub fn new(volatile_config: VolatileConfig) -> Self {
        Self {
            volatile_config,
            journal: None,
            vector_store: None,
        }
    }

    /// Installs the journal implementation. This is required before calling [`build`](Self::build).
    #[must_use]
    pub fn with_journal(mut self, journal: Arc<dyn Journal>) -> Self {
        self.journal = Some(journal);
        self
    }

    /// Installs an optional vector store client.
    #[must_use]
    pub fn with_vector_store(mut self, store: Arc<dyn VectorStoreClient>) -> Self {
        self.vector_store = Some(store);
        self
    }

    /// Builds the [`MemoryBus`].
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::MissingJournal`] when no journal was provided.
    pub fn build(self) -> MemoryResult<MemoryBus> {
        let journal = self.journal.ok_or(MemoryError::MissingJournal)?;
        Ok(MemoryBus {
            volatile: Arc::new(VolatileMemory::new(self.volatile_config)),
            journal,
            vector_store: self.vector_store,
        })
    }
}

/// Central memory facade used by the runtime.
#[derive(Clone)]
pub struct MemoryBus {
    volatile: Arc<VolatileMemory>,
    journal: Arc<dyn Journal>,
    vector_store: Option<Arc<dyn VectorStoreClient>>,
}

impl MemoryBus {
    /// Creates a builder for a memory bus.
    #[must_use]
    pub fn builder(config: VolatileConfig) -> MemoryBusBuilder {
        MemoryBusBuilder::new(config)
    }

    /// Returns the underlying volatile store.
    #[must_use]
    pub fn volatile(&self) -> &Arc<VolatileMemory> {
        &self.volatile
    }

    /// Returns the configured journal.
    #[must_use]
    pub fn journal(&self) -> &Arc<dyn Journal> {
        &self.journal
    }

    /// Returns the configured vector store, if present.
    #[must_use]
    pub fn vector_store(&self) -> Option<&Arc<dyn VectorStoreClient>> {
        self.vector_store.as_ref()
    }

    /// Persists a record across all configured stores.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when writing to the journal or vector store
    /// fails.
    pub async fn record(&self, record: MemoryRecord) -> MemoryResult<()> {
        self.volatile.push(record.clone()).await;
        self.journal.append(&record).await?;

        if let (Some(store), Some(embedding)) = (&self.vector_store, record.embedding().cloned()) {
            let metadata = if record.metadata().is_empty() {
                Value::Null
            } else {
                Value::Object(record.metadata().clone())
            };

            let point = VectorPoint::new(record.id(), embedding)
                .with_metadata(metadata)
                .with_tags(record.tags().to_vec());
            store.upsert(point).await?;
        }

        Ok(())
    }

    /// Returns recent records from volatile memory.
    #[must_use]
    pub async fn recent(&self, limit: usize) -> Vec<MemoryRecord> {
        self.volatile.recent(limit).await
    }

    /// Reads the tail of the journal.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when reading or decoding entries from the
    /// journal fails.
    pub async fn journal_tail(&self, limit: usize) -> MemoryResult<Vec<MemoryRecord>> {
        self.journal.tail(limit).await
    }

    /// Queries the configured vector store.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::MissingVectorStore`] when the bus was not
    /// initialised with a vector store implementation.
    pub async fn recall(&self, query: VectorQuery) -> MemoryResult<Vec<VectorMatch>> {
        let store = self
            .vector_store
            .as_ref()
            .ok_or(MemoryError::MissingVectorStore)?;
        store.query(query).await
    }

    /// Returns utilisation statistics for the volatile store.
    #[must_use]
    pub async fn stats(&self) -> VolatileStats {
        self.volatile.stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::num::NonZeroUsize;

    use crate::journal::FileJournal;
    use crate::record::MemoryChannel;
    use crate::vector_store_api::LocalVectorStore;

    fn temp_path() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("memory-bus-{}.log", uuid::Uuid::new_v4()));
        path
    }

    #[tokio::test]
    async fn records_to_all_components() {
        let path = temp_path();
        let journal: Arc<dyn crate::journal::Journal> =
            Arc::new(FileJournal::open(&path).await.unwrap());
        let vector_store: Arc<dyn crate::vector_store_api::VectorStoreClient> =
            Arc::new(LocalVectorStore::new());

        let bus = MemoryBus::builder(VolatileConfig::new(NonZeroUsize::new(8).unwrap()))
            .with_journal(journal.clone())
            .with_vector_store(vector_store.clone())
            .build()
            .unwrap();

        let record = MemoryRecord::builder(MemoryChannel::Input, Bytes::from_static(b"hello"))
            .tag("mxp")
            .unwrap()
            .build()
            .unwrap();

        bus.record(record.clone()).await.unwrap();

        let recent = bus.recent(1).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].payload(), &Bytes::from_static(b"hello"));

        let journal_tail = bus.journal_tail(1).await.unwrap();
        assert_eq!(journal_tail.len(), 1);

        // Without an embedding the vector store should remain empty.
        let matches = bus
            .recall(VectorQuery::new(
                crate::embeddings::EmbeddingVector::new(vec![1.0]).unwrap(),
                NonZeroUsize::new(1).unwrap(),
            ))
            .await
            .unwrap();
        assert!(matches.is_empty());

        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    #[tokio::test]
    async fn missing_vector_store_errors() {
        let path = temp_path();
        let journal: Arc<dyn crate::journal::Journal> =
            Arc::new(FileJournal::open(&path).await.unwrap());
        let bus = MemoryBus::builder(VolatileConfig::default())
            .with_journal(journal.clone())
            .build()
            .unwrap();

        let err = bus
            .recall(VectorQuery::new(
                crate::embeddings::EmbeddingVector::new(vec![1.0]).unwrap(),
                NonZeroUsize::new(1).unwrap(),
            ))
            .await
            .expect_err("missing vector store should error");
        assert!(matches!(err, MemoryError::MissingVectorStore));

        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }
}
