//! Test client to send requests to agents via MXP

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use mxp::{Message, MessageType, Transport, TransportConfig};

const COORDINATOR_ADDR: &str = "127.0.0.1:50051";

fn main() -> Result<()> {
    println!("🧪 MXP Agent Test Client\n");
    println!("Select test:");
    println!("  1. Code Review");
    println!("  2. Debug Error");
    println!();
    print!("Enter choice (1 or 2): ");
    
    // Read from stdin
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let choice = input.trim();

    // Create transport
    let config = TransportConfig {
        buffer_size: 4096,
        max_buffers: 128,
        read_timeout: Some(Duration::from_secs(30)),
        write_timeout: Some(Duration::from_secs(10)),
        #[cfg(feature = "debug-tools")]
        pcap_send_path: None,
        #[cfg(feature = "debug-tools")]
        pcap_recv_path: None,
    };

    let transport = Transport::new(config);
    let addr: SocketAddr = "127.0.0.1:0".parse()?; // Bind to any available port
    let handle = transport
        .bind(addr)
        .map_err(|e| anyhow::anyhow!("bind failed: {:?}", e))?;

    let coordinator: SocketAddr = COORDINATOR_ADDR.parse()?;

    match choice {
        "1" => {
            println!("\n📝 Sending code review request...\n");
            
            let code = r#"pub fn process_data(data: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for item in data {
        result.push(item.to_uppercase());
    }
    result
}"#;

            let request = serde_json::json!({
                "type": "code_review",
                "code": code,
            });

            let message = Message::new(MessageType::Call, serde_json::to_vec(&request)?);
            let encoded = message.encode();
            
            handle.send(&encoded, coordinator)
                .map_err(|e| anyhow::anyhow!("send failed: {:?}", e))?;
            println!("✓ Request sent to coordinator");
            println!("⏳ Waiting for response...\n");

            // Wait for response
            let mut buffer = handle.acquire_buffer();
            match handle.receive(&mut buffer) {
                Ok((_len, peer)) => {
                    if let Ok(msg) = Message::decode(buffer.as_slice().to_vec()) {
                        if let Some(MessageType::Response) = msg.message_type() {
                            if let Ok(response) = serde_json::from_slice::<serde_json::Value>(msg.payload()) {
                                println!("📬 Response from {}:\n", peer);
                                println!("{}\n", serde_json::to_string_pretty(&response)?);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Receive error: {:?}", e);
                }
            }
        }
        "2" => {
            println!("\n🐛 Sending debug request...\n");
            
            let error = "I'm getting 'cannot borrow as mutable' error in Rust. \
                         The code tries to modify a vector while iterating over it.";

            let request = serde_json::json!({
                "type": "debug",
                "error": error,
            });

            let message = Message::new(MessageType::Call, serde_json::to_vec(&request)?);
            let encoded = message.encode();
            
            handle.send(&encoded, coordinator)
                .map_err(|e| anyhow::anyhow!("send failed: {:?}", e))?;
            println!("✓ Request sent to coordinator");
            println!("⏳ Waiting for solution...\n");

            // Wait for response
            let mut buffer = handle.acquire_buffer();
            match handle.receive(&mut buffer) {
                Ok((_len, peer)) => {
                    if let Ok(msg) = Message::decode(buffer.as_slice().to_vec()) {
                        if let Some(MessageType::Response) = msg.message_type() {
                            if let Ok(response) = serde_json::from_slice::<serde_json::Value>(msg.payload()) {
                                println!("📬 Response from {}:\n", peer);
                                println!("{}\n", serde_json::to_string_pretty(&response)?);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Receive error: {:?}", e);
                }
            }
        }
        _ => {
            println!("Invalid choice!");
        }
    }

    Ok(())
}

