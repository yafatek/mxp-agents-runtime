//! Prompt orchestration utilities for agents.
//!
//! Provides code-based templates, context window management, and compression strategies
//! to optimize token usage while maintaining conversation history.

#![warn(missing_docs, clippy::pedantic)]

pub mod context;
pub mod template;

pub mod validators {
    //! Schema validation for prompts and outputs.
}

pub mod guardrails {
    //! Guardrail enforcement and alignment policies.
}

// Re-export commonly used types
pub use context::{
    ContextError, ContextMessage, ContextResult, ContextWindowConfig, ContextWindowManager,
};
pub use template::{PromptTemplate, TemplateBuilder, TemplateError, TemplateResult};
