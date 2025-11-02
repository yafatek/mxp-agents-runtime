# MXP Agent Mesh - Testing Your Entire SDK

Three separate agents that communicate via the **real MXP protocol** to test your complete SDK implementation.

## What This Tests

✅ **MXP Protocol** - Real UDP-based agent-to-agent communication  
✅ **Agent Registration** - Agents register capabilities with coordinator  
✅ **Message Routing** - Coordinator routes requests to appropriate agents  
✅ **Ollama Integration** - LLM inference via your adapter  
✅ **System Prompts** - Template-based prompts  
✅ **Streaming** - Real-time token streaming  
✅ **Context Management** - Multi-turn conversations (debugger)  

## The Agents

1. **Coordinator** (`50051`) - Routes messages, no LLM
2. **Code Reviewer** (`50052`) - Reviews Rust code, uses Ollama
3. **Debugger** (`50053`) - Debugs errors with context, uses Ollama

## Running

**Terminal 1 - Coordinator:**
```bash
cd agents-runtime-sdk
cargo run -p agent-coordinator
```

**Terminal 2 - Code Reviewer:**
```bash
cargo run -p agent-code-reviewer
```

**Terminal 3 - Debugger:**
```bash
cargo run -p agent-debugger
```

## Testing the System

Once all 3 agents are running, open a **4th terminal** and run:

```bash
cargo run -p test-client
```

The test client will prompt you:
```
🧪 MXP Agent Test Client

Select test:
  1. Code Review
  2. Debug Error

Enter choice (1 or 2):
```

### Option 1: Code Review
- Sends Rust code to CodeReviewer agent
- Agent analyzes code using Ollama + system prompts
- Streams response back via MXP
- You'll see the review in real-time!

### Option 2: Debug Error
- Sends error description to DebugBot agent
- Agent uses context management to help debug
- Streams solution back via MXP
- Multi-turn conversation support!

## Manual Testing (Advanced)

You can also send raw MXP messages using any UDP client. The payload format is:

**Code Review Request:**
```json
{
  "type": "code_review",
  "code": "<your rust code>"
}
```

**Debug Request:**
```json
{
  "type": "debug",
  "error": "<error description>"
}
```

Send to coordinator at `127.0.0.1:50051` as MXP `Call` message.

## What You'll See

1. Coordinator starts on port 50051
2. Code Reviewer registers via MXP
3. Debugger registers via MXP  
4. All agents communicate over raw UDP using MXP protocol
5. Messages are encoded/decoded using MXP binary format

## MXP Protocol Flow

```
1. Agent creates Message: Message::new(MessageType::AgentRegister, payload_bytes)
2. Agent encodes: let bytes = message.encode()
3. Agent sends via UDP: handle.send(&bytes, coordinator_addr)
4. Coordinator receives: handle.receive(&mut buffer)
5. Coordinator decodes: Message::decode(buffer)
6. Process and respond following same flow
```

## Architecture

```
┌──────────────────┐
│   Coordinator    │  Port 50051
│  (Routes msgs)   │  MXP Transport
└────────┬─────────┘
         │ MXP over UDP
    ┌────┴────┐
    │         │
┌───▼──┐  ┌──▼────┐
│Review│  │Debug  │
│50052 │  │50053  │
└──────┘  └───────┘
 Ollama    Ollama+Context
```

## Technical Details

- **Protocol**: MXP 1.0 (32-byte headers, XXHash3 checksums)
- **Transport**: UDP sockets (blocking, wrapped in tokio::spawn_blocking)
- **Message Format**: Binary encoding via `Message::encode()`
- **Payload**: JSON serialized with serde_json
- **LLM**: Ollama gemma3 model

This is a **REAL distributed agent mesh** using your SDK!
