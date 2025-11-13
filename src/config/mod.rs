pub mod agent;
pub mod cache;
pub mod registry;

use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use self::agent::AgentProfile;
pub use self::cache::ConfigCache;
pub use self::registry::AgentRegistry;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration file not found at {0}")]
    NotFound(PathBuf),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Missing required field: {0}. {1}")]
    MissingRequired(String, String),
}

/// Main application configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub model: ModelConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub agents: HashMap<String, AgentProfile>,

    #[serde(default)]
    pub default_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "DatabaseConfig::default_path")]
    pub path: PathBuf,
}

impl DatabaseConfig {
    fn default_path() -> PathBuf {
        BaseDirs::new()
            .map(|base| base.home_dir().join(".agent_cli").join("agent_data.duckdb"))
            .unwrap_or_else(|| PathBuf::from(".agent_cli/agent_data.duckdb"))
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "ModelConfig::default_provider")]
    pub provider: String,

    #[serde(default)]
    pub model_name: Option<String>,

    #[serde(default)]
    pub embeddings_model: Option<String>,

    #[serde(default)]
    pub api_key_source: Option<String>,

    #[serde(default = "ModelConfig::default_temperature")]
    pub temperature: f32,
}

impl ModelConfig {
    fn default_provider() -> String {
        "mock".to_string()
    }

    fn default_temperature() -> f32 {
        0.7
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: Self::default_provider(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: Self::default_temperature(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "UiConfig::default_theme")]
    pub theme: String,

    #[serde(default = "UiConfig::default_prompt")]
    pub prompt: String,
}

impl UiConfig {
    fn default_theme() -> String {
        "default".to_string()
    }

    fn default_prompt() -> String {
        "> ".to_string()
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Self::default_theme(),
            prompt: Self::default_prompt(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "LoggingConfig::default_level")]
    pub level: String,
}

impl LoggingConfig {
    fn default_level() -> String {
        "info".to_string()
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
        }
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

impl AppConfig {
    /// Load configuration with the following precedence (highest to lowest):
    /// 1. Environment variables with AGENT_ prefix
    /// 2. config.toml in current working directory
    /// 3. ~/.agent_cli/config.toml
    /// 4. Default values
    pub fn load() -> Result<Self> {
        // Start with defaults
        let mut config = Self::default();

        // Try to load from home directory first
        if let Some(base_dirs) = BaseDirs::new() {
            let home_config_path = base_dirs.home_dir().join(".agent_cli").join("config.toml");
            if home_config_path.exists() {
                config = Self::load_from_file(&home_config_path)
                    .context("loading config from home directory")?;
            }
        }

        // Try to load from current working directory (overrides home config)
        let cwd_config_path = PathBuf::from("config.toml");
        if cwd_config_path.exists() {
            config = Self::load_from_file(&cwd_config_path)
                .context("loading config from current directory")?;
        }

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Validate the final config
        config.validate()?;

        Ok(config)
    }

    /// Load configuration from a specific file path
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("reading config file: {}", path.display()))?;

        let mut config: Self = toml::from_str(&contents)
            .with_context(|| format!("parsing config file: {}", path.display()))?;

        // Set defaults for any missing fields
        config.fill_defaults();

        Ok(config)
    }

    /// Apply environment variable overrides with AGENT_ prefix
    pub fn apply_env_overrides(&mut self) {
        // Database overrides
        if let Ok(path) = env::var("AGENT_DB_PATH") {
            self.database.path = PathBuf::from(path);
        }

        // Model overrides
        if let Ok(provider) = env::var("AGENT_MODEL_PROVIDER") {
            self.model.provider = provider;
        }

        if let Ok(model_name) = env::var("AGENT_MODEL_NAME") {
            self.model.model_name = Some(model_name);
        }

        if let Ok(embeddings_model) = env::var("AGENT_EMBEDDINGS_MODEL") {
            self.model.embeddings_model = Some(embeddings_model);
        }

        if let Ok(api_key_source) = env::var("AGENT_API_KEY_SOURCE") {
            self.model.api_key_source = Some(api_key_source);
        }

        if let Ok(temp) = env::var("AGENT_MODEL_TEMPERATURE") {
            if let Ok(temp_float) = temp.parse::<f32>() {
                self.model.temperature = temp_float;
            }
        }

        // UI overrides
        if let Ok(theme) = env::var("AGENT_UI_THEME") {
            self.ui.theme = theme;
        }

        // Logging overrides
        if let Ok(level) = env::var("AGENT_LOG_LEVEL") {
            self.logging.level = level;
        }

        // Default agent override
        if let Ok(agent) = env::var("AGENT_DEFAULT_AGENT") {
            self.default_agent = Some(agent);
        }
    }

