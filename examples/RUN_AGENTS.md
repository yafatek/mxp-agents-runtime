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
1. Test Client sends Call to Coordinator
2. Coordinator adds request_id and forwards Call to Agent
3. Agent processes request via LLM
4. Agent sends Response with request_id back to Coordinator
5. Coordinator looks up original sender via request_id
6. Coordinator forwards Response to Test Client
7. Test Client receives and displays result
```

**Implementation Details:**
- Each request gets a unique `request_id` (UUID)
- Coordinator tracks pending requests: `HashMap<request_id, original_sender_addr>`
- Agents echo `request_id` in their responses
- Coordinator uses `request_id` to route responses back to correct client
- After routing, `request_id` is removed from pending map

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
