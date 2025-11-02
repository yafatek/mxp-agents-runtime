//! Policy decision types returned by engines.

use serde::{Deserialize, Serialize};

/// Describes the outcome of a policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// Action is permitted without further intervention.
    Allow,
    /// Action is rejected outright.
    Deny,
    /// Action requires human approval or additional checks before proceeding.
    Escalate,
}

/// Structured decision emitted by a policy engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    kind: DecisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    required_approvals: Vec<String>,
}

impl PolicyDecision {
    /// Returns an allow decision with no additional context.
    #[must_use]
    pub fn allow() -> Self {
        Self {
            kind: DecisionKind::Allow,
            reason: None,
            required_approvals: Vec::new(),
        }
    }

    /// Returns a deny decision with an explanatory reason.
    #[must_use]
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            kind: DecisionKind::Deny,
            reason: Some(reason.into()),
            required_approvals: Vec::new(),
        }
    }

    /// Returns an escalate decision with optional approver identifiers.
    #[must_use]
    pub fn escalate(reason: impl Into<String>, approvers: Vec<String>) -> Self {
        Self {
            kind: DecisionKind::Escalate,
            reason: Some(reason.into()),
            required_approvals: approvers,
        }
    }

    /// Returns the decision kind.
    #[must_use]
    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    /// Returns true when the decision allows the action to proceed.
    #[must_use]
    pub fn is_allow(&self) -> bool {
        self.kind == DecisionKind::Allow
    }

    /// Returns true when the decision denies the action.
    #[must_use]
    pub fn is_deny(&self) -> bool {
        self.kind == DecisionKind::Deny
    }

    /// Returns true when additional approvals are required.
    #[must_use]
    pub fn is_escalate(&self) -> bool {
        self.kind == DecisionKind::Escalate
    }

    /// Returns the optional reason associated with the decision.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// Returns approver identifiers required for escalation decisions.
    #[must_use]
    pub fn required_approvals(&self) -> &[String] {
        &self.required_approvals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_helpers_work() {
        let allow = PolicyDecision::allow();
        assert!(allow.is_allow());
        assert!(!allow.is_deny());

        let deny = PolicyDecision::deny("blocked");
        assert!(deny.is_deny());
        assert_eq!(deny.reason(), Some("blocked"));

        let escalate = PolicyDecision::escalate("needs approval", vec!["secops".into()]);
        assert!(escalate.is_escalate());
        assert_eq!(escalate.required_approvals(), ["secops"]);
    }
}
