//! Policy engine traits and rule-based implementation.

use std::collections::BTreeSet;
use std::sync::RwLock;

use agent_primitives::AgentId;
use async_trait::async_trait;
use thiserror::Error;
use tracing::debug;

use crate::contracts::{PolicyAction, PolicyRequest};
use crate::decision::PolicyDecision;

/// Errors surfaced by policy engines.
#[derive(Debug, Error)]
pub enum PolicyError {
    /// Request failed validation before evaluation.
    #[error("invalid policy request: {0}")]
    InvalidRequest(&'static str),
    /// Rule configuration error.
    #[error("invalid policy rule: {0}")]
    InvalidRule(&'static str),
    /// Backend integration returned an error.
    #[error("policy backend failure: {reason}")]
    Backend {
        /// Human-readable explanation for logging and operators.
        reason: String,
    },
}

/// Result alias for policy operations.
pub type PolicyResult<T> = Result<T, PolicyError>;

/// Trait implemented by policy engines.
#[async_trait]
pub trait PolicyEngine: Send + Sync {
    /// Evaluates the supplied policy request.
    async fn evaluate(&self, request: &PolicyRequest) -> PolicyResult<PolicyDecision>;
}

/// Matches a policy request based on action type and optional tags.
#[derive(Debug, Clone)]
pub struct RuleMatcher {
    action: ActionMatcher,
    required_tags: BTreeSet<String>,
}

impl RuleMatcher {
    /// Creates a matcher that accepts all actions.
    #[must_use]
    pub fn any() -> Self {
        Self {
            action: ActionMatcher::Any,
            required_tags: BTreeSet::new(),
        }
    }

    /// Creates a matcher targeting a specific tool name.
    #[must_use]
    pub fn for_tool(name: impl Into<String>) -> Self {
        Self {
            action: ActionMatcher::Tool {
                name: Some(name.into()),
            },
            required_tags: BTreeSet::new(),
        }
    }

    /// Creates a matcher targeting any tool invocation.
    #[must_use]
    pub fn for_any_tool() -> Self {
        Self {
            action: ActionMatcher::Tool { name: None },
            required_tags: BTreeSet::new(),
        }
    }

    /// Creates a matcher for model inference of a particular provider/model.
    #[must_use]
    pub fn for_model(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            action: ActionMatcher::Model {
                provider: Some(provider.into()),
                model: Some(model.into()),
            },
            required_tags: BTreeSet::new(),
        }
    }

    /// Creates a matcher for all model inference requests.
    #[must_use]
    pub fn for_any_model() -> Self {
        Self {
            action: ActionMatcher::Model {
                provider: None,
                model: None,
            },
            required_tags: BTreeSet::new(),
        }
    }

    /// Requires that the request carries the supplied tags.
    #[must_use]
    pub fn with_required_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            let tag = tag.into();
            if !tag.trim().is_empty() {
                self.required_tags.insert(tag);
            }
        }
        self
    }

    fn matches(&self, request: &PolicyRequest) -> bool {
        self.action.matches(request.agent_id(), request.action())
            && self
                .required_tags
                .iter()
                .all(|tag| request.context().tags().contains(tag))
    }
}

/// Matches requests based on the action shape.
#[derive(Debug, Clone)]
pub enum ActionMatcher {
    /// Match all actions.
    Any,
    /// Match tool invocations, optionally narrowing on name.
    Tool {
        /// Optional tool name to match.
        name: Option<String>,
    },
    /// Match model inference actions.
    Model {
        /// Optional provider name.
        provider: Option<String>,
        /// Optional model name.
        model: Option<String>,
    },
    /// Match event emissions.
    Event {
        /// Optional event type.
        event_type: Option<String>,
    },
}

impl ActionMatcher {
    fn matches(&self, agent_id: AgentId, action: &PolicyAction) -> bool {
        let _ = agent_id;
        match (self, action) {
            (Self::Any, _) => true,
            (Self::Tool { name }, PolicyAction::InvokeTool { name: action_name }) => {
                name.as_ref().is_none_or(|expected| expected == action_name)
            }
            (
                Self::Model { provider, model },
                PolicyAction::ModelInference {
                    provider: action_provider,
                    model: action_model,
                },
            ) => {
                provider
                    .as_ref()
                    .is_none_or(|expected| expected == action_provider)
                    && model
                        .as_ref()
                        .is_none_or(|expected| expected == action_model)
            }
            (
                Self::Event { event_type },
                PolicyAction::EmitEvent {
                    event_type: action_event,
                },
            ) => event_type
                .as_ref()
                .is_none_or(|expected| expected == action_event),
            _ => false,
        }
    }
}

