//! Shared model adapter traits and data structures.

use std::fmt;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result alias used by model adapters.
pub type AdapterResult<T> = Result<T, AdapterError>;

/// Streaming response emitted by [`ModelAdapter::infer`].
pub type AdapterStream = Pin<Box<dyn Stream<Item = AdapterResult<InferenceChunk>> + Send>>;

/// Error type shared by adapter implementations.
#[derive(Debug, Error)]
pub enum AdapterError {
    /// Adapter is misconfigured or missing credentials.
    #[error("adapter not configured: {reason}")]
    Configuration {
        /// Additional context for the failure.
        reason: String,
    },

    /// The supplied request was invalid for the target model.
    #[error("invalid inference request: {reason}")]
    InvalidRequest {
        /// Reason describing why the request could not be processed.
        reason: String,
    },

    /// Transport-level failures (network, protocol, etc.).
    #[error("adapter transport error: {reason}")]
    Transport {
        /// Additional context about the error.
        reason: String,
    },

    /// The provider rejected the request due to rate limiting.
    #[error("adapter rate limited (retry after {retry_after:?})")]
    RateLimited {
        /// Suggested delay before retrying.
        retry_after: Option<Duration>,
    },

    /// The provider returned a malformed response.
    #[error("adapter response error: {reason}")]
    Response {
        /// Additional context about the response failure.
        reason: String,
    },
}

impl AdapterError {
    /// Convenience constructor for invalid requests.
    #[must_use]
    pub fn invalid_request(reason: impl Into<String>) -> Self {
        Self::InvalidRequest {
            reason: reason.into(),
        }
    }

    /// Convenience constructor for configuration issues.
    #[must_use]
    pub fn configuration(reason: impl Into<String>) -> Self {
        Self::Configuration {
            reason: reason.into(),
        }
    }

    /// Convenience constructor for transport failures.
    #[must_use]
    pub fn transport(reason: impl Into<String>) -> Self {
        Self::Transport {
            reason: reason.into(),
        }
    }
}

/// Minimal metadata describing a model adapter instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterMetadata {
    provider: &'static str,
    model: String,
    #[allow(dead_code)]
    version: Option<String>,
}

impl AdapterMetadata {
    /// Creates metadata for the supplied provider and model identifier.
    #[must_use]
    pub fn new(provider: &'static str, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            version: None,
        }
    }

    /// Sets the adapter version information.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Returns the provider identifier (e.g., "openai").
    #[must_use]
    pub const fn provider(&self) -> &'static str {
        self.provider
    }

    /// Returns the configured model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// Roles supported in chat-style prompts.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System messages steer the assistant behaviour.
    System,
    /// User-authored content.
    User,
    /// Assistant (model) responses.
    Assistant,
    /// Tool messages returned to the planner loop.
    Tool,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        })
    }
}

/// Represents an instruction or message in a chat-style prompt.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PromptMessage {
    role: MessageRole,
    content: String,
}

impl PromptMessage {
    /// Creates a new prompt message.
    #[must_use]
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    /// Returns the message role.
    #[must_use]
    pub const fn role(&self) -> MessageRole {
        self.role
    }

    /// Returns the message content.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Request submitted to a model adapter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct InferenceRequest {
    /// Optional system prompt that guides model behavior.
    /// Adapters will transform this to provider-specific formats:
    /// - OpenAI: Prepended as {"role": "system", "content": "..."}
    /// - Anthropic: Extracted to top-level "system" parameter
    /// - Gemini: Transformed to "systemInstruction"
    /// - Ollama: Prepended as {"role": "system", "content": "..."}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    /// Conversation messages (user, assistant, tool).
    messages: Vec<PromptMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tools: Vec<String>,
}

impl InferenceRequest {
    /// Creates a request with the supplied messages.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::InvalidRequest`] if the message list is empty.
    pub fn new(messages: Vec<PromptMessage>) -> AdapterResult<Self> {
        if messages.is_empty() {
            return Err(AdapterError::invalid_request(
                "inference request requires at least one message",
            ));
        }

        Ok(Self {
            system_prompt: None,
            messages,
            max_output_tokens: None,
            temperature: None,
            tools: Vec::new(),
        })
    }

    /// Sets the system prompt that guides model behavior.
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Sets the maximum output token budget.
    #[must_use]
    pub fn with_max_output_tokens(mut self, tokens: u32) -> Self {
        self.max_output_tokens = Some(tokens);
        self
    }

    /// Sets the sampling temperature.
    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Declares tool names that the adapter may invoke.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Returns the system prompt if configured.
    #[must_use]
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Returns the prompt messages.
    #[must_use]
    pub fn messages(&self) -> &[PromptMessage] {
        &self.messages
    }

    /// Returns the configured maximum output tokens.
    #[must_use]
    pub const fn max_output_tokens(&self) -> Option<u32> {
        self.max_output_tokens
    }

    /// Returns the configured sampling temperature.
    #[must_use]
    pub const fn temperature(&self) -> Option<f32> {
        self.temperature
    }

    /// Returns the declared tool names.
    #[must_use]
    pub fn tools(&self) -> &[String] {
        &self.tools
    }
}

/// Streaming chunk returned by the adapter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct InferenceChunk {
    /// Partial token delta emitted by the provider.
    pub delta: String,
    /// Whether the generation is complete.
    pub done: bool,
}

impl InferenceChunk {
    /// Creates a new chunk.
    #[must_use]
    pub fn new(delta: impl Into<String>, done: bool) -> Self {
        Self {
            delta: delta.into(),
            done,
        }
    }
}

/// Trait implemented by all model adapters.
#[async_trait]
pub trait ModelAdapter: Send + Sync {
    /// Returns basic metadata describing the adapter instance.
    fn metadata(&self) -> &AdapterMetadata;

    /// Executes the inference request, returning a streaming response.
    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_request_messages() {
        let err = InferenceRequest::new(Vec::new()).expect_err("messages required");
        assert!(matches!(err, AdapterError::InvalidRequest { .. }));
    }

    #[test]
    fn builds_request() {
        let request = InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "ping")])
            .unwrap()
            .with_max_output_tokens(256)
            .with_temperature(0.7)
            .with_tools(vec!["echo".to_owned()]);

        assert_eq!(request.messages().len(), 1);
        assert_eq!(request.max_output_tokens(), Some(256));
        assert_eq!(request.temperature(), Some(0.7));
        assert_eq!(request.tools(), &["echo".to_owned()]);
    }
}
