//! `Ollama` adapter implementation.

use std::{fmt, time::Duration};

use async_trait::async_trait;
use futures::stream;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Request, Uri};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use crate::http_client::{HyperClient, build_https_client};
use crate::traits::{
    AdapterError, AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk, InferenceRequest,
    MessageRole, ModelAdapter, PromptMessage,
};

/// Configuration for the `Ollama` adapter.
#[derive(Clone, Debug)]
pub struct OllamaConfig {
    base_url: String,
    model: String,
    default_temperature: Option<f32>,
    timeout: Duration,
}

impl OllamaConfig {
    /// Creates a configuration for the supplied model using default settings.
    ///
    /// # Panics
    ///
    /// Panics if the built-in default base URL is invalid. The default value is
    /// constant and verified during development.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            base_url: "http://127.0.0.1:11434/".to_owned(),
            model: model.into(),
            default_temperature: None,
            timeout: Duration::from_secs(60),
        }
    }

    /// Overrides the base URL of the local Ollama daemon.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the supplied URL is invalid.
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> AdapterResult<Self> {
        let sanitized = sanitize_base_url(base_url.as_ref())?;
        self.base_url = sanitized;
        Ok(self)
    }

    /// Sets the default sampling temperature used when the request does not
    /// provide one explicitly.
    #[must_use]
    pub fn with_default_temperature(mut self, temperature: f32) -> Self {
        self.default_temperature = Some(temperature);
        self
    }

    /// Sets the HTTP timeout for requests to the Ollama daemon.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// `Ollama` adapter that calls the local Ollama daemon over HTTP/HTTPS.
pub struct OllamaAdapter {
    client: HyperClient,
    endpoint: Uri,
    metadata: AdapterMetadata,
    timeout: Duration,
    default_temperature: Option<f32>,
}

impl fmt::Debug for OllamaAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OllamaAdapter")
            .field("model", &self.metadata.model())
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl OllamaAdapter {
    /// Constructs a new adapter from the supplied configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the endpoint is invalid or the HTTP
    /// client cannot be constructed.
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(config: OllamaConfig) -> AdapterResult<Self> {
        let endpoint = format!("{}api/chat", config.base_url)
            .parse::<Uri>()
            .map_err(|err| {
                AdapterError::configuration(format!("invalid Ollama endpoint: {err}"))
            })?;

        let client = build_https_client()?;
        let metadata = AdapterMetadata::new("ollama", config.model.clone());

        Ok(Self {
            client,
            endpoint,
            metadata,
            timeout: config.timeout,
            default_temperature: config.default_temperature,
        })
    }

    fn build_request(&self, request: &InferenceRequest) -> ChatRequest {
        let messages = request.messages().iter().map(map_prompt_message).collect();

        let options = if request.temperature().is_some()
            || self.default_temperature.is_some()
            || request.max_output_tokens().is_some()
        {
            Some(ChatOptions {
                temperature: request.temperature().or(self.default_temperature),
                max_output_tokens: request.max_output_tokens(),
            })
        } else {
            None
        };

        ChatRequest {
            model: self.metadata.model().to_owned(),
            stream: false,
            messages,
            options,
        }
    }
}

#[async_trait]
impl ModelAdapter for OllamaAdapter {
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }

    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream> {
        let payload = self.build_request(&request);
        let body = serde_json::to_vec(&payload).map_err(|err| {
            AdapterError::invalid_request(format!("failed to encode Ollama request: {err}"))
        })?;

        let req = Request::post(self.endpoint.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .map_err(|err| {
                AdapterError::transport(format!("failed to build Ollama request: {err}"))
            })?;

        let response = timeout(self.timeout, self.client.request(req))
            .await
            .map_err(|_| AdapterError::transport("Ollama request timed out"))?
            .map_err(|err| AdapterError::transport(format!("Ollama request failed: {err}")))?;

        let status = response.status();
        let bytes = to_bytes(response.into_body()).await.map_err(|err| {
            AdapterError::transport(format!("failed to read Ollama response: {err}"))
        })?;

        if !status.is_success() {
            let reason = String::from_utf8_lossy(&bytes).to_string();
            return Err(AdapterError::Response {
                reason: format!("Ollama returned {status}: {reason}"),
            });
        }

        let response: ChatResponse =
            serde_json::from_slice(&bytes).map_err(|err| AdapterError::Response {
                reason: format!("failed to decode Ollama response: {err}"),
            })?;

        if let Some(error) = response.error {
            return Err(AdapterError::Response { reason: error });
        }

        let content = response
            .message
            .map(|message| message.content)
            .or(response.response)
            .unwrap_or_default();

        let stream = stream::once(async move { Ok(InferenceChunk::new(content, true)) });
        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    stream: bool,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ChatOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "num_predict")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    message: Option<ChatMessage>,
    #[serde(default)]
    response: Option<String>,
    #[serde(default, rename = "done")]
    _done: bool,
    #[serde(default)]
    error: Option<String>,
}

fn map_prompt_message(message: &PromptMessage) -> ChatMessage {
    match message.role() {
        MessageRole::Tool => ChatMessage {
            role: "user".to_owned(),
            content: format!("[tool output] {}", message.content()),
        },
        role => ChatMessage {
            role: role.to_string(),
            content: message.content().to_owned(),
        },
    }
}

fn sanitize_base_url(input: &str) -> AdapterResult<String> {
    let mut base = input.trim().to_owned();
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(AdapterError::configuration(
            "Ollama base URL must start with http:// or https://",
        ));
    }
    if !base.ends_with('/') {
        base.push('/');
    }
    base.parse::<Uri>()
        .map_err(|err| AdapterError::configuration(format!("invalid Ollama base URL: {err}")))?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{InferenceRequest, MessageRole, PromptMessage};

    #[test]
    fn rejects_base_url_without_scheme() {
        let err = OllamaConfig::new("gemma")
            .with_base_url("localhost:11434")
            .expect_err("missing scheme should error");
        assert!(matches!(err, AdapterError::Configuration { .. }));
    }

    #[test]
    fn sanitize_adds_trailing_slash() {
        let cfg = OllamaConfig::new("gemma")
            .with_base_url("http://localhost:11434")
            .expect("valid url");
        assert_eq!(cfg.base_url, "http://localhost:11434/");
    }

    #[test]
    fn prompt_mapping_handles_tool_role() {
        let message = PromptMessage::new(MessageRole::Tool, "output");
        let mapped = map_prompt_message(&message);
        assert_eq!(mapped.role, "user");
        assert!(mapped.content.contains("tool output"));
    }

    #[test]
    fn chat_response_parsing_prefers_message() {
        let json = r#"{
            "message": {"role": "assistant", "content": "hi"},
            "response": "ignored"
        }"#;

        let parsed: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.message.unwrap().content, "hi");
    }

    #[test]
    fn build_request_respects_defaults() {
        let config = OllamaConfig::new("gemma").with_default_temperature(0.1);
        let adapter = OllamaAdapter::new(config).expect("adapter");
        let request =
            InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "hello")]).unwrap();

        let chat = adapter.build_request(&request);
        assert_eq!(chat.model, adapter.metadata.model());
        assert_eq!(chat.messages.len(), 1);
        assert!(chat.options.is_some());
    }
}
