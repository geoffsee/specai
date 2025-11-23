# spec-ai
### (Experimental)
Agentic AI written in Rust. 

## Documentation

For detailed documentation, see:
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) - System architecture and component overview
- [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) - Complete configuration guide
- [`docs/SETUP.md`](docs/SETUP.md) - Detailed setup instructions
- [`docs/SELF-INIT.md`](docs/SELF-INIT.md) - Bootstrap self-discovery process
- [`docs/VERIFY.md`](docs/VERIFY.md) - Testing and verification guide

Example configurations are available in `examples/configs/`:
- `config.openai.example.toml` - OpenAI provider setup
- `config.lmstudio.toml` - Local LM Studio configuration
- `config.multi_model.example.toml` - Multi-model reasoning setup
- `config.graph.example.toml` - Knowledge graph configuration

Example code demonstrating various features can be found in `examples/code/`.

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

## Quick Start

```shell
cargo binstall spec-ai --features bundled
```

## Container Usage

The project includes a Containerfile for running spec-ai in a containerized environment:

The container image includes all required dependencies (GraalVM, Tesseract, DuckDB) and uses the bundled DuckDB feature.

### Installation

```bash
cargo install spec-ai --features bundled
# OR
cargo binstall spec-ai --features bundled
```

### Configuration

On first run, spec-ai will automatically create a `spec-ai.config.toml` file with default settings in your current directory. You can edit this file to customize your configuration.

Alternatively, place your configuration in `~/.spec-ai/spec-ai.config.toml` for user-wide settings, or use the `--config` flag to specify a custom location.

**Using Custom Config Files:**

```bash
# Specify a custom config file (created with defaults if it doesn't exist)
spec-ai --config /path/to/my-config.toml

# Use different configs for different projects
spec-ai -c ./project-a.toml
spec-ai -c ./project-b.toml
```

### Configuration Precedence

Configuration is loaded in the following order (highest precedence first):

1. **Command-Line Flag** - `--config <PATH>` (if specified)
2. **Environment Variables** - `AGENT_*` prefix (e.g., `AGENT_MODEL_PROVIDER=openai`)
3. **Current Directory** - `./spec-ai.config.toml`
4. **Home Directory** - `~/.spec-ai/spec-ai.config.toml`
5. **Environment Variable** - `CONFIG_PATH` environment variable
6. **Embedded Default** - A default configuration is embedded in the binary and created if no config file exists

### Running

```bash
# Start a chat session
spec-ai

# Start a chat session using the configuration in the specified file
spec-ai --config /path/to/config.toml

# Show help
spec-ai --help
```

**Command-Line Options:**
- `-c, --config <PATH>` - Specify a custom configuration file path
- `-h, --help` - Display usage information

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

To run spec files, use the `spec-ai run` command:

```bash
spec-ai run                      # runs specs/smoke.spec by default
spec-ai run specs/               # run all *.spec files inside specs/
spec-ai run custom.spec          # run a specific spec file
spec-ai run spec1.spec spec2.spec # run multiple spec files
```

The default `specs/smoke.spec` is purposely simple and works against the mock provider so you can verify the CLI still functions after code changes.

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

`prompt_user` is implicitly allowed (unless you add it to `denied_tools`) so agents can always escalate to a human for clarification.

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

Copyright (c) 2025 spec-ai Contributors

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
Create an issue. Open a PR.