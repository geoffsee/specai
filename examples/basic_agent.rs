//! Basic Agent Example
//!
//! Demonstrates how to create and use an agent with the mock provider.

use anyhow::Result;
use spec_ai::agent::AgentBuilder;
use spec_ai::config::{AgentProfile, AppConfig};
use spec_ai::persistence::Persistence;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Basic Agent Example ===\n");

    // Load configuration (or use defaults)
    let config = AppConfig::load().unwrap_or_default();

    // Create persistence layer
    let db_path = PathBuf::from("examples/demo.duckdb");
    let persistence = Persistence::new(&db_path)?;

    // Create an agent profile
    let profile = AgentProfile {
        prompt: Some("You are a helpful AI assistant. Be concise and friendly.".to_string()),
        style: Some("professional".to_string()),
        temperature: Some(0.7),
        model_provider: Some("mock".to_string()),
        model_name: Some("mock-model".to_string()),
        allowed_tools: None,
        denied_tools: None,
        memory_k: 10,
        top_p: 0.9,
        max_context_tokens: Some(4096),
        ..AgentProfile::default()
    };

    // Build the agent
    println!("Building agent...");
    let mut agent = AgentBuilder::new()
        .with_profile(profile)
        .with_config(config.clone())
        .with_persistence(persistence)
        .with_session_id("example-session")
        .build()?;

    println!("Agent created with session ID: {}\n", agent.session_id());

    // Interact with the agent
    let questions = vec![
        "Hello! How are you today?",
        "What can you help me with?",
        "Tell me about Rust programming.",
    ];

    for (i, question) in questions.iter().enumerate() {
        println!("Q{}: {}", i + 1, question);

        let output = agent.run_step(question).await?;

        println!("A{}: {}", i + 1, output.response);

        if let Some(usage) = output.token_usage {
            println!(
                "   (tokens: {} prompt + {} completion = {} total)",
                usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
            );
        }

        println!();
    }

    // Show conversation history
    println!("=== Conversation History ===");
    let history = agent.conversation_history();
    println!("Total messages in memory: {}", history.len());

    println!("\nExample completed successfully!");
    Ok(())
}
