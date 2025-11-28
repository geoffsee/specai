use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: i64,
    pub session_id: String,
    pub node_type: NodeType,
    pub label: String,
    pub properties: JsonValue,
    pub embedding_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Entity,
    Concept,
    Fact,
    Message,
    ToolResult,
    Event,
    Goal,
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
            NodeType::Goal => "goal",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "entity" => NodeType::Entity,
            "concept" => NodeType::Concept,
            "fact" => NodeType::Fact,
            "message" => NodeType::Message,
            "tool_result" => NodeType::ToolResult,
            "event" => NodeType::Event,
            "goal" => NodeType::Goal,
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
    pub properties: Option<JsonValue>,
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
    FollowsFrom,
    Uses,
    Produces,
    DependsOn,
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
            EdgeType::Custom(value) => value.clone(),
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_uppercase().as_str() {
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
    pub pattern: String,
    pub parameters: HashMap<String, JsonValue>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_type_round_trips() {
        let variants = [
            (NodeType::Entity, "entity"),
            (NodeType::Concept, "concept"),
            (NodeType::Fact, "fact"),
            (NodeType::Message, "message"),
            (NodeType::ToolResult, "tool_result"),
            (NodeType::Event, "event"),
            (NodeType::Goal, "goal"),
        ];

        for (variant, label) in variants {
            assert_eq!(variant.as_str(), label);
            assert_eq!(NodeType::from_str(label), variant);
        }

        // Unknown strings should fall back to Entity
        assert_eq!(NodeType::from_str("unknown"), NodeType::Entity);
    }

    #[test]
    fn edge_type_round_trips() {
        let variants = [
            (EdgeType::RelatesTo, "RELATES_TO"),
            (EdgeType::CausedBy, "CAUSED_BY"),
            (EdgeType::PartOf, "PART_OF"),
            (EdgeType::Mentions, "MENTIONS"),
            (EdgeType::FollowsFrom, "FOLLOWS_FROM"),
            (EdgeType::Uses, "USES"),
            (EdgeType::Produces, "PRODUCES"),
            (EdgeType::DependsOn, "DEPENDS_ON"),
        ];

        for (variant, label) in variants {
            assert_eq!(variant.as_str(), label);
            assert_eq!(EdgeType::from_str(label), variant);
        }

        // Custom types should be preserved verbatim
        if let EdgeType::Custom(value) = EdgeType::from_str("MY_EDGE") {
            assert_eq!(value, "MY_EDGE");
        } else {
            panic!("expected custom edge type");
        }
    }
}
