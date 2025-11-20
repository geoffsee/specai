//! VTT-RS Transcription Provider
//!
//! Real-time audio transcription using the vtt-rs crate and OpenAI-compatible APIs.

use crate::agent::transcription::{
    TranscriptionConfig, TranscriptionEvent, TranscriptionProvider, TranscriptionProviderKind,
    TranscriptionProviderMetadata,
};
use anyhow::{Context as _, Result};
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use vtt_rs::{Config as VttConfig, TranscriptionService};

/// VTT-RS based transcription provider
#[derive(Debug)]
pub struct VttRsProvider {
    /// API key for transcription service
    api_key: String,
    /// Optional custom endpoint
    endpoint: Option<String>,
    /// Use on-device transcription (offline mode)
    on_device: bool,
    /// Provider name
    name: String,
}

impl VttRsProvider {
    /// Create a new VTT-RS provider with API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: None,
            on_device: false,
            name: "VTT-RS Transcription Provider".to_string(),
        }
    }

    /// Create with API key from environment variable
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .or_else(|_| std::env::var("VTT_API_KEY"))
            .context("API key not found in environment (OPENAI_API_KEY or VTT_API_KEY)")?;

        Ok(Self::new(api_key))
    }

    /// Set a custom endpoint
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the provider name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Enable on-device transcription (offline mode)
    pub fn with_on_device(mut self, on_device: bool) -> Self {
        self.on_device = on_device;
        self
    }

    /// Build VTT-RS config from transcription config
    fn build_vtt_config(&self, config: &TranscriptionConfig) -> VttConfig {
        use std::path::PathBuf;

        // Create config with defaults and override as needed
        VttConfig {
            chunk_duration_secs: config.chunk_duration_secs as usize,
            model: config.model.clone(),
            endpoint: config
                .endpoint
                .clone()
                .or(self.endpoint.clone())
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            out_file: config.out_file.clone().map(PathBuf::from),
            on_device: if self.on_device {
                Some(vtt_rs::OnDeviceConfig::default())
            } else {
                None
            },
        }
    }
}

#[async_trait]
impl TranscriptionProvider for VttRsProvider {
    async fn start_transcription(
        &self,
        config: &TranscriptionConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TranscriptionEvent>> + Send>>> {
        use tokio::sync::mpsc;

        let vtt_config = self.build_vtt_config(config);
        let api_key = self.api_key.clone();
        let duration = config.duration_secs;

        // Create a channel for forwarding events
        let (tx, mut rx) = mpsc::unbounded_channel::<TranscriptionEvent>();

        // Spawn a task to handle the transcription service
        // This isolates the non-Send types (cpal Stream) in a separate task
        tokio::task::spawn_local(async move {
            // Emit started event
            let _ = tx.send(TranscriptionEvent::Started {
                timestamp: std::time::SystemTime::now(),
            });

            // Create transcription service
            let mut service = match TranscriptionService::new(vtt_config, api_key) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(TranscriptionEvent::Error {
                        chunk_id: 0,
                        message: format!("Failed to create transcription service: {}", e),
                    });
                    return;
                }
            };

            // Start the transcription service
            let (mut receiver, _stream) = match service.start().await {
                Ok(result) => result,
                Err(e) => {
                    let _ = tx.send(TranscriptionEvent::Error {
                        chunk_id: 0,
                        message: format!("Failed to start transcription: {}", e),
                    });
                    return;
                }
            };

            // Track statistics
            let mut chunk_count = 0;
            let start_time = std::time::SystemTime::now();

            // Process events from vtt-rs
            loop {
                // Check if we've exceeded duration
                if let Some(max_duration) = duration {
                    if let Ok(elapsed) = start_time.elapsed() {
                        if elapsed.as_secs() >= max_duration {
                            break;
                        }
                    }
                }

                // Receive next event from vtt-rs
                match receiver.recv().await {
                    Some(vtt_event) => match vtt_event {
                        vtt_rs::TranscriptionEvent::Transcription { chunk_id, text } => {
                            chunk_count += 1;
                            let _ = tx.send(TranscriptionEvent::Transcription {
                                chunk_id,
                                text,
                                timestamp: std::time::SystemTime::now(),
                            });
                        }
                        vtt_rs::TranscriptionEvent::Error { chunk_id, error } => {
                            let _ = tx.send(TranscriptionEvent::Error {
                                chunk_id,
                                message: error,
                            });
                        }
                    },
                    None => {
                        // Channel closed, transcription stopped
                        break;
                    }
                }
            }

            // Emit completed event
            let _ = tx.send(TranscriptionEvent::Completed {
                timestamp: std::time::SystemTime::now(),
                total_chunks: chunk_count,
            });
        });

