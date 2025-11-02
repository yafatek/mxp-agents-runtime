//! MXP-native autonomous agent runtime SDK facade.
//!
//! Depend on this crate via `cargo add mxp-agents`. It bundles the internal runtime
//! crates behind feature flags so downstream users can enable or disable components
//! as needed for their agents.

#![warn(missing_docs, clippy::pedantic)]

/// Re-export shared primitives for convenience.
pub use agent_primitives as primitives;

/// Agent lifecycle runtime (enabled by `kernel` feature).
#[cfg(feature = "kernel")]
pub use agent_kernel as kernel;

/// LLM and service adapters (enabled by `adapters` feature).
#[cfg(feature = "adapters")]
pub use agent_adapters as adapters;

/// Tool registration and enforcement (enabled by `tools` feature).
#[cfg(feature = "tools")]
pub use agent_tools as tools;

/// Memory subsystem (enabled by `memory` feature).
#[cfg(feature = "memory")]
pub use agent_memory as memory;

/// Policy and governance (enabled by `policy` feature).
#[cfg(feature = "policy")]
pub use agent_policy as policy;

/// Observability and replay (enabled by `telemetry` feature).
#[cfg(feature = "telemetry")]
pub use agent_telemetry as telemetry;

/// Prompt orchestration (enabled by `prompts` feature).
#[cfg(feature = "prompts")]
pub use agent_prompts as prompts;

/// Configuration management (enabled by `config` feature).
#[cfg(feature = "config")]
pub use agent_config as config;
