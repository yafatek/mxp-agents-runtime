//! Call message execution pipeline.

use std::sync::{Arc, Mutex};

use agent_adapters::traits::{
    AdapterError, InferenceRequest, MessageRole, ModelAdapter, PromptMessage,
};
use agent_tools::registry::{ToolError, ToolRegistry};
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;

use crate::{HandlerContext, HandlerError, HandlerResult};

/// Executes MXP `Call` messages by invoking registered tools and the
/// configured [`ModelAdapter`].
pub struct CallExecutor {
    adapter: Arc<dyn ModelAdapter>,
    tools: Arc<ToolRegistry>,
}

impl CallExecutor {
    /// Creates a new call executor.
    #[must_use]
    pub fn new(adapter: Arc<dyn ModelAdapter>, tools: Arc<ToolRegistry>) -> Self {
        Self { adapter, tools }
    }

    /// Executes the call pipeline using data extracted from the handler context.
    ///
    /// # Errors
    ///
    /// Returns [`HandlerError`] when payload decoding, tool execution, or model
    /// inference fails.
    pub async fn execute(&self, ctx: &HandlerContext) -> HandlerResult<CallOutcome> {
        let payload = parse_payload(ctx)?;

        let mut messages = payload.messages;
        let mut tool_names = Vec::new();
        let mut tool_results = Vec::new();

        for invocation in payload.tools {
            let tool_output = self
                .tools
                .invoke(&invocation.name, invocation.input.clone())
                .await
                .map_err(|err| map_tool_error(&invocation.name, &err))?;

            let message_content =
                serde_json::to_string(&tool_output).unwrap_or_else(|_| String::new());
            messages.push(PromptMessage::new(MessageRole::Tool, message_content));
            tool_names.push(invocation.name.clone());
            tool_results.push(ToolInvocationResult {
                name: invocation.name,
                output: tool_output,
            });
        }

        let mut request = InferenceRequest::new(messages)
            .map_err(|err| HandlerError::custom(format!("invalid request: {err}")))?;

        if let Some(max_tokens) = payload.max_output_tokens {
            request = request.with_max_output_tokens(max_tokens);
        }

        if let Some(temperature) = payload.temperature {
            request = request.with_temperature(temperature);
        }

        if !tool_names.is_empty() {
            request = request.with_tools(tool_names);
        }

        let mut stream = self
            .adapter
            .infer(request)
            .await
            .map_err(|err| map_adapter_error(&err, self.adapter.metadata()))?;

        let mut response = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| map_adapter_error(&err, self.adapter.metadata()))?;
            response.push_str(&chunk.delta);
            if chunk.done {
                break;
            }
        }

        Ok(CallOutcome {
            response,
            tool_results,
        })
    }
}

fn parse_payload(ctx: &HandlerContext) -> HandlerResult<CallPayload> {
    let payload = ctx.message().payload();
    if payload.is_empty() {
        return Err(HandlerError::custom("call payload missing"));
    }

    serde_json::from_slice::<CallPayload>(payload.as_ref())
        .map_err(|err| HandlerError::custom(format!("failed to decode call payload: {err}")))
}

fn map_tool_error(name: &str, err: &ToolError) -> HandlerError {
    HandlerError::custom(format!("tool `{name}` failed: {err}"))
}

fn map_adapter_error(
    err: &AdapterError,
    metadata: &agent_adapters::traits::AdapterMetadata,
) -> HandlerError {
    HandlerError::custom(format!(
        "adapter `{}` for model `{}` error: {err}",
        metadata.provider(),
        metadata.model()
    ))
}

/// Outcome of processing a call message.
#[derive(Debug)]
pub struct CallOutcome {
    response: String,
    tool_results: Vec<ToolInvocationResult>,
}

impl CallOutcome {
    /// Returns the aggregated model response text.
    #[must_use]
    pub fn response(&self) -> &str {
        &self.response
    }

    /// Returns the tool invocation results that were executed as part of this call.
    #[must_use]
    pub fn tool_results(&self) -> &[ToolInvocationResult] {
        &self.tool_results
    }
}

/// Result describing an executed tool invocation.
#[derive(Debug, Clone)]
pub struct ToolInvocationResult {
    /// Name of the tool that was invoked.
    pub name: String,
    /// Output produced by the tool.
    pub output: Value,
}

#[derive(Debug, Deserialize)]
struct CallPayload {
    messages: Vec<PromptMessage>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
    #[serde(default)]
    tools: Vec<ToolInvocation>,
}

