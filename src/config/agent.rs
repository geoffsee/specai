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
}

impl AgentProfile {
    fn default_memory_k() -> usize {
        10
    }

    fn default_top_p() -> f32 {
        0.9
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
