use anyhow::{Context, Result, anyhow};
use async_openai::{
    Client as OpenAIClient, config::OpenAIConfig, types::CreateEmbeddingRequestArgs,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Trait that describes an embeddings-capable service.
#[async_trait]
pub trait EmbeddingsService: Send + Sync + 'static {
    /// Generate an embedding for the provided input using the given model name.
    async fn create_embeddings(&self, model: &str, input: &str) -> Result<Vec<f32>>;
}

/// Client that wraps an embeddings service and keeps track of the model name.
#[derive(Clone)]
pub struct EmbeddingsClient {
    model: String,
    service: Arc<dyn EmbeddingsService>,
}

impl EmbeddingsClient {
    /// Create a client that uses the default OpenAI configuration (OPENAI_API_KEY).
    pub fn new(model: impl Into<String>) -> Self {
        Self::with_service(
            model,
            Arc::new(OpenAIEmbeddingsService::new()) as Arc<dyn EmbeddingsService>,
        )
    }

    /// Create a client that uses the provided API key.
    pub fn with_api_key(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        let service = OpenAIEmbeddingsService::with_api_key(api_key);
        Self::with_service(model, Arc::new(service))
    }

    /// Create a client that uses the provided OpenAI configuration.
    pub fn with_config(model: impl Into<String>, config: OpenAIConfig) -> Self {
        let service = OpenAIEmbeddingsService::with_config(config);
        Self::with_service(model, Arc::new(service))
    }

    /// Create a client around a custom embeddings service implementation.
    pub fn with_service(model: impl Into<String>, service: Arc<dyn EmbeddingsService>) -> Self {
        Self {
            model: model.into(),
            service,
        }
    }

    /// Ask the underlying service for an embedding.
    pub async fn embed(&self, input: &str) -> Result<Vec<f32>> {
        self.service.create_embeddings(&self.model, input).await
    }
}

/// Default service implementation that uses the async-openai client.
#[derive(Clone)]
pub struct OpenAIEmbeddingsService {
    client: OpenAIClient<OpenAIConfig>,
}

impl OpenAIEmbeddingsService {
    /// Create a service with the default OpenAI configuration.
    pub fn new() -> Self {
        Self {
            client: OpenAIClient::new(),
        }
    }

    /// Create a service backed by a specific API key.
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        Self::with_config(config)
    }

    /// Create a service with a custom OpenAI configuration.
    pub fn with_config(config: OpenAIConfig) -> Self {
        Self {
            client: OpenAIClient::with_config(config),
        }
    }
}

#[async_trait]
impl EmbeddingsService for OpenAIEmbeddingsService {
    async fn create_embeddings(&self, model: &str, input: &str) -> Result<Vec<f32>> {
        let request = CreateEmbeddingRequestArgs::default()
            .model(model)
            .input(input)
            .build()
            .context("Failed to build embedding request")?;

        let response = self
            .client
            .embeddings()
            .create(request)
            .await
            .context("OpenAI embeddings request failed")?;

        let embedding = response
            .data
            .first()
            .ok_or_else(|| anyhow!("OpenAI embeddings response was empty"))?
            .embedding
            .clone();

        Ok(embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use async_trait::async_trait;
    use std::sync::Arc;

    #[derive(Clone)]
    struct DummyService {
        embedding: Option<Vec<f32>>,
        fail: bool,
    }

    impl DummyService {
        fn ok(embedding: Vec<f32>) -> Self {
            Self {
                embedding: Some(embedding),
                fail: false,
            }
        }

        fn err() -> Self {
            Self {
                embedding: None,
                fail: true,
            }
        }
    }

    #[async_trait]
    impl EmbeddingsService for DummyService {
        async fn create_embeddings(&self, _model: &str, _input: &str) -> Result<Vec<f32>> {
            if self.fail {
                Err(anyhow!("boom"))
            } else {
                Ok(self.embedding.clone().unwrap_or_default())
            }
        }
    }

    #[tokio::test]
    async fn embed_returns_the_service_embedding() {
        let embedding = vec![0.1, 0.2];
        let service = Arc::new(DummyService::ok(embedding.clone()));
        let client = EmbeddingsClient::with_service("model", service);

        let result = client.embed("input").await.unwrap();

        assert_eq!(result, embedding);
    }

    #[tokio::test]
    async fn embed_propagates_errors() {
        let service = Arc::new(DummyService::err());
        let client = EmbeddingsClient::with_service("model", service);

        let result = client.embed("input").await;

        assert!(result.is_err());
    }
}
