# SpecAI - Agent CLI

A "production-quality", completely vibe coded, agentic AI CLI built in Rust with persistent state, flexible configuration, and extensible architecture.

## Features

### ✅ Completed

#### Epic 1: Persistence Layer
- DuckDB-based persistence with automatic migrations
- Message history storage and retrieval
- Memory vectors with cosine similarity search
- Tool execution logging
- Policy/configuration caching

#### Epic 2: Configuration System
- TOML-based configuration with multiple precedence levels
- Environment variable overrides (`AGENT_*` prefix)
- Multiple agent profiles with custom settings
- Agent registry with persistence
- Configuration caching and change detection

#### Epic 3: Core Agent Architecture
- Structured agent reasoning loop (interpret → plan → execute → reflect)
- Model abstraction layer supporting multiple providers
- Memory subsystem with embedding-based recall
- Conversation history and context management

#### Epic 4: CLI Interface
- Interactive terminal REPL with markdown support
- Command parser for control commands
- Multi-agent session management
- Session switching and isolation

#### Epic 5: Tool Registry
- Dynamic tool registration and execution
- Built-in tools (echo, math, file operations)
- Tool permission system with allowlist/denylist
- Tool execution logging

#### Epic 6: Policy Framework
- Declarative policy engine for behavior control
- Wildcard pattern matching for flexible rules
- Runtime policy reload
- Integration with agent permission system
- Default-deny security model

#### Epic 7: Extensibility and Integration
- Plugin system with lifecycle management
  - Dynamic plugin registration and initialization
  - Plugin health checking
  - Thread-safe concurrent access
- REST API server (optional `api` feature)
  - `/health` - Service health monitoring
  - `/agents` - Agent discovery
  - `/query` - Synchronous queries
  - `/stream` - Server-Sent Events streaming
  - API key authentication
  - CORS support

## Quick Start

### Installation

```bash
cargo build --release
```

### Configuration

Copy the example configuration:

```bash
cp config.toml.example config.toml
```

Or place it in `~/.agent_cli/config.toml` for user-wide settings.

### Configuration Precedence

Configuration is loaded in the following order (highest precedence first):

1. **Environment Variables** - `AGENT_*` prefix (e.g., `AGENT_MODEL_PROVIDER=openai`)
2. **Current Directory** - `./config.toml`
3. **Home Directory** - `~/.agent_cli/config.toml`
4. **Defaults** - Sensible defaults for all settings

### Available Environment Variables

- `AGENT_DB_PATH` - Database file path
- `AGENT_MODEL_PROVIDER` - Model provider (mock, openai, anthropic, ollama)
- `AGENT_MODEL_NAME` - Specific model name
- `AGENT_MODEL_TEMPERATURE` - Generation temperature (0.0-2.0)
- `AGENT_API_KEY_SOURCE` - API key source
- `AGENT_LOG_LEVEL` - Logging level (trace, debug, info, warn, error)
- `AGENT_UI_THEME` - UI theme
- `AGENT_DEFAULT_AGENT` - Default agent profile to use

### Running

```bash
cargo run
```

Expected output:
```
Configuration loaded:
  Database: /Users/you/.agent_cli/agent_data.duckdb
  Model Provider: mock
  Temperature: 0.7
  Logging Level: info
  UI Theme: default
```

## Architecture

### Module Structure

```
src/
├── config/          # Configuration system
│   ├── mod.rs       # Core config loading and validation
│   ├── agent.rs     # Agent profile definitions
│   ├── registry.rs  # Agent registry and switching
│   └── cache.rs     # Configuration caching
├── persistence/     # Database layer
│   ├── mod.rs       # Persistence API
│   └── migrations.rs # Database migrations
├── types.rs         # Shared types
├── lib.rs           # Library exports
└── main.rs          # CLI entry point
```

### Agent Profiles

Define multiple agents with different personalities and capabilities:

```toml
[agents.coder]
prompt = "You are a helpful coding assistant"
temperature = 0.3
allowed_tools = ["file_read", "file_write", "bash"]
memory_k = 10

[agents.researcher]
prompt = "You are a research assistant"
temperature = 0.8
denied_tools = ["bash", "file_write"]
memory_k = 20
```

### Key APIs

#### Configuration
```rust
use spec_ai::config::{AppConfig, AgentRegistry, ConfigCache};
use spec_ai::persistence::Persistence;

// Load configuration
let config = AppConfig::load()?;

// Create agent registry
let persistence = Persistence::new(&config.database.path)?;
let registry = AgentRegistry::new(config.agents.clone(), persistence.clone());
registry.init()?;

// Switch agents
registry.set_active("coder")?;
let (name, profile) = registry.active()?.unwrap();

// Cache configuration
let cache = ConfigCache::new(persistence);
cache.store_effective_config(&config)?;
```

#### Persistence
```rust
use spec_ai::persistence::Persistence;

let persistence = Persistence::new_default()?;

// Store and retrieve messages
let id = persistence.insert_message("session1", MessageRole::User, "Hello")?;
let messages = persistence.list_messages("session1", 10)?;

// Memory vectors
let embedding = vec![0.1, 0.2, 0.3];
persistence.insert_memory_vector("session1", Some(id), &embedding)?;
let similar = persistence.recall_top_k("session1", &embedding, 5)?;
```

## Testing

Run all tests:
```bash
cargo test --all-targets
```

Run API tests (requires `api` feature):
```bash
cargo test --features api
```

Run specific test suites:
```bash
cargo test --lib plugin
cargo test --lib policy
cargo test --test policy_integration_tests
```

**Current test coverage: 162 tests passing**
- 136 unit tests (plugin: 13, api: 13, agent: 26, config: 28, tools: 26, policy: 18, cli: 6, other: 6)
- 26 integration tests (cli: 6, config: 11, persistence: 5, policy: 8, tools: 9)

## API Server Usage

The API server is available as an optional feature. To use it:

```bash
# Build with API support
cargo build --features api

# Example API usage
curl http://localhost:3000/health
curl http://localhost:3000/agents
curl -X POST http://localhost:3000/query \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello!", "agent": "coder"}'
```

See [EPIC7_SUMMARY.md](EPIC7_SUMMARY.md) for detailed API documentation.

## Next Steps

All planned epics (1-7) are now complete! The system is production-ready with:

- ✅ **Epic 1**: Persistence Layer (DuckDB integration)
- ✅ **Epic 2**: Configuration System (TOML, env vars, profiles)
- ✅ **Epic 3**: Core Agent Architecture (reasoning loop, model abstraction)
- ✅ **Epic 4**: CLI Interface (REPL, commands, sessions)
- ✅ **Epic 5**: Tool Registry (dynamic tools, permissions)
- ✅ **Epic 6**: Policy Framework (declarative rules, runtime reload)
- ✅ **Epic 7**: Extensibility and Integration (plugins, REST API)

See [docs/spec.md](docs/spec.md) for the complete specification and [EPIC7_SUMMARY.md](EPIC7_SUMMARY.md) for the latest implementation details.

## Development

### Code Quality

- Comprehensive error handling with `anyhow` and `thiserror`
- Type-safe configuration with validation
- Idiomatic Rust with clippy compliance
- Full test coverage for critical paths

### Design Principles

- **Separation of Concerns**: Config, persistence, and business logic are cleanly separated
- **Testability**: All components can be tested in isolation
- **Extensibility**: Easy to add new agents, tools, and providers
- **Safety**: Strong typing, validation, and error handling throughout

## License

MIT License

Copyright (c) 2024 SpecAI Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Contributing
Open a PR
