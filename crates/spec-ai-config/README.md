# spec-ai-config

Configuration management for the spec-ai framework.

## Overview

This crate provides configuration models, persistence layer, and shared types for the spec-ai framework. It handles:

- **Configuration Models**: Type-safe configuration structures with validation
- **Persistence Layer**: DuckDB-based storage for agent data and session history
- **Shared Types**: Common types used across the framework
- **Configuration Loading**: Multi-source configuration with precedence handling

## Features

### Database Support
- `bundled` - Use bundled DuckDB library (recommended for most users)
- `duck-sys` - Use system-installed DuckDB

## Configuration Sources

The framework loads configuration from multiple sources with the following precedence (highest first):

1. Command-line flags
2. Environment variables (with `AGENT_*` prefix)
3. Current directory (`./spec-ai.config.toml`)
4. Home directory (`~/.spec-ai/spec-ai.config.toml`)
5. `CONFIG_PATH` environment variable
6. Embedded default configuration

## Storage

Configuration data is persisted to DuckDB, providing:
- Fast query performance
- ACID compliance
- SQL-like query capabilities
- Efficient storage for large datasets

Default database location: `~/.agent_cli/agent_data.duckdb`

## Usage

This is an internal crate used by all other spec-ai crates for configuration management.

For configuration documentation, see [`docs/CONFIGURATION.md`](../../docs/CONFIGURATION.md) in the main repository.

For end-user documentation, see the main [spec-ai README](../../README.md).