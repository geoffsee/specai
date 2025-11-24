# Verification Guide


This document provides step-by-step instructions to verify all major features of spec-ai are working correctly.

## Automated Test Suites

The project includes comprehensive automated tests separated into fast unit tests and slow integration tests.

### Running Tests

```bash
# Run fast unit tests only (default, ~50 seconds)
cargo test

# Run integration tests only (slow, includes binary builds)
cargo test --features integration-tests

# Run all tests (unit + integration)
cargo test --features integration-tests
```

### Test Organization

**Unit Tests** (fast, included in default `cargo test`):
- 245+ unit tests in `src/` modules
- Integration tests in `tests/` (audio, CLI, config, graph, persistence, etc.)
- Complete in ~50 seconds

**Integration Tests** (slow, requires `--features integration-tests`):
- `test_different_scenarios` - Tests all 5 audio transcription scenarios
- `test_speed_multiplier` - Timing-sensitive audio test
- `test_binary_builds_successfully` - Compiles release binary
- `test_full_binary_spec_execution` - Full end-to-end binary test

Integration tests are automatically skipped in default `cargo test` runs to keep the development feedback loop fast.

## Prerequisites

Before running verification tests:

```bash
# Build the project
cargo build --release

# Or run in debug mode
cargo build
```

## 1. Basic CLI Functionality

### 1.1 Help Command

```bash
# Test help output
spec-ai --help
# OR
cargo run -- --help
```

**Expected Output:**
```
Usage: spec-ai [OPTIONS]

Options:
  -c, --config <PATH>    Path to config file (default: ./spec-ai.config.toml or ~/.spec-ai/spec-ai.config.toml)
  -h, --help             Print this help message
```

### 1.2 Default Startup

```bash
# Start with default config
spec-ai
# OR
cargo run
```

**Expected:**
- Config file created if it doesn't exist: `spec-ai.config.toml`
- Database initialized
- CLI prompt appears showing reasoning lines and status

**Verify:**
- [ ] Config file exists
- [ ] Database file exists at path shown in config
- [ ] No errors during startup
- [ ] Prompt is interactive and waiting for input

### 1.3 Custom Config Path

```bash
# Test custom config location
spec-ai --config /tmp/test-config.toml
# OR
cargo run -- -c /tmp/test-config.toml
```

**Expected:**
- Creates config at specified path if it doesn't exist
- Loads config from that path
- Uses database path specified in that config

**Verify:**
- [ ] Config created at `/tmp/test-config.toml`
- [ ] Application uses settings from custom config
- [ ] Can edit config and restart to see changes

## 2. Configuration System

### 2.1 Config Loading Priority

```bash
# Test 1: Current directory takes precedence
echo '[model]
provider = "mock"
model_name = "test-current-dir"' > ./spec-ai.config.toml

spec-ai
```

**Expected:** Model name shows "test-current-dir"

```bash
# Test 2: Command-line flag overrides everything
spec-ai --config /tmp/custom.toml
```

**Expected:** Uses /tmp/custom.toml instead of ./spec-ai.config.toml

### 2.2 Environment Variables

```bash
# Test environment variable override
AGENT_MODEL_PROVIDER=mock spec-ai
```

**Expected:** Provider set to "mock" regardless of config file

**Verify:**
- [ ] Environment variables override config values
- [ ] Command-line flag has highest priority
- [ ] Config file values used when no override present

## 3. Agent Profiles

### 3.1 Default Agent

```bash
spec-ai
```

At the prompt:
```
> /agents
```

**Expected:** List of available agents with one marked as active

### 3.2 Switch Agents

```
> /switch coder
> /agents
```

**Expected:**
- Success message: "Switched active agent to 'coder'"
- Agent list shows 'coder' as active

**Verify:**
- [ ] Can list all agents
- [ ] Can switch between agents
- [ ] Active agent marker updates correctly

## 4. Session Management

### 4.1 Create New Session

```
> /session new test-session-1
```

**Expected:** "Started new session 'test-session-1'"

### 4.2 List Sessions

```
> /session list
```

**Expected:** Shows all sessions including 'test-session-1'

### 4.3 Switch Sessions

```
> /session switch test-session-1
```

**Expected:** "Switched to session 'test-session-1'"

**Verify:**
- [ ] Can create named sessions
- [ ] Can list all sessions
- [ ] Can switch between sessions
- [ ] Each session has independent message history

## 5. Message Storage and Memory

### 5.1 Send Messages

```
> Hello, can you help me?
```

**Expected:** Agent responds (mock provider gives canned response)

### 5.2 View Message History

```
> /memory show 5
```

**Expected:** Shows last 5 messages (user + assistant)

**Verify:**
- [ ] Messages are stored in database
- [ ] Can retrieve message history
- [ ] Message history persists across restarts

## 6. Transcription System

### 6.1 Start Transcription (Background)

```
> /listen start 10
```

**Expected:**
- "Started background transcription using [provider] (duration: 10 seconds)"
- Transcription runs in background
- CLI remains usable

