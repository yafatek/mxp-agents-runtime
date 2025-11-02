## Phase 0 – Workspace Scaffolding Record

This note documents the initial scaffolding work for the Agents Runtime SDK so future contributors understand how the workspace is organised and which guardrails we enforce from the start.

### Workspace Layout

- `Cargo.toml` defines a Cargo workspace that includes:
  - Internal crates: `agent-primitives`, `agent-kernel`, `agent-adapters`, `agent-tools`, `agent-memory`, `agent-policy`, `agent-telemetry`, `agent-prompts`, `agent-config`.
  - `mxp-agents`: the public facade crate that downstream users install via `cargo add mxp-agents`.
- Shared package metadata (edition, version, license) and dependency versions live in `[workspace.package]` / `[workspace.dependencies]` to keep crate manifests minimal.
- Internal crates remain `publish = false`; only the facade will be published when stable.

### Intentional Defaults

- Every crate enables `#![warn(missing_docs, clippy::pedantic)]` to force docs + lint cleanliness from the first commit.
- Placeholder modules are present but minimal—each carries module-level docs instead of nested inner attributes to satisfy Clippy.
- `agent-primitives` already exposes:
  - `AgentId` (UUID-backed identifier with parse/format helpers).
  - `CapabilityId` and `Capability` descriptors with validation-friendly builders.
  - Shared `Error`/`Result` types for consistent error reporting.

### Dependency Baseline

- Core dependencies pinned in the workspace:
  - `anyhow 1.0.100`
  - `async-trait 0.1.89`
  - `serde 1.0.228` with derive
  - `thiserror 2.0.17`
  - `tracing 0.1.41`
  - `uuid 1.18.1` with `serde` + `v4`
  - `mxp 0.1.103` for protocol bindings

### Quality Gates (must run before merging)

```bash
# format
cargo fmt

# compile everything
cargo check

# lint using pedantic profile across all crates/features
cargo clippy --all-targets --all-features

# execute unit + doc tests
cargo test
```

All four commands succeeded on the initial scaffold.

### Notable Decisions

- We ship a single installable crate (`mxp-agents`) that re-exports internal crates behind feature flags. This keeps the public API surface cohesive while letting us iterate internally.
- Capability descriptors require explicit names, versions, and non-empty scope lists so governance tooling can depend on well-formed metadata.
- Builder APIs gain `#[must_use]` attributes and documented error contracts to maintain correctness as the SDK grows.

### Next Steps

1. Flesh out `agent-kernel` with lifecycle state machines and MXP handlers.
2. Implement prompt, memory, and policy foundations using the primitives defined here.
3. Maintain this document whenever workspace structure or baseline tooling changes.

