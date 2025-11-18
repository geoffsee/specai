# spec-ai

An agentic AI CLI tool written in Rust.

## Quick Start

```shell
cargo binstall spec-ai
```

## Native dependencies

`file_extract` depends on [`extractous`](https://docs.rs/extractous/latest/extractous), which compiles native Apache Tika libraries through GraalVM and optionally calls Tesseract for OCR. Install the following before invoking `file_extract` or running `cargo test file_extract`:

1. **GraalVM 23+ with native-image support**
   * Use [sdkman](https://sdkman.io) to install a compatible JDK:
     ```bash
     sdk install java 23.0.1-graalce    # Linux
     sdk install java 24.1.1.r23-nik    # macOS (Bellsoft Liberica NIK avoids AWT issues)
     ```
   * If you already have GraalVM, set `GRAALVM_HOME` to its installation root before building.
   * Confirm the active JVM by running `java -version`; it should report `GraalVM`.

2. **Tesseract OCR and language packs (optional but required for OCR PDFs)**
   * Debian/Ubuntu: `sudo apt install tesseract-ocr tesseract-ocr-deu tesseract-ocr-ara`
   * macOS: `brew install tesseract tesseract-lang`

3. **Other build prerequisites**
   * The extractous build script may also require `pkg-config`, `cmake`, or other native toolchain utilities depending on your platform.
   * If you hit `system-configuration` / `reqwest` panics such as `Attempted to create a NULL object`, ensure your macOS SDK/frameworks are intact for CoreFoundation and run the build again after setting `GRAALVM_HOME`.

Once the native prerequisites are installed, rerun `cargo clean && cargo test file_extract`; the initial build can take two-to-three minutes while the Tika native libs compile.

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
- `AGENT_MODEL_PROVIDER` - Model provider (mock, openai, anthropic, ollama, mlx, lmstudio)
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

## Structured Specs

You can capture complex tasks in reusable `.spec` files and hand them directly to the agent:

1. Write a TOML file with the `.spec` extension describing the goal, tasks, and expected deliverables.
2. Start the CLI (`cargo run`) and enter `/spec run path/to/plan.spec` (or the shorthand `/spec path/to/plan.spec`).

Example spec:

```toml
name = "Docs refresh"
goal = "Document the new spec runner command across the README"

context = """
Capture the CLI command syntax and show a working example.
"""

tasks = [
  "Explain how to invoke the command",
  "Provide a sample `.spec` file",
  "Highlight the goal/deliverables requirement"
]

deliverables = [
  "Updated README summary of the feature",
  "Code snippets demonstrating the workflow"
]
```

Specs must include a `goal` plus at least one entry in `tasks` or `deliverables`. The CLI prints a preview before executing the spec with the current agent.

To batch test specs (or just run a smoke check), use the helper script:

```bash
scripts/run_specs.sh            # runs specs/smoke.spec by default
scripts/run_specs.sh specs/     # run every *.spec inside specs/
SPEC_AI_CMD="cargo run --" scripts/run_specs.sh custom.spec
```

The default `specs/smoke.spec` is purposely simple and works against the mock provider so you can verify the CLI still functions after code changes.

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
allowed_tools = ["file_read", "file_write", "bash", "file_extract"]
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
