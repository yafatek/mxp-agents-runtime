//! Capability descriptors shared across the agent runtime.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const MAX_ID_LEN: usize = 64;
const MAX_NAME_LEN: usize = 96;
const MAX_SCOPE_LEN: usize = 64;

/// Identifier for a capability that an agent may expose.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Creates a new capability identifier after validating its format.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCapabilityId`] if the supplied identifier is empty,
    /// too long, or contains unsupported characters.
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        validate_identifier(&id)?;
        Ok(Self(id))
    }

    /// Returns the capability identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<CapabilityId> for String {
    fn from(value: CapabilityId) -> Self {
        value.0
    }
}

fn validate_identifier(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(Error::InvalidCapabilityId {
            id: String::new(),
            reason: "identifier cannot be empty".into(),
        });
    }

    if id.len() > MAX_ID_LEN {
        return Err(Error::InvalidCapabilityId {
            id: id.into(),
            reason: format!("identifier length must be <= {MAX_ID_LEN}"),
        });
    }

    if !id
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-' | '_' | '.'))
    {
        return Err(Error::InvalidCapabilityId {
            id: id.into(),
            reason: "identifier must contain lowercase alphanumeric, dash, underscore, or dot"
                .into(),
        });
    }

    Ok(())
}

/// Describes a capability exposed by an agent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    id: CapabilityId,
    name: String,
    description: Option<String>,
    version: String,
    scopes: Vec<String>,
}

impl Capability {
    /// Starts building a capability descriptor.
    #[must_use]
    pub fn builder(id: CapabilityId) -> CapabilityBuilder {
        CapabilityBuilder {
            id,
            name: None,
            description: None,
            version: None,
            scopes: BTreeSet::new(),
        }
    }

    /// Returns the unique capability identifier.
    #[must_use]
    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    /// Human-friendly capability name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Optional capability description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Semantic version string of the capability schema.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Capability scopes advertised to the governance engine.
    #[must_use]
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
}

/// Builder for [`Capability`].
pub struct CapabilityBuilder {
    id: CapabilityId,
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    scopes: BTreeSet<String>,
}

impl CapabilityBuilder {
    /// Sets the display name for the capability.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCapability`] if the name is empty or exceeds the
    /// maximum supported length.
    pub fn name(mut self, name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Error::InvalidCapability {
                reason: "name cannot be empty".into(),
            });
        }
        if name.len() > MAX_NAME_LEN {
            return Err(Error::InvalidCapability {
                reason: format!("name length must be <= {MAX_NAME_LEN}"),
            });
        }
        self.name = Some(name);
        Ok(self)
    }

    /// Sets an optional description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the version string for the capability.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCapability`] if the version string is empty.
    pub fn version(mut self, version: impl Into<String>) -> Result<Self> {
        let version = version.into();
        if version.trim().is_empty() {
            return Err(Error::InvalidCapability {
                reason: "version cannot be empty".into(),
            });
        }
        self.version = Some(version);
        Ok(self)
    }

    /// Adds a scope entry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCapability`] if the scope is empty or exceeds the
    /// maximum supported length.
    pub fn add_scope(mut self, scope: impl Into<String>) -> Result<Self> {
        let scope = scope.into();
        validate_scope(&scope)?;
        self.scopes.insert(scope);
        Ok(self)
    }

    /// Finalises the capability descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCapability`] if required fields are missing or no
    /// scopes were registered.
    pub fn build(self) -> Result<Capability> {
        let name = self.name.ok_or_else(|| Error::InvalidCapability {
            reason: "name must be provided".into(),
        })?;

        let version = self.version.ok_or_else(|| Error::InvalidCapability {
            reason: "version must be provided".into(),
        })?;

        let scopes = if self.scopes.is_empty() {
            return Err(Error::InvalidCapability {
                reason: "at least one scope must be specified".into(),
            });
        } else {
            self.scopes.into_iter().collect()
        };

        Ok(Capability {
            id: self.id,
            name,
            description: self.description,
            version,
            scopes,
        })
    }
}

fn validate_scope(scope: &str) -> Result<()> {
    if scope.trim().is_empty() {
        return Err(Error::InvalidCapability {
            reason: "scope cannot be empty".into(),
        });
    }
    if scope.len() > MAX_SCOPE_LEN {
        return Err(Error::InvalidCapability {
            reason: format!("scope length must be <= {MAX_SCOPE_LEN}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_capability_success() {
        let id = CapabilityId::new("plan.execute").expect("id");
        let capability = Capability::builder(id)
            .name("Planner")
            .and_then(|b| b.version("1.0.0"))
            .and_then(|b| b.add_scope("read:tasks"))
            .and_then(|b| b.add_scope("write:plans"))
            .map(|b| b.description("Plan execution"))
            .and_then(CapabilityBuilder::build)
            .expect("build");

        assert_eq!(capability.name(), "Planner");
        assert_eq!(capability.scopes().len(), 2);
    }

    #[test]
    fn capability_requires_scope() {
        let id = CapabilityId::new("empty.scope").expect("id");
        let err = Capability::builder(id)
            .name("Empty")
            .and_then(|b| b.version("1.0"))
            .and_then(CapabilityBuilder::build)
            .expect_err("should fail");

        matches!(err, Error::InvalidCapability { .. });
    }
}
