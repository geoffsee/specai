# spec-ai-policy

Policy enforcement and plugin system for the spec-ai framework.

## Overview

This crate provides the policy engine that controls and restricts agent behavior through:

- **Policy Engine**: Rule-based enforcement of agent capabilities and permissions
- **Plugin System**: Extensible architecture for adding custom policies
- **Tool Restrictions**: Control which tools agents can access
- **Agent Profiles**: Different permission sets for different agent types

## Features

The policy system enables:

- **Tool Allowlists**: Explicitly allow specific tools per agent profile
- **Tool Denylists**: Block specific tools from being used
- **Memory Limits**: Control conversation history retention (`memory_k`)
- **Temperature Controls**: Enforce temperature ranges per agent
- **Custom Policies**: Extend with custom policy plugins

## Agent Profiles

Define agents with different capabilities through policy configuration:

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

The `prompt_user` tool is implicitly allowed unless explicitly denied, ensuring agents can always escalate to humans for clarification.

## Dependencies

This crate depends on:
- `spec-ai-config` - Configuration management

## Usage

This is an internal crate used by:
- `spec-ai-core` - For enforcing policies during agent execution
- `spec-ai-api` - For API-level policy enforcement
- `spec-ai-cli` - For CLI command restrictions

For end-user documentation, see the main [spec-ai README](../../README.md).