/// Rule consisting of a matcher and a resulting decision.
#[derive(Debug, Clone)]
pub struct PolicyRule {
    name: String,
    matcher: RuleMatcher,
    decision: PolicyDecision,
}

impl PolicyRule {
    /// Creates a new rule with the supplied matcher and decision.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::InvalidRule`] when the rule name is empty.
    pub fn new(
        name: impl Into<String>,
        matcher: RuleMatcher,
        decision: PolicyDecision,
    ) -> PolicyResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(PolicyError::InvalidRule("rule name cannot be empty"));
        }

        Ok(Self {
            name,
            matcher,
            decision,
        })
    }

    /// Returns the rule name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the decision attached to the rule.
    #[must_use]
    pub fn decision(&self) -> &PolicyDecision {
        &self.decision
    }

    fn matches(&self, request: &PolicyRequest) -> bool {
        self.matcher.matches(request)
    }
}

/// Rule-based, in-memory policy engine.
#[derive(Debug)]
pub struct RuleBasedEngine {
    rules: RwLock<Vec<PolicyRule>>,
    default_decision: PolicyDecision,
}

impl RuleBasedEngine {
    /// Constructs a new rule-based engine with the provided default decision.
    #[must_use]
    pub fn new(default_decision: PolicyDecision) -> Self {
        Self {
            rules: RwLock::new(Vec::new()),
            default_decision,
        }
    }

    /// Adds a rule to the engine in insertion order.
    ///
    /// # Panics
    ///
    /// Panics if the internal rule store lock has been poisoned.
    pub fn add_rule(&self, rule: PolicyRule) {
        let mut guard = self.rules.write().expect("policy rules poisoned");
        guard.push(rule);
    }
}

#[async_trait]
impl PolicyEngine for RuleBasedEngine {
    async fn evaluate(&self, request: &PolicyRequest) -> PolicyResult<PolicyDecision> {
        let guard = self.rules.read().expect("policy rules poisoned");
        for rule in guard.iter() {
            if rule.matches(request) {
                debug!(rule = rule.name(), action = %request.action().label(), "policy rule matched");
                return Ok(rule.decision().clone());
            }
        }

        Ok(self.default_decision.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{PolicyAction, PolicyRequest};

    fn request_for_tool(name: &str) -> PolicyRequest {
        PolicyRequest::new(
            AgentId::random(),
            PolicyAction::InvokeTool { name: name.into() },
        )
    }

    #[tokio::test]
    async fn rule_matching_prefers_first_match() {
        let engine = RuleBasedEngine::new(PolicyDecision::allow());
        engine.add_rule(
            PolicyRule::new(
                "deny-echo",
                RuleMatcher::for_tool("echo"),
                PolicyDecision::deny("tool disabled"),
            )
            .unwrap(),
        );
        engine.add_rule(
            PolicyRule::new(
                "escalate-all-tools",
                RuleMatcher::for_any_tool(),
                PolicyDecision::escalate("needs approval", vec!["secops".into()]),
            )
            .unwrap(),
        );

        let decision = engine.evaluate(&request_for_tool("echo")).await.unwrap();
        assert!(decision.is_deny());
        assert_eq!(decision.reason(), Some("tool disabled"));

        let decision = engine.evaluate(&request_for_tool("other")).await.unwrap();
        assert!(decision.is_escalate());
    }

    #[tokio::test]
    async fn default_decision_applies_when_no_rules_match() {
        let engine = RuleBasedEngine::new(PolicyDecision::deny("no rules"));
        let decision = engine.evaluate(&request_for_tool("unknown")).await.unwrap();

        assert!(decision.is_deny());
    }

    #[tokio::test]
    async fn tag_matching_requires_subset() {
        let engine = RuleBasedEngine::new(PolicyDecision::allow());
        let matcher = RuleMatcher::for_any_tool().with_required_tags(["cap:write".to_owned()]);
        engine.add_rule(PolicyRule::new("cap-required", matcher, PolicyDecision::allow()).unwrap());

        let mut request = request_for_tool("writer");
        request
            .context_mut()
            .extend_tags(["cap:write", "tenant:a"].into_iter().map(String::from));

        let decision = engine.evaluate(&request).await.unwrap();
        assert!(decision.is_allow());

        let mut request = request_for_tool("writer");
        request
            .context_mut()
            .extend_tags(["cap:read"].into_iter().map(String::from));
        let decision = engine.evaluate(&request).await.unwrap();
        assert!(decision.is_allow());
    }
}
