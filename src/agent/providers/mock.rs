//! Mock Model Provider
//!
//! A simple mock provider for testing that returns canned responses.

use crate::agent::model::{
    GenerationConfig, ModelProvider, ModelResponse, ProviderKind, ProviderMetadata, TokenUsage,
};
use anyhow::Result;
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// Mock provider that returns predefined responses
#[derive(Debug, Clone)]
pub struct MockProvider {
    /// Canned responses to cycle through
    responses: Vec<String>,
    /// Current response index
    current_index: std::sync::Arc<std::sync::Mutex<usize>>,
    /// Model name to report
    model_name: String,
}

impl MockProvider {
    /// Create a new mock provider with a single response
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            responses: vec![response.into()],
            current_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            model_name: "mock-model".to_string(),
        }
    }

    /// Create a new mock provider with multiple responses
    pub fn with_responses(responses: Vec<String>) -> Self {
        Self {
            responses,
            current_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            model_name: "mock-model".to_string(),
        }
    }

    /// Set the model name
    pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = model_name.into();
        self
    }

    /// Get the next response (cycles through available responses)
    fn next_response(&self) -> String {
        let mut index = self.current_index.lock().unwrap();
        let response = self.responses[*index % self.responses.len()].clone();
        *index += 1;
        response
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new("This is a mock response from the test provider.")
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn generate(&self, _prompt: &str, _config: &GenerationConfig) -> Result<ModelResponse> {
        let content = self.next_response();
        let prompt_tokens = 10; // Mock values
        let completion_tokens = content.split_whitespace().count() as u32;

        Ok(ModelResponse {
            content,
            model: self.model_name.clone(),
            usage: Some(TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }),
            finish_reason: Some("stop".to_string()),
            tool_calls: None,
        })
    }

    async fn stream(
        &self,
        _prompt: &str,
        _config: &GenerationConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let content = self.next_response();
        let words: Vec<String> = content.split_whitespace().map(|s| s.to_string()).collect();

        let stream = stream! {
            for word in words {
                yield Ok(format!("{} ", word));
                // Simulate network delay
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        };

        Ok(Box::pin(stream))
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "Mock Provider".to_string(),
            supported_models: vec![
                "mock-model".to_string(),
                "mock-gpt-4".to_string(),
                "mock-claude-3".to_string(),
            ],
            supports_streaming: true,
        }
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Mock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_provider_generate() {
        let provider = MockProvider::new("Hello, world!");
        let config = GenerationConfig::default();

        let response = provider.generate("test prompt", &config).await.unwrap();

        assert_eq!(response.content, "Hello, world!");
        assert_eq!(response.model, "mock-model");
        assert!(response.usage.is_some());
        assert_eq!(response.finish_reason, Some("stop".to_string()));
    }

    #[tokio::test]
    async fn test_mock_provider_multiple_responses() {
        let provider = MockProvider::with_responses(vec![
            "First response".to_string(),
            "Second response".to_string(),
            "Third response".to_string(),
        ]);
        let config = GenerationConfig::default();

        let resp1 = provider.generate("prompt", &config).await.unwrap();
        assert_eq!(resp1.content, "First response");

        let resp2 = provider.generate("prompt", &config).await.unwrap();
        assert_eq!(resp2.content, "Second response");

        let resp3 = provider.generate("prompt", &config).await.unwrap();
        assert_eq!(resp3.content, "Third response");

        // Should cycle back to first
        let resp4 = provider.generate("prompt", &config).await.unwrap();
        assert_eq!(resp4.content, "First response");
    }

    #[tokio::test]
    async fn test_mock_provider_stream() {
        let provider = MockProvider::new("Hello world test");
        let config = GenerationConfig::default();

        let mut stream = provider.stream("test prompt", &config).await.unwrap();
        let mut chunks = Vec::new();

        while let Some(chunk) = stream.next().await {
            chunks.push(chunk.unwrap());
        }

        assert_eq!(chunks.len(), 3); // "Hello ", "world ", "test "
        assert!(chunks[0].contains("Hello"));
        assert!(chunks[1].contains("world"));
        assert!(chunks[2].contains("test"));
    }

    #[tokio::test]
    async fn test_mock_provider_metadata() {
        let provider = MockProvider::default();
        let metadata = provider.metadata();

        assert_eq!(metadata.name, "Mock Provider");
        assert!(metadata.supports_streaming);
        assert!(!metadata.supported_models.is_empty());
    }

    #[tokio::test]
    async fn test_mock_provider_custom_model_name() {
        let provider = MockProvider::new("test").with_model_name("custom-model");
        let config = GenerationConfig::default();

        let response = provider.generate("prompt", &config).await.unwrap();
        assert_eq!(response.model, "custom-model");
    }
}
