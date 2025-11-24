//! Application-level configuration
//!
//! Defines the top-level application configuration, including model settings,
//! database configuration, UI preferences, and logging.

use crate::config::agent::AgentProfile;
use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Embedded default configuration file
const DEFAULT_CONFIG: &str = include_str!("../../spec-ai.config.toml");

/// Configuration file name
const CONFIG_FILE_NAME: &str = "spec-ai.config.toml";

/// Top-level application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,
    /// Model provider configuration
    #[serde(default)]
    pub model: ModelConfig,
    /// UI configuration
    #[serde(default)]
    pub ui: UiConfig,
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
    /// Audio transcription configuration
    #[serde(default)]
    pub audio: AudioConfig,
    /// Mesh networking configuration
    #[serde(default)]
    pub mesh: MeshConfig,
    /// Available agent profiles
    #[serde(default)]
    pub agents: HashMap<String, AgentProfile>,
    /// Default agent to use (if not specified)
    #[serde(default)]
    pub default_agent: Option<String>,
}

impl AppConfig {
    /// Load configuration from file or create a default configuration
    pub fn load() -> Result<Self> {
        // Try to load from spec-ai.config.toml in current directory
        if let Ok(content) = std::fs::read_to_string(CONFIG_FILE_NAME) {
            return toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", CONFIG_FILE_NAME, e));
        }

        // Try to load from ~/.spec-ai/spec-ai.config.toml
        if let Ok(base_dirs) =
            BaseDirs::new().ok_or(anyhow::anyhow!("Could not determine home directory"))
        {
            let home_config = base_dirs.home_dir().join(".spec-ai").join(CONFIG_FILE_NAME);
            if let Ok(content) = std::fs::read_to_string(&home_config) {
                return toml::from_str(&content).map_err(|e| {
                    anyhow::anyhow!("Failed to parse {}: {}", home_config.display(), e)
                });
            }
        }

        // Try to load from environment variable CONFIG_PATH
        if let Ok(config_path) = std::env::var("CONFIG_PATH") {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                return toml::from_str(&content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e));
            }
        }

        // No config file found - create one from embedded default
        eprintln!(
            "No configuration file found. Creating {} with default settings...",
            CONFIG_FILE_NAME
        );
        if let Err(e) = std::fs::write(CONFIG_FILE_NAME, DEFAULT_CONFIG) {
            eprintln!("Warning: Could not create {}: {}", CONFIG_FILE_NAME, e);
            eprintln!("Continuing with default configuration in memory.");
        } else {
            eprintln!(
                "Created {}. You can edit this file to customize your settings.",
                CONFIG_FILE_NAME
            );
        }

        // Parse and return the embedded default config
        toml::from_str(DEFAULT_CONFIG)
            .map_err(|e| anyhow::anyhow!("Failed to parse embedded default config: {}", e))
    }

    /// Load configuration from a specific file path
    /// If the file doesn't exist, creates it with default settings
    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        // Try to read existing file
        match std::fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content).map_err(|e| {
                anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e)
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist - create it with default config
                eprintln!(
                    "Configuration file not found at {}. Creating with default settings...",
                    path.display()
                );

                // Create parent directories if needed
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .context(format!("Failed to create directory {}", parent.display()))?;
                }

                // Write default config
                std::fs::write(path, DEFAULT_CONFIG).context(format!(
                    "Failed to create config file at {}",
                    path.display()
                ))?;

                eprintln!(
                    "Created {}. You can edit this file to customize your settings.",
                    path.display()
                );

                // Parse and return the embedded default config
                toml::from_str(DEFAULT_CONFIG)
                    .map_err(|e| anyhow::anyhow!("Failed to parse embedded default config: {}", e))
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failed to read config file {}: {}",
                path.display(),
                e
            )),
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate model provider: must be non-empty and supported
        if self.model.provider.is_empty() {
            return Err(anyhow::anyhow!("Model provider cannot be empty"));
        }
        // Validate against known provider names independent of compile-time feature flags
        {
            let p = self.model.provider.to_lowercase();
            let known = ["mock", "openai", "anthropic", "ollama", "mlx", "lmstudio"];
            if !known.contains(&p.as_str()) {
                return Err(anyhow::anyhow!(
                    "Invalid model provider: {}",
                    self.model.provider
                ));
            }
        }

        // Validate temperature
        if self.model.temperature < 0.0 || self.model.temperature > 2.0 {
            return Err(anyhow::anyhow!(
                "Temperature must be between 0.0 and 2.0, got {}",
                self.model.temperature
            ));
        }

        // Validate log level
        match self.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            _ => return Err(anyhow::anyhow!("Invalid log level: {}", self.logging.level)),
        }

        // If a default agent is specified, it must exist in the agents map
        if let Some(default_agent) = &self.default_agent {
            if !self.agents.contains_key(default_agent) {
                return Err(anyhow::anyhow!(
                    "Default agent '{}' not found in agents map",
                    default_agent
                ));
            }
        }

        Ok(())
    }

    /// Apply environment variable overrides to the configuration
    pub fn apply_env_overrides(&mut self) {
        // Helper: prefer AGENT_* over SPEC_AI_* if both present
        fn first(a: &str, b: &str) -> Option<String> {
            std::env::var(a).ok().or_else(|| std::env::var(b).ok())
        }

        if let Some(provider) = first("AGENT_MODEL_PROVIDER", "SPEC_AI_PROVIDER") {
            self.model.provider = provider;
        }
        if let Some(model_name) = first("AGENT_MODEL_NAME", "SPEC_AI_MODEL") {
            self.model.model_name = Some(model_name);
        }
        if let Some(api_key_source) = first("AGENT_API_KEY_SOURCE", "SPEC_AI_API_KEY_SOURCE") {
            self.model.api_key_source = Some(api_key_source);
        }
        if let Some(temp_str) = first("AGENT_MODEL_TEMPERATURE", "SPEC_AI_TEMPERATURE") {
            if let Ok(temp) = temp_str.parse::<f32>() {
                self.model.temperature = temp;
            }
        }
        if let Some(level) = first("AGENT_LOG_LEVEL", "SPEC_AI_LOG_LEVEL") {
            self.logging.level = level;
        }
        if let Some(db_path) = first("AGENT_DB_PATH", "SPEC_AI_DB_PATH") {
            self.database.path = PathBuf::from(db_path);
        }
        if let Some(theme) = first("AGENT_UI_THEME", "SPEC_AI_UI_THEME") {
            self.ui.theme = theme;
        }
        if let Some(default_agent) = first("AGENT_DEFAULT_AGENT", "SPEC_AI_DEFAULT_AGENT") {
            self.default_agent = Some(default_agent);
        }
    }

    /// Get a summary of the configuration
    pub fn summary(&self) -> String {
        let mut summary = String::new();
        summary.push_str("Configuration loaded:\n");
        summary.push_str(&format!("Database: {}\n", self.database.path.display()));
        summary.push_str(&format!("Model Provider: {}\n", self.model.provider));
        if let Some(model) = &self.model.model_name {
            summary.push_str(&format!("Model Name: {}\n", model));
        }
        summary.push_str(&format!("Temperature: {}\n", self.model.temperature));
        summary.push_str(&format!("Logging Level: {}\n", self.logging.level));
        summary.push_str(&format!("UI Theme: {}\n", self.ui.theme));
        summary.push_str(&format!("Available Agents: {}\n", self.agents.len()));
        if let Some(default) = &self.default_agent {
            summary.push_str(&format!("Default Agent: {}\n", default));
        }
        summary
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the database file
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("spec-ai.duckdb"),
        }
    }
}

