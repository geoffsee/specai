use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "system" => MessageRole::System,
            "assistant" => MessageRole::Assistant,
            _ => MessageRole::User,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: i64,
    pub session_id: String,
    pub node_type: NodeType,
    pub label: String,
    pub properties: serde_json::Value,
    pub embedding_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Entity,     // Person, place, thing
    Concept,    // Abstract ideas
    Fact,       // Statements or claims
    Message,    // Linked to messages table
    ToolResult, // Linked to tool_log
    Event,      // Temporal events
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::Entity => "entity",
            NodeType::Concept => "concept",
            NodeType::Fact => "fact",
            NodeType::Message => "message",
            NodeType::ToolResult => "tool_result",
            NodeType::Event => "event",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "entity" => NodeType::Entity,
            "concept" => NodeType::Concept,
            "fact" => NodeType::Fact,
            "message" => NodeType::Message,
            "tool_result" => NodeType::ToolResult,
            "event" => NodeType::Event,
            _ => NodeType::Entity,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: i64,
    pub session_id: String,
    pub source_id: i64,
    pub target_id: i64,
    pub edge_type: EdgeType,
    pub predicate: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub weight: f32,
    pub temporal_start: Option<DateTime<Utc>>,
    pub temporal_end: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    RelatesTo,
    CausedBy,
    PartOf,
    Mentions,
    FollowsFrom, // For conversation flow
    Uses,        // Tool usage
    Produces,    // Tool output
    DependsOn,   // Dependencies
    Custom(String),
}

impl EdgeType {
    pub fn as_str(&self) -> String {
        match self {
            EdgeType::RelatesTo => "RELATES_TO".to_string(),
            EdgeType::CausedBy => "CAUSED_BY".to_string(),
            EdgeType::PartOf => "PART_OF".to_string(),
            EdgeType::Mentions => "MENTIONS".to_string(),
            EdgeType::FollowsFrom => "FOLLOWS_FROM".to_string(),
            EdgeType::Uses => "USES".to_string(),
            EdgeType::Produces => "PRODUCES".to_string(),
            EdgeType::DependsOn => "DEPENDS_ON".to_string(),
            EdgeType::Custom(s) => s.clone(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "RELATES_TO" => EdgeType::RelatesTo,
            "CAUSED_BY" => EdgeType::CausedBy,
            "PART_OF" => EdgeType::PartOf,
            "MENTIONS" => EdgeType::Mentions,
            "FOLLOWS_FROM" => EdgeType::FollowsFrom,
            "USES" => EdgeType::Uses,
            "PRODUCES" => EdgeType::Produces,
            "DEPENDS_ON" => EdgeType::DependsOn,
            custom => EdgeType::Custom(custom.to_string()),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}
