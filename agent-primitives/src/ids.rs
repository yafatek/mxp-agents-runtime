//! Agent identifier types.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Error;

/// Unique identifier for an agent participating in the MXP mesh.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Generates a random agent identifier.
    #[must_use]
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates an identifier from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::random()
    }
}

impl Display for AgentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<Uuid> for AgentId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<AgentId> for Uuid {
    fn from(value: AgentId) -> Self {
        value.0
    }
}

impl FromStr for AgentId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(s).map_err(Error::from)?;
        Ok(Self::from_uuid(uuid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_agent_id() {
        let id = AgentId::random();
        let parsed = id.to_string().parse::<AgentId>().expect("parse");
        assert_eq!(id, parsed);
    }
}