/// Model provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Provider name (e.g., "openai", "anthropic", "mlx", "lmstudio", "mock")
    pub provider: String,
    /// Model name to use (e.g., "gpt-4", "claude-3-opus")
    #[serde(default)]
    pub model_name: Option<String>,
    /// Embeddings model name (optional, for semantic search)
    #[serde(default)]
    pub embeddings_model: Option<String>,
    /// API key source (e.g., environment variable name or path)
    #[serde(default)]
    pub api_key_source: Option<String>,
    /// Default temperature for model completions (0.0 to 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_temperature() -> f32 {
    0.7
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "mock".to_string(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: default_temperature(),
        }
    }
}

/// UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Command prompt string
    pub prompt: String,
    /// UI theme name
    pub theme: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            prompt: "> ".to_string(),
            theme: "default".to_string(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

/// Mesh networking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Enable mesh networking
    #[serde(default)]
    pub enabled: bool,
    /// Registry port for mesh coordination
    #[serde(default = "default_registry_port")]
    pub registry_port: u16,
    /// Heartbeat interval in seconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
    /// Leader timeout in seconds (how long before new election)
    #[serde(default = "default_leader_timeout")]
    pub leader_timeout_secs: u64,
    /// Replication factor for knowledge graph
    #[serde(default = "default_replication_factor")]
    pub replication_factor: usize,
    /// Auto-join mesh on startup
    #[serde(default)]
    pub auto_join: bool,
}

