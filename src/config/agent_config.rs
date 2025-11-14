//! Application-level configuration
//!
//! Defines the top-level application configuration, including model settings,
//! database configuration, UI preferences, and logging.

use crate::config::agent::AgentProfile;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        // Try to load from config.toml in current directory
        if let Ok(content) = std::fs::read_to_string("config.toml") {
            return toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse config.toml: {}", e));
        }

        // Try to load from environment variable CONFIG_PATH
        if let Ok(config_path) = std::env::var("CONFIG_PATH") {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                return toml::from_str(&content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e));
            }
        }

        // Return default configuration
        Ok(Self::default())
    }

    /// Load configuration from a specific file path
    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))
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
            let known = ["mock", "openai", "anthropic", "ollama", "mlx"];
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
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid log level: {}",
                    self.logging.level
                ))
            }
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
            std::env::var(a)
                .ok()
                .or_else(|| std::env::var(b).ok())
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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            model: ModelConfig::default(),
            ui: UiConfig::default(),
            logging: LoggingConfig::default(),
            agents: HashMap::new(),
            default_agent: None,
        }
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
    /// Provider name (e.g., "openai", "anthropic", "mlx", "mock")
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
