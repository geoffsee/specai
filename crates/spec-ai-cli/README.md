# spec-ai-cli

Command-line interface and REPL for the spec-ai framework.

## Overview

This crate provides the user-facing CLI and interactive REPL for interacting with AI agents. Features include:

- **Interactive REPL**: Terminal-based chat interface
- **Spec Runner**: Execute structured task specifications
- **Multi-Agent Support**: Switch between different agent profiles
- **Rich Terminal UI**: Markdown rendering, syntax highlighting, and color themes
- **Command System**: Built-in commands for session management

## Features

The CLI supports all core features through feature flags:

### Default Features
- `openai` - OpenAI API integration
- `lmstudio` - LM Studio local models
- `web-scraping` - Web scraping capabilities
- `vttrs` - Video/subtitle processing
- `api` - HTTP API server integration

### Optional Features
- `anthropic` - Anthropic Claude API
- `ollama` - Ollama local models
- `mlx` - Apple MLX framework
- `bundled` - Bundled DuckDB (recommended)
- `duck-sys` - System DuckDB library

## Installation

```bash
# Install from source
cargo install spec-ai --features bundled

# Or use cargo-binstall
cargo binstall spec-ai --features bundled
```

## Usage

```bash
# Start interactive session
spec-ai

# Use custom config
spec-ai --config /path/to/config.toml

# Run a spec file
spec-ai run my-task.spec

# Show help
spec-ai --help
```

## REPL Commands

Within the interactive session:

- `/spec run <file>` - Execute a spec file
- `/help` - Show available commands
- `/exit` or `Ctrl+D` - Exit the session

## Structured Specs

Create reusable task specifications in `.spec` files:

```toml
name = "Example Task"
goal = "Demonstrate spec file usage"

tasks = [
  "Step 1: Do something",
  "Step 2: Do something else"
]

deliverables = [
  "Output artifact",
  "Documentation"
]
```

Run with:
```bash
spec-ai run example.spec
```

## Dependencies

This crate depends on:
- `spec-ai-core` - Core agent runtime
- `spec-ai-config` - Configuration management
- `spec-ai-policy` - Policy enforcement
- `spec-ai-api` - API server (optional)

## Development

Run from source:
```bash
cargo run -p spec-ai-cli
```

For detailed documentation, see:
- [`docs/SETUP.md`](../../docs/SETUP.md) - Setup instructions
- [`docs/CONFIGURATION.md`](../../docs/CONFIGURATION.md) - Configuration guide
- Main [spec-ai README](../../README.md)