fn default_registry_port() -> u16 {
    3000
}

fn default_heartbeat_interval() -> u64 {
    5
}

fn default_leader_timeout() -> u64 {
    15
}

fn default_replication_factor() -> usize {
    2
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            registry_port: default_registry_port(),
            heartbeat_interval_secs: default_heartbeat_interval(),
            leader_timeout_secs: default_leader_timeout(),
            replication_factor: default_replication_factor(),
            auto_join: true,
        }
    }
}

/// Audio transcription configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Enable audio transcription
    #[serde(default)]
    pub enabled: bool,
    /// Transcription provider (mock, vttrs)
    #[serde(default = "default_transcription_provider")]
    pub provider: String,
    /// Transcription model (e.g., "whisper-1", "whisper-large-v3")
    #[serde(default)]
    pub model: Option<String>,
    /// API key source for cloud transcription
    #[serde(default)]
    pub api_key_source: Option<String>,
    /// Use on-device transcription (offline mode)
    #[serde(default)]
    pub on_device: bool,
    /// Custom API endpoint (optional)
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Audio chunk duration in seconds
    #[serde(default = "default_chunk_duration")]
    pub chunk_duration_secs: f64,
    /// Default transcription duration in seconds
    #[serde(default = "default_duration")]
    pub default_duration_secs: u64,
    /// Default transcription duration in seconds (legacy field name)
    #[serde(default = "default_duration")]
    pub default_duration: u64,
    /// Output file path for transcripts (optional)
    #[serde(default)]
    pub out_file: Option<String>,
    /// Language code (e.g., "en", "es", "fr")
    #[serde(default)]
    pub language: Option<String>,
    /// Whether to automatically respond to transcriptions
    #[serde(default)]
    pub auto_respond: bool,
    /// Mock scenario for testing (e.g., "simple_conversation", "emotional_context")
    #[serde(default = "default_mock_scenario")]
    pub mock_scenario: String,
    /// Delay between mock transcription events in milliseconds
    #[serde(default = "default_event_delay_ms")]
    pub event_delay_ms: u64,
}

fn default_transcription_provider() -> String {
    "vttrs".to_string()
}

fn default_chunk_duration() -> f64 {
    5.0
}

fn default_duration() -> u64 {
    30
}

fn default_mock_scenario() -> String {
    "simple_conversation".to_string()
}

fn default_event_delay_ms() -> u64 {
    500
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_transcription_provider(),
            model: Some("whisper-1".to_string()),
            api_key_source: None,
            on_device: false,
            endpoint: None,
            chunk_duration_secs: default_chunk_duration(),
            default_duration_secs: default_duration(),
            default_duration: default_duration(),
            out_file: None,
            language: None,
            auto_respond: false,
            mock_scenario: default_mock_scenario(),
            event_delay_ms: default_event_delay_ms(),
        }
    }
}
