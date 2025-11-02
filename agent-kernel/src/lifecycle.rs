//! Lifecycle state machine for MXP agents.

use agent_primitives::AgentId;
use thiserror::Error;
use tracing::debug;

/// Discretely states an agent can occupy during its lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Kernel constructed but not yet initialized.
    Init,
    /// Dependencies are initialized and the agent is ready for activation.
    Ready,
    /// Agent is actively handling workloads.
    Active,
    /// Agent is temporarily paused but can resume.
    Suspended,
    /// Agent is draining in-flight work prior to shut down.
    Retiring,
    /// Agent fully terminated; no further work should be scheduled.
    Terminated,
}

impl AgentState {
    /// Returns `true` when the state represents a running agent.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns `true` once the agent has terminated.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Terminated)
    }
}

/// Events that trigger lifecycle transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// Finish bootstrapping resources.
    Boot,
    /// Begin processing workloads.
    Activate,
    /// Pause execution while retaining state.
    Suspend,
    /// Resume execution after a suspension.
    Resume,
    /// Initiate a graceful shutdown.
    Retire,
    /// Finalize shutdown after draining work.
    Terminate,
    /// Immediately abort the agent, forcing termination.
    Abort,
}

/// Lifecycle state manager.
#[derive(Debug, Clone, Copy)]
pub struct Lifecycle {
    agent_id: AgentId,
    state: AgentState,
}

impl Lifecycle {
    /// Constructs a lifecycle controller for the given agent.
    #[must_use]
    pub const fn new(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            state: AgentState::Init,
        }
    }

    /// Returns the owning agent identifier.
    #[must_use]
    pub const fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Returns the current state.
    #[must_use]
    pub const fn state(&self) -> AgentState {
        self.state
    }

    /// Applies a lifecycle event, returning the resulting state.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidTransition`] when the supplied event is not
    /// allowed from the current state.
    pub fn transition(&mut self, event: LifecycleEvent) -> LifecycleResult<AgentState> {
        let next = match (self.state, event) {
            (AgentState::Init, LifecycleEvent::Boot) => Some(AgentState::Ready),
            (AgentState::Ready, LifecycleEvent::Activate)
            | (AgentState::Suspended, LifecycleEvent::Resume) => Some(AgentState::Active),
            (
                AgentState::Ready | AgentState::Active | AgentState::Suspended,
                LifecycleEvent::Retire,
            ) => Some(AgentState::Retiring),
            (AgentState::Active, LifecycleEvent::Suspend) => Some(AgentState::Suspended),
            (AgentState::Retiring | AgentState::Terminated, LifecycleEvent::Terminate)
            | (_, LifecycleEvent::Abort) => Some(AgentState::Terminated),
            _ => None,
        };

        let Some(next_state) = next else {
            return Err(LifecycleError::InvalidTransition {
                agent_id: self.agent_id,
                from: self.state,
                event,
            });
        };

        if next_state != self.state {
            debug!(
                agent_id = %self.agent_id,
                ?self.state,
                ?next_state,
                ?event,
                "agent lifecycle transition"
            );
            self.state = next_state;
        }

        Ok(self.state)
    }
}

/// Errors emitted by the lifecycle controller.
#[derive(Debug, Error)]
pub enum LifecycleError {
    /// Transition was not permitted from the current state.
    #[error("invalid lifecycle transition from {from:?} via {event:?} for agent {agent_id}")]
    InvalidTransition {
        /// Identifier of the agent whose transition failed.
        agent_id: AgentId,
        /// State prior to the attempted transition.
        from: AgentState,
        /// Event that triggered the failure.
        event: LifecycleEvent,
    },
}

/// Result alias used for lifecycle operations.
pub type LifecycleResult<T> = Result<T, LifecycleError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn new_id() -> AgentId {
        AgentId::random()
    }

    #[test]
    fn boot_to_active_flow() {
        let agent_id = new_id();
        let mut lifecycle = Lifecycle::new(agent_id);

        assert_eq!(lifecycle.state(), AgentState::Init);
        lifecycle.transition(LifecycleEvent::Boot).unwrap();
        assert_eq!(lifecycle.state(), AgentState::Ready);
        lifecycle.transition(LifecycleEvent::Activate).unwrap();
        assert!(lifecycle.state().is_active());
    }

    #[test]
    fn suspend_and_resume() {
        let agent_id = new_id();
        let mut lifecycle = Lifecycle::new(agent_id);

        lifecycle.transition(LifecycleEvent::Boot).unwrap();
        lifecycle.transition(LifecycleEvent::Activate).unwrap();
        lifecycle.transition(LifecycleEvent::Suspend).unwrap();
        assert_eq!(lifecycle.state(), AgentState::Suspended);
        lifecycle.transition(LifecycleEvent::Resume).unwrap();
        assert_eq!(lifecycle.state(), AgentState::Active);
    }

    #[test]
    fn abort_is_global() {
        let agent_id = new_id();
        let mut lifecycle = Lifecycle::new(agent_id);

        lifecycle.transition(LifecycleEvent::Abort).unwrap();
        assert!(lifecycle.state().is_terminal());
        // Further aborts keep the state terminal.
        lifecycle.transition(LifecycleEvent::Abort).unwrap();
        assert_eq!(lifecycle.state(), AgentState::Terminated);
    }

    #[test]
    fn invalid_transition_errors() {
        let agent_id = new_id();
        let mut lifecycle = Lifecycle::new(agent_id);

        let err = lifecycle
            .transition(LifecycleEvent::Activate)
            .expect_err("activate should fail from init");

        matches!(err, LifecycleError::InvalidTransition { .. });
    }
}
