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

    if current < 4 {
        apply_v4(conn)?;
        set_version(conn, 4)?;
        migrations_applied = true;
    }

    if current < 5 {
        apply_v5(conn)?;
        set_version(conn, 5)?;
        migrations_applied = true;
    }

    if current < 6 {
        apply_v6(conn)?;
        set_version(conn, 6)?;
        migrations_applied = true;
    }

    if current < 7 {
        apply_v7(conn)?;
        set_version(conn, 7)?;
        migrations_applied = true;
    }

    if current < 8 {
        apply_v8(conn)?;
        set_version(conn, 8)?;
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

fn apply_v4(conn: &Connection) -> Result<()> {
    // Transcriptions table for audio transcription storage
    conn.execute_batch(
        r#"
        -- Sequence for transcriptions table
        CREATE SEQUENCE IF NOT EXISTS transcriptions_id_seq START 1;

        -- Transcriptions table
        CREATE TABLE IF NOT EXISTS transcriptions (
            id BIGINT PRIMARY KEY DEFAULT nextval('transcriptions_id_seq'),
            session_id TEXT NOT NULL,
            chunk_id INTEGER NOT NULL,
            text TEXT NOT NULL,
            timestamp TIMESTAMP NOT NULL,
            embedding_id BIGINT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (embedding_id) REFERENCES memory_vectors(id)
        );

        -- Create indexes for performance
        CREATE INDEX IF NOT EXISTS idx_transcriptions_session ON transcriptions(session_id);
        CREATE INDEX IF NOT EXISTS idx_transcriptions_session_chunk ON transcriptions(session_id, chunk_id);
        CREATE INDEX IF NOT EXISTS idx_transcriptions_embedding ON transcriptions(embedding_id);
        "#,
    )
    .context("applying v4 schema (transcriptions table)")
}

fn apply_v5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE SEQUENCE IF NOT EXISTS tokenized_files_id_seq START 1;

        CREATE TABLE IF NOT EXISTS tokenized_files (
            id BIGINT PRIMARY KEY DEFAULT nextval('tokenized_files_id_seq'),
            session_id TEXT NOT NULL,
            path TEXT NOT NULL,
            file_hash TEXT NOT NULL,
            raw_tokens INTEGER NOT NULL,
            cleaned_tokens INTEGER NOT NULL,
            bytes_captured INTEGER NOT NULL,
            truncated BOOLEAN DEFAULT FALSE,
            embedding_id BIGINT,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(session_id, path),
            FOREIGN KEY (embedding_id) REFERENCES memory_vectors(id)
        );

        CREATE INDEX IF NOT EXISTS idx_tokenized_files_session ON tokenized_files(session_id);
        CREATE INDEX IF NOT EXISTS idx_tokenized_files_hash ON tokenized_files(file_hash);
        "#,
    )
    .context("applying v5 schema (tokenized file cache)")
}

fn apply_v6(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE SEQUENCE IF NOT EXISTS mesh_messages_id_seq START 1;

        -- Service registry for mesh instances
        CREATE TABLE IF NOT EXISTS mesh_registry (
            instance_id TEXT PRIMARY KEY,
            hostname TEXT NOT NULL,
            port INTEGER NOT NULL,
            capabilities TEXT, -- JSON array of capabilities
            is_leader BOOLEAN DEFAULT FALSE,
            last_heartbeat TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        -- Inter-agent messaging
        CREATE TABLE IF NOT EXISTS mesh_messages (
            id BIGINT PRIMARY KEY DEFAULT nextval('mesh_messages_id_seq'),
            source_instance TEXT NOT NULL,
            target_instance TEXT,
            message_type TEXT NOT NULL,
            payload TEXT, -- JSON payload
            status TEXT DEFAULT 'pending', -- pending, delivered, failed
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            delivered_at TIMESTAMP,
            FOREIGN KEY (source_instance) REFERENCES mesh_registry(instance_id),
            FOREIGN KEY (target_instance) REFERENCES mesh_registry(instance_id)
        );

        -- Distributed consensus/locking
        CREATE TABLE IF NOT EXISTS mesh_consensus (
            resource TEXT PRIMARY KEY,
            owner_instance TEXT NOT NULL,
            acquired_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            expires_at TIMESTAMP NOT NULL,
            version INTEGER DEFAULT 1,
            FOREIGN KEY (owner_instance) REFERENCES mesh_registry(instance_id)
        );

        -- Indexes for efficient queries
        CREATE INDEX IF NOT EXISTS idx_mesh_registry_leader ON mesh_registry(is_leader);
        CREATE INDEX IF NOT EXISTS idx_mesh_messages_target ON mesh_messages(target_instance, status);
        CREATE INDEX IF NOT EXISTS idx_mesh_messages_created ON mesh_messages(created_at);
        CREATE INDEX IF NOT EXISTS idx_mesh_consensus_expires ON mesh_consensus(expires_at);
        "#,
    )
    .context("applying v6 schema (mesh networking)")
}

