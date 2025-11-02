//! Coordinator - Routes requests to agents via MXP

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use mxp::{Message, MessageType, Transport, TransportConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

const COORDINATOR_PORT: u16 = 50051;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisteredAgent {
    agent_id: String,
    name: String,
    capabilities: Vec<String>,
    endpoint: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    info!("🎯 Coordinator starting...");
    info!("Listening on port {}", COORDINATOR_PORT);

    // Create MXP transport
    let config = TransportConfig::default();
    let transport = Transport::new(config);
    let addr: SocketAddr = format!("127.0.0.1:{}", COORDINATOR_PORT).parse()?;
    let handle = transport.bind(addr).map_err(|e| anyhow::anyhow!("Bind failed: {:?}", e))?;

    info!("✓ MXP transport ready");

    let agents: Arc<RwLock<HashMap<String, RegisteredAgent>>> =
        Arc::new(RwLock::new(HashMap::new()));

    info!("🚀 Coordinator ready\n");
    info!("═══════════════════════════════════════════════════════════════");
    info!("Start the other agents:");
    info!("  Terminal 2: cargo run -p agent-code-reviewer");
    info!("  Terminal 3: cargo run -p agent-debugger");
    info!("═══════════════════════════════════════════════════════════════\n");

    // Spawn blocking MXP receiver
    let agents_clone = Arc::clone(&agents);
    let handle_clone = handle.clone();
    tokio::task::spawn_blocking(move || loop {
        let mut buffer = handle_clone.acquire_buffer();
        match handle_clone.receive(&mut buffer) {
            Ok((_len, peer)) => {
                if let Ok(msg) = Message::decode(buffer.as_slice().to_vec()) {
                    match msg.message_type() {
                        Some(MessageType::AgentRegister) => {
                            let payload = String::from_utf8_lossy(msg.payload());
                            info!("📝 Registration from {}: {}", peer, payload);

                            if let Ok(reg) = serde_json::from_str::<serde_json::Value>(&payload) {
                                if let (Some(agent_id), Some(name), Some(caps), Some(endpoint)) = (
                                    reg.get("agent_id").and_then(|v| v.as_str()),
                                    reg.get("name").and_then(|v| v.as_str()),
                                    reg.get("capabilities").and_then(|v| v.as_array()),
                                    reg.get("endpoint").and_then(|v| v.as_str()),
                                ) {
                                    let capabilities: Vec<String> = caps
                                        .iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect();

                                    if let Ok(addr) = endpoint.parse() {
                                        let agent = RegisteredAgent {
                                            agent_id: agent_id.to_string(),
                                            name: name.to_string(),
                                            capabilities: capabilities.clone(),
                                            endpoint: addr,
                                        };

                                        // Use blocking write
                                        let rt = tokio::runtime::Handle::current();
                                        rt.block_on(async {
                                            agents_clone
                                                .write()
                                                .await
                                                .insert(agent_id.to_string(), agent);
                                        });

                                        info!("✓ Registered: {} ({})", name, agent_id);
                                        info!("  Capabilities: {:?}", capabilities);
                                        info!("  Endpoint: {}\n", addr);

                                        // Send ACK
                                        let ack = Message::new(MessageType::Ack, &[]);
                                        let _ = handle_clone.send(&ack.encode(), peer);
                                    }
                                }
                            }
                        }
                        Some(MessageType::Response) => {
                            let payload = String::from_utf8_lossy(msg.payload());
                            info!("📬 Response: {}\n", payload);
                        }
                        Some(MessageType::Call) => {
                            let payload = String::from_utf8_lossy(msg.payload());
                            info!("📞 Call request from {}: {}", peer, payload);

                            if let Ok(request) = serde_json::from_str::<serde_json::Value>(&payload) {
                                if let Some(task_type) = request.get("type").and_then(|v| v.as_str()) {
                                    let rt = tokio::runtime::Handle::current();
                                    let agents_lock = rt.block_on(async { agents_clone.read().await });

                                    let target_agent = match task_type {
                                        "code_review" => agents_lock
                                            .values()
                                            .find(|a| a.capabilities.contains(&"code.review".to_string())),
                                        "debug" => agents_lock
                                            .values()
                                            .find(|a| a.capabilities.contains(&"debug.assist".to_string())),
                                        _ => None,
                                    };

                                    if let Some(agent) = target_agent {
                                        info!("→ Routing to {} at {}", agent.name, agent.endpoint);

                                        // Forward the message
                                        let forward_msg = Message::new(MessageType::Call, msg.payload().to_vec());
                                        match handle_clone.send(&forward_msg.encode(), agent.endpoint) {
                                            Ok(_) => info!("✓ Request forwarded\n"),
                                            Err(e) => error!("Failed to forward: {:?}", e),
                                        }
                                    } else {
                                        error!("No agent found for task type: {}", task_type);
                                    }
                                }
                            }
                        }
                        _ => {
                            info!("Received {:?} from {}", msg.message_type(), peer);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Receive error: {:?}", e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });

    // Keep main thread alive
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