    /// Fill in default values for any missing fields
    fn fill_defaults(&mut self) {
        // Defaults are already set via Default trait and serde defaults
    }

    /// Validate the configuration and return actionable errors
    pub fn validate(&self) -> Result<()> {
        // Validate model provider
        let valid_providers = vec!["mock", "openai", "anthropic", "ollama", "mlx"];
        if !valid_providers.contains(&self.model.provider.as_str()) {
            return Err(ConfigError::Invalid(
                format!(
                    "model.provider must be one of: {}. Got: {}. Set via config.toml or AGENT_MODEL_PROVIDER env var.",
                    valid_providers.join(", "),
                    self.model.provider
                )
            ).into());
        }

        // Validate temperature range
        if self.model.temperature < 0.0 || self.model.temperature > 2.0 {
            return Err(ConfigError::Invalid(
                format!(
                    "model.temperature must be between 0.0 and 2.0. Got: {}. Set via config.toml or AGENT_MODEL_TEMPERATURE env var.",
                    self.model.temperature
                )
            ).into());
        }

        // Validate logging level
        let valid_levels = vec!["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.to_lowercase().as_str()) {
            return Err(ConfigError::Invalid(
                format!(
                    "logging.level must be one of: {}. Got: {}. Set via config.toml or AGENT_LOG_LEVEL env var.",
                    valid_levels.join(", "),
                    self.logging.level
                )
            ).into());
        }

        // Validate default agent exists if specified
        if let Some(default_agent) = &self.default_agent {
            if !self.agents.contains_key(default_agent) {
                return Err(ConfigError::Invalid(
                    format!(
                        "default_agent '{}' not found in agents map. Available agents: {}. Set via config.toml or AGENT_DEFAULT_AGENT env var.",
                        default_agent,
                        if self.agents.is_empty() {
                            "none".to_string()
                        } else {
                            self.agents.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        }
                    )
                ).into());
            }
        }

        // Validate each agent profile
        for (name, profile) in &self.agents {
            profile
                .validate()
                .with_context(|| format!("validating agent profile '{}'", name))?;
        }

        Ok(())
    }

    /// Get a summary of the loaded configuration for startup logging
    pub fn summary(&self) -> String {
        let mut lines = vec![
            "Configuration loaded:".to_string(),
            format!("  Database: {}", self.database.path.display()),
            format!("  Model Provider: {}", self.model.provider),
        ];

        if let Some(model_name) = &self.model.model_name {
            lines.push(format!("  Model Name: {}", model_name));
        }

        if let Some(embeddings_model) = &self.model.embeddings_model {
            lines.push(format!("  Embeddings Model: {}", embeddings_model));
        }

        lines.push(format!("  Temperature: {}", self.model.temperature));
        lines.push(format!("  Logging Level: {}", self.logging.level));
        lines.push(format!("  UI Theme: {}", self.ui.theme));

        if !self.agents.is_empty() {
            lines.push(format!("  Agents: {}", self.agents.len()));
            for name in self.agents.keys() {
                lines.push(format!("    - {}", name));
            }
        }

        if let Some(default_agent) = &self.default_agent {
            lines.push(format!("  Default Agent: {}", default_agent));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.model.provider, "mock");
        assert_eq!(config.model.temperature, 0.7);
        assert!(config.model.embeddings_model.is_none());
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.ui.theme, "default");
    }

    #[test]
    fn test_validate_valid_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_provider() {
        let mut config = AppConfig::default();
        config.model.provider = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_temperature() {
        let mut config = AppConfig::default();
        config.model.temperature = 3.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_log_level() {
        let mut config = AppConfig::default();
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_env_override_provider() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::set_var("AGENT_MODEL_PROVIDER", "openai");
        }
        let mut config = AppConfig::default();
        config.apply_env_overrides();
        assert_eq!(config.model.provider, "openai");
        unsafe {
            env::remove_var("AGENT_MODEL_PROVIDER");
        }
    }

    #[test]
    fn test_env_override_temperature() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::set_var("AGENT_MODEL_TEMPERATURE", "0.5");
        }
        let mut config = AppConfig::default();
        config.apply_env_overrides();
        assert_eq!(config.model.temperature, 0.5);
        unsafe {
            env::remove_var("AGENT_MODEL_TEMPERATURE");
        }
    }

    #[test]
    fn test_load_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config_content = r#"
[model]
provider = "openai"
temperature = 0.9

[logging]
level = "debug"
"#;

        fs::write(&config_path, config_content).unwrap();

        let config = AppConfig::load_from_file(&config_path).unwrap();
        assert_eq!(config.model.provider, "openai");
        assert_eq!(config.model.temperature, 0.9);
        assert_eq!(config.logging.level, "debug");
    }
}
