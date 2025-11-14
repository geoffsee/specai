//! Provider Factory
//!
//! Creates model provider instances based on configuration.

use crate::agent::model::{ModelProvider, ProviderKind};
#[cfg(feature = "mlx")]
use crate::agent::providers::MLXProvider;
use crate::agent::providers::MockProvider;
#[cfg(feature = "openai")]
use crate::agent::providers::OpenAIProvider;
use crate::config::{ModelConfig};
use anyhow::{Context, Result, anyhow};
use std::sync::Arc;

/// Create a model provider from configuration
pub fn create_provider(config: &ModelConfig) -> Result<Arc<dyn ModelProvider>> {
    let provider_kind = ProviderKind::from_str(&config.provider)
        .ok_or_else(|| anyhow!("Unknown provider: {}", config.provider))?;

    match provider_kind {
        ProviderKind::Mock => {
            // Create mock provider with optional custom responses
            let provider = if let Some(model_name) = &config.model_name {
                MockProvider::default().with_model_name(model_name.clone())
            } else {
                MockProvider::default()
            };
            Ok(Arc::new(provider))
        }

        #[cfg(feature = "openai")]
        ProviderKind::OpenAI => {
            // Get API key from config
            let api_key = if let Some(source) = &config.api_key_source {
                resolve_api_key(source)?
            } else {
                // Default to OPENAI_API_KEY environment variable
                load_api_key_from_env("OPENAI_API_KEY")?
            };

            // Create OpenAI provider
            let mut provider = OpenAIProvider::with_api_key(api_key);

            // Set model if specified in config
            if let Some(model_name) = &config.model_name {
                provider = provider.with_model(model_name.clone());
            }

            Ok(Arc::new(provider))
        }

        #[cfg(feature = "anthropic")]
        ProviderKind::Anthropic => {
            // TODO: Implement Anthropic provider
            Err(anyhow!("Anthropic provider not yet implemented"))
        }

        #[cfg(feature = "ollama")]
        ProviderKind::Ollama => {
            // TODO: Implement Ollama provider
            Err(anyhow!("Ollama provider not yet implemented"))
        }

        #[cfg(feature = "mlx")]
        ProviderKind::MLX => {
            // MLX requires a model name
            let model_name = config
                .model_name
                .as_ref()
                .ok_or_else(|| anyhow!("MLX provider requires a model_name to be specified"))?;

            // Create MLX provider with default endpoint (localhost:10240)
            // Users can customize this by setting MLX_ENDPOINT environment variable
            let provider = if let Ok(endpoint) = std::env::var("MLX_ENDPOINT") {
                MLXProvider::with_endpoint(endpoint, model_name)
            } else {
                MLXProvider::new(model_name)
            };

            Ok(Arc::new(provider))
        }
    }
}

/// Resolve API key from a source string
///
/// Supports the following formats:
/// - `env:VAR_NAME` - Load from environment variable
/// - `file:PATH` - Load from file
/// - Any other string - Use as-is (direct API key)
pub fn resolve_api_key(source: &str) -> Result<String> {
    if let Some(env_var) = source.strip_prefix("env:") {
        load_api_key_from_env(env_var)
    } else if let Some(path) = source.strip_prefix("file:") {
        load_api_key_from_file(path)
    } else {
        // Treat as direct API key
        Ok(source.to_string())
    }
}

/// Load API key from environment variable
pub fn load_api_key_from_env(env_var: &str) -> Result<String> {
    std::env::var(env_var).context(format!("Environment variable {} not set", env_var))
}

/// Load API key from file
pub fn load_api_key_from_file(path: &str) -> Result<String> {
    // Handle tilde expansion manually
    let expanded_path = if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            std::path::PathBuf::from(home).join(stripped)
        } else {
            std::path::PathBuf::from(path)
        }
    } else {
        std::path::PathBuf::from(path)
    };

    std::fs::read_to_string(&expanded_path)
        .context(format!("Failed to read API key from file: {}", path))
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelConfig;

    #[test]
    fn test_create_mock_provider() {
        let config = ModelConfig {
            provider: "mock".to_string(),
            model_name: Some("test-model".to_string()),
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.8,
        };

        let provider = create_provider(&config).unwrap();
        assert_eq!(provider.kind(), ProviderKind::Mock);
    }

    #[test]
    fn test_create_unknown_provider() {
        let config = ModelConfig {
            provider: "unknown-provider".to_string(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        };

        let result = create_provider(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_api_key_from_env() {
        unsafe {
            std::env::set_var("TEST_API_KEY", "env-key-value");
        }
        let key = load_api_key_from_env("TEST_API_KEY").unwrap();
        assert_eq!(key, "env-key-value");
        unsafe {
            std::env::remove_var("TEST_API_KEY");
        }
    }

    #[test]
    fn test_load_api_key_env_var_missing() {
        let result = load_api_key_from_env("NONEXISTENT_VAR");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_api_key_direct() {
        let key = resolve_api_key("sk-direct-api-key").unwrap();
        assert_eq!(key, "sk-direct-api-key");
    }

    #[test]
    fn test_resolve_api_key_from_env() {
        unsafe {
            std::env::set_var("TEST_RESOLVE_KEY", "env-resolved-value");
        }
        let key = resolve_api_key("env:TEST_RESOLVE_KEY").unwrap();
        assert_eq!(key, "env-resolved-value");
        unsafe {
            std::env::remove_var("TEST_RESOLVE_KEY");
        }
    }

    #[test]
    fn test_resolve_api_key_from_file() {
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_api_key.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "file-api-key-value").unwrap();

        let key = resolve_api_key(&format!("file:{}", file_path.display())).unwrap();
        assert_eq!(key, "file-api-key-value");
    }

    #[test]
    fn test_load_api_key_from_file_with_whitespace() {
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_key_whitespace.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "  api-key-with-spaces  ").unwrap();

        let key = load_api_key_from_file(file_path.to_str().unwrap()).unwrap();
        assert_eq!(key, "api-key-with-spaces");
    }
}
