## Agents Runtime SDK Architecture Plan

This document captures the first-pass architecture and build plan for the Agents Runtime SDK. It defines the crate layout, runtime components, API contracts, and the backlog required to deliver a production-ready SDK for MXP-native autonomous agents.

Refer back to `docs/overview.md` for the narrative positioning; this document is the execution blueprint.

Additional references:
- `docs/features.md` — current feature set per crate and facade flag.
- `docs/usage.md` — step-by-step instructions for bootstrapping an agent.
- `docs/errors.md` — error catalogue and troubleshooting guidance.

### 1. Crate & Module Layout

| Crate | Purpose | Key Modules |
|-------|---------|-------------|
| `agent-kernel` | Lifecycle state machine and execution loop for agents. | `bootstrap`, `scheduler`, `planner`, `mxp_handlers`, `shutdown` |
| `agent-adapters` | Integrations for LLMs, embeddings, and other AI services. | `openai`, `anthropic`, `gemini`, `ollama`, `mxp_model` |
| `agent-tools` | Tool discovery, schema generation, capability enforcement. | `registry`, `macros`, `runtime`, `sandbox` |
| `agent-memory` | Short-term buffers, episodic journal, MXP Vector Store binding. | `volatile`, `journal`, `vector_store_api`, `embeddings` |
| `agent-policy` | Governance integration and policy evaluation. | `engine`, `contracts`, `decision`, `integrations` |
| `agent-telemetry` | Observability, tracing, metrics, replay capture. | `tracing`, `metrics`, `replay`, `health` |
| `agent-prompts` | Prompt templates, schema validation, alignment policies. | `templates`, `validators`, `context_ops`, `guardrails` |
| `agent-config` | Typed configuration, secrets handling, environment loading. | `loader`, `schema`, `sops` |
| `agent-cli` (later) | Developer tool to scaffold projects, run local agents, inspect state. | `commands`, `templates`, `devserver` |

All crates live under `agents-runtime-sdk/` using a Cargo workspace. Shared types (errors, base traits) live in `agent-primitives` crate to avoid circular dependencies.

### 2. Core Runtime Components

**2.1 AgentKernel**
- State machine: `Init → Ready → Active → Suspended → Retiring → Terminated`.
- Responsible for dependency wiring (model adapters, tools, memory, policy, telemetry), MXP endpoint binding, task scheduling, and lifecycle transitions.
- Exposes async hooks: `on_bootstrap`, `on_message`, `on_tick`, `on_shutdown`.

**2.2 Scheduler & Planner**
- Scheduler orchestrates concurrent task execution with priorities (e.g., message handling vs. background reasoning).
- Planner builds/maintains task graphs for long-running goals. Supports self-critique loops and partial rollback.
- Use `tokio` runtime primitives; enforce bounded concurrency per agent to preserve determinism.

**2.3 MXP Interface**
- Abstractions for all MXP message types; typed handlers for `AgentRegister`, `AgentHeartbeat`, `Call`, `Response`, `Event`, `StreamOpen/Chunk/Close`, `Error`.
- Provide convenience to map MXP payloads to/from strongly typed structs using `bytemuck`-compatible zero-copy slices.
- Integrate checksum validation (XXHash3) and header validation at the edge of the runtime.

**2.4 Tool Runtime**
- `#[tool]` proc macro generates schema metadata and capability tags.
- Registry stores metadata, handles versioning, and enforces capability-based access during execution.
- Tool invocation pipeline supports sandboxing (WASM or separate process) with timeouts, CPU/memory quotas, and secret scoping.

**2.5 Model Adapters**
- `ModelAdapter` trait: `async fn infer(&self, request: InferenceRequest) -> Result<InferenceStream>`.
- Support streaming tokens, cost tracking, retries, and deterministic prompt assembly.
- Collaborates with the prompt subsystem for template expansion and alignment checks before sending requests.

**2.6 Prompt Orchestration**
- `PromptEngine` resolves prompt templates, injects agent/tenant metadata, and maintains deterministic prompt history slices.
- Support hierarchical templates (system → role → task), variable interpolation, and compilable prompt graphs for multi-step planners.
- Enforce guardrails (PII filters, policy tags, instruction whitelists) before handing prompts to adapters.
- Manage context window budgeting, summary compression, and inclusion/exclusion rules per task type.

**2.7 Memory Subsystem**
- `MemoryBus` orchestrates multiple stores:
  - `VolatileMemory`: in-process ring buffer for recent exchanges.
  - `EpisodicJournal`: append-only log persisted via MXP `Stream*` with replay semantics.
  - `VectorStoreClient`: trait for the forthcoming MXP Vector Store (initial stub with local fallback).
- Embedding service integration for generating vectors (local vs hosted models).
- Policy gating determines what data can leave the agent boundary.

