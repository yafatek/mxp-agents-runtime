# System Prompts & Context Management - Design Document

## Overview

This document defines the design for system prompt handling and context window management in the MXP Agents Runtime SDK.

## Design Principles

1. **Minimal Boilerplate**: Users should get started with minimal configuration
2. **SDK-Managed Complexity**: Context window management happens transparently
3. **Progressive Disclosure**: Advanced features available when needed, hidden by default
4. **Respect Existing Architecture**: Build on current `InferenceRequest` and adapter patterns
5. **Provider-Optimized**: Each adapter uses native system prompt format

## Architecture Decisions

### 1. System Prompt Handling

**Decision**: Optional system prompt with builder pattern (already implemented)

```rust
// Simple case - no system prompt
let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "Hello"),
])?;

// With system prompt
let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "Hello"),
])?
.with_system_prompt("You are a helpful assistant");
```

**Rationale**:
- Optional when not needed (minimal overhead)
- Clear separation from conversation messages
- Builder pattern consistent with existing SDK style
- Each adapter transforms to provider-specific format

**Adapter Transformations**:
- **OpenAI/Ollama**: Prepend as `{"role": "system", "content": "..."}`
- **Anthropic**: Extract to top-level `"system": "..."` parameter
- **Gemini**: Transform to `"systemInstruction": {"parts": [...]}`

### 2. Context Window Management

**Decision**: Transparent SDK-managed with optional override

**Default Behavior**:
- SDK automatically manages context window per adapter
- Uses model-specific defaults (GPT-4: 8K, Claude: 100K, etc.)
- Transparent compression when needed
- No user configuration required

**Implementation**:
```rust
// Context management happens automatically in adapters
// User doesn't see ContextWindowManager unless they want to

// Advanced override (optional)
let adapter = OllamaAdapter::new(config)?
    .with_context_config(ContextWindowConfig {
        max_tokens: 16000,
        recent_window_size: 10,
        ..Default::default()
    });
```

**Where It Lives**:
- `ContextWindowManager` in `agent-prompts` crate
- Integrated into adapters (not exposed by default)
- Optional configuration via adapter builder methods

### 3. Template System

**Decision**: Internal SDK helper, optionally user-facing

**Primary Use**:
- SDK uses templates for internal system prompts
- Users pass plain strings by default
- Templates available in `agent-prompts` for advanced users

```rust
// Simple: users just pass strings
let request = InferenceRequest::new(messages)?
    .with_system_prompt("You are helpful");

// Advanced: templates available if needed
use agent_prompts::PromptTemplate;
let template = PromptTemplate::builder("You are {{role}}")
    .with_variable("role", "helpful")
    .build()?;
let request = InferenceRequest::new(messages)?
    .with_system_prompt(template.render()?);
```

### 4. Conversation State Management

**Decision**: Hybrid approach - both stateless and stateful supported

**Stateless (Current)**:
```rust
let adapter = OllamaAdapter::new(config)?;
let request = InferenceRequest::new(messages)?;
let response = adapter.infer(request).await?;
```

**Stateful (Future Enhancement)**:
```rust
// For future consideration - not in initial implementation
let mut conversation = Conversation::new(adapter);
conversation.send("Hello").await?;
conversation.send("How are you?").await?;
```

## Implementation Plan

### Phase 1: Core Infrastructure (Already Done ✓)
- [x] Add `system_prompt` field to `InferenceRequest`
- [x] Update OpenAI adapter to handle system prompts
- [x] Update Ollama adapter to handle system prompts
- [x] Implement Anthropic adapter with native `system` parameter
- [x] Implement Gemini adapter with `systemInstruction`

### Phase 2: Context Management (Current)
- [ ] Integrate `ContextWindowManager` into adapters (optional)
- [ ] Add model-specific token limits as defaults
- [ ] Make context config optional override
- [ ] Document context management behavior

### Phase 3: Polish & Documentation
- [ ] Update examples to show best practices
- [ ] Document system prompt patterns
- [ ] Add migration guide (if needed)
- [ ] Performance benchmarks

## API Examples

### Minimal "Hello World"
```rust
use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_adapters::traits::{InferenceRequest, MessageRole, ModelAdapter, PromptMessage};

let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;
let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "Hello"),
])?;
let mut stream = adapter.infer(request).await?;
```

### With System Prompt
```rust
let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "What is MXP?"),
])?
.with_system_prompt("You are an expert on MXP protocol")
.with_temperature(0.7);
```

### With Context Management (Advanced)
```rust
let adapter = OllamaAdapter::new(config)?
    .with_context_config(ContextWindowConfig {
        max_tokens: 4096,
        ..Default::default()
    });
```

### Multi-Provider Example
```rust
// Same code works across all providers
let openai = OpenAiAdapter::new(OpenAiConfig::from_env("gpt-4"))?;
let anthropic = AnthropicAdapter::new(AnthropicConfig::from_env("claude-3-5-sonnet-20241022"))?;
let gemini = GeminiAdapter::new(GeminiConfig::from_env("gemini-1.5-pro"))?;

let request = InferenceRequest::new(messages)?
    .with_system_prompt("You are helpful");

// All adapters handle system prompt appropriately
let response1 = openai.infer(request.clone()).await?;
let response2 = anthropic.infer(request.clone()).await?;
let response3 = gemini.infer(request).await?;
```

## Token Budget Management

### Default Strategy
1. **Recent Window**: Keep last 10 messages always
2. **Importance Scoring**: Tool calls and decisions scored higher
3. **Compression**: Summarize older messages when budget exceeded
4. **Pinning**: System prompts and critical context never removed

### Configuration (Optional)
```rust
ContextWindowConfig {
    max_tokens: 8192,              // Model-specific default
    recent_window_size: 10,        // Always keep recent N
    min_importance_threshold: 30,  // Remove low-importance first
    enable_summarization: true,    // Compress older messages
}
```

## Testing Strategy

### Unit Tests
- System prompt extraction and transformation per adapter
- Context window compression logic
- Template rendering and variable substitution

### Integration Tests
- End-to-end with each adapter
- Multi-turn conversations with context management
- Token budget enforcement

### Performance Tests
- Token estimation accuracy
- Compression overhead
- Memory usage with large contexts

## Future Enhancements

### Phase 4: Advanced Features (Post-MVP)
- [ ] LLM-based summarization (vs simple compression)
- [ ] Semantic compression using vector store
- [ ] Prompt caching (Anthropic supports this)
- [ ] Streaming context updates
- [ ] Multi-agent conversation threading

### Phase 5: Optimization
- [ ] Integrate `tiktoken` for accurate token counting
- [ ] Per-model token limits from API
- [ ] Adaptive compression strategies
- [ ] Context window auto-detection

## Migration Path

Since we're building from scratch (no existing users):
- No backward compatibility needed
- Clean API from day one
- Can evolve freely based on feedback

## References

- OpenAI Chat Completions: https://platform.openai.com/docs/api-reference/chat
- Anthropic Messages API: https://docs.anthropic.com/claude/reference/messages_post
- Gemini API: https://ai.google.dev/api/generate-content
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md

