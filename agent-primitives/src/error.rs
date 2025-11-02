//! Shared error definitions for agent primitives.

use thiserror::Error;
use uuid::Error as UuidError;

/// Result alias used throughout the agent runtime.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur while manipulating agent primitive types.
#[derive(Debug, Error)]
pub enum Error {
    /// The provided agent identifier could not be parsed.
    #[error("invalid agent id: {source}")]
    InvalidAgentId {
        /// Source parsing error from the UUID library.
        #[from]
        source: UuidError,
    },

    /// Capability identifier failed validation.
    #[error("invalid capability id `{id}`: {reason}")]
    InvalidCapabilityId {
        /// The offending identifier string.
        id: String,
        /// Human-readable reason for rejection.
        reason: String,
    },

    /// Capability definition failed validation.
    #[error("invalid capability: {reason}")]
    InvalidCapability {
        /// Human-readable reason for rejection.
        reason: String,
    },
}
