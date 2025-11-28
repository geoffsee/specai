# spec-ai

Public library crate for the spec-ai framework.

## Overview

This is the main library crate that re-exports all public APIs from the spec-ai workspace crates. It provides a unified interface for building AI agent applications.

## Features

### Default Features
- `openai` - OpenAI API integration
- `lmstudio` - LM Studio local models
- `web-scraping` - Web scraping capabilities
- `vttrs` - Video/subtitle processing
- `api` - HTTP API server
- `cli` - Command-line interface

### Optional Features

**LLM Providers:**
- `anthropic` - Anthropic Claude API
- `ollama` - Ollama local models
- `mlx` - Apple MLX framework

**Database:**
- `bundled` - Bundled DuckDB library (recommended)
- `duck-sys` - System DuckDB library

**Other:**
- `integration-tests` - Enable integration tests
- `axum-extra` - Additional Axum web framework features

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
spec-ai = "0.5"
```

Or install the CLI:

```bash
cargo install spec-ai --features bundled
```

## Usage

### As a Library

```rust
use spec_ai::prelude::*;

// Your agent application code here
```

### As a Binary

This crate also provides the `spec-ai` binary:

```bash
# Start interactive session
spec-ai

# Run a spec file
spec-ai run task.spec

# Use custom config
spec-ai --config custom.toml
```

## Workspace Structure

This crate re-exports functionality from:

- **spec-ai-core** - Agent runtime, tools, embeddings
- **spec-ai-config** - Configuration management and persistence
- **spec-ai-knowledge-graph** - Knowledge graph storage, vector clocks, types
- **spec-ai-policy** - Policy engine and plugin system
- **spec-ai-api** - HTTP API server
- **spec-ai-cli** - Command-line interface

## Documentation

For detailed documentation, see:

- [Main README](../../README.md) - Overview and quick start
- [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) - System architecture
- [`docs/CONFIGURATION.md`](../../docs/CONFIGURATION.md) - Configuration guide
- [`docs/SETUP.md`](../../docs/SETUP.md) - Setup instructions

## Examples

Example configurations and code can be found in the repository:

- `examples/configs/` - Configuration examples
- `examples/code/` - Code examples
- `specs/` - Example spec files

## License

MIT License - see [LICENSE](../../LICENSE) or the main README for details.

## Contributing

Create an issue or open a PR at the [spec-ai repository](https://github.com/geoffsee/spec-ai).
