#!/bin/bash
set -e

echo "🔧 Adding version numbers to all internal dependencies..."

# Fix all crates
for toml in agent-*/Cargo.toml mxp-agents/Cargo.toml; do
    if [ -f "$toml" ]; then
        echo "📝 Fixing $toml..."
        
        # Add version = "0.1" to all agent-* dependencies
        sed -i '' 's/agent-primitives = { path/agent-primitives = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-config = { path/agent-config = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-kernel = { path/agent-kernel = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-adapters = { path/agent-adapters = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-tools = { path/agent-tools = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-tools-macros = { path/agent-tools-macros = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-memory = { path/agent-memory = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-policy = { path/agent-policy = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-telemetry = { path/agent-telemetry = { version = "0.1", path/g' "$toml"
        sed -i '' 's/agent-prompts = { path/agent-prompts = { version = "0.1", path/g' "$toml"
        
        # Remove duplicate version if already exists
        sed -i '' 's/version = "0.1", version = "0.1"/version = "0.1"/g' "$toml"
    fi
done

echo "✅ All Cargo.toml files fixed!"
echo ""
echo "Verify with: git diff"

