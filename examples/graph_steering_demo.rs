use anyhow::Result;
use serde_json::json;
use spec_ai::agent::{AgentBuilder, AgentCore};
use spec_ai::config::{AgentProfile, AppConfig};
use spec_ai::persistence::Persistence;
use spec_ai::tools::{Tool, ToolRegistry, ToolResult};
use spec_ai::types::{EdgeType, NodeType};
use std::sync::Arc;
use async_trait::async_trait;

/// This example demonstrates how the knowledge graph steers CLI behavior
/// by influencing tool recommendations, memory recall, and decision making.
#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Knowledge Graph Steering Demo ===\n");

    // Initialize components
    let persistence = Persistence::new("graph_steering_demo.db")?;
    let config = AppConfig::default();
    let session_id = "demo_session";

    // Create an agent profile with graph features enabled
    let mut profile = AgentProfile::default();
    profile.prompt = Some(
        "You are an AI assistant with knowledge graph capabilities. \
         Use the graph to maintain context and make informed decisions."
            .to_string(),
    );
    profile.enable_graph = true;
    profile.graph_memory = true;
    profile.auto_graph = true;
    profile.graph_steering = true;
    profile.graph_depth = 3;
    profile.graph_weight = 0.6;  // 60% graph, 40% semantic
    profile.graph_threshold = 0.7;

    println!("Graph Configuration:");
    println!("  Enabled: {}", profile.enable_graph);
    println!("  Auto Build: {}", profile.auto_graph);
    println!("  Graph Steering: {}", profile.graph_steering);
    println!("  Traversal Depth: {}", profile.graph_depth);
    println!("  Graph Weight: {:.2}\n", profile.graph_weight);

    // Seed the knowledge graph with initial context
    println!("Seeding knowledge graph with context...\n");
    seed_knowledge_graph(&persistence, session_id)?;

    // Demonstrate graph-based tool recommendation
    demonstrate_tool_recommendation(&persistence, session_id)?;

    // Demonstrate graph-based memory recall
    demonstrate_graph_memory(&persistence, session_id).await?;

    // Demonstrate graph steering in decision making
    demonstrate_decision_steering(&persistence, session_id)?;

    println!("\n=== Demo Complete ===");
    Ok(())
}

/// Seed the knowledge graph with initial context
fn seed_knowledge_graph(persistence: &Persistence, session_id: &str) -> Result<()> {
    println!("Creating knowledge nodes:");

    // Create project context
    let project_node = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Project",
        &json!({
            "name": "E-commerce Website",
            "type": "web_application",
            "language": "Python",
            "framework": "Django"
        }),
        None,
    )?;
    println!("  ✓ Project: E-commerce Website");

    // Create task nodes
    let task1 = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Task",
        &json!({
            "name": "Add user authentication",
            "status": "in_progress",
            "priority": "high"
        }),
        None,
    )?;
    println!("  ✓ Task: Add user authentication");

    let task2 = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Task",
        &json!({
            "name": "Implement payment gateway",
            "status": "pending",
            "priority": "high",
            "dependencies": ["authentication"]
        }),
        None,
    )?;
    println!("  ✓ Task: Implement payment gateway");

    // Create tool nodes
    let django_auth = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Tool",
        &json!({
            "name": "django.contrib.auth",
            "type": "authentication_library",
            "language": "Python"
        }),
        None,
    )?;
    println!("  ✓ Tool: django.contrib.auth");

    let stripe = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Tool",
        &json!({
            "name": "Stripe API",
            "type": "payment_gateway",
            "integration": "REST API"
        }),
        None,
    )?;
    println!("  ✓ Tool: Stripe API");

    // Create relationships
    println!("\nCreating relationships:");

    persistence.insert_graph_edge(
        session_id,
        task1,
        project_node,
        EdgeType::PartOf,
        Some("belongs_to"),
        None,
        1.0,
    )?;
    println!("  → Authentication task belongs to project");

    persistence.insert_graph_edge(
        session_id,
        task2,
        project_node,
        EdgeType::PartOf,
        Some("belongs_to"),
        None,
        1.0,
    )?;
    println!("  → Payment task belongs to project");

    persistence.insert_graph_edge(
        session_id,
        task2,
        task1,
        EdgeType::DependsOn,
        Some("requires"),
        Some(&json!({"reason": "Need user accounts for payments"})),
        0.9,
    )?;
    println!("  → Payment task depends on authentication");

    persistence.insert_graph_edge(
        session_id,
        task1,
        django_auth,
        EdgeType::Uses,
        Some("recommended_tool"),
        Some(&json!({"confidence": 0.95})),
        0.95,
    )?;
    println!("  → Authentication task uses Django auth");

    persistence.insert_graph_edge(
        session_id,
        task2,
        stripe,
        EdgeType::Uses,
        Some("recommended_tool"),
        Some(&json!({"confidence": 0.85})),
        0.85,
    )?;
    println!("  → Payment task uses Stripe API");

    Ok(())
}

