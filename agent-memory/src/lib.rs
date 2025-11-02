//! Memory subsystem building blocks used by MXP agents.

#![warn(missing_docs, clippy::pedantic)]

mod bus;
mod embeddings;
mod error;
mod journal;
mod record;
mod vector_store_api;
mod volatile;

pub use bus::{MemoryBus, MemoryBusBuilder};
pub use embeddings::EmbeddingVector;
pub use error::{MemoryError, MemoryResult};
pub use journal::{FileJournal, Journal};
pub use record::{MemoryChannel, MemoryRecord, MemoryRecordBuilder};
pub use vector_store_api::{
    LocalVectorStore, VectorMatch, VectorPoint, VectorQuery, VectorStoreClient,
};
pub use volatile::{VolatileConfig, VolatileMemory, VolatileStats};
