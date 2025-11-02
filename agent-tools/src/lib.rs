//! Tool discovery and capability enforcement.
//!
//! Phase 0 scaffolding: macro expansion and runtime hooks land later.

#![warn(missing_docs, clippy::pedantic)]

pub mod macros {
    //! Proc-macro helpers (placeholder until macro crate is added).
}

pub mod registry {
    //! Runtime registry for tool metadata.
}

pub mod sandbox {
    //! Tool execution sandboxing components.
}
