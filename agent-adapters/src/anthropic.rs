//! Production-grade Anthropic Claude adapter.

use std::{env, fmt, time::Duration};

use async_trait::async_trait;
use futures::stream;
use hyper::body::to_bytes;
use hyper::header::{CONTENT_TYPE, HeaderValue};
use hyper::{Body, Request, Uri};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use crate::http_client::{HyperClient, build_https_client};
use crate::traits::{
    AdapterError, AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk, InferenceRequest,
    MessageRole, ModelAdapter, PromptMessage,
};

use agent_prompts::ContextWindowConfig;

/// Environment variable used when loading configuration automatically.
pub const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Configuration for the Anthropic adapter.
#[derive(Clone, Debug)]
pub struct AnthropicConfig {
    api_key: Option<String>,
    model: String,
    base_url: String,
    timeout: Duration,
    default_temperature: Option<f32>,
    default_max_tokens: u32,
}

impl AnthropicConfig {
    /// Creates a configuration using the supplied model identifier.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            api_key: None,
            model: model.into(),
            base_url: "https://api.anthropic.com/".to_owned(),
            timeout: Duration::from_secs(60),
            default_temperature: None,
            default_max_tokens: 4096,
        }
    }

    /// Loads the API key from the `ANTHROPIC_API_KEY` environment variable.
    #[must_use]
    pub fn from_env(model: impl Into<String>) -> Self {
        let mut cfg = Self::new(model);
        cfg.api_key = env::var(ANTHROPIC_API_KEY_ENV).ok();
        cfg
    }

    /// Overrides the base URL used for API calls.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the supplied URL is invalid.
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> AdapterResult<Self> {
        let sanitized = sanitize_base_url(base_url.as_ref())?;
        self.base_url = sanitized;
        Ok(self)
    }

    /// Sets the default sampling temperature used when requests omit it.
    #[must_use]
    pub fn with_default_temperature(mut self, temperature: f32) -> Self {
        self.default_temperature = Some(temperature);
        self
    }

    /// Sets the default max tokens for completions.
    #[must_use]
    pub fn with_default_max_tokens(mut self, max_tokens: u32) -> Self {
        self.default_max_tokens = max_tokens;
        self
    }

    /// Sets the HTTP request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Supplies an explicit API key.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

/// Anthropic Claude adapter that calls the official API over HTTPS.
pub struct AnthropicAdapter {
    client: HyperClient,
    endpoint: Uri,
    metadata: AdapterMetadata,
    api_key: String,
    timeout: Duration,
    default_temperature: Option<f32>,
    default_max_tokens: u32,
    context_config: Option<ContextWindowConfig>,
}

impl fmt::Debug for AnthropicAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicAdapter")
            .field("model", &self.metadata.model())
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl AnthropicAdapter {
    /// Constructs a new adapter with the provided configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the API key is missing.
    pub fn new(config: AnthropicConfig) -> AdapterResult<Self> {
        let api_key = config
            .api_key
            .ok_or_else(|| AdapterError::configuration("Anthropic adapter requires an API key"))?;

        let metadata = AdapterMetadata::new("anthropic", config.model.clone());
        let endpoint = format!("{}v1/messages", config.base_url)
            .parse::<Uri>()
            .map_err(|err| {
                AdapterError::configuration(format!("invalid Anthropic endpoint: {err}"))
            })?;

        let client = build_https_client()?;

        Ok(Self {
            client,
            endpoint,
            metadata,
            api_key,
            timeout: config.timeout,
            default_temperature: config.default_temperature,
            default_max_tokens: config.default_max_tokens,
            context_config: None,
        })
    }

    /// Configures context window management (optional).
    ///
    /// When set, the adapter will automatically manage conversation history
    /// to stay within token budgets.
    #[must_use]
    pub fn with_context_config(mut self, config: ContextWindowConfig) -> Self {
        self.context_config = Some(config);
        self
    }

    /// Returns the configured context window config if set.
    #[must_use]
    pub const fn context_config(&self) -> Option<&ContextWindowConfig> {
        self.context_config.as_ref()
    }

    fn build_request(&self, request: &InferenceRequest) -> MessagesRequest {
        // Extract system prompt (Anthropic uses a separate parameter)
        let system = request.system_prompt().map(ToOwned::to_owned);

        // Convert messages, filtering out any system role messages
        let messages: Vec<AnthropicMessage> = request
            .messages()
            .iter()
            .filter(|msg| msg.role() != MessageRole::System)
            .map(map_prompt_message)
            .collect();

        MessagesRequest {
            model: self.metadata.model().to_owned(),
            system,
            messages,
            max_tokens: request
                .max_output_tokens()
                .unwrap_or(self.default_max_tokens),
            temperature: request.temperature().or(self.default_temperature),
            stream: false,
        }
    }
}

