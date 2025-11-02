//! Call message execution pipeline.

use std::fmt;
use std::sync::{Arc, Mutex};

use agent_adapters::traits::{
    AdapterError, InferenceRequest, MessageRole, ModelAdapter, PromptMessage,
};
use agent_memory::{MemoryBus, MemoryChannel, MemoryError, MemoryRecord};
use agent_policy::{
    DecisionKind, PolicyAction, PolicyDecision, PolicyEngine, PolicyError, PolicyRequest,
};
use agent_primitives::AgentId;
use agent_tools::registry::{ToolError, ToolRegistry};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use mxp::{Message, MessageType};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{debug, info, warn};

use crate::{HandlerContext, HandlerError, HandlerResult};

/// Emits MXP audit events when policy decisions deny or escalate requests.
pub trait AuditEmitter: Send + Sync {
    /// Emits the supplied MXP event message.
    fn emit(&self, message: Message);
}

/// Tracing-based audit emitter that logs MXP audit events.
#[derive(Default)]
pub struct TracingAuditEmitter;

impl AuditEmitter for TracingAuditEmitter {
    fn emit(&self, message: Message) {
        let payload = String::from_utf8_lossy(message.payload());
        info!(
            event = ?message.message_type(),
            payload = %payload,
            "policy audit event emitted"
        );
    }
}

/// Observer invoked whenever a policy decision is produced.
pub trait PolicyObserver: Send + Sync {
    /// Records the decision emitted for the supplied request subject.
    fn on_decision(&self, request: &PolicyRequest, decision: &PolicyDecision, subject: &str);
}

/// Observer that emits decisions to the tracing system.
#[derive(Default)]
pub struct TracingPolicyObserver;

impl PolicyObserver for TracingPolicyObserver {
    fn on_decision(&self, request: &PolicyRequest, decision: &PolicyDecision, subject: &str) {
        let reason = decision.reason().unwrap_or_default();
        let approvers = decision.required_approvals();
        match decision.kind() {
            DecisionKind::Allow => {
                debug!(agent_id = %request.agent_id(), subject, "policy allow");
            }
            DecisionKind::Deny => {
                warn!(
                    agent_id = %request.agent_id(),
                    subject,
                    reason,
                    "policy deny"
                );
            }
            DecisionKind::Escalate => {
                warn!(
                    agent_id = %request.agent_id(),
                    subject,
                    reason,
                    approvers = ?approvers,
                    "policy escalate"
                );
            }
        }
    }
}

/// Composite observer that forwards decisions to a collection of observers.
pub struct CompositePolicyObserver {
    observers: Vec<Arc<dyn PolicyObserver>>,
}

impl CompositePolicyObserver {
    /// Creates a new composite observer from the supplied list.
    #[must_use]
    pub fn new<I>(observers: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn PolicyObserver>>,
    {
        Self {
            observers: observers.into_iter().collect(),
        }
    }

    /// Adds an observer to the composite set.
    pub fn push(&mut self, observer: Arc<dyn PolicyObserver>) {
        self.observers.push(observer);
    }
}

impl PolicyObserver for CompositePolicyObserver {
    fn on_decision(&self, request: &PolicyRequest, decision: &PolicyDecision, subject: &str) {
        for observer in &self.observers {
            observer.on_decision(request, decision, subject);
        }
    }
}

/// Observer that emits MXP audit events for deny/escalate outcomes.
pub struct MxpAuditObserver {
    emitter: Arc<dyn AuditEmitter>,
}

impl MxpAuditObserver {
    /// Creates a new MXP audit observer using the provided emitter.
    #[must_use]
    pub fn new(emitter: Arc<dyn AuditEmitter>) -> Self {
        Self { emitter }
    }
}

impl PolicyObserver for MxpAuditObserver {
    fn on_decision(&self, request: &PolicyRequest, decision: &PolicyDecision, subject: &str) {
        if matches!(decision.kind(), DecisionKind::Deny | DecisionKind::Escalate) {
            let payload = json!({
                "agent_id": request.agent_id().to_string(),
                "subject": subject,
                "decision": format!("{:?}", decision.kind()),
                "reason": decision.reason(),
                "approvers": decision.required_approvals(),
                "metadata": request.context().metadata(),
            });
            let payload_string = payload.to_string();
            let message = Message::new(MessageType::Event, payload_string.as_bytes());
            self.emitter.emit(message);
        }
    }
}

/// Executes MXP `Call` messages by invoking registered tools and the
/// configured [`ModelAdapter`].
#[derive(Clone)]
pub struct CallExecutor {
    adapter: Arc<dyn ModelAdapter>,
    tools: Arc<ToolRegistry>,
    policy: Option<Arc<dyn PolicyEngine>>,
    policy_observer: Option<Arc<dyn PolicyObserver>>,
}

impl fmt::Debug for CallExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let metadata = self.adapter.metadata();
        f.debug_struct("CallExecutor")
            .field("provider", &metadata.provider())
            .field("model", &metadata.model())
            .field("policy_configured", &self.policy.is_some())
            .field("observer_configured", &self.policy_observer.is_some())
            .finish_non_exhaustive()
    }
}

