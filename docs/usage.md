## Using the MXP Agents SDK

This guide walks through installing `mxp-agents`, configuring the runtime, and running an
agent that speaks MXP with full governance and memory integration.

### 1. Install the Facade Crate

```sh
cargo add mxp-agents
```

Enable the components you need via feature flags. For a full runtime you typically select:

```toml
[dependencies]
mxp-agents = { version = "0.1", features = [
  "agent-kernel",
  "agent-adapters",
  "agent-tools",
  "agent-memory",
  "agent-policy",
] }
```

### 2. Define Your Agent Manifest

```rust
use mxp_agents::agent_primitives::{AgentManifest, Capability};

let manifest = AgentManifest::builder()
    .name("inventory-coordinator")
    .version("0.1.0")
    .capability(
        Capability::builder()
            .id("inv.lookup")
            .name("InventoryLookup")
            .scope("inventory:read")
            .build()?,
    )
    .tag("sre")
    .build()?;
```

### 3. Register Tools

Annotate an async function with `#[tool]` metadata. The macro generates JSON
glue, capability validation, and helper functions for registration.

```rust
use mxp_agents::agent_tools::macros::tool;
use mxp_agents::agent_tools::registry::{ToolRegistry, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct LookupRequest {
    sku: String,
}

#[derive(Serialize)]
struct LookupResponse {
    quantity: u32,
}

#[tool(
    name = "inv_lookup",
    version = "1.0.0",
    description = "Inventory lookup tool",
    capabilities = ["inventory.lookup"],
)]
async fn inventory_lookup(req: LookupRequest) -> ToolResult<LookupResponse> {
    // domain logic
    Ok(LookupResponse { quantity: 42 })
}

let registry = ToolRegistry::new();
register_inventory_lookup(&registry)?;
```

The macro also exposes `<tool_name>_binding()` if you prefer to add tools to a
batch before registration.

### 4. Configure Adapters

```rust
use mxp_agents::agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use std::sync::Arc;

let adapter = Arc::new(OllamaAdapter::new(
    OllamaConfig::default().with_model("deepseek-r1:latest"),
)?);
```

Other adapters (OpenAI, Anthropic, Gemini) follow the same pattern with provider-specific
configs. All connectors share the `ModelAdapter` trait for streaming inference.

### 5. Bootstrap Memory & Policy

```rust
use mxp_agents::agent_memory::{MemoryBusBuilder, VolatileConfig};
use mxp_agents::agent_policy::{PolicyDecision, PolicyRule, RuleBasedEngine, RuleMatcher};

let memory_bus = MemoryBusBuilder::new(VolatileConfig::default()).build()?;

let policy = RuleBasedEngine::new(PolicyDecision::allow());
policy.add_rule(
    PolicyRule::new(
        "deny-risky",
        RuleMatcher::for_tool("inv_delete"),
        PolicyDecision::deny("inventory deletion disabled"),
    )?,
);
```

Optionally attach observers to publish audit events:

```rust
use mxp_agents::agent_kernel::{
    AuditEmitter, CompositeAuditEmitter, CompositePolicyObserver, GovernanceAuditEmitter,
    MxpAuditObserver, TracingAuditEmitter, TracingPolicyObserver,
};
use mxp::Transport;

let mut audit_emitters: Vec<Arc<dyn AuditEmitter>> = vec![
    Arc::new(TracingAuditEmitter) as Arc<_>,
];

if let Some(governance_addr) = governance_addr {
    let transport = Transport::default();
    let handle = transport.bind("0.0.0.0:0".parse()?)?;
    audit_emitters.push(Arc::new(GovernanceAuditEmitter::new(handle, governance_addr)) as Arc<_>);
}

let audit_emitter: Arc<dyn AuditEmitter> = if audit_emitters.len() == 1 {
    Arc::clone(&audit_emitters[0])
} else {
    Arc::new(CompositeAuditEmitter::new(audit_emitters))
};

let observer = CompositePolicyObserver::new([
    Arc::new(TracingPolicyObserver) as Arc<_>,
    Arc::new(MxpAuditObserver::new(audit_emitter)) as Arc<_>,
]);
```

### 6. Assemble the Kernel

```rust
use mxp_agents::agent_kernel::{KernelMessageHandler, TracingCallSink};

let call_sink = Arc::new(TracingCallSink::default());
let handler = KernelMessageHandler::new(adapter, Arc::new(registry), call_sink)
    .with_memory(Arc::new(memory_bus))
    .with_policy(Arc::new(policy))
    .with_policy_observer(observer);

let kernel = AgentKernel::new(manifest, handler) // plus registry + scheduler config
    .with_scheduler(SchedulerConfig::default())
    .spawn()?;
```

### 7. Run & Observe
- Send MXP `Call` messages to the kernel to trigger `CallExecutor`.
- Tool invocations, model responses, and memory writes will appear in the configured journal.
- Policy denials/escalations emit tracing logs and MXP audit events for governance agents.

### Example Project

See `examples/basic-agent` for a ready-made binary that:
- connects to a local Ollama instance,
- registers sample tools,
- enforces a rule-based policy,
- persists transcripts to disk,
- emits audit events via `TracingAuditEmitter` and optionally forwards them to a governance agent.

Run it with:

```sh
cargo run -p basic-agent -- --governance 127.0.0.1:9100
```

### Next Steps
- Wire up your own MXP transport so the kernel can register, heartbeat, and receive messages from the Relay mesh.
- Extend the policy engine with escalation flows to human operators.
- Implement vector store backends that satisfy the `VectorStoreClient` trait for semantic recall.

