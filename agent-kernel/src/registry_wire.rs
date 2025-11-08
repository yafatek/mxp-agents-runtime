//! Wire-level structures for communicating with the MXP Nexus registry over MXP.

use std::collections::HashMap;
use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Registration payload emitted by agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    /// Unique identifier for the agent (UUID string representation).
    pub id: String,
    /// Human readable agent name.
    pub name: String,
    /// Capabilities advertised by the agent.
    pub capabilities: Vec<String>,
    /// MXP endpoint where the agent is reachable.
    pub address: SocketAddr,
    /// Additional metadata such as version, description, tags, etc.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Successful registration acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    /// Indicates whether the registration succeeded.
    pub success: bool,
    /// Agent identifier acknowledged by the registry.
    pub agent_id: String,
    /// Informational message.
    pub message: String,
}

/// Agent discovery request payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverRequest {
    /// Capability filter.
    pub capability: String,
}

/// Snapshot of an agent returned by discovery calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    /// Agent identifier.
    pub id: String,
    /// Human readable name.
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Capability identifiers.
    pub capabilities: Vec<String>,
    /// Optional tags associated with the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// MXP endpoint address.
    pub address: SocketAddr,
    /// Reported health status.
    pub status: AgentStatus,
    /// Timestamp of the last heartbeat observed by the registry.
    pub last_heartbeat: DateTime<Utc>,
    /// Timestamp the agent was first registered.
    pub registered_at: DateTime<Utc>,
}

/// Discovery response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverResponse {
    /// Capability that was queried.
    pub capability: String,
    /// Matching agents.
    pub agents: Vec<AgentRecord>,
    /// Count of returned agents.
    pub count: usize,
}

/// Heartbeat request emitted by agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Identifier of the agent sending the heartbeat.
    pub agent_id: String,
}

/// Heartbeat acknowledgement returned to agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// Indicates whether the heartbeat succeeded.
    pub success: bool,
    /// Signals that the agent must re-register.
    pub needs_register: bool,
    /// Agent identifier associated with the heartbeat.
    pub agent_id: String,
    /// Registry timestamp recorded for the heartbeat.
    pub timestamp: DateTime<Utc>,
    /// Optional informational message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Error payload used for protocol error responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Human readable error message.
    pub error: String,
    /// Machine readable error code.
    pub code: String,
}

/// Simplified agent status representation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent is healthy and online.
    Online,
    /// Agent missed heartbeats and is considered offline.
    Offline,
    /// Agent is online but reporting degraded health.
    Degraded,
}
