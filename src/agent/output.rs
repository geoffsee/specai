//! Shared agent output data types used by the core loop and CLI

use crate::agent::model::TokenUsage;
use crate::tools::ToolResult;
use crate::types::MessageRole;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Output from an agent execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    /// The response text
    pub response: String,
    /// Message identifier for the persisted assistant response
    pub response_message_id: Option<i64>,
    /// Token usage information
    pub token_usage: Option<TokenUsage>,
    /// Detailed tool invocations performed during this turn
    pub tool_invocations: Vec<ToolInvocation>,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Semantic memory recall statistics for this turn (if embeddings enabled)
    pub recall_stats: Option<MemoryRecallStats>,
    /// Unique identifier for correlating this run with logs/telemetry
    pub run_id: String,
    /// Optional recommendation produced by graph steering
    pub next_action: Option<String>,
    /// Model's internal reasoning/thinking process (extracted from <think> tags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Human-readable summary of the reasoning (if reasoning was present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    /// Snapshot of graph state for debugging purposes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_debug: Option<GraphDebugInfo>,
}

/// Minimal snapshot of a recent graph node for debugging output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDebugNode {
    pub id: i64,
    pub node_type: String,
    pub label: String,
}

/// Debug information about the graph state captured for run stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDebugInfo {
    pub enabled: bool,
    pub graph_memory_enabled: bool,
    pub auto_graph_enabled: bool,
    pub graph_steering_enabled: bool,
    pub node_count: usize,
    pub edge_count: usize,
    pub recent_nodes: Vec<GraphDebugNode>,
}

/// A single tool invocation, including arguments and outcome metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub name: String,
    pub arguments: Value,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolInvocation {
    pub fn from_result(name: &str, arguments: Value, result: &ToolResult) -> Self {
        let output = if result.output.trim().is_empty() {
            None
        } else {
            Some(result.output.clone())
        };

        Self {
            name: name.to_string(),
            arguments,
            success: result.success,
            output,
            error: result.error.clone(),
        }
    }
}

/// Telemetry about memory recall for a single turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecallStats {
    pub strategy: MemoryRecallStrategy,
    pub matches: Vec<MemoryRecallMatch>,
}

/// Strategy used for memory recall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryRecallStrategy {
    Semantic { requested: usize, returned: usize },
    RecentContext { limit: usize },
}

/// Summary of an individual recalled memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecallMatch {
    pub message_id: Option<i64>,
    pub score: f32,
    pub role: MessageRole,
    pub preview: String,
}
