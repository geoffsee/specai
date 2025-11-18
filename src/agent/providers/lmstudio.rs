//! LM Studio Model Provider
//!
//! Integrates with the LM Studio local server which exposes an OpenAI-compatible API.
//! This allows using locally hosted models while still benefiting from the agent
//! framework's standard tooling and function calling surface.

use crate::agent::model::{
    parse_thinking_tokens, GenerationConfig, ModelProvider, ModelResponse, ProviderKind,
    ProviderMetadata, TokenUsage, ToolCall,
};
use anyhow::{anyhow, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionTool, CreateChatCompletionRequestArgs,
    },
    Client,
};
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// LM Studio provider implemented through the OpenAI-compatible API surface.
#[derive(Debug, Clone)]
pub struct LMStudioProvider {
    /// Async OpenAI client configured for the LM Studio endpoint.
    client: Client<OpenAIConfig>,
    /// Model identifier (as configured within LM Studio).
    model: String,
    /// Optional system message applied to all prompts.
    system_message: Option<String>,
    /// Optional OpenAI tool definitions for native function calling support.
    tools: Option<Vec<ChatCompletionTool>>,
}

impl LMStudioProvider {
    /// Create a provider pointing at the default LM Studio endpoint (http://localhost:1234/v1).
    pub fn new(model: impl Into<String>) -> Self {
        let config = OpenAIConfig::new()
            .with_api_base("http://localhost:1234/v1")
            .with_api_key("lm-studio");

        Self {
            client: Client::with_config(config),
            model: model.into(),
            system_message: None,
            tools: None,
        }
    }

    /// Create a provider with a custom HTTP endpoint (e.g., remote LM Studio host).
    pub fn with_endpoint(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        let endpoint_str = endpoint.into();
        let api_base = if endpoint_str.ends_with("/v1") {
            endpoint_str
        } else {
            format!("{}/v1", endpoint_str)
        };

        let config = OpenAIConfig::new()
            .with_api_base(api_base)
            .with_api_key("lm-studio");

        Self {
            client: Client::with_config(config),
            model: model.into(),
            system_message: None,
            tools: None,
        }
    }

    /// Create a provider from a fully customized OpenAI configuration.
    pub fn with_config(config: OpenAIConfig, model: impl Into<String>) -> Self {
        Self {
            client: Client::with_config(config),
            model: model.into(),
            system_message: None,
            tools: None,
        }
    }

    /// Override the model identifier for future requests.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Apply a persistent system message to every chat request.
    pub fn with_system_message(mut self, message: impl Into<String>) -> Self {
        self.system_message = Some(message.into());
        self
    }

    /// Attach OpenAI-native tools for function calling.
    pub fn with_tools(mut self, tools: Vec<ChatCompletionTool>) -> Self {
        self.tools = if tools.is_empty() { None } else { Some(tools) };
        self
    }

    fn build_messages(&self, prompt: &str) -> Result<Vec<ChatCompletionRequestMessage>> {
        let mut messages = Vec::new();

        if let Some(system_msg) = &self.system_message {
            let system_message = ChatCompletionRequestSystemMessageArgs::default()
                .content(system_msg.clone())
                .build()
                .map_err(|e| anyhow!("Failed to build system message: {}", e))?;
            messages.push(ChatCompletionRequestMessage::System(system_message));
        }

        let user_message = ChatCompletionRequestUserMessageArgs::default()
            .content(prompt)
            .build()
            .map_err(|e| anyhow!("Failed to build user message: {}", e))?;
        messages.push(ChatCompletionRequestMessage::User(user_message));

        Ok(messages)
    }
}

#[async_trait]
impl ModelProvider for LMStudioProvider {
    async fn generate(&self, prompt: &str, config: &GenerationConfig) -> Result<ModelResponse> {
        let messages = self.build_messages(prompt)?;

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

        if let Some(ref tools) = self.tools {
            request_builder.tools(tools.clone());
        }

        let request = request_builder
            .build()
            .map_err(|e| anyhow!("Failed to build LM Studio request: {}", e))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| anyhow!("LM Studio API error: {}", e))?;

        let choice = response
            .choices
            .first()
            .ok_or_else(|| anyhow!("No response choices returned"))?;

