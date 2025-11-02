use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use agent_kernel::{
    AgentKernel, AgentMessageHandler, AgentRegistry, HandlerContext, HandlerResult,
    LifecycleEvent, RegistrationConfig, SchedulerConfig, TaskScheduler,
};
use agent_primitives::{AgentId, AgentManifest, Capability, CapabilityId};
use async_trait::async_trait;
use mxp::{Message, MessageType};

struct TestHandler {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentMessageHandler for TestHandler {
    async fn handle_call(&self, _ctx: HandlerContext) -> HandlerResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

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
    let handler_metrics = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(TestHandler {
        calls: Arc::clone(&handler_metrics),
    });

    let scheduler = TaskScheduler::new(SchedulerConfig::default());
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
    let message = Message::new(MessageType::Call, b"integration payload");
    let handle = kernel.schedule_message(message).unwrap();
    handle.await.unwrap().unwrap();
    assert_eq!(handler_metrics.load(Ordering::SeqCst), 1);

    kernel.transition(LifecycleEvent::Retire).unwrap();
    kernel.transition(LifecycleEvent::Terminate).unwrap();

    // Allow deregistration task to run.
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(registry.deregistrations.load(Ordering::SeqCst) >= 1);

    // Clean up scheduler tasks.
    scheduler.close();
}

