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
    let mut migrations_applied = false;

    if current < 1 {
        apply_v1(conn)?;
        set_version(conn, 1)?;
        migrations_applied = true;
    }

    if current < 2 {
        apply_v2(conn)?;
        set_version(conn, 2)?;
        migrations_applied = true;
    }

    if current < 3 {
        apply_v3(conn)?;
        set_version(conn, 3)?;
        migrations_applied = true;
    }

    // Force checkpoint after migrations to ensure WAL is merged into the database file.
    // This prevents ALTER TABLE operations from being stuck in the WAL, which can cause
    // "no default database set" errors during WAL replay on subsequent startups.
    // See: https://github.com/duckdb/duckdb/discussions/10200
    if migrations_applied {
        conn.execute_batch("FORCE CHECKPOINT;")
            .context("forcing checkpoint after migrations")?;
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

fn apply_v3(conn: &Connection) -> Result<()> {
    // Knowledge graph tables with DuckPGQ support
    conn.execute_batch(
        r#"
        -- Install DuckPGQ extension for graph capabilities
        -- Note: DuckPGQ requires DuckDB v1.1.3+
        -- For now, we'll create the tables without the extension
        -- Users can manually install DuckPGQ when available

        -- Sequences for graph tables
        CREATE SEQUENCE IF NOT EXISTS graph_nodes_id_seq START 1;
        CREATE SEQUENCE IF NOT EXISTS graph_edges_id_seq START 1;
        CREATE SEQUENCE IF NOT EXISTS graph_metadata_id_seq START 1;

        -- Graph nodes table
        CREATE TABLE IF NOT EXISTS graph_nodes (
            id BIGINT PRIMARY KEY DEFAULT nextval('graph_nodes_id_seq'),
            session_id TEXT NOT NULL,
            node_type TEXT NOT NULL,  -- 'entity', 'concept', 'fact', 'message', 'tool_result'
            label TEXT NOT NULL,       -- semantic label (Person, Location, Action, etc.)
            properties TEXT NOT NULL,  -- JSON properties specific to node type
            embedding_id BIGINT,       -- FK to memory_vectors for semantic search
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (embedding_id) REFERENCES memory_vectors(id)
        );

        -- Graph edges table
        CREATE TABLE IF NOT EXISTS graph_edges (
            id BIGINT PRIMARY KEY DEFAULT nextval('graph_edges_id_seq'),
            session_id TEXT NOT NULL,
            source_id BIGINT NOT NULL,
            target_id BIGINT NOT NULL,
            edge_type TEXT NOT NULL,   -- 'RELATES_TO', 'CAUSED_BY', 'PART_OF', 'MENTIONS', etc.
            predicate TEXT,            -- RDF-style predicate for triple store
            properties TEXT,           -- JSON for edge metadata
            weight REAL DEFAULT 1.0,   -- for weighted graphs
            temporal_start TIMESTAMP,  -- for temporal graphs
            temporal_end TIMESTAMP,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (source_id) REFERENCES graph_nodes(id),
            FOREIGN KEY (target_id) REFERENCES graph_nodes(id)
        );

        -- Graph metadata table
        CREATE TABLE IF NOT EXISTS graph_metadata (
            id BIGINT PRIMARY KEY DEFAULT nextval('graph_metadata_id_seq'),
            session_id TEXT NOT NULL,
            graph_name TEXT NOT NULL,
            is_created BOOLEAN DEFAULT FALSE,  -- Track if DuckPGQ graph object exists
            schema_version INTEGER DEFAULT 1,
            config TEXT,  -- JSON config for graph-specific settings
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(session_id, graph_name)
        );

        -- Create indexes for performance
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_session ON graph_nodes(session_id);
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_type ON graph_nodes(node_type);
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_label ON graph_nodes(label);
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_embedding ON graph_nodes(embedding_id);

        CREATE INDEX IF NOT EXISTS idx_graph_edges_session ON graph_edges(session_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_source ON graph_edges(source_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_target ON graph_edges(target_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_type ON graph_edges(edge_type);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_temporal ON graph_edges(temporal_start, temporal_end);
        "#,
    )
    .context("applying v3 schema (knowledge graph tables)")
}
