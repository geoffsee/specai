use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Invalid agent configuration: {0}")]
    Invalid(String),
}

/// Configuration for a specific agent profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// System prompt for this agent
    #[serde(default)]
    pub prompt: Option<String>,

    /// Conversational style or personality
    #[serde(default)]
    pub style: Option<String>,

    /// Temperature override for this agent (0.0 to 2.0)
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Model provider override (e.g., "openai", "anthropic")
    #[serde(default)]
    pub model_provider: Option<String>,

    /// Model name override (e.g., "gpt-4", "claude-3-opus")
    #[serde(default)]
    pub model_name: Option<String>,

    /// List of tools this agent is allowed to use
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// List of tools this agent is forbidden from using
    #[serde(default)]
    pub denied_tools: Option<Vec<String>>,

    /// Memory parameters: number of messages to recall (k for top-k)
    #[serde(default = "AgentProfile::default_memory_k")]
    pub memory_k: usize,

    /// Top-p sampling parameter for memory recall
    #[serde(default = "AgentProfile::default_top_p")]
    pub top_p: f32,

    /// Maximum context window size for this agent
    #[serde(default)]
    pub max_context_tokens: Option<usize>,

    // ========== Knowledge Graph Configuration ==========
    /// Enable knowledge graph features for this agent
    #[serde(default)]
    pub enable_graph: bool,

    /// Use graph-based memory recall (combines with embeddings)
    #[serde(default)]
    pub graph_memory: bool,

    /// Maximum graph traversal depth for context building
    #[serde(default = "AgentProfile::default_graph_depth")]
    pub graph_depth: usize,

    /// Weight for graph-based relevance vs semantic similarity (0.0 to 1.0)
    #[serde(default = "AgentProfile::default_graph_weight")]
    pub graph_weight: f32,

    /// Automatically build graph from conversations
    #[serde(default)]
    pub auto_graph: bool,

    /// Graph-based tool recommendation threshold (0.0 to 1.0)
    #[serde(default = "AgentProfile::default_graph_threshold")]
    pub graph_threshold: f32,

    /// Use graph for decision steering
    #[serde(default)]
    pub graph_steering: bool,

    // ========== Multi-Model Reasoning Configuration ==========
    /// Enable fast reasoning with a smaller model
    #[serde(default)]
    pub fast_reasoning: bool,

    /// Model provider for fast reasoning (e.g., "mlx", "ollama")
    #[serde(default)]
    pub fast_model_provider: Option<String>,

    /// Model name for fast reasoning (e.g., "mlx-community/Llama-3.2-3B-Instruct-4bit")
    #[serde(default)]
    pub fast_model_name: Option<String>,

    /// Temperature for fast model (typically lower for consistency)
    #[serde(default = "AgentProfile::default_fast_temperature")]
    pub fast_model_temperature: f32,

    /// Tasks to delegate to fast model
    #[serde(default = "AgentProfile::default_fast_tasks")]
    pub fast_model_tasks: Vec<String>,

    /// Confidence threshold to escalate to main model
    #[serde(default = "AgentProfile::default_escalation_threshold")]
    pub escalation_threshold: f32,

    /// Display reasoning summary to user (requires fast model for summarization)
    #[serde(default)]
    pub show_reasoning: bool,
}

impl AgentProfile {
    fn default_memory_k() -> usize {
        10
    }

    fn default_top_p() -> f32 {
        0.9
    }

    fn default_graph_depth() -> usize {
        3
    }

    fn default_graph_weight() -> f32 {
        0.5 // Equal weight to graph and semantic
    }

    fn default_graph_threshold() -> f32 {
        0.7 // Recommend tools with >70% relevance
    }

    fn default_fast_temperature() -> f32 {
        0.3 // Lower temperature for consistency in fast model
    }

    fn default_fast_tasks() -> Vec<String> {
        vec![
            "entity_extraction".to_string(),
            "graph_analysis".to_string(),
            "decision_routing".to_string(),
            "tool_selection".to_string(),
            "confidence_scoring".to_string(),
        ]
    }

    fn default_escalation_threshold() -> f32 {
        0.6 // Escalate to main model if confidence < 60%
    }

    /// Validate the agent profile configuration
    pub fn validate(&self) -> Result<()> {
        // Validate temperature if specified
        if let Some(temp) = self.temperature {
            if temp < 0.0 || temp > 2.0 {
                return Err(AgentError::Invalid(format!(
                    "temperature must be between 0.0 and 2.0, got {}",
                    temp
                ))
                .into());
            }
        }

        // Validate top_p
        if self.top_p < 0.0 || self.top_p > 1.0 {
            return Err(AgentError::Invalid(format!(
                "top_p must be between 0.0 and 1.0, got {}",
                self.top_p
            ))
            .into());
        }

        // Validate graph_weight
        if self.graph_weight < 0.0 || self.graph_weight > 1.0 {
            return Err(AgentError::Invalid(format!(
                "graph_weight must be between 0.0 and 1.0, got {}",
                self.graph_weight
            ))
            .into());
        }

        // Validate graph_threshold
        if self.graph_threshold < 0.0 || self.graph_threshold > 1.0 {
            return Err(AgentError::Invalid(format!(
                "graph_threshold must be between 0.0 and 1.0, got {}",
                self.graph_threshold
            ))
            .into());
        }

        // Validate that allowed_tools and denied_tools don't overlap
        if let (Some(allowed), Some(denied)) = (&self.allowed_tools, &self.denied_tools) {
            let allowed_set: HashSet<_> = allowed.iter().collect();
            let denied_set: HashSet<_> = denied.iter().collect();
            let overlap: Vec<_> = allowed_set.intersection(&denied_set).collect();

            if !overlap.is_empty() {
                return Err(AgentError::Invalid(format!(
                    "tools cannot be both allowed and denied: {:?}",
                    overlap
                ))
                .into());
            }
        }

        // Validate model provider if specified
        if let Some(provider) = &self.model_provider {
            let valid_providers = vec!["mock", "openai", "anthropic", "ollama", "mlx"];
            if !valid_providers.contains(&provider.as_str()) {
                return Err(AgentError::Invalid(format!(
                    "model_provider must be one of: {}. Got: {}",
                    valid_providers.join(", "),
                    provider
                ))
                .into());
            }
        }

        Ok(())
    }

