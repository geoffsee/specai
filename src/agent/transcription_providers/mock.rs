//! Mock Transcription Provider for Testing

use crate::agent::transcription::{
    TranscriptionConfig, TranscriptionEvent, TranscriptionProvider, TranscriptionProviderKind,
    TranscriptionProviderMetadata,
};
use anyhow::Result;
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// Mock transcription provider for testing
#[derive(Debug, Clone)]
pub struct MockTranscriptionProvider {
    /// Predefined transcriptions to emit
    transcriptions: Vec<String>,
    /// Provider name
    name: String,
}

impl MockTranscriptionProvider {
    /// Create a new mock provider with default transcriptions
    pub fn new() -> Self {
        Self {
            transcriptions: vec![
                "Hello, this is a test transcription.".to_string(),
                "The audio is being transcribed in real-time.".to_string(),
                "This is a mock provider for testing purposes.".to_string(),
            ],
            name: "Mock Transcription Provider".to_string(),
        }
    }

    /// Create a mock provider with custom transcriptions
    pub fn with_transcriptions(transcriptions: Vec<String>) -> Self {
        Self {
            transcriptions,
            name: "Mock Transcription Provider".to_string(),
        }
    }

    /// Set the provider name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Default for MockTranscriptionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranscriptionProvider for MockTranscriptionProvider {
    async fn start_transcription(
        &self,
        config: &TranscriptionConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TranscriptionEvent>> + Send>>> {
        let transcriptions = self.transcriptions.clone();
        let chunk_duration = config.chunk_duration_secs;
        let total_duration = config.duration_secs.unwrap_or(30);

        let stream = stream! {
            // Emit started event
            yield Ok(TranscriptionEvent::Started {
                timestamp: std::time::SystemTime::now(),
            });

            let mut chunk_id = 0;
            let num_chunks = (total_duration as f64 / chunk_duration).ceil() as usize;
            let chunk_duration_ms = (chunk_duration * 1000.0) as u64;

            for i in 0..num_chunks {
                // Wait for chunk duration
                tokio::time::sleep(tokio::time::Duration::from_millis(chunk_duration_ms)).await;

                // Get transcription (cycle through available ones)
                let text = transcriptions[i % transcriptions.len()].clone();

                // Emit transcription event
                yield Ok(TranscriptionEvent::Transcription {
                    chunk_id,
                    text,
                    timestamp: std::time::SystemTime::now(),
                });

                chunk_id += 1;
            }

            // Emit completed event
            yield Ok(TranscriptionEvent::Completed {
                timestamp: std::time::SystemTime::now(),
                total_chunks: chunk_id,
            });
        };

        Ok(Box::pin(stream))
    }

    fn metadata(&self) -> TranscriptionProviderMetadata {
        TranscriptionProviderMetadata {
            name: self.name.clone(),
            supported_models: vec!["mock-model".to_string()],
            supports_streaming: true,
            supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string()],
        }
    }

    fn kind(&self) -> TranscriptionProviderKind {
        TranscriptionProviderKind::Mock
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = MockTranscriptionProvider::new();
        let config = TranscriptionConfig {
            duration_secs: Some(1),
            chunk_duration_secs: 0.1,
            ..Default::default()
        };

        let mut stream = provider.start_transcription(&config).await.unwrap();

        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        // Should have: Started + multiple Transcriptions + Completed
        assert!(!events.is_empty());
        assert!(matches!(events[0], TranscriptionEvent::Started { .. }));
        assert!(matches!(
            events[events.len() - 1],
            TranscriptionEvent::Completed { .. }
        ));
    }

    #[test]
    fn test_mock_provider_metadata() {
        let provider = MockTranscriptionProvider::new();
        let metadata = provider.metadata();

        assert_eq!(metadata.name, "Mock Transcription Provider");
        assert!(metadata.supports_streaming);
        assert!(!metadata.supported_models.is_empty());
    }
}
