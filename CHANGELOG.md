# Changelog

All notable changes to the Agents Runtime SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2025-11-07

### Added
- `MxpRegistryClient` for MXP-native registry registration, heartbeats (with `needs_register` handling), and final deregistration.
- Shared registry wire types (`RegisterRequest`, `DiscoverResponse`, `HeartbeatResponse`, etc.) exposed from `agent-kernel::registry_wire` for agents to consume when talking to the MXP Nexus registry.
- Documentation updates covering registry configuration and usage in `docs/usage.md`.

### Changed
- Runtime registry control loop now converts manifests into wire payloads with metadata (version, description, tags) before registering.
- Heartbeat loop interprets registry responses and triggers re-registration when required.

### Testing
- `cargo fmt`
- `cargo clippy --all -- -D warnings`
- `cargo test --all`

## [0.2.0] - 2025-11-07

### Added
- **MXP v0.2.0 Integration**: Upgraded to MXP protocol v0.2.0 with verified performance improvements
  - 37x faster than JSON (60ns codec performance)
  - 16.6M msg/s throughput
  - Property-based tests for reliability
- **Phase 2 Complete**: Memory & Policy implementation
  - `MemoryBus` with volatile cache and file-backed journal
  - `PolicyEngine` with rule-based governance
  - Audit pipeline with MXP integration
- **LLM Adapters**: All major providers supported
  - OpenAI, Anthropic, Gemini, Ollama
  - System prompts with provider-native optimizations
  - Streaming support across all adapters
- **Tool System**: Production-ready `#[tool]` macro
  - Automatic schema generation
  - Capability-based access control
  - Multi-argument support with JSON serialization
- **Context Management**: Optional context window management
  - `ContextWindowManager` for long conversations
  - `PromptTemplate` system for reusable prompts
  - Importance scoring and message pinning

### Changed
- Bumped all crate versions from 0.1.0 to 0.2.0
- Updated MXP dependency from 0.1.103 to 0.2.0
- Improved documentation across all crates

### Testing
- 83 tests passing (22 + 18 + 12 + 8 + 5 + 13 + 5)
- All examples building successfully
- Clippy clean (only feature flag warnings)

### Documentation
- Complete API documentation for all public items
- Working examples for all major features
- Architecture and usage guides
- MXP compatibility analysis

## [0.1.0] - 2025-11-06

### Added
- Initial release of Agents Runtime SDK
- **Phase 0**: Foundation
  - Cargo workspace with 11 crates
  - Shared primitives (`AgentId`, `Capability`, `AgentManifest`)
  - Error model and documentation scaffolding
- **Phase 1**: Minimal Viable Runtime
  - `AgentKernel` lifecycle state machine
  - `KernelMessageHandler` + `CallExecutor`
  - Tool registry and execution pipeline
  - Basic examples

### Features
- MXP protocol integration (v0.1.103)
- Agent lifecycle management
- LLM connectors (OpenAI, Anthropic, Gemini, Ollama)
- Tool registration with `#[tool]` macro
- Policy hooks and governance
- Memory integration
- Distributed tracing support

[0.2.1]: https://github.com/yafatek/mxpnexus/compare/agents-sdk-v0.2.0...agents-sdk-v0.2.1
[0.2.0]: https://github.com/yafatek/mxpnexus/compare/agents-sdk-v0.1.0...agents-sdk-v0.2.0
[0.1.0]: https://github.com/yafatek/mxpnexus/releases/tag/agents-sdk-v0.1.0

