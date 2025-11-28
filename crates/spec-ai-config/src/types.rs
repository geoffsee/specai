use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Agent(String), // Agent with instance_id
}

impl MessageRole {
    pub fn as_str(&self) -> String {
        match self {
            MessageRole::System => "system".to_string(),
            MessageRole::User => "user".to_string(),
            MessageRole::Assistant => "assistant".to_string(),
            MessageRole::Agent(id) => format!("agent:{}", id),
        }
    }

    pub fn from_str(s: &str) -> Self {
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("agent:") {
            let id = s[6..].to_string();
            MessageRole::Agent(id)
        } else {
            match lower.as_str() {
                "system" => MessageRole::System,
                "assistant" => MessageRole::Assistant,
                _ => MessageRole::User,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryVector {
    pub id: i64,
    pub session_id: String,
    pub message_id: Option<i64>,
    pub embedding: Vec<f32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLog {
    pub id: i64,
    pub session_id: String,
    pub agent: String,
    pub run_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
    pub success: bool,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

// ========== Knowledge Graph Types ==========
// Re-exported from knowledge-graph crate for consolidation
pub use spec_ai_knowledge_graph::{NodeType, EdgeType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: i64,
    pub session_id: String,
    pub node_type: spec_ai_knowledge_graph::NodeType,
    pub label: String,
    pub properties: serde_json::Value,
    pub embedding_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: i64,
    pub session_id: String,
    pub source_id: i64,
    pub target_id: i64,
    pub edge_type: spec_ai_knowledge_graph::EdgeType,
    pub predicate: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub weight: f32,
    pub temporal_start: Option<DateTime<Utc>>,
    pub temporal_end: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    pub pattern: String, // SQL/PGQ pattern
    pub parameters: HashMap<String, serde_json::Value>,
    pub limit: Option<usize>,
    pub return_type: GraphQueryReturnType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphQueryReturnType {
    Nodes,
    Edges,
    Paths,
    Count,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPath {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub length: usize,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQueryResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub paths: Vec<GraphPath>,
    pub count: Option<usize>,
}

// Re-exported from knowledge-graph crate
pub use spec_ai_knowledge_graph::TraversalDirection;
