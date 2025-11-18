pub mod builtin;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

use self::builtin::{
    AudioTranscriptionTool, BashTool, EchoTool, FileExtractTool, FileReadTool, FileWriteTool,
    GraphTool, MathTool, PromptUserTool, SearchTool, ShellTool, WebSearchTool,
};
use crate::persistence::Persistence;

#[cfg(feature = "openai")]
use async_openai::types::ChatCompletionTool;

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Output from the tool
    pub output: String,
    /// Error message if execution failed
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    /// Create a failure result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
        }
    }
}

/// Trait for all tools that can be executed by the agent
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters
    fn parameters(&self) -> Value;

    /// Execute the tool with the given arguments
    async fn execute(&self, args: Value) -> Result<ToolResult>;
}

/// Registry for managing and executing tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Create a registry populated with all built-in tools.
    ///
    /// Tools that require persistence (e.g., `graph`) are only registered when
    /// an [`Arc<Persistence>`] is provided.
    pub fn with_builtin_tools(persistence: Option<Arc<Persistence>>) -> Self {
        let mut registry = Self::new();

        // Register all built-in tools
        registry.register(Arc::new(EchoTool::new()));
        registry.register(Arc::new(MathTool::new()));
        registry.register(Arc::new(FileReadTool::new()));
        registry.register(Arc::new(FileExtractTool::new()));
        registry.register(Arc::new(FileWriteTool::new()));
        registry.register(Arc::new(PromptUserTool::new()));
        registry.register(Arc::new(SearchTool::new()));
        registry.register(Arc::new(BashTool::new()));
        registry.register(Arc::new(ShellTool::new()));
        registry.register(Arc::new(WebSearchTool::new()));

        if let Some(persistence) = persistence {
            registry.register(Arc::new(GraphTool::new(persistence.clone())));
            registry.register(Arc::new(AudioTranscriptionTool::with_persistence(
                persistence,
            )));
        } else {
            registry.register(Arc::new(AudioTranscriptionTool::new()));
        }

        tracing::debug!("ToolRegistry created with {} tools", registry.tools.len());
        for name in registry.tools.keys() {
            tracing::debug!("  - Tool: {}", name);
        }

        registry
    }

    /// Register a tool in the registry
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tool names
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a tool is registered
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Execute a tool by name with the given arguments
    pub async fn execute(&self, name: &str, args: Value) -> Result<ToolResult> {
        let tool = self
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;

        debug!("Executing tool '{}'", name);
        let result = tool.execute(args).await;
        match &result {
            Ok(res) => {
                debug!(
                    "Tool '{}' completed: success={}, error={:?}",
                    name, res.success, res.error
                );
            }
            Err(err) => {
                debug!("Tool '{}' failed to execute: {}", name, err);
            }
        }
        result
    }

    /// Get the number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Convert all tools in the registry to OpenAI ChatCompletionTool format.
    ///
    /// Used by providers that support native function calling (OpenAI-compatible,
    /// including MLX and LM Studio when enabled).
    #[cfg(any(feature = "openai", feature = "mlx", feature = "lmstudio"))]
    pub fn to_openai_tools(&self) -> Vec<ChatCompletionTool> {
        use crate::agent::function_calling::tool_to_openai_function;

        self.tools
            .values()
            .map(|tool| {
                tool_to_openai_function(tool.name(), tool.description(), &tool.parameters())
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }

        fn description(&self) -> &str {
            "A dummy tool for testing"
        }

        fn parameters(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn execute(&self, _args: Value) -> Result<ToolResult> {
            Ok(ToolResult::success("dummy output"))
        }
    }

    #[tokio::test]
    async fn test_register_and_get_tool() {
        let mut registry = ToolRegistry::new();
        let tool = Arc::new(DummyTool);

        registry.register(tool.clone());

        assert!(registry.has("dummy"));
        assert!(registry.get("dummy").is_some());
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn test_list_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));

        let tools = registry.list();
        assert_eq!(tools.len(), 1);
        assert!(tools.contains(&"dummy"));
    }

    #[tokio::test]
    async fn test_execute_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));

        let result = registry.execute("dummy", Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "dummy output");
    }

    #[tokio::test]
    async fn test_execute_nonexistent_tool() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", Value::Null).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tool_result_success() {
        let result = ToolResult::success("test output");
        assert!(result.success);
        assert_eq!(result.output, "test output");
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_tool_result_failure() {
        let result = ToolResult::failure("test error");
        assert!(!result.success);
        assert_eq!(result.error, Some("test error".to_string()));
    }
}
