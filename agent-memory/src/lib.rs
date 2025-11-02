//! Memory interfaces for agents.
//!
//! Phase 0 scaffolding: concrete stores arrive in later phases.

#![warn(missing_docs, clippy::pedantic)]

pub mod volatile {
    //! In-process volatile memory primitives.
}

pub mod journal {
    //! Episodic journal persistence.
}

pub mod vector_store_api {
    //! MXP Vector Store bindings.
}

pub mod embeddings {
    //! Embedding generation interfaces.
}
