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

### Quick Start

```rust
use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_adapters::traits::{InferenceRequest, MessageRole, ModelAdapter, PromptMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an adapter (works with OpenAI, Anthropic, Gemini, or Ollama)
    let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;

    // Build a request with system prompt
    let request = InferenceRequest::new(vec![
        PromptMessage::new(MessageRole::User, "What is MXP?"),
    ])?
    .with_system_prompt("You are an expert on MXP protocol")
    .with_temperature(0.7);

    // Get streaming response
    let mut stream = adapter.infer(request).await?;
    
    // Process chunks
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        print!("{}", chunk.delta);
    }
    
    Ok(())
}
```

### System Prompts

All adapters support system prompts with provider-native optimizations:

```rust
// OpenAI/Ollama: Prepends as first message
let openai = OpenAiAdapter::new(OpenAiConfig::from_env("gpt-4"))?;

// Anthropic: Uses dedicated 'system' parameter
let anthropic = AnthropicAdapter::new(AnthropicConfig::from_env("claude-3-5-sonnet-20241022"))?;

// Gemini: Uses 'systemInstruction' field
let gemini = GeminiAdapter::new(GeminiConfig::from_env("gemini-1.5-pro"))?;

// Same API works across all providers
let request = InferenceRequest::new(messages)?
    .with_system_prompt("You are a helpful assistant");
```

### Context Window Management (Optional)

For long conversations, enable automatic context management:

```rust
use agent_prompts::ContextWindowConfig;

let adapter = OllamaAdapter::new(config)?
    .with_context_config(ContextWindowConfig {
        max_tokens: 4096,
        recent_window_size: 10,
        ..Default::default()
    });

// SDK automatically manages conversation history within token budget
```

### Getting started

1. Model your agent using the runtime primitives (`AgentKernel`, adapters, tool registry).
2. Wire MXP endpoints for discovery and message handling.
3. Configure memory providers (in-memory ring buffer today, pluggable MXP Vector Store soon).
4. Instrument with `tracing` spans and policy hooks.

See `docs/overview.md` for architectural detail and `docs/usage.md` for comprehensive examples.

### Documentation Map

- `docs/architecture.md` — crate layout, component contracts, roadmap.
- `docs/features.md` — current feature set and facade feature flags.
- `docs/usage.md` — end-to-end setup guide for building an agent, including tooling examples.
- `docs/errors.md` — error surfaces and troubleshooting tips.


### Future
// Move Memories to external Github projects, like embeddings, vectors ...etc each as a repo, then treat them as external deps where the run time can pull what is required
// by default the agents are note require a memory to run tho, then can  be stateless. for now we will keep them in this project for the sake of simplicity. 
