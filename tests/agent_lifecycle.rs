use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use agent_adapters::traits::{
    AdapterMetadata, AdapterResult, AdapterStream, InferenceChunk, InferenceRequest, ModelAdapter,
};
use agent_kernel::{
    AgentKernel, AgentRegistry, CollectingSink, KernelMessageHandler, LifecycleEvent,
    RegistrationConfig, SchedulerConfig, TaskScheduler,
};
use agent_primitives::{AgentId, AgentManifest, Capability, CapabilityId};
use agent_tools::registry::{ToolMetadata, ToolRegistry};
use async_trait::async_trait;
use mxp::{Message, MessageType};
use futures::stream;
use serde_json::json;

struct TestRegistry {
    registers: Arc<AtomicUsize>,
    heartbeats: Arc<AtomicUsize>,
    deregistrations: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentRegistry for TestRegistry {
    async fn register(&self, _manifest: &AgentManifest) -> agent_kernel::RegistryResult<()> {
        self.registers.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn heartbeat(&self, _manifest: &AgentManifest) -> agent_kernel::RegistryResult<()> {
        self.heartbeats.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn deregister(&self, _manifest: &AgentManifest) -> agent_kernel::RegistryResult<()> {
        self.deregistrations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn capability() -> agent_primitives::Result<Capability> {
    Capability::builder(CapabilityId::new("integration.call")?)
        .name("Integration")?
        .version("1.0.0")?
        .add_scope("call:integration")?
        .build()
}

fn manifest(agent_id: AgentId) -> AgentManifest {
    AgentManifest::builder(agent_id)
        .name("integration-agent")
        .unwrap()
        .version("0.1.0")
        .unwrap()
        .capabilities(vec![capability().unwrap()])
        .build()
        .unwrap()
}

#[tokio::test]
async fn kernel_handles_messages_and_registry_hooks() {
    let agent_id = AgentId::random();
    let scheduler = TaskScheduler::new(SchedulerConfig::default());
    let adapter = Arc::new(StaticAdapter::new("static-response"));
    let tools = Arc::new(ToolRegistry::new());
    tools
        .register_tool(
            ToolMetadata::new("echo", "1.0.0").unwrap(),
            |input: serde_json::Value| async move { Ok(input) },
        )
        .unwrap();
    let sink = CollectingSink::new();
    let handler = Arc::new(KernelMessageHandler::new(adapter, tools, Arc::clone(&sink)));

    let mut kernel = AgentKernel::new(agent_id, handler, scheduler.clone());

    let registry = Arc::new(TestRegistry {
        registers: Arc::new(AtomicUsize::new(0)),
        heartbeats: Arc::new(AtomicUsize::new(0)),
        deregistrations: Arc::new(AtomicUsize::new(0)),
    });

    kernel.set_registry(
        Arc::clone(&registry),
        manifest(agent_id),
        RegistrationConfig::new(
            Duration::from_millis(20),
            Duration::from_millis(10),
            Duration::from_millis(40),
            NonZeroUsize::new(2).unwrap(),
        ),
    );

    kernel.transition(LifecycleEvent::Boot).unwrap();
    kernel.transition(LifecycleEvent::Activate).unwrap();

    // Give registry loop time to register and emit heartbeats.
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(registry.registers.load(Ordering::SeqCst) >= 1);
    assert!(registry.heartbeats.load(Ordering::SeqCst) >= 1);

    // Feed a Call message through the scheduler and wait for completion.
    let payload = json!({
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Ping"}
        ],
        "tools": [
            {"name": "echo", "input": {"value": 42}}
        ],
        "temperature": 0.2
    });
    let payload = payload.to_string();
    let message = Message::new(MessageType::Call, payload.as_bytes());
    let handle = kernel.schedule_message(message).unwrap();
    handle.await.unwrap().unwrap();
    let outcomes = sink.drain();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].response(), "static-response");
    assert_eq!(outcomes[0].tool_results().len(), 1);

    kernel.transition(LifecycleEvent::Retire).unwrap();
    kernel.transition(LifecycleEvent::Terminate).unwrap();

    // Allow deregistration task to run.
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(registry.deregistrations.load(Ordering::SeqCst) >= 1);

    // Clean up scheduler tasks.
    scheduler.close();
}

struct StaticAdapter {
    metadata: AdapterMetadata,
    response: String,
}

impl StaticAdapter {
    fn new(response: impl Into<String>) -> Self {
        Self {
            metadata: AdapterMetadata::new("test", "static"),
            response: response.into(),
        }
    }
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

