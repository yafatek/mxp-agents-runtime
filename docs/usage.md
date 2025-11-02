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
use mxp_agents::agent_tools::registry::ToolResult;
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
```

Every tool function annotated this way is registered automatically through the builder when
you pass the function pointers directly.

#### Tool Anatomy

- **Input/Output types**: Any `Deserialize`/`Serialize` data structures are valid; the macro
  performs conversion and surfaces detailed decoding/encoding errors.
- **Metadata**: `name` + `version` are required. `description` and `capabilities` enrich the
  registry and downstream policy decisions.
- **Generated helpers**:
  - `<fn>_binding()` → returns a `ToolBinding` with metadata + executor (useful for advanced
    scenarios or tests).
  - `register_<fn>()` → convenience wrapper that directly registers into a `ToolRegistry` when
    you need full manual control.

#### Registering via Binding

```rust
let binding = inventory_lookup_binding()?;
// inspect metadata before registration
assert_eq!(binding.metadata().name(), "inv_lookup");
registry.register_binding(binding)?;
```

#### Executing Tools Directly

```rust
let payload = serde_json::json!({ "sku": "ABC-123" });
let result = registry.invoke("inv_lookup", payload).await?;
println!("lookup result: {result}");
```

### 4. Tool Capabilities & Policy Hooks

Define capability descriptors in `agent-primitives` so that policy engines can reason about tool
access. Associate them with the tool metadata produced by the macro.

```rust
use mxp_agents::agent_primitives::{Capability, CapabilityId};

fn inventory_capability() -> mxp_agents::agent_primitives::Result<Capability> {
    Capability::builder(CapabilityId::new("inventory.lookup")?)
        .name("Inventory Lookup")?
        .version("1.0.0")?
        .add_scope("inventory:read")?
        .build()
}

// Later, attach the capability id to the tool metadata (either by specifying
// `capabilities = ["inventory.lookup"]` in the macro or mutating the binding).
```

With policy enabled, the `CallExecutor` will pass tool metadata (including capabilities) to
`PolicyEngine::evaluate`. Combine this with `RuleMatcher::for_tool("inv_lookup")` to allow/deny
invocations or require escalation.

### 5. Configure Adapters

```rust
use mxp_agents::agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use std::sync::Arc;

let adapter = Arc::new(OllamaAdapter::new(
    OllamaConfig::new("gemma2:2b"),
)?);
```

Other adapters (OpenAI, Anthropic, Gemini) follow the same pattern with provider-specific
configs. All connectors share the `ModelAdapter` trait for streaming inference.

#### Supported Adapters

**OpenAI**
```rust
use mxp_agents::agent_adapters::openai::{OpenAiAdapter, OpenAiConfig};

let adapter = OpenAiAdapter::new(
    OpenAiConfig::from_env("gpt-4")
        .with_default_temperature(0.7)
)?;
```

**Anthropic**
```rust
use mxp_agents::agent_adapters::anthropic::{AnthropicAdapter, AnthropicConfig};

let adapter = AnthropicAdapter::new(
    AnthropicConfig::from_env("claude-3-5-sonnet-20241022")
        .with_default_max_tokens(4096)
)?;
```

**Gemini**
```rust
use mxp_agents::agent_adapters::gemini::{GeminiAdapter, GeminiConfig};

let adapter = GeminiAdapter::new(
    GeminiConfig::from_env("gemini-1.5-pro")
)?;
```

**Ollama (Local)**
```rust
use mxp_agents::agent_adapters::ollama::{OllamaAdapter, OllamaConfig};

let adapter = OllamaAdapter::new(
    OllamaConfig::new("gemma2:2b")
        .with_base_url("http://localhost:11434")?
)?;
```

### 5a. System Prompts

System prompts guide model behavior and are supported across all adapters with provider-native optimizations.

#### Basic Usage

```rust
use mxp_agents::agent_adapters::traits::{InferenceRequest, MessageRole, PromptMessage};

let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "What is MXP?"),
])?
.with_system_prompt("You are an expert on MXP protocol. Be concise and technical.");
```

#### Provider-Specific Handling

The SDK automatically transforms system prompts to each provider's native format:

- **OpenAI/Ollama**: Prepends as `{"role": "system", "content": "..."}`
- **Anthropic**: Uses dedicated `"system": "..."` parameter (separate from messages)
- **Gemini**: Uses `"systemInstruction": {"parts": [...]}` field

This means you write the same code, and each adapter optimizes for its provider:

```rust
// Same code works across all providers
let adapters = vec![
    openai_adapter,
    anthropic_adapter,
    gemini_adapter,
    ollama_adapter,
];