**2.8 Policy & Governance**
- `PolicyEngine` loads policies (Rego/DSL) and evaluates `PolicyRequest` objects describing intent, data sensitivity, and tool usage.
- Synchronous flow: returns `PolicyDecision::Allow`, `::Deny`, or `::Escalate` (human approval required).
- Integrations with Relay governance service via MXP `Call` to dedicated governance agents.
- Observers: `TracingPolicyObserver`, `MxpAuditObserver`, and `CompositePolicyObserver` enable multi-sink decision fan-out.
- Audit events emitted for denials/escalations via MXP `Event` payloads for downstream governance agents.
- `CompositeAuditEmitter` and `GovernanceAuditEmitter` propagate audit events to tracing sinks and remote governance agents over MXP transport.

**2.9 Telemetry & Replay**
- `tracing` instrumentation with standard span structure: agent_id, task_id, mxp_message_id.
- Metrics exporters supporting Prometheus + OpenTelemetry.
- MXP audit emission pipeline for policy denials/escalations.
- Replay recorder capturing MXP exchanges and internal state snapshots for deterministic debugging.
- Health reporting: readiness/liveness endpoints, heartbeat metrics, config digests.

### 3. Cross-Cutting Concerns

- **Error Handling**: Common `Error` enum per crate, re-exported in `agent-primitives`, using `thiserror`. Provide fallible constructors and `#[must_use]` results.
- **Configuration**: Typed config structs with serde, optional SOPS integration. Support layered config (defaults → file → env → runtime overrides).
- **Secrets Management**: Borrowed handles to secrets; never clone into logs. Provide integration shims for Vault/AWS Secrets Manager in later phases.
- **Testing Strategy**: Unit tests for modules, integration tests simulating MXP flows (feature-gated to not require live mesh), proptest for message encoding/decoding, Criterion benchmarks for hot paths.
- **Docs & Examples**: Each public API must have rustdoc with examples. Provide `examples/` crate demonstrating a simple planner agent with governance workflow.

### 4. Build Phases & Deliverables

| Phase | Goals | Deliverables |
|-------|-------|--------------|
| Phase 0: Foundations | Set up workspace, shared primitives, CI scaffolding. | Cargo workspace, lint/format config, CI pipelines, initial docs. |
| Phase 1: Minimal Viable Runtime | Implement `AgentKernel`, basic MXP handlers, single model adapter, simple tool registry. | Working demo agent responding to MXP `Call` with static tool + LLM call, tests for lifecycle. |
| Phase 2: Memory & Policy | Add MemoryBus (volatile + journal stub), PolicyEngine with mock backend, audit logging. | Governance-aware agent example, documentation, proptest coverage for memory. |
| Phase 3: Tool Sandbox & Advanced Adapters | WASM/process sandboxing, additional model adapters, embedding pipeline. | Benchmarks for tool execution, streaming inference support, safety checks. |
| Phase 4: MXP Vector Store Integration | Ship first-party vector store client, upgrade memory API, gather performance metrics. | End-to-end agent that stores/retrieves long-term memory via MXP Vector Store. |
| Phase 5: Telemetry & Developer Tooling | Replay/debug tooling, metrics exporters, CLI scaffolding. | `agent-cli`, replay docs, full observability instrumentation. |

### 5. Backlog (Initial Tickets)

1. Create `Cargo.toml` workspace and stub crates listed above.
2. Define `agent-primitives` types (AgentId, Capability, PolicyDecision, Telemetry metadata).
3. Implement `AgentKernel` skeleton with lifecycle states and MXP binding traits.
4. Provide `ModelAdapter` trait + OpenAI adapter implementation (API keys via config).
5. Build `ToolRegistry` with `#[tool]` macro and capability metadata extraction.
6. Add `MemoryBus` with volatile memory and journaling stub.
7. Integrate policy check pipeline with mock allow/deny for testing.
8. Instrument runtime with base `tracing` spans and metrics counters.
9. Write first example agent and integration test harness using MXP crate (feature-flagged offline mode).
10. Draft API documentation and maintain `docs/overview.md` + this architecture document as crates evolve.

### 6. Risks & Mitigations

- **Vector store availability**: Implement local fallback so agents can compile/run before MXP store ships.
- **Sandbox overhead**: Benchmark early; allow configurable execution modes (in-process vs sandbox) to meet latency SLAs.
- **Policy latency**: Provide async escalation path with timeouts and fallback behaviours.
- **MXP protocol drift**: Generate bindings from `mxp-protocol` crate, set up contract tests to detect spec changes.
- **Model API variance**: Standardise on adapter trait with strong request typing; add conformance tests per adapter.

### 7. Next Steps

1. Finalise crate scaffolding and module boundaries based on this plan.
2. Align with MXP protocol maintainers to ensure message definitions stay in sync.
3. Start implementation with Phase 0 tasks, updating docs as architecture evolves.

---

This plan should remain a living document—update it alongside major architectural decisions or roadmap changes.

