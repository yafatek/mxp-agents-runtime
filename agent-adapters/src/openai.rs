//! Production-grade `OpenAI` adapter.

use std::{env, fmt, time::Duration};

use async_trait::async_trait;
use futures::stream;
use hyper::body::to_bytes;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE};
use hyper::{Body, Request, Uri};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use crate::http_client::{HyperClient, build_https_client};
use crate::traits::{
    AdapterError, AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk, InferenceRequest,
    ModelAdapter, PromptMessage,
};

/// Environment variable used when loading configuration automatically.
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";

/// Configuration for the `OpenAI` adapter.
#[derive(Clone, Debug)]
pub struct OpenAiConfig {
    api_key: Option<String>,
    model: String,
    base_url: String,
    timeout: Duration,
    default_temperature: Option<f32>,
}

impl OpenAiConfig {
    /// Creates a configuration using the supplied model identifier.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            api_key: None,
            model: model.into(),
            base_url: "https://api.openai.com/".to_owned(),
            timeout: Duration::from_secs(60),
            default_temperature: None,
        }
    }

    /// Loads the API key from the `OPENAI_API_KEY` environment variable.
    #[must_use]
    pub fn from_env(model: impl Into<String>) -> Self {
        let mut cfg = Self::new(model);
        cfg.api_key = env::var(OPENAI_API_KEY_ENV).ok();
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

/// `OpenAI` adapter that calls the official API over HTTPS.
pub struct OpenAiAdapter {
    client: HyperClient,
    endpoint: Uri,
    metadata: AdapterMetadata,
    api_key: String,
    timeout: Duration,
    default_temperature: Option<f32>,
}

impl fmt::Debug for OpenAiAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiAdapter")
            .field("model", &self.metadata.model())
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl OpenAiAdapter {
    /// Constructs a new adapter with the provided configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the API key is missing.
    pub fn new(config: OpenAiConfig) -> AdapterResult<Self> {
        let api_key = config
            .api_key
            .ok_or_else(|| AdapterError::configuration("OpenAI adapter requires an API key"))?;

        let metadata = AdapterMetadata::new("openai", config.model.clone());
        let endpoint = format!("{}v1/chat/completions", config.base_url)
            .parse::<Uri>()
            .map_err(|err| {
                AdapterError::configuration(format!("invalid OpenAI endpoint: {err}"))
            })?;

        let client = build_https_client()?;

        Ok(Self {
            client,
            endpoint,
            metadata,
            api_key,
            timeout: config.timeout,
            default_temperature: config.default_temperature,
        })
    }

    fn build_request(&self, request: &InferenceRequest) -> ChatCompletionRequest {
        let messages = request.messages().iter().map(map_prompt_message).collect();

        ChatCompletionRequest {
            model: self.metadata.model().to_owned(),
            messages,
            temperature: request.temperature().or(self.default_temperature),
            max_tokens: request.max_output_tokens(),
            stream: false,
        }
    }
}

#[async_trait]
impl ModelAdapter for OpenAiAdapter {
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }

    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream> {
        let payload = self.build_request(&request);
        let body = serde_json::to_vec(&payload).map_err(|err| {
            AdapterError::invalid_request(format!("failed to encode OpenAI request: {err}"))
        })?;

        let mut builder = Request::post(self.endpoint.clone());
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder = builder.header(AUTHORIZATION, format!("Bearer {}", self.api_key));

        let request = builder.body(Body::from(body)).map_err(|err| {
            AdapterError::transport(format!("failed to build OpenAI request: {err}"))
        })?;

        let response = timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| AdapterError::transport("OpenAI request timed out"))?
            .map_err(|err| AdapterError::transport(format!("OpenAI request failed: {err}")))?;

        let status = response.status();
        let bytes = to_bytes(response.into_body()).await.map_err(|err| {
            AdapterError::transport(format!("failed to read OpenAI response: {err}"))
        })?;

        if !status.is_success() {
            let reason = String::from_utf8_lossy(&bytes).to_string();
            return Err(AdapterError::Response {
                reason: format!("OpenAI returned {status}: {reason}"),
            });
        }

        let response: ChatCompletionResponse =
            serde_json::from_slice(&bytes).map_err(|err| AdapterError::Response {
                reason: format!("failed to decode OpenAI response: {err}"),
            })?;

        let content = response
            .choices
            .into_iter()
            .find_map(|choice| choice.message.and_then(|message| message.content))
            .unwrap_or_default();

        let stream = stream::once(async move { Ok(InferenceChunk::new(content, true)) });
        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "max_tokens")]
    max_tokens: Option<u32>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChoiceMessage>,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

fn map_prompt_message(message: &PromptMessage) -> OpenAiMessage {
    OpenAiMessage {
        role: message.role().to_string(),
        content: message.content().to_owned(),
    }
}

fn sanitize_base_url(input: &str) -> AdapterResult<String> {
    let mut base = input.trim().to_owned();
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(AdapterError::configuration(
            "OpenAI base URL must start with http:// or https://",
        ));
    }
    if !base.ends_with('/') {
        base.push('/');
    }
    base.parse::<Uri>()
        .map_err(|err| AdapterError::configuration(format!("invalid OpenAI base URL: {err}")))?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{InferenceRequest, MessageRole, PromptMessage};

    #[test]
    fn base_url_requires_scheme() {
        let err = OpenAiConfig::new("gpt-4")
            .with_base_url("api.openai.com")
            .expect_err("missing scheme should error");

        assert!(matches!(err, AdapterError::Configuration { .. }));
    }

    #[test]
    fn sanitize_allows_trailing_slash() {
        let cfg = OpenAiConfig::new("gpt-4")
            .with_base_url("https://example.com/openai")
            .expect("valid URL");
        assert_eq!(cfg.base_url, "https://example.com/openai/");
    }

    #[test]
    fn prompt_mapping_preserves_role() {
        let message = PromptMessage::new(MessageRole::User, "hello");
        let mapped = map_prompt_message(&message);
        assert_eq!(mapped.role, "user");
        assert_eq!(mapped.content, "hello");
    }

    #[test]
    fn response_parsing_extracts_content() {
        let json = r#"{
            "choices": [
                { "message": { "content": "hi" } }
            ]
        }"#;

        let parsed: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let content = parsed
            .choices
            .into_iter()
            .find_map(|choice| choice.message.and_then(|msg| msg.content))
            .unwrap();

        assert_eq!(content, "hi");
    }

    #[test]
    fn build_request_uses_defaults() {
        let config = OpenAiConfig::new("gpt-4")
            .with_default_temperature(0.2)
            .with_api_key("test_key");
        let adapter = OpenAiAdapter::new(config).expect("adapter");
        let request = InferenceRequest::new(vec![
            PromptMessage::new(MessageRole::System, "system"),
            PromptMessage::new(MessageRole::User, "hello"),
        ])
        .unwrap();

        let chat = adapter.build_request(&request);
        assert_eq!(chat.model, adapter.metadata.model());
        assert_eq!(chat.messages.len(), 2);
        assert!(chat.temperature.is_some());
    }
}
