//! MLX Model Provider
//!
//! Integration with MLX (Apple's machine learning framework) via OpenAI-compatible API.
//! MLX provides local inference on Apple Silicon with an OpenAI-compatible server.

use crate::agent::model::{
    GenerationConfig, ModelProvider, ModelResponse, ProviderKind, ProviderMetadata, TokenUsage,
};
use anyhow::{Result, anyhow};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
};
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// MLX provider that uses OpenAI-compatible API
#[derive(Debug, Clone)]
pub struct MLXProvider {
    /// The async-openai client configured for MLX endpoint
    client: Client<OpenAIConfig>,
    /// Model name (e.g., "mlx-community/Llama-3.2-3B-Instruct-4bit")
    model: String,
    /// Optional system message for all requests
    system_message: Option<String>,
}

impl MLXProvider {
    /// Create a new MLX provider with the default configuration
    ///
    /// This will connect to http://localhost:10240 by default.
    /// The model must be specified since MLX can serve various models.
    pub fn new(model: impl Into<String>) -> Self {
        let config = OpenAIConfig::new()
            .with_api_base("http://localhost:10240/v1")
            .with_api_key("mlx-key"); // MLX doesn't require a real key, but the client needs one

        Self {
            client: Client::with_config(config),
            model: model.into(),
            system_message: None,
        }
    }

    /// Create a new MLX provider with a custom endpoint
    pub fn with_endpoint(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        let endpoint_str = endpoint.into();
        let api_base = if endpoint_str.ends_with("/v1") {
            endpoint_str
        } else {
            format!("{}/v1", endpoint_str)
        };

        let config = OpenAIConfig::new()
            .with_api_base(api_base)
            .with_api_key("mlx-key");

        Self {
            client: Client::with_config(config),
            model: model.into(),
            system_message: None,
        }
    }

    /// Create a new MLX provider with a custom configuration
    pub fn with_config(config: OpenAIConfig, model: impl Into<String>) -> Self {
        Self {
            client: Client::with_config(config),
            model: model.into(),
            system_message: None,
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set a system message to be included in all requests
    pub fn with_system_message(mut self, message: impl Into<String>) -> Self {
        self.system_message = Some(message.into());
        self
    }

    /// Build the messages for the chat completion request
    fn build_messages(&self, prompt: &str) -> Result<Vec<ChatCompletionRequestMessage>> {
        let mut messages = Vec::new();

        // Add system message if present
        if let Some(system_msg) = &self.system_message {
            let system_message = ChatCompletionRequestSystemMessageArgs::default()
                .content(system_msg.clone())
                .build()
                .map_err(|e| anyhow!("Failed to build system message: {}", e))?;
            messages.push(ChatCompletionRequestMessage::System(system_message));
        }

        // Add user prompt
        let user_message = ChatCompletionRequestUserMessageArgs::default()
            .content(prompt)
            .build()
            .map_err(|e| anyhow!("Failed to build user message: {}", e))?;
        messages.push(ChatCompletionRequestMessage::User(user_message));

        Ok(messages)
    }
}

#[async_trait]
impl ModelProvider for MLXProvider {
    async fn generate(&self, prompt: &str, config: &GenerationConfig) -> Result<ModelResponse> {
        let messages = self.build_messages(prompt)?;

        // Build the request with configuration
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&self.model).messages(messages);

        if let Some(temp) = config.temperature {
            request_builder.temperature(temp);
        }
        if let Some(max_tokens) = config.max_tokens {
            request_builder.max_tokens(max_tokens);
        }
        if let Some(top_p) = config.top_p {
            request_builder.top_p(top_p);
        }
        if let Some(freq_penalty) = config.frequency_penalty {
            request_builder.frequency_penalty(freq_penalty);
        }
        if let Some(pres_penalty) = config.presence_penalty {
            request_builder.presence_penalty(pres_penalty);
        }
        if let Some(stop) = &config.stop_sequences {
            request_builder.stop(stop.clone());
        }

        let request = request_builder
            .build()
            .map_err(|e| anyhow!("Failed to build request: {}", e))?;

        // Make the API call
        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| anyhow!("MLX API error: {}", e))?;

        // Extract the response
        let choice = response
            .choices
            .first()
            .ok_or_else(|| anyhow!("No response choices returned"))?;

        let content = choice
            .message
            .content
            .clone()
            .ok_or_else(|| anyhow!("No content in response"))?;

        let usage = response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(ModelResponse {
            content,
            model: response.model,
            usage,
            finish_reason: choice.finish_reason.as_ref().map(|r| format!("{:?}", r)),
        })
    }

