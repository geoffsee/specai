//! ABI-stable types for the plugin interface
//!
//! This module defines the stable interface between the host application and plugins.
//! All types here use `abi_stable` to ensure binary compatibility across different
//! compiler versions.

use abi_stable::{
    declare_root_module_statics,
    library::RootModule,
    package_version_strings,
    sabi_types::VersionStrings,
    std_types::{ROption, RStr, RString, RVec},
    StableAbi,
};

/// Version of the plugin API.
/// Bump this when making breaking changes to the plugin interface.
pub const PLUGIN_API_VERSION: u32 = 1;

/// ABI-stable result type for plugin tool execution
#[repr(C)]
#[derive(StableAbi, Debug, Clone)]
pub struct PluginToolResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Output from the tool (empty on failure)
    pub output: RString,
    /// Error message if execution failed
    pub error: ROption<RString>,
}

impl PluginToolResult {
    /// Create a successful result
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: RString::from(output.into()),
            error: ROption::RNone,
        }
    }

    /// Create a failure result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: RString::new(),
            error: ROption::RSome(RString::from(error.into())),
        }
    }
}

/// ABI-stable tool metadata
#[repr(C)]
#[derive(StableAbi, Debug, Clone)]
pub struct PluginToolInfo {
    /// Unique name of the tool
    pub name: RString,
    /// Human-readable description of what the tool does
    pub description: RString,
    /// JSON Schema describing the tool's parameters (as JSON string)
    pub parameters_json: RString,
}

impl PluginToolInfo {
    /// Create new tool info
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_json: impl Into<String>,
    ) -> Self {
        Self {
            name: RString::from(name.into()),
            description: RString::from(description.into()),
            parameters_json: RString::from(parameters_json.into()),
        }
    }
}

/// ABI-stable tool interface that plugins implement.
///
/// This struct contains function pointers for the tool's operations.
/// Plugins create static instances of this struct for each tool they provide.
#[repr(C)]
#[derive(StableAbi)]
pub struct PluginTool {
    /// Get tool metadata (name, description, parameters schema)
    pub info: extern "C" fn() -> PluginToolInfo,

    /// Execute the tool with JSON-encoded arguments
    ///
    /// # Arguments
    /// * `args_json` - JSON string containing the tool arguments
    ///
    /// # Returns
    /// Result containing output or error message
    pub execute: extern "C" fn(args_json: RStr<'_>) -> PluginToolResult,

    /// Optional: Initialize plugin with host context
    ///
    /// Called once when the plugin is loaded. Can be used to set up
    /// resources or validate the environment.
    ///
    /// # Arguments
    /// * `context_json` - JSON string with context from the host (currently empty object)
    ///
    /// # Returns
    /// `true` if initialization succeeded, `false` to abort loading
    pub initialize: Option<extern "C" fn(context_json: RStr<'_>) -> bool>,
}

/// Reference to a PluginTool for use in collections
pub type PluginToolRef = &'static PluginTool;

/// Root module that plugins export.
///
/// This is the entry point for the plugin. The host loads this module
/// and uses it to discover and access the plugin's tools.
#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = PluginModuleRef)))]
pub struct PluginModule {
    /// Get the plugin API version
    ///
    /// Must return `PLUGIN_API_VERSION` for compatibility
    pub api_version: extern "C" fn() -> u32,

    /// Get all tools provided by this plugin
    pub get_tools: extern "C" fn() -> RVec<PluginToolRef>,

    /// Get the plugin name for identification
    pub plugin_name: extern "C" fn() -> RString,

    /// Optional cleanup function called when the plugin is unloaded
    #[sabi(last_prefix_field)]
    pub shutdown: Option<extern "C" fn()>,
}

impl RootModule for PluginModuleRef {
    declare_root_module_statics! {PluginModuleRef}

    const BASE_NAME: &'static str = "spec_ai_plugin";
    const NAME: &'static str = "spec_ai_plugin";
    const VERSION_STRINGS: VersionStrings = package_version_strings!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_tool_result_success() {
        let result = PluginToolResult::success("test output");
        assert!(result.success);
        assert_eq!(result.output.as_str(), "test output");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_plugin_tool_result_failure() {
        let result = PluginToolResult::failure("test error");
        assert!(!result.success);
        assert!(result.output.is_empty());
        match &result.error {
            ROption::RSome(s) => assert_eq!(s.as_str(), "test error"),
            ROption::RNone => panic!("Expected error message"),
        }
    }

    #[test]
    fn test_plugin_tool_info() {
        let info = PluginToolInfo::new("test", "A test tool", r#"{"type": "object"}"#);
        assert_eq!(info.name.as_str(), "test");
        assert_eq!(info.description.as_str(), "A test tool");
        assert_eq!(info.parameters_json.as_str(), r#"{"type": "object"}"#);
    }
}
