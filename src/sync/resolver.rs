use crate::api::sync::{SyncedEdge, SyncedNode};
use crate::sync::{ClockOrder, VectorClock};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Represents a detected conflict for audit trail
#[derive(Debug, Clone)]
pub struct ConflictRecord {
    pub node_id: String,
    pub conflict_type: ConflictType,
    pub local_version: JsonValue,
    pub remote_version: JsonValue,
    pub resolution: ConflictResolution,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ConflictType {
    VectorClockConcurrent,
    SemanticConflict(String),
    TypeMismatch,
    DeleteUpdate, // One side deleted, other updated
}

#[derive(Debug, Clone)]
pub enum ConflictResolution {
    AcceptRemote,
    KeepLocal,
    Merged(JsonValue),
    RequiresManualReview,
}

/// Conflict resolution strategies for graph synchronization
pub struct ConflictResolver {
    instance_id: String,
    conflict_log: std::sync::Arc<std::sync::Mutex<Vec<ConflictRecord>>>,
}

impl ConflictResolver {
    pub fn new(instance_id: String) -> Self {
        Self {
            instance_id,
            conflict_log: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Resolve a node conflict using vector clock merge and property reconciliation
    pub fn resolve_node_conflict(
        &self,
        incoming: &SyncedNode,
        our_node: Option<&SyncedNode>,
        our_vector_clock: &mut VectorClock,
    ) -> Result<ConflictResolution> {
        // Get incoming vector clock
        let incoming_vc = &incoming.vector_clock;

        // Determine clock ordering
        let clock_order = our_vector_clock.compare(&incoming_vc);

        debug!(
            "Resolving node conflict for {}: clock_order = {:?}",
            incoming.id, clock_order
        );

        let resolution = match clock_order {
            ClockOrder::Before => {
                // Our version is older, accept incoming
                info!("Node {} - our version is older, accepting remote", incoming.id);
                our_vector_clock.merge(&incoming_vc);
                ConflictResolution::AcceptRemote
            }
            ClockOrder::After => {
                // Our version is newer, keep local
                info!("Node {} - our version is newer, keeping local", incoming.id);
                ConflictResolution::KeepLocal
            }
            ClockOrder::Equal => {
                // Same version, no conflict
                debug!("Node {} - versions are equal, no changes needed", incoming.id);
                ConflictResolution::KeepLocal
            }
            ClockOrder::Concurrent => {
                // True conflict - need to merge
                warn!("Node {} - concurrent modification detected", incoming.id);

                if let Some(local_node) = our_node {
                    // Detect semantic conflicts
                    let semantic_conflicts = self.detect_semantic_conflicts(local_node, incoming);

                    if !semantic_conflicts.is_empty() {
                        warn!(
                            "Semantic conflicts detected for node {}: {:?}",
                            incoming.id, semantic_conflicts
                        );
                    }

                    // Get timestamps (already DateTime<Utc>)
                    let local_ts = local_node.updated_at;
                    let remote_ts = incoming.updated_at;

                    // Apply type-specific merge strategy
                    let merged_properties = if incoming.node_type == local_node.node_type {
                        self.apply_type_specific_merge(
                            incoming.node_type.as_str(),
                            &local_node.properties,
                            &incoming.properties,
                        )
                    } else {
                        // Type mismatch - this is a serious conflict
                        warn!(
                            "Node type mismatch for {}: local={:?}, remote={:?}",
                            incoming.id, local_node.node_type, incoming.node_type
                        );

                        // Record for manual review
                        self.record_conflict(ConflictRecord {
                            node_id: incoming.id.to_string(),
                            conflict_type: ConflictType::TypeMismatch,
                            local_version: serde_json::to_value(local_node)?,
                            remote_version: serde_json::to_value(incoming)?,
                            resolution: ConflictResolution::RequiresManualReview,
                            timestamp: Utc::now(),
                        });

                        return Ok(ConflictResolution::RequiresManualReview);
                    };

                    // Choose label based on timestamps
                    let merged_label = if remote_ts > local_ts {
                        incoming.label.clone()
                    } else {
                        local_node.label.clone()
                    };

                    // Merge vector clocks
                    our_vector_clock.merge(&incoming_vc);
                    our_vector_clock.increment(&self.instance_id);

                    // Create merged node
                    let merged_node = json!({
                        "id": incoming.id,
                        "label": merged_label,
                        "node_type": incoming.node_type,
                        "properties": merged_properties,
                        "vector_clock": our_vector_clock.to_json()?,
                        "updated_at": Utc::now().to_rfc3339(),
                    });

                    // Record the conflict and resolution
                    self.record_conflict(ConflictRecord {
                        node_id: incoming.id.to_string(),
                        conflict_type: ConflictType::VectorClockConcurrent,
                        local_version: serde_json::to_value(local_node)?,
                        remote_version: serde_json::to_value(incoming)?,
                        resolution: ConflictResolution::Merged(merged_node.clone()),
                        timestamp: Utc::now(),
                    });

                    ConflictResolution::Merged(merged_node)
                } else {
                    // Node doesn't exist locally but we have a concurrent clock
                    // This could happen if node was deleted locally
                    warn!(
                        "Node {} exists remotely but not locally with concurrent clock",
                        incoming.id
                    );

                    // Check if we have a tombstone for this node
                    // For now, accept the remote version
                    our_vector_clock.merge(&incoming_vc);
                    ConflictResolution::AcceptRemote
                }
            }
        };

        Ok(resolution)
    }

    /// Resolve an edge conflict
    pub fn resolve_edge_conflict(
        &self,
        incoming: &SyncedEdge,
        our_edge: Option<&SyncedEdge>,
        our_vector_clock: &mut VectorClock,
    ) -> Result<ConflictResolution> {
        // Get incoming vector clock
        let incoming_vc = &incoming.vector_clock;

        // Determine clock ordering
        let clock_order = our_vector_clock.compare(&incoming_vc);

        debug!(
            "Resolving edge conflict for {}: clock_order = {:?}",
            incoming.id, clock_order
        );

        let resolution = match clock_order {
            ClockOrder::Before => {
                // Our version is older, accept incoming
                info!("Edge {} - our version is older, accepting remote", incoming.id);
                our_vector_clock.merge(&incoming_vc);
                ConflictResolution::AcceptRemote
            }
            ClockOrder::After => {
                // Our version is newer, keep local
                info!("Edge {} - our version is newer, keeping local", incoming.id);
                ConflictResolution::KeepLocal
            }
            ClockOrder::Equal => {
                // Same version, no conflict
                debug!("Edge {} - versions are equal, no changes needed", incoming.id);
                ConflictResolution::KeepLocal
            }
            ClockOrder::Concurrent => {
                // True conflict - need to merge
                warn!("Edge {} - concurrent modification detected", incoming.id);

                if let Some(local_edge) = our_edge {
                    // Get timestamps
                    let local_ts = local_edge.created_at;  // Edges don't have updated_at
                    let remote_ts = incoming.created_at;

                    // Merge properties based on timestamps
                    let empty_props = serde_json::json!({});
                    let local_props = local_edge.properties.as_ref().unwrap_or(&empty_props);
                    let remote_props = incoming.properties.as_ref().unwrap_or(&empty_props);
                    let merged_properties = self.merge_json_properties(
                        local_props,
                        remote_props,
                        local_ts,
                        remote_ts,
                    );

                    // Choose weight and predicate based on timestamps
                    let (merged_weight, merged_predicate) = if remote_ts > local_ts {
                        (incoming.weight, incoming.predicate.clone())
                    } else {
                        (local_edge.weight, local_edge.predicate.clone())
                    };

                    // Verify edge endpoints haven't changed
                    if local_edge.source_id != incoming.source_id || local_edge.target_id != incoming.target_id {
                        warn!(
                            "Edge {} endpoints changed - requires manual review",
                            incoming.id
                        );

                        self.record_conflict(ConflictRecord {
                            node_id: incoming.id.to_string(),
                            conflict_type: ConflictType::SemanticConflict(
                                "Edge endpoints mismatch".to_string()
                            ),
                            local_version: serde_json::to_value(local_edge)?,
                            remote_version: serde_json::to_value(incoming)?,
                            resolution: ConflictResolution::RequiresManualReview,
                            timestamp: Utc::now(),
                        });

                        return Ok(ConflictResolution::RequiresManualReview);
                    }

                    // Merge vector clocks
                    our_vector_clock.merge(&incoming_vc);
                    our_vector_clock.increment(&self.instance_id);

                    // Create merged edge
                    let merged_edge = json!({
                        "id": incoming.id,
                        "session_id": incoming.session_id,
                        "source_id": incoming.source_id,
                        "target_id": incoming.target_id,
                        "edge_type": incoming.edge_type,
                        "predicate": merged_predicate,
                        "weight": merged_weight,
                        "properties": Some(merged_properties),
                        "temporal_start": incoming.temporal_start,
                        "temporal_end": incoming.temporal_end,
                        "created_at": incoming.created_at,
                        "vector_clock": our_vector_clock,
                        "last_modified_by": incoming.last_modified_by,
                        "is_deleted": false,
                        "sync_enabled": true,
                    });

                    // Record the conflict and resolution
                    self.record_conflict(ConflictRecord {
                        node_id: incoming.id.to_string(),
                        conflict_type: ConflictType::VectorClockConcurrent,
                        local_version: serde_json::to_value(local_edge)?,
                        remote_version: serde_json::to_value(incoming)?,
                        resolution: ConflictResolution::Merged(merged_edge.clone()),
                        timestamp: Utc::now(),
                    });

                    ConflictResolution::Merged(merged_edge)
                } else {
                    // Edge doesn't exist locally but we have a concurrent clock
                    our_vector_clock.merge(&incoming_vc);
                    ConflictResolution::AcceptRemote
                }
            }
        };

        Ok(resolution)
    }

    /// Record a conflict for audit trail
    fn record_conflict(&self, record: ConflictRecord) {
        if let Ok(mut log) = self.conflict_log.lock() {
            log.push(record);
        }
    }

    /// Get all recorded conflicts
    pub fn get_conflict_log(&self) -> Vec<ConflictRecord> {
        self.conflict_log.lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    /// Clear the conflict log
    pub fn clear_conflict_log(&self) {
        if let Ok(mut log) = self.conflict_log.lock() {
            log.clear();
        }
    }

    /// Merge two JSON objects, preferring newer values based on timestamps
    #[allow(dead_code)]
    pub fn merge_json_properties(
        &self,
        local: &JsonValue,
        remote: &JsonValue,
        local_timestamp: chrono::DateTime<chrono::Utc>,
        remote_timestamp: chrono::DateTime<chrono::Utc>,
    ) -> JsonValue {
        match (local, remote) {
            (JsonValue::Object(local_map), JsonValue::Object(remote_map)) => {
                let mut merged = serde_json::Map::new();

                // Start with local properties
                for (key, value) in local_map {
                    merged.insert(key.clone(), value.clone());
                }

                // Merge remote properties
                for (key, remote_value) in remote_map {
                    if let Some(local_value) = local_map.get(key) {
                        // Key exists in both - recursively merge or use timestamp
                        if local_value.is_object() && remote_value.is_object() {
                            merged.insert(
                                key.clone(),
                                self.merge_json_properties(
                                    local_value,
                                    remote_value,
                                    local_timestamp,
                                    remote_timestamp,
                                ),
                            );
                        } else {
                            // Use timestamp to decide
                            if remote_timestamp > local_timestamp {
                                merged.insert(key.clone(), remote_value.clone());
                            }
                        }
                    } else {
                        // Key only in remote, add it
                        merged.insert(key.clone(), remote_value.clone());
                    }
                }

                JsonValue::Object(merged)
            }
            (JsonValue::Array(local_arr), JsonValue::Array(remote_arr)) => {
                // For arrays, merge and deduplicate
                let mut merged_arr = local_arr.clone();
                for item in remote_arr {
                    if !merged_arr.contains(item) {
                        merged_arr.push(item.clone());
                    }
                }
                JsonValue::Array(merged_arr)
            }
            _ => {
                // For scalar values, use timestamp
                if remote_timestamp > local_timestamp {
                    remote.clone()
                } else {
                    local.clone()
                }
            }
        }
    }

    /// Detect semantic conflicts (application-specific logic)
    #[allow(dead_code)]
    pub fn detect_semantic_conflicts(
        &self,
        local: &SyncedNode,
        remote: &SyncedNode,
    ) -> Vec<String> {
        let mut conflicts = Vec::new();

        // Example: Check if critical fields differ
        if local.label != remote.label {
            conflicts.push(format!(
                "Label mismatch: '{}' vs '{}'",
                local.label, remote.label
            ));
        }

        if local.node_type != remote.node_type {
            conflicts.push(format!(
                "Node type mismatch: {:?} vs {:?}",
                local.node_type, remote.node_type
            ));
        }

        conflicts
    }

    /// Apply a merge strategy based on node type
    #[allow(dead_code)]
    pub fn apply_type_specific_merge(
        &self,
        node_type: &str,
        local: &JsonValue,
        remote: &JsonValue,
    ) -> JsonValue {
        // Application-specific merge rules
        match node_type {
            "entity" => {
                // For entities, merge properties but preserve local identifiers
                self.merge_preserving_keys(local, remote, &["id", "created_by"])
            }
            "concept" => {
                // For concepts, prefer remote definitions
                remote.clone()
            }
            "fact" => {
                // For facts, combine evidence from both
                self.merge_combining_arrays(local, remote, &["evidence", "sources"])
            }
            _ => {
                // Default: prefer newer (remote in conflict scenarios)
                remote.clone()
            }
        }
    }

    fn merge_preserving_keys(
        &self,
        local: &JsonValue,
        remote: &JsonValue,
        preserve_keys: &[&str],
    ) -> JsonValue {
        if let (JsonValue::Object(local_map), JsonValue::Object(remote_map)) = (local, remote) {
            let mut merged = remote_map.clone();
            for key in preserve_keys {
                if let Some(value) = local_map.get(*key) {
                    merged.insert(key.to_string(), value.clone());
                }
            }
            JsonValue::Object(merged)
        } else {
            remote.clone()
        }
    }

    fn merge_combining_arrays(
        &self,
        local: &JsonValue,
        remote: &JsonValue,
        array_keys: &[&str],
    ) -> JsonValue {
        if let (JsonValue::Object(local_map), JsonValue::Object(remote_map)) = (local, remote) {
            let mut merged = local_map.clone();

            for (key, remote_value) in remote_map {
                if array_keys.contains(&key.as_str()) {
                    // Combine arrays
                    if let Some(JsonValue::Array(local_arr)) = merged.get(key) {
                        if let JsonValue::Array(remote_arr) = remote_value {
                            let mut combined = local_arr.clone();
                            for item in remote_arr {
                                if !combined.contains(item) {
                                    combined.push(item.clone());
                                }
                            }
                            merged.insert(key.clone(), JsonValue::Array(combined));
                        }
                    } else {
                        merged.insert(key.clone(), remote_value.clone());
                    }
                } else {
                    // Overwrite with remote value
                    merged.insert(key.clone(), remote_value.clone());
                }
            }

            JsonValue::Object(merged)
        } else {
            remote.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_json_objects() {
        let resolver = ConflictResolver::new("test-instance".to_string());

        let local = json!({
            "name": "Alice",
            "age": 30,
            "city": "NYC"
        });

        let remote = json!({
            "name": "Alice",
            "age": 31,
            "country": "USA"
        });

        let local_time = chrono::Utc::now();
        let remote_time = local_time + chrono::Duration::seconds(10);

        let merged = resolver.merge_json_properties(&local, &remote, local_time, remote_time);

        assert_eq!(merged["name"], "Alice");
        assert_eq!(merged["age"], 31); // Remote is newer
        assert_eq!(merged["city"], "NYC"); // Preserved from local
        assert_eq!(merged["country"], "USA"); // Added from remote
    }

    #[test]
    fn test_merge_arrays() {
        let resolver = ConflictResolver::new("test-instance".to_string());

        let local = json!(["a", "b", "c"]);
        let remote = json!(["b", "c", "d"]);

        let local_time = chrono::Utc::now();
        let remote_time = local_time + chrono::Duration::seconds(10);

        let merged = resolver.merge_json_properties(&local, &remote, local_time, remote_time);

        if let JsonValue::Array(arr) = merged {
            assert!(arr.contains(&json!("a")));
            assert!(arr.contains(&json!("b")));
            assert!(arr.contains(&json!("c")));
            assert!(arr.contains(&json!("d")));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_preserve_keys() {
        let resolver = ConflictResolver::new("test-instance".to_string());

        let local = json!({
            "id": "123",
            "name": "Local",
            "created_by": "user1"
        });

        let remote = json!({
            "id": "456",
            "name": "Remote",
            "created_by": "user2"
        });

        let merged = resolver.merge_preserving_keys(&local, &remote, &["id", "created_by"]);

        assert_eq!(merged["id"], "123"); // Preserved
        assert_eq!(merged["name"], "Remote"); // From remote
        assert_eq!(merged["created_by"], "user1"); // Preserved
    }
}
