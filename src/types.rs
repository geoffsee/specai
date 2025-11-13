use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "system" => MessageRole::System,
            "assistant" => MessageRole::Assistant,
            _ => MessageRole::User,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryVector {
    pub id: i64,
    pub session_id: String,
    pub message_id: Option<i64>,
    pub embedding: Vec<f32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLog {
    pub id: i64,
    pub session_id: String,
    pub agent: String,
    pub run_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
    pub success: bool,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}
