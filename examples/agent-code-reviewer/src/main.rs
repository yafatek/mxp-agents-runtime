//! Code Review Agent - Reviews Rust code via MXP

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_adapters::traits::{InferenceRequest, MessageRole, ModelAdapter, PromptMessage};
use agent_primitives::AgentId;
use agent_prompts::PromptTemplate;
use anyhow::Result;
use futures::StreamExt;
use mxp::{Message, MessageType, Transport, TransportConfig};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

const AGENT_PORT: u16 = 50052;
const COORDINATOR_ADDR: &str = "127.0.0.1:50051";

#[derive(Serialize, Deserialize)]
struct RegisterPayload {
    agent_id: String,
    name: String,
    capabilities: Vec<String>,
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    info!("🔍 Code Review Agent starting...");

    let agent_id = AgentId::random();
    info!("Agent ID: {}", agent_id);

    // Create MXP transport
    let transport = Transport::new(TransportConfig {
        buffer_size: 4096,
        max_buffers: 128,
        read_timeout: Some(Duration::from_secs(30)),
        write_timeout: Some(Duration::from_secs(10)),
        #[cfg(feature = "debug-tools")]
        pcap_send_path: None,
        #[cfg(feature = "debug-tools")]
        pcap_recv_path: None,
    });

    let addr: SocketAddr = format!("127.0.0.1:{}", AGENT_PORT).parse()?;
    let handle = transport
        .bind(addr)
        .map_err(|e| anyhow::anyhow!("bind failed: {:?}", e))?;
    info!("✓ MXP transport bound to {}", addr);

    // Create Ollama adapter
    let adapter = Arc::new(OllamaAdapter::new(OllamaConfig::new("gemma3"))?);
    info!("✓ Ollama adapter ready");

    // System prompt
    let template = PromptTemplate::builder(
        "You are {{name}}, an expert Rust code reviewer. \
         Focus on: safety, performance, idiomatic patterns. Be concise.",
    )
    .with_variable("name", "CodeReviewer")
    .build()?;
    let system_prompt = template.render()?;

    // Register with coordinator
    tokio::spawn({
        let handle_clone = handle.clone();
        let agent_id_str = agent_id.to_string();
        async move {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let payload = RegisterPayload {
                agent_id: agent_id_str,
                name: "CodeReviewer".to_string(),
                capabilities: vec!["code.review".to_string()],
                endpoint: format!("127.0.0.1:{}", AGENT_PORT),
            };

            let message = Message::new(
                MessageType::AgentRegister,
                serde_json::to_vec(&payload).unwrap(),
            );

            let coordinator: SocketAddr = COORDINATOR_ADDR.parse().unwrap();
            let encoded = message.encode();
            
            match handle_clone.send(&encoded, coordinator) {
                Ok(_) => info!("✓ Registered with coordinator"),
                Err(e) => error!("Registration failed: {:?}", e),
            }
        }
    });

    info!("🚀 Agent ready, waiting for review requests...\n");

    // Message loop
    tokio::task::spawn_blocking(move || loop {
        let mut buffer = handle.acquire_buffer();
        match handle.receive(&mut buffer) {
            Ok((_len, peer)) => {
                if let Ok(msg) = Message::decode(buffer.as_slice().to_vec()) {
                    info!("📨 Received {:?} from {}", msg.message_type(), peer);

                    if matches!(msg.message_type(), Some(MessageType::Call)) {
                        let payload_bytes = msg.payload();
                        if let Ok(request) = serde_json::from_slice::<serde_json::Value>(payload_bytes) {
                            if let Some(code) = request.get("code").and_then(|v| v.as_str()) {
                                info!("🔍 Reviewing code...\n");

                                let review_request = InferenceRequest::new(vec![PromptMessage::new(
                                    MessageRole::User,
                                    format!("Review this Rust code:\n\n```rust\n{}\n```", code),
                                )])
                                .unwrap()
                                .with_system_prompt(&system_prompt)
                                .with_temperature(0.3);

                                let rt = tokio::runtime::Handle::current();
                                if let Ok(mut stream) = rt.block_on(adapter.infer(review_request)) {
                                    let mut review = String::new();
                                    while let Some(Ok(chunk)) = rt.block_on(stream.next()) {
                                        print!("{}", chunk.delta);
                                        review.push_str(&chunk.delta);
                                    }
                                    println!("\n");

                                    // Build response with request_id if present
                                    let mut response = serde_json::json!({
                                        "agent": "CodeReviewer",
                                        "review": review,
                                        "status": "complete"
                                    });

                                    // Copy request_id if present
                                    if let Some(request_id) = request.get("request_id") {
                                        if let Some(obj) = response.as_object_mut() {
                                            obj.insert("request_id".to_string(), request_id.clone());
                                        }
                                    }

                                    let response_msg = Message::new(
                                        MessageType::Response,
                                        serde_json::to_vec(&response).unwrap(),
                                    );

                                    if let Err(e) = handle.send(&response_msg.encode(), peer) {
                                        error!("Failed to send response: {:?}", e);
                                    } else {
                                        info!("✓ Review sent\n");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Receive error: {:?}", e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    })
    .await?;

    Ok(())
}
