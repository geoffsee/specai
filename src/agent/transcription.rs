//! Transcription Provider Abstraction Layer
//!
//! This module defines the core traits and types for integrating with various transcription providers.
//! It provides a unified interface that abstracts away provider-specific details.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Configuration for transcription requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// Duration to record in seconds (None = continuous until stopped)
    pub duration_secs: Option<u64>,
    /// Audio chunk duration in seconds
    pub chunk_duration_secs: f64,
    /// Model to use for transcription (e.g., "whisper-1")
    pub model: String,
    /// Optional output file path for transcript
    pub out_file: Option<String>,
    /// Language code (e.g., "en", "es", "fr")
    pub language: Option<String>,
    /// Custom API endpoint (if different from default)
    pub endpoint: Option<String>,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            duration_secs: Some(30),
            chunk_duration_secs: 5.0,
            model: "whisper-1".to_string(),
            out_file: None,
            language: None,
            endpoint: None,
        }
    }
}

/// Event emitted during transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscriptionEvent {
    /// Successful transcription of an audio chunk
    Transcription {
        /// Chunk identifier
        chunk_id: usize,
        /// Transcribed text
        text: String,
        /// Timestamp when this chunk was processed
        timestamp: std::time::SystemTime,
    },
    /// Error during transcription
    Error {
        /// Chunk identifier where error occurred
        chunk_id: usize,
        /// Error message
        message: String,
    },
    /// Transcription session started
    Started {
        /// Start timestamp
        timestamp: std::time::SystemTime,
    },
    /// Transcription session completed
    Completed {
        /// End timestamp
        timestamp: std::time::SystemTime,
        /// Total chunks processed
        total_chunks: usize,
    },
}

/// Transcription session statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionStats {
    /// Total duration recorded in seconds
    pub duration_secs: f64,
    /// Total chunks processed
    pub total_chunks: usize,
    /// Number of successful transcriptions
    pub successful_chunks: usize,
    /// Number of errors
    pub error_count: usize,
    /// Total characters transcribed
    pub total_chars: usize,
}

/// Provider metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionProviderMetadata {
    /// Provider name
    pub name: String,
    /// Supported models
    pub supported_models: Vec<String>,
    /// Supports streaming
    pub supports_streaming: bool,
    /// Supported languages
    pub supported_languages: Vec<String>,
}

/// Types of transcription providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionProviderKind {
    Mock,
    #[cfg(feature = "vttrs")]
    VttRs,
}

impl TranscriptionProviderKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mock" => Some(TranscriptionProviderKind::Mock),
            #[cfg(feature = "vttrs")]
            "vttrs" | "vtt-rs" => Some(TranscriptionProviderKind::VttRs),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TranscriptionProviderKind::Mock => "mock",
            #[cfg(feature = "vttrs")]
            TranscriptionProviderKind::VttRs => "vttrs",
        }
    }
}

/// Core trait that all transcription providers must implement
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Start a transcription session and return a stream of events
    async fn start_transcription(
        &self,
        config: &TranscriptionConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TranscriptionEvent>> + Send>>>;

    /// Get provider metadata
    fn metadata(&self) -> TranscriptionProviderMetadata;

    /// Get the provider kind
    fn kind(&self) -> TranscriptionProviderKind;

    /// Check if the provider is available and configured correctly
    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_kind_from_str() {
        assert_eq!(
            TranscriptionProviderKind::from_str("mock"),
            Some(TranscriptionProviderKind::Mock)
        );
        assert_eq!(
            TranscriptionProviderKind::from_str("Mock"),
            Some(TranscriptionProviderKind::Mock)
        );
        assert_eq!(
            TranscriptionProviderKind::from_str("MOCK"),
            Some(TranscriptionProviderKind::Mock)
        );
        assert_eq!(TranscriptionProviderKind::from_str("invalid"), None);
    }

    #[test]
    fn test_provider_kind_as_str() {
        assert_eq!(TranscriptionProviderKind::Mock.as_str(), "mock");
    }

    #[test]
    fn test_transcription_config_default() {
        let config = TranscriptionConfig::default();
        assert_eq!(config.duration_secs, Some(30));
        assert_eq!(config.chunk_duration_secs, 5.0);
        assert_eq!(config.model, "whisper-1");
    }

    #[test]
    fn test_transcription_config_serialization() {
        let config = TranscriptionConfig {
            duration_secs: Some(60),
            chunk_duration_secs: 3.0,
            model: "whisper-large".to_string(),
            out_file: Some("/tmp/transcript.txt".to_string()),
            language: Some("en".to_string()),
            endpoint: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TranscriptionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.duration_secs, deserialized.duration_secs);
        assert_eq!(config.model, deserialized.model);
    }
}
