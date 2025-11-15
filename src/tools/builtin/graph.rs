use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::persistence::Persistence;
use crate::tools::{Tool, ToolResult};
use crate::types::{EdgeType, NodeType, TraversalDirection};

pub struct GraphTool {
    persistence: Arc<Persistence>,
}

impl GraphTool {
    pub fn new(persistence: Arc<Persistence>) -> Self {
        Self { persistence }
    }
}

#[async_trait]
impl Tool for GraphTool {
    fn name(&self) -> &str {
        "graph"
    }

    fn description(&self) -> &str {
        "Create, query, and traverse knowledge graphs. Supports operations: \
         create_node, create_edge, delete_node, delete_edge, get_node, get_edge, \
         list_nodes, list_edges, find_path, traverse_neighbors"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": [
                        "create_node", "create_edge", "delete_node", "delete_edge",
                        "get_node", "get_edge", "list_nodes", "list_edges",
                        "find_path", "traverse_neighbors", "update_node"
                    ],
                    "description": "The graph operation to perform"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID for graph isolation"
                },
                "node_id": {
                    "type": "integer",
                    "description": "Node ID (for get_node, delete_node, update_node, traverse_neighbors)"
                },
                "edge_id": {
                    "type": "integer",
                    "description": "Edge ID (for get_edge, delete_edge)"
                },
                "node_type": {
                    "type": "string",
                    "enum": ["entity", "concept", "fact", "message", "tool_result", "event"],
                    "description": "Type of node to create or filter by"
                },
                "label": {
                    "type": "string",
                    "description": "Semantic label for the node (e.g., 'Person', 'Location', 'Action')"
                },
                "properties": {
                    "type": "object",
                    "description": "JSON properties for the node or edge"
                },
                "source_id": {
                    "type": "integer",
                    "description": "Source node ID for edge creation or path finding"
                },
                "target_id": {
                    "type": "integer",
                    "description": "Target node ID for edge creation or path finding"
                },
                "edge_type": {
                    "type": "string",
                    "enum": [
                        "RELATES_TO", "CAUSED_BY", "PART_OF", "MENTIONS",
                        "FOLLOWS_FROM", "USES", "PRODUCES", "DEPENDS_ON"
                    ],
                    "description": "Type of edge relationship"
                },
                "custom_edge_type": {
                    "type": "string",
                    "description": "Custom edge type if not using predefined types"
                },
                "predicate": {
                    "type": "string",
                    "description": "RDF-style predicate for the edge"
                },
                "weight": {
                    "type": "number",
                    "default": 1.0,
                    "description": "Weight for the edge"
                },
                "direction": {
                    "type": "string",
                    "enum": ["outgoing", "incoming", "both"],
                    "default": "outgoing",
                    "description": "Direction for traversal operations"
                },
                "depth": {
                    "type": "integer",
                    "default": 1,
                    "minimum": 1,
                    "maximum": 10,
                    "description": "Depth for traversal operations"
                },
                "max_hops": {
                    "type": "integer",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 20,
                    "description": "Maximum hops for path finding"
                },
                "limit": {
                    "type": "integer",
                    "default": 100,
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "Limit for list operations"
                }
            },
            "required": ["operation", "session_id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let operation = args["operation"]
            .as_str()
            .context("operation must be a string")?;

        let session_id = args["session_id"]
            .as_str()
            .context("session_id must be a string")?;

        // Clone persistence for use in spawn_blocking
        let persistence = Arc::clone(&self.persistence);

        match operation {
            "create_node" => {
                let node_type = args["node_type"]
                    .as_str()
                    .context("node_type is required for create_node")?;
                let label = args["label"]
                    .as_str()
                    .context("label is required for create_node")?;
                let properties = args["properties"].clone();

                let node_type = NodeType::from_str(node_type);
                let session_id = session_id.to_string();
                let label = label.to_string();

                let result = tokio::task::spawn_blocking(move || {
                    persistence.insert_graph_node(&session_id, node_type, &label, &properties, None)
                })
                .await
                .context("task join error")??;

                Ok(ToolResult::success(
                    json!({
                        "node_id": result,
                        "message": format!("Created node with ID {}", result)
                    })
                    .to_string(),
                ))
            }

            "create_edge" => {
                let source_id = args["source_id"]
                    .as_i64()
                    .context("source_id is required for create_edge")?;
                let target_id = args["target_id"]
                    .as_i64()
                    .context("target_id is required for create_edge")?;

                let edge_type = if let Some(custom) = args["custom_edge_type"].as_str() {
                    EdgeType::Custom(custom.to_string())
                } else if let Some(et) = args["edge_type"].as_str() {
                    EdgeType::from_str(et)
                } else {
                    EdgeType::RelatesTo
                };

                let predicate = args["predicate"].as_str().map(|s| s.to_string());
                let properties = if args["properties"].is_null() {
                    None
                } else {
                    Some(args["properties"].clone())
                };
                let weight = args["weight"].as_f64().unwrap_or(1.0) as f32;
                let session_id = session_id.to_string();

                let result = tokio::task::spawn_blocking(move || {
                    persistence.insert_graph_edge(
                        &session_id,
                        source_id,
                        target_id,
                        edge_type,
                        predicate.as_deref(),
                        properties.as_ref(),
                        weight,
                    )
                })
                .await
                .context("task join error")??;

                Ok(ToolResult::success(
                    json!({
                        "edge_id": result,
                        "message": format!("Created edge with ID {}", result)
                    })
                    .to_string(),
                ))
            }

            "get_node" => {
                let node_id = args["node_id"]
                    .as_i64()
                    .context("node_id is required for get_node")?;

                let result =
                    tokio::task::spawn_blocking(move || persistence.get_graph_node(node_id))
                        .await
                        .context("task join error")??;

                match result {
                    Some(node) => Ok(ToolResult::success(serde_json::to_string_pretty(&node)?)),
                    None => Ok(ToolResult::failure(format!("Node {} not found", node_id))),
                }
            }

            "get_edge" => {
                let edge_id = args["edge_id"]
                    .as_i64()
                    .context("edge_id is required for get_edge")?;

                let result =
                    tokio::task::spawn_blocking(move || persistence.get_graph_edge(edge_id))
                        .await
                        .context("task join error")??;

                match result {
                    Some(edge) => Ok(ToolResult::success(serde_json::to_string_pretty(&edge)?)),
                    None => Ok(ToolResult::failure(format!("Edge {} not found", edge_id))),
                }
            }

            "list_nodes" => {
                let node_type = args["node_type"].as_str().map(NodeType::from_str);
                let limit = args["limit"].as_i64().or(Some(100));
                let session_id = session_id.to_string();

                let result = tokio::task::spawn_blocking(move || {
                    persistence.list_graph_nodes(&session_id, node_type, limit)
                })
                .await
                .context("task join error")??;

                Ok(ToolResult::success(
                    json!({
                        "count": result.len(),
                        "nodes": result
                    })
                    .to_string(),
                ))
            }

            "list_edges" => {
                let source_id = args["source_id"].as_i64();
                let target_id = args["target_id"].as_i64();
                let session_id = session_id.to_string();

                let result = tokio::task::spawn_blocking(move || {
                    persistence.list_graph_edges(&session_id, source_id, target_id)
                })
                .await
                .context("task join error")??;

                Ok(ToolResult::success(
                    json!({
                        "count": result.len(),
                        "edges": result
                    })
                    .to_string(),
                ))
            }

            "delete_node" => {
                let node_id = args["node_id"]
                    .as_i64()
                    .context("node_id is required for delete_node")?;

                tokio::task::spawn_blocking(move || persistence.delete_graph_node(node_id))
                    .await
                    .context("task join error")??;

                Ok(ToolResult::success(format!("Deleted node {}", node_id)))
            }

            "delete_edge" => {
                let edge_id = args["edge_id"]
                    .as_i64()
                    .context("edge_id is required for delete_edge")?;

                tokio::task::spawn_blocking(move || persistence.delete_graph_edge(edge_id))
                    .await
                    .context("task join error")??;

                Ok(ToolResult::success(format!("Deleted edge {}", edge_id)))
            }

            "update_node" => {
                let node_id = args["node_id"]
                    .as_i64()
                    .context("node_id is required for update_node")?;
                let properties = args["properties"].clone();

                tokio::task::spawn_blocking(move || {
                    persistence.update_graph_node(node_id, &properties)
                })
                .await
                .context("task join error")??;

                Ok(ToolResult::success(format!("Updated node {}", node_id)))
            }

            "find_path" => {
                let source_id = args["source_id"]
                    .as_i64()
                    .context("source_id is required for find_path")?;
                let target_id = args["target_id"]
                    .as_i64()
                    .context("target_id is required for find_path")?;
                let max_hops = args["max_hops"].as_u64().map(|h| h as usize);
                let session_id = session_id.to_string();

                let result = tokio::task::spawn_blocking(move || {
                    persistence.find_shortest_path(&session_id, source_id, target_id, max_hops)
                })
                .await
                .context("task join error")??;

                match result {
                    Some(path) => Ok(ToolResult::success(
                        json!({
                            "found": true,
                            "length": path.length,
                            "total_weight": path.weight,
                            "path": path
                        })
                        .to_string(),
                    )),
                    None => Ok(ToolResult::success(
                        json!({
                            "found": false,
                            "message": format!("No path found from {} to {}", source_id, target_id)
                        })
                        .to_string(),
                    )),
                }
            }

            "traverse_neighbors" => {
                let node_id = args["node_id"]
                    .as_i64()
                    .context("node_id is required for traverse_neighbors")?;
                let depth = args["depth"].as_u64().unwrap_or(1) as usize;
                let direction = args["direction"]
                    .as_str()
                    .map(|d| match d {
                        "incoming" => TraversalDirection::Incoming,
                        "both" => TraversalDirection::Both,
                        _ => TraversalDirection::Outgoing,
                    })
                    .unwrap_or(TraversalDirection::Outgoing);
                let session_id = session_id.to_string();

                let result = tokio::task::spawn_blocking(move || {
                    persistence.traverse_neighbors(&session_id, node_id, direction, depth)
                })
                .await
                .context("task join error")??;

                Ok(ToolResult::success(
                    json!({
                        "count": result.len(),
                        "neighbors": result
                    })
                    .to_string(),
                ))
            }

            _ => Ok(ToolResult::failure(format!(
                "Unknown operation: {}",
                operation
            ))),
        }
    }
}