impl CallExecutor {
    /// Creates a new call executor.
    #[must_use]
    pub fn new(adapter: Arc<dyn ModelAdapter>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            adapter,
            tools,
            policy: None,
            policy_observer: None,
        }
    }

    /// Configures the policy engine used for governance decisions.
    pub fn set_policy(&mut self, policy: Arc<dyn PolicyEngine>) {
        self.policy = Some(policy);
    }

    /// Configures the policy engine, returning the updated executor for chaining.
    #[must_use]
    pub fn with_policy(mut self, policy: Arc<dyn PolicyEngine>) -> Self {
        self.set_policy(policy);
        self
    }

    /// Returns the policy engine if one has been configured.
    #[must_use]
    pub fn policy(&self) -> Option<&Arc<dyn PolicyEngine>> {
        self.policy.as_ref()
    }

    /// Installs a policy observer for integration hooks.
    pub fn set_policy_observer(&mut self, observer: Arc<dyn PolicyObserver>) {
        self.policy_observer = Some(observer);
    }

    /// Configures a policy observer, returning the updated executor for chaining.
    #[must_use]
    pub fn with_policy_observer(mut self, observer: Arc<dyn PolicyObserver>) -> Self {
        self.set_policy_observer(observer);
        self
    }

    /// Returns the policy observer if configured.
    #[must_use]
    pub fn policy_observer(&self) -> Option<&Arc<dyn PolicyObserver>> {
        self.policy_observer.as_ref()
    }

    fn notify_policy(&self, request: &PolicyRequest, decision: &PolicyDecision, subject: &str) {
        if let Some(observer) = &self.policy_observer {
            observer.on_decision(request, decision, subject);
        }
    }

    async fn enforce_tool_policy(
        &self,
        ctx: &HandlerContext,
        invocation: &ToolInvocation,
    ) -> HandlerResult<()> {
        let Some(policy) = self.policy.as_ref() else {
            return Ok(());
        };

        let mut request = PolicyRequest::new(
            ctx.agent_id(),
            PolicyAction::InvokeTool {
                name: invocation.name.clone(),
            },
        );

        request
            .context_mut()
            .insert_metadata("input", invocation.input.clone());

        if let Some(handle) = self.tools.get(&invocation.name) {
            let metadata = handle.metadata().clone();
            request
                .context_mut()
                .insert_metadata("tool_version", Value::from(metadata.version().to_owned()));
            if let Some(description) = metadata.description() {
                request
                    .context_mut()
                    .insert_metadata("tool_description", Value::from(description.to_owned()));
            }

            if !metadata.capabilities().is_empty() {
                let capabilities: Vec<String> = metadata
                    .capabilities()
                    .iter()
                    .map(|cap| cap.as_str().to_owned())
                    .collect();
                request
                    .context_mut()
                    .insert_metadata("capabilities", Value::from(capabilities.clone()));
                request
                    .context_mut()
                    .extend_tags(capabilities.iter().map(|cap| format!("cap:{cap}")));
            }
        }

        let decision = policy
            .evaluate(&request)
            .await
            .map_err(|err| map_policy_error(&err))?;

        self.notify_policy(&request, &decision, &request.action().label());
        enforce_decision(&decision, &request.action().label())
    }

    async fn enforce_inference_policy(
        &self,
        ctx: &HandlerContext,
        message_count: usize,
        tool_names: &[String],
    ) -> HandlerResult<()> {
        let Some(policy) = self.policy.as_ref() else {
            return Ok(());
        };

        let metadata = self.adapter.metadata();
        let mut request = PolicyRequest::new(
            ctx.agent_id(),
            PolicyAction::ModelInference {
                provider: metadata.provider().to_owned(),
                model: metadata.model().to_owned(),
            },
        );

        request
            .context_mut()
            .insert_metadata("message_count", Value::from(message_count as u64));

        if !tool_names.is_empty() {
            request
                .context_mut()
                .insert_metadata("tools", Value::from(tool_names.to_owned()));
            request
                .context_mut()
                .extend_tags(tool_names.iter().map(|name| format!("tool:{name}")));
        }

        let decision = policy
            .evaluate(&request)
            .await
            .map_err(|err| map_policy_error(&err))?;

        self.notify_policy(&request, &decision, &request.action().label());
        enforce_decision(&decision, &request.action().label())
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
            self.enforce_tool_policy(ctx, &invocation).await?;

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

        self.enforce_inference_policy(ctx, messages.len(), &tool_names)
            .await?;

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

fn map_memory_error(err: &MemoryError) -> HandlerError {
    HandlerError::custom(format!("memory error: {err}"))
}

fn map_policy_error(err: &PolicyError) -> HandlerError {
    HandlerError::custom(format!("policy engine error: {err}"))
}

fn enforce_decision(decision: &PolicyDecision, subject: &str) -> HandlerResult<()> {
    match decision.kind() {
        DecisionKind::Allow => Ok(()),
        DecisionKind::Deny => {
            let reason = decision.reason().unwrap_or("policy denied the request");
            Err(HandlerError::custom(format!(
                "policy denied {subject}: {reason}"
            )))
        }
        DecisionKind::Escalate => {
            let reason = decision.reason().unwrap_or("policy escalation required");
            let approvers = decision.required_approvals();
            let detail = if approvers.is_empty() {
                reason.to_owned()
            } else {
                format!("{reason} (approvers: {})", approvers.join(", "))
            };
            Err(HandlerError::custom(format!(
                "policy escalation required for {subject}: {detail}"
            )))
        }
    }
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
    memory: Option<Arc<MemoryBus>>,
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
        Self {
            executor,
            sink,
            memory: None,
        }
    }

    /// Configures the memory bus used to persist call transcripts.
    #[must_use]
    pub fn with_memory(mut self, memory: Arc<MemoryBus>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Installs or replaces the memory bus after construction.
    pub fn set_memory(&mut self, memory: Arc<MemoryBus>) {
        self.memory = Some(memory);
    }

    /// Configures the policy engine used to guard tool execution and model inference.
    #[must_use]
    pub fn with_policy(mut self, policy: Arc<dyn PolicyEngine>) -> Self {
        self.set_policy(policy);
        self
    }

    /// Installs or replaces the policy engine after construction.
    pub fn set_policy(&mut self, policy: Arc<dyn PolicyEngine>) {
        Arc::make_mut(&mut self.executor).set_policy(policy);
    }

    /// Configures the policy observer used to record governance decisions.
    #[must_use]
    pub fn with_policy_observer(mut self, observer: Arc<dyn PolicyObserver>) -> Self {
        self.set_policy_observer(observer);
        self
    }

    /// Installs or replaces the policy observer after construction.
    pub fn set_policy_observer(&mut self, observer: Arc<dyn PolicyObserver>) {
        Arc::make_mut(&mut self.executor).set_policy_observer(observer);
    }

    /// Returns the configured policy observer, if any.
    #[must_use]
    pub fn policy_observer(&self) -> Option<&Arc<dyn PolicyObserver>> {
        self.executor.policy_observer()
    }

    /// Returns the configured memory bus, if any.
    #[must_use]
    pub fn memory(&self) -> Option<&Arc<MemoryBus>> {
        self.memory.as_ref()
    }

    async fn record_inbound(&self, ctx: &HandlerContext) -> HandlerResult<()> {
        let Some(memory) = &self.memory else {
            return Ok(());
        };

        let record = MemoryRecord::builder(MemoryChannel::Input, ctx.message().payload().clone())
            .tag("mxp.call")
            .map_err(|err| map_memory_error(&err))?
            .metadata("direction", Value::from("inbound"))
            .metadata("message_type", Value::from("call"))
            .metadata("agent_id", Value::from(ctx.agent_id().to_string()))
            .build()
            .map_err(|err| map_memory_error(&err))?;

        self.enforce_memory_policy(ctx.agent_id(), &record).await?;
        memory
            .record(record)
            .await
            .map_err(|err| map_memory_error(&err))?;
        Ok(())
    }

    async fn record_outbound(&self, agent_id: AgentId, outcome: &CallOutcome) -> HandlerResult<()> {
        let Some(memory) = &self.memory else {
            return Ok(());
        };

        for tool in outcome.tool_results() {
            let payload = Bytes::from(serde_json::to_vec(&tool.output).map_err(|err| {
                HandlerError::custom(format!("failed to encode tool output: {err}"))
            })?);
            let record = MemoryRecord::builder(MemoryChannel::Tool, payload)
                .tag("mxp.call")
                .map_err(|err| map_memory_error(&err))?
                .tag("tool")
                .map_err(|err| map_memory_error(&err))?
                .metadata("direction", Value::from("tool"))
                .metadata("tool_name", Value::from(tool.name.clone()))
                .build()
                .map_err(|err| map_memory_error(&err))?;
            self.enforce_memory_policy(agent_id, &record).await?;
            memory
                .record(record)
                .await
                .map_err(|err| map_memory_error(&err))?;
        }

        let response_record = MemoryRecord::builder(
            MemoryChannel::Output,
            Bytes::from(outcome.response().to_owned()),
        )
        .tag("mxp.call")
        .map_err(|err| map_memory_error(&err))?
        .metadata("direction", Value::from("outbound"))
        .metadata("message_type", Value::from("call"))
        .build()
        .map_err(|err| map_memory_error(&err))?;

        self.enforce_memory_policy(agent_id, &response_record)
            .await?;
        memory
            .record(response_record)
            .await
            .map_err(|err| map_memory_error(&err))?;
        Ok(())
    }

    async fn enforce_memory_policy(
        &self,
        agent_id: AgentId,
        record: &MemoryRecord,
    ) -> HandlerResult<()> {
        let Some(policy) = self.executor.policy() else {
            return Ok(());
        };

        let request = PolicyRequest::from_memory_record(agent_id, record);
        let decision = policy
            .evaluate(&request)
            .await
            .map_err(|err| map_policy_error(&err))?;

        self.executor
            .notify_policy(&request, &decision, &request.action().label());
        enforce_decision(&decision, &request.action().label())
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
        self.record_inbound(&ctx).await?;

        let outcome = self.executor.execute(&ctx).await?;

        self.record_outbound(ctx.agent_id(), &outcome).await?;

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
    use agent_memory::{FileJournal, MemoryBusBuilder, MemoryChannel, VolatileConfig};
    use agent_policy::{PolicyAction, PolicyDecision, PolicyEngine, PolicyRequest, PolicyResult};
    use agent_primitives::AgentId;
    use agent_tools::registry::{ToolMetadata, ToolRegistry};
    use futures::stream;
    use mxp::Message;
    use serde_json::json;
    use std::num::NonZeroUsize;
    use std::sync::{Arc, Mutex};

    use crate::{AgentMessageHandler, HandlerContext, HandlerError};

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

    struct DenyPolicy;

    #[async_trait]
    impl PolicyEngine for DenyPolicy {
        async fn evaluate(&self, request: &PolicyRequest) -> PolicyResult<PolicyDecision> {
            match request.action() {
                PolicyAction::InvokeTool { .. } => Ok(PolicyDecision::deny("disabled by policy")),
                _ => Ok(PolicyDecision::allow()),
            }
        }
    }

    fn temp_path() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("handler-test-{}.log", AgentId::random()));
        path
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

    #[tokio::test]
    async fn policy_denies_tool_invocation() {
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
        let handler = KernelMessageHandler::new(adapter, tools, sink.clone())
            .with_policy(Arc::new(DenyPolicy));

        let payload = json!({
            "messages": [
                {"role": "user", "content": "ping"}
            ],
            "tools": [
                {"name": "echo", "input": {"value": 1}}
            ]
        });

        let message = mxp::Message::new(mxp::MessageType::Call, payload.to_string().as_bytes());
        let ctx = HandlerContext::from_message(agent_primitives::AgentId::random(), message);

        let err = handler
            .handle_call(ctx)
            .await
            .expect_err("policy should deny");
        match err {
            HandlerError::Custom(reason) => assert!(reason.contains("policy denied")),
            other => panic!("unexpected error: {other:?}"),
        }

        assert!(sink.drain().is_empty());
    }

    #[tokio::test]
    async fn persists_transcript_via_memory_bus() {
        let adapter = Arc::new(StaticAdapter {
            metadata: AdapterMetadata::new("test", "static"),
            response: "ok".to_owned(),
        });

        let tools = Arc::new(ToolRegistry::new());
        tools
            .register_tool(
                ToolMetadata::new("echo", "1.0.0").unwrap(),
                |input: Value| async move { Ok(input) },
            )
            .unwrap();

        let sink = CollectingSink::new();
        let path = temp_path();
        let journal: Arc<dyn agent_memory::Journal> =
            Arc::new(FileJournal::open(&path).await.unwrap());
        let bus = Arc::new(
            MemoryBusBuilder::new(VolatileConfig::new(NonZeroUsize::new(8).unwrap()))
                .with_journal(journal.clone())
                .build()
                .unwrap(),
        );

        let handler =
            KernelMessageHandler::new(adapter, tools, sink.clone()).with_memory(bus.clone());

        let payload = json!({
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "tools": [
                {"name": "echo", "input": {"value": 1}}
            ]
        });

        let message = mxp::Message::new(mxp::MessageType::Call, payload.to_string().as_bytes());
        let ctx = HandlerContext::from_message(agent_primitives::AgentId::random(), message);

        handler.handle_call(ctx).await.unwrap();

        let records = bus.recent(5).await;
        assert_eq!(records.len(), 3);
        assert!(matches!(records[0].channel(), MemoryChannel::Input));
        assert!(matches!(records[1].channel(), MemoryChannel::Tool));
        assert!(matches!(records[2].channel(), MemoryChannel::Output));

        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    struct RecordingObserver {
        decisions: Mutex<Vec<(String, DecisionKind)>>,
    }

    impl RecordingObserver {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                decisions: Mutex::new(Vec::new()),
            })
        }
    }

    impl PolicyObserver for RecordingObserver {
        fn on_decision(&self, request: &PolicyRequest, decision: &PolicyDecision, subject: &str) {
            let mut guard = self.decisions.lock().expect("observer poisoned");
            guard.push((
                format!("{}:{}", request.agent_id(), subject),
                decision.kind(),
            ));
        }
    }

    #[tokio::test]
    async fn observer_receives_decisions() {
        let adapter = Arc::new(StaticAdapter {
            metadata: AdapterMetadata::new("test", "static"),
            response: "ok".to_owned(),
        });
        let tools = Arc::new(ToolRegistry::new());
        tools
            .register_tool(
                ToolMetadata::new("echo", "1.0.0").unwrap(),
                |input: Value| async move { Ok(input) },
            )
            .unwrap();

        let sink = CollectingSink::new();
        let observer = RecordingObserver::new();
        let handler = KernelMessageHandler::new(adapter, tools, sink.clone())
            .with_policy(Arc::new(DenyPolicy))
            .with_policy_observer(observer.clone());

        let payload = json!({
            "messages": [
                {"role": "user", "content": "ping"}
            ],
            "tools": [
                {"name": "echo", "input": {"value": 1}}
            ]
        });

        let message = mxp::Message::new(mxp::MessageType::Call, payload.to_string().as_bytes());
        let ctx = HandlerContext::from_message(agent_primitives::AgentId::random(), message);

        handler
            .handle_call(ctx)
            .await
            .expect_err("policy should deny");

        let records = observer
            .decisions
            .lock()
            .expect("observer poisoned")
            .clone();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].1, DecisionKind::Deny);
    }

    struct MemoryDenyPolicy;

    #[async_trait]
    impl PolicyEngine for MemoryDenyPolicy {
        async fn evaluate(&self, request: &PolicyRequest) -> PolicyResult<PolicyDecision> {
            match request.action() {
                PolicyAction::EmitEvent { event_type } if event_type == "memory_record" => {
                    Ok(PolicyDecision::deny("memory recording disabled"))
                }
                _ => Ok(PolicyDecision::allow()),
            }
        }
    }

    #[tokio::test]
    async fn policy_denies_memory_recording() {
        let adapter = Arc::new(StaticAdapter {
            metadata: AdapterMetadata::new("test", "static"),
            response: "ok".to_owned(),
        });
        let tools = Arc::new(ToolRegistry::new());
        tools
            .register_tool(
                ToolMetadata::new("echo", "1.0.0").unwrap(),
                |input: Value| async move { Ok(input) },
            )
            .unwrap();

        let sink = CollectingSink::new();
        let journal_path = temp_path();
        let journal: Arc<dyn agent_memory::Journal> =
            Arc::new(FileJournal::open(&journal_path).await.expect("journal"));
        let memory_bus = Arc::new(
            MemoryBusBuilder::new(VolatileConfig::default())
                .with_journal(journal)
                .build()
                .expect("bus"),
        );

        let handler = KernelMessageHandler::new(adapter, tools, sink)
            .with_memory(memory_bus)
            .with_policy(Arc::new(MemoryDenyPolicy));

        let payload = json!({
            "messages": [
                {"role": "user", "content": "ping"}
            ],
            "tools": [
                {"name": "echo", "input": {"value": 1}}
            ]
        });

        let message = mxp::Message::new(mxp::MessageType::Call, payload.to_string().as_bytes());
        let ctx = HandlerContext::from_message(agent_primitives::AgentId::random(), message);

        let err = handler
            .handle_call(ctx)
            .await
            .expect_err("policy should deny");
        match err {
            HandlerError::Custom(reason) => assert!(reason.contains("policy denied")),
            other => panic!("unexpected error: {other:?}"),
        }

        if journal_path.exists() {
            let _ = std::fs::remove_file(&journal_path);
        }
    }

    struct RecordingAuditEmitter {
        events: Mutex<Vec<Message>>,
    }

    impl RecordingAuditEmitter {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
            })
        }
    }

    impl AuditEmitter for RecordingAuditEmitter {
        fn emit(&self, message: Message) {
            self.events.lock().expect("emitter poisoned").push(message);
        }
    }

    struct EscalatePolicy;

    #[async_trait]
    impl PolicyEngine for EscalatePolicy {
        async fn evaluate(&self, _request: &PolicyRequest) -> PolicyResult<PolicyDecision> {
            Ok(PolicyDecision::escalate(
                "needs approval",
                vec!["secops".into()],
            ))
        }
    }

    #[tokio::test]
    async fn audit_observer_emits_event_on_escalation() {
        let adapter = Arc::new(StaticAdapter {
            metadata: AdapterMetadata::new("test", "static"),
            response: "ok".to_owned(),
        });
        let tools = Arc::new(ToolRegistry::new());
        tools
            .register_tool(
                ToolMetadata::new("echo", "1.0.0").unwrap(),
                |input: Value| async move { Ok(input) },
            )
            .unwrap();

        let sink = CollectingSink::new();
        let emitter = RecordingAuditEmitter::new();
        let observer = CompositePolicyObserver::new([
            Arc::new(TracingPolicyObserver) as Arc<dyn PolicyObserver>,
            Arc::new(MxpAuditObserver::new(emitter.clone())) as Arc<dyn PolicyObserver>,
        ]);

        let handler = KernelMessageHandler::new(adapter, tools, sink)
            .with_policy(Arc::new(EscalatePolicy))
            .with_policy_observer(Arc::new(observer) as Arc<dyn PolicyObserver>);

        let payload = json!({
            "messages": [
                {"role": "user", "content": "ping"}
            ]
        });

        let message = mxp::Message::new(mxp::MessageType::Call, payload.to_string().as_bytes());
        let ctx = HandlerContext::from_message(agent_primitives::AgentId::random(), message);

        let err = handler
            .handle_call(ctx)
            .await
            .expect_err("policy should escalate");
        match err {
            HandlerError::Custom(reason) => assert!(reason.contains("policy escalation")),
            other => panic!("unexpected error: {other:?}"),
        }

        let events = emitter.events.lock().expect("emitter poisoned");
        assert_eq!(events.len(), 1);
        let payload = String::from_utf8_lossy(events[0].payload());
        assert!(payload.contains("needs approval"));
        assert!(payload.contains("secops"));
    }
}
