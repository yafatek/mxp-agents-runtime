//! Governance policy evaluation for agents.

#![warn(missing_docs, clippy::pedantic)]

mod contracts;
mod decision;
mod engine;
mod integrations;

pub use contracts::{PolicyAction, PolicyContext, PolicyRequest};
pub use decision::{DecisionKind, PolicyDecision};
pub use engine::{
    ActionMatcher, PolicyEngine, PolicyError, PolicyResult, PolicyRule, RuleBasedEngine,
    RuleMatcher,
};
pub use integrations::{GovernanceClient, RemotePolicyEngine};