for adapter in adapters {
    let request = InferenceRequest::new(messages.clone())?
        .with_system_prompt("You are helpful");
    
    let response = adapter.infer(request).await?;
    // Each adapter uses provider-native format
}
```

#### Template-Based System Prompts

For dynamic system prompts, use the template system:

```rust
use mxp_agents::agent_prompts::{PromptTemplate, TemplateBuilder};

let template = PromptTemplate::builder(
    "You are {{agent_name}}, an AI agent in the MXP mesh. {{personality}}"
)
.with_variable("agent_name", "RelayBot")
.with_variable("personality", "You are concise, technical, and helpful.")
.build()?;

let system_prompt = template.render()?;

let request = InferenceRequest::new(messages)?
    .with_system_prompt(system_prompt);
```

#### Runtime Variable Substitution

```rust
use std::collections::HashMap;

let template = PromptTemplate::builder("You are {{role}}. {{task}}")
    .with_variable("role", "a code reviewer")
    .with_variable("task", "Review Rust code for best practices")
    .build()?;

// Override at runtime
let mut runtime_vars = HashMap::new();
runtime_vars.insert("role".to_owned(), "a security auditor".to_owned());
runtime_vars.insert("task".to_owned(), "Find security vulnerabilities".to_owned());

let specialized_prompt = template.render_with(&runtime_vars)?;
```

#### Best Practices

1. **Be Specific**: Clear instructions yield better results
   ```rust
   // ❌ Vague
   .with_system_prompt("Be helpful")
   
   // ✅ Specific
   .with_system_prompt("You are an expert Rust developer. Provide code examples with explanations. Focus on safety and performance.")
   ```

2. **Set Constraints**: Define what the model should/shouldn't do
   ```rust
   .with_system_prompt(
       "You are a customer service agent. Be polite and professional. \
        Never share internal company information. \
        If you don't know something, say so clearly."
   )
   ```

3. **Use Templates for Consistency**: Reuse system prompts across agents
   ```rust
   const BASE_PROMPT: &str = "You are {{agent_type}} in the MXP mesh. {{guidelines}}";
   
   let template = PromptTemplate::builder(BASE_PROMPT)
       .with_variable("guidelines", "Follow MXP protocol standards.")
       .build()?;
   ```

### 5b. Context Window Management (Optional)

For long conversations, enable automatic context management to stay within token budgets:

```rust
use mxp_agents::agent_prompts::ContextWindowConfig;

let adapter = OllamaAdapter::new(config)?
    .with_context_config(ContextWindowConfig {
        max_tokens: 4096,                  // Token budget
        recent_window_size: 10,            // Always keep last N messages
        min_importance_threshold: 30,      // Remove low-importance first
        enable_summarization: true,        // Compress older messages
    });
```

#### How It Works

The SDK uses a three-tier strategy:

1. **Recent Window**: Last N messages always preserved
2. **Importance Scoring**: High-value messages (tool calls, decisions) kept longer
3. **Compression**: Older messages summarized when budget exceeded

#### Message Importance

```rust
use mxp_agents::agent_prompts::ContextMessage;

// High importance - never removed
let critical = ContextMessage::new("system", "Critical context")
    .with_importance(100)
    .pinned();

// Medium importance
let normal = ContextMessage::new("user", "Regular question")
    .with_importance(50);

// Low importance - removed first
let filler = ContextMessage::new("assistant", "Acknowledgment")
    .with_importance(20);
```

#### Manual Context Management

For advanced use cases, manage context directly:

```rust
use mxp_agents::agent_prompts::ContextWindowManager;

let mut manager = ContextWindowManager::new(ContextWindowConfig::default());

// Add messages
manager.add_message(ContextMessage::new("user", "Hello"));
manager.add_message(ContextMessage::new("assistant", "Hi there!"));

// Get managed messages
let messages = manager.get_messages();

// Check token usage
println!("Tokens: {} / {}", manager.current_tokens(), manager.max_tokens());

// Get summarized history if available
if let Some(summary) = manager.summarized_history() {
    println!("History: {}", summary);
}
```

### 6. Bootstrap Memory & Policy

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

### 7. Assemble the Kernel

```rust
use mxp_agents::agent_kernel::{KernelMessageHandler, TracingCallSink};

let call_sink = Arc::new(TracingCallSink::default());
let handler = KernelMessageHandler::builder(adapter, call_sink)
    .with_tool_functions([inventory_lookup])?
    .with_memory(Arc::new(memory_bus))
    .with_policy(Arc::new(policy))
    .with_policy_observer(observer)
    .build()?;

let kernel = AgentKernel::new(manifest, handler) // plus registry + scheduler config
    .with_scheduler(SchedulerConfig::default())
    .spawn()?;
```

### 8. Run & Observe
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

