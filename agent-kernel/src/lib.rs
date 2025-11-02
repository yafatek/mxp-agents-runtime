//! Agent lifecycle state machine and execution loop.
//!
//! This crate provides the building blocks required by MXP-native agents: lifecycle
//! management, message routing, and a lightweight scheduler backed by `tokio`.

#![warn(missing_docs, clippy::pedantic)]

mod call;
mod lifecycle;
mod mxp_handlers;
mod registry;
mod scheduler;

use std::sync::Arc;

use agent_primitives::{AgentId, AgentManifest};
use mxp::Message;
use mxp_handlers::dispatch_message;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::warn;

pub use call::{
    CallExecutor, CallOutcome, CallOutcomeSink, CollectingSink, KernelMessageHandler,
    PolicyObserver, ToolInvocationResult, TracingCallSink, TracingPolicyObserver,
};
pub use lifecycle::{AgentState, Lifecycle, LifecycleError, LifecycleEvent, LifecycleResult};
pub use mxp_handlers::{AgentMessageHandler, HandlerContext, HandlerError, HandlerResult};
pub use registry::{AgentRegistry, RegistrationConfig, RegistryError, RegistryResult};
pub use scheduler::{SchedulerConfig, SchedulerError, SchedulerResult, TaskScheduler};

use registry::RegistrationController;

/// Core runtime that wires lifecycle, scheduler, and MXP handlers.
#[derive(Debug)]
pub struct AgentKernel<H>
where
    H: AgentMessageHandler + 'static,
{
    agent_id: AgentId,
    lifecycle: Lifecycle,
    handler: Arc<H>,
    scheduler: TaskScheduler,
    registry: Option<RegistrationController>,
}

impl<H> AgentKernel<H>
where
    H: AgentMessageHandler + 'static,
{
    /// Creates a new agent kernel with the provided handler and scheduler.
    #[must_use]
    pub fn new(agent_id: AgentId, handler: Arc<H>, scheduler: TaskScheduler) -> Self {
        Self {
            agent_id,
            lifecycle: Lifecycle::new(agent_id),
            handler,
            scheduler,
            registry: None,
        }
    }

    /// Provides registry integration for mesh discovery and heartbeat management.
    pub fn set_registry<R>(
        &mut self,
        registry: Arc<R>,
        manifest: AgentManifest,
        config: RegistrationConfig,
    ) where
        R: AgentRegistry + 'static,
    {
        let registry: Arc<dyn AgentRegistry> = registry;
        self.registry = Some(RegistrationController::new(registry, manifest, config));
    }

    /// Returns the identifier associated with this agent.
    #[must_use]
    pub const fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub fn state(&self) -> AgentState {
        self.lifecycle.state()
    }

    /// Applies a lifecycle event, returning the new state on success.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError`](LifecycleError) when the transition is
    /// not permitted from the current state.
    pub fn transition(&mut self, event: LifecycleEvent) -> KernelResult<AgentState> {
        let state = self.lifecycle.transition(event)?;
        if let Some(controller) = &mut self.registry {
            if let Err(err) = controller.on_state_change(state, &self.scheduler) {
                warn!(?err, "registry hook failed during state transition");
                return Err(err.into());
            }
        }
        Ok(state)
    }

    /// Handles an MXP message immediately on the current task.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the message handler implementation.
    pub async fn handle_message(&self, message: Message) -> HandlerResult {
        let ctx = HandlerContext::from_message(self.agent_id, message);
        dispatch_message(self.handler.as_ref(), ctx).await
    }

    /// Enqueues an MXP message for asynchronous processing via the scheduler.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] when the scheduler has been closed.
    pub fn schedule_message(&self, message: Message) -> SchedulerResult<JoinHandle<HandlerResult>> {
        let handler = Arc::clone(&self.handler);
        let agent_id = self.agent_id;
        self.scheduler.spawn(async move {
            let ctx = HandlerContext::from_message(agent_id, message);
            dispatch_message(handler.as_ref(), ctx).await
        })
    }

    /// Returns a reference to the underlying scheduler.
    #[must_use]
    pub fn scheduler(&self) -> &TaskScheduler {
        &self.scheduler
    }
}

/// Errors emitted by [`AgentKernel`] operations.
#[derive(Debug, Error)]
pub enum KernelError {
    /// Lifecycle transition failure.
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    /// Registry hook failure.
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

/// Result alias for kernel operations.
pub type KernelResult<T> = Result<T, KernelError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use agent_primitives::{Capability, CapabilityId};

    struct NullHandler;

    impl AgentMessageHandler for NullHandler {}

    #[derive(Default)]
    struct CountingRegistry {
        registers: Arc<AtomicUsize>,
        heartbeats: Arc<AtomicUsize>,
        deregisters: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AgentRegistry for CountingRegistry {
        async fn register(&self, _manifest: &AgentManifest) -> RegistryResult<()> {
            self.registers.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn heartbeat(&self, _manifest: &AgentManifest) -> RegistryResult<()> {
            self.heartbeats.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn deregister(&self, _manifest: &AgentManifest) -> RegistryResult<()> {
            self.deregisters.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn capability() -> Capability {
        Capability::builder(CapabilityId::new("kernel.test").unwrap())
            .name("Test")
            .unwrap()
            .version("1.0.0")
            .unwrap()
            .add_scope("read:test")
            .unwrap()
            .build()
            .unwrap()
    }

    fn manifest() -> AgentManifest {
        AgentManifest::builder(AgentId::random())
            .name("kernel-agent")
            .unwrap()
            .version("0.0.1")
            .unwrap()
            .capabilities(vec![capability()])
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn registry_hooks_trigger_lifecycle_actions() {
        let scheduler = TaskScheduler::default();
        let handler = Arc::new(NullHandler);
        let mut kernel = AgentKernel::new(AgentId::random(), handler, scheduler.clone());

        let registry = Arc::new(CountingRegistry::default());
        let config = RegistrationConfig::new(
            Duration::from_millis(10),
            Duration::from_millis(5),
            Duration::from_millis(20),
            NonZeroUsize::new(3).unwrap(),
        );
        kernel.set_registry(registry.clone(), manifest(), config);

        kernel.transition(LifecycleEvent::Boot).unwrap();
        kernel.transition(LifecycleEvent::Activate).unwrap();

        tokio::time::sleep(Duration::from_millis(35)).await;
        assert!(registry.registers.load(Ordering::SeqCst) >= 1);
        assert!(registry.heartbeats.load(Ordering::SeqCst) >= 1);

        kernel.transition(LifecycleEvent::Retire).unwrap();
        kernel.transition(LifecycleEvent::Terminate).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(registry.deregisters.load(Ordering::SeqCst) >= 1);
    }
}
