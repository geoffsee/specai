pub mod migrations;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::BaseDirs;
use duckdb::{params, Connection};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use spec_ai_knowledge_graph::KnowledgeGraphStore;

use crate::types::{
    EdgeType, GraphEdge, GraphNode, GraphPath, MemoryVector, Message, MessageRole, NodeType,
    PolicyEntry, TraversalDirection,
};

#[derive(Clone)]
pub struct Persistence {
    conn: Arc<Mutex<Connection>>,
    instance_id: String,
    graph_store: KnowledgeGraphStore,
}

impl Persistence {
    /// Create or open the database at the provided path and run migrations.
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        Self::with_instance_id(db_path, generate_instance_id())
    }

    /// Create with a specific instance_id
    pub fn with_instance_id<P: AsRef<Path>>(db_path: P, instance_id: String) -> Result<Self> {
        let db_path = expand_tilde(db_path.as_ref())?;
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir).context("creating DB directory")?;
        }
        let conn = Connection::open(&db_path).context("opening DuckDB")?;
        migrations::run(&conn).context("running migrations")?;
        let conn_arc = Arc::new(Mutex::new(conn));
        let graph_store = KnowledgeGraphStore::new(conn_arc.clone(), instance_id.clone());
        Ok(Self {
            conn: conn_arc,
            instance_id,
            graph_store,
        })
    }

    /// Get the instance ID for this persistence instance
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Get direct access to the KnowledgeGraphStore for Phase 2+ consumer migration
    pub fn graph_store(&self) -> &KnowledgeGraphStore {
        &self.graph_store
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

fn generate_instance_id() -> String {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    let uuid = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
    format!("{}-{}", hostname, uuid)
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

// Type Conversion Helpers

fn from_kg_node(node: spec_ai_knowledge_graph::GraphNode) -> GraphNode {
    GraphNode {
        id: node.id,
        session_id: node.session_id,
        node_type: node.node_type,
        label: node.label,
        properties: node.properties,
        embedding_id: node.embedding_id,
        created_at: node.created_at,
        updated_at: node.updated_at,
    }
}

fn from_kg_edge(edge: spec_ai_knowledge_graph::GraphEdge) -> GraphEdge {
    GraphEdge {
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
    }
}

fn from_kg_path(path: spec_ai_knowledge_graph::GraphPath) -> GraphPath {
    GraphPath {
        nodes: path.nodes.into_iter().map(from_kg_node).collect(),
        edges: path.edges.into_iter().map(from_kg_edge).collect(),
        length: path.length,
        weight: path.weight,
    }
}


impl Persistence {
    // ---------- Graph Node Operations ----------

    pub fn insert_graph_node(
        &self,
        session_id: &str,
        node_type: spec_ai_knowledge_graph::NodeType,
        label: &str,
        properties: &JsonValue,
        embedding_id: Option<i64>,
    ) -> Result<i64> {
        self.graph_store.insert_graph_node(session_id, node_type, label, properties, embedding_id)
    }

    pub fn get_graph_node(&self, node_id: i64) -> Result<Option<GraphNode>> {
        self.graph_store
            .get_graph_node(node_id)
            .map(|opt| opt.map(from_kg_node))
    }

    pub fn list_graph_nodes(
        &self,
        session_id: &str,
        node_type: Option<spec_ai_knowledge_graph::NodeType>,
        limit: Option<i64>,
    ) -> Result<Vec<GraphNode>> {
        self.graph_store
            .list_graph_nodes(session_id, node_type, limit)
            .map(|nodes| nodes.into_iter().map(from_kg_node).collect())
    }

    pub fn count_graph_nodes(&self, session_id: &str) -> Result<i64> {
        self.graph_store.count_graph_nodes(session_id)
    }

    pub fn update_graph_node(&self, node_id: i64, properties: &JsonValue) -> Result<()> {
        self.graph_store.update_graph_node(node_id, properties)
    }

    pub fn delete_graph_node(&self, node_id: i64) -> Result<()> {
        self.graph_store.delete_graph_node(node_id)
    }

    // ---------- Graph Edge Operations ----------

    pub fn insert_graph_edge(
        &self,
        session_id: &str,
        source_id: i64,
        target_id: i64,
        edge_type: spec_ai_knowledge_graph::EdgeType,
        predicate: Option<&str>,
        properties: Option<&JsonValue>,
        weight: f32,
    ) -> Result<i64> {
        self.graph_store.insert_graph_edge(
            session_id,
            source_id,
            target_id,
            edge_type,
            predicate,
            properties,
            weight,
        )
    }

    pub fn get_graph_edge(&self, edge_id: i64) -> Result<Option<GraphEdge>> {
        self.graph_store
            .get_graph_edge(edge_id)
            .map(|opt| opt.map(from_kg_edge))
    }

    pub fn list_graph_edges(
        &self,
        session_id: &str,
        source_id: Option<i64>,
        target_id: Option<i64>,
    ) -> Result<Vec<GraphEdge>> {
        self.graph_store
            .list_graph_edges(session_id, source_id, target_id)
            .map(|edges| edges.into_iter().map(from_kg_edge).collect())
    }

    pub fn count_graph_edges(&self, session_id: &str) -> Result<i64> {
        self.graph_store.count_graph_edges(session_id)
    }

    pub fn delete_graph_edge(&self, edge_id: i64) -> Result<()> {
        self.graph_store.delete_graph_edge(edge_id)
    }

    // ---------- Graph Traversal Operations ----------

    pub fn find_shortest_path(
        &self,
        session_id: &str,
        source_id: i64,
        target_id: i64,
        max_hops: Option<usize>,
    ) -> Result<Option<GraphPath>> {
        self.graph_store
            .find_shortest_path(session_id, source_id, target_id, max_hops)
            .map(|opt| opt.map(from_kg_path))
    }

    pub fn traverse_neighbors(
        &self,
        session_id: &str,
        node_id: i64,
        direction: spec_ai_knowledge_graph::TraversalDirection,
        depth: usize,
    ) -> Result<Vec<GraphNode>> {
        self.graph_store
            .traverse_neighbors(session_id, node_id, direction, depth)
            .map(|nodes| nodes.into_iter().map(from_kg_node).collect())
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

    pub fn list_transcriptions(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<(i64, i64, String, DateTime<Utc>)>> {
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
        Ok(transcriptions
            .into_iter()
            .map(|(_, _, text, _)| text)
            .collect::<Vec<_>>()
            .join(" "))
    }

    pub fn delete_transcriptions(&self, session_id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM transcriptions WHERE session_id = ?",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn get_transcription_by_embedding(&self, embedding_id: i64) -> Result<Option<String>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT text FROM transcriptions WHERE embedding_id = ? LIMIT 1")?;
        let result: Result<String, _> = stmt.query_row(params![embedding_id], |row| row.get(0));
        match result {
            Ok(text) => Ok(Some(text)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ---------- Tokenized Files Cache ----------

    /// Persist tokenization metadata for a file, replacing any existing entry for the path.
    pub fn upsert_tokenized_file(
        &self,
        session_id: &str,
        path: &str,
        file_hash: &str,
        raw_tokens: usize,
        cleaned_tokens: usize,
        bytes_captured: usize,
        truncated: bool,
        embedding_id: Option<i64>,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM tokenized_files WHERE session_id = ? AND path = ?",
            params![session_id, path],
        )?;
        let mut stmt = conn.prepare("INSERT INTO tokenized_files (session_id, path, file_hash, raw_tokens, cleaned_tokens, bytes_captured, truncated, embedding_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id")?;
        let id: i64 = stmt.query_row(
            params![
                session_id,
                path,
                file_hash,
                raw_tokens as i64,
                cleaned_tokens as i64,
                bytes_captured as i64,
                truncated,
                embedding_id
            ],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn get_tokenized_file(
        &self,
        session_id: &str,
        path: &str,
    ) -> Result<Option<TokenizedFileRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, session_id, path, file_hash, raw_tokens, cleaned_tokens, bytes_captured, truncated, embedding_id, CAST(updated_at AS TEXT) FROM tokenized_files WHERE session_id = ? AND path = ? LIMIT 1")?;
        let mut rows = stmt.query(params![session_id, path])?;
        if let Some(row) = rows.next()? {
            let record = TokenizedFileRecord::from_row(row)?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub fn list_tokenized_files(&self, session_id: &str) -> Result<Vec<TokenizedFileRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, session_id, path, file_hash, raw_tokens, cleaned_tokens, bytes_captured, truncated, embedding_id, CAST(updated_at AS TEXT) FROM tokenized_files WHERE session_id = ? ORDER BY path")?;
        let mut rows = stmt.query(params![session_id])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(TokenizedFileRecord::from_row(row)?);
        }
        Ok(out)
    }

    // ========== Mesh Message Persistence ==========

    /// Store a mesh message in the database
    pub fn mesh_message_store(
        &self,
        message_id: &str,
        source_instance: &str,
        target_instance: Option<&str>,
        message_type: &str,
        payload: &JsonValue,
        status: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        let payload_json = serde_json::to_string(payload)?;
        conn.execute(
            "INSERT INTO mesh_messages (message_id, source_instance, target_instance, message_type, payload, status) VALUES (?, ?, ?, ?, ?, ?)",
            params![message_id, source_instance, target_instance, message_type, payload_json, status],
        )?;
        // Get the last inserted ID
        let id: i64 = conn.query_row("SELECT last_insert_rowid()", params![], |row| row.get(0))?;
        Ok(id)
    }

    /// Check if a message with this ID already exists (for duplicate detection)
    pub fn mesh_message_exists(&self, message_id: &str) -> Result<bool> {
        let conn = self.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mesh_messages WHERE message_id = ?",
            params![message_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Update message status (e.g., delivered, failed)
    pub fn mesh_message_update_status(&self, message_id: i64, status: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE mesh_messages SET status = ?, delivered_at = CURRENT_TIMESTAMP WHERE id = ?",
            params![status, message_id],
        )?;
        Ok(())
    }

    /// Get pending messages for a target instance
    pub fn mesh_message_get_pending(
        &self,
        target_instance: &str,
    ) -> Result<Vec<MeshMessageRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, source_instance, target_instance, message_type, payload, status, CAST(created_at AS TEXT), CAST(delivered_at AS TEXT)
             FROM mesh_messages
             WHERE (target_instance = ? OR target_instance IS NULL) AND status = 'pending'
             ORDER BY created_at",
        )?;
        let mut rows = stmt.query(params![target_instance])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(MeshMessageRecord::from_row(row)?);
        }
        Ok(out)
    }

    /// Get message history for analytics
    pub fn mesh_message_get_history(
        &self,
        instance_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MeshMessageRecord>> {
        let conn = self.conn();
        let query = if instance_id.is_some() {
            format!(
                "SELECT id, source_instance, target_instance, message_type, payload, status, CAST(created_at AS TEXT), CAST(delivered_at AS TEXT)
                 FROM mesh_messages
                 WHERE source_instance = ? OR target_instance = ?
                 ORDER BY created_at DESC LIMIT {}",
                limit
            )
        } else {
            format!(
                "SELECT id, source_instance, target_instance, message_type, payload, status, CAST(created_at AS TEXT), CAST(delivered_at AS TEXT)
                 FROM mesh_messages
                 ORDER BY created_at DESC LIMIT {}",
                limit
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let mut rows = if let Some(inst) = instance_id {
            stmt.query(params![inst, inst])?
        } else {
            stmt.query(params![])?
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(MeshMessageRecord::from_row(row)?);
        }
        Ok(out)
    }

    // ===== Graph Synchronization Methods =====

    /// Append an entry to the graph changelog
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
        self.graph_store.graph_changelog_append(
            session_id,
            instance_id,
            entity_type,
            entity_id,
            operation,
            vector_clock,
            data,
        )
    }

    /// Get changelog entries since a given timestamp for a session
    pub fn graph_changelog_get_since(
        &self,
        session_id: &str,
        since_timestamp: &str,
    ) -> Result<Vec<ChangelogEntry>> {
        self.graph_store
            .graph_changelog_get_since(session_id, since_timestamp)
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|e| ChangelogEntry {
                        id: e.id,
                        session_id: e.session_id,
                        instance_id: e.instance_id,
                        entity_type: e.entity_type,
                        entity_id: e.entity_id,
                        operation: e.operation,
                        vector_clock: e.vector_clock,
                        data: e.data,
                        created_at: e.created_at,
                    })
                    .collect()
            })
    }

    /// Prune old changelog entries (keep last N days)
    pub fn graph_changelog_prune(&self, days_to_keep: i64) -> Result<usize> {
        self.graph_store.graph_changelog_prune(days_to_keep)
    }

    /// Get the vector clock for an instance/session/graph combination
    pub fn graph_sync_state_get(
        &self,
        instance_id: &str,
        session_id: &str,
        graph_name: &str,
    ) -> Result<Option<String>> {
        self.graph_store
            .graph_sync_state_get(instance_id, session_id, graph_name)
    }

    /// Update the vector clock for an instance/session/graph combination
    pub fn graph_sync_state_update(
        &self,
        instance_id: &str,
        session_id: &str,
        graph_name: &str,
        vector_clock: &str,
    ) -> Result<()> {
        self.graph_store
            .graph_sync_state_update(instance_id, session_id, graph_name, vector_clock)
    }

    /// Enable or disable sync for a graph
    pub fn graph_set_sync_enabled(
        &self,
        session_id: &str,
        graph_name: &str,
        enabled: bool,
    ) -> Result<()> {
        self.graph_store
            .graph_set_sync_enabled(session_id, graph_name, enabled)
    }

    /// Check if sync is enabled for a graph
    pub fn graph_get_sync_enabled(&self, session_id: &str, graph_name: &str) -> Result<bool> {
        self.graph_store
            .graph_get_sync_enabled(session_id, graph_name)
    }

    /// List all graphs for a session
    pub fn graph_list(&self, session_id: &str) -> Result<Vec<String>> {
        self.graph_store.graph_list(session_id)
    }

    /// Get a node with its sync metadata
    pub fn graph_get_node_with_sync(&self, node_id: i64) -> Result<Option<SyncedNodeRecord>> {
        self.graph_store
            .graph_get_node_with_sync(node_id)
            .map(|opt| {
                opt.map(|r| SyncedNodeRecord {
                    id: r.id,
                    session_id: r.session_id,
                    node_type: r.node_type,
                    label: r.label,
                    properties: r.properties,
                    embedding_id: r.embedding_id,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    vector_clock: r.vector_clock,
                    last_modified_by: r.last_modified_by,
                    is_deleted: r.is_deleted,
                    sync_enabled: r.sync_enabled,
                })
            })
    }

    /// Get all synced nodes for a session with optional filters
    pub fn graph_list_nodes_with_sync(
        &self,
        session_id: &str,
        sync_enabled_only: bool,
        include_deleted: bool,
    ) -> Result<Vec<SyncedNodeRecord>> {
        self.graph_store
            .graph_list_nodes_with_sync(session_id, sync_enabled_only, include_deleted)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|r| SyncedNodeRecord {
                        id: r.id,
                        session_id: r.session_id,
                        node_type: r.node_type,
                        label: r.label,
                        properties: r.properties,
                        embedding_id: r.embedding_id,
                        created_at: r.created_at,
                        updated_at: r.updated_at,
                        vector_clock: r.vector_clock,
                        last_modified_by: r.last_modified_by,
                        is_deleted: r.is_deleted,
                        sync_enabled: r.sync_enabled,
                    })
                    .collect()
            })
    }

    /// Get an edge with its sync metadata
    pub fn graph_get_edge_with_sync(&self, edge_id: i64) -> Result<Option<SyncedEdgeRecord>> {
        self.graph_store
            .graph_get_edge_with_sync(edge_id)
            .map(|opt| {
                opt.map(|r| SyncedEdgeRecord {
                    id: r.id,
                    session_id: r.session_id,
                    source_id: r.source_id,
                    target_id: r.target_id,
                    edge_type: r.edge_type,
                    predicate: r.predicate,
                    properties: r.properties,
                    weight: r.weight,
                    temporal_start: r.temporal_start,
                    temporal_end: r.temporal_end,
                    created_at: r.created_at,
                    vector_clock: r.vector_clock,
                    last_modified_by: r.last_modified_by,
                    is_deleted: r.is_deleted,
                    sync_enabled: r.sync_enabled,
                })
            })
    }

    /// Get all synced edges for a session with optional filters
    pub fn graph_list_edges_with_sync(
        &self,
        session_id: &str,
        sync_enabled_only: bool,
        include_deleted: bool,
    ) -> Result<Vec<SyncedEdgeRecord>> {
        self.graph_store
            .graph_list_edges_with_sync(session_id, sync_enabled_only, include_deleted)
            .map(|edges| {
                edges
                    .into_iter()
                    .map(|r| SyncedEdgeRecord {
                        id: r.id,
                        session_id: r.session_id,
                        source_id: r.source_id,
                        target_id: r.target_id,
                        edge_type: r.edge_type,
                        predicate: r.predicate,
                        properties: r.properties,
                        weight: r.weight,
                        temporal_start: r.temporal_start,
                        temporal_end: r.temporal_end,
                        created_at: r.created_at,
                        vector_clock: r.vector_clock,
                        last_modified_by: r.last_modified_by,
                        is_deleted: r.is_deleted,
                        sync_enabled: r.sync_enabled,
                    })
                    .collect()
            })
    }

    /// Update a node's sync metadata
    pub fn graph_update_node_sync_metadata(
        &self,
        node_id: i64,
        vector_clock: &str,
        last_modified_by: &str,
        sync_enabled: bool,
    ) -> Result<()> {
        self.graph_store
            .graph_update_node_sync_metadata(node_id, vector_clock, last_modified_by, sync_enabled)
    }

    /// Update an edge's sync metadata
    pub fn graph_update_edge_sync_metadata(
        &self,
        edge_id: i64,
        vector_clock: &str,
        last_modified_by: &str,
        sync_enabled: bool,
    ) -> Result<()> {
        self.graph_store
            .graph_update_edge_sync_metadata(edge_id, vector_clock, last_modified_by, sync_enabled)
    }

    /// Mark a node as deleted (tombstone pattern)
    pub fn graph_mark_node_deleted(
        &self,
        node_id: i64,
        vector_clock: &str,
        deleted_by: &str,
    ) -> Result<()> {
        self.graph_store
            .graph_mark_node_deleted(node_id, vector_clock, deleted_by)
    }

    /// Mark an edge as deleted (tombstone pattern)
    pub fn graph_mark_edge_deleted(
        &self,
        edge_id: i64,
        vector_clock: &str,
        deleted_by: &str,
    ) -> Result<()> {
        self.graph_store
            .graph_mark_edge_deleted(edge_id, vector_clock, deleted_by)
    }
}

