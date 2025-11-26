//! Plugin system for custom tools in spec-ai
//!
//! This crate provides the infrastructure for loading and running custom tools
//! implemented as dynamic libraries (`.dylib` on macOS, `.so` on Linux, `.dll` on Windows).
//!
//! # For Plugin Authors
//!
//! To create a plugin, add this crate as a dependency with the `plugin-api` feature:
//!
//! ```toml
//! [dependencies]
//! spec-ai-plugin = { version = "0.4", features = ["plugin-api"] }
//! ```
//!
//! Then implement your tools using the ABI-stable types:
//!
//! ```rust,ignore
//! use abi_stable::std_types::{RStr, RString, RVec};
//! use spec_ai_plugin::abi::{
//!     PluginModule, PluginModuleRef, PluginTool, PluginToolInfo,
//!     PluginToolRef, PluginToolResult, PLUGIN_API_VERSION,
//! };
//!
//! // Define your tool
//! extern "C" fn my_tool_info() -> PluginToolInfo {
//!     PluginToolInfo::new(
//!         "my_tool",
//!         "Description of my tool",
//!         r#"{"type": "object", "properties": {}}"#,
//!     )
//! }
//!
//! extern "C" fn my_tool_execute(args_json: RStr<'_>) -> PluginToolResult {
//!     PluginToolResult::success("Tool executed successfully")
//! }
//!
//! static MY_TOOL: PluginTool = PluginTool {
//!     info: my_tool_info,
//!     execute: my_tool_execute,
//!     initialize: None,
//! };
//!
//! // Export the plugin module
//! extern "C" fn api_version() -> u32 { PLUGIN_API_VERSION }
//! extern "C" fn plugin_name() -> RString { RString::from("my-plugin") }
//! extern "C" fn get_tools() -> RVec<PluginToolRef> {
//!     RVec::from(vec![&MY_TOOL])
//! }
//!
//! #[abi_stable::export_root_module]
//! fn get_library() -> PluginModuleRef {
//!     PluginModuleRef::from_prefix(PluginModule {
//!         api_version,
//!         plugin_name,
//!         get_tools,
//!         shutdown: None,
//!     })
//! }
//! ```
//!
//! # For Host Applications
//!
//! Use the [`loader::PluginLoader`] to discover and load plugins from a directory:
//!
//! ```rust,ignore
//! use spec_ai_plugin::loader::{PluginLoader, expand_tilde};
//! use std::path::Path;
//!
//! let mut loader = PluginLoader::new();
//! let stats = loader.load_directory(&expand_tilde(Path::new("~/.spec-ai/tools")))?;
//!
//! println!("Loaded {} plugins with {} tools", stats.loaded, stats.tools_loaded);
//!
//! for (tool, plugin_name) in loader.all_tools() {
//!     let info = (tool.info)();
//!     println!("  - {} from {}", info.name, plugin_name);
//! }
//! ```

pub mod abi;
pub mod error;
pub mod loader;

// Re-export commonly used types
pub use abi::{
    PluginModule, PluginModuleRef, PluginTool, PluginToolInfo, PluginToolRef, PluginToolResult,
    PLUGIN_API_VERSION,
};
pub use error::PluginError;
pub use loader::{expand_tilde, LoadStats, LoadedPlugin, PluginLoader};
