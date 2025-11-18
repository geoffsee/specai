use anyhow::Result;
use serde_json::json;
use spec_ai::persistence::Persistence;
use spec_ai::tools::builtin::GraphTool;
use spec_ai::tools::Tool;
use spec_ai::types::{EdgeType, NodeType};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_node_degree_and_list_hubs() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("graph_degree.db");
    let persistence = Arc::new(Persistence::new(&db_path)?);
    let tool = GraphTool::new(persistence.clone());
    let session_id = "degree_session";

    // Create nodes A, B, C
    let a_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "A",
        &json!({ "name": "A" }),
        None,
    )?;
    let b_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "B",
        &json!({ "name": "B" }),
        None,
    )?;
    let c_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "C",
        &json!({ "name": "C" }),
        None,
    )?;

    // Edges: A -> B, A -> C, B -> C
    persistence.insert_graph_edge(session_id, a_id, b_id, EdgeType::DependsOn, None, None, 1.0)?;
    persistence.insert_graph_edge(session_id, a_id, c_id, EdgeType::DependsOn, None, None, 1.0)?;
    persistence.insert_graph_edge(session_id, b_id, c_id, EdgeType::DependsOn, None, None, 1.0)?;

    // Node degree for C: fan-in hub (2 incoming, 0 outgoing)
    let args = json!({
        "operation": "node_degree",
        "session_id": session_id,
        "node_id": c_id
    });
    let result = tool.execute(args).await?;
    assert!(result.success);
    let payload: serde_json::Value = serde_json::from_str(&result.output)?;
    assert_eq!(payload["node_id"].as_i64().unwrap(), c_id);
    assert_eq!(payload["in_degree"].as_i64().unwrap(), 2);
    assert_eq!(payload["out_degree"].as_i64().unwrap(), 0);
    assert_eq!(payload["total_degree"].as_i64().unwrap(), 2);

    // List hubs by incoming degree (fan-in); C should appear as the top (and only) hub with degree 2
    let args = json!({
        "operation": "list_hubs",
        "session_id": session_id,
        "direction": "incoming",
        "min_degree": 2,
        "limit": 5
    });
    let result = tool.execute(args).await?;
    assert!(result.success);
    let payload: serde_json::Value = serde_json::from_str(&result.output)?;
    assert_eq!(payload["direction"], "incoming");
    assert_eq!(payload["min_degree"].as_i64().unwrap(), 2);
    assert_eq!(payload["count"].as_u64().unwrap(), 1);

    let hubs = payload["hubs"].as_array().unwrap();
    assert_eq!(hubs.len(), 1);
    let hub = &hubs[0];
    assert_eq!(hub["node"]["id"].as_i64().unwrap(), c_id);
    assert_eq!(hub["in_degree"].as_i64().unwrap(), 2);
    assert_eq!(hub["out_degree"].as_i64().unwrap(), 0);
    assert_eq!(hub["total_degree"].as_i64().unwrap(), 2);

    // Sanity check: A should be the fan-out hub if we look at outgoing degree
    let args = json!({
        "operation": "list_hubs",
        "session_id": session_id,
        "direction": "outgoing",
        "min_degree": 1,
        "limit": 5
    });
    let result = tool.execute(args).await?;
    assert!(result.success);
    let payload: serde_json::Value = serde_json::from_str(&result.output)?;
    let hubs = payload["hubs"].as_array().unwrap();
    assert!(!hubs.is_empty());
    assert_eq!(hubs[0]["node"]["id"].as_i64().unwrap(), a_id);
    assert_eq!(hubs[0]["out_degree"].as_i64().unwrap(), 2);

    Ok(())
}