#[derive(Debug, Clone)]
pub struct TokenizedFileRecord {
    pub id: i64,
    pub session_id: String,
    pub path: String,
    pub file_hash: String,
    pub raw_tokens: usize,
    pub cleaned_tokens: usize,
    pub bytes_captured: usize,
    pub truncated: bool,
    pub embedding_id: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

impl TokenizedFileRecord {
    fn from_row(row: &duckdb::Row) -> Result<Self> {
        let id: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let path: String = row.get(2)?;
        let file_hash: String = row.get(3)?;
        let raw_tokens: i64 = row.get(4)?;
        let cleaned_tokens: i64 = row.get(5)?;
        let bytes_captured: i64 = row.get(6)?;
        let truncated: bool = row.get(7)?;
        let embedding_id: Option<i64> = row.get(8)?;
        let updated_at: String = row.get(9)?;

        Ok(Self {
            id,
            session_id,
            path,
            file_hash,
            raw_tokens: raw_tokens.max(0) as usize,
            cleaned_tokens: cleaned_tokens.max(0) as usize,
            bytes_captured: bytes_captured.max(0) as usize,
            truncated,
            embedding_id,
            updated_at: updated_at.parse().unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MeshMessageRecord {
    pub id: i64,
    pub source_instance: String,
    pub target_instance: Option<String>,
    pub message_type: String,
    pub payload: JsonValue,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

impl MeshMessageRecord {
    fn from_row(row: &duckdb::Row) -> Result<Self> {
        let id: i64 = row.get(0)?;
        let source_instance: String = row.get(1)?;
        let target_instance: Option<String> = row.get(2)?;
        let message_type: String = row.get(3)?;
        let payload_str: String = row.get(4)?;
        let payload: JsonValue = serde_json::from_str(&payload_str)?;
        let status: String = row.get(5)?;
        let created_at_str: String = row.get(6)?;
        let delivered_at_str: Option<String> = row.get(7)?;

        Ok(MeshMessageRecord {
            id,
            source_instance,
            target_instance,
            message_type,
            payload,
            status,
            created_at: created_at_str.parse().unwrap_or_else(|_| Utc::now()),
            delivered_at: delivered_at_str.and_then(|s| s.parse().ok()),
        })
    }
}

// ===== Graph Sync Record Types =====

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
    pub properties: serde_json::Value,
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
        let properties: serde_json::Value = serde_json::from_str(&properties_str).map_err(|e| {
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
    pub properties: Option<serde_json::Value>,
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
        let properties: Option<serde_json::Value> = properties_str
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
