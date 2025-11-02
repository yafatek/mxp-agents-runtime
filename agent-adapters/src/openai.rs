//! Mocked `OpenAI` adapter implementation.

use std::env;

use async_trait::async_trait;
use futures::stream;

use crate::traits::{
    AdapterError, AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk, InferenceRequest,
    MessageRole, ModelAdapter,
};

/// Environment variable used when loading configuration automatically.
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";

/// Configuration for the `OpenAI` adapter.
#[derive(Clone, Debug)]
pub struct OpenAiConfig {
    api_key: Option<String>,
    model: String,
    default_temperature: f32,
    mock_responses: bool,
}

impl OpenAiConfig {
    /// Creates a configuration using the supplied model identifier.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            api_key: None,
            model: model.into(),
            default_temperature: 0.0,
            mock_responses: false,
        }
    }

    /// Loads the API key from the `OPENAI_API_KEY` environment variable.
    #[must_use]
    pub fn from_env(model: impl Into<String>) -> Self {
        let mut cfg = Self::new(model);
        cfg.api_key = env::var(OPENAI_API_KEY_ENV).ok();
        cfg
    }

    /// Enables mocked responses for offline operation.
    #[must_use]
    pub const fn with_mock_responses(mut self, enabled: bool) -> Self {
        self.mock_responses = enabled;
        self
    }

    /// Sets the default sampling temperature.
    #[must_use]
    pub const fn with_default_temperature(mut self, temperature: f32) -> Self {
        self.default_temperature = temperature;
        self
    }

    /// Supplies an explicit API key.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

/// `OpenAI` adapter emitting deterministic mock responses for now.
#[derive(Clone, Debug)]
pub struct OpenAiAdapter {
    config: OpenAiConfig,
    metadata: AdapterMetadata,
}

impl OpenAiAdapter {
    /// Constructs a new adapter with the provided configuration.
    #[must_use]
    pub fn new(config: OpenAiConfig) -> Self {
        let metadata = AdapterMetadata::new("openai", config.model.clone());
        Self { config, metadata }
    }

    fn ensure_ready(&self) -> AdapterResult<()> {
        if self.config.mock_responses || self.config.api_key.is_some() {
            return Ok(());
        }

        Err(AdapterError::configuration(
            "OpenAI adapter requires an API key or mock responses",
        ))
    }
}

#[async_trait]
impl ModelAdapter for OpenAiAdapter {
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }

    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream> {
        self.ensure_ready()?;

        let temperature = request
            .temperature()
            .unwrap_or(self.config.default_temperature);

        let last_user_message = request
            .messages()
            .iter()
            .rev()
            .find(|message| message.role() == MessageRole::User)
            .map_or_else(
                || "(no user prompt provided)".to_owned(),
                |message| message.content().to_owned(),
            );

        let tools = if request.tools().is_empty() {
            String::new()
        } else {
            format!(" tools={:?}", request.tools())
        };

        let response = format!(
            "[mocked-openai:{} temp={:.2}{}] {}",
            self.metadata.model(),
            temperature,
            tools,
            last_user_message,
        );

        let stream = stream::iter([
            Ok(InferenceChunk::new(response, false)),
            Ok(InferenceChunk::new(String::new(), true)),
        ]);

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;
    use crate::traits::{InferenceRequest, MessageRole, PromptMessage};

    #[tokio::test]
    async fn produces_mock_stream() {
        let adapter = OpenAiAdapter::new(OpenAiConfig::new("gpt-mock").with_mock_responses(true));

        let request = InferenceRequest::new(vec![
            PromptMessage::new(MessageRole::System, "You are helpful."),
            PromptMessage::new(MessageRole::User, "Ping"),
        ])
        .unwrap()
        .with_temperature(0.25)
        .with_tools(vec!["echo".into()]);

        let mut stream = adapter.infer(request).await.unwrap();
        let mut collected = Vec::new();
        while let Some(chunk) = stream.next().await.transpose().unwrap() {
            collected.push((chunk.delta, chunk.done));
        }

        assert_eq!(collected.len(), 2);
        assert!(!collected[0].1);
        assert!(collected[1].1);
        assert!(collected[0].0.contains("mocked-openai"));
    }

    #[tokio::test]
    async fn missing_api_key_errors_without_mocking() {
        let adapter = OpenAiAdapter::new(OpenAiConfig::new("gpt-mock"));
        let request =
            InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "Ping")]).unwrap();

        let err = adapter.infer(request).await;
        assert!(matches!(err, Err(AdapterError::Configuration { .. })));
    }
}
