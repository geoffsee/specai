# spec-ai-core

Core functionality for the spec-ai framework.

## Overview

This crate provides the foundational components for building AI agents, including:

- **Agent Runtime**: Core agent execution engine and lifecycle management
- **Tool System**: Extensible tool framework for agent capabilities
- **Embeddings**: Vector embeddings for semantic search and similarity
- **Provider Integrations**: Support for multiple LLM providers
- **CLI Helpers**: Terminal UI components and utilities

## Features

The crate supports multiple LLM providers and capabilities through feature flags:

### LLM Providers
- `openai` - OpenAI API integration
- `anthropic` - Anthropic Claude API integration
- `ollama` - Ollama local model support
- `mlx` - Apple MLX framework integration
- `lmstudio` - LM Studio local model support

### Additional Features
- `vttrs` - Video/subtitle processing support
- `web-scraping` - Web scraping capabilities via Spider
- `api` - HTTP API functionality
- `integration-tests` - Integration test support

## Dependencies

This crate depends on:
- `spec-ai-config` - Configuration management
- `spec-ai-knowledge-graph` - Knowledge graph storage and types
- `spec-ai-policy` - Policy enforcement

## Platform-Specific Behavior

On non-macOS platforms, the `extractous` dependency is included for document extraction using GraalVM/Tika. This is excluded on macOS due to AWT compatibility issues.

## Usage

This is an internal crate primarily used by:
- `spec-ai-cli` - The command-line interface
- `spec-ai-api` - The HTTP API server
- `spec-ai` - The public library crate

For end-user documentation, see the main [spec-ai README](../../README.md).