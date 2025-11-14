use anyhow::Result;
use spec_ai::agent::factory::create_provider;
use spec_ai::agent::{AgentBuilder, AgentCore};
use spec_ai::config::{AgentProfile, AppConfig, ModelConfig};
use spec_ai::persistence::Persistence;
use std::sync::Arc;
use std::time::Instant;
use tokio;

/// This example demonstrates hierarchical multi-model reasoning
/// using a fast model (Llama-3.2-3B) for preliminary tasks and
/// a larger model for complex reasoning.
#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Multi-Model Performance Benchmark ===\n");

    // Initialize components
    let persistence = Persistence::new("multi_model_demo.db")?;
    let session_id = "benchmark_session";

    // Configure main model (e.g., GPT-4 or Claude)
    let main_model_config = ModelConfig {
        provider: "mock".to_string(), // Change to "openai" or "anthropic" for real usage
        model_name: Some("gpt-4".to_string()),
        embeddings_model: None,
        api_key_source: None,
        temperature: 0.7,
    };

    // Configure fast model (Llama-3.2-3B via MLX)
    let fast_model_config = ModelConfig {
        provider: "mock".to_string(), // Change to "mlx" for real usage
        model_name: Some("mlx-community/Llama-3.2-3B-Instruct-4bit".to_string()),
        embeddings_model: None,
        api_key_source: None,
        temperature: 0.3,
    };

    // Create model providers
    let main_provider = create_provider(&main_model_config)?;
    let fast_provider = create_provider(&fast_model_config)?;

    // Create agent profile with multi-model configuration
    let profile = AgentProfile {
        prompt: Some(
            "You are an AI assistant with hierarchical reasoning capabilities. \
             Use fast models for quick tasks and escalate to main model for complex reasoning."
                .to_string(),
        ),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: None,
        denied_tools: None,
        memory_k: 10,
        top_p: 0.9,
        max_context_tokens: Some(4096),

        // Graph configuration (disabled for benchmark)
        enable_graph: false,
        graph_memory: false,
        auto_graph: false,
        graph_steering: false,
        graph_depth: 3,
        graph_weight: 0.5,
        graph_threshold: 0.7,

        // Multi-model configuration
        fast_reasoning: true,
        fast_model_provider: Some("mlx".to_string()),
        fast_model_name: Some("mlx-community/Llama-3.2-3B-Instruct-4bit".to_string()),
        fast_model_temperature: 0.3,
        fast_model_tasks: vec![
            "entity_extraction".to_string(),
            "graph_analysis".to_string(),
            "decision_routing".to_string(),
            "tool_selection".to_string(),
            "confidence_scoring".to_string(),
        ],
        escalation_threshold: 0.6, // Escalate if confidence < 60%
    };

    // Build agent with fast model provider
    let mut agent = AgentCore::new(
        profile.clone(),
        main_provider,
        None, // No embeddings for this demo
        persistence,
        session_id.to_string(),
        Some("benchmark_agent".to_string()),
        Arc::new(spec_ai::tools::ToolRegistry::new()),
        Arc::new(spec_ai::policy::PolicyEngine::new()),
    )
    .with_fast_provider(fast_provider);

    println!("Configuration:");
    println!(
        "  Main Model: {}",
        main_model_config.model_name.unwrap_or_default()
    );
    println!(
        "  Fast Model: {}",
        profile.fast_model_name.unwrap_or_default()
    );
    println!(
        "  Fast Model Temperature: {}",
        profile.fast_model_temperature
    );
    println!("  Escalation Threshold: {}\n", profile.escalation_threshold);

    // Benchmark different task types
    benchmark_entity_extraction(&mut agent).await?;
    benchmark_decision_routing(&mut agent).await?;
    benchmark_complex_reasoning(&mut agent).await?;
    benchmark_mixed_workload(&mut agent).await?;

    println!("\n=== Benchmark Complete ===");
    Ok(())
}