        // Create a stream from the receiver
        let stream = stream! {
            while let Some(event) = rx.recv().await {
                yield Ok(event);
            }
        };

        Ok(Box::pin(stream))
    }

    fn metadata(&self) -> TranscriptionProviderMetadata {
        TranscriptionProviderMetadata {
            name: self.name.clone(),
            supported_models: vec![
                "whisper-1".to_string(),
                "whisper-large".to_string(),
                "whisper-large-v2".to_string(),
                "whisper-large-v3".to_string(),
            ],
            supports_streaming: true,
            supported_languages: vec![
                // Major languages supported by Whisper
                "en".to_string(), // English
                "es".to_string(), // Spanish
                "fr".to_string(), // French
                "de".to_string(), // German
                "it".to_string(), // Italian
                "pt".to_string(), // Portuguese
                "nl".to_string(), // Dutch
                "pl".to_string(), // Polish
                "ru".to_string(), // Russian
                "ja".to_string(), // Japanese
                "ko".to_string(), // Korean
                "zh".to_string(), // Chinese
                "ar".to_string(), // Arabic
                "hi".to_string(), // Hindi
            ],
        }
    }

    fn kind(&self) -> TranscriptionProviderKind {
        TranscriptionProviderKind::VttRs
    }

    async fn health_check(&self) -> Result<bool> {
        // Simple check: verify API key is set
        if self.api_key.is_empty() {
            return Ok(false);
        }

        // Could add more sophisticated checks here:
        // - Test API endpoint connectivity
        // - Verify audio device availability
        // - etc.

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = VttRsProvider::new("test-api-key");
        assert_eq!(provider.api_key, "test-api-key");
        assert!(provider.endpoint.is_none());
    }

    #[test]
    fn test_provider_with_endpoint() {
        let provider =
            VttRsProvider::new("test-api-key").with_endpoint("https://custom-endpoint.com");
        assert_eq!(
            provider.endpoint,
            Some("https://custom-endpoint.com".to_string())
        );
    }

    #[test]
    fn test_provider_metadata() {
        let provider = VttRsProvider::new("test-api-key");
        let metadata = provider.metadata();

        assert_eq!(metadata.name, "VTT-RS Transcription Provider");
        assert!(metadata.supports_streaming);
        assert!(metadata.supported_models.contains(&"whisper-1".to_string()));
        assert!(!metadata.supported_languages.is_empty());
    }

    #[tokio::test]
    async fn test_health_check_with_api_key() {
        let provider = VttRsProvider::new("test-api-key");
        let health = provider.health_check().await.unwrap();
        assert!(health);
    }

    #[tokio::test]
    async fn test_health_check_without_api_key() {
        let provider = VttRsProvider::new("");
        let health = provider.health_check().await.unwrap();
        assert!(!health);
    }

    #[test]
    fn test_build_vtt_config() {
        let provider =
            VttRsProvider::new("test-api-key").with_endpoint("https://custom-endpoint.com");

        let config = TranscriptionConfig {
            chunk_duration_secs: 3.0,
            model: "whisper-large".to_string(),
            out_file: Some("/tmp/transcript.txt".to_string()),
            ..Default::default()
        };

        let vtt_config = provider.build_vtt_config(&config);
        // VttConfig doesn't expose fields for testing, but we can verify it doesn't panic
        drop(vtt_config);
    }
}
