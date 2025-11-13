/// API request and response models
use serde::{Deserialize, Serialize};

/// Request to query the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// The user's message/query
    pub message: String,
    /// Optional session ID for conversation continuity
    pub session_id: Option<String>,
    /// Optional agent profile to use
    pub agent: Option<String>,
    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,
    /// Optional temperature override
    pub temperature: Option<f32>,
    /// Optional max tokens
    pub max_tokens: Option<usize>,
}

/// Response from the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// The agent's response message
    pub response: String,
    /// Session ID for this conversation
    pub session_id: String,
    /// Agent profile used
    pub agent: String,
    /// Tool calls made (if any)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCallInfo>,
    /// Processing metadata
    pub metadata: ResponseMetadata,
}

/// Information about a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: serde_json::Value,
    /// Execution status
    pub success: bool,
    /// Tool output (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Error message if the tool failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    /// Timestamp of response
    pub timestamp: String,
    /// Model used
    pub model: String,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Unique identifier for correlating with telemetry
    pub run_id: String,
}

/// Streaming response chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamChunk {
    /// Initial metadata
    #[serde(rename = "start")]
    Start { session_id: String, agent: String },
    /// Content chunk
    #[serde(rename = "chunk")]
    Content { text: String },
    /// Tool call notification
    #[serde(rename = "tool_call")]
    ToolCall {
        name: String,
        arguments: serde_json::Value,
    },
    /// Tool result
    #[serde(rename = "tool_result")]
    ToolResult {
        name: String,
        result: serde_json::Value,
    },
    /// End of stream
    #[serde(rename = "end")]
    End { metadata: ResponseMetadata },
    /// Error occurred
    #[serde(rename = "error")]
    Error { message: String },
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Error code
    pub code: String,
    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(code: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Server version
    pub version: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Active sessions count
    pub active_sessions: usize,
}

/// Agent list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListResponse {
    /// Available agents
    pub agents: Vec<AgentInfo>,
}

/// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent ID
    pub id: String,
    /// Agent description/prompt
    pub description: String,
    /// Allowed tools
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub allowed_tools: Vec<String>,
    /// Denied tools
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub denied_tools: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_request_serialization() {
        let req = QueryRequest {
            message: "Hello".to_string(),
            session_id: Some("sess123".to_string()),
            agent: Some("coder".to_string()),
            stream: false,
            temperature: Some(0.7),
            max_tokens: Some(1000),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: QueryRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.message, "Hello");
        assert_eq!(deserialized.session_id, Some("sess123".to_string()));
    }

    #[test]
    fn test_query_response_serialization() {
        let resp = QueryResponse {
            response: "Hi there".to_string(),
            session_id: "sess123".to_string(),
            agent: "coder".to_string(),
            tool_calls: vec![],
            metadata: ResponseMetadata {
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                model: "mock".to_string(),
                processing_time_ms: 100,
                run_id: "run-1".to_string(),
            },
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: QueryResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.response, "Hi there");
        assert_eq!(deserialized.session_id, "sess123");
    }

    #[test]
    fn test_stream_chunk_variants() {
        let chunks = vec![
            StreamChunk::Start {
                session_id: "sess1".to_string(),
                agent: "coder".to_string(),
            },
            StreamChunk::Content {
                text: "Hello".to_string(),
            },
            StreamChunk::End {
                metadata: ResponseMetadata {
                    timestamp: "2024-01-01T00:00:00Z".to_string(),
                    model: "mock".to_string(),
                    processing_time_ms: 100,
                    run_id: "run-1".to_string(),
                },
            },
        ];

        for chunk in chunks {
            let json = serde_json::to_string(&chunk).unwrap();
            let _deserialized: StreamChunk = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_error_response() {
        let err = ErrorResponse::new("invalid_request", "Invalid API key")
            .with_details(serde_json::json!({"hint": "Check your configuration"}));

        assert_eq!(err.error, "Invalid API key");
        assert_eq!(err.code, "invalid_request");
        assert!(err.details.is_some());
    }

    #[test]
    fn test_health_response() {
        let health = HealthResponse {
            status: "healthy".to_string(),
            version: "0.1.0".to_string(),
            uptime_seconds: 3600,
            active_sessions: 5,
        };

        let json = serde_json::to_string(&health).unwrap();
        let deserialized: HealthResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.status, "healthy");
        assert_eq!(deserialized.uptime_seconds, 3600);
    }
}