        let raw_content = choice.message.content.clone().unwrap_or_default();
        let (reasoning, content) = parse_thinking_tokens(&raw_content);

        let tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|call| {
                        let arguments = serde_json::from_str(&call.function.arguments).ok()?;
                        Some(ToolCall {
                            id: call.id.clone(),
                            function_name: call.function.name.clone(),
                            arguments,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|calls| !calls.is_empty());

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
            tool_calls,
            reasoning,
        })
    }

    async fn stream(
        &self,
        prompt: &str,
        config: &GenerationConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let messages = self.build_messages(prompt)?;

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
            .map_err(|e| anyhow!("Failed to build LM Studio streaming request: {}", e))?;

        let mut response_stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(|e| anyhow!("LM Studio streaming API error: {}", e))?;

        let stream = stream! {
            use futures::StreamExt;

            let mut buffer = String::new();
            let mut in_think_block = false;
            let mut think_ended = false;

            while let Some(result) = response_stream.next().await {
                match result {
                    Ok(response) => {
                        if let Some(choice) = response.choices.first() {
                            if let Some(content) = &choice.delta.content {
                                buffer.push_str(content);

                                if buffer.contains("<think>") && !in_think_block {
                                    in_think_block = true;
                                }

                                if buffer.contains("</think>") && in_think_block {
                                    in_think_block = false;
                                    think_ended = true;
                                    if let Some(idx) = buffer.find("</think>") {
                                        buffer = buffer[idx + "</think>".len()..].to_string();
                                    }
                                }

                                if !in_think_block && (think_ended || !buffer.contains("<think>")) {
                                    let output = buffer.clone();
                                    buffer.clear();
                                    if !output.is_empty() {
                                        yield Ok(output);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(anyhow!("Stream error: {}", e));
                        break;
                    }
                }
            }

            if !buffer.is_empty() && !in_think_block {
                yield Ok(buffer);
            }
        };

        Ok(Box::pin(stream))
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "LM Studio".to_string(),
            supported_models: vec![
                "lmstudio-community/Llama-3.2-3B-Instruct".to_string(),
                "lmstudio-community/Mistral-7B-Instruct".to_string(),
                "lmstudio-community/phi-3-medium-4k-instruct".to_string(),
            ],
            supports_streaming: true,
        }
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::LMStudio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_lmstudio_provider_creation() {
        let provider = LMStudioProvider::new("lmstudio-community/Llama-3.2-3B-Instruct");
        assert_eq!(provider.model, "lmstudio-community/Llama-3.2-3B-Instruct");
        assert!(provider.system_message.is_none());
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_lmstudio_provider_with_custom_endpoint() {
        let provider = LMStudioProvider::with_endpoint(
            "http://192.168.1.2:1234",
            "lmstudio-community/Llama-3.2-3B-Instruct",
        );
        assert_eq!(provider.model, "lmstudio-community/Llama-3.2-3B-Instruct");
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_lmstudio_provider_with_model() {
        let provider = LMStudioProvider::new("model-a").with_model("model-b");
        assert_eq!(provider.model, "model-b");
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_lmstudio_provider_with_system_message() {
        let provider =
            LMStudioProvider::new("test-model").with_system_message("You are a helpful assistant.");
        assert_eq!(
            provider.system_message,
            Some("You are a helpful assistant.".to_string())
        );
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_lmstudio_provider_metadata() {
        let provider = LMStudioProvider::new("test-model");
        let metadata = provider.metadata();

        assert_eq!(metadata.name, "LM Studio");
        assert!(metadata.supports_streaming);
        assert!(!metadata.supported_models.is_empty());
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_lmstudio_provider_kind() {
        let provider = LMStudioProvider::new("test-model");
        assert_eq!(provider.kind(), ProviderKind::LMStudio);
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_build_messages_without_system() {
        let provider = LMStudioProvider::new("test-model");
        let messages = provider.build_messages("Hello").unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "system proxy APIs unavailable in sandbox"
    )]
    fn test_build_messages_with_system() {
        let provider =
            LMStudioProvider::new("test-model").with_system_message("You are a helpful assistant.");
        let messages = provider.build_messages("Hello").unwrap();
        assert_eq!(messages.len(), 2);
    }
}
