# Publishing Agents Runtime SDK to Crates.io

## Overview

The agents-runtime-sdk is a workspace with multiple crates. We'll publish them in dependency order.

## Publishing Order

1. **agent-primitives** (no dependencies)
2. **agent-config** (no dependencies)
3. **agent-telemetry** (depends on primitives)
4. **agent-tools-macros** (proc macro, no dependencies)
5. **agent-tools** (depends on primitives, macros)
6. **agent-memory** (depends on primitives)
7. **agent-policy** (depends on primitives)
8. **agent-prompts** (depends on primitives)
9. **agent-adapters** (depends on primitives, tools, memory, policy)
10. **agent-kernel** (depends on primitives, adapters, tools, memory, policy)
11. **mxp-agents** (facade crate, depends on all)

## Pre-Publishing Checklist

### 1. Update Cargo.toml for Each Crate

Each crate needs:
- ✅ `description` - Short description
- ✅ `repository` - GitHub URL
- ✅ `homepage` - Project website
- ✅ `documentation` - docs.rs URL
- ✅ `keywords` - Max 5 keywords
- ✅ `categories` - Relevant crates.io categories
- ✅ `readme` - Path to README
- ✅ Remove `publish = false` (currently set on many crates)

### 2. Version Strategy

**Current**: `0.1.0` (workspace-level)

**For First Release**:
- All crates: `0.1.0`
- After testing: `0.2.0` (beta)
- Production-ready: `1.0.0`

### 3. Git Tags

We test on `main` and deploy on tags:

```bash
# Create a tag for release
git tag -a v0.1.0 -m "Release v0.1.0 - Initial agents runtime SDK"
git push origin v0.1.0
```

## Publishing Commands

### Step 1: Login to Crates.io

```bash
cargo login
# Enter your API token from https://crates.io/me
```

### Step 2: Dry Run (Test Publishing)

```bash
# Test each crate in order
cd agent-primitives && cargo publish --dry-run
cd ../agent-config && cargo publish --dry-run
cd ../agent-telemetry && cargo publish --dry-run
cd ../agent-tools-macros && cargo publish --dry-run
cd ../agent-tools && cargo publish --dry-run
cd ../agent-memory && cargo publish --dry-run
cd ../agent-policy && cargo publish --dry-run
cd ../agent-prompts && cargo publish --dry-run
cd ../agent-adapters && cargo publish --dry-run
cd ../agent-kernel && cargo publish --dry-run
cd ../mxp-agents && cargo publish --dry-run
```

### Step 3: Actual Publishing

```bash
# Publish in dependency order
cd agent-primitives && cargo publish && sleep 30
cd ../agent-config && cargo publish && sleep 30
cd ../agent-telemetry && cargo publish && sleep 30
cd ../agent-tools-macros && cargo publish && sleep 30
cd ../agent-tools && cargo publish && sleep 30
cd ../agent-memory && cargo publish && sleep 30
cd ../agent-policy && cargo publish && sleep 30
cd ../agent-prompts && cargo publish && sleep 30
cd ../agent-adapters && cargo publish && sleep 30
cd ../agent-kernel && cargo publish && sleep 30
cd ../mxp-agents && cargo publish
```

**Note**: The `sleep 30` is important! Crates.io needs time to index each crate before dependent crates can be published.

## Automated Publishing Script

Create `.github/workflows/publish.yml`:

```yaml
name: Publish to Crates.io

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true
      
      - name: Login to Crates.io
        run: cargo login ${{ secrets.CARGO_REGISTRY_TOKEN }}
      
      - name: Publish agent-primitives
        run: cd agents-runtime-sdk/agent-primitives && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-config
        run: cd agents-runtime-sdk/agent-config && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-telemetry
        run: cd agents-runtime-sdk/agent-telemetry && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-tools-macros
        run: cd agents-runtime-sdk/agent-tools-macros && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-tools
        run: cd agents-runtime-sdk/agent-tools && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-memory
        run: cd agents-runtime-sdk/agent-memory && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-policy
        run: cd agents-runtime-sdk/agent-policy && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-prompts
        run: cd agents-runtime-sdk/agent-prompts && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-adapters
        run: cd agents-runtime-sdk/agent-adapters && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish agent-kernel
        run: cd agents-runtime-sdk/agent-kernel && cargo publish
        
      - name: Wait for crates.io indexing
        run: sleep 30
      
      - name: Publish mxp-agents
        run: cd agents-runtime-sdk/mxp-agents && cargo publish
```

## Post-Publishing

### 1. Verify on Crates.io

Check each crate:
- https://crates.io/crates/mxp-agents
- https://crates.io/crates/agent-kernel
- etc.

### 2. Test Installation

```bash
cargo new test-mxp-agents
cd test-mxp-agents
cargo add mxp-agents
cargo build
```

### 3. Update Documentation

- Update README.md with crates.io badges
- Update website with installation instructions
- Announce on Discord, Twitter, LinkedIn

## Troubleshooting

### Error: "crate not found"

**Cause**: Dependent crate not yet indexed by crates.io

**Solution**: Wait 30-60 seconds and try again

### Error: "already published"

**Cause**: Version already exists on crates.io

**Solution**: Bump version number in workspace Cargo.toml

### Error: "missing field `description`"

**Cause**: Cargo.toml missing required metadata

**Solution**: Add description, repository, etc. (see checklist above)

## Version Bumping

### Patch Release (0.1.0 → 0.1.1)

```bash
# Update workspace version
sed -i '' 's/version = "0.1.0"/version = "0.1.1"/' Cargo.toml

# Commit and tag
git add Cargo.toml
git commit -m "Bump version to 0.1.1"
git tag -a v0.1.1 -m "Release v0.1.1"
git push origin main v0.1.1
```

### Minor Release (0.1.0 → 0.2.0)

```bash
# Update workspace version
sed -i '' 's/version = "0.1.0"/version = "0.2.0"/' Cargo.toml

# Commit and tag
git add Cargo.toml
git commit -m "Bump version to 0.2.0"
git tag -a v0.2.0 -m "Release v0.2.0 - New features"
git push origin main v0.2.0
```

### Major Release (0.2.0 → 1.0.0)

```bash
# Update workspace version
sed -i '' 's/version = "0.2.0"/version = "1.0.0"/' Cargo.toml

# Commit and tag
git add Cargo.toml
git commit -m "Release v1.0.0 - Production ready"
git tag -a v1.0.0 -m "Release v1.0.0 - Production ready"
git push origin main v1.0.0
```

## Workflow Summary

1. **Development**: Work on `main` branch
2. **Testing**: Run tests, examples, benchmarks
3. **Version Bump**: Update version in workspace Cargo.toml
4. **Tag**: Create git tag `v0.1.0`
5. **Push Tag**: `git push origin v0.1.0`
6. **GitHub Actions**: Automatically publishes to crates.io
7. **Verify**: Check crates.io and test installation

## Quick Reference

```bash
# Prepare for release
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check

# Create release tag
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# Manual publish (if not using GitHub Actions)
./scripts/publish.sh

# Verify
cargo search mxp-agents
```

## Contact

- **Issues**: GitHub Issues
- **Questions**: Discord #sdk-support
- **Security**: security@relaymxp.xyz