### 6.2 Check Transcription Status

```
> /listen status
```

**Expected:**
- Shows status: running or completed
- Shows elapsed time
- Shows duration

### 6.3 Stop Transcription

```
> /listen stop
```

**Expected:**
- "Stopped transcription (ran for X seconds, saved Y chunks to database)"
- Chunks saved to database with embeddings

### 6.4 Verify Transcription Storage

After stopping transcription, restart the application and query the database:

```sql
-- Using DuckDB CLI or similar
SELECT COUNT(*) FROM transcriptions;
SELECT * FROM transcriptions LIMIT 5;
```

**Expected:**
- Transcription chunks are in database
- Each chunk has an embedding_id (if embeddings enabled)

**Verify:**
- [ ] `/listen start` works without blocking
- [ ] Can use CLI while transcription runs
- [ ] `/listen status` shows accurate information
- [ ] `/listen stop` saves chunks to database
- [ ] Transcriptions have embeddings linked

## 7. Semantic Memory Retrieval

### 7.1 Store Content with Embeddings

**Note:** Requires embeddings provider configured (OpenAI, Anthropic, etc.)

```
> Hello, my favorite color is blue
> I live in San Francisco
> I work as a software engineer
```

### 7.2 Test Semantic Recall

```
> What is my favorite color?
```

**Expected (with embeddings enabled):**
- Agent recalls "blue" from earlier message
- Response includes "[Recall: semantic]" in reasoning

```
> Where do I live?
```

**Expected:**
- Agent recalls "San Francisco"
- Shows semantic recall in reasoning

### 7.3 Test Transcription Recall

After recording transcriptions:

```
> What did I say in my audio recording?
```

**Expected:**
- Agent retrieves transcription chunks from database
- Transcriptions appear in semantic search results
- Prefixed with "[Transcription]" in context

**Verify:**
- [ ] Messages have embeddings generated
- [ ] Semantic search returns relevant messages
- [ ] Transcriptions included in semantic search
- [ ] Transcriptions ranked by relevance

## 8. Knowledge Graph (if enabled)

### 8.1 Initialize Graph

```
> /init
```

**Expected:**
- Bootstraps repository knowledge graph
- Shows stats: "X nodes and Y edges captured"

### 8.2 Check Graph Status

```
> /graph status
```

**Expected:** Shows current graph configuration and whether enabled

### 8.3 Show Graph Contents

```
> /graph show 10
```

**Expected:** Lists up to 10 graph nodes with types and labels

**Verify:**
- [ ] Can initialize knowledge graph
- [ ] Graph status command works
- [ ] Can view graph contents
- [ ] Graph data persists across sessions

## 9. Policy System

### 9.1 Reload Policies

```
> /policy reload
```

**Expected:** "Policies reloaded. X rule(s) active."

**Verify:**
- [ ] Can reload policies from database
- [ ] Policies are applied to agent actions

## 10. Spec Execution

### 10.1 Create a Test Spec

Create `test.spec`:
```toml
name = "Verification Test"
goal = "Verify the spec runner works"

tasks = [
    "Confirm the spec loaded",
    "Return success message"
]

deliverables = [
    "Confirmation message"
]
```

### 10.2 Run the Spec

```
> /spec test.spec
```

**Expected:**
- Shows spec preview
- Executes spec
- Returns agent response addressing the tasks

**Verify:**
- [ ] Can load .spec files
- [ ] Spec preview displays correctly
- [ ] Spec execution works
- [ ] Agent responds to spec tasks

## 11. Database Persistence

### 11.1 Verify Database Schema

```bash
# Connect to database
duckdb ~/.spec-ai/agent_data.duckdb
```

```sql
-- Check tables exist
SHOW TABLES;

-- Expected tables:
-- - schema_migrations
-- - messages
-- - memory_vectors
-- - tool_log
-- - policy_cache
-- - graph_nodes
-- - graph_edges
-- - graph_metadata
-- - transcriptions

-- Check migration version
SELECT * FROM schema_migrations;

-- Expected: version = 4 (or higher)

-- Check transcriptions table structure
DESCRIBE transcriptions;

-- Expected columns:
-- - id
-- - session_id
-- - chunk_id
-- - text
-- - timestamp
-- - embedding_id
-- - created_at
```

**Verify:**
- [ ] All tables exist
- [ ] Migration to v4+ completed
- [ ] Transcriptions table has correct schema
- [ ] Foreign key constraints in place

## 12. Audio Configuration

### 12.1 Check Audio Config

View `spec-ai.config.toml`:

```toml
[audio]
provider = "mock"  # or "vttrs"
model = "whisper-1"
on_device = false
chunk_duration_secs = 5.0
default_duration_secs = 30
```

### 12.2 Test Mock Provider

```
> /listen start 5
```

**Expected (with mock provider):**
- Pre-defined transcriptions appear every chunk_duration_secs
- Chunks saved to database

