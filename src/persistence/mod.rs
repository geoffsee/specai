pub mod migrations;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::BaseDirs;
use duckdb::{params, Connection};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::types::{
    EdgeType, GraphEdge, GraphNode, GraphPath, MemoryVector, Message, MessageRole, NodeType,
    PolicyEntry, TraversalDirection,
};

#[derive(Clone)]
pub struct Persistence {
    conn: Arc<Mutex<Connection>>,
}

impl Persistence {
    /// Create or open the database at the provided path and run migrations.
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_path = expand_tilde(db_path.as_ref())?;
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir).context("creating DB directory")?;
        }
        let conn = Connection::open(&db_path).context("opening DuckDB")?;
        migrations::run(&conn).context("running migrations")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Checkpoint the database to ensure all WAL data is written to the main database file.
    /// Call this before shutdown to ensure clean database state.
    pub fn checkpoint(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch("CHECKPOINT;")
            .context("checkpointing database")
    }

    /// Creates or opens the default database at ~/.spec-ai/agent_data.duckdb
    pub fn new_default() -> Result<Self> {
        let base = BaseDirs::new().context("base directories not available")?;
        let path = base.home_dir().join(".agent_cli").join("agent_data.duckdb");
        Self::new(path)
    }

    /// Get access to the pooled database connection.
    /// Returns a MutexGuard that provides exclusive access to the connection.
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .expect("database connection mutex poisoned")
    }

    // ---------- Messages ----------

    pub fn insert_message(
        &self,
        session_id: &str,
        role: MessageRole,
        content: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "INSERT INTO messages (session_id, role, content) VALUES (?, ?, ?) RETURNING id",
        )?;
        let id: i64 = stmt.query_row(params![session_id, role.as_str(), content], |row| {
            row.get(0)
        })?;
        Ok(id)
    }

    pub fn list_messages(&self, session_id: &str, limit: i64) -> Result<Vec<Message>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, session_id, role, content, CAST(created_at AS TEXT) as created_at FROM messages WHERE session_id = ? ORDER BY id DESC LIMIT ?")?;
        let mut rows = stmt.query(params![session_id, limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let sid: String = row.get(1)?;
            let role: String = row.get(2)?;
            let content: String = row.get(3)?;
            let created_at: String = row.get(4)?; // DuckDB returns TIMESTAMP as string
            let created_at: DateTime<Utc> = created_at.parse().unwrap_or_else(|_| Utc::now());
            out.push(Message {
                id,
                session_id: sid,
                role: MessageRole::from_str(&role),
                content,
                created_at,
            });
        }
        out.reverse();
        Ok(out)
    }

    pub fn get_message(&self, message_id: i64) -> Result<Option<Message>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, session_id, role, content, CAST(created_at AS TEXT) as created_at FROM messages WHERE id = ?")?;
        let mut rows = stmt.query(params![message_id])?;
        if let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let sid: String = row.get(1)?;
            let role: String = row.get(2)?;
            let content: String = row.get(3)?;
            let created_at: String = row.get(4)?;
            let created_at: DateTime<Utc> = created_at.parse().unwrap_or_else(|_| Utc::now());
            Ok(Some(Message {
                id,
                session_id: sid,
                role: MessageRole::from_str(&role),
                content,
                created_at,
            }))
        } else {
            Ok(None)
        }
    }

    /// Simple pruning by keeping only the most recent `keep_latest` messages.
    pub fn prune_messages(&self, session_id: &str, keep_latest: i64) -> Result<u64> {
        let conn = self.conn();
        let mut stmt = conn.prepare("DELETE FROM messages WHERE session_id = ? AND id NOT IN (SELECT id FROM messages WHERE session_id = ? ORDER BY id DESC LIMIT ?)")?;
        let changed = stmt.execute(params![session_id, session_id, keep_latest])? as u64;
        Ok(changed)
    }

    // ---------- Memory Vectors ----------

    pub fn insert_memory_vector(
        &self,
        session_id: &str,
        message_id: Option<i64>,
        embedding: &[f32],
    ) -> Result<i64> {
        let conn = self.conn();
        let embedding_json = serde_json::to_string(embedding)?;
        let mut stmt = conn.prepare("INSERT INTO memory_vectors (session_id, message_id, embedding) VALUES (?, ?, ?) RETURNING id")?;
        let id: i64 = stmt.query_row(params![session_id, message_id, embedding_json], |row| {
            row.get(0)
        })?;
        Ok(id)
    }

    pub fn recall_top_k(
        &self,
        session_id: &str,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<(MemoryVector, f32)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, session_id, message_id, embedding, CAST(created_at AS TEXT) as created_at FROM memory_vectors WHERE session_id = ?")?;
        let mut rows = stmt.query(params![session_id])?;
        let mut scored: Vec<(MemoryVector, f32)> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let sid: String = row.get(1)?;
            let message_id: Option<i64> = row.get(2)?;
            let embedding_text: String = row.get(3)?;
            let created_at: String = row.get(4)?;
            let created_at: DateTime<Utc> = created_at.parse().unwrap_or_else(|_| Utc::now());
            let embedding: Vec<f32> = serde_json::from_str(&embedding_text).unwrap_or_default();
            let score = cosine_similarity(query_embedding, &embedding);
            scored.push((
                MemoryVector {
                    id,
                    session_id: sid,
                    message_id,
                    embedding,
                    created_at,
                },
                score,
            ));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }

    /// List known session IDs ordered by most recent activity
    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT session_id, MAX(created_at) as last FROM messages GROUP BY session_id ORDER BY last DESC"
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let sid: String = row.get(0)?;
            out.push(sid);
        }
        Ok(out)
    }

    // ---------- Tool Log ----------

    pub fn log_tool(
        &self,
        session_id: &str,
        agent_name: &str,
        run_id: &str,
        tool_name: &str,
        arguments: &JsonValue,
        result: &JsonValue,
        success: bool,
        error: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn();
        let mut stmt = conn.prepare("INSERT INTO tool_log (session_id, agent, run_id, tool_name, arguments, result, success, error) VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id")?;
        let id: i64 = stmt.query_row(
            params![
                session_id,
                agent_name,
                run_id,
                tool_name,
                arguments.to_string(),
                result.to_string(),
                success,
                error.unwrap_or("")
            ],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    // ---------- Policy Cache ----------

    pub fn policy_upsert(&self, key: &str, value: &JsonValue) -> Result<()> {
        let conn = self.conn();
        // DuckDB upsert workaround: delete then insert atomically within a transaction.
        conn.execute_batch("BEGIN TRANSACTION;")?;
        {
            let mut del = conn.prepare("DELETE FROM policy_cache WHERE key = ?")?;
            let _ = del.execute(params![key])?;
            let mut ins = conn.prepare("INSERT INTO policy_cache (key, value, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP)")?;
            let _ = ins.execute(params![key, value.to_string()])?;
        }
        conn.execute_batch("COMMIT;")?;
        Ok(())
    }

    pub fn policy_get(&self, key: &str) -> Result<Option<PolicyEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value, CAST(updated_at AS TEXT) as updated_at FROM policy_cache WHERE key = ?")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            let value_text: String = row.get(1)?;
            let updated_at: String = row.get(2)?;
            let updated_at: DateTime<Utc> = updated_at.parse().unwrap_or_else(|_| Utc::now());
            let value: JsonValue = serde_json::from_str(&value_text).unwrap_or(JsonValue::Null);
            Ok(Some(PolicyEntry {
                key,
                value,
                updated_at,
            }))
        } else {
            Ok(None)
        }
    }
}

