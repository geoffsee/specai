//! Plugin-specific error types

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during plugin loading and execution
#[derive(Error, Debug)]
pub enum PluginError {
    /// Plugin directory is not a directory
    #[error("Plugin directory is not a directory: {0}")]
    NotADirectory(PathBuf),

    /// Failed to load plugin from file
    #[error("Failed to load plugin from {path}: {message}")]
    LoadFailed { path: PathBuf, message: String },

    /// Plugin API version doesn't match host
    #[error("Plugin API version mismatch: expected {expected}, found {found} in {path}")]
    VersionMismatch {
        expected: u32,
        found: u32,
        path: PathBuf,
    },

    /// Duplicate plugin name
    #[error("Duplicate plugin name: {0}")]
    DuplicatePlugin(String),

    /// Duplicate tool name
    #[error("Duplicate tool name '{tool}' from plugin '{plugin}'")]
    DuplicateTool { tool: String, plugin: String },

    /// Invalid tool info from plugin
    #[error("Invalid tool info from plugin: {0}")]
    InvalidToolInfo(String),

    /// Plugin execution failed
    #[error("Plugin execution failed: {0}")]
    ExecutionFailed(String),
}
