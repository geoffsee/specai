use anyhow::Result;
use serde_json::json;
use spec_ai::persistence::Persistence;
use spec_ai::types::{EdgeType, NodeType, TraversalDirection};

/// This example demonstrates the knowledge graph capabilities of spec-ai.
/// It creates a simple knowledge graph representing relationships between
/// AI concepts and then performs various graph operations.
fn main() -> Result<()> {
    println!("=== Knowledge Graph Example ===\n");

    // Initialize persistence with a temporary database
    let persistence = Persistence::new("knowledge_graph_example.db")?;
    let session_id = "example_session";

    println!("Creating knowledge graph nodes...");

    // Create nodes representing AI concepts
    let ai_id = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Technology",
        &json!({
            "name": "Artificial Intelligence",
            "description": "The simulation of human intelligence in machines",
            "year_coined": 1956
        }),
        None,
    )?;
    println!("  Created node: Artificial Intelligence (ID: {})", ai_id);

    let ml_id = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Technology",
        &json!({
            "name": "Machine Learning",
            "description": "Algorithms that improve through experience",
            "year_coined": 1959
        }),
        None,
    )?;
    println!("  Created node: Machine Learning (ID: {})", ml_id);

    let dl_id = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Technology",
        &json!({
            "name": "Deep Learning",
            "description": "Neural networks with multiple layers",
            "year_coined": 2006
        }),
        None,
    )?;
    println!("  Created node: Deep Learning (ID: {})", dl_id);

    let nn_id = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Technology",
        &json!({
            "name": "Neural Networks",
            "description": "Computing systems inspired by biological neural networks",
            "year_coined": 1943
        }),
        None,
    )?;
    println!("  Created node: Neural Networks (ID: {})", nn_id);

    let nlp_id = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Technology",
        &json!({
            "name": "Natural Language Processing",
            "description": "Interaction between computers and human language",
            "year_coined": 1950
        }),
        None,
    )?;
    println!(
        "  Created node: Natural Language Processing (ID: {})",
        nlp_id
    );

    // Create person nodes
    let turing_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Person",
        &json!({
            "name": "Alan Turing",
            "birth_year": 1912,
            "contribution": "Turing Test, theoretical computer science"
        }),
        None,
    )?;
    println!("  Created node: Alan Turing (ID: {})", turing_id);

    let hinton_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Person",
        &json!({
            "name": "Geoffrey Hinton",
            "birth_year": 1947,
            "contribution": "Backpropagation, deep learning"
        }),
        None,
    )?;
    println!("  Created node: Geoffrey Hinton (ID: {})", hinton_id);

    println!("\nCreating relationships between concepts...");

    // Create hierarchical relationships
    persistence.insert_graph_edge(
        session_id,
        ml_id,
        ai_id,
        EdgeType::PartOf,
        Some("subset_of"),
        Some(&json!({"relationship": "ML is a subset of AI"})),
        1.0,
    )?;
    println!("  Machine Learning -> is part of -> Artificial Intelligence");

    persistence.insert_graph_edge(
        session_id,
        dl_id,
        ml_id,
        EdgeType::PartOf,
        Some("subset_of"),
        Some(&json!({"relationship": "DL is a subset of ML"})),
        1.0,
    )?;
    println!("  Deep Learning -> is part of -> Machine Learning");

    persistence.insert_graph_edge(
        session_id,
        nlp_id,
        ai_id,
        EdgeType::PartOf,
        Some("application_of"),
        Some(&json!({"relationship": "NLP is an application of AI"})),
        1.0,
    )?;
    println!("  NLP -> is part of -> Artificial Intelligence");

    // Create dependency relationships
    persistence.insert_graph_edge(
        session_id,
        dl_id,
        nn_id,
        EdgeType::DependsOn,
        Some("based_on"),
        Some(&json!({"relationship": "Deep Learning is based on Neural Networks"})),
        1.0,
    )?;
    println!("  Deep Learning -> depends on -> Neural Networks");

    // Create person-to-concept relationships
    persistence.insert_graph_edge(
        session_id,
        turing_id,
        ai_id,
        EdgeType::Custom("PIONEERED".to_string()),
        Some("pioneered"),
        Some(&json!({"year": 1950, "contribution": "Turing Test"})),
        1.0,
    )?;
    println!("  Alan Turing -> pioneered -> Artificial Intelligence");

    persistence.insert_graph_edge(
        session_id,
        hinton_id,
        dl_id,
        EdgeType::Custom("PIONEERED".to_string()),
        Some("pioneered"),
        Some(&json!({"year": 2006, "contribution": "Deep Belief Networks"})),
        1.0,
    )?;
    println!("  Geoffrey Hinton -> pioneered -> Deep Learning");

    println!("\n=== Graph Traversal Examples ===\n");

    // Example 1: Find all concepts that are part of AI (1 level deep)
    println!("1. Direct sub-concepts of AI:");
    let ai_parts = persistence.list_graph_edges(session_id, None, Some(ai_id))?;
    for edge in ai_parts {
        if edge.edge_type == EdgeType::PartOf {
            if let Some(node) = persistence.get_graph_node(edge.source_id)? {
                println!(
                    "   - {} ({})",
                    node.properties["name"].as_str().unwrap_or("Unknown"),
                    node.label
                );
            }
        }
    }

    // Example 2: Traverse neighbors of Machine Learning
    println!("\n2. Neighbors of Machine Learning (depth 1):");
    let ml_neighbors =
        persistence.traverse_neighbors(session_id, ml_id, TraversalDirection::Both, 1)?;
    for neighbor in ml_neighbors {
        println!(
            "   - {} ({})",
            neighbor.properties["name"].as_str().unwrap_or("Unknown"),
            neighbor.label
        );
    }

    // Example 3: Find shortest path from Deep Learning to AI
    println!("\n3. Path from Deep Learning to Artificial Intelligence:");
    if let Some(path) = persistence.find_shortest_path(session_id, dl_id, ai_id, None)? {
        println!("   Path length: {} edges", path.length);
        println!("   Nodes in path:");
        for node in path.nodes {
            println!(
                "     -> {}",
                node.properties["name"].as_str().unwrap_or("Unknown")
            );
        }
    }

    // Example 4: List all Person entities
    println!("\n4. All Person entities in the graph:");
    let people = persistence.list_graph_nodes(session_id, Some(NodeType::Entity), None)?;
    for person in people.iter().filter(|n| n.label == "Person") {
        println!(
            "   - {} (born {})",
            person.properties["name"].as_str().unwrap_or("Unknown"),
            person.properties["birth_year"]
        );
    }

    // Example 5: Find all concepts pioneered by people
    println!("\n5. Concepts pioneered by researchers:");
    let all_edges = persistence.list_graph_edges(session_id, None, None)?;
    for edge in all_edges {
        if let EdgeType::Custom(ref edge_type) = edge.edge_type {
            if edge_type == "PIONEERED" {
                if let (Some(person), Some(concept)) = (
                    persistence.get_graph_node(edge.source_id)?,
                    persistence.get_graph_node(edge.target_id)?,
                ) {
                    println!(
                        "   {} pioneered {}",
                        person.properties["name"].as_str().unwrap_or("Unknown"),
                        concept.properties["name"].as_str().unwrap_or("Unknown")
                    );
                }
            }
        }
    }

    // Example 6: Explore the full knowledge graph structure
    println!("\n6. Complete graph structure:");
    println!(
        "   Total nodes: {}",
        persistence.list_graph_nodes(session_id, None, None)?.len()
    );
    println!(
        "   Total edges: {}",
        persistence.list_graph_edges(session_id, None, None)?.len()
    );

    // Count by node type
    let concepts = persistence.list_graph_nodes(session_id, Some(NodeType::Concept), None)?;
    let entities = persistence.list_graph_nodes(session_id, Some(NodeType::Entity), None)?;
    println!("   - Concepts: {}", concepts.len());
    println!("   - Entities: {}", entities.len());

    println!("\n=== Triple Store Pattern Example ===\n");

    // Demonstrate RDF-style triple queries
    println!("Triples in the knowledge graph:");
    let edges = persistence.list_graph_edges(session_id, None, None)?;
    for edge in edges.iter().take(5) {
        if let (Some(subject), Some(object)) = (
            persistence.get_graph_node(edge.source_id)?,
            persistence.get_graph_node(edge.target_id)?,
        ) {
            let predicate = edge
                .predicate
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or_else(|| match &edge.edge_type {
                    EdgeType::Custom(s) => s.as_str(),
                    _ => "relates_to",
                });
            println!(
                "   ({}) --[{}]--> ({})",
                subject.properties["name"].as_str().unwrap_or("?"),
                predicate,
                object.properties["name"].as_str().unwrap_or("?")
            );
        }
    }

    println!("\n=== Knowledge Graph Example Complete ===");

    Ok(())
}
