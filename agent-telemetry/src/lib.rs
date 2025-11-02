//! Observability utilities for agents.
//!
//! Phase 0 scaffolding: concrete exporters arrive later.

#![warn(missing_docs, clippy::pedantic)]

pub mod tracing_support {
    //! Structured tracing helpers.
}

pub mod metrics {
    //! Metrics exporter configuration.
}

pub mod replay {
    //! Replay and deterministic debugging utilities.
}

pub mod health {
    //! Health reporting utilities.
}
