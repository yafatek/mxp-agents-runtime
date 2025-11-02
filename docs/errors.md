## Error Handling & Troubleshooting

The MXP Agents SDK exposes explicit error enums per crate. This guide documents the
surface area and recommended remediation steps.

### Agent Kernel (`agent_kernel::HandlerError`)
- `Transport`: MXP framing or message type mismatch. Verify the incoming payload matches the MXP spec.
- `Custom`: downstream components (tools, policy, memory) returned domain-specific errors. Inspect the message for context.
- Policy failures:
  - Tool invocation denied/escalated → check `agent-policy` rules and observer output.
  - Memory recording denied → confirm the action is intended or adjust the policy rule.

**Troubleshooting**
- Enable `RUST_LOG=debug` to capture `TracingPolicyObserver` output.
- Audit events (via `MxpAuditObserver`) include approver lists and reasoning for escalations.

### Agent Adapters (`agent_adapters::traits::AdapterError`)
- `Configuration`: Missing credentials, invalid base URLs, or unsupported models. Correct config values.
- `Http`: Network/TLS failures from `hyper`/`rustls`. Check connectivity and certificates.
- `Protocol`: Provider returned an unexpected schema. Capture the raw payload using the tracing logs provided in adapter modules.
- `RateLimited` / `Quota`: Back-off according to provider guidelines or switch to local adapters like Ollama.

**Troubleshooting**
- Run adapters with `TRACE=adapter=debug` to emit sanitized HTTP request metadata.
- For Ollama, confirm the daemon is available (`ollama list`).

### Agent Memory (`agent_memory::MemoryError`)
- `Journal`: File I/O failures. Ensure the journal path is writable and monitor disk usage.
- `VectorStore`: Backend errors. Retry operations or verify the MXP vector store service is reachable.
- `Build`: Invalid `MemoryRecord` construction (e.g., missing tags). Review builder usage; the compile-time docs list required fields.

**Troubleshooting**
- Use `MemoryBus::journal().tail()` in tests or diagnostics to inspect the latest records.
- Denied writes may originate from policy—check for corresponding audit events.

### Agent Policy (`agent_policy::PolicyError`)
- `Evaluation`: Async rule evaluation failed. Inspect the underlying cause (often misconfigured remote governance client).
- `RuleConflict`: Overlapping rules without deterministic precedence. Reorder or refine matchers.
- `Observer`: Downstream observer returned an error. Ensure observers remain infallible when possible.

**Troubleshooting**
- Use the `CompositePolicyObserver` to mirror decisions into both tracing and external systems during rollout.
- Validate policy definitions in unit tests by constructing `PolicyRequest` fixtures (see `agent-policy` tests).

### Agent Tools (`agent_tools::registry::ToolError`)
- `DuplicateRegistration`: Tool name already registered. Choose unique names or reuse handles.
- `Execution`: Tool async block returned an error. Bubble up rich `serde_json::Value` payloads to help debugging.
- `Capability`: Capability metadata invalid. Check IDs (must be lowercase, kebab-case) and scope formatting.

**Troubleshooting**
- Wrap tool logic in `anyhow::Context` to produce layered error reports before converting to JSON.
- Use the memory transcript to inspect tool input/output pairs.

### Observability & Audit Failures
- The `TracingAuditEmitter` should never fail; if you implement a custom emitter, keep it resilient and non-blocking.
- If audit events are missing, ensure the `MxpAuditObserver` is attached and policies can produce denials or escalations.
- `GovernanceAuditEmitter` relies on MXP transport; network errors surface via `tracing::warn!` logs. Validate socket binding and remote reachability when troubleshooting.

### Escalation Playbooks
- When `PolicyDecision::Escalate` is returned, the `required_approvals` list indicates stakeholders. Feed MXP audit events into your governance mesh so approvers receive actionable tickets.
- Use `AgentKernel::scheduler()` to inject compensating tasks once approvals are granted.

### Testing Strategy
- Unit tests cover happy-path and failure-path behaviour (see `agent-kernel/src/call.rs` test module).
- Integration tests (`tests/agent_lifecycle.rs`) validate registry + handler errors collectively.
- Recommended: add property-based tests for tool metadata validation and policy rule matching in your project.

