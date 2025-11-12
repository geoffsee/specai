//! OpenAI Model Provider
//!
//! Integration with OpenAI's API using the async-openai crate.

use crate::agent::model::{
    GenerationConfig, ModelProvider, ModelResponse, ProviderKind, ProviderMetadata, TokenUsage,
};
use anyhow::{anyhow, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client,
};
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// OpenAI provider that wraps the async-openai crate
#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    /// The async-openai client
    client: Client<OpenAIConfig>,
    /// Default model to use (e.g., "gpt-4.1", "gpt-4.1-mini")
    model: String,
    /// Optional system message for all requests
    system_message: Option<String>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with the default configuration
    ///
    /// This will use the OPENAI_API_KEY environment variable for authentication
    /// and default to the "gpt-4.1-mini" model.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            model: "gpt-4.1-mini".to_string(),
            system_message: None,
        }
    }

    /// Create a new OpenAI provider with a custom API key
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        Self {
            client: Client::with_config(config),
            model: "gpt-4.1-mini".to_string(),
            system_message: None,
        }
    }

    /// Create a new OpenAI provider with a custom configuration
    pub fn with_config(config: OpenAIConfig) -> Self {
        Self {
            client: Client::with_config(config),
            model: "gpt-4.1-mini".to_string(),
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

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
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
            .map_err(|e| anyhow!("OpenAI API error: {}", e))?;

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
            .map_err(|e| anyhow!("OpenAI streaming API error: {}", e))?;

        // Convert the OpenAI stream to our stream format
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
            name: "OpenAI".to_string(),
            supported_models: vec![
                "gpt-4.1".to_string(),
                "gpt-4-turbo-preview".to_string(),
                "gpt-4-32k".to_string(),
                "gpt-4.1-mini".to_string(),
                "gpt-4.1-mini-16k".to_string(),
            ],
            supports_streaming: true,
        }
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAI
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAIProvider::new();
        assert_eq!(provider.model, "gpt-4.1-mini");
        assert!(provider.system_message.is_none());
    }

    #[test]
    fn test_openai_provider_with_model() {
        let provider = OpenAIProvider::new().with_model("gpt-4.1");
        assert_eq!(provider.model, "gpt-4.1");
    }

    #[test]
    fn test_openai_provider_with_system_message() {
        let provider =
            OpenAIProvider::new().with_system_message("You are a helpful assistant.");
        assert_eq!(
            provider.system_message,
            Some("You are a helpful assistant.".to_string())
        );
    }

    #[test]
    fn test_openai_provider_metadata() {
        let provider = OpenAIProvider::new();
        let metadata = provider.metadata();

        assert_eq!(metadata.name, "OpenAI");
        assert!(metadata.supports_streaming);
        assert!(metadata.supported_models.contains(&"gpt-4.1".to_string()));
        assert!(metadata
            .supported_models
            .contains(&"gpt-4.1-mini".to_string()));
    }

    #[test]
    fn test_openai_provider_kind() {
        let provider = OpenAIProvider::new();
        assert_eq!(provider.kind(), ProviderKind::OpenAI);
    }

    #[test]
    fn test_build_messages_without_system() {
        let provider = OpenAIProvider::new();
        let messages = provider.build_messages("Hello, world!").unwrap();

        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_build_messages_with_system() {
        let provider = OpenAIProvider::new().with_system_message("You are a helpful assistant.");
        let messages = provider.build_messages("Hello, world!").unwrap();

        assert_eq!(messages.len(), 2);
    }
}