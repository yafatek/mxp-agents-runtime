use crate::error::{PromptError, PromptResult};

/// Coordinates prompt templates, system instructions, and context budgeting.
#[derive(Debug, Default)]
pub struct PromptManager;

/// Builder for [`PromptManager`].
#[derive(Debug, Default)]
pub struct PromptManagerBuilder;

impl PromptManagerBuilder {
    /// Creates a new builder with default settings.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Finalises builder construction.
    pub fn build(self) -> PromptResult<PromptManager> {
        Ok(PromptManager)
    }
}

impl PromptManager {
    /// Returns a new builder instance.
    #[must_use]
    pub fn builder() -> PromptManagerBuilder {
        PromptManagerBuilder::new()
    }

    /// Placeholder hook for validating prompt inputs.
    pub fn validate(&self) -> PromptResult<()> {
        Ok(())
    }
}

