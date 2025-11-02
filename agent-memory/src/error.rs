//! Error types for the memory subsystem.

use serde_json::Error as SerdeError;
use thiserror::Error;

/// Errors emitted by memory components.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// The provided configuration was invalid.
    #[error("invalid memory configuration: {0}")]
    InvalidConfig(&'static str),
    /// Underlying I/O failure while reading or writing journal files.
    #[error("i/o error: {source}")]
    Io {
        /// Source [`std::io::Error`].
        #[from]
        source: std::io::Error,
    },
    /// Serialization or deserialization error.
    #[error("serialization error: {source}")]
    Serialization {
        /// Source [`serde_json::Error`].
        #[from]
        source: SerdeError,
    },
    /// Operation that requires a configured journal was invoked without one.
    #[error("memory journal not configured")]
    MissingJournal,
    /// Operation that requires a configured vector store was invoked without one.
    #[error("vector store client not configured")]
    MissingVectorStore,
    /// Vector store backend reported an application error.
    #[error("vector store error: {reason}")]
    VectorStore {
        /// Human-readable reason describing the failure.
        reason: String,
    },
    /// Memory record metadata failed validation.
    #[error("invalid memory record: {0}")]
    InvalidRecord(&'static str),
}

impl MemoryError {
    /// Helper to construct vector store errors from string-like values.
    #[must_use]
    pub fn vector_store(reason: impl Into<String>) -> Self {
        Self::VectorStore {
            reason: reason.into(),
        }
    }
}

/// Result type alias for memory operations.
pub type MemoryResult<T> = Result<T, MemoryError>;