    /// Check if a tool is allowed for this agent
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // If denied list exists and contains the tool, deny it
        if let Some(denied) = &self.denied_tools {
            if denied.iter().any(|t| t == tool_name) {
                return false;
            }
        }

        // If allowed list exists, only allow tools in the list
        if let Some(allowed) = &self.allowed_tools {
            return allowed.iter().any(|t| t == tool_name);
        }

        // If no restrictions, allow all tools
        true
    }

    /// Get the effective temperature (override or default)
    pub fn effective_temperature(&self, default: f32) -> f32 {
        self.temperature.unwrap_or(default)
    }

    /// Get the effective model provider (override or default)
    pub fn effective_provider<'a>(&'a self, default: &'a str) -> &'a str {
        self.model_provider.as_deref().unwrap_or(default)
    }

    /// Get the effective model name (override or default)
    pub fn effective_model_name<'a>(&'a self, default: Option<&'a str>) -> Option<&'a str> {
        self.model_name.as_deref().or(default)
    }
}

impl Default for AgentProfile {
    fn default() -> Self {
        Self {
            prompt: None,
            style: None,
            temperature: None,
            model_provider: None,
            model_name: None,
            allowed_tools: None,
            denied_tools: None,
            memory_k: Self::default_memory_k(),
            top_p: Self::default_top_p(),
            max_context_tokens: None,
            enable_graph: true, // Enable by default
            graph_memory: true, // Enable by default
            graph_depth: Self::default_graph_depth(),
            graph_weight: Self::default_graph_weight(),
            auto_graph: true, // Enable by default
            graph_threshold: Self::default_graph_threshold(),
            graph_steering: true,                         // Enable by default
            fast_reasoning: true,                         // Enable multi-model by default
            fast_model_provider: Some("mlx".to_string()), // Default to MLX for Apple Silicon
            fast_model_name: Some("mlx-community/Llama-3.2-3B-Instruct-4bit".to_string()),
            fast_model_temperature: Self::default_fast_temperature(),
            fast_model_tasks: Self::default_fast_tasks(),
            escalation_threshold: Self::default_escalation_threshold(),
            show_reasoning: false, // Disabled by default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agent_profile() {
        let profile = AgentProfile::default();
        assert_eq!(profile.memory_k, 10);
        assert_eq!(profile.top_p, 0.9);

        // Verify multi-model is enabled by default
        assert!(profile.fast_reasoning);
        assert_eq!(profile.fast_model_provider, Some("mlx".to_string()));
        assert_eq!(
            profile.fast_model_name,
            Some("mlx-community/Llama-3.2-3B-Instruct-4bit".to_string())
        );
        assert_eq!(profile.fast_model_temperature, 0.3);
        assert_eq!(profile.escalation_threshold, 0.6);

        // Verify knowledge graph is enabled by default
        assert!(profile.enable_graph);
        assert!(profile.graph_memory);
        assert!(profile.auto_graph);
        assert!(profile.graph_steering);

        assert!(profile.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_temperature() {
        let mut profile = AgentProfile::default();
        profile.temperature = Some(3.0);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_top_p() {
        let mut profile = AgentProfile::default();
        profile.top_p = 1.5;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn test_validate_tool_overlap() {
        let mut profile = AgentProfile::default();
        profile.allowed_tools = Some(vec!["tool1".to_string(), "tool2".to_string()]);
        profile.denied_tools = Some(vec!["tool2".to_string(), "tool3".to_string()]);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn test_is_tool_allowed_no_restrictions() {
        let profile = AgentProfile::default();
        assert!(profile.is_tool_allowed("any_tool"));
    }

    #[test]
    fn test_is_tool_allowed_with_allowlist() {
        let mut profile = AgentProfile::default();
        profile.allowed_tools = Some(vec!["tool1".to_string(), "tool2".to_string()]);

        assert!(profile.is_tool_allowed("tool1"));
        assert!(profile.is_tool_allowed("tool2"));
        assert!(!profile.is_tool_allowed("tool3"));
    }

    #[test]
    fn test_is_tool_allowed_with_denylist() {
        let mut profile = AgentProfile::default();
        profile.denied_tools = Some(vec!["tool1".to_string()]);

        assert!(!profile.is_tool_allowed("tool1"));
        assert!(profile.is_tool_allowed("tool2"));
    }

    #[test]
    fn test_effective_temperature() {
        let mut profile = AgentProfile::default();
        assert_eq!(profile.effective_temperature(0.7), 0.7);

        profile.temperature = Some(0.5);
        assert_eq!(profile.effective_temperature(0.7), 0.5);
    }

    #[test]
    fn test_effective_provider() {
        let mut profile = AgentProfile::default();
        assert_eq!(profile.effective_provider("mock"), "mock");

        profile.model_provider = Some("openai".to_string());
        assert_eq!(profile.effective_provider("mock"), "openai");
    }
}
