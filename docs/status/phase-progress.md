# Phase Progress Tracker

## Current Phase — Phase 2: Memory & Policy

- **Status:** Core objectives implemented and exercised in `examples/basic-agent`.
- **Highlights:**
  - `MemoryBus` composes volatile cache plus file-backed journal with policy-gated writes.
  - `PolicyEngine` (`RuleBasedEngine`) now guards tool execution, model inference, and memory persistence.
  - Audit fan-out (`CompositeAuditEmitter`, `GovernanceAuditEmitter`, `MxpAuditObserver`) routes denials/escalations over MXP.
  - `#[tool]` macro + builder path delivers single-call registration (`KernelMessageHandler::builder().with_tools([my_tool])`).
  - Ollama adapter ships with shared `hyper`/`rustls` HTTP stack for local LLM development.

## Phase 0 Recap — Foundations

- Cargo workspace with facade crate `mxp-agents` and internal component crates.
- Shared primitives (`AgentId`, `Capability`, `AgentManifest`, error model) stabilised in `agent-primitives`.
- Documentation scaffolding: `docs/overview.md`, `docs/architecture.md`, `docs/features.md`, `docs/usage.md`, `docs/errors.md`, README map.

## Phase 1 Recap — Minimal Viable Runtime

- `AgentKernel` lifecycle state machine (`Init → … → Terminated`) with scheduler and MXP call handling pipeline.
- `KernelMessageHandler` + `CallExecutor` integrate model adapters, tool registry, policy, and memory recording.
- Tooling surface: `ToolRegistry`, async execution, capability metadata; `#[tool]` macro auto-generates bindings and inventory registration.
- Example agent (`examples/basic-agent`) exercises lifecycle, Ollama inference, policy hooks, audit emission.

## Phase 2 Deliverables Achieved

- **Memory:**
  - `VolatileMemory` lock-free ring buffer and `FileJournal` persistence linked via `MemoryBus`.
  - Tool outputs, model responses, and inbound MXP payloads persisted with policy enforcement.
- **Policy & Governance:**
  - `PolicyRequest::from_memory_record`, `PolicyObserver` abstractions, tracing + MXP audit observers.
  - Composite audit emitter with MXP transport integration (`GovernanceAuditEmitter`).
- **Tool Runtime:**
  - Multi-argument `#[tool]` support with automatic JSON (de)serialization and capability tagging.
  - Inventory-backed discovery feeding `KernelMessageHandlerBuilder::with_tools`.
- **Adapters:**
  - Ollama adapter uses shared HTTP transport and streaming response handling; consistent error propagation across adapters.
- **Docs & Examples:**
  - Usage guide updated for new builder API, audit pipeline, and memory/policy setup.

## Outstanding / Next-Sprint Actions

- Harden prompt subsystem (Phase 2 backlog): integrate new `agent-prompts` templates with kernel pipeline and add guardrail enforcement.
- Expand policy engine tests (property-based) for memory requests and escalation paths.
- Wire anthropic/openai adapters to shared HTTP transport refactor; address remaining clippy warnings (e.g., irrefutable `if let`).
- Begin sandbox design spike (Phase 3 dependency): evaluate WASM isolation options and budget limits.
- Prepare MXP governance integration test harness (simulate remote audit sink acceptance).

## Helpful References

- Architecture plan: `docs/architecture.md`
- Feature breakdown: `docs/features.md`
- Usage walkthrough: `docs/usage.md`
- Error catalogue: `docs/errors.md`
- Example agent entry point: `examples/basic-agent/src/main.rs`


