# spec-ai Configuration Guide

This document describes all available configuration options for spec-ai. Configuration can be provided through multiple sources with a clear precedence hierarchy.

## Table of Contents

1. [Configuration Sources & Precedence](#configuration-sources--precedence)
2. [Configuration File Format](#configuration-file-format)
3. [Global Configuration](#global-configuration)
   - [Database Configuration](#database-configuration)
   - [Model Configuration](#model-configuration)
   - [UI Configuration](#ui-configuration)
   - [Logging Configuration](#logging-configuration)
   - [Audio Configuration](#audio-configuration)
4. [Agent Profiles](#agent-profiles)
   - [Basic Settings](#basic-settings)
   - [Tool Permissions](#tool-permissions)
   - [Memory Configuration](#memory-configuration)
   - [Knowledge Graph Features](#knowledge-graph-features)
   - [Multi-Model Reasoning](#multi-model-reasoning)
   - [Audio Transcription](#audio-transcription)
5. [Service Mesh Configuration](#service-mesh-configuration)
   - [Mesh Registry](#mesh-registry)
   - [Instance Registration](#instance-registration)
   - [Message Bus](#message-bus)
6. [Graph Synchronization](#graph-synchronization)
   - [Sync Coordinator](#sync-coordinator)
   - [Sync Strategy](#sync-strategy)
   - [Conflict Resolution](#conflict-resolution)
7. [Environment Variables](#environment-variables)
8. [Command-Line Arguments](#command-line-arguments)
9. [Example Configurations](#example-configurations)

## Configuration Sources & Precedence

Configuration is loaded in the following order (highest to lowest priority):

1. **Command-line arguments** (`--config /path/to/config`)
2. **Environment variables** (e.g., `AGENT_MODEL_PROVIDER`, `SPEC_AI_PROVIDER`)
3. **Current directory config** (`./spec-ai.config.toml`)
4. **Home directory config** (`~/.spec-ai/spec-ai.config.toml`)
5. **Embedded default config** (built into the binary)

### File Locations

- **Primary location**: `spec-ai.config.toml` in current directory
- **User location**: `~/.spec-ai/spec-ai.config.toml`
- **Custom location**: Via `--config` flag or `CONFIG_PATH` environment variable

If no configuration file exists, spec-ai will automatically create one with sensible defaults.

## Configuration File Format

Configuration files use TOML format. The basic structure is:

```toml
# Global settings
default_agent = "default"

[database]
# Database configuration

[model]
# Model provider configuration

[ui]
# UI configuration

[logging]
# Logging configuration

[audio]
# Audio transcription configuration

[agents.agent_name]
# Agent-specific configuration
```

## Global Configuration

### Default Agent

```toml
# Specifies which agent profile to use by default
default_agent = "default"  # Must match a key in the agents table
```

### Database Configuration

```toml
[database]
# Path to the DuckDB database file
# Supports ~ for home directory expansion
path = "spec-ai.duckdb"  # Default: "spec-ai.duckdb"
```

### Model Configuration

```toml
[model]
# Model provider to use
# Options: "mock", "openai", "anthropic", "ollama", "mlx", "lmstudio"
provider = "openai"  # Required

# Model name to use (provider-specific)
# OpenAI: "gpt-4", "gpt-4-turbo", "gpt-3.5-turbo"
# Anthropic: "claude-3-opus", "claude-3-sonnet", "claude-3-haiku"
# Ollama: Any locally available model
# MLX: Any MLX-compatible model
# LMStudio: Any model served by LM Studio
model_name = "gpt-4"  # Optional, provider-specific default used if not set

# Embeddings model for semantic search
# OpenAI: "text-embedding-3-small", "text-embedding-3-large", "text-embedding-ada-002"
embeddings_model = "text-embedding-3-small"  # Optional

# API key source
# Formats:
#   "env:VARIABLE_NAME" - Read from environment variable
#   "file:/path/to/file" - Read from file
#   "direct:actual_key" - Direct key (not recommended)
api_key_source = "env:OPENAI_API_KEY"  # Optional

# Temperature for model completions
# Range: 0.0 (deterministic) to 2.0 (very creative)
temperature = 0.7  # Default: 0.7
```

### UI Configuration

```toml
[ui]
# Command prompt string displayed in REPL
prompt = "> "  # Default: "> "

# UI theme
# Options: "default", "dark", "light"
theme = "default"  # Default: "default"
```

### Logging Configuration

```toml
[logging]
# Log level for application
# Options: "trace", "debug", "info", "warn", "error"
level = "info"  # Default: "info"
```

### Audio Configuration

```toml
[audio]
# Enable audio transcription globally
enabled = false  # Default: false

# Transcription provider
# Options: "mock" (for testing), "vttrs" (real transcription)
provider = "vttrs"  # Default: "vttrs"

# Transcription model
# OpenAI: "whisper-1"
# Local: "whisper-large-v3", etc.
model = "whisper-1"  # Default: "whisper-1"

# API key source for cloud transcription
# Same format as model.api_key_source
api_key_source = "env:OPENAI_API_KEY"  # Optional

# Use on-device (offline) transcription
# Requires local Whisper model
on_device = false  # Default: false

# Custom API endpoint (for alternative providers)
endpoint = "https://api.openai.com/v1"  # Optional

# Audio chunk duration in seconds
# How often to send audio for transcription
chunk_duration_secs = 5.0  # Default: 5.0

# Default transcription duration in seconds
# Used by /listen command
default_duration_secs = 30  # Default: 30

# Output file path for saving transcripts
out_file = "~/transcripts/recording.txt"  # Optional

# Language code for transcription
# Examples: "en", "es", "fr", "de", "ja", "zh"
language = "en"  # Optional, auto-detect if not set

# Automatically respond to transcriptions with AI
auto_respond = false  # Default: false

# Mock scenario for testing (when provider = "mock")
# Options: "simple_conversation", "emotional_context", etc.
mock_scenario = "simple_conversation"  # Default: "simple_conversation"

# Delay between mock transcription events (milliseconds)
event_delay_ms = 500  # Default: 500
```

## Agent Profiles

Agent profiles define per-agent settings that override global defaults. Define agents under `[agents.agent_name]` sections.

### Basic Settings

```toml
[agents.example]
# System prompt for this agent
# Defines the agent's role and capabilities
prompt = """You are a helpful assistant with expertise in..."""  # Optional

# Conversational style or personality
# Examples: "professional", "casual", "technical", "friendly"
style = "professional"  # Optional

# Temperature override for this agent
# Range: 0.0 to 2.0
temperature = 0.5  # Optional, uses global default if not set

# Model provider override
# Options: same as model.provider
model_provider = "anthropic"  # Optional

# Model name override
# Provider-specific model names
model_name = "claude-3-opus"  # Optional

# Maximum context window size (in tokens)
# Limits the total context sent to the model
max_context_tokens = 8192  # Optional
```

### Tool Permissions

```toml
[agents.example]
# List of tools this agent is allowed to use
# If specified, ONLY these tools are available
allowed_tools = [
    "file_read",
    "file_write",
    "bash",
    "search"
]  # Optional

# List of tools this agent is forbidden from using
# These tools require user approval when attempted
denied_tools = [
    "bash",
    "shell",
    "file_write"
]  # Optional

# Note: "prompt_user" is always allowed unless explicitly denied
# Tools cannot be both allowed and denied
```

### Memory Configuration

```toml
[agents.example]
# Number of messages to recall from history
# Used for semantic memory retrieval
memory_k = 10  # Default: 10

# Top-p sampling parameter for memory recall
# Range: 0.0 to 1.0
# Controls diversity of recalled memories
top_p = 0.9  # Default: 0.9
```

### Knowledge Graph Features

```toml
[agents.example]
# Enable knowledge graph features for this agent
enable_graph = true  # Default: true

# Use graph-based memory recall
# Combines with embeddings for enhanced context
graph_memory = true  # Default: true

# Maximum graph traversal depth for context building
# Higher values explore more distant relationships
graph_depth = 3  # Default: 3

# Weight for graph-based relevance vs semantic similarity
# Range: 0.0 (pure semantic) to 1.0 (pure graph)
graph_weight = 0.5  # Default: 0.5

# Automatically build graph from conversations
# Extracts entities and relationships automatically
auto_graph = true  # Default: true

# Graph-based tool recommendation threshold
# Range: 0.0 to 1.0
# Tools with relevance above this threshold are suggested
graph_threshold = 0.7  # Default: 0.7

# Use graph for decision steering
# Allows graph relationships to influence agent decisions
graph_steering = true  # Default: true
```

### Multi-Model Reasoning

```toml
[agents.example]
# Enable fast reasoning with a smaller model
# Uses a fast model for simple tasks, main model for complex ones
fast_reasoning = true  # Default: true

# Model provider for fast reasoning
# Often uses local models for speed
fast_model_provider = "lmstudio"  # Default: "lmstudio"

# Model name for fast reasoning
# Should be a smaller, faster model
fast_model_name = "lmstudio-community/Llama-3.2-3B-Instruct"  # Default

# Temperature for fast model
# Usually lower for consistency
fast_model_temperature = 0.3  # Default: 0.3

# Tasks to delegate to fast model
# These tasks run 10-15x faster with the small model
fast_model_tasks = [
    "entity_extraction",      # Extract names, dates, URLs
    "graph_analysis",         # Analyze graph relationships
    "decision_routing",       # Determine task complexity
    "tool_selection",        # Choose appropriate tools
    "confidence_scoring",    # Assess response confidence
    "syntax_checking",       # For code agents
    "linting",              # For code agents
    "keyword_extraction",    # For research agents
]  # Default: first 5 tasks

# Confidence threshold to escalate to main model
# Range: 0.0 to 1.0
# If fast model confidence < threshold, use main model
escalation_threshold = 0.6  # Default: 0.6

# Display reasoning summary to user
# Shows a concise summary of the model's thought process
# Requires fast_reasoning = true
show_reasoning = false  # Default: false
```

### Audio Transcription

```toml
[agents.example]
# Enable audio transcription for this agent
enable_audio_transcription = false  # Default: false

# Audio response mode
# Options: "immediate" (respond as audio comes in), "batch" (wait for completion)
audio_response_mode = "immediate"  # Default: "immediate"

# Preferred audio transcription scenario for testing
# Used with mock provider
audio_scenario = "technical_discussion"  # Optional
```

## Service Mesh Configuration

The service mesh enables multiple spec-ai instances to communicate, share knowledge, and coordinate tasks. This is useful for distributed deployments, multi-agent collaboration, and horizontal scaling.

### Mesh Registry

```toml
[mesh]
# Enable service mesh functionality
enabled = false  # Default: false

# Host address for the mesh API server
host = "0.0.0.0"  # Default: "0.0.0.0"

# Port for the mesh API server
port = 8080  # Default: 8080

# Heartbeat interval in seconds
# How often instances report their status to the registry
heartbeat_interval_secs = 30  # Default: 30

# Stale instance timeout in seconds
# Instances without heartbeat for this duration are removed
stale_timeout_secs = 90  # Default: 90

# Enable leader election
# First registered instance becomes leader; automatic failover on leader departure
leader_election = true  # Default: true
```

### Instance Registration

When an instance joins the mesh, it registers with its capabilities and available agent profiles.

```toml
[mesh.instance]
# Custom instance ID (auto-generated if not specified)
# Format: "{hostname}-{uuid}"
instance_id = "my-instance-001"  # Optional

# Capabilities this instance provides
# Used for task routing and delegation
capabilities = [
    "code_analysis",
    "web_scraping",
    "audio_transcription"
]  # Optional

# Agent profiles available on this instance
# Automatically populated from [agents.*] sections if not specified
agent_profiles = ["coder", "researcher"]  # Optional
```

### Message Bus

Inter-instance communication is handled through the message bus.

```toml
[mesh.messaging]
# Enable inter-instance messaging
enabled = true  # Default: true when mesh.enabled = true

# Message types supported:
# - query: Request information from another agent
# - response: Response to a query
# - notification: One-way notification
# - task_delegation: Delegate a task to another agent
# - task_result: Result of a delegated task
# - graph_sync: Knowledge graph synchronization

# Maximum message queue size per instance
max_queue_size = 1000  # Default: 1000

# Message retention period in seconds
message_retention_secs = 3600  # Default: 3600 (1 hour)
```

#### Message Bus API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/registry/register` | POST | Register a new instance |
| `/registry/agents` | GET | List all registered instances |
| `/registry/heartbeat/{instance_id}` | POST | Send heartbeat |
| `/registry/deregister/{instance_id}` | DELETE | Remove an instance |
| `/messages/send/{source_instance}` | POST | Send a message |
| `/messages/{instance_id}` | GET | Get pending messages |
| `/messages/ack/{instance_id}` | POST | Acknowledge messages |

## Graph Synchronization

Knowledge graph synchronization allows multiple spec-ai instances to share and merge their knowledge graphs. This enables collaborative knowledge building across distributed deployments.

### Sync Coordinator

```toml
[sync]
# Enable automatic graph synchronization
enabled = false  # Default: false

# Sync interval in seconds
# How often to check for sync opportunities
sync_interval_secs = 60  # Default: 60

# Maximum concurrent sync operations
# Limits resource usage during synchronization
max_concurrent_syncs = 3  # Default: 3

# Retry interval for failed syncs in seconds
retry_interval_secs = 300  # Default: 300 (5 minutes)

# Maximum retry attempts before giving up
max_retries = 3  # Default: 3
```

### Sync Strategy

The sync engine automatically chooses between full and incremental synchronization based on the amount of changes.

```toml
[sync.strategy]
# Threshold for switching to full sync (percentage)
# If more than this percentage of nodes changed, do a full sync
incremental_threshold = 0.3  # Default: 0.3 (30%)

# Changelog retention period in days
# Tombstones and change records older than this are purged
changelog_retention_days = 7  # Default: 7

# Enable sync for new graphs by default
sync_enabled_by_default = false  # Default: false
```

#### Sync Types

| Type | Description | Use Case |
|------|-------------|----------|
| `Full` | Complete graph snapshot | Initial sync, large changes (>30% churn) |
| `Incremental` | Delta changes only | Regular updates, small changes |

### Conflict Resolution

When concurrent edits occur, the sync engine uses vector clocks and semantic checks to resolve conflicts.

```toml
[sync.conflict_resolution]
# Default resolution strategy
# Options: "last_write_wins", "manual", "merge"
strategy = "last_write_wins"  # Default: "last_write_wins"

# Enable automatic merging for compatible changes
auto_merge = true  # Default: true

# Log conflicts for manual review
log_conflicts = true  # Default: true

# Conflict log retention in days
conflict_log_retention_days = 30  # Default: 30
```

#### Conflict Resolution Outcomes

| Resolution | Description |
|------------|-------------|
| `AcceptRemote` | Use the incoming version |
| `KeepLocal` | Keep the existing local version |
| `Merged` | Combine both versions semantically |
| `RequiresManualReview` | Flag for human intervention |

### Per-Agent Sync Settings

```toml
[agents.example]
# Enable graph sync for this agent's knowledge graph
enable_graph_sync = true  # Default: inherits from sync.sync_enabled_by_default

# Graphs to include in synchronization
# Empty list means sync all graphs
sync_graphs = ["primary", "research"]  # Optional

# Graphs to exclude from synchronization
exclude_from_sync = ["private", "scratch"]  # Optional
```

### Distributed Configuration Example

```toml
# Full distributed deployment with mesh and sync

[mesh]
enabled = true
host = "0.0.0.0"
port = 8080
heartbeat_interval_secs = 30
stale_timeout_secs = 90

[mesh.instance]
capabilities = ["code_analysis", "research", "graph_sync"]

[mesh.messaging]
enabled = true
max_queue_size = 1000

[sync]
enabled = true
sync_interval_secs = 60
max_concurrent_syncs = 3
retry_interval_secs = 300

[sync.strategy]
incremental_threshold = 0.3
changelog_retention_days = 7

[sync.conflict_resolution]
strategy = "last_write_wins"
auto_merge = true
log_conflicts = true

[agents.distributed]
prompt = "You are part of a distributed agent network."
enable_graph = true
enable_graph_sync = true
sync_graphs = ["shared_knowledge"]
```

## Environment Variables

Environment variables override configuration file settings. Two prefixes are supported:
- `AGENT_*` (preferred)
- `SPEC_AI_*` (legacy)

If both are set, `AGENT_*` takes precedence.

### Available Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `AGENT_MODEL_PROVIDER` | Model provider override | `openai` |
| `AGENT_MODEL_NAME` | Model name override | `gpt-4` |
| `AGENT_API_KEY_SOURCE` | API key source override | `env:OPENAI_API_KEY` |
| `AGENT_MODEL_TEMPERATURE` | Temperature override | `0.7` |
| `AGENT_LOG_LEVEL` | Log level override | `debug` |
| `AGENT_DB_PATH` | Database path override | `~/my-agent.db` |
| `AGENT_UI_THEME` | UI theme override | `dark` |
| `AGENT_DEFAULT_AGENT` | Default agent override | `coder` |
| `CONFIG_PATH` | Configuration file path | `/etc/spec-ai/config.toml` |

### API Key Environment Variables

Model providers typically expect API keys in specific environment variables:

| Provider | Environment Variable |
|----------|---------------------|
| OpenAI | `OPENAI_API_KEY` |
| Anthropic | `ANTHROPIC_API_KEY` |
| MLX | Not required (local) |
| Ollama | Not required (local) |
| LMStudio | Not required (local) |

## Command-Line Arguments

### Global Options

```bash
# Specify custom configuration file
spec-ai --config /path/to/config.toml

# Run specific spec files
spec-ai run path/to/spec.spec

# Run all spec files in a directory
spec-ai run spec/

# Run multiple spec files
spec-ai run spec1.spec spec2.spec spec/dir/
```

### Subcommands

#### `run` - Execute spec files

```bash
# Run default spec (spec/smoke.spec)
spec-ai run

# Run specific spec file
spec-ai run my-spec.spec

# Run all spec in directory
spec-ai run spec/

# Run with custom config
spec-ai --config custom.toml run spec/
```

## Example Configurations

### Minimal Configuration

```toml
# Minimal configuration using OpenAI
[model]
provider = "openai"
model_name = "gpt-4"

[agents.default]
prompt = "You are a helpful assistant."
```

### Local Model Configuration

```toml
# Configuration for local models (no API key required)
[model]
provider = "ollama"
model_name = "llama3.2:3b"
temperature = 0.5

[agents.default]
prompt = "You are a helpful local assistant."

# Disable features that require embeddings API
enable_graph = false
graph_memory = false
```

### Multi-Agent Configuration

```toml
default_agent = "general"

[model]
provider = "openai"
model_name = "gpt-4"
embeddings_model = "text-embedding-3-small"
api_key_source = "env:OPENAI_API_KEY"

# General purpose agent
[agents.general]
prompt = "You are a helpful general assistant."
temperature = 0.7
memory_k = 20

# Code-focused agent
[agents.coder]
prompt = "You are an expert programmer who writes clean, efficient code."
temperature = 0.3
allowed_tools = ["file_read", "file_write", "bash", "search"]
fast_reasoning = true
fast_model_tasks = ["syntax_checking", "linting", "import_resolution"]

# Research agent
[agents.researcher]
prompt = "You are a research assistant specialized in gathering and analyzing information."
temperature = 0.5
denied_tools = ["bash", "file_write"]
memory_k = 30
graph_depth = 5
graph_weight = 0.7

# Creative writing agent
[agents.writer]
prompt = "You are a creative writing assistant."
temperature = 1.2
style = "casual"
graph_steering = false  # Less steering for creativity
fast_reasoning = false  # Creativity benefits from main model
```

### Advanced Features Configuration

```toml
[model]
provider = "openai"
model_name = "gpt-4"
embeddings_model = "text-embedding-3-small"

[agents.advanced]
prompt = "You are an AI with advanced reasoning capabilities."

# Full knowledge graph features
enable_graph = true
graph_memory = true
auto_graph = true
graph_steering = true
graph_depth = 4
graph_weight = 0.6
graph_threshold = 0.65

# Multi-model reasoning with LM Studio
fast_reasoning = true
fast_model_provider = "lmstudio"
fast_model_name = "lmstudio-community/Llama-3.2-3B-Instruct"
fast_model_temperature = 0.2
fast_model_tasks = [
    "entity_extraction",
    "graph_analysis",
    "decision_routing",
    "tool_selection",
    "confidence_scoring",
    "metadata_extraction"
]
escalation_threshold = 0.7
show_reasoning = true

# Memory configuration
memory_k = 25
top_p = 0.95
max_context_tokens = 16384
```

### Security-Conscious Configuration

```toml
[model]
provider = "openai"
api_key_source = "file:~/.secrets/openai.key"  # Store key in protected file

[agents.secure]
prompt = "You are a security-conscious assistant."

# Deny all potentially dangerous tools by default
denied_tools = [
    "bash",
    "shell",
    "file_write",
    "file_extract",
    "web_scraper"
]

# Only allow safe read operations
allowed_tools = [
    "file_read",
    "search",
    "prompt_user"
]

# Conservative settings
temperature = 0.3
memory_k = 5
```

## Validation Rules

The configuration system enforces several validation rules:

1. **Temperature**: Must be between 0.0 and 2.0
2. **Top-p**: Must be between 0.0 and 1.0
3. **Graph weight**: Must be between 0.0 and 1.0
4. **Graph threshold**: Must be between 0.0 and 1.0
5. **Escalation threshold**: Must be between 0.0 and 1.0
6. **Model provider**: Must be one of: mock, openai, anthropic, ollama, mlx, lmstudio
7. **Log level**: Must be one of: trace, debug, info, warn, error
8. **Tool permissions**: A tool cannot be both allowed and denied
9. **Default agent**: Must exist in the agents table if specified
10. **Audio provider**: Must be one of: mock, vttrs

## Configuration Tips

1. **Start simple**: Begin with minimal configuration and add features as needed
2. **Use agent profiles**: Create specialized agents for different tasks
3. **Secure API keys**: Use environment variables or protected files, never commit keys
4. **Test locally**: Use mock or local providers for testing without API costs
5. **Monitor logs**: Set appropriate log levels for debugging vs production
6. **Profile memory**: Adjust memory_k based on conversation complexity
7. **Tune graph settings**: Experiment with graph_weight and graph_depth for your use case
8. **Optimize fast models**: Choose fast_model_tasks based on your workload
9. **Version control**: Keep configuration files in version control (without secrets)
10. **Document changes**: Comment your configuration to explain custom settings