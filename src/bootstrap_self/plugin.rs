use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;

use crate::persistence::Persistence;

/// Context provided to each bootstrap plugin
#[derive(Clone)]
pub struct PluginContext<'a> {
    pub persistence: &'a Persistence,
    pub session_id: &'a str,
    pub repo_root: &'a PathBuf,
}

/// Outcome from a single plugin's bootstrap run
#[derive(Debug, Clone)]
pub struct PluginOutcome {
    pub plugin_name: String,
    pub nodes_created: usize,
    pub edges_created: usize,
    pub root_node_id: Option<i64>, // The main entity node created by this plugin
    pub phases: Vec<String>,
    pub metadata: serde_json::Value,
}

impl PluginOutcome {
    pub fn new(plugin_name: impl Into<String>) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            nodes_created: 0,
            edges_created: 0,
            root_node_id: None,
            phases: Vec::new(),
            metadata: json!({}),
        }
    }
}

/// Trait that bootstrap plugins must implement
pub trait BootstrapPlugin: Send + Sync {
    /// The name of this plugin (e.g., "rust-cargo", "python-pyproject")
    fn name(&self) -> &'static str;

    /// Returns the bootstrap phases this plugin will execute
    fn phases(&self) -> Vec<&'static str>;

    /// Returns true if this plugin should be auto-activated for the given repository
    fn should_activate(&self, repo_root: &PathBuf) -> bool;

    /// Execute the bootstrap for this plugin
    fn run(&self, context: PluginContext) -> Result<PluginOutcome>;
}
