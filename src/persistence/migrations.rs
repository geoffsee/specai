use anyhow::{Context, Result};
use duckdb::Connection;

pub fn run(conn: &Connection) -> Result<()> {
    // Simple migration system: ensure a schema version table and apply migrations sequentially.
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .context("creating schema_migrations table")?;

    let current = current_version(conn)?;
    if current < 1 {
        apply_v1(conn)?;
        set_version(conn, 1)?;
    }

    if current < 2 {
        apply_v2(conn)?;
        set_version(conn, 2)?;
    }

    Ok(())
}

fn current_version(conn: &Connection) -> Result<i64> {
    let mut stmt = conn.prepare("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")?;
    let v: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(v)
}

fn set_version(conn: &Connection, v: i64) -> Result<()> {
    conn.execute("INSERT INTO schema_migrations (version) VALUES (?)", [v])?;
    Ok(())
}

fn apply_v1(conn: &Connection) -> Result<()> {
    // Core tables per spec: messages, memory_vectors, tool_log, policy_cache
    conn.execute_batch(
        r#"
        -- Sequences for surrogate keys
        CREATE SEQUENCE IF NOT EXISTS messages_id_seq START 1;
        CREATE SEQUENCE IF NOT EXISTS memory_vectors_id_seq START 1;
        CREATE SEQUENCE IF NOT EXISTS tool_log_id_seq START 1;

        CREATE TABLE IF NOT EXISTS messages (
            id BIGINT PRIMARY KEY DEFAULT nextval('messages_id_seq'),
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS memory_vectors (
            id BIGINT PRIMARY KEY DEFAULT nextval('memory_vectors_id_seq'),
            session_id TEXT NOT NULL,
            message_id BIGINT,
            embedding TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (message_id) REFERENCES messages(id)
        );

        CREATE TABLE IF NOT EXISTS tool_log (
            id BIGINT PRIMARY KEY DEFAULT nextval('tool_log_id_seq'),
            tool_name TEXT NOT NULL,
            arguments TEXT NOT NULL,
            result TEXT NOT NULL,
            success BOOLEAN NOT NULL,
            error TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS policy_cache (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
        CREATE INDEX IF NOT EXISTS idx_memory_vectors_session ON memory_vectors(session_id);
        "#,
    )
    .context("applying v1 schema")?;

    Ok(())
}

fn apply_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        ALTER TABLE tool_log ADD COLUMN session_id TEXT;
        ALTER TABLE tool_log ADD COLUMN agent TEXT;
        ALTER TABLE tool_log ADD COLUMN run_id TEXT;
        UPDATE tool_log SET session_id = COALESCE(session_id, ''), agent = COALESCE(agent, ''), run_id = COALESCE(run_id, '');
        "#,
    )
    .context("applying v2 schema (tool telemetry columns)")
}