fn apply_v7(conn: &Connection) -> Result<()> {
    // Graph synchronization: Add vector clocks, change tracking, and sync state
    conn.execute_batch(
        r#"
        -- Add sync metadata columns to graph_nodes
        ALTER TABLE graph_nodes ADD COLUMN vector_clock TEXT DEFAULT '{}';
        ALTER TABLE graph_nodes ADD COLUMN last_modified_by TEXT;
        ALTER TABLE graph_nodes ADD COLUMN is_deleted BOOLEAN DEFAULT FALSE;
        ALTER TABLE graph_nodes ADD COLUMN sync_enabled BOOLEAN DEFAULT FALSE;

        -- Add sync metadata columns to graph_edges
        ALTER TABLE graph_edges ADD COLUMN vector_clock TEXT DEFAULT '{}';
        ALTER TABLE graph_edges ADD COLUMN last_modified_by TEXT;
        ALTER TABLE graph_edges ADD COLUMN is_deleted BOOLEAN DEFAULT FALSE;
        ALTER TABLE graph_edges ADD COLUMN sync_enabled BOOLEAN DEFAULT FALSE;

        -- Add sync toggle to graph_metadata for graph-level opt-in
        ALTER TABLE graph_metadata ADD COLUMN sync_enabled BOOLEAN DEFAULT FALSE;

        -- Create sequence for changelog
        CREATE SEQUENCE IF NOT EXISTS graph_changelog_id_seq START 1;

        -- Change log for incremental sync
        CREATE TABLE IF NOT EXISTS graph_changelog (
            id BIGINT PRIMARY KEY DEFAULT nextval('graph_changelog_id_seq'),
            session_id TEXT NOT NULL,
            instance_id TEXT NOT NULL,
            entity_type TEXT NOT NULL,  -- 'node' or 'edge'
            entity_id BIGINT NOT NULL,
            operation TEXT NOT NULL,  -- 'create', 'update', 'delete'
            vector_clock TEXT NOT NULL,  -- JSON map of instance_id -> version
            data TEXT,  -- Full entity JSON snapshot
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (instance_id) REFERENCES mesh_registry(instance_id)
        );

        -- Sync state tracking: per-instance vector clocks for each session/graph
        CREATE TABLE IF NOT EXISTS graph_sync_state (
            instance_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            graph_name TEXT NOT NULL,
            vector_clock TEXT NOT NULL,  -- JSON map of instance_id -> version
            last_sync_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (instance_id, session_id, graph_name),
            FOREIGN KEY (instance_id) REFERENCES mesh_registry(instance_id)
        );

        -- Indexes for sync operations
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_sync ON graph_nodes(sync_enabled, session_id);
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_deleted ON graph_nodes(is_deleted);
        CREATE INDEX IF NOT EXISTS idx_graph_nodes_modified ON graph_nodes(last_modified_by);

        CREATE INDEX IF NOT EXISTS idx_graph_edges_sync ON graph_edges(sync_enabled, session_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_deleted ON graph_edges(is_deleted);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_modified ON graph_edges(last_modified_by);

        CREATE INDEX IF NOT EXISTS idx_graph_changelog_session ON graph_changelog(session_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_graph_changelog_instance ON graph_changelog(instance_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_graph_changelog_entity ON graph_changelog(entity_type, entity_id);

        CREATE INDEX IF NOT EXISTS idx_graph_sync_state_session ON graph_sync_state(session_id, graph_name);
        "#,
    )
    .context("applying v7 schema (graph synchronization)")
}

fn apply_v8(conn: &Connection) -> Result<()> {
    // Add message_id column to mesh_messages for UUID v7 tracking
    // This allows consistent message IDs across instances for deduplication and correlation

    // Check if message_id column already exists
    let column_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM information_schema.columns
             WHERE table_name = 'mesh_messages' AND column_name = 'message_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !column_exists {
        // Since mesh_messages has foreign keys, we need to recreate the table
        // Old messages are transient anyway, so we just delete and recreate
        conn.execute_batch(
            r#"
            -- Drop the old table (messages are transient, this is safe)
            DROP TABLE IF EXISTS mesh_messages;

            -- Recreate with message_id column
            CREATE TABLE mesh_messages (
                id BIGINT PRIMARY KEY DEFAULT nextval('mesh_messages_id_seq'),
                message_id TEXT UNIQUE NOT NULL,
                source_instance TEXT NOT NULL,
                target_instance TEXT,
                message_type TEXT NOT NULL,
                payload TEXT,
                status TEXT DEFAULT 'pending',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                delivered_at TIMESTAMP,
                FOREIGN KEY (source_instance) REFERENCES mesh_registry(instance_id),
                FOREIGN KEY (target_instance) REFERENCES mesh_registry(instance_id)
            );
            "#,
        )
        .context("recreating mesh_messages with message_id column")?;
    }

    // Add indexes (IF NOT EXISTS makes this idempotent)
    conn.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_mesh_messages_message_id_unique ON mesh_messages(message_id);
        CREATE INDEX IF NOT EXISTS idx_mesh_messages_status ON mesh_messages(status);
        "#,
    )
    .context("adding indexes to mesh_messages")?;

    Ok(())
}
