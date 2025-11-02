//! `Ollama` adapter implementation.

use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

use crate::traits::{
    AdapterError, AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk, InferenceRequest,
    MessageRole, ModelAdapter, PromptMessage,
};

/// Configuration for the `Ollama` adapter.
#[derive(Clone, Debug)]
pub struct OllamaConfig {
    base_url: Url,
    model: String,
    default_temperature: Option<f32>,
    timeout: Duration,
    mock_responses: bool,
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
            base_url: Url::parse("http://127.0.0.1:11434/").expect("valid default base url"),
            model: model.into(),
            default_temperature: None,
            timeout: Duration::from_secs(60),
            mock_responses: false,
        }
    }

    /// Overrides the base URL of the local Ollama daemon.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the supplied URL is invalid.
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> AdapterResult<Self> {
        self.base_url = Url::parse(base_url.as_ref()).map_err(|err| {
            AdapterError::configuration(format!("invalid Ollama base url: {err}"))
        })?;
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

    /// Enables mocked responses for offline testing.
    #[must_use]
    pub fn with_mock_responses(mut self, enabled: bool) -> Self {
        self.mock_responses = enabled;
        self
    }
}

/// `Ollama` adapter that calls the local Ollama daemon over HTTP.
#[derive(Debug)]
pub struct OllamaAdapter {
    client: Client,
    config: OllamaConfig,
    metadata: AdapterMetadata,
    chat_endpoint: Url,
}

impl OllamaAdapter {
    /// Constructs a new adapter from the supplied configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the chat endpoint cannot be
    /// formed, or [`AdapterError::Transport`] if the HTTP client cannot be
    /// constructed.
    pub fn new(config: OllamaConfig) -> AdapterResult<Self> {
        let chat_endpoint = config
            .base_url
            .join("api/chat")
            .map_err(|err| AdapterError::configuration(format!("invalid chat endpoint: {err}")))?;

        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|err| {
                AdapterError::transport(format!("failed to build http client: {err}"))
            })?;

        let metadata = AdapterMetadata::new("ollama", config.model.clone());

        Ok(Self {
            client,
            config,
            metadata,
            chat_endpoint,
        })
    }

    fn mock_stream(&self, request: &InferenceRequest) -> AdapterStream {
        let fallback = "(no user prompt provided)".to_owned();
        let last_user_message = request
            .messages()
            .iter()
            .rev()
            .find(|message| message.role() == MessageRole::User)
            .map_or(fallback, |message| message.content().to_owned());

        let content = format!(
            "[mocked-ollama:{}] {}",
            self.metadata.model(),
            last_user_message
        );

        Box::pin(stream::iter([Ok(InferenceChunk::new(content, true))]))
    }

    fn to_chat_request(&self, request: &InferenceRequest) -> ChatRequest {
        let messages = request.messages().iter().map(map_prompt_message).collect();

        let temperature = request.temperature().or(self.config.default_temperature);

        let options = if temperature.is_some() || request.max_output_tokens().is_some() {
            Some(ChatOptions {
                temperature,
                max_output_tokens: request.max_output_tokens(),
            })
        } else {
            None
        };

        ChatRequest {
            model: self.config.model.clone(),
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
        if self.config.mock_responses {
            return Ok(self.mock_stream(&request));
        }

        let chat_request = self.to_chat_request(&request);

        let response = self
            .client
            .post(self.chat_endpoint.clone())
            .json(&chat_request)
            .send()
            .await
            .map_err(|err| AdapterError::transport(format!("ollama request failed: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unavailable>".to_owned());
            return Err(AdapterError::Response {
                reason: format!("ollama returned {status}: {body}"),
            });
        }

        let chat_response: ChatResponse =
            response
                .json()
                .await
                .map_err(|err| AdapterError::Response {
                    reason: format!("failed to decode ollama response: {err}"),
                })?;

        if let Some(error) = chat_response.error {
            return Err(AdapterError::Response { reason: error });
        }

        let content = chat_response
            .message
            .map(|message| message.content)
            .or(chat_response.response)
            .unwrap_or_default();

        Ok(Box::pin(stream::iter([Ok(InferenceChunk::new(
            content, true,
        ))])))
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

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn mock_responses_flow() {
        let config = OllamaConfig::new("gemma3").with_mock_responses(true);
        let adapter = OllamaAdapter::new(config).expect("adapter");

        let request = InferenceRequest::new(vec![
            PromptMessage::new(MessageRole::System, "You are terse."),
            PromptMessage::new(MessageRole::User, "Ping"),
        ])
        .unwrap();

        let mut stream = adapter.infer(request).await.unwrap();
        let mut collected = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            collected.push(chunk);
        }

        assert_eq!(collected.len(), 1);
        assert!(collected[0].delta.contains("mocked-ollama"));
        assert!(collected[0].done);
    }

    #[tokio::test]
    async fn transport_error_when_unavailable() {
        let config = OllamaConfig::new("gemma3")
            .with_mock_responses(false)
            .with_base_url("http://127.0.0.1:9/")
            .unwrap();
        let adapter = OllamaAdapter::new(config).expect("adapter");
        let request =
            InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "Ping")]).unwrap();

        let result = adapter.infer(request).await;

        assert!(matches!(
            result,
            Err(AdapterError::Transport { .. } | AdapterError::Response { .. })
        ));
    }
}
