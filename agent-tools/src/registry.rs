//! Runtime registry for tool metadata and execution.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use agent_primitives::CapabilityId;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Result alias for tool operations.
pub type ToolResult<T> = Result<T, ToolError>;

/// Future alias produced by generated tool bindings.
pub type ToolFuture = Pin<Box<dyn Future<Output = ToolResult<Value>> + Send>>;

/// Declarative binding returned by the `#[tool]` macro.
#[derive(Clone)]
pub struct ToolBinding {
    metadata: ToolMetadata,
    executor: fn(Value) -> ToolFuture,
}

impl ToolBinding {
    /// Creates a new tool binding from metadata and an executor function.
    #[must_use]
    pub fn new(metadata: ToolMetadata, executor: fn(Value) -> ToolFuture) -> Self {
        Self { metadata, executor }
    }

    /// Returns the metadata associated with this binding.
    #[must_use]
    pub fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    /// Registers the binding with the provided registry.
    ///
    /// # Errors
    ///
    /// Propagates [`ToolError::DuplicateTool`] if a tool with the same name
    /// has already been registered.
    pub fn register(self, registry: &ToolRegistry) -> ToolResult<()> {
        let ToolBinding { metadata, executor } = self;
        registry.register_tool(metadata, executor)
    }
}

/// Metadata describing a registered tool.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolMetadata {
    name: String,
    version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    capabilities: Vec<CapabilityId>,
}

impl ToolMetadata {
    /// Creates metadata for the supplied identifier and version.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::InvalidMetadata`] if either field is empty.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> ToolResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ToolError::InvalidMetadata {
                reason: "tool name cannot be empty".into(),
            });
        }

        let version = version.into();
        if version.trim().is_empty() {
            return Err(ToolError::InvalidMetadata {
                reason: "tool version cannot be empty".into(),
            });
        }

        Ok(Self {
            name,
            version,
            description: None,
            capabilities: Vec::new(),
        })
    }

    /// Sets the human-readable description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Attaches capability identifiers required for invocation.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: Vec<CapabilityId>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Returns the tool name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the semantic version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the optional description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the required capability identifiers.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityId] {
        &self.capabilities
    }
}

/// Trait implemented by tool executors.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Invokes the tool with the given JSON input, returning JSON output.
    async fn invoke(&self, input: Value) -> ToolResult<Value>;
}

#[async_trait]
impl<F, Fut> Tool for F
where
    F: Send + Sync + Fn(Value) -> Fut,
    Fut: Future<Output = ToolResult<Value>> + Send,
{
    async fn invoke(&self, input: Value) -> ToolResult<Value> {
        (self)(input).await
    }
}

/// Handle returned by the registry for direct invocation.
#[derive(Clone)]
pub struct ToolHandle {
    metadata: ToolMetadata,
    executor: Arc<dyn Tool>,
}

impl ToolHandle {
    /// Returns the associated metadata.
    #[must_use]
    pub fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    /// Executes the underlying tool implementation.
    ///
    /// # Errors
    ///
    /// Propagates any [`ToolError::Execution`] returned by the underlying
    /// implementation.
    pub async fn invoke(&self, input: Value) -> ToolResult<Value> {
        self.executor.invoke(input).await
    }
}

/// Registry that stores tool implementations keyed by name.
#[derive(Default)]
pub struct ToolRegistry {
    inner: RwLock<HashMap<String, ToolHandle>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.read().expect("tool registry poisoned");
        let names: Vec<_> = inner.keys().cloned().collect();
        f.debug_struct("ToolRegistry")
            .field("registered", &names)
            .finish()
    }
}

