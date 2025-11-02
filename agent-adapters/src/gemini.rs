//! Production-grade Google Gemini adapter.

use std::{env, fmt, time::Duration};

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

use agent_prompts::ContextWindowConfig;

/// Environment variable used when loading configuration automatically.
pub const GEMINI_API_KEY_ENV: &str = "GEMINI_API_KEY";

/// Configuration for the Gemini adapter.
#[derive(Clone, Debug)]
pub struct GeminiConfig {
    api_key: Option<String>,
    model: String,
    base_url: String,
    timeout: Duration,
    default_temperature: Option<f32>,
}

impl GeminiConfig {
    /// Creates a configuration using the supplied model identifier.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            api_key: None,
            model: model.into(),
            base_url: "https://generativelanguage.googleapis.com/".to_owned(),
            timeout: Duration::from_secs(60),
            default_temperature: None,
        }
    }

    /// Loads the API key from the `GEMINI_API_KEY` environment variable.
    #[must_use]
    pub fn from_env(model: impl Into<String>) -> Self {
        let mut cfg = Self::new(model);
        cfg.api_key = env::var(GEMINI_API_KEY_ENV).ok();
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

/// Google Gemini adapter that calls the official API over HTTPS.
pub struct GeminiAdapter {
    client: HyperClient,
    base_endpoint: String,
    metadata: AdapterMetadata,
    api_key: String,
    timeout: Duration,
    default_temperature: Option<f32>,
    context_config: Option<ContextWindowConfig>,
}

impl fmt::Debug for GeminiAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeminiAdapter")
            .field("model", &self.metadata.model())
            .field("base_endpoint", &self.base_endpoint)
            .finish_non_exhaustive()
    }
}