#[derive(Debug, Deserialize)]
struct ToolInvocation {
    name: String,
    #[serde(default)]
    input: Value,
}

/// Handler implementation that wires the call executor into the MXP handler trait.
pub struct KernelMessageHandler {
    executor: Arc<CallExecutor>,
    sink: Arc<dyn CallOutcomeSink>,
}

impl KernelMessageHandler {
    /// Creates a new handler using the provided adapter and registry.
    #[must_use]
    pub fn new(
        adapter: Arc<dyn ModelAdapter>,
        tools: Arc<ToolRegistry>,
        sink: Arc<dyn CallOutcomeSink>,
    ) -> Self {
        let executor = Arc::new(CallExecutor::new(adapter, tools));
        Self { executor, sink }
    }

    /// Returns the underlying executor for advanced scenarios.
    #[must_use]
    pub fn executor(&self) -> &CallExecutor {
        &self.executor
    }
}

#[async_trait]
impl crate::AgentMessageHandler for KernelMessageHandler {
    async fn handle_call(&self, ctx: HandlerContext) -> HandlerResult {
        let outcome = self.executor.execute(&ctx).await?;
        self.sink.record(outcome);
        Ok(())
    }
}

/// Observer trait used to capture call outcomes (for logging, metrics, etc.).
pub trait CallOutcomeSink: Send + Sync {
    /// Records the outcome of a call invocation.
    fn record(&self, outcome: CallOutcome);
}

/// Sink implementation that logs to tracing.
#[derive(Default)]
pub struct TracingCallSink;

impl CallOutcomeSink for TracingCallSink {
    fn record(&self, outcome: CallOutcome) {
        let tool_names: Vec<String> = outcome
            .tool_results()
            .iter()
            .map(|result| result.name.clone())
            .collect();
        tracing::info!(
            response = outcome.response(),
            tools = ?tool_names,
            "call execution completed"
        );
    }
}

/// Sink used during testing to capture outcomes.
#[derive(Default)]
pub struct CollectingSink {
    results: Mutex<Vec<CallOutcome>>,
}

impl CollectingSink {
    /// Creates a new collecting sink.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            results: Mutex::new(Vec::new()),
        })
    }

    /// Returns the collected outcomes.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned by a previous panic.
    #[must_use]
    pub fn drain(&self) -> Vec<CallOutcome> {
        let mut lock = self.results.lock().expect("collecting sink poisoned");
        lock.drain(..).collect()
    }
}

impl CallOutcomeSink for CollectingSink {
    fn record(&self, outcome: CallOutcome) {
        self.results
            .lock()
            .expect("collecting sink poisoned")
            .push(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agent_adapters::traits::{AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk};
    use agent_tools::registry::{ToolMetadata, ToolRegistry};
    use futures::stream;
    use serde_json::json;

    use crate::{AgentMessageHandler, HandlerContext};

    struct StaticAdapter {
        metadata: AdapterMetadata,
        response: String,
    }

    #[async_trait]
    impl ModelAdapter for StaticAdapter {
        fn metadata(&self) -> &AdapterMetadata {
            &self.metadata
        }

        async fn infer(&self, _request: InferenceRequest) -> AdapterResult<AdapterStream> {
            let chunk = InferenceChunk::new(self.response.clone(), true);
            Ok(Box::pin(stream::once(async move { Ok(chunk) })))
        }
    }

    #[tokio::test]
    async fn executes_call_pipeline() {
        let adapter = Arc::new(StaticAdapter {
            metadata: AdapterMetadata::new("test", "static"),
            response: "static-response".to_owned(),
        });
        let tools = Arc::new(ToolRegistry::new());
        tools
            .register_tool(
                ToolMetadata::new("echo", "1.0.0").unwrap(),
                |input: Value| async move { Ok(input) },
            )
            .unwrap();

        let sink = CollectingSink::new();
        let handler = KernelMessageHandler::new(adapter, tools, sink.clone());

        let payload = json!({
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Ping"}
            ],
            "tools": [
                {"name": "echo", "input": {"value": 1}}
            ]
        });

        let message = mxp::Message::new(mxp::MessageType::Call, payload.to_string().as_bytes());
        let ctx = HandlerContext::from_message(agent_primitives::AgentId::random(), message);

        handler.handle_call(ctx).await.unwrap();

        let results = sink.drain();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].response(), "static-response");
        assert_eq!(results[0].tool_results().len(), 1);
    }
}
