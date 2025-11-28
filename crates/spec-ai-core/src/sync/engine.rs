use super::protocol::{GraphSyncPayload, SyncType, SyncedEdge, SyncedNode, Tombstone};
use super::{ConflictResolution, ConflictResolver, VectorClock};
use crate::persistence::{ChangelogEntry, Persistence, SyncedEdgeRecord, SyncedNodeRecord};
use anyhow::Result;

/// Threshold for deciding between full and incremental sync
/// If more than this percentage of nodes changed, do a full sync
const INCREMENTAL_THRESHOLD: f32 = 0.3; // 30%

/// Graph synchronization engine with adaptive strategy
pub struct SyncEngine {
    persistence: Persistence,
    instance_id: String,
    resolver: ConflictResolver,
}

#[derive(Debug, Clone)]
pub struct SyncStats {
    pub nodes_sent: usize,
    pub edges_sent: usize,
    pub tombstones_sent: usize,
    pub nodes_applied: usize,
    pub edges_applied: usize,
    pub tombstones_applied: usize,
    pub conflicts_detected: usize,
    pub conflicts_resolved: usize,
    pub sync_type: String,
}

impl SyncEngine {
    pub fn new(persistence: Persistence, instance_id: String) -> Self {
        Self {
            persistence,
            instance_id: instance_id.clone(),
            resolver: ConflictResolver::new(instance_id),
        }
    }

    /// Decide whether to use full or incremental sync based on changelog size
    pub async fn decide_sync_strategy(
        &self,
        session_id: &str,
        graph_name: &str,
        their_vector_clock: &VectorClock,
    ) -> Result<SyncType> {
        // Get our current vector clock
        let our_vc_str = self
            .persistence
            .graph_sync_state_get(&self.instance_id, session_id, graph_name)?
            .unwrap_or_else(|| "{}".to_string());
        let our_vc = VectorClock::from_json(&our_vc_str)?;

        // If they're way behind or we have no common history, do full sync
        if their_vector_clock.is_empty() || our_vc.is_empty() {
            return Ok(SyncType::Full);
        }

        // Count total nodes in the graph
        let total_nodes = self.persistence.count_graph_nodes(session_id)?;

        if total_nodes == 0 {
            return Ok(SyncType::Full);
        }

        // Estimate changed nodes by checking changelog
        // This is an approximation - in production you'd want a more precise count
        let since_timestamp = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::hours(24))
            .unwrap()
            .to_rfc3339();

        let changelog_entries = self
            .persistence
            .graph_changelog_get_since(session_id, &since_timestamp)?;

        // Calculate change ratio
        let changed_count = changelog_entries.len();
        let change_ratio = (changed_count as f32) / (total_nodes as f32);