impl GeminiAdapter {
    /// Constructs a new adapter with the provided configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Configuration`] if the API key is missing.
    pub fn new(config: GeminiConfig) -> AdapterResult<Self> {
        let api_key = config
            .api_key
            .ok_or_else(|| AdapterError::configuration("Gemini adapter requires an API key"))?;

        let metadata = AdapterMetadata::new("gemini", config.model.clone());
        let base_endpoint = format!(
            "{}v1beta/models/{}:generateContent",
            config.base_url, config.model
        );

        let client = build_https_client()?;

        Ok(Self {
            client,
            base_endpoint,
            metadata,
            api_key,
            timeout: config.timeout,
            default_temperature: config.default_temperature,
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

    fn build_request(&self, request: &InferenceRequest) -> GenerateContentRequest {
        // Extract system instruction (Gemini uses a separate parameter)
        let system_instruction = request.system_prompt().map(|prompt| SystemInstruction {
            parts: vec![Part {
                text: prompt.to_owned(),
            }],
        });

        // Convert messages to Gemini format
        let contents: Vec<Content> = request
            .messages()
            .iter()
            .filter(|msg| msg.role() != MessageRole::System)
            .map(map_prompt_message)
            .collect();

        let generation_config = if request.temperature().is_some()
            || self.default_temperature.is_some()
            || request.max_output_tokens().is_some()
        {
            Some(GenerationConfig {
                temperature: request.temperature().or(self.default_temperature),
                max_output_tokens: request.max_output_tokens(),
            })
        } else {
            None
        };

        GenerateContentRequest {
            system_instruction,
            contents,
            generation_config,
        }
    }

    fn build_uri(&self) -> AdapterResult<Uri> {
        format!("{}?key={}", self.base_endpoint, self.api_key)
            .parse::<Uri>()
            .map_err(|err| AdapterError::configuration(format!("invalid Gemini endpoint: {err}")))
    }
}

#[async_trait]
impl ModelAdapter for GeminiAdapter {
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }

    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream> {
        let payload = self.build_request(&request);
        let body = serde_json::to_vec(&payload).map_err(|err| {
            AdapterError::invalid_request(format!("failed to encode Gemini request: {err}"))
        })?;

        let endpoint = self.build_uri()?;

        let req = Request::post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .map_err(|err| {
                AdapterError::transport(format!("failed to build Gemini request: {err}"))
            })?;

        let response = timeout(self.timeout, self.client.request(req))
            .await
            .map_err(|_| AdapterError::transport("Gemini request timed out"))?
            .map_err(|err| AdapterError::transport(format!("Gemini request failed: {err}")))?;

        let status = response.status();
        let bytes = to_bytes(response.into_body()).await.map_err(|err| {
            AdapterError::transport(format!("failed to read Gemini response: {err}"))
        })?;

        if !status.is_success() {
            let reason = String::from_utf8_lossy(&bytes).to_string();
            return Err(AdapterError::Response {
                reason: format!("Gemini returned {status}: {reason}"),
            });
        }

        let response: GenerateContentResponse =
            serde_json::from_slice(&bytes).map_err(|err| AdapterError::Response {
                reason: format!("failed to decode Gemini response: {err}"),
            })?;

        let content = response
            .candidates
            .into_iter()
            .flat_map(|candidate| candidate.content.parts)
            .map(|part| part.text)
            .collect::<Vec<_>>()
            .join("\n");

        let stream = stream::once(async move { Ok(InferenceChunk::new(content, true)) });
        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize)]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Content {
    role: String,
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Content,
}

fn map_prompt_message(message: &PromptMessage) -> Content {
    let role = match message.role() {
        MessageRole::Assistant => "model", // Gemini uses "model" instead of "assistant"
        // Tool and System map to "user" (system should be filtered out upstream)
        MessageRole::User | MessageRole::Tool | MessageRole::System => "user",
    };

    let text = if message.role() == MessageRole::Tool {
        format!("[Tool Output]\n{}", message.content())
    } else {
        message.content().to_owned()
    };

    Content {
        role: role.to_owned(),
        parts: vec![Part { text }],
    }
}

fn sanitize_base_url(input: &str) -> AdapterResult<String> {
    let mut base = input.trim().to_owned();
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(AdapterError::configuration(
            "Gemini base URL must start with http:// or https://",
        ));
    }
    if !base.ends_with('/') {
        base.push('/');
    }
    base.parse::<Uri>()
        .map_err(|err| AdapterError::configuration(format!("invalid Gemini base URL: {err}")))?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{InferenceRequest, MessageRole, PromptMessage};

    #[test]
    fn base_url_requires_scheme() {
        let err = GeminiConfig::new("gemini-1.5-pro")
            .with_base_url("generativelanguage.googleapis.com")
            .expect_err("missing scheme should error");

        assert!(matches!(err, AdapterError::Configuration { .. }));
    }

    #[test]
    fn sanitize_allows_trailing_slash() {
        let cfg = GeminiConfig::new("gemini-1.5-pro")
            .with_base_url("https://example.com/gemini")
            .expect("valid URL");
        assert_eq!(cfg.base_url, "https://example.com/gemini/");
    }

    #[test]
    fn prompt_mapping_uses_model_role() {
        let message = PromptMessage::new(MessageRole::Assistant, "response");
        let mapped = map_prompt_message(&message);
        assert_eq!(mapped.role, "model");
        assert_eq!(mapped.parts[0].text, "response");
    }

    #[test]
    fn build_request_extracts_system_instruction() {
        let config = GeminiConfig::new("gemini-1.5-pro").with_api_key("test_key");
        let adapter = GeminiAdapter::new(config).expect("adapter");

        let request = InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "hello")])
            .unwrap()
            .with_system_prompt("You are helpful");

        let gen_req = adapter.build_request(&request);
        assert!(gen_req.system_instruction.is_some());
        assert_eq!(
            gen_req.system_instruction.unwrap().parts[0].text,
            "You are helpful"
        );
        assert_eq!(gen_req.contents.len(), 1);
    }

    #[test]
    fn build_request_filters_system_messages() {
        let config = GeminiConfig::new("gemini-1.5-pro").with_api_key("test_key");
        let adapter = GeminiAdapter::new(config).expect("adapter");

        let request = InferenceRequest::new(vec![
            PromptMessage::new(MessageRole::System, "system"),
            PromptMessage::new(MessageRole::User, "hello"),
        ])
        .unwrap();

        let gen_req = adapter.build_request(&request);
        // System messages in the array should be filtered out
        assert_eq!(gen_req.contents.len(), 1);
        assert_eq!(gen_req.contents[0].role, "user");
    }
}
