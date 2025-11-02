//! Example demonstrating system prompts and context window management.

use std::collections::HashMap;

use agent_adapters::ollama::{OllamaAdapter, OllamaConfig};
use agent_adapters::traits::{InferenceRequest, MessageRole, PromptMessage};
use agent_prompts::{ContextMessage, ContextWindowConfig, ContextWindowManager, PromptTemplate};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    println!("=== MXP Agents: Prompt Management Example ===\n");

    // Example 1: System Prompt Templates
    demonstrate_templates()?;

    // Example 2: Context Window Management
    demonstrate_context_management();

    // Example 3: Using System Prompts with Adapters
    demonstrate_adapter_usage()?;

    Ok(())
}

fn demonstrate_templates() -> Result<()> {
    println!("--- Example 1: System Prompt Templates ---\n");

    // Create a template with variables
    let template =
        PromptTemplate::builder("You are {{role}}. Your task is to {{task}}. {{constraints}}")
            .with_variable("role", "a helpful AI assistant")
            .with_variable("task", "answer questions accurately and concisely")
            .with_variable("constraints", "Always be respectful and professional.")
            .build()?;

    let rendered = template.render()?;
    println!("Rendered template:\n{rendered}\n");

    // Runtime variable override
    let mut runtime_vars = HashMap::new();
    runtime_vars.insert("role".to_owned(), "a code reviewer".to_owned());
    runtime_vars.insert(
        "task".to_owned(),
        "review Rust code for best practices".to_owned(),
    );

    let specialized = template.render_with(&runtime_vars)?;
    println!("Specialized template:\n{specialized}\n");

    Ok(())
}

fn demonstrate_context_management() {
    println!("--- Example 2: Context Window Management ---\n");

    let config = ContextWindowConfig {
        max_tokens: 500,
        recent_window_size: 5,
        min_importance_threshold: 40,
        enable_summarization: true,
    };

    let mut manager = ContextWindowManager::new(config);

    // Add a pinned system context
    let system_context = ContextMessage::new(
        "system",
        "You are an expert Rust developer helping with MXP protocol implementation.",
    )
    .with_importance(100)
    .pinned();

    manager.add_message(system_context);

    // Simulate a conversation
    println!(
        "Simulating conversation with {} token budget...\n",
        manager.max_tokens()
    );

    for i in 1..=20 {
        let importance = if i % 5 == 0 { 80 } else { 50 };

        manager.add_message(
            ContextMessage::new(
                "user",
                format!("Question {i}: How do I implement feature X?"),
            )
            .with_importance(importance),
        );

        manager.add_message(
            ContextMessage::new(
                "assistant",
                format!("Answer {i}: Here's how to implement feature X..."),
            )
            .with_importance(importance),
        );
    }

    println!("Messages in context: {}", manager.get_messages().len());
    println!(
        "Current tokens: {} / {}",
        manager.current_tokens(),
        manager.max_tokens()
    );

    if let Some(summary) = manager.summarized_history() {
        println!("\nSummarized history:\n{summary}");
    }

    // Show that pinned messages are preserved
    let messages = manager.get_messages();
    let has_system = messages.iter().any(|m| m.role == "system");
    println!("\nSystem context preserved: {has_system}");
}

fn demonstrate_adapter_usage() -> Result<()> {
    println!("\n--- Example 3: System Prompts with Adapters ---\n");

    // Create a system prompt template
    let system_template = PromptTemplate::builder(
        "You are {{agent_name}}, an AI agent in the MXP mesh. {{personality}}",
    )
    .with_variable("agent_name", "RelayBot")
    .with_variable("personality", "You are concise, technical, and helpful.")
    .build()?;

    let system_prompt = system_template.render()?;
    println!("System prompt: {system_prompt}\n");

    // Create an adapter (Ollama for local testing)
    let _adapter = OllamaAdapter::new(OllamaConfig::new("gemma2:2b"))?;

    // Build a request with the new system_prompt field
    let request =
        InferenceRequest::new(vec![PromptMessage::new(MessageRole::User, "What is MXP?")])?
            .with_system_prompt(system_prompt)
            .with_temperature(0.7)
            .with_max_output_tokens(100);

    println!("Request configuration:");
    println!("  - System prompt: {}", request.system_prompt().is_some());
    println!("  - Messages: {}", request.messages().len());
    println!("  - Temperature: {:?}", request.temperature());
    println!("  - Max tokens: {:?}\n", request.max_output_tokens());

    // Note: Actual inference would require a running Ollama instance
    println!("(Skipping actual inference - requires running Ollama instance)");
    println!("\nThe adapter will:");
    println!("  - OpenAI/Ollama: Prepend system prompt as first message");
    println!("  - Anthropic: Use dedicated 'system' parameter");
    println!("  - Gemini: Use 'systemInstruction' field");

    Ok(())
}
