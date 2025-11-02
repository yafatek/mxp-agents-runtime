//! Policy request and action contracts for governance evaluation.

use std::collections::BTreeSet;

use agent_memory::MemoryRecord;
use agent_primitives::AgentId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Describes the action being evaluated by the policy engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyAction {
    /// Request to invoke a registered tool.
    InvokeTool {
        /// Name of the tool to be invoked.
        name: String,
    },
    /// Request to execute an LLM inference via a model adapter.
    ModelInference {
        /// Provider backing the adapter (e.g. `ollama`, `openai`).
        provider: String,
        /// Concrete model identifier.
        model: String,
    },
    /// Request to emit an MXP event into the mesh.
    EmitEvent {
        /// Application-defined event type identifier.
        event_type: String,
    },
}

impl PolicyAction {
    /// Returns a concise, human-readable label for the action.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::InvokeTool { name } => format!("tool `{name}`"),
            Self::ModelInference { provider, model } => {
                format!("model `{provider}/{model}`")
            }
            Self::EmitEvent { event_type } => format!("event `{event_type}`"),
        }
    }
}

/// Context supplied to a policy evaluation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyContext {
    metadata: Map<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    tags: BTreeSet<String>,
}

impl PolicyContext {
    /// Inserts metadata into the request context.
    pub fn insert_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Adds metadata to the context and returns the updated instance.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.insert_metadata(key, value);
        self
    }

    /// Adds a tag to the context, ignoring empty or whitespace-only strings.
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !tag.trim().is_empty() {
            self.tags.insert(tag);
        }
    }

    /// Extends the context with multiple tags.
    pub fn extend_tags<I, S>(&mut self, tags: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            self.add_tag(tag);
        }
    }

    /// Returns the metadata associated with the context.
    #[must_use]
    pub fn metadata(&self) -> &Map<String, Value> {
        &self.metadata
    }

    /// Returns the tags associated with the context.
    #[must_use]
    pub fn tags(&self) -> &BTreeSet<String> {
        &self.tags
    }
}

/// Full request sent to the policy engine for evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequest {
    agent_id: AgentId,
    action: PolicyAction,
    #[serde(default)]
    context: PolicyContext,
}

impl PolicyRequest {
    /// Creates a policy request for the specified agent and action.
    #[must_use]
    pub fn new(agent_id: AgentId, action: PolicyAction) -> Self {
        Self {
            agent_id,
            action,
            context: PolicyContext::default(),
        }
    }

    /// Returns the agent identifier associated with the request.
    #[must_use]
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Returns the targeted policy action.
    #[must_use]
    pub fn action(&self) -> &PolicyAction {
        &self.action
    }

    /// Returns the context attached to the request.
    #[must_use]
    pub fn context(&self) -> &PolicyContext {
        &self.context
    }

    /// Returns a mutable reference to the context.
    pub fn context_mut(&mut self) -> &mut PolicyContext {
        &mut self.context
    }

    /// Adds metadata to the request context.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.context.insert_metadata(key, value);
        self
    }

    /// Adds a tag to the request context.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.context.add_tag(tag);
        self
    }

    /// Adds multiple tags to the context.
    #[must_use]
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.context.extend_tags(tags);
        self
    }

    /// Constructs a policy request describing a memory record emission.
    #[must_use]
    pub fn from_memory_record(agent_id: AgentId, record: &MemoryRecord) -> Self {
        let mut request = Self::new(
            agent_id,
            PolicyAction::EmitEvent {
                event_type: "memory_record".into(),
            },
        );

        request
            .context_mut()
            .insert_metadata("channel", Value::from(format!("{:?}", record.channel())));
        request
            .context_mut()
            .insert_metadata("tags", Value::from(record.tags().to_vec()));
        request
            .context_mut()
            .insert_metadata("id", Value::from(record.id().to_string()));
        request
            .context_mut()
            .extend_tags(record.tags().iter().cloned());

        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_context_manages_tags() {
        let mut ctx = PolicyContext::default();
        ctx.add_tag("alpha");
        ctx.add_tag("alpha");
        ctx.extend_tags(["beta", " ", "gamma"]);

        assert_eq!(ctx.tags().len(), 3);
        assert!(ctx.tags().contains("alpha"));
        assert!(ctx.tags().contains("beta"));
        assert!(ctx.tags().contains("gamma"));
    }

    #[test]
    fn request_builder_adds_metadata() {
        let agent = AgentId::random();
        let request = PolicyRequest::new(
            agent,
            PolicyAction::InvokeTool {
                name: "echo".into(),
            },
        )
        .with_metadata("foo", Value::from(1))
        .with_tag("cap:read");

        assert_eq!(request.context().metadata().len(), 1);
        assert!(request.context().tags().contains("cap:read"));
    }

    #[test]
    fn request_from_memory_record_contains_metadata() {
        use agent_memory::{MemoryChannel, MemoryRecord};
        use bytes::Bytes;

        let payload = Bytes::from_static(b"payload");
        let record = MemoryRecord::builder(MemoryChannel::Input, payload)
            .tag("cap:read")
            .unwrap()
            .build()
            .unwrap();
        let agent = AgentId::random();
        let request = PolicyRequest::from_memory_record(agent, &record);

        assert!(request.context().metadata().contains_key("channel"));
        assert!(request.context().metadata().contains_key("id"));
        assert!(request.context().tags().contains("cap:read"));
    }
}
