use crate::error::PromptResult;

/// Represents a system instruction applied to all downstream prompts.
#[derive(Debug, Clone, Default)]
pub struct SystemInstruction {
    content: String,
}

/// Builder for [`SystemInstruction`].
#[derive(Debug, Default)]
pub struct SystemInstructionBuilder {
    content: String,
}

impl SystemInstructionBuilder {
    /// Creates a new builder instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the instruction content.
    #[must_use]
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Builds the instruction.
    pub fn build(self) -> PromptResult<SystemInstruction> {
        Ok(SystemInstruction {
            content: self.content,
        })
    }
}

impl SystemInstruction {
    /// Returns the textual content.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

