# Implementation Plan: System Prompts & Context Management

## Current State (What's Already Done ✓)

### Completed
- ✅ `InferenceRequest` has `system_prompt: Option<String>` field
- ✅ `with_system_prompt()` builder method
- ✅ OpenAI adapter handles system prompts (prepends as first message)
- ✅ Ollama adapter handles system prompts (prepends as first message)
- ✅ Anthropic adapter fully implemented with native `system` parameter
- ✅ Gemini adapter fully implemented with `systemInstruction`
- ✅ `PromptTemplate` system in `agent-prompts` crate
- ✅ `ContextWindowManager` in `agent-prompts` crate
- ✅ All adapters have comprehensive tests
- ✅ No linter errors

## What Needs Adjustment

### 1. Keep Current Implementation Clean
**Status**: Current implementation is good, just needs documentation

**Action**: None - keep as is

### 2. Make Context Management Optional
**Status**: Currently standalone, needs integration

**Action**: 
- Add optional context management to adapters
- Keep it hidden by default
- Expose via builder pattern for advanced users

### 3. Simplify Examples
**Status**: Example is comprehensive but could be simpler

**Action**:
- Keep `examples/prompt-management` as advanced example
- Update `examples/basic-agent` to show simple usage
- Add quick-start snippet to README

## Implementation Steps

### Step 1: Update Adapter Trait (Optional Context Support)
**File**: `agent-adapters/src/traits.rs`

Add optional context management to `ModelAdapter` trait:
```rust
// Optional: adapters can implement context management
pub trait ModelAdapter: Send + Sync {
    fn metadata(&self) -> &AdapterMetadata;
    async fn infer(&self, request: InferenceRequest) -> AdapterResult<AdapterStream>;
    
    // Optional: override for context-aware adapters
    fn context_config(&self) -> Option<&ContextWindowConfig> {
        None
    }
}
```

**Rationale**: Makes context management opt-in, not required

### Step 2: Add Context Config to Adapter Builders
**Files**: 
- `agent-adapters/src/openai.rs`
- `agent-adapters/src/anthropic.rs`
- `agent-adapters/src/gemini.rs`
- `agent-adapters/src/ollama.rs`

Add optional field and builder method:
```rust
pub struct OpenAiAdapter {
    // ... existing fields
    context_config: Option<ContextWindowConfig>,
}

impl OpenAiAdapter {
    // ... existing methods
    
    /// Configures context window management (optional).
    #[must_use]
    pub fn with_context_config(mut self, config: ContextWindowConfig) -> Self {
        self.context_config = Some(config);
        self
    }
}
```

**Rationale**: Progressive disclosure - available when needed

### Step 3: Update Documentation
**Files**:
- `README.md` - Add quick-start
- `docs/usage.md` - Update with system prompt examples
- `examples/basic-agent/README.md` - Simple usage

### Step 4: Simplify Basic Example
**File**: `examples/basic-agent/src/main.rs`

Show minimal usage:
```rust
// Simple system prompt usage
let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;

let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "What is MXP?"),
])?
.with_system_prompt("You are an expert on MXP protocol");

let mut stream = adapter.infer(request).await?;
```

### Step 5: Keep Advanced Example Separate
**File**: `examples/prompt-management/src/main.rs`

Keep as comprehensive example showing:
- Template system
- Context management
- Importance scoring
- Message pinning

## What NOT to Change

### Keep As-Is:
1. ✅ Current `InferenceRequest` API - it's clean
2. ✅ Adapter implementations - they work correctly
3. ✅ System prompt transformation logic - provider-optimized
4. ✅ Template and context manager - well-designed
5. ✅ Test coverage - comprehensive

### Don't Add:
1. ❌ Backward compatibility layers (not needed)
2. ❌ Complex abstractions (keep it simple)
3. ❌ Global state (avoid if possible)
4. ❌ Required configuration (make everything optional)

## Testing Plan

### Current Tests (Keep)
- ✅ Adapter unit tests
- ✅ System prompt transformation tests
- ✅ Template rendering tests
- ✅ Context compression tests

### Add (If Needed)
- [ ] Simple integration test showing happy path
- [ ] Multi-provider test with same request
- [ ] Documentation tests (doc comments)

## Documentation Updates Needed

### 1. README.md
Add quick-start section:
```markdown
## Quick Start

```rust
use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_adapters::traits::{InferenceRequest, MessageRole, PromptMessage};

let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;
let request = InferenceRequest::new(vec![
    PromptMessage::new(MessageRole::User, "Hello!"),
])?
.with_system_prompt("You are helpful");

let response = adapter.infer(request).await?;
```
```

### 2. docs/usage.md
Update system prompt section with:
- Simple examples
- Provider differences
- Best practices
- When to use templates

### 3. Inline Documentation
Ensure all public APIs have:
- Clear rustdoc comments
- Usage examples
- Link to design doc

## Timeline

### Immediate (Now)
- [x] Design document created
- [x] Implementation plan documented
- [ ] Get approval from maintainer (you!)

### Phase 1 (After Approval)
- [ ] Add context config to adapters (optional)
- [ ] Update documentation
- [ ] Simplify basic example

### Phase 2 (Polish)
- [ ] Add more examples
- [ ] Performance benchmarks
- [ ] Integration tests

### Phase 3 (Future)
- [ ] LLM-based summarization
- [ ] Semantic compression
- [ ] Advanced features

## Success Criteria

### Must Have
- ✅ System prompts work across all adapters
- ✅ Clean, simple API
- ✅ Minimal boilerplate
- ✅ Good documentation
- ✅ Comprehensive tests

### Nice to Have
- [ ] Context management integrated (optional)
- [ ] Multiple examples (simple + advanced)
- [ ] Performance benchmarks
- [ ] Migration guide

### Don't Need (Yet)
- ❌ LLM-based summarization
- ❌ Vector store integration
- ❌ Streaming context updates
- ❌ Multi-agent threading

## Questions for Review

1. **Is the current `InferenceRequest` API acceptable?**
   - Optional system prompt via builder pattern
   - Clean separation from messages

2. **Should context management be integrated now or later?**
   - Option A: Integrate now (optional feature)
   - Option B: Keep separate, integrate in Phase 2

3. **Are the examples at the right level?**
   - Basic example: simple usage
   - Advanced example: all features

4. **Any other design concerns?**

## Next Steps

**Awaiting your approval on:**
1. Design document
2. Implementation approach
3. What to implement first

**Then I'll:**
1. Implement approved changes
2. Update documentation
3. Add tests as needed
4. Get your review before moving to next phase

---

**Note**: This is a living document. Update as we learn and iterate.

