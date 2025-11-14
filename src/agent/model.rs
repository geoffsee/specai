//! Model Provider Abstraction Layer
//!
//! This module defines the core traits and types for integrating with various LLM providers.
//! It provides a unified interface that abstracts away provider-specific details.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Configuration for model generation requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    /// Sampling temperature (0.0 - 2.0)
    pub temperature: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Stop sequences
    pub stop_sequences: Option<Vec<String>>,
    /// Top-p sampling
    pub top_p: Option<f32>,
    /// Frequency penalty
    pub frequency_penalty: Option<f32>,
    /// Presence penalty
    pub presence_penalty: Option<f32>,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            max_tokens: Some(2048),
            stop_sequences: None,
            top_p: Some(1.0),
            frequency_penalty: None,
            presence_penalty: None,
        }
    }
}

/// Tool call from a model response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,
    /// Name of the function/tool to call
    pub function_name: String,
    /// Arguments as JSON
    pub arguments: serde_json::Value,
}

/// Response from a model generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    /// Generated content
    pub content: String,
    /// Model used for generation
    pub model: String,
    /// Token usage statistics
    pub usage: Option<TokenUsage>,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Tool calls from the model (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Provider metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// Provider name
    pub name: String,
    /// Supported models
    pub supported_models: Vec<String>,
    /// Supports streaming
    pub supports_streaming: bool,
}

/// Types of model providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Mock,
    #[cfg(feature = "openai")]
    OpenAI,
    #[cfg(feature = "anthropic")]
    Anthropic,
    #[cfg(feature = "ollama")]
    Ollama,
    #[cfg(feature = "mlx")]
    MLX,
}

impl ProviderKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mock" => Some(ProviderKind::Mock),
            #[cfg(feature = "openai")]
            "openai" => Some(ProviderKind::OpenAI),
            #[cfg(feature = "anthropic")]
            "anthropic" => Some(ProviderKind::Anthropic),
            #[cfg(feature = "ollama")]
            "ollama" => Some(ProviderKind::Ollama),
            #[cfg(feature = "mlx")]
            "mlx" => Some(ProviderKind::MLX),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Mock => "mock",
            #[cfg(feature = "openai")]
            ProviderKind::OpenAI => "openai",
            #[cfg(feature = "anthropic")]
            ProviderKind::Anthropic => "anthropic",
            #[cfg(feature = "ollama")]
            ProviderKind::Ollama => "ollama",
            #[cfg(feature = "mlx")]
            ProviderKind::MLX => "mlx",
        }
    }
}

/// Core trait that all model providers must implement
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Generate a response to the given prompt
    async fn generate(&self, prompt: &str, config: &GenerationConfig) -> Result<ModelResponse>;

    /// Stream a response to the given prompt
    async fn stream(
        &self,
        prompt: &str,
        config: &GenerationConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;

    /// Get provider metadata
    fn metadata(&self) -> ProviderMetadata;

    /// Get the provider kind
    fn kind(&self) -> ProviderKind;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_kind_from_str() {
        assert_eq!(ProviderKind::from_str("mock"), Some(ProviderKind::Mock));
        assert_eq!(ProviderKind::from_str("Mock"), Some(ProviderKind::Mock));
        assert_eq!(ProviderKind::from_str("MOCK"), Some(ProviderKind::Mock));
        assert_eq!(ProviderKind::from_str("invalid"), None);
    }

    #[test]
    fn test_provider_kind_as_str() {
        assert_eq!(ProviderKind::Mock.as_str(), "mock");
    }

    #[test]
    fn test_generation_config_default() {
        let config = GenerationConfig::default();
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.max_tokens, Some(2048));
        assert_eq!(config.top_p, Some(1.0));
    }

    #[test]
    fn test_generation_config_serialization() {
        let config = GenerationConfig {
            temperature: Some(0.9),
            max_tokens: Some(1024),
            stop_sequences: Some(vec!["STOP".to_string()]),
            top_p: Some(0.95),
            frequency_penalty: None,
            presence_penalty: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GenerationConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.temperature, deserialized.temperature);
        assert_eq!(config.max_tokens, deserialized.max_tokens);
    }
}
