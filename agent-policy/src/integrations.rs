//! Integrations with external governance systems.

use std::sync::Arc;

use async_trait::async_trait;

use crate::contracts::PolicyRequest;
use crate::decision::PolicyDecision;
use crate::engine::{PolicyEngine, PolicyResult};

/// Trait implemented by remote governance backends.
#[async_trait]
pub trait GovernanceClient: Send + Sync {
    /// Evaluates the supplied request and returns a decision from the backend.
    async fn evaluate(&self, request: &PolicyRequest) -> PolicyResult<PolicyDecision>;
}

/// Policy engine adapter that delegates to a remote governance client.
#[derive(Clone)]
pub struct RemotePolicyEngine<C>
where
    C: GovernanceClient + 'static,
{
    client: Arc<C>,
}

impl<C> RemotePolicyEngine<C>
where
    C: GovernanceClient + 'static,
{
    /// Creates a new remote policy engine using the provided client.
    #[must_use]
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<C> PolicyEngine for RemotePolicyEngine<C>
where
    C: GovernanceClient + 'static,
{
    async fn evaluate(&self, request: &PolicyRequest) -> PolicyResult<PolicyDecision> {
        self.client.evaluate(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_primitives::AgentId;
    use async_trait::async_trait;

    use crate::contracts::{PolicyAction, PolicyRequest};
    use crate::decision::PolicyDecision;

    struct StaticClient;

    #[async_trait]
    impl GovernanceClient for StaticClient {
        async fn evaluate(&self, _request: &PolicyRequest) -> PolicyResult<PolicyDecision> {
            Ok(PolicyDecision::allow())
        }
    }

    #[tokio::test]
    async fn remote_engine_delegates_to_client() {
        let engine = RemotePolicyEngine::new(Arc::new(StaticClient));
        let request = PolicyRequest::new(
            AgentId::random(),
            PolicyAction::InvokeTool {
                name: "echo".into(),
            },
        );

        let decision = engine.evaluate(&request).await.unwrap();
        assert!(decision.is_allow());
    }
}