/// Demonstrate how the graph recommends tools based on context
fn demonstrate_tool_recommendation(persistence: &Persistence, session_id: &str) -> Result<()> {
    println!("\n=== Tool Recommendation Demo ===");
    println!("Query: What tools should I use for the current tasks?\n");

    // Find active tasks
    let task_nodes = persistence.list_graph_nodes(session_id, Some(NodeType::Concept), Some(10))?;

    for task in task_nodes {
        if task.label == "Task" {
            let task_name = task.properties["name"].as_str().unwrap_or("Unknown");
            let status = task.properties["status"].as_str().unwrap_or("unknown");

            println!("Task: {} ({})", task_name, status);

            // Find recommended tools via graph edges
            let edges = persistence.list_graph_edges(session_id, Some(task.id), None)?;

            for edge in edges {
                if edge.edge_type == EdgeType::Uses {
                    if let Some(tool_node) = persistence.get_graph_node(edge.target_id)? {
                        let tool_name = tool_node.properties["name"].as_str().unwrap_or("Unknown");
                        let confidence = edge.weight;

                        if confidence >= 0.7 {  // Graph threshold
                            println!("  → Recommended: {} (confidence: {:.2})", tool_name, confidence);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Demonstrate graph-enhanced memory recall
async fn demonstrate_graph_memory(persistence: &Persistence, session_id: &str) -> Result<()> {
    println!("\n=== Graph Memory Recall Demo ===");
    println!("Query: Tell me about the payment implementation\n");

    // Find payment-related nodes
    let nodes = persistence.list_graph_nodes(session_id, None, None)?;

    for node in &nodes {
        if let Some(name) = node.properties["name"].as_str() {
            if name.to_lowercase().contains("payment") {
                println!("Found relevant node: {} ({})", name, node.label);

                // Traverse graph to find related context
                let neighbors = persistence.traverse_neighbors(
                    session_id,
                    node.id,
                    spec_ai::types::TraversalDirection::Both,
                    2,  // 2 hops
                )?;

                println!("  Related context (via graph traversal):");
                for neighbor in neighbors {
                    if let Some(neighbor_name) = neighbor.properties["name"].as_str() {
                        println!("    - {} ({})", neighbor_name, neighbor.label);
                    }
                }

                // Show dependencies
                let edges = persistence.list_graph_edges(session_id, Some(node.id), None)?;
                for edge in edges {
                    if edge.edge_type == EdgeType::DependsOn {
                        if let Some(dep) = persistence.get_graph_node(edge.target_id)? {
                            if let Some(dep_name) = dep.properties["name"].as_str() {
                                println!("  ⚠ Depends on: {}", dep_name);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Demonstrate how the graph steers decision making
fn demonstrate_decision_steering(persistence: &Persistence, session_id: &str) -> Result<()> {
    println!("\n=== Decision Steering Demo ===");
    println!("Question: Should we start implementing the payment gateway now?\n");

    // Check dependencies via graph
    let nodes = persistence.list_graph_nodes(session_id, Some(NodeType::Concept), None)?;

    for node in nodes {
        if let Some(name) = node.properties["name"].as_str() {
            if name.contains("payment") {
                // Check if dependencies are satisfied
                let deps = persistence.list_graph_edges(session_id, Some(node.id), None)?;

                for dep_edge in deps {
                    if dep_edge.edge_type == EdgeType::DependsOn {
                        if let Some(dep_node) = persistence.get_graph_node(dep_edge.target_id)? {
                            let dep_name = dep_node.properties["name"].as_str().unwrap_or("Unknown");
                            let dep_status = dep_node.properties["status"].as_str().unwrap_or("unknown");

                            println!("Graph Analysis:");
                            println!("  Payment gateway depends on: {}", dep_name);
                            println!("  Dependency status: {}", dep_status);

                            if dep_status != "completed" {
                                println!("\n🔴 Decision: NO - Complete authentication first");
                                println!("   Reason: Graph shows unmet dependency");

                                // Show recommended next action
                                println!("\n   Recommended action based on graph:");
                                println!("   1. Complete '{}'", dep_name);
                                println!("   2. Then proceed with payment gateway");
                            } else {
                                println!("\n🟢 Decision: YES - Dependencies satisfied");
                                println!("   Reason: Graph shows all requirements met");
                            }
                        }
                    }
                }
            }
        }
    }

    // Show graph statistics
    let all_nodes = persistence.list_graph_nodes(session_id, None, None)?;
    let all_edges = persistence.list_graph_edges(session_id, None, None)?;

    println!("\nGraph Statistics:");
    println!("  Total nodes: {}", all_nodes.len());
    println!("  Total edges: {}", all_edges.len());
    println!("  Graph is actively steering decisions based on relationships");

    Ok(())
}