/// Benchmark entity extraction (should use fast model)
async fn benchmark_entity_extraction(agent: &mut AgentCore) -> Result<()> {
    println!("--- Entity Extraction Benchmark ---");

    let test_cases = vec![
        "John Smith works at Google in Mountain View.",
        "The meeting is scheduled for tomorrow at 3pm with sarah@example.com.",
        "Visit https://example.com for more information about our products.",
    ];

    for (i, text) in test_cases.iter().enumerate() {
        let start = Instant::now();

        // In production, this would trigger fast model for entity extraction
        let response = agent
            .run_step(&format!("Extract all entities from this text: {}", text))
            .await?;

        let elapsed = start.elapsed();
        println!(
            "  Case {}: {:.2}ms - {} tokens",
            i + 1,
            elapsed.as_millis(),
            response.token_usage.map(|u| u.total_tokens).unwrap_or(0)
        );
    }

    Ok(())
}

/// Benchmark decision routing (should use fast model)
async fn benchmark_decision_routing(agent: &mut AgentCore) -> Result<()> {
    println!("\n--- Decision Routing Benchmark ---");

    let queries = vec![
        "What's 2+2?",               // Simple, route to fast
        "Explain quantum computing", // Complex, route to main
        "List the days of the week", // Simple, route to fast
    ];

    for (i, query) in queries.iter().enumerate() {
        let start = Instant::now();

        // In production, fast model would determine routing
        let response = agent
            .run_step(&format!("Determine complexity and route: {}", query))
            .await?;

        let elapsed = start.elapsed();
        println!(
            "  Query {}: {:.2}ms - Routed to: {}",
            i + 1,
            elapsed.as_millis(),
            if query.contains("quantum") {
                "main model"
            } else {
                "fast model"
            }
        );
    }

    Ok(())
}

/// Benchmark complex reasoning (should use main model)
async fn benchmark_complex_reasoning(agent: &mut AgentCore) -> Result<()> {
    println!("\n--- Complex Reasoning Benchmark ---");

    let complex_tasks = vec![
        "Analyze the pros and cons of renewable energy",
        "Write a haiku about artificial intelligence",
        "Explain the difference between supervised and unsupervised learning",
    ];

    for (i, task) in complex_tasks.iter().enumerate() {
        let start = Instant::now();

        // These should escalate to main model due to complexity
        let response = agent.run_step(task).await?;

        let elapsed = start.elapsed();
        println!(
            "  Task {}: {:.2}ms - {} tokens (main model)",
            i + 1,
            elapsed.as_millis(),
            response.token_usage.map(|u| u.total_tokens).unwrap_or(0)
        );
    }

    Ok(())
}

/// Benchmark mixed workload
async fn benchmark_mixed_workload(agent: &mut AgentCore) -> Result<()> {
    println!("\n--- Mixed Workload Benchmark ---");

    let mixed_tasks = vec![
        ("Extract: Apple Inc. stock price is $150", "fast"),
        ("Analyze market trends for technology sector", "main"),
        ("Count words in this sentence", "fast"),
        ("Design a database schema for e-commerce", "main"),
        (
            "Find URLs in: Check https://example.com and https://test.org",
            "fast",
        ),
    ];

    let mut fast_total_time = 0u128;
    let mut main_total_time = 0u128;
    let mut fast_count = 0;
    let mut main_count = 0;

    for (task, expected_model) in mixed_tasks {
        let start = Instant::now();

        let _response = agent.run_step(task).await?;

        let elapsed = start.elapsed().as_millis();

        if expected_model == "fast" {
            fast_total_time += elapsed;
            fast_count += 1;
        } else {
            main_total_time += elapsed;
            main_count += 1;
        }
    }

    println!("\n  Summary:");
    println!(
        "    Fast Model Tasks: {} (avg {:.2}ms)",
        fast_count,
        if fast_count > 0 {
            fast_total_time as f64 / fast_count as f64
        } else {
            0.0
        }
    );
    println!(
        "    Main Model Tasks: {} (avg {:.2}ms)",
        main_count,
        if main_count > 0 {
            main_total_time as f64 / main_count as f64
        } else {
            0.0
        }
    );

    let speedup = if main_count > 0 && fast_count > 0 {
        (main_total_time as f64 / main_count as f64) / (fast_total_time as f64 / fast_count as f64)
    } else {
        1.0
    };

    println!("    Speedup Factor: {:.2}x for fast model tasks", speedup);

    Ok(())
}