impl ToolRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a tool implementation.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::DuplicateTool`] if the name is already present.
    ///
    /// # Panics
    ///
    /// Panics if the internal registry lock is poisoned.
    pub fn register_tool<T>(&self, metadata: ToolMetadata, tool: T) -> ToolResult<()>
    where
        T: Tool + 'static,
    {
        let mut inner = self.inner.write().expect("tool registry poisoned");
        let name = metadata.name().to_owned();
        if inner.contains_key(&name) {
            return Err(ToolError::DuplicateTool { name });
        }

        inner.insert(
            name,
            ToolHandle {
                metadata,
                executor: Arc::new(tool),
            },
        );

        Ok(())
    }

    /// Registers a binding produced by the `#[tool]` macro.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::DuplicateTool`] if the binding name already exists
    /// within the registry.
    pub fn register_binding(&self, binding: ToolBinding) -> ToolResult<()> {
        binding.register(self)
    }

    /// Returns a handle to the tool matching the supplied name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<ToolHandle> {
        let inner = self.inner.read().ok()?;
        inner.get(name).cloned()
    }

    /// Invokes a registered tool directly.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::UnknownTool`] when the tool is not found or
    /// propagates [`ToolError::Execution`] when the implementation fails.
    pub async fn invoke(&self, name: &str, input: Value) -> ToolResult<Value> {
        let handle = self.get(name).ok_or_else(|| ToolError::UnknownTool {
            name: name.to_owned(),
        })?;
        handle.invoke(input).await
    }

    /// Lists the metadata of all registered tools.
    ///
    /// # Panics
    ///
    /// Panics if the internal registry lock is poisoned.
    #[must_use]
    pub fn list(&self) -> Vec<ToolMetadata> {
        let inner = self.inner.read().expect("tool registry poisoned");
        inner
            .values()
            .map(|handle| handle.metadata.clone())
            .collect()
    }
}

/// Errors produced by tool registration and invocation.
#[derive(Debug, Error)]
pub enum ToolError {
    /// Tool metadata failed validation.
    #[error("invalid tool metadata: {reason}")]
    InvalidMetadata {
        /// Human-readable reason for rejection.
        reason: String,
    },

    /// Tool name collided with an existing registration.
    #[error("tool `{name}` is already registered")]
    DuplicateTool {
        /// Name of the offending tool.
        name: String,
    },

    /// Requested tool does not exist.
    #[error("tool `{name}` is not registered")]
    UnknownTool {
        /// Name of the missing tool.
        name: String,
    },

    /// Tool execution failed.
    #[error("tool execution failed: {reason}")]
    Execution {
        /// Human-readable error returned by the tool implementation.
        reason: String,
    },
}

impl ToolError {
    /// Creates an execution error from the supplied reason.
    #[must_use]
    pub fn execution(reason: impl Into<String>) -> Self {
        Self::Execution {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agent_primitives::CapabilityId;

    fn metadata() -> ToolMetadata {
        ToolMetadata::new("echo", "1.0.0")
            .unwrap()
            .with_description("Echo incoming payload")
            .with_capabilities(vec![CapabilityId::new("tool.echo").unwrap()])
    }

    #[tokio::test]
    async fn register_and_invoke_tool() {
        let registry = ToolRegistry::new();
        registry
            .register_tool(metadata(), |input: Value| async move { Ok(input) })
            .unwrap();

        let payload = serde_json::json!({ "message": "hello" });
        let output = registry.invoke("echo", payload.clone()).await.unwrap();
        assert_eq!(output, payload);
    }

    #[tokio::test]
    async fn register_binding_invokes_executor() {
        let registry = ToolRegistry::new();
        let binding = ToolBinding::new(metadata(), |input: Value| -> ToolFuture {
            Box::pin(async move { Ok(input) })
        });

        registry.register_binding(binding).unwrap();

        let payload = serde_json::json!({ "message": "binding" });
        let output = registry.invoke("echo", payload.clone()).await.unwrap();
        assert_eq!(output, payload);
    }

    #[tokio::test]
    async fn duplicate_registration_errors() {
        let registry = ToolRegistry::new();

        registry
            .register_tool(metadata(), |input: Value| async move { Ok(input) })
            .unwrap();

        let err = registry
            .register_tool(
                ToolMetadata::new("echo", "1.0.1").unwrap(),
                |v: Value| async move { Ok(v) },
            )
            .expect_err("duplicate registration should fail");

        assert!(matches!(err, ToolError::DuplicateTool { name } if name == "echo"));
    }

    #[tokio::test]
    async fn unknown_tool_errors() {
        let registry = ToolRegistry::new();
        let err = registry
            .invoke("missing", Value::Null)
            .await
            .expect_err("unknown tool should error");

        assert!(matches!(err, ToolError::UnknownTool { name } if name == "missing"));
    }

    #[tokio::test]
    async fn invalid_metadata_errors() {
        let err = ToolMetadata::new("", "1.0.0").expect_err("empty name should error");
        assert!(matches!(err, ToolError::InvalidMetadata { .. }));

        let err = ToolMetadata::new("echo", " ").expect_err("empty version should error");
        assert!(matches!(err, ToolError::InvalidMetadata { .. }));
    }
}
