use crate::sync::VectorClock;
use crate::types::{EdgeType, GraphEdge, GraphNode, NodeType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type of graph synchronization operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncType {
    /// Request complete graph snapshot
    RequestFull,
    /// Request incremental changes since given vector clock
    RequestIncremental,
    /// Full graph snapshot response
    Full,
    /// Incremental delta update response
    Incremental,
    /// Acknowledgment of received sync
    Ack,
    /// Conflict notification requiring resolution
    Conflict,
}

/// Main payload for MessageType::GraphSync messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSyncPayload {
    /// Type of sync operation
    pub sync_type: SyncType,
    /// Session ID for the graph being synced
    pub session_id: String,
    /// Graph name (from graph_metadata)
    pub graph_name: Option<String>,
    /// Vector clock representing the state of this sync
    pub vector_clock: VectorClock,
    /// Nodes to sync (empty for requests)
    #[serde(default)]
    pub nodes: Vec<SyncedNode>,
    /// Edges to sync (empty for requests)
    #[serde(default)]
    pub edges: Vec<SyncedEdge>,
    /// Tombstones for deleted entities
    #[serde(default)]
    pub tombstones: Vec<Tombstone>,
    /// Optional correlation ID for request/response matching
    pub correlation_id: Option<String>,
    /// For Conflict type: description of the conflict
    pub conflict_info: Option<String>,
}

/// Graph node with sync metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedNode {
    /// Core node data
    pub id: i64,
    pub session_id: String,
    pub node_type: NodeType,
    pub label: String,
    pub properties: serde_json::Value,
    pub embedding_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Sync metadata
    pub vector_clock: VectorClock,
    pub last_modified_by: Option<String>,
    pub is_deleted: bool,
    pub sync_enabled: bool,
}

/// Graph edge with sync metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedEdge {
    /// Core edge data
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

    /// Sync metadata
    pub vector_clock: VectorClock,
    pub last_modified_by: Option<String>,
    pub is_deleted: bool,
    pub sync_enabled: bool,
}

/// Tombstone for tracking deleted entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tombstone {
    /// Type of entity: 'node' or 'edge'
    pub entity_type: String,
    /// ID of the deleted entity
    pub entity_id: i64,
    /// Vector clock at time of deletion
    pub vector_clock: VectorClock,
    /// Instance that performed the deletion
    pub deleted_by: String,
    /// When the deletion occurred
    pub deleted_at: DateTime<Utc>,
}

/// Request for full graph sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFullRequest {
    pub session_id: String,
    pub graph_name: Option<String>,
    pub requesting_instance: String,
}

/// Request for incremental sync since a given vector clock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncIncrementalRequest {
    pub session_id: String,
    pub graph_name: Option<String>,
    pub requesting_instance: String,
    pub since_vector_clock: VectorClock,
}

/// Response containing graph data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub session_id: String,
    pub graph_name: Option<String>,
    pub vector_clock: VectorClock,
    pub nodes: Vec<SyncedNode>,
    pub edges: Vec<SyncedEdge>,
    pub tombstones: Vec<Tombstone>,
    pub is_incremental: bool,
}

/// Acknowledgment of successful sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAck {
    pub session_id: String,
    pub graph_name: Option<String>,
    pub vector_clock: VectorClock,
    pub nodes_applied: usize,
    pub edges_applied: usize,
    pub tombstones_applied: usize,
    pub conflicts_detected: usize,
}

/// Conflict notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    pub session_id: String,
    pub graph_name: Option<String>,
    pub entity_type: String,
    pub entity_id: i64,
    pub local_vector_clock: VectorClock,
    pub remote_vector_clock: VectorClock,
    pub description: String,
}

impl GraphSyncPayload {
    /// Create a full sync request
    pub fn request_full(
        session_id: String,
        graph_name: Option<String>,
        requesting_instance: String,
    ) -> Self {
        let mut vector_clock = VectorClock::new();
        vector_clock.increment(&requesting_instance);

        Self {
            sync_type: SyncType::RequestFull,
            session_id,
            graph_name,
            vector_clock,
            nodes: Vec::new(),
            edges: Vec::new(),
            tombstones: Vec::new(),
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            conflict_info: None,
        }
    }

    /// Create an incremental sync request
    pub fn request_incremental(
        session_id: String,
        graph_name: Option<String>,
        requesting_instance: String,
        since_vector_clock: VectorClock,
    ) -> Self {
        let mut vector_clock = since_vector_clock.clone();
        vector_clock.increment(&requesting_instance);

        Self {
            sync_type: SyncType::RequestIncremental,
            session_id,
            graph_name,
            vector_clock,
            nodes: Vec::new(),
            edges: Vec::new(),
            tombstones: Vec::new(),
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            conflict_info: None,
        }
    }

