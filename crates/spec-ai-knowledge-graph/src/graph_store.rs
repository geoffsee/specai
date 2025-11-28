use crate::types::{EdgeType, GraphEdge, GraphNode, GraphPath, NodeType, TraversalDirection};
use crate::vector_clock::VectorClock;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use duckdb::{params, Connection};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct KnowledgeGraphStore {
    conn: Arc<Mutex<Connection>>,
    instance_id: String,
}

impl KnowledgeGraphStore {
    pub fn new(conn: Arc<Mutex<Connection>>, instance_id: impl Into<String>) -> Self {
        Self {
            conn,
            instance_id: instance_id.into(),
        }
    }

    pub fn from_connection(conn: Connection, instance_id: impl Into<String>) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            instance_id: instance_id.into(),
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("database connection poisoned")
    }

    // ---------- Graph Node Operations ----------

    pub fn insert_graph_node(
        &self,
        session_id: &str,
        node_type: NodeType,
        label: &str,
        properties: &JsonValue,
        embedding_id: Option<i64>,
    ) -> Result<i64> {
        let sync_enabled = self
            .graph_get_sync_enabled(session_id, "default")
            .unwrap_or(false);

        let mut vector_clock = VectorClock::new();
        vector_clock.increment(&self.instance_id);
        let vc_json = vector_clock.to_json()?;

        let conn = self.conn();

        let mut stmt = conn.prepare(
            "INSERT INTO graph_nodes (session_id, node_type, label, properties, embedding_id,
                                     vector_clock, last_modified_by, sync_enabled)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )?;
        let id: i64 = stmt.query_row(
            params![
                session_id,
                node_type.as_str(),
                label,
                properties.to_string(),
                embedding_id,
                vc_json,
                self.instance_id,
                sync_enabled,
            ],
            |row| row.get(0),
        )?;

        if sync_enabled {
            let node_data = serde_json::json!({
                "id": id,
                "session_id": session_id,
                "node_type": node_type.as_str(),
                "label": label,
                "properties": properties,
                "embedding_id": embedding_id,
            });

            self.graph_changelog_append(
                session_id,
                &self.instance_id,
                "node",
                id,
                "create",
                &vc_json,
                Some(&node_data.to_string()),
            )?;
        }

        Ok(id)
    }

    pub fn get_graph_node(&self, node_id: i64) -> Result<Option<GraphNode>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, node_type, label, properties, embedding_id,
                    CAST(created_at AS TEXT), CAST(updated_at AS TEXT)
             FROM graph_nodes WHERE id = ?",
        )?;
        let mut rows = stmt.query(params![node_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_graph_node(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_graph_nodes(
        &self,
        session_id: &str,
        node_type: Option<NodeType>,
        limit: Option<i64>,
    ) -> Result<Vec<GraphNode>> {
        let conn = self.conn();

        let nodes = if let Some(nt) = node_type {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, node_type, label, properties, embedding_id,
                        CAST(created_at AS TEXT), CAST(updated_at AS TEXT)
                 FROM graph_nodes WHERE session_id = ? AND node_type = ?
                 ORDER BY id DESC LIMIT ?",
            )?;
            let query = stmt.query(params![session_id, nt.as_str(), limit.unwrap_or(100)])?;
            Self::collect_graph_nodes(query)?
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, node_type, label, properties, embedding_id,
                        CAST(created_at AS TEXT), CAST(updated_at AS TEXT)
                 FROM graph_nodes WHERE session_id = ?
                 ORDER BY id DESC LIMIT ?",
            )?;
            let query = stmt.query(params![session_id, limit.unwrap_or(100)])?;
            Self::collect_graph_nodes(query)?
        };

        Ok(nodes)
    }

    pub fn count_graph_nodes(&self, session_id: &str) -> Result<i64> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM graph_nodes WHERE session_id = ?")?;
        let count: i64 = stmt.query_row(params![session_id], |row| row.get(0))?;
        Ok(count)
    }

    pub fn update_graph_node(&self, node_id: i64, properties: &JsonValue) -> Result<()> {
        let conn = self.conn();

        let mut stmt = conn.prepare(
            "SELECT session_id, node_type, label, vector_clock, sync_enabled
             FROM graph_nodes WHERE id = ?",
        )?;

        let (session_id, node_type, label, current_vc_json, sync_enabled): (
            String,
            String,
            String,
            Option<String>,
            bool,
        ) = stmt.query_row(params![node_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4).unwrap_or(false),
            ))
        })?;

        let mut vector_clock = if let Some(vc_json) = current_vc_json {
            VectorClock::from_json(&vc_json).unwrap_or_else(|_| VectorClock::new())
        } else {
            VectorClock::new()
        };
        vector_clock.increment(&self.instance_id);
        let vc_json = vector_clock.to_json()?;

        conn.execute(
            "UPDATE graph_nodes
             SET properties = ?,
                 vector_clock = ?,
                 last_modified_by = ?,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = ?",
            params![properties.to_string(), vc_json, self.instance_id, node_id],
        )?;

        if sync_enabled {
            let node_data = serde_json::json!({
                "id": node_id,
                "session_id": session_id,
                "node_type": node_type,
                "label": label,
                "properties": properties,
            });

            self.graph_changelog_append(
                &session_id,
                &self.instance_id,
                "node",
                node_id,
                "update",
                &vc_json,
                Some(&node_data.to_string()),
            )?;
        }

        Ok(())
    }

    pub fn delete_graph_node(&self, node_id: i64) -> Result<()> {
        let conn = self.conn();

        let mut stmt = conn.prepare(
            "SELECT session_id, node_type, label, properties, vector_clock, sync_enabled
             FROM graph_nodes WHERE id = ?",
        )?;

        let result = stmt.query_row(params![node_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, bool>(5).unwrap_or(false),
            ))
        });

        if let Ok((session_id, node_type, label, properties, current_vc_json, sync_enabled)) =
            result
        {
            if sync_enabled {
                let mut vector_clock = if let Some(vc_json) = current_vc_json {
                    VectorClock::from_json(&vc_json).unwrap_or_else(|_| VectorClock::new())
                } else {
                    VectorClock::new()
                };
                vector_clock.increment(&self.instance_id);
                let vc_json = vector_clock.to_json()?;

                conn.execute(
                    "INSERT INTO graph_tombstones
                     (session_id, entity_type, entity_id, deleted_by, vector_clock)
                     VALUES (?, ?, ?, ?, ?)",
                    params![session_id, "node", node_id, self.instance_id, vc_json],
                )?;

                let node_data = serde_json::json!({
                    "id": node_id,
                    "session_id": session_id,
                    "node_type": node_type,
                    "label": label,
                    "properties": properties,
                });

                self.graph_changelog_append(
                    &session_id,
                    &self.instance_id,
                    "node",
                    node_id,
                    "delete",
                    &vc_json,
                    Some(&node_data.to_string()),
                )?;
            }
        }

        conn.execute("DELETE FROM graph_nodes WHERE id = ?", params![node_id])?;
        Ok(())
    }

    // ---------- Graph Edge Operations ----------

    pub fn insert_graph_edge(
        &self,
        session_id: &str,
        source_id: i64,
        target_id: i64,
        edge_type: EdgeType,
        predicate: Option<&str>,
        properties: Option<&JsonValue>,
        weight: f32,
    ) -> Result<i64> {
        let sync_enabled = self
            .graph_get_sync_enabled(session_id, "default")
            .unwrap_or(false);

        let mut vector_clock = VectorClock::new();
        vector_clock.increment(&self.instance_id);
        let vc_json = vector_clock.to_json()?;

        let conn = self.conn();

        let mut stmt = conn.prepare(
            "INSERT INTO graph_edges (session_id, source_id, target_id, edge_type, predicate, properties, weight,
                                     vector_clock, last_modified_by, sync_enabled)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )?;
        let props_str = properties.map(|p| p.to_string());
        let id: i64 = stmt.query_row(
            params![
                session_id,
                source_id,
                target_id,
                edge_type.as_str(),
                predicate,
                props_str,
                weight,
                vc_json,
                self.instance_id,
                sync_enabled,
            ],
            |row| row.get(0),
        )?;

        if sync_enabled {
            let edge_data = serde_json::json!({
                "id": id,
                "session_id": session_id,
                "source_id": source_id,
                "target_id": target_id,
                "edge_type": edge_type.as_str(),
                "predicate": predicate,
                "properties": properties,
                "weight": weight,
            });

            self.graph_changelog_append(
                session_id,
                &self.instance_id,
                "edge",
                id,
                "insert",
                &vc_json,
                Some(&edge_data.to_string()),
            )?;
        }

        Ok(id)
    }

    pub fn get_graph_edge(&self, edge_id: i64) -> Result<Option<GraphEdge>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                    CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT)
             FROM graph_edges WHERE id = ?",
        )?;
        let mut rows = stmt.query(params![edge_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_graph_edge(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_graph_edges(
        &self,
        session_id: &str,
        source_id: Option<i64>,
        target_id: Option<i64>,
    ) -> Result<Vec<GraphEdge>> {
        let conn = self.conn();

        let edges = match (source_id, target_id) {
            (Some(src), Some(tgt)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                            CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT)
                     FROM graph_edges WHERE session_id = ? AND source_id = ? AND target_id = ?",
                )?;
                let query = stmt.query(params![session_id, src, tgt])?;
                Self::collect_graph_edges(query)?
            }
            (Some(src), None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                            CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT)
                     FROM graph_edges WHERE session_id = ? AND source_id = ?",
                )?;
                let query = stmt.query(params![session_id, src])?;
                Self::collect_graph_edges(query)?
            }
            (None, Some(tgt)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                            CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT)
                     FROM graph_edges WHERE session_id = ? AND target_id = ?",
                )?;
                let query = stmt.query(params![session_id, tgt])?;
                Self::collect_graph_edges(query)?
            }
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                            CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT)
                     FROM graph_edges WHERE session_id = ?",
                )?;
                let query = stmt.query(params![session_id])?;
                Self::collect_graph_edges(query)?
            }
        };

        Ok(edges)
    }

    pub fn count_graph_edges(&self, session_id: &str) -> Result<i64> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM graph_edges WHERE session_id = ?")?;
        let count: i64 = stmt.query_row(params![session_id], |row| row.get(0))?;
        Ok(count)
    }

    pub fn delete_graph_edge(&self, edge_id: i64) -> Result<()> {
        let conn = self.conn();

        let mut stmt = conn.prepare(
            "SELECT session_id, source_id, target_id, edge_type, predicate, properties, weight,
                    vector_clock, sync_enabled
             FROM graph_edges WHERE id = ?",
        )?;

        let result = stmt.query_row(params![edge_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, f32>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, bool>(8).unwrap_or(false),
            ))
        });

        if let Ok((
            session_id,
            source_id,
            target_id,
            edge_type,
            predicate,
            properties,
            weight,
            current_vc_json,
            sync_enabled,
        )) = result
        {
            if sync_enabled {
                let mut vector_clock = if let Some(vc_json) = current_vc_json {
                    VectorClock::from_json(&vc_json).unwrap_or_else(|_| VectorClock::new())
                } else {
                    VectorClock::new()
                };
                vector_clock.increment(&self.instance_id);
                let vc_json = vector_clock.to_json()?;

                conn.execute(
                    "INSERT INTO graph_tombstones
                     (session_id, entity_type, entity_id, deleted_by, vector_clock)
                     VALUES (?, ?, ?, ?, ?)",
                    params![session_id, "edge", edge_id, self.instance_id, vc_json],
                )?;

                let edge_data = serde_json::json!({
                    "id": edge_id,
                    "session_id": session_id,
                    "source_id": source_id,
                    "target_id": target_id,
                    "edge_type": edge_type,
                    "predicate": predicate,
                    "properties": properties,
                    "weight": weight,
                });

                self.graph_changelog_append(
                    &session_id,
                    &self.instance_id,
                    "edge",
                    edge_id,
                    "delete",
                    &vc_json,
                    Some(&edge_data.to_string()),
                )?;
            }
        }

        conn.execute("DELETE FROM graph_edges WHERE id = ?", params![edge_id])?;
        Ok(())
    }

    // ---------- Graph Traversal Operations ----------

    pub fn find_shortest_path(
        &self,
        session_id: &str,
        source_id: i64,
        target_id: i64,
        max_hops: Option<usize>,
    ) -> Result<Option<GraphPath>> {
        let max_depth = max_hops.unwrap_or(10);

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent_map = HashMap::new();

        queue.push_back((source_id, 0));
        visited.insert(source_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            if current_id == target_id {
                let path = self.reconstruct_path(&parent_map, source_id, target_id)?;
                return Ok(Some(path));
            }

            if depth >= max_depth {
                continue;
            }

            let edges = self.list_graph_edges(session_id, Some(current_id), None)?;
            for edge in edges {
                let target = edge.target_id;
                if !visited.contains(&target) {
                    visited.insert(target);
                    parent_map.insert(target, (current_id, edge));
                    queue.push_back((target, depth + 1));
                }
            }
        }

        Ok(None)
    }

    pub fn traverse_neighbors(
        &self,
        session_id: &str,
        node_id: i64,
        direction: TraversalDirection,
        depth: usize,
    ) -> Result<Vec<GraphNode>> {
        if depth == 0 {
            return Ok(vec![]);
        }

        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back((node_id, 0));
        visited.insert(node_id);

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if current_depth > 0 {
                if let Some(node) = self.get_graph_node(current_id)? {
                    result.push(node);
                }
            }

            if current_depth >= depth {
                continue;
            }

            let edges = match direction {
                TraversalDirection::Outgoing => {
                    self.list_graph_edges(session_id, Some(current_id), None)?
                }
                TraversalDirection::Incoming => {
                    self.list_graph_edges(session_id, None, Some(current_id))?
                }
                TraversalDirection::Both => {
                    let mut out_edges =
                        self.list_graph_edges(session_id, Some(current_id), None)?;
                    let in_edges = self.list_graph_edges(session_id, None, Some(current_id))?;
                    out_edges.extend(in_edges);
                    out_edges
                }
            };

            for edge in edges {
                let next_id = match direction {
                    TraversalDirection::Outgoing => edge.target_id,
                    TraversalDirection::Incoming => edge.source_id,
                    TraversalDirection::Both => {
                        if edge.source_id == current_id {
                            edge.target_id
                        } else {
                            edge.source_id
                        }
                    }
                };

                if !visited.contains(&next_id) {
                    visited.insert(next_id);
                    queue.push_back((next_id, current_depth + 1));
                }
            }
        }

        Ok(result)
    }

    fn row_to_graph_node(row: &duckdb::Row) -> Result<GraphNode> {
        let id: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let node_type: String = row.get(2)?;
        let label: String = row.get(3)?;
        let properties: String = row.get(4)?;
        let embedding_id: Option<i64> = row.get(5)?;
        let created_at: String = row.get(6)?;
        let updated_at: String = row.get(7)?;

        Ok(GraphNode {
            id,
            session_id,
            node_type: NodeType::from_str(&node_type),
            label,
            properties: serde_json::from_str(&properties).unwrap_or(JsonValue::Null),
            embedding_id,
            created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: updated_at.parse().unwrap_or_else(|_| Utc::now()),
        })
    }

    fn row_to_graph_edge(row: &duckdb::Row) -> Result<GraphEdge> {
        let id: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let source_id: i64 = row.get(2)?;
        let target_id: i64 = row.get(3)?;
        let edge_type: String = row.get(4)?;
        let predicate: Option<String> = row.get(5)?;
        let properties: Option<String> = row.get(6)?;
        let weight: f32 = row.get(7)?;
        let temporal_start: Option<String> = row.get(8)?;
        let temporal_end: Option<String> = row.get(9)?;
        let created_at: String = row.get(10)?;

        Ok(GraphEdge {
            id,
            session_id,
            source_id,
            target_id,
            edge_type: EdgeType::from_str(&edge_type),
            predicate,
            properties: properties.and_then(|p| serde_json::from_str(&p).ok()),
            weight,
            temporal_start: temporal_start.and_then(|s| s.parse().ok()),
            temporal_end: temporal_end.and_then(|s| s.parse().ok()),
            created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
        })
    }

    fn collect_graph_nodes(mut rows: duckdb::Rows) -> Result<Vec<GraphNode>> {
        let mut nodes = Vec::new();
        while let Some(row) = rows.next()? {
            nodes.push(Self::row_to_graph_node(row)?);
        }
        Ok(nodes)
    }

    fn collect_graph_edges(mut rows: duckdb::Rows) -> Result<Vec<GraphEdge>> {
        let mut edges = Vec::new();
        while let Some(row) = rows.next()? {
            edges.push(Self::row_to_graph_edge(row)?);
        }
        Ok(edges)
    }

    fn reconstruct_path(
        &self,
        parent_map: &HashMap<i64, (i64, GraphEdge)>,
        source_id: i64,
        target_id: i64,
    ) -> Result<GraphPath> {
        let mut path_edges = Vec::new();
        let mut path_nodes = Vec::new();
        let mut current = target_id;
        let mut total_weight = 0.0;

        while current != source_id {
            if let Some((parent, edge)) = parent_map.get(&current) {
                path_edges.push(edge.clone());
                total_weight += edge.weight;
                current = *parent;
            } else {
                break;
            }
        }

        path_edges.reverse();

        if let Some(node) = self.get_graph_node(source_id)? {
            path_nodes.push(node);
        }
        for edge in &path_edges {
            if let Some(node) = self.get_graph_node(edge.target_id)? {
                path_nodes.push(node);
            }
        }

        Ok(GraphPath {
            length: path_edges.len(),
            weight: total_weight,
            nodes: path_nodes,
            edges: path_edges,
        })
    }

    // ===== Graph Synchronization Methods =====

    pub fn graph_changelog_append(
        &self,
        session_id: &str,
        instance_id: &str,
        entity_type: &str,
        entity_id: i64,
        operation: &str,
        vector_clock: &str,
        data: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO graph_changelog (session_id, instance_id, entity_type, entity_id, operation, vector_clock, data)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![session_id, instance_id, entity_type, entity_id, operation, vector_clock, data],
        )?;
        let id: i64 = conn.query_row("SELECT last_insert_rowid()", params![], |row| row.get(0))?;
        Ok(id)
    }

    pub fn graph_changelog_get_since(
        &self,
        session_id: &str,
        since_timestamp: &str,
    ) -> Result<Vec<ChangelogEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, instance_id, entity_type, entity_id, operation, vector_clock, data, CAST(created_at AS TEXT)
             FROM graph_changelog
             WHERE session_id = ? AND created_at > ?
             ORDER BY created_at ASC",
        )?;
        let mut rows = stmt.query(params![session_id, since_timestamp])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push(ChangelogEntry::from_row(row)?);
        }
        Ok(entries)
    }

    pub fn graph_changelog_prune(&self, days_to_keep: i64) -> Result<usize> {
        let conn = self.conn();
        let cutoff = Utc::now() - Duration::days(days_to_keep);
        let cutoff_str = cutoff.to_rfc3339();
        let deleted = conn.execute(
            "DELETE FROM graph_changelog WHERE created_at < ?",
            params![cutoff_str],
        )?;
        Ok(deleted)
    }

    pub fn graph_sync_state_get(
        &self,
        instance_id: &str,
        session_id: &str,
        graph_name: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn();
        let result: Result<String, _> = conn.query_row(
            "SELECT vector_clock FROM graph_sync_state WHERE instance_id = ? AND session_id = ? AND graph_name = ?",
            params![instance_id, session_id, graph_name],
            |row| row.get(0),
        );
        match result {
            Ok(vc) => Ok(Some(vc)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn graph_sync_state_update(
        &self,
        instance_id: &str,
        session_id: &str,
        graph_name: &str,
        vector_clock: &str,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute("BEGIN TRANSACTION", params![])?;
        conn.execute(
            "DELETE FROM graph_sync_state WHERE instance_id = ? AND session_id = ? AND graph_name = ?",
            params![instance_id, session_id, graph_name],
        )?;
        conn.execute(
            "INSERT INTO graph_sync_state (instance_id, session_id, graph_name, vector_clock) VALUES (?, ?, ?, ?)",
            params![instance_id, session_id, graph_name, vector_clock],
        )?;
        conn.execute("COMMIT", params![])?;
        Ok(())
    }

    pub fn graph_set_sync_enabled(
        &self,
        session_id: &str,
        graph_name: &str,
        enabled: bool,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE graph_metadata SET sync_enabled = ? WHERE session_id = ? AND graph_name = ?",
            params![enabled, session_id, graph_name],
        )?;
        Ok(())
    }

    pub fn graph_get_sync_enabled(&self, session_id: &str, graph_name: &str) -> Result<bool> {
        let conn = self.conn();
        let result: Result<bool, _> = conn.query_row(
            "SELECT sync_enabled FROM graph_metadata WHERE session_id = ? AND graph_name = ?",
            params![session_id, graph_name],
            |row| row.get(0),
        );
        match result {
            Ok(enabled) => Ok(enabled),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn graph_list(&self, session_id: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT graph_name FROM graph_metadata WHERE session_id = ?
             UNION
             SELECT DISTINCT 'default' as graph_name
             FROM graph_nodes WHERE session_id = ?
             ORDER BY graph_name",
        )?;

        let mut graphs = Vec::new();
        let mut rows = stmt.query(params![session_id, session_id])?;
        while let Some(row) = rows.next()? {
            let graph_name: String = row.get(0)?;
            graphs.push(graph_name);
        }

        if graphs.is_empty() {
            let node_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM graph_nodes WHERE session_id = ?",
                params![session_id],
                |row| row.get(0),
            )?;
            if node_count > 0 {
                graphs.push("default".to_string());
            }
        }

        Ok(graphs)
    }

    pub fn graph_get_node_with_sync(&self, node_id: i64) -> Result<Option<SyncedNodeRecord>> {
        let conn = self.conn();
        let result: Result<SyncedNodeRecord, _> = conn.query_row(
            "SELECT id, session_id, node_type, label, properties, embedding_id,
                    CAST(created_at AS TEXT), CAST(updated_at AS TEXT),
                    COALESCE(vector_clock, '{}'), last_modified_by, is_deleted, sync_enabled
             FROM graph_nodes WHERE id = ?",
            params![node_id],
            SyncedNodeRecord::from_row,
        );
        match result {
            Ok(node) => Ok(Some(node)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn graph_list_nodes_with_sync(
        &self,
        session_id: &str,
        sync_enabled_only: bool,
        include_deleted: bool,
    ) -> Result<Vec<SyncedNodeRecord>> {
        let conn = self.conn();
        let mut query = String::from(
            "SELECT id, session_id, node_type, label, properties, embedding_id,
                    CAST(created_at AS TEXT), CAST(updated_at AS TEXT),
                    COALESCE(vector_clock, '{}'), last_modified_by, is_deleted, sync_enabled
             FROM graph_nodes WHERE session_id = ?",
        );

        if sync_enabled_only {
            query.push_str(" AND sync_enabled = TRUE");
        }
        if !include_deleted {
            query.push_str(" AND is_deleted = FALSE");
        }
        query.push_str(" ORDER BY created_at ASC");

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(params![session_id])?;
        let mut nodes = Vec::new();
        while let Some(row) = rows.next()? {
            nodes.push(SyncedNodeRecord::from_row(row)?);
        }
        Ok(nodes)
    }

    pub fn graph_get_edge_with_sync(&self, edge_id: i64) -> Result<Option<SyncedEdgeRecord>> {
        let conn = self.conn();
        let result: Result<SyncedEdgeRecord, _> = conn.query_row(
            "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                    CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT),
                    COALESCE(vector_clock, '{}'), last_modified_by, is_deleted, sync_enabled
             FROM graph_edges WHERE id = ?",
            params![edge_id],
            SyncedEdgeRecord::from_row,
        );
        match result {
            Ok(edge) => Ok(Some(edge)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn graph_list_edges_with_sync(
        &self,
        session_id: &str,
        sync_enabled_only: bool,
        include_deleted: bool,
    ) -> Result<Vec<SyncedEdgeRecord>> {
        let conn = self.conn();
        let mut query = String::from(
            "SELECT id, session_id, source_id, target_id, edge_type, predicate, properties, weight,
                    CAST(temporal_start AS TEXT), CAST(temporal_end AS TEXT), CAST(created_at AS TEXT),
                    COALESCE(vector_clock, '{}'), last_modified_by, is_deleted, sync_enabled
             FROM graph_edges WHERE session_id = ?",
        );

        if sync_enabled_only {
            query.push_str(" AND sync_enabled = TRUE");
        }
        if !include_deleted {
            query.push_str(" AND is_deleted = FALSE");
        }
        query.push_str(" ORDER BY created_at ASC");

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(params![session_id])?;
        let mut edges = Vec::new();
        while let Some(row) = rows.next()? {
            edges.push(SyncedEdgeRecord::from_row(row)?);
        }
        Ok(edges)
    }

    pub fn graph_update_node_sync_metadata(
        &self,
        node_id: i64,
        vector_clock: &str,
        last_modified_by: &str,
        sync_enabled: bool,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE graph_nodes SET vector_clock = ?, last_modified_by = ?, sync_enabled = ?, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?",
            params![vector_clock, last_modified_by, sync_enabled, node_id],
        )?;
        Ok(())
    }

    pub fn graph_update_edge_sync_metadata(
        &self,
        edge_id: i64,
        vector_clock: &str,
        last_modified_by: &str,
        sync_enabled: bool,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE graph_edges SET vector_clock = ?, last_modified_by = ?, sync_enabled = ?
             WHERE id = ?",
            params![vector_clock, last_modified_by, sync_enabled, edge_id],
        )?;
        Ok(())
    }

    pub fn graph_mark_node_deleted(
        &self,
        node_id: i64,
        vector_clock: &str,
        deleted_by: &str,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE graph_nodes SET is_deleted = TRUE, vector_clock = ?, last_modified_by = ?, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?",
            params![vector_clock, deleted_by, node_id],
        )?;
        Ok(())
    }

    pub fn graph_mark_edge_deleted(
        &self,
        edge_id: i64,
        vector_clock: &str,
        deleted_by: &str,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE graph_edges SET is_deleted = TRUE, vector_clock = ?, last_modified_by = ?
             WHERE id = ?",
            params![vector_clock, deleted_by, edge_id],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ChangelogEntry {
    pub id: i64,
    pub session_id: String,
    pub instance_id: String,
    pub entity_type: String,
    pub entity_id: i64,
    pub operation: String,
    pub vector_clock: String,
    pub data: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ChangelogEntry {
    fn from_row(row: &duckdb::Row) -> Result<Self, duckdb::Error> {
        let id: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let instance_id: String = row.get(2)?;
        let entity_type: String = row.get(3)?;
        let entity_id: i64 = row.get(4)?;
        let operation: String = row.get(5)?;
        let vector_clock: String = row.get(6)?;
        let data: Option<String> = row.get(7)?;
        let created_at_str: String = row.get(8)?;

        Ok(ChangelogEntry {
            id,
            session_id,
            instance_id,
            entity_type,
            entity_id,
            operation,
            vector_clock,
            data,
            created_at: created_at_str.parse().unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyncedNodeRecord {
    pub id: i64,
    pub session_id: String,
    pub node_type: String,
    pub label: String,
    pub properties: JsonValue,
    pub embedding_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub vector_clock: String,
    pub last_modified_by: Option<String>,
    pub is_deleted: bool,
    pub sync_enabled: bool,
}

impl SyncedNodeRecord {
    fn from_row(row: &duckdb::Row) -> Result<Self, duckdb::Error> {
        let id: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let node_type: String = row.get(2)?;
        let label: String = row.get(3)?;
        let properties_str: String = row.get(4)?;
        let properties: JsonValue = serde_json::from_str(&properties_str).map_err(|e| {
            duckdb::Error::FromSqlConversionFailure(4, duckdb::types::Type::Text, Box::new(e))
        })?;
        let embedding_id: Option<i64> = row.get(5)?;
        let created_at_str: String = row.get(6)?;
        let updated_at_str: String = row.get(7)?;
        let vector_clock: String = row.get(8)?;
        let last_modified_by: Option<String> = row.get(9)?;
        let is_deleted: bool = row.get(10)?;
        let sync_enabled: bool = row.get(11)?;

        Ok(SyncedNodeRecord {
            id,
            session_id,
            node_type,
            label,
            properties,
            embedding_id,
            created_at: created_at_str.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: updated_at_str.parse().unwrap_or_else(|_| Utc::now()),
            vector_clock,
            last_modified_by,
            is_deleted,
            sync_enabled,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyncedEdgeRecord {
    pub id: i64,
    pub session_id: String,
    pub source_id: i64,
    pub target_id: i64,
    pub edge_type: String,
    pub predicate: Option<String>,
    pub properties: Option<JsonValue>,
    pub weight: f32,
    pub temporal_start: Option<DateTime<Utc>>,
    pub temporal_end: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub vector_clock: String,
    pub last_modified_by: Option<String>,
    pub is_deleted: bool,
    pub sync_enabled: bool,
}

impl SyncedEdgeRecord {
    fn from_row(row: &duckdb::Row) -> Result<Self, duckdb::Error> {
        let id: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let source_id: i64 = row.get(2)?;
        let target_id: i64 = row.get(3)?;
        let edge_type: String = row.get(4)?;
        let predicate: Option<String> = row.get(5)?;
        let properties_str: Option<String> = row.get(6)?;
        let properties: Option<JsonValue> = properties_str
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());
        let weight: f32 = row.get(7)?;
        let temporal_start_str: Option<String> = row.get(8)?;
        let temporal_end_str: Option<String> = row.get(9)?;
        let created_at_str: String = row.get(10)?;
        let vector_clock: String = row.get(11)?;
        let last_modified_by: Option<String> = row.get(12)?;
        let is_deleted: bool = row.get(13)?;
        let sync_enabled: bool = row.get(14)?;

        Ok(SyncedEdgeRecord {
            id,
            session_id,
            source_id,
            target_id,
            edge_type,
            predicate,
            properties,
            weight,
            temporal_start: temporal_start_str.and_then(|s| s.parse().ok()),
            temporal_end: temporal_end_str.and_then(|s| s.parse().ok()),
            created_at: created_at_str.parse().unwrap_or_else(|_| Utc::now()),
            vector_clock,
            last_modified_by,
            is_deleted,
            sync_enabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use serde_json::json;

    fn setup_store() -> KnowledgeGraphStore {
        setup_store_with(|_| {})
    }

    fn setup_store_with<F>(extra: F) -> KnowledgeGraphStore
    where
        F: FnOnce(&Connection),
    {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            r#"
            CREATE SEQUENCE IF NOT EXISTS graph_nodes_id_seq START 1;
            CREATE SEQUENCE IF NOT EXISTS graph_edges_id_seq START 1;
            CREATE SEQUENCE IF NOT EXISTS graph_metadata_id_seq START 1;
            CREATE SEQUENCE IF NOT EXISTS graph_changelog_id_seq START 1;
            CREATE SEQUENCE IF NOT EXISTS graph_tombstones_id_seq START 1;

            CREATE TABLE graph_nodes (
                id BIGINT PRIMARY KEY DEFAULT nextval('graph_nodes_id_seq'),
                session_id TEXT NOT NULL,
                node_type TEXT NOT NULL,
                label TEXT NOT NULL,
                properties TEXT NOT NULL,
                embedding_id BIGINT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                vector_clock TEXT DEFAULT '{}',
                last_modified_by TEXT,
                is_deleted BOOLEAN DEFAULT FALSE,
                sync_enabled BOOLEAN DEFAULT FALSE
            );

            CREATE TABLE graph_edges (
                id BIGINT PRIMARY KEY DEFAULT nextval('graph_edges_id_seq'),
                session_id TEXT NOT NULL,
                source_id BIGINT NOT NULL,
                target_id BIGINT NOT NULL,
                edge_type TEXT NOT NULL,
                predicate TEXT,
                properties TEXT,
                weight REAL DEFAULT 1.0,
                temporal_start TIMESTAMP,
                temporal_end TIMESTAMP,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                vector_clock TEXT DEFAULT '{}',
                last_modified_by TEXT,
                is_deleted BOOLEAN DEFAULT FALSE,
                sync_enabled BOOLEAN DEFAULT FALSE
            );

            CREATE TABLE graph_metadata (
                id BIGINT PRIMARY KEY DEFAULT nextval('graph_metadata_id_seq'),
                session_id TEXT NOT NULL,
                graph_name TEXT NOT NULL,
                sync_enabled BOOLEAN DEFAULT FALSE
            );

            CREATE TABLE graph_changelog (
                id BIGINT PRIMARY KEY DEFAULT nextval('graph_changelog_id_seq'),
                session_id TEXT NOT NULL,
                instance_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id BIGINT NOT NULL,
                operation TEXT NOT NULL,
                vector_clock TEXT NOT NULL,
                data TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE graph_sync_state (
                instance_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                graph_name TEXT NOT NULL,
                vector_clock TEXT NOT NULL,
                last_sync_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE graph_tombstones (
                id BIGINT PRIMARY KEY DEFAULT nextval('graph_tombstones_id_seq'),
                session_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id BIGINT NOT NULL,
                deleted_by TEXT NOT NULL,
                vector_clock TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .expect("create graph schema");

        extra(&conn);

        KnowledgeGraphStore::from_connection(conn, "test-instance")
    }

    #[test]
    fn insert_update_delete_node_flow() -> Result<()> {
        let store = setup_store();
        let props = json!({ "kind": "repository" });
        let node_id =
            store.insert_graph_node("session", NodeType::Entity, "SpecAI", &props, None)?;

        let nodes = store.list_graph_nodes("session", None, Some(10))?;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].label, "SpecAI");

        let updated_props = json!({ "kind": "repository", "stars": 42 });
        store.update_graph_node(node_id, &updated_props)?;
        let updated = store.get_graph_node(node_id)?.expect("node exists");
        assert_eq!(updated.properties["stars"], 42);

        store.delete_graph_node(node_id)?;
        assert!(store.get_graph_node(node_id)?.is_none());
        Ok(())
    }

    #[test]
    fn create_edges_and_find_paths() -> Result<()> {
        let store = setup_store();
        let a = store.insert_graph_node("session", NodeType::Entity, "A", &json!({}), None)?;
        let b = store.insert_graph_node("session", NodeType::Entity, "B", &json!({}), None)?;
        let c = store.insert_graph_node("session", NodeType::Entity, "C", &json!({}), None)?;

        store.insert_graph_edge("session", a, b, EdgeType::RelatesTo, None, None, 1.0)?;
        store.insert_graph_edge("session", b, c, EdgeType::RelatesTo, None, None, 1.0)?;

        let path = store
            .find_shortest_path("session", a, c, Some(5))?
            .expect("path exists");
        assert_eq!(path.nodes.len(), 3);
        assert_eq!(path.edges.len(), 2);
        assert_eq!(path.length, 2);
        assert_eq!(path.nodes.first().unwrap().label, "A");
        assert_eq!(path.nodes.last().unwrap().label, "C");

        let edges = store.list_graph_edges("session", None, None)?;
        assert_eq!(edges.len(), 2);

        Ok(())
    }

    #[test]
    fn traverse_neighbors_respects_direction() -> Result<()> {
        let store = setup_store();
        let alpha =
            store.insert_graph_node("session", NodeType::Entity, "Alpha", &json!({}), None)?;
        let beta =
            store.insert_graph_node("session", NodeType::Entity, "Beta", &json!({}), None)?;
        let gamma =
            store.insert_graph_node("session", NodeType::Entity, "Gamma", &json!({}), None)?;

        store.insert_graph_edge("session", alpha, beta, EdgeType::RelatesTo, None, None, 1.0)?;
        store.insert_graph_edge("session", beta, gamma, EdgeType::RelatesTo, None, None, 1.0)?;

        let outgoing =
            store.traverse_neighbors("session", alpha, TraversalDirection::Outgoing, 2)?;
        assert_eq!(outgoing.len(), 2);
        assert!(outgoing.iter().any(|node| node.label == "Beta"));
        assert!(outgoing.iter().any(|node| node.label == "Gamma"));

        let incoming =
            store.traverse_neighbors("session", gamma, TraversalDirection::Incoming, 2)?;
        assert_eq!(incoming.len(), 2);
        assert!(incoming.iter().any(|node| node.label == "Beta"));
        assert!(incoming.iter().any(|node| node.label == "Alpha"));

        Ok(())
    }
}
