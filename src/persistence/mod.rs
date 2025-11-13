pub mod migrations;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::BaseDirs;
use duckdb::{Connection, params};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};

use crate::types::{MemoryVector, Message, MessageRole, PolicyEntry};

#[derive(Clone, Debug)]
pub struct Persistence {
    db_path: PathBuf,
}

impl Persistence {
    /// Create or open the database at the provided path and run migrations.
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir).context("creating DB directory")?;
        }
        let conn = Connection::open(&db_path).context("opening DuckDB")?;
        migrations::run(&conn).context("running migrations")?;
        Ok(Self { db_path })
    }

    /// Creates or opens the default database at ~/.agent_cli/agent_data.duckdb
    pub fn new_default() -> Result<Self> {
        let base = BaseDirs::new().context("base directories not available")?;
        let path = base.home_dir().join(".agent_cli").join("agent_data.duckdb");
        Self::new(path)
    }

    /// Get a fresh blocking connection to the DuckDB database file.
    /// This acts as a simple connection factory and can be used from `spawn_blocking` in async contexts.
    pub fn conn(&self) -> Result<Connection> {
        Connection::open(&self.db_path).context("opening DuckDB connection")
    }

    // ---------- Messages ----------

    pub fn insert_message(
        &self,
        session_id: &str,
        role: MessageRole,
        content: &str,
    ) -> Result<i64> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "INSERT INTO messages (session_id, role, content) VALUES (?, ?, ?) RETURNING id",
        )?;
        let id: i64 = stmt.query_row(params![session_id, role.as_str(), content], |row| {
            row.get(0)
        })?;
        Ok(id)
    }

    pub fn list_messages(&self, session_id: &str, limit: i64) -> Result<Vec<Message>> {
        let conn = self.conn()?;
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
        let conn = self.conn()?;
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
        let conn = self.conn()?;
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
        let conn = self.conn()?;
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
        let conn = self.conn()?;
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
        let conn = self.conn()?;
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
        tool_name: &str,
        arguments: &JsonValue,
        result: &JsonValue,
        success: bool,
        error: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("INSERT INTO tool_log (tool_name, arguments, result, success, error) VALUES (?, ?, ?, ?, ?) RETURNING id")?;
        let id: i64 = stmt.query_row(
            params![
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
        let conn = self.conn()?;
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
        let conn = self.conn()?;
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