### 12.3 Test VTT-RS Provider (if configured)

```toml
[audio]
provider = "vttrs"
on_device = true  # or false for OpenAI API
```

```
> /listen start 10
```

**Expected (with vttrs):**
- Real audio recording starts
- Real-time transcription appears
- Chunks saved with embeddings

**Verify:**
- [ ] Mock provider works for testing
- [ ] VTT-RS provider works (if configured)
- [ ] On-device mode works (if enabled)
- [ ] Cloud mode works with API key (if configured)

## 13. Error Handling

### 13.1 Invalid Config Path

```bash
spec-ai --config /nonexistent/dir/config.toml
```

**Expected:** Creates directory and config file OR clear error message

### 13.2 Malformed Config

Edit config file with invalid TOML:
```toml
[model
provider = "invalid
```

```bash
spec-ai
```

**Expected:** Clear parse error pointing to line number

### 13.3 Database Lock

```bash
# Terminal 1
spec-ai

# Terminal 2 (while Terminal 1 still running)
spec-ai
```

**Expected (Terminal 2):**
```
Error: Another instance of spec-ai is already running.

Only one instance can access the database at a time.
Please close the other instance or wait for it to finish.
```

**Verify:**
- [ ] Graceful error handling for invalid paths
- [ ] Clear parse errors for malformed config
- [ ] Database locking prevents corruption
- [ ] User-friendly error messages

## 14. Performance

### 14.1 Startup Time

```bash
time spec-ai --help
```

**Expected:** < 1 second for help

```bash
time cargo run
```

**Expected:**
- First run: ~2-5 seconds (config creation, DB init)
- Subsequent runs: < 1 second

### 14.2 Large Message History

Create session with 100+ messages, then:

```
> /memory show 100
```

**Expected:** Returns in < 2 seconds

**Verify:**
- [ ] Fast startup time
- [ ] Responsive with large message history
- [ ] Database queries are efficient

## 15. Full Integration Test

Complete end-to-end workflow:

```bash
# 1. Start fresh
rm -rf ~/.spec-ai
rm -f ./spec-ai.config.toml

# 2. Start application
spec-ai

# 3. Send a message
> Hello, I'm testing the system

# 4. Start transcription
> /listen start 5

# 5. Check status
> /listen status

# 6. Stop transcription
> /listen stop

# 7. View memory
> /memory show 10

# 8. Create new session
> /session new integration-test

# 9. List sessions
> /session list

# 10. Initialize graph (if supported)
> /init

# 11. Quit
> /quit
```

**Expected:**
- All commands execute successfully
- Data persists in database
- No errors or crashes
- Clean shutdown

**Verify:**
- [ ] Complete workflow executes without errors
- [ ] All data persisted correctly
- [ ] Can restart and continue session
- [ ] Database is not corrupted

## Checklist Summary

### Core Functionality
- [ ] Application starts successfully
- [ ] Help command works
- [ ] Custom config path works
- [ ] Configuration loading priority correct
- [ ] Environment variables work

### Agent System
- [ ] Can list agents
- [ ] Can switch agents
- [ ] Agent profiles load correctly

### Session Management
- [ ] Can create sessions
- [ ] Can list sessions
- [ ] Can switch sessions
- [ ] Sessions are independent

### Message & Memory
- [ ] Messages saved to database
- [ ] Can view message history
- [ ] Semantic search works (with embeddings)
- [ ] Message recall works correctly

### Transcription System
- [ ] Can start background transcription
- [ ] Can check transcription status
- [ ] Can stop transcription
- [ ] Chunks saved to database
- [ ] Embeddings generated for chunks
- [ ] Transcriptions in semantic search

### Database
- [ ] All tables created correctly
- [ ] Migration v4+ applied
- [ ] Data persists across restarts
- [ ] No corruption on shutdown
- [ ] Database locking works

### Error Handling
- [ ] Invalid config handled gracefully
- [ ] Missing files handled correctly
- [ ] Database lock prevents corruption
- [ ] Clear error messages

### Performance
- [ ] Fast startup
- [ ] Responsive with large datasets
- [ ] Efficient database queries

## Troubleshooting

### Issue: Config file not found
**Solution:** Check file path and permissions, or let app create default

### Issue: Database locked
**Solution:** Close other instances or wait for lock to release

### Issue: Embeddings not working
**Solution:** Verify embeddings provider configured with valid API key

### Issue: Transcription fails to start
**Solution:** Check audio provider config and dependencies (vtt-rs, etc.)

### Issue: Semantic search returns no results
**Solution:** Ensure embeddings are enabled and messages have been embedded

## Success Criteria

All features are verified working when:

1. ✅ All checklist items above are checked
2. ✅ No errors during normal operation
3. ✅ Data persists correctly across restarts
4. ✅ Performance is acceptable for typical use
5. ✅ Error handling is graceful and informative

---

**Last Updated:** 2025-01-19
**Spec-AI Version:** 0.1.24
