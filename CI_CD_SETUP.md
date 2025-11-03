# CI/CD Setup for Agents Runtime SDK

## ✅ Complete Setup (Same as MXP Protocol)

### Workflows Created:

1. **`.github/workflows/ci.yml`** - Tests on every push to main
2. **`.github/workflows/release.yml`** - Publishes on tag push

---

## 🔄 CI Workflow (Runs on Push to Main)

Triggers on:
- Push to `main` branch
- Pull requests to `main`

Jobs:
1. **Test** - Runs all workspace tests
2. **Format** - Checks code formatting
3. **Security** - Runs cargo audit

---

## 🚀 Release Workflow (Runs on Tag Push)

Triggers on:
- Tags matching `v*.*.*` (e.g., `v0.1.0`)

Jobs:
1. **Verify** - Runs tests, fmt check, security audit
2. **Publish** - Publishes all 11 crates to crates.io in order

---

## 📋 Usage

### Development (Test on Main):

```bash
cd agents-runtime-sdk

# Make changes
git add .
git commit -m "Add new feature"
git push origin main

# ✅ CI automatically runs: tests, fmt, security audit
```

### Release (Deploy with Tags):

```bash
# Create and push tag
git tag -a v0.1.0 -m "Release v0.1.0 - Initial agents SDK"
git push origin v0.1.0

# ✅ Release workflow automatically:
# 1. Verifies tests pass
# 2. Checks formatting
# 3. Runs security audit
# 4. Updates version in Cargo.toml
# 5. Publishes all crates in order
```

---

## 📦 Publishing Order

The release workflow publishes crates in dependency order:

1. `agent-primitives` (no dependencies)
2. `agent-config`
3. `agent-telemetry`
4. `agent-tools-macros`
5. `agent-tools`
6. `agent-memory`
7. `agent-policy`
8. `agent-prompts`
9. `agent-adapters`
10. `agent-kernel`
11. `mxp-agents` (facade - what users install)

Each crate waits 30 seconds for crates.io indexing before publishing the next.

---

## 🔑 Required Secrets

Add to GitHub repo settings → Secrets → Actions:

- `CARGO_REGISTRY_TOKEN` - Your crates.io API token
  - Get it from: https://crates.io/me
  - Click "New Token"
  - Copy and add to GitHub secrets

---

## ✅ What Gets Checked on Main:

- ✅ All workspace tests pass
- ✅ Code is formatted (`cargo fmt`)
- ✅ No security vulnerabilities (`cargo audit`)

---

## ✅ What Gets Checked on Release:

- ✅ Tag format is valid (`v*.*.*`)
- ✅ All workspace tests pass
- ✅ Code is formatted
- ✅ No security vulnerabilities
- ✅ Version is updated in Cargo.toml
- ✅ All crates publish successfully

---

## 🎯 Example Workflow

```bash
# 1. Work on main branch
cd agents-runtime-sdk
git checkout main

# 2. Make changes and test locally
cargo test --workspace
cargo fmt
cargo clippy --workspace

# 3. Commit and push to main (triggers CI)
git add .
git commit -m "Add streaming support"
git push origin main

# 4. Wait for CI to pass (GitHub Actions)

# 5. When ready to release, create tag
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# 6. Release workflow automatically publishes to crates.io
# 7. Users can now: cargo add mxp-agents
```

---

## 🔍 Monitoring

- **CI Status**: Check GitHub Actions tab
- **Crates.io**: https://crates.io/crates/mxp-agents
- **Docs**: https://docs.rs/mxp-agents

---

## 🐛 Troubleshooting

### CI Fails on Main

```bash
# Check what failed
# Fix locally
cargo test --workspace
cargo fmt
cargo audit

# Push fix
git add .
git commit -m "Fix CI"
git push origin main
```

### Release Fails

```bash
# Check GitHub Actions logs
# Common issues:
# - Tests failing
# - Formatting issues
# - Security vulnerabilities
# - Crates.io token expired

# Fix and create new tag
git tag -d v0.1.0  # Delete local tag
git push origin :refs/tags/v0.1.0  # Delete remote tag
# Fix issues
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

---

## ✨ Benefits

1. **Automated Testing** - Every push is tested
2. **Consistent Formatting** - Enforced on every commit
3. **Security Checks** - Automatic vulnerability scanning
4. **One-Command Release** - Just push a tag
5. **No Manual Steps** - Everything is automated

---

## 📝 Notes

- Same pattern as `mxp-protocol` crate
- CI runs on every push to main
- Release runs on tag push
- All checks must pass before publishing
- Version is automatically updated from tag

