## MXP Agents SDK Features

This document enumerates the capabilities available in the `mxp-agents` bundle and the
individual crates that compose it. Every feature listed here is production-grade—no
mocked implementations ship in the public facade.

### Facade Crate (`mxp-agents`)
- Single dependency that re-exports the core runtime behind feature flags.
- Feature groups:
  - `agent-kernel`: lifecycle orchestration, MXP handlers, tool and model execution.
  - `agent-memory`: volatile ring buffer, persisted journal, vector store interfaces.
  - `agent-policy`: rule-based governance, observers, MXP audit emission hooks.
  - `agent-adapters`: LLM connectors (OpenAI, Anthropic, Gemini, Ollama, MXP hosted).
  - `agent-tools`: `#[tool]` macro, registry, sandbox placeholder for future isolation.
  - `agent-prompts`: prompt orchestration (phase 2 backlog).

### Agent Kernel
- Deterministic lifecycle state machine (`Init → Ready → Active → Suspended → Retiring → Terminated`).
- Tokio-backed scheduler with bounded concurrency and graceful shutdown.
- MXP message dispatchers with type-specific handlers for `AgentRegister`, `Call`, `Response`, and more.
- `CallExecutor` pipeline that evaluates tools, enforces policy, and streams adapter responses.
- Memory journaling for inbound/outbound payloads and tool traces.
- Governance integrations:
  - Policy enforcement for tools, model inference, and memory writes.
  - Composite observers for tracing and MXP audit events.

### Agent Memory
- `MemoryBus` facade composing:
  - `VolatileMemory`: lock-free ring buffer for short-term context.
  - `FileJournal`: append-only log for audit and replay.
  - `VectorStoreClient`: pluggable interface with local stub and MXP vector roadmap.
- Structured `MemoryRecord` builder for channel-typed entries (`Input`, `Output`, `Tool`).
- Tagging and metadata used for policy evaluation and observability.

### Agent Policy
- `PolicyEngine` trait with async evaluation.
- `RuleBasedEngine` supporting tag, action, and metadata matchers.
- `PolicyDecision` outcomes (`Allow`, `Deny`, `Escalate`) with reasons and required approvals.
- `PolicyObserver` trait with `CompositePolicyObserver`, `TracingPolicyObserver`, and
  `MxpAuditObserver` for emitting MXP events.
- `PolicyRequest::from_memory_record` for applying governance to persistence.

### Agent Adapters
- Shared `ModelAdapter` trait with streaming inference API.
- HTTP client implemented with `hyper` + `rustls` for minimal footprint.
- Production connectors:
  - `OpenAiAdapter`
  - `OllamaAdapter` (local dev with 8B class models)
  - Gemini and Anthropic adapters (see architecture roadmap for release schedule)
- Sanitised base URL handling and structured error propagation.

### Agent Tools
- `#[tool]` macro derives metadata, capability lists, JSON (de)serialisation, and exposes helper fns (`<name>_binding`, `register_<name>`).
- Automatically bridges typed Rust structs to LLM-compatible JSON payloads.
- `ToolRegistry` can register declarative bindings or raw executors; capability validation occurs at registration.
- Sandbox module placeholder for capability-based isolation (Phase 3 roadmap).

### Example Agents
- `examples/basic-agent` demonstrates end-to-end composition with:
  - Ollama adapter using local model weights.
  - Rule-based policy that denies high-risk tools.
  - Composite observer capturing tracing + MXP audit events.
  - Memory bus with volatile cache and file journal.

### Observability Surface
- `tracing` spans from policy observers and call outcomes.
- Audit pipeline converts policy escalations and denials into MXP `Event` messages for downstream governance agents.
- `CompositeAuditEmitter` fans events to multiple sinks (tracing, MXP governance delivery).
- `GovernanceAuditEmitter` publishes events directly to remote agents over the MXP transport stack.

### Compatibility & Performance Guarantees
- Zero-allocation hot paths in call execution and scheduler loops where applicable.
- `#[must_use]` annotations and exhaustive error types across public APIs.
- `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test --all-features` gate all contributions.

### Roadmap Cross-Reference
- Phase 2 (Memory & Policy): vector store integration, sandboxing, prompt orchestration.
- Phase 3 (Telemetry & Replay): MXP audit consumers, replay tooling, policy escalation UX.
- Refer to `docs/architecture.md` for the full multi-phase plan.