fn expand_tilde(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();
    if path_str == "~" {
        let base = BaseDirs::new().context("base directories not available")?;
        Ok(base.home_dir().to_path_buf())
    } else if let Some(stripped) = path_str.strip_prefix("~/") {
        let base = BaseDirs::new().context("base directories not available")?;
        Ok(base.home_dir().join(stripped))
    } else {
        Ok(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn expands_home_directory_prefix() {
        let base = BaseDirs::new().expect("home directory available");
        let expected = base.home_dir().join("demo.db");
        let result = expand_tilde(Path::new("~/demo.db")).expect("path expansion succeeds");
        assert_eq!(result, expected);
    }

    #[test]
    fn leaves_regular_paths_unchanged() {
        let input = Path::new("relative/path.db");
        let result = expand_tilde(input).expect("path expansion succeeds");
        assert_eq!(result, input);
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

// ========== Knowledge Graph Methods ==========

impl Persistence {
    // ---------- Graph Node Operations ----------

    pub fn insert_graph_node(
        &self,
        session_id: &str,
        node_type: NodeType,
        label: &str,
        properties: &JsonValue,
        embedding_id: Option<i64>,
    ) -> Result<i64> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "INSERT INTO graph_nodes (session_id, node_type, label, properties, embedding_id)
             VALUES (?, ?, ?, ?, ?) RETURNING id",
        )?;
        let id: i64 = stmt.query_row(
            params![
                session_id,
                node_type.as_str(),
                label,
                properties.to_string(),
                embedding_id,
            ],
            |row| row.get(0),
        )?;
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
        conn.execute(
            "UPDATE graph_nodes SET properties = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            params![properties.to_string(), node_id],
        )?;
        Ok(())
    }

    pub fn delete_graph_node(&self, node_id: i64) -> Result<()> {
        let conn = self.conn();
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
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "INSERT INTO graph_edges (session_id, source_id, target_id, edge_type, predicate, properties, weight)
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
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
            ],
            |row| row.get(0),
        )?;
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
        // Simple BFS implementation for finding shortest path
        // In production, this would use DuckPGQ's ANY SHORTEST functionality
        let max_depth = max_hops.unwrap_or(10);

        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut parent_map = std::collections::HashMap::new();

        queue.push_back((source_id, 0));
        visited.insert(source_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            if current_id == target_id {
                // Reconstruct path
                let path = self.reconstruct_path(&parent_map, source_id, target_id)?;
                return Ok(Some(path));
            }

            if depth >= max_depth {
                continue;
            }

            // Get outgoing edges
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

        let mut visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();

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

            // Get edges based on direction
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

    // ---------- Helper Methods ----------

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
        parent_map: &std::collections::HashMap<i64, (i64, GraphEdge)>,
        source_id: i64,
        target_id: i64,
    ) -> Result<GraphPath> {
        let mut path_edges = Vec::new();
        let mut path_nodes = Vec::new();
        let mut current = target_id;
        let mut total_weight = 0.0;

        // Collect edges in reverse order
        while current != source_id {
            if let Some((parent, edge)) = parent_map.get(&current) {
                path_edges.push(edge.clone());
                total_weight += edge.weight;
                current = *parent;
            } else {
                break;
            }
        }

        // Reverse to get correct order
        path_edges.reverse();

        // Collect nodes
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

    // ---------- Transcriptions ----------

    pub fn insert_transcription(
        &self,
        session_id: &str,
        chunk_id: i64,
        text: &str,
        timestamp: chrono::DateTime<Utc>,
    ) -> Result<i64> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "INSERT INTO transcriptions (session_id, chunk_id, text, timestamp, embedding_id) VALUES (?, ?, ?, ?, NULL) RETURNING id",
        )?;
        let id: i64 = stmt.query_row(
            params![session_id, chunk_id, text, timestamp.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn update_transcription_embedding(
        &self,
        transcription_id: i64,
        embedding_id: i64,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE transcriptions SET embedding_id = ? WHERE id = ?",
            params![embedding_id, transcription_id],
        )?;
        Ok(())
    }

    pub fn list_transcriptions(&self, session_id: &str, limit: Option<i64>) -> Result<Vec<(i64, i64, String, DateTime<Utc>)>> {
        let conn = self.conn();
        let query = if let Some(lim) = limit {
            format!(
                "SELECT id, chunk_id, text, CAST(timestamp AS TEXT) FROM transcriptions WHERE session_id = ? ORDER BY chunk_id ASC LIMIT {}",
                lim
            )
        } else {
            "SELECT id, chunk_id, text, CAST(timestamp AS TEXT) FROM transcriptions WHERE session_id = ? ORDER BY chunk_id ASC".to_string()
        };

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(params![session_id])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let chunk_id: i64 = row.get(1)?;
            let text: String = row.get(2)?;
            let timestamp_str: String = row.get(3)?;
            let timestamp: DateTime<Utc> = timestamp_str.parse().unwrap_or_else(|_| Utc::now());
            out.push((id, chunk_id, text, timestamp));
        }

        Ok(out)
    }

    pub fn get_full_transcription(&self, session_id: &str) -> Result<String> {
        let transcriptions = self.list_transcriptions(session_id, None)?;
        Ok(transcriptions.into_iter().map(|(_, _, text, _)| text).collect::<Vec<_>>().join(" "))
    }

    pub fn delete_transcriptions(&self, session_id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM transcriptions WHERE session_id = ?", params![session_id])?;
        Ok(())
    }

    pub fn get_transcription_by_embedding(&self, embedding_id: i64) -> Result<Option<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT text FROM transcriptions WHERE embedding_id = ? LIMIT 1"
        )?;
        let result: Result<String, _> = stmt.query_row(params![embedding_id], |row| row.get(0));
        match result {
            Ok(text) => Ok(Some(text)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
