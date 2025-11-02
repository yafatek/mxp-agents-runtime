//! Agent metadata advertised to the Relay mesh directory.

use serde::{Deserialize, Serialize};

use crate::{AgentId, Capability};

#[cfg(test)]
use crate::{CapabilityBuilder, CapabilityId};

/// Human-readable description of an agent's identity and capabilities.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentManifest {
    id: AgentId,
    name: String,
    version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

impl AgentManifest {
    /// Starts building an [`AgentManifest`].
    #[must_use]
    pub fn builder(id: AgentId) -> AgentManifestBuilder {
        AgentManifestBuilder {
            id,
            name: None,
            version: None,
            description: None,
            capabilities: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// Returns the agent identifier.
    #[must_use]
    pub const fn id(&self) -> AgentId {
        self.id
    }

    /// Returns the agent display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the semantic version string identifying the agent build.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the optional description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the advertised capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    /// Returns any optional tags associated with the agent.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// Builder for [`AgentManifest`].
#[derive(Debug)]
pub struct AgentManifestBuilder {
    id: AgentId,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    capabilities: Vec<Capability>,
    tags: Vec<String>,
}

impl AgentManifestBuilder {
    /// Sets the human-readable name for the agent.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InvalidCapability`] when the name is empty. The
    /// reuse of the capability error keeps dependency weight low; callers should
    /// treat it as an input validation failure.
    pub fn name(mut self, name: impl Into<String>) -> crate::Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(crate::Error::InvalidCapability {
                reason: "manifest name cannot be empty".into(),
            });
        }
        self.name = Some(name);
        Ok(self)
    }

    /// Sets the semantic version string.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InvalidCapability`] when the version string is empty.
    pub fn version(mut self, version: impl Into<String>) -> crate::Result<Self> {
        let version = version.into();
        if version.trim().is_empty() {
            return Err(crate::Error::InvalidCapability {
                reason: "manifest version cannot be empty".into(),
            });
        }
        self.version = Some(version);
        Ok(self)
    }

    /// Sets an optional description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Replaces the capability set.
    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<Capability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Adds a tag label.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InvalidCapability`] when the supplied tag is empty.
    pub fn add_tag(mut self, tag: impl Into<String>) -> crate::Result<Self> {
        let tag = tag.into();
        if tag.trim().is_empty() {
            return Err(crate::Error::InvalidCapability {
                reason: "manifest tag cannot be empty".into(),
            });
        }
        self.tags.push(tag);
        Ok(self)
    }

    /// consumes builder and returns manifest.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InvalidCapability`] if mandatory fields are missing
    /// or contain invalid data.
    pub fn build(self) -> crate::Result<AgentManifest> {
        let name = self.name.ok_or_else(|| crate::Error::InvalidCapability {
            reason: "manifest name must be provided".into(),
        })?;
        let version = self
            .version
            .ok_or_else(|| crate::Error::InvalidCapability {
                reason: "manifest version must be provided".into(),
            })?;

        Ok(AgentManifest {
            id: self.id,
            name,
            version,
            description: self.description,
            capabilities: self.capabilities,
            tags: self.tags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_capability() -> Capability {
        Capability::builder(CapabilityId::new("test.cap").expect("id"))
            .name("Test")
            .and_then(|b| b.version("1.0.0"))
            .and_then(|b| b.add_scope("read:test"))
            .and_then(CapabilityBuilder::build)
            .expect("capability")
    }

    #[test]
    fn builds_manifest() {
        let manifest = AgentManifest::builder(AgentId::random())
            .name("demo")
            .unwrap()
            .version("1.2.3")
            .unwrap()
            .description("demo agent")
            .capabilities(vec![base_capability()])
            .add_tag("alpha")
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(manifest.name(), "demo");
        assert_eq!(manifest.version(), "1.2.3");
        assert_eq!(manifest.description(), Some("demo agent"));
        assert_eq!(manifest.capabilities().len(), 1);
        assert_eq!(manifest.tags(), ["alpha"]);
    }

    #[test]
    fn name_is_required() {
        let result = AgentManifest::builder(AgentId::random()).build();
        assert!(result.is_err());
    }
}
