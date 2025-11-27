//! Data models used by the demo application.

use spec_ai_tui::style::Color;

/// Mock chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    /// Optional tool name (for tool role)
    pub tool_name: Option<String>,
    /// Whether this message is a prompt waiting for user input
    pub is_prompt: bool,
}

impl ChatMessage {
    pub fn new(role: &str, content: &str, timestamp: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: timestamp.to_string(),
            tool_name: None,
            is_prompt: false,
        }
    }

    pub fn tool(name: &str, content: &str, timestamp: &str) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.to_string(),
            timestamp: timestamp.to_string(),
            tool_name: Some(name.to_string()),
            is_prompt: false,
        }
    }

    /// Create an assistant message that prompts the user for input
    pub fn prompt(content: &str, timestamp: &str) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.to_string(),
            timestamp: timestamp.to_string(),
            tool_name: None,
            is_prompt: true,
        }
    }
}

/// Mock tool execution
#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub name: String,
    pub status: ToolStatus,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ToolStatus {
    Running,
    Success,
    Failed,
}

/// Agent-spawned process
#[derive(Debug, Clone)]
pub struct AgentProcess {
    pub pid: u32,
    pub command: String,
    pub agent: String, // Which agent spawned this
    pub status: ProcessStatus,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub output_lines: Vec<String>, // Last few lines of output
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessStatus {
    Running,
    Stopped,
    Completed,
    Failed,
}

impl AgentProcess {
    pub fn new(pid: u32, command: &str, agent: &str) -> Self {
        Self {
            pid,
            command: command.to_string(),
            agent: agent.to_string(),
            status: ProcessStatus::Running,
            exit_code: None,
            elapsed_ms: 0,
            output_lines: Vec::new(),
        }
    }

    pub fn status_icon(&self) -> (&'static str, Color) {
        match self.status {
            ProcessStatus::Running => ("●", Color::Green),
            ProcessStatus::Stopped => ("◉", Color::Yellow),
            ProcessStatus::Completed => ("✓", Color::DarkGrey),
            ProcessStatus::Failed => ("✗", Color::Red),
        }
    }

    pub fn elapsed_display(&self) -> String {
        let secs = self.elapsed_ms / 1000;
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m{}s", secs / 60, secs % 60)
        } else {
            format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
        }
    }
}

/// Chat session for history
#[derive(Debug, Clone)]
pub struct Session {
    pub id: usize,
    pub title: String,
    pub preview: String,
    pub timestamp: String,
    pub message_count: usize,
    pub messages: Vec<ChatMessage>,
}
