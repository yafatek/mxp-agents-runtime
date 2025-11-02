//! Core shared types and traits for MXP-native agents.

#![warn(missing_docs, clippy::pedantic)]

mod capability;
mod error;
mod ids;
mod manifest;

/// Capability descriptors and supporting builders.
pub use capability::{Capability, CapabilityBuilder, CapabilityId};
/// Error type and result alias shared across the SDK.
pub use error::{Error, Result};
/// Unique identifier for MXP agents within the mesh.
pub use ids::AgentId;
/// Agent metadata advertised to the Relay mesh directory.
pub use manifest::{AgentManifest, AgentManifestBuilder};
