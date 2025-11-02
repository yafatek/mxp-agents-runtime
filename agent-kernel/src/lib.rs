//! Agent lifecycle state machine and execution loop.
//!
//! This crate provides the building blocks required by MXP-native agents: lifecycle
//! management, message routing, and a lightweight scheduler backed by `tokio`.

#![warn(missing_docs, clippy::pedantic)]

mod lifecycle;
mod mxp_handlers;
mod scheduler;

use std::sync::Arc;

use agent_primitives::AgentId;
use lifecycle::{AgentState, Lifecycle, LifecycleEvent, LifecycleResult};
use mxp::Message;
use mxp_handlers::{dispatch_message, AgentMessageHandler, HandlerContext, HandlerResult};
use scheduler::{SchedulerResult, TaskScheduler};
use tokio::task::JoinHandle;

pub use lifecycle::{AgentState, LifecycleError, LifecycleEvent, LifecycleResult};
pub use mxp_handlers::{AgentMessageHandler, HandlerContext, HandlerError, HandlerResult};
pub use scheduler::{SchedulerConfig, SchedulerError, SchedulerResult, TaskScheduler};

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
        }
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
    /// Returns [`LifecycleError`](lifecycle::LifecycleError) when the transition is
    /// not permitted from the current state.
    pub fn transition(&mut self, event: LifecycleEvent) -> LifecycleResult<AgentState> {
        self.lifecycle.transition(event)
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
    pub fn schedule_message(
        &self,
        message: Message,
    ) -> SchedulerResult<JoinHandle<HandlerResult>> {
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
