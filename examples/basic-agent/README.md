# Basic Agent Example

This example demonstrates how to bootstrap a minimal MXP agent using the
`agent-kernel` runtime. It wires together the lifecycle state machine, a simple
message handler, and a logging-only registry implementation.

## Running

```bash
cargo run -p basic-agent
```

The agent will register with the mock registry, log heartbeat events every few
seconds, and echo incoming `Call` payload telemetry to the console. Press
`Ctrl+C` to trigger the shutdown path and deregistration flow.

