//! Simple MXP agent example demonstrating system prompts and basic usage.

use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_adapters::traits::{InferenceRequest, MessageRole, ModelAdapter, PromptMessage};
use agent_prompts::{ContextWindowConfig, PromptTemplate};
use anyhow::Result;
use futures::StreamExt;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    info!("=== MXP Agents: Basic Example ===\n");

    // Example 1: Simple inference with system prompt
    simple_inference().await?;

    // Example 2: Using templates
    template_example().await?;

    // Example 3: With context management
    context_management_example().await?;

    Ok(())
}

/// Example 1: Simple inference with system prompt
async fn simple_inference() -> Result<()> {
    info!("--- Example 1: Simple Inference ---");

    // Create adapter
    let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;

    // Build request with system prompt
    let request = InferenceRequest::new(vec![PromptMessage::new(
        MessageRole::User,
        "Explain MXP protocol in one sentence",
    )])?
    .with_system_prompt("You are an expert on MXP protocol. Be concise.")
    .with_temperature(0.7);

    info!("Sending request...");

    // Get response
    let mut stream = adapter.infer(request).await?;
    let mut response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        response.push_str(&chunk.delta);
        if chunk.done {
            break;
        }
    }

    info!("Response: {}\n", response);
    Ok(())
}

/// Example 2: Using templates for dynamic system prompts
async fn template_example() -> Result<()> {
    info!("--- Example 2: Template-Based System Prompts ---");

    // Create a reusable template
    let template = PromptTemplate::builder("You are {{role}}. {{task}}")
        .with_variable("role", "a helpful AI assistant")
        .with_variable("task", "Answer questions clearly and concisely.")
        .build()?;

    let system_prompt = template.render()?;
    info!("System prompt: {}", system_prompt);

    let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;

    let request =
        InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "What is Rust?")])?
            .with_system_prompt(system_prompt);

    let mut stream = adapter.infer(request).await?;
    let mut response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        response.push_str(&chunk.delta);
        if chunk.done {
            break;
        }
    }

    info!("Response: {}\n", response);
    Ok(())
}

/// Example 3: With context window management
async fn context_management_example() -> Result<()> {
    info!("--- Example 3: Context Window Management ---");

    // Create adapter with context management
    let adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?.with_context_config(
        ContextWindowConfig {
            max_tokens: 2048,
            recent_window_size: 5,
            ..Default::default()
        },
    );

    info!(
        "Context config: max_tokens={}, recent_window={}",
        adapter.context_config().map_or(0, |c| c.max_tokens),
        adapter.context_config().map_or(0, |c| c.recent_window_size)
    );

    // Simulate a conversation
    let messages = vec![
        PromptMessage::new(MessageRole::User, "Hello!"),
        PromptMessage::new(MessageRole::Assistant, "Hi there!"),
        PromptMessage::new(MessageRole::User, "What can you help with?"),
    ];

    let request =
        InferenceRequest::new(messages)?.with_system_prompt("You are a helpful assistant");

    let mut stream = adapter.infer(request).await?;
    let mut response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        response.push_str(&chunk.delta);
        if chunk.done {
            break;
        }
    }

    info!("Response: {}\n", response);
    Ok(())
}
