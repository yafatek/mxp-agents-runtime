#!/bin/bash
set -e

echo "🔧 Preparing crates for publishing..."
echo ""

# Update all remaining Cargo.toml files with metadata
CRATES=(
    "agent-config:Agent configuration management for MXP runtime"
    "agent-telemetry:Telemetry and observability for MXP agents"
    "agent-tools-macros:Procedural macros for agent tool registration"
    "agent-tools:Tool registry and execution sandbox for MXP agents"
    "agent-memory:Memory bus and vector store integration for MXP agents"
    "agent-policy:Policy engine and governance hooks for MXP agents"
    "agent-prompts:Prompt management and context window handling for MXP agents"
    "agent-adapters:LLM adapters for OpenAI, Anthropic, Gemini, and Ollama"
)

for entry in "${CRATES[@]}"; do
    IFS=':' read -r crate desc <<< "$entry"
    echo "📝 Updating $crate..."
    
    # Check if Cargo.toml exists
    if [ ! -f "$crate/Cargo.toml" ]; then
        echo "⚠️  Skipping $crate (Cargo.toml not found)"
        continue
    fi
    
    # Check if publish = false exists and remove it
    if grep -q "publish = false" "$crate/Cargo.toml"; then
        sed -i '' '/publish = false/d' "$crate/Cargo.toml"
    fi
    
    # Check if description exists
    if ! grep -q "description =" "$crate/Cargo.toml"; then
        # Add metadata after license line
        sed -i '' "/license.workspace = true/a\\
repository.workspace = true\\
homepage.workspace = true\\
keywords.workspace = true\\
categories.workspace = true\\
authors.workspace = true\\
description = \"$desc\"\\
readme = \"../README.md\"
" "$crate/Cargo.toml"
    fi
done

echo ""
echo "✅ All crates prepared for publishing!"
echo ""
echo "Next steps:"
echo "1. Review changes: git diff"
echo "2. Test build: cargo build --workspace"
echo "3. Run tests: cargo test --workspace"
echo "4. Dry run: ./scripts/publish.sh (will do dry-run first)"
echo "5. Create tag: git tag -a v0.1.0 -m 'Release v0.1.0'"
echo "6. Push tag: git push origin v0.1.0"

