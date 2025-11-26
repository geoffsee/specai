//! Adapter for wrapping plugin tools as async Tool implementations

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use spec_ai_plugin::{PluginToolRef, PluginToolResult as PluginResult};

use super::{Tool, ToolResult};

/// Wraps an ABI-stable plugin tool and implements the async Tool trait
pub struct PluginToolAdapter {
    tool_ref: PluginToolRef,
    /// Cached tool name
    name: String,
    /// Cached tool description
    description: String,
    /// Cached parameters schema
    parameters: Value,
    /// Name of the plugin this tool came from
    plugin_name: String,
}

impl PluginToolAdapter {
    /// Create a new adapter for a plugin tool
    ///
    /// # Arguments
    /// * `tool_ref` - Reference to the plugin tool
    /// * `plugin_name` - Name of the plugin for identification
    ///
    /// # Returns
    /// The adapter, or an error if the tool's parameters JSON is invalid
    pub fn new(tool_ref: PluginToolRef, plugin_name: impl Into<String>) -> Result<Self> {
        let info = (tool_ref.info)();

        let parameters: Value = serde_json::from_str(info.parameters_json.as_str())
            .map_err(|e| anyhow::anyhow!("Invalid parameters JSON from plugin: {}", e))?;

        Ok(Self {
            tool_ref,
            name: info.name.to_string(),
            description: info.description.to_string(),
            parameters,
            plugin_name: plugin_name.into(),
        })
    }

    /// Get the name of the plugin this tool came from
    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }
}

#[async_trait]
impl Tool for PluginToolAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        // Serialize arguments to JSON
        let args_json = serde_json::to_string(&args)?;

        // Call the plugin's execute function
        let result: PluginResult = (self.tool_ref.execute)(args_json.as_str().into());

        // Convert to our ToolResult
        Ok(ToolResult {
            success: result.success,
            output: result.output.to_string(),
            error: result.error.map(|e| e.to_string()).into_option(),
        })
    }
}

impl std::fmt::Debug for PluginToolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginToolAdapter")
            .field("name", &self.name)
            .field("plugin_name", &self.plugin_name)
            .field("description", &self.description)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Testing actual plugin loading requires a compiled plugin library.
    // These tests verify the adapter logic with mock data.

    #[test]
    fn test_tool_result_conversion() {
        // Test success conversion
        let plugin_result = PluginResult::success("test output");
        let result = ToolResult {
            success: plugin_result.success,
            output: plugin_result.output.to_string(),
            error: plugin_result.error.map(|e| e.to_string()).into_option(),
        };
        assert!(result.success);
        assert_eq!(result.output, "test output");
        assert!(result.error.is_none());

        // Test failure conversion
        let plugin_result = PluginResult::failure("test error");
        let result = ToolResult {
            success: plugin_result.success,
            output: plugin_result.output.to_string(),
            error: plugin_result.error.map(|e| e.to_string()).into_option(),
        };
        assert!(!result.success);
        assert_eq!(result.error, Some("test error".to_string()));
    }
}