    async fn stream(
        &self,
        prompt: &str,
        config: &GenerationConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let messages = self.build_messages(prompt)?;

        // Build the streaming request
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder
            .model(&self.model)
            .messages(messages)
            .stream(true);

        if let Some(temp) = config.temperature {
            request_builder.temperature(temp);
        }
        if let Some(max_tokens) = config.max_tokens {
            request_builder.max_tokens(max_tokens);
        }
        if let Some(top_p) = config.top_p {
            request_builder.top_p(top_p);
        }
        if let Some(freq_penalty) = config.frequency_penalty {
            request_builder.frequency_penalty(freq_penalty);
        }
        if let Some(pres_penalty) = config.presence_penalty {
            request_builder.presence_penalty(pres_penalty);
        }
        if let Some(stop) = &config.stop_sequences {
            request_builder.stop(stop.clone());
        }

        let request = request_builder
            .build()
            .map_err(|e| anyhow!("Failed to build streaming request: {}", e))?;

        // Make the streaming API call
        let mut response_stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(|e| anyhow!("MLX streaming API error: {}", e))?;

        // Convert the OpenAI-compatible stream to our stream format
        let stream = stream! {
            use futures::StreamExt;

            while let Some(result) = response_stream.next().await {
                match result {
                    Ok(response) => {
                        if let Some(choice) = response.choices.first() {
                            if let Some(content) = &choice.delta.content {
                                yield Ok(content.clone());
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(anyhow!("Stream error: {}", e));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "MLX".to_string(),
            supported_models: vec![
                "mlx-community/Llama-3.2-3B-Instruct-4bit".to_string(),
                "mlx-community/Llama-3.2-1B-Instruct-4bit".to_string(),
                "mlx-community/Mistral-7B-Instruct-v0.3-4bit".to_string(),
                "mlx-community/gemma-2-2b-it-4bit".to_string(),
                // MLX supports many models - these are just examples
            ],
            supports_streaming: true,
        }
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::MLX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_mlx_provider_creation() {
        let provider = MLXProvider::new("mlx-community/Llama-3.2-3B-Instruct-4bit");
        assert_eq!(provider.model, "mlx-community/Llama-3.2-3B-Instruct-4bit");
        assert!(provider.system_message.is_none());
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_mlx_provider_with_custom_endpoint() {
        let provider = MLXProvider::with_endpoint(
            "http://192.168.1.100:8080",
            "mlx-community/Llama-3.2-3B-Instruct-4bit",
        );
        assert_eq!(provider.model, "mlx-community/Llama-3.2-3B-Instruct-4bit");
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_mlx_provider_with_model() {
        let provider = MLXProvider::new("model-1").with_model("model-2");
        assert_eq!(provider.model, "model-2");
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_mlx_provider_with_system_message() {
        let provider =
            MLXProvider::new("test-model").with_system_message("You are a helpful assistant.");
        assert_eq!(
            provider.system_message,
            Some("You are a helpful assistant.".to_string())
        );
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_mlx_provider_metadata() {
        let provider = MLXProvider::new("test-model");
        let metadata = provider.metadata();

        assert_eq!(metadata.name, "MLX");
        assert!(metadata.supports_streaming);
        assert!(!metadata.supported_models.is_empty());
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_mlx_provider_kind() {
        let provider = MLXProvider::new("test-model");
        assert_eq!(provider.kind(), ProviderKind::MLX);
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_build_messages_without_system() {
        let provider = MLXProvider::new("test-model");
        let messages = provider.build_messages("Hello, world!").unwrap();

        assert_eq!(messages.len(), 1);
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "system proxy APIs unavailable in sandbox")]
    fn test_build_messages_with_system() {
        let provider =
            MLXProvider::new("test-model").with_system_message("You are a helpful assistant.");
        let messages = provider.build_messages("Hello, world!").unwrap();

        assert_eq!(messages.len(), 2);
    }
}
