pub mod plugin;
pub mod plugins;
pub mod registry;

use crate::persistence::Persistence;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use plugins::RustCargoPlugin;
use registry::PluginRegistry;

#[derive(Debug)]
pub struct BootstrapOutcome {
    pub repository_node_id: i64,
    pub nodes_created: usize,
    pub edges_created: usize,
    pub repository_name: String,
    pub component_count: usize,
    pub document_count: usize,
    pub phases: Vec<String>,
}

pub struct BootstrapSelf<'a> {
    persistence: &'a Persistence,
    session_id: &'a str,
    repo_root: PathBuf,
    plugins: PluginRegistry,
}

impl<'a> BootstrapSelf<'a> {
    pub fn new(persistence: &'a Persistence, session_id: &'a str, repo_root: PathBuf) -> Self {
        Self {
            persistence,
            session_id,
            repo_root,
            plugins: PluginRegistry::new(),
        }
    }

    pub fn from_environment(persistence: &'a Persistence, session_id: &'a str) -> Result<Self> {
        let repo_root = resolve_repo_root()?;
        Ok(Self::new(persistence, session_id, repo_root))
    }

    /// Initialize the plugin registry with default plugins
    fn init_plugins(&self) -> Result<()> {
        self.plugins
            .register(Arc::new(RustCargoPlugin))?;
        Ok(())
    }

    /// Run bootstrap with specified plugins, or auto-detect if plugins is None
    pub fn run_with_plugins(&self, plugins: Option<Vec<String>>) -> Result<BootstrapOutcome> {
        self.init_plugins()?;

        let active_plugins = if let Some(plugin_names) = plugins {
            // Use specified plugins
            self.plugins.get_by_names(&plugin_names)?
        } else {
            // Auto-detect plugins
            self.plugins.get_enabled(&self.repo_root)?
        };

        if active_plugins.is_empty() {
            return Err(anyhow!(
                "No bootstrap plugins found for repository at {}",
                self.repo_root.display()
            ));
        }

        let context = plugin::PluginContext {
            persistence: self.persistence,
            session_id: self.session_id,
            repo_root: &self.repo_root,
        };

        let mut total_nodes = 0;
        let mut total_edges = 0;
        let mut all_phases = Vec::new();
        let mut repository_name = String::new();
        let mut component_count = 0;
        let mut document_count = 0;
        let mut root_node_id = None;

        for plugin in active_plugins {
            let outcome = plugin.run(context.clone())?;

            total_nodes += outcome.nodes_created;
            total_edges += outcome.edges_created;
            all_phases.extend(outcome.phases);

            // Use the first plugin's root node ID
            if root_node_id.is_none() {
                root_node_id = outcome.root_node_id;
            }

            // Extract metadata from first plugin that provides it
            if let Some(name) = outcome.metadata.get("repository_name").and_then(|v| v.as_str()) {
                repository_name = name.to_string();
            }
            if let Some(count) = outcome.metadata.get("component_count").and_then(|v| v.as_u64()) {
                component_count = component_count.max(count as usize);
            }
            if let Some(count) = outcome.metadata.get("document_count").and_then(|v| v.as_u64()) {
                document_count = document_count.max(count as usize);
            }
        }

        let repository_node_id =
            root_node_id.ok_or_else(|| anyhow!("No repository node created by plugins"))?;

        Ok(BootstrapOutcome {
            repository_node_id,
            nodes_created: total_nodes,
            edges_created: total_edges,
            repository_name,
            component_count,
            document_count,
            phases: all_phases,
        })
    }

    /// Run bootstrap with auto-detection (backward compatibility)
    pub fn run(&self) -> Result<BootstrapOutcome> {
        self.run_with_plugins(None)
    }
}

pub fn resolve_repo_root() -> Result<PathBuf> {
    if let Ok(override_path) = std::env::var("SPEC_AI_BOOTSTRAP_ROOT") {
        let candidate = PathBuf::from(override_path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let cwd = std::env::current_dir().context("resolving current directory")?;
    find_repo_root(&cwd).ok_or_else(|| {
        anyhow!(
            "Unable to find repository root starting from {}",
            cwd.display()
        )
    })
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() || current.join("Cargo.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}