    /// Create a full sync response
    pub fn response_full(
        session_id: String,
        graph_name: Option<String>,
        vector_clock: VectorClock,
        nodes: Vec<SyncedNode>,
        edges: Vec<SyncedEdge>,
        tombstones: Vec<Tombstone>,
        correlation_id: Option<String>,
    ) -> Self {
        Self {
            sync_type: SyncType::Full,
            session_id,
            graph_name,
            vector_clock,
            nodes,
            edges,
            tombstones,
            correlation_id,
            conflict_info: None,
        }
    }

    /// Create an incremental sync response
    pub fn response_incremental(
        session_id: String,
        graph_name: Option<String>,
        vector_clock: VectorClock,
        nodes: Vec<SyncedNode>,
        edges: Vec<SyncedEdge>,
        tombstones: Vec<Tombstone>,
        correlation_id: Option<String>,
    ) -> Self {
        Self {
            sync_type: SyncType::Incremental,
            session_id,
            graph_name,
            vector_clock,
            nodes,
            edges,
            tombstones,
            correlation_id,
            conflict_info: None,
        }
    }

    /// Create an acknowledgment
    pub fn ack(
        session_id: String,
        graph_name: Option<String>,
        vector_clock: VectorClock,
        nodes_applied: usize,
        edges_applied: usize,
        tombstones_applied: usize,
        conflicts_detected: usize,
        correlation_id: Option<String>,
    ) -> Self {
        Self {
            sync_type: SyncType::Ack,
            session_id,
            graph_name,
            vector_clock,
            nodes: Vec::new(),
            edges: Vec::new(),
            tombstones: Vec::new(),
            correlation_id,
            conflict_info: Some(format!(
                "Applied {}/{}/{} (nodes/edges/tombstones), {} conflicts",
                nodes_applied, edges_applied, tombstones_applied, conflicts_detected
            )),
        }
    }

    /// Create a conflict notification
    pub fn conflict(
        session_id: String,
        graph_name: Option<String>,
        entity_type: String,
        entity_id: i64,
        local_vector_clock: VectorClock,
        remote_vector_clock: VectorClock,
        correlation_id: Option<String>,
    ) -> Self {
        Self {
            sync_type: SyncType::Conflict,
            session_id,
            graph_name,
            vector_clock: local_vector_clock.clone(),
            nodes: Vec::new(),
            edges: Vec::new(),
            tombstones: Vec::new(),
            correlation_id,
            conflict_info: Some(format!(
                "Conflict detected for {} {}: local={}, remote={}",
                entity_type, entity_id, local_vector_clock, remote_vector_clock
            )),
        }
    }
}

impl SyncedNode {
    /// Convert from GraphNode (without sync metadata)
    pub fn from_node(node: GraphNode, vector_clock: VectorClock, last_modified_by: Option<String>) -> Self {
        Self {
            id: node.id,
            session_id: node.session_id,
            node_type: node.node_type,
            label: node.label,
            properties: node.properties,
            embedding_id: node.embedding_id,
            created_at: node.created_at,
            updated_at: node.updated_at,
            vector_clock,
            last_modified_by,
            is_deleted: false,
            sync_enabled: false,
        }
    }

    /// Convert to GraphNode (strip sync metadata)
    pub fn to_node(&self) -> GraphNode {
        GraphNode {
            id: self.id,
            session_id: self.session_id.clone(),
            node_type: self.node_type.clone(),
            label: self.label.clone(),
            properties: self.properties.clone(),
            embedding_id: self.embedding_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl SyncedEdge {
    /// Convert from GraphEdge (without sync metadata)
    pub fn from_edge(edge: GraphEdge, vector_clock: VectorClock, last_modified_by: Option<String>) -> Self {
        Self {
            id: edge.id,
            session_id: edge.session_id,
            source_id: edge.source_id,
            target_id: edge.target_id,
            edge_type: edge.edge_type,
            predicate: edge.predicate,
            properties: edge.properties,
            weight: edge.weight,
            temporal_start: edge.temporal_start,
            temporal_end: edge.temporal_end,
            created_at: edge.created_at,
            vector_clock,
            last_modified_by,
            is_deleted: false,
            sync_enabled: false,
        }
    }

    /// Convert to GraphEdge (strip sync metadata)
    pub fn to_edge(&self) -> GraphEdge {
        GraphEdge {
            id: self.id,
            session_id: self.session_id.clone(),
            source_id: self.source_id,
            target_id: self.target_id,
            edge_type: self.edge_type.clone(),
            predicate: self.predicate.clone(),
            properties: self.properties.clone(),
            weight: self.weight,
            temporal_start: self.temporal_start,
            temporal_end: self.temporal_end,
            created_at: self.created_at,
        }
    }
}

impl Tombstone {
    /// Create a new tombstone for a deleted entity
    pub fn new(
        entity_type: String,
        entity_id: i64,
        vector_clock: VectorClock,
        deleted_by: String,
    ) -> Self {
        Self {
            entity_type,
            entity_id,
            vector_clock,
            deleted_by,
            deleted_at: Utc::now(),
        }
    }
}
