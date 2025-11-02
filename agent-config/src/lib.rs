//! Configuration management for agents.
//!
//! Phase 0 scaffolding: concrete loaders and schema definitions to follow.

#![warn(missing_docs, clippy::pedantic)]

pub mod loader {
    //! Configuration loader implementations.
}

pub mod schema {
    //! Strongly typed configuration schemas.
}

pub mod sops {
    //! Secret management integrations (e.g., SOPS).
}
