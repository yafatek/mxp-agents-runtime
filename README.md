## Agents Runtime SDK

Rust SDK for building autonomous AI agents that operate over the MXP (`mxp://`) protocol. The focus is low-latency planning, secure execution, and predictable behaviour—this SDK is what agents use before they are deployed onto the Relay mesh.

Install once via the bundled facade crate:

```sh
cargo add mxp-agents
```

### Why it exists

- Provide a unified runtime that wraps LLMs, tools, memory, and governance without depending on QUIC or third-party transports.
- Ensure every agent built for Relay speaks MXP natively and adheres to platform security, observability, and performance rules.
- Offer a developer-friendly path to compose agents locally, then promote them into the Relay platform when ready.

### Scope

- **In scope**: agent lifecycle management, LLM connectors, tool registration, policy hooks, MXP message handling, memory integration (including the upcoming MXP Vector Store).
- **Out of scope**: Relay deployment tooling, mesh scheduling, or any "deep agents" research-oriented SDK—handled by separate projects.

### Supported LLM stacks

- OpenAI, Anthropic, Gemini, Ollama, and future MXP-hosted models via a shared `ModelAdapter` trait.

### MXP integration

- MXP crate (e.g. `mxp = "0.1.103"`) provides the transport primitives. We no longer rely on QUIC; all messaging assumes the custom MXP stack and UDP carrier.
- Helpers for `AgentRegister`, `AgentHeartbeat`, `Call`, `Response`, `Event`, and `Stream*` payloads are part of the SDK surface.

### Key concepts

- Tools are pure Rust functions annotated with `#[tool]`; the SDK converts them into schemas consumable by LLMs and enforces capability scopes at runtime.
- Agents can share external state (memory bus, MXP Vector Store) or remain fully isolated.
- Governance and policy enforcement are first-class: hooks exist for allow/deny decisions and human-in-the-loop steps.

### Getting started

1. Model your agent using the runtime primitives (`AgentKernel`, adapters, tool registry).
2. Wire MXP endpoints for discovery and message handling.
3. Configure memory providers (in-memory ring buffer today, pluggable MXP Vector Store soon).
4. Instrument with `tracing` spans and policy hooks.

See `docs/overview.md` for architectural detail and roadmap. Keep this README as the quick orientation for contributors.