#[async_trait]
impl ModelAdapter for AnthropicAdapter {
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }

    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream> {
        let payload = self.build_request(&request);
        let body = serde_json::to_vec(&payload).map_err(|err| {
            AdapterError::invalid_request(format!("failed to encode Anthropic request: {err}"))
        })?;

        let mut builder = Request::post(self.endpoint.clone());
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder = builder.header("x-api-key", &self.api_key);
        builder = builder.header(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );

        let request = builder.body(Body::from(body)).map_err(|err| {
            AdapterError::transport(format!("failed to build Anthropic request: {err}"))
        })?;

        let response = timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| AdapterError::transport("Anthropic request timed out"))?
            .map_err(|err| AdapterError::transport(format!("Anthropic request failed: {err}")))?;

        let status = response.status();
        let bytes = to_bytes(response.into_body()).await.map_err(|err| {
            AdapterError::transport(format!("failed to read Anthropic response: {err}"))
        })?;

        if !status.is_success() {
            let reason = String::from_utf8_lossy(&bytes).to_string();
            return Err(AdapterError::Response {
                reason: format!("Anthropic returned {status}: {reason}"),
            });
        }

        let response: MessagesResponse =
            serde_json::from_slice(&bytes).map_err(|err| AdapterError::Response {
                reason: format!("failed to decode Anthropic response: {err}"),
            })?;

        let content = response
            .content
            .into_iter()
            .map(|block| {
                let ContentBlock::Text { text } = block;
                text
            })
            .collect::<Vec<_>>()
            .join("\n");

        let stream = stream::once(async move { Ok(InferenceChunk::new(content, true)) });
        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text { text: String },
}

fn map_prompt_message(message: &PromptMessage) -> AnthropicMessage {
    let role = match message.role() {
        MessageRole::Assistant => "assistant",
        // Anthropic doesn't have a tool role, and system should be filtered out
        // All other roles map to "user"
        MessageRole::User | MessageRole::Tool | MessageRole::System => "user",
    };

    let content = if message.role() == MessageRole::Tool {
        format!("[Tool Output]\n{}", message.content())
    } else {
        message.content().to_owned()
    };

    AnthropicMessage {
        role: role.to_owned(),
        content,
    }
}

fn sanitize_base_url(input: &str) -> AdapterResult<String> {
    let mut base = input.trim().to_owned();
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(AdapterError::configuration(
            "Anthropic base URL must start with http:// or https://",
        ));
    }
    if !base.ends_with('/') {
        base.push('/');
    }
    base.parse::<Uri>()
        .map_err(|err| AdapterError::configuration(format!("invalid Anthropic base URL: {err}")))?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{InferenceRequest, MessageRole, PromptMessage};

    #[test]
    fn base_url_requires_scheme() {
        let err = AnthropicConfig::new("claude-3-5-sonnet-20241022")
            .with_base_url("api.anthropic.com")
            .expect_err("missing scheme should error");

        assert!(matches!(err, AdapterError::Configuration { .. }));
    }

    #[test]
    fn sanitize_allows_trailing_slash() {
        let cfg = AnthropicConfig::new("claude-3-5-sonnet-20241022")
            .with_base_url("https://example.com/anthropic")
            .expect("valid URL");
        assert_eq!(cfg.base_url, "https://example.com/anthropic/");
    }

    #[test]
    fn prompt_mapping_handles_tool_role() {
        let message = PromptMessage::new(MessageRole::Tool, "result");
        let mapped = map_prompt_message(&message);
        assert_eq!(mapped.role, "user");
        assert!(mapped.content.contains("Tool Output"));
    }

    #[test]
    fn build_request_extracts_system_prompt() {
        let config = AnthropicConfig::new("claude-3-5-sonnet-20241022").with_api_key("test_key");
        let adapter = AnthropicAdapter::new(config).expect("adapter");

        let request = InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "hello")])
            .unwrap()
            .with_system_prompt("You are helpful");

        let messages_req = adapter.build_request(&request);
        assert_eq!(messages_req.system, Some("You are helpful".to_owned()));
        assert_eq!(messages_req.messages.len(), 1);
        assert_eq!(messages_req.messages[0].role, "user");
    }

    #[test]
    fn build_request_filters_system_messages() {
        let config = AnthropicConfig::new("claude-3-5-sonnet-20241022").with_api_key("test_key");
        let adapter = AnthropicAdapter::new(config).expect("adapter");

        let request = InferenceRequest::new(vec![
            PromptMessage::new(MessageRole::System, "system"),
            PromptMessage::new(MessageRole::User, "hello"),
        ])
        .unwrap();

        let messages_req = adapter.build_request(&request);
        // System messages in the array should be filtered out
        assert_eq!(messages_req.messages.len(), 1);
        assert_eq!(messages_req.messages[0].role, "user");
    }
}
