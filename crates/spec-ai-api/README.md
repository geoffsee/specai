# spec-ai-api

HTTP API server for the spec-ai framework.

## Overview

This crate provides a REST API and mesh server for remote agent interaction and coordination. It enables:

- **HTTP API**: RESTful endpoints for agent interaction
- **Mesh Server**: Distributed agent coordination and synchronization
- **Remote Access**: Network-based agent communication
- **Session Management**: Multi-session support with isolated contexts

## Technology Stack

Built on modern async Rust web technologies:

- **axum** - Fast, ergonomic web framework
- **tower** - Modular service layers and middleware
- **tokio** - Async runtime
- **serde** - JSON serialization/deserialization

## Features

The API server provides:

- Agent chat endpoints
- Session management
- Tool execution via HTTP
- Real-time streaming responses
- Multi-agent coordination
- CORS support for web clients

## Dependencies

This crate depends on:
- `spec-ai-core` - Core agent runtime (with `api` feature enabled)
- `spec-ai-config` - Configuration management
- `spec-ai-policy` - Policy enforcement for API requests

## Usage

Enable the API server using the `api` feature flag:

```bash
cargo install spec-ai --features api
```

The API server is automatically started when configured in `spec-ai.config.toml`.

## Testing

Run API-specific tests:

```bash
cargo test --features api
```

For end-user documentation, see the main [spec-ai README](../../README.md).
