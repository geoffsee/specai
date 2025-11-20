//! Transcription Provider Factory
//!
//! Creates transcription provider instances based on configuration.

use crate::agent::transcription::{TranscriptionProvider, TranscriptionProviderKind};
use crate::agent::transcription_providers::MockTranscriptionProvider;
#[cfg(feature = "vttrs")]
use crate::agent::transcription_providers::VttRsProvider;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for transcription providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionProviderConfig {
    /// Provider type (mock, vttrs, etc.)
    pub provider: String,
    /// Optional API key source (env:VAR_NAME, file:PATH, or direct key)
    pub api_key_source: Option<String>,
    /// Optional custom endpoint
    pub endpoint: Option<String>,
    /// Use on-device transcription (offline mode)
    #[serde(default)]
    pub on_device: bool,
    /// Provider-specific settings
    #[serde(default)]
    pub settings: serde_json::Value,
}

impl Default for TranscriptionProviderConfig {
    fn default() -> Self {
        Self {
            provider: "mock".to_string(),
            api_key_source: None,
            endpoint: None,
            on_device: false,
            settings: serde_json::Value::Null,
        }
    }
}

/// Create a transcription provider from configuration
pub fn create_transcription_provider(
    config: &TranscriptionProviderConfig,
) -> Result<Arc<dyn TranscriptionProvider>> {
    let provider_kind = TranscriptionProviderKind::from_str(&config.provider)
        .ok_or_else(|| anyhow!("Unknown transcription provider: {}", config.provider))?;

    match provider_kind {
        TranscriptionProviderKind::Mock => {
            // Create mock provider
            let provider = MockTranscriptionProvider::new();
            Ok(Arc::new(provider))
        }

        #[cfg(feature = "vttrs")]
        TranscriptionProviderKind::VttRs => {
            // On-device mode doesn't require API key
            let api_key = if config.on_device {
                String::new() // Empty API key for on-device mode
            } else if let Some(source) = &config.api_key_source {
                resolve_api_key(source)?
            } else {
                // Default to OPENAI_API_KEY or VTT_API_KEY environment variable
                std::env::var("OPENAI_API_KEY")
                    .or_else(|_| std::env::var("VTT_API_KEY"))
                    .unwrap_or_default()
            };

            // Create VTT-RS provider
            let mut provider = VttRsProvider::new(api_key);

            // Set custom endpoint if specified
            if let Some(endpoint) = &config.endpoint {
                provider = provider.with_endpoint(endpoint.clone());
            }

            // Set on-device mode if enabled
            if config.on_device {
                provider = provider.with_on_device(true);
            }

            Ok(Arc::new(provider))
        }
    }
}

/// Create a transcription provider with just a provider kind string (for convenience)
pub fn create_transcription_provider_simple(
    provider_kind: &str,
) -> Result<Arc<dyn TranscriptionProvider>> {
    let config = TranscriptionProviderConfig {
        provider: provider_kind.to_string(),
        ..Default::default()
    };
    create_transcription_provider(&config)
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

    #[test]
    fn test_create_mock_provider() {
        let config = TranscriptionProviderConfig {
            provider: "mock".to_string(),
            ..Default::default()
        };

        let provider = create_transcription_provider(&config).unwrap();
        assert_eq!(provider.kind(), TranscriptionProviderKind::Mock);
    }

    #[test]
    fn test_create_unknown_provider() {
        let config = TranscriptionProviderConfig {
            provider: "unknown-provider".to_string(),
            ..Default::default()
        };

        let result = create_transcription_provider(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_simple_mock_provider() {
        let provider = create_transcription_provider_simple("mock").unwrap();
        assert_eq!(provider.kind(), TranscriptionProviderKind::Mock);
    }

    #[test]
    fn test_load_api_key_from_env() {
        unsafe {
            std::env::set_var("TEST_TRANSCRIPTION_API_KEY", "env-key-value");
        }
        let key = load_api_key_from_env("TEST_TRANSCRIPTION_API_KEY").unwrap();
        assert_eq!(key, "env-key-value");
        unsafe {
            std::env::remove_var("TEST_TRANSCRIPTION_API_KEY");
        }
    }

    #[test]
    fn test_resolve_api_key_direct() {
        let key = resolve_api_key("sk-direct-api-key").unwrap();
        assert_eq!(key, "sk-direct-api-key");
    }

    #[test]
    fn test_resolve_api_key_from_env() {
        unsafe {
            std::env::set_var("TEST_RESOLVE_TRANSCRIPTION_KEY", "env-resolved-value");
        }
        let key = resolve_api_key("env:TEST_RESOLVE_TRANSCRIPTION_KEY").unwrap();
        assert_eq!(key, "env-resolved-value");
        unsafe {
            std::env::remove_var("TEST_RESOLVE_TRANSCRIPTION_KEY");
        }
    }

    #[test]
    fn test_config_default() {
        let config = TranscriptionProviderConfig::default();
        assert_eq!(config.provider, "mock");
        assert!(config.api_key_source.is_none());
        assert!(config.endpoint.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = TranscriptionProviderConfig {
            provider: "vttrs".to_string(),
            api_key_source: Some("env:OPENAI_API_KEY".to_string()),
            endpoint: Some("https://api.openai.com".to_string()),
            on_device: false,
            settings: serde_json::json!({"custom": "value"}),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TranscriptionProviderConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.provider, deserialized.provider);
        assert_eq!(config.api_key_source, deserialized.api_key_source);
        assert_eq!(config.endpoint, deserialized.endpoint);
        assert_eq!(config.on_device, deserialized.on_device);
    }
}