        if change_ratio > INCREMENTAL_THRESHOLD {
            Ok(SyncType::Full)
        } else {
            Ok(SyncType::Incremental)
        }
    }

    /// Perform a full graph sync - send entire graph
    pub async fn sync_full(&self, session_id: &str, graph_name: &str) -> Result<GraphSyncPayload> {
        // Get all synced nodes and edges
        let nodes = self
            .persistence
            .graph_list_nodes_with_sync(session_id, true, false)?;
        let edges = self
            .persistence
            .graph_list_edges_with_sync(session_id, true, false)?;

        // Get our current vector clock
        let vc_str = self
            .persistence
            .graph_sync_state_get(&self.instance_id, session_id, graph_name)?
            .unwrap_or_else(|| "{}".to_string());
        let vector_clock = VectorClock::from_json(&vc_str)?;

        // Convert to sync protocol types
        let synced_nodes: Vec<SyncedNode> = nodes
            .into_iter()
            .map(|n| self.node_record_to_synced(n))
            .collect();
        let synced_edges: Vec<SyncedEdge> = edges
            .into_iter()
            .map(|e| self.edge_record_to_synced(e))
            .collect();

        Ok(GraphSyncPayload::response_full(
            session_id.to_string(),
            Some(graph_name.to_string()),
            vector_clock,
            synced_nodes,
            synced_edges,
            Vec::new(), // No tombstones in full sync
            None,
        ))
    }

    /// Perform incremental sync - send only changes since their vector clock
    pub async fn sync_incremental(
        &self,
        session_id: &str,
        graph_name: &str,
        their_vector_clock: &VectorClock,
    ) -> Result<GraphSyncPayload> {
        // Get our current vector clock
        let our_vc_str = self
            .persistence
            .graph_sync_state_get(&self.instance_id, session_id, graph_name)?
            .unwrap_or_else(|| "{}".to_string());
        let our_vector_clock = VectorClock::from_json(&our_vc_str)?;

        // Get changelog entries since their last sync
        // For simplicity, we'll get recent changes and filter by vector clock
        let since_timestamp = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(7))
            .unwrap()
            .to_rfc3339();

        let changelog = self
            .persistence
            .graph_changelog_get_since(session_id, &since_timestamp)?;

        // Filter changelog entries that happened after their vector clock
        let relevant_changes: Vec<&ChangelogEntry> = changelog
            .iter()
            .filter(|entry| {
                if let Ok(entry_vc) = VectorClock::from_json(&entry.vector_clock) {
                    their_vector_clock.happens_before(&entry_vc)
                        || their_vector_clock.is_concurrent(&entry_vc)
                } else {
                    false
                }
            })
            .collect();

        // Group by entity type and ID
        let mut node_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut edge_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut tombstones: Vec<Tombstone> = Vec::new();

        for entry in relevant_changes {
            match entry.entity_type.as_str() {
                "node" => {
                    if entry.operation == "delete" {
                        let vc = VectorClock::from_json(&entry.vector_clock)?;
                        tombstones.push(Tombstone::new(
                            "node".to_string(),
                            entry.entity_id,
                            vc,
                            entry.instance_id.clone(),
                        ));
                    } else {
                        node_ids.insert(entry.entity_id);
                    }
                }
                "edge" => {
                    if entry.operation == "delete" {
                        let vc = VectorClock::from_json(&entry.vector_clock)?;
                        tombstones.push(Tombstone::new(
                            "edge".to_string(),
                            entry.entity_id,
                            vc,
                            entry.instance_id.clone(),
                        ));
                    } else {
                        edge_ids.insert(entry.entity_id);
                    }
                }
                _ => {}
            }
        }

        // Fetch full entities for changed nodes/edges
        let mut synced_nodes = Vec::new();
        for node_id in node_ids {
            if let Some(node) = self.persistence.graph_get_node_with_sync(node_id)? {
                if node.sync_enabled && !node.is_deleted {
                    synced_nodes.push(self.node_record_to_synced(node));
                }
            }
        }

        let mut synced_edges = Vec::new();
        for edge_id in edge_ids {
            if let Some(edge) = self.persistence.graph_get_edge_with_sync(edge_id)? {
                if edge.sync_enabled && !edge.is_deleted {
                    synced_edges.push(self.edge_record_to_synced(edge));
                }
            }
        }

        Ok(GraphSyncPayload::response_incremental(
            session_id.to_string(),
            Some(graph_name.to_string()),
            our_vector_clock,
            synced_nodes,
            synced_edges,
            tombstones,
            None,
        ))
    }

    /// Apply incoming sync payload to local graph
    pub async fn apply_sync(
        &self,
        payload: &GraphSyncPayload,
        graph_name: &str,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats {
            nodes_sent: 0,
            edges_sent: 0,
            tombstones_sent: 0,
            nodes_applied: 0,
            edges_applied: 0,
            tombstones_applied: 0,
            conflicts_detected: 0,
            conflicts_resolved: 0,
            sync_type: format!("{:?}", payload.sync_type),
        };

        // Get our current vector clock
        let our_vc_str = self
            .persistence
            .graph_sync_state_get(&self.instance_id, &payload.session_id, graph_name)?
            .unwrap_or_else(|| "{}".to_string());
        let mut our_vector_clock = VectorClock::from_json(&our_vc_str)?;

        // Apply nodes
        for node in &payload.nodes {
            match self.apply_synced_node(node, &mut our_vector_clock).await {
                Ok(applied) => {
                    if applied {
                        stats.nodes_applied += 1;
                    }
                }
                Err(e) if e.to_string().contains("conflict") => {
                    stats.conflicts_detected += 1;
                    // Get existing node for conflict resolution
                    let existing_node = self
                        .persistence
                        .graph_get_node_with_sync(node.id)?
                        .map(|n| self.node_record_to_synced(n));

                    // Try to resolve conflict
                    match self.resolver.resolve_node_conflict(
                        node,
                        existing_node.as_ref(),
                        &mut our_vector_clock,
                    ) {
                        Ok(ConflictResolution::AcceptRemote) => {
                            // Apply the remote version
                            self.update_node_from_synced(node)?;
                            stats.conflicts_resolved += 1;
                            stats.nodes_applied += 1;
                        }
                        Ok(ConflictResolution::KeepLocal) => {
                            // Keep our version, no action needed
                            stats.conflicts_resolved += 1;
                        }
                        Ok(ConflictResolution::Merged(merged_value)) => {
                            // Apply the merged version
                            if let Ok(merged_node) =
                                serde_json::from_value::<SyncedNode>(merged_value)
                            {
                                self.update_node_from_synced(&merged_node)?;
                                stats.conflicts_resolved += 1;
                                stats.nodes_applied += 1;
                            }
                        }
                        Ok(ConflictResolution::RequiresManualReview) => {
                            tracing::warn!("Node {} conflict requires manual review", node.id);
                            // Don't count as resolved
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to resolve conflict for node {}: {}",
                                node.id,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to apply node {}: {}", node.id, e);
                }
            }
        }

        // Apply edges
        for edge in &payload.edges {
            match self.apply_synced_edge(edge, &mut our_vector_clock).await {
                Ok(applied) => {
                    if applied {
                        stats.edges_applied += 1;
                    }
                }
                Err(e) if e.to_string().contains("conflict") => {
                    stats.conflicts_detected += 1;
                    // Get existing edge for conflict resolution
                    let existing_edge = self
                        .persistence
                        .graph_get_edge_with_sync(edge.id)?
                        .map(|e| self.edge_record_to_synced(e));

                    // Try to resolve conflict
                    match self.resolver.resolve_edge_conflict(
                        edge,
                        existing_edge.as_ref(),
                        &mut our_vector_clock,
                    ) {
                        Ok(ConflictResolution::AcceptRemote) => {
                            // Apply the remote version
                            self.update_edge_from_synced(edge)?;
                            stats.conflicts_resolved += 1;
                            stats.edges_applied += 1;
                        }
                        Ok(ConflictResolution::KeepLocal) => {
                            // Keep our version, no action needed
                            stats.conflicts_resolved += 1;
                        }
                        Ok(ConflictResolution::Merged(merged_value)) => {
                            // Apply the merged version
                            if let Ok(merged_edge) =
                                serde_json::from_value::<SyncedEdge>(merged_value)
                            {
                                self.update_edge_from_synced(&merged_edge)?;
                                stats.conflicts_resolved += 1;
                                stats.edges_applied += 1;
                            }
                        }
                        Ok(ConflictResolution::RequiresManualReview) => {
                            tracing::warn!("Edge {} conflict requires manual review", edge.id);
                            // Don't count as resolved
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to resolve conflict for edge {}: {}",
                                edge.id,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to apply edge {}: {}", edge.id, e);
                }
            }
        }

        // Apply tombstones
        for tombstone in &payload.tombstones {
            match self.apply_tombstone(tombstone, &mut our_vector_clock).await {
                Ok(applied) => {
                    if applied {
                        stats.tombstones_applied += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to apply tombstone for {} {}: {}",
                        tombstone.entity_type,
                        tombstone.entity_id,
                        e
                    );
                }
            }
        }

        // Merge their vector clock into ours
        our_vector_clock.merge(&payload.vector_clock);

        // Update our sync state
        let updated_vc_str = our_vector_clock.to_json()?;
        self.persistence.graph_sync_state_update(
            &self.instance_id,
            &payload.session_id,
            graph_name,
            &updated_vc_str,
        )?;

        Ok(stats)
    }

    /// Apply a single synced node with conflict detection
    async fn apply_synced_node(
        &self,
        node: &SyncedNode,
        our_vector_clock: &mut VectorClock,
    ) -> Result<bool> {
        // Check if node exists locally
        let existing = self.persistence.graph_get_node_with_sync(node.id)?;

        if let Some(existing_node) = existing {
            // Node exists - check for conflicts
            let existing_vc = VectorClock::from_json(&existing_node.vector_clock)?;
            let incoming_vc = &node.vector_clock;

            match incoming_vc.compare(&existing_vc) {
                crate::sync::ClockOrder::After => {
                    // Incoming is newer, apply it
                    self.update_node_from_synced(node)?;
                    our_vector_clock.merge(incoming_vc);
                    Ok(true)
                }
                crate::sync::ClockOrder::Before | crate::sync::ClockOrder::Equal => {
                    // Our version is newer or equal, skip
                    Ok(false)
                }
                crate::sync::ClockOrder::Concurrent => {
                    // Conflict - let resolver handle it
                    anyhow::bail!("conflict detected for node {}", node.id);
                }
            }
        } else {
            // Node doesn't exist, insert it
            self.insert_node_from_synced(node)?;
            our_vector_clock.merge(&node.vector_clock);
            Ok(true)
        }
    }

    /// Apply a single synced edge with conflict detection
    async fn apply_synced_edge(
        &self,
        edge: &SyncedEdge,
        our_vector_clock: &mut VectorClock,
    ) -> Result<bool> {
        let existing = self.persistence.graph_get_edge_with_sync(edge.id)?;

        if let Some(existing_edge) = existing {
            let existing_vc = VectorClock::from_json(&existing_edge.vector_clock)?;
            let incoming_vc = &edge.vector_clock;

            match incoming_vc.compare(&existing_vc) {
                crate::sync::ClockOrder::After => {
                    self.update_edge_from_synced(edge)?;
                    our_vector_clock.merge(incoming_vc);
                    Ok(true)
                }
                crate::sync::ClockOrder::Before | crate::sync::ClockOrder::Equal => Ok(false),
                crate::sync::ClockOrder::Concurrent => {
                    anyhow::bail!("conflict detected for edge {}", edge.id);
                }
            }
        } else {
            self.insert_edge_from_synced(edge)?;
            our_vector_clock.merge(&edge.vector_clock);
            Ok(true)
        }
    }

    /// Apply a tombstone (deleted entity)
    async fn apply_tombstone(
        &self,
        tombstone: &Tombstone,
        our_vector_clock: &mut VectorClock,
    ) -> Result<bool> {
        let vc_str = tombstone.vector_clock.to_json()?;

        match tombstone.entity_type.as_str() {
            "node" => {
                self.persistence.graph_mark_node_deleted(
                    tombstone.entity_id,
                    &vc_str,
                    &tombstone.deleted_by,
                )?;
            }
            "edge" => {
                self.persistence.graph_mark_edge_deleted(
                    tombstone.entity_id,
                    &vc_str,
                    &tombstone.deleted_by,
                )?;
            }
            _ => {
                anyhow::bail!("unknown entity type: {}", tombstone.entity_type);
            }
        }

        our_vector_clock.merge(&tombstone.vector_clock);
        Ok(true)
    }

    // Helper methods for converting between record types

    fn node_record_to_synced(&self, record: SyncedNodeRecord) -> SyncedNode {
        use spec_ai_knowledge_graph::NodeType;
        SyncedNode {
            id: record.id,
            session_id: record.session_id,
            node_type: NodeType::from_str(&record.node_type),
            label: record.label,
            properties: record.properties,
            embedding_id: record.embedding_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
            vector_clock: VectorClock::from_json(&record.vector_clock).unwrap_or_default(),
            last_modified_by: record.last_modified_by,
            is_deleted: record.is_deleted,
            sync_enabled: record.sync_enabled,
        }
    }

    fn edge_record_to_synced(&self, record: SyncedEdgeRecord) -> SyncedEdge {
        use spec_ai_knowledge_graph::EdgeType;
        SyncedEdge {
            id: record.id,
            session_id: record.session_id,
            source_id: record.source_id,
            target_id: record.target_id,
            edge_type: EdgeType::from_str(&record.edge_type),
            predicate: record.predicate,
            properties: record.properties,
            weight: record.weight,
            temporal_start: record.temporal_start,
            temporal_end: record.temporal_end,
            created_at: record.created_at,
            vector_clock: VectorClock::from_json(&record.vector_clock).unwrap_or_default(),
            last_modified_by: record.last_modified_by,
            is_deleted: record.is_deleted,
            sync_enabled: record.sync_enabled,
        }
    }

    fn update_node_from_synced(&self, node: &SyncedNode) -> Result<()> {
        let vc_str = node.vector_clock.to_json()?;
        let last_modified = node.last_modified_by.as_deref().unwrap_or("unknown");

        self.persistence.graph_update_node_sync_metadata(
            node.id,
            &vc_str,
            last_modified,
            node.sync_enabled,
        )?;

        // Also update the node properties
        self.persistence
            .update_graph_node(node.id, &node.properties)?;

        Ok(())
    }

    fn update_edge_from_synced(&self, edge: &SyncedEdge) -> Result<()> {
        let vc_str = edge.vector_clock.to_json()?;
        let last_modified = edge.last_modified_by.as_deref().unwrap_or("unknown");

        self.persistence.graph_update_edge_sync_metadata(
            edge.id,
            &vc_str,
            last_modified,
            edge.sync_enabled,
        )?;

        Ok(())
    }

    fn insert_node_from_synced(&self, node: &SyncedNode) -> Result<()> {
        // Insert the node first using knowledge-graph types
        let node_id = self.persistence.insert_graph_node(
            &node.session_id,
            node.node_type.clone(),
            &node.label,
            &node.properties,
            node.embedding_id,
        )?;

        // Then update its sync metadata
        let vc_str = node.vector_clock.to_json()?;
        let last_modified = node.last_modified_by.as_deref().unwrap_or("unknown");

        self.persistence.graph_update_node_sync_metadata(
            node_id,
            &vc_str,
            last_modified,
            node.sync_enabled,
        )?;

        Ok(())
    }

    fn insert_edge_from_synced(&self, edge: &SyncedEdge) -> Result<()> {
        // Insert the edge first using knowledge-graph types
        let edge_id = self.persistence.insert_graph_edge(
            &edge.session_id,
            edge.source_id,
            edge.target_id,
            edge.edge_type.clone(),
            edge.predicate.as_deref(),
            edge.properties.as_ref(),
            edge.weight,
        )?;

        // Then update its sync metadata
        let vc_str = edge.vector_clock.to_json()?;
        let last_modified = edge.last_modified_by.as_deref().unwrap_or("unknown");

        self.persistence.graph_update_edge_sync_metadata(
            edge_id,
            &vc_str,
            last_modified,
            edge.sync_enabled,
        )?;

        Ok(())
    }
}
