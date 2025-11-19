use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::plugin::BootstrapPlugin;
use anyhow::{anyhow, Result};

pub struct PluginRegistry {
    plugins: Mutex<Vec<Arc<dyn BootstrapPlugin>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Mutex::new(Vec::new()),
        }
    }

    /// Register a plugin
    pub fn register(&self, plugin: Arc<dyn BootstrapPlugin>) -> Result<()> {
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to lock plugin registry: {}", e))?;
        plugins.push(plugin);
        Ok(())
    }

    /// Get a plugin by name
    pub fn get_by_name(&self, name: &str) -> Result<Option<Arc<dyn BootstrapPlugin>>> {
        let plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to lock plugin registry: {}", e))?;
        Ok(plugins.iter().find(|p| p.name() == name).cloned())
    }

    /// Get all registered plugins
    pub fn all_plugins(&self) -> Result<Vec<Arc<dyn BootstrapPlugin>>> {
        let plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to lock plugin registry: {}", e))?;
        Ok(plugins.clone())
    }

    /// Get plugins that should auto-activate for the given repo
    pub fn get_enabled(&self, repo_root: &PathBuf) -> Result<Vec<Arc<dyn BootstrapPlugin>>> {
        let plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to lock plugin registry: {}", e))?;
        Ok(plugins
            .iter()
            .filter(|p| p.should_activate(repo_root))
            .cloned()
            .collect())
    }

    /// Get plugins by a list of names
    pub fn get_by_names(&self, names: &[String]) -> Result<Vec<Arc<dyn BootstrapPlugin>>> {
        let plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to lock plugin registry: {}", e))?;
        let mut result = Vec::new();
        for name in names {
            let plugin = plugins
                .iter()
                .find(|p| p.name() == name)
                .ok_or_else(|| anyhow!("Plugin not found: {}", name))?
                .clone();
            result.push(plugin);
        }
        Ok(result)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        match self.plugins.lock() {
            Ok(plugins) => Self {
                plugins: Mutex::new(plugins.clone()),
            },
            Err(_) => Self::new(),
        }
    }
}
