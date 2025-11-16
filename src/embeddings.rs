use anyhow::{anyhow, Context, Result};
use async_openai::{
    config::OpenAIConfig, types::CreateEmbeddingRequestArgs, Client as OpenAIClient,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Trait that describes an embeddings-capable service.
#[async_trait]
pub trait EmbeddingsService: Send + Sync + 'static {
    /// Generate embeddings for the provided inputs using the given model name.
    async fn create_embeddings(&self, model: &str, inputs: Vec<String>) -> Result<Vec<Vec<f32>>>;
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

    /// Ask the underlying service for embeddings for a batch of inputs.
    pub async fn embed_batch<T>(&self, inputs: &[T]) -> Result<Vec<Vec<f32>>>
    where
        T: AsRef<str>,
    {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let sanitized_inputs = inputs
            .iter()
            .map(|input| sanitize_embedding_input(input.as_ref()))
            .collect::<Vec<_>>();

        self.service
            .create_embeddings(&self.model, sanitized_inputs)
            .await
    }

    /// Ask the underlying service for an embedding for a single input.
    pub async fn embed(&self, input: &str) -> Result<Vec<f32>> {
        let inputs = [input];
        let mut embeddings = self.embed_batch(&inputs).await?;
        Ok(embeddings.pop().unwrap_or_default())
    }
}

fn sanitize_embedding_input(input: &str) -> String {
    const MAX_LEN: usize = 4096;
    let mut processed = input
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n");

    if processed.len() > MAX_LEN {
        processed.truncate(MAX_LEN);
        processed.push_str("\\n[truncated]");
    }

    processed
}

#[cfg(test)]
mod embedding_sanitizer_tests {
    use super::sanitize_embedding_input;

    #[test]
    fn sanitizes_newlines_and_backslashes() {
        let raw = "line1\nline2\r\npath\\to\\file";
        let sanitized = sanitize_embedding_input(raw);
        assert_eq!(sanitized, "line1\\nline2\\r\\npath\\\\to\\\\file");
    }

    #[test]
    fn truncates_long_payloads() {
        let raw = "a".repeat(5000);
        let sanitized = sanitize_embedding_input(&raw);
        assert!(sanitized.ends_with("\\n[truncated]"));
        assert!(sanitized.len() <= 4096 + "\\n[truncated]".len());
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
    async fn create_embeddings(&self, model: &str, inputs: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let request = CreateEmbeddingRequestArgs::default()
            .model(model)
            .input(inputs)
            .build()
            .context("Failed to build embedding request")?;

        let response = self
            .client
            .embeddings()
            .create(request)
            .await
            .context("OpenAI embeddings request failed")?;

        let embeddings = response
            .data
            .into_iter()
            .map(|item| item.embedding)
            .collect::<Vec<_>>();

        if embeddings.is_empty() {
            Err(anyhow!("OpenAI embeddings response was empty"))
        } else {
            Ok(embeddings)
        }
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
        embeddings: Vec<Vec<f32>>,
        fail: bool,
    }

    impl DummyService {
        fn ok_single(embedding: Vec<f32>) -> Self {
            Self {
                embeddings: vec![embedding],
                fail: false,
            }
        }

        fn ok_batch(embeddings: Vec<Vec<f32>>) -> Self {
            Self {
                embeddings,
                fail: false,
            }
        }

        fn err() -> Self {
            Self {
                embeddings: Vec::new(),
                fail: true,
            }
        }
    }

    #[async_trait]
    impl EmbeddingsService for DummyService {
        async fn create_embeddings(
            &self,
            _model: &str,
            _inputs: Vec<String>,
        ) -> Result<Vec<Vec<f32>>> {
            if self.fail {
                return Err(anyhow!("boom"));
            }

            if self.embeddings.is_empty() {
                return Ok(Vec::new());
            }

            Ok(self.embeddings.clone())
        }
    }

    #[tokio::test]
    async fn embed_returns_the_service_embedding() {
        let embedding = vec![0.1, 0.2];
        let service = Arc::new(DummyService::ok_single(embedding.clone()));
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

    #[tokio::test]
    async fn embed_batch_returns_all_embeddings() {
        let service = Arc::new(DummyService::ok_batch(vec![vec![0.1, 0.2], vec![0.3, 0.4]]));
        let client = EmbeddingsClient::with_service("model", service);

        let inputs = ["first", "second"];
        let result = client.embed_batch(&inputs).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![0.1, 0.2]);
        assert_eq!(result[1], vec![0.3, 0.4]);
    }
}
