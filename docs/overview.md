## Agents Runtime SDK Overview

MXP Nexus's Agents Runtime SDK lets teams build, ship, and operate autonomous agents that communicate natively over MXP (`mxp://`). The SDK focuses on deterministic behaviour, low-latency execution, and strong governance so that every agent published into the MXP Nexus mesh behaves predictably and safely.

### Design Principles

- **MXP-first**: All external communication runs on the bespoke MXP stack—no QUIC fallback. Every component assumes MXP semantics (32-byte header, XXHash3 checksums, <16 MB payloads).
- **Deterministic autonomy**: Agents plan, act, and recover through well-defined state machines; no implicit background work.
- **Security & governance**: Capability-scoped tools, policy enforcement, audit logging, and identity attestation are built-in, not optional extras.
- **Performance baseline**: Sub-millisecond round trips, zero-copy message handling, and predictable scheduling even under heavy load.
- **Operator empathy**: Observability, hot reload hooks, and lifecycle controls make it easy to debug and evolve agents in production.

### Core Surface Area

| Component | Responsibility |
|-----------|----------------|
| `AgentKernel` | Owns lifecycle (initialise → active → quiesce → retire), orchestrates tasks, and exposes MXP handlers. |
| `ModelAdapter` | Async abstraction over LLMs (OpenAI, Anthropic, Gemini, Ollama, custom MXP models) with streaming, cost routing, and retries. |
| `ToolRegistry` | Discovers and wraps Rust-native tool functions annotated with `#[tool]`, enforces capability scopes, handles MXP function-call binding. |
| `MemoryBus` | Unified interface for short-term buffers, episodic journals, and long-term MXP Vector Store access. |
| `PolicyEngine` | Evaluates intents against security/governance policies (allow, deny, require-human). |
| `Telemetry` | Structured `tracing` spans, metrics exporters, replay capture and deterministic re-run utilities. |

### Agent Lifecycle

1. **Provision**: Agents register with the MXP Nexus mesh directory via MXP `AgentRegister`, publishing capabilities, attestation metadata, and governance profile.
2. **Initialise**: `AgentKernel` boots dependencies (model adapters, tools, memory providers) and validates policy bindings.
3. **Operate**: Incoming MXP messages (`Call`, `Event`, `Stream*`) enter the planner loop, which may invoke tools, schedule subtasks, or request operator approval.
4. **Checkpoint**: Significant state transitions are recorded into the episodic journal and long-term memory (see MXP Vector Store below).
5. **Governed Actions**: Before executing high-risk tasks, the agent awaits `PolicyDecision` responses (auto-approved or escalated to humans).
6. **Retire**: Agent drains inflight work, emits `AgentHeartbeat` cessation notice, and persists final state.

### LLM Integration

- `ModelAdapter` trait uses `async fn infer` plus streaming APIs; adapters ship for OpenAI, Anthropic, Gemini, Ollama, and bespoke MXP-hosted models.
- Structured prompting helpers manage system prompts, conversation history slicing, and output schema validation.
- Planner loop supports hierarchical task graphs, self-critique steps, and reversible actions when policy denies execution.

### Memory & The MXP Vector Store

- **Short-term memory**: Bounded ring buffer kept in-process for context windows; zero-copy slices handed to adapters.
- **Episodic journal**: Append-only log persisted via MXP `StreamChunk`, enabling deterministic replay and audit.
- **MXP Vector Store (long-term)**: Upcoming first-party store optimised for MXP workloads—vector embeddings, metadata, and policy tags travel over MXP without third-party dependencies. The SDK exposes traits today so agents can compile against the API and swap in the concrete store once available.
  - Encryption-at-rest and in-flight with agent-specific keys.
  - Multi-tenant isolation aligned with MXP Nexus governance domains.
  - Pluggable embedding pipelines to support both centralised and on-device embedding generation.

### Security & Governance

- Capability-based tool access with signed manifests and runtime enforcement.
- Policy engine integrates with MXP Nexus governance service; supports synchronous deny/allow as well as human approval callbacks.
- Every external effect emits a structured audit span (`tracing` + MXP `Event`) for compliance and incident forensics.
- Agents authenticate using Ed25519 identities; MXP payloads may be encrypted with AEAD for sensitive data.

### Observability & Operations

- Rich `tracing` instrumentation with correlation IDs propagated via MXP headers.
- Metrics exporters (Prometheus/OpenTelemetry) track throughput, queue depth, policy wait times, and tool latency.
- Diagnostic replay mode allows operators to reproduce MXP message streams against staging agents.
- Health interface exposes readiness/liveness, heartbeat intervals, and configuration digests for drift detection.

### Roadmap Snapshot

1. Finalise core crate layout (`agent-kernel`, `model-adapters`, `memory`, `policy`, `telemetry`).
2. Ship reference agent example (task planner + governance workflow) with integration tests.
3. Deliver initial MXP Vector Store server + SDK bindings.
4. Harden policy and audit pipeline, add proptest coverage for MXP message handling.

Refer back to this document when shaping API surface or writing module-specific docs; keep it updated as the implementation matures. For the workspace bootstrap decisions and quality gate commands, see [`phase-0.md`](./phase-0.md).

