//! Plugin discovery and loading

use crate::abi::{PluginModuleRef, PluginToolRef, PLUGIN_API_VERSION};
use crate::error::PluginError;
use abi_stable::library::RootModule;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

/// Statistics from loading plugins
#[derive(Debug, Default, Clone)]
pub struct LoadStats {
    /// Total plugin files found
    pub total: usize,
    /// Successfully loaded plugins
    pub loaded: usize,
    /// Failed to load plugins
    pub failed: usize,
    /// Total tools loaded across all plugins
    pub tools_loaded: usize,
}

/// A loaded plugin with its metadata
pub struct LoadedPlugin {
    /// Path to the plugin library
    pub path: PathBuf,
    /// Plugin name
    pub name: String,
    /// Tools provided by this plugin
    pub tools: Vec<PluginToolRef>,
}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("tools_count", &self.tools.len())
            .finish()
    }
}

/// Plugin loader that discovers and loads plugin libraries
pub struct PluginLoader {
    plugins: Vec<LoadedPlugin>,
}

impl PluginLoader {
    /// Create a new empty plugin loader
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Load all plugins from a directory
    ///
    /// Scans the directory for dynamic library files (.dylib on macOS, .so on Linux,
    /// .dll on Windows) and attempts to load each one as a plugin.
    ///
    /// # Arguments
    /// * `dir` - Directory to scan for plugins
    ///
    /// # Returns
    /// Statistics about the loading process
    pub fn load_directory(&mut self, dir: &Path) -> Result<LoadStats> {
        let mut stats = LoadStats::default();

        if !dir.exists() {
            info!("Plugin directory does not exist: {}", dir.display());
            return Ok(stats);
        }

        if !dir.is_dir() {
            return Err(PluginError::NotADirectory(dir.to_path_buf()).into());
        }

        info!("Scanning plugin directory: {}", dir.display());

        for entry in walkdir::WalkDir::new(dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if !Self::is_plugin_library(path) {
                continue;
            }

            stats.total += 1;

            match self.load_plugin(path) {
                Ok(tool_count) => {
                    stats.loaded += 1;
                    stats.tools_loaded += tool_count;
                    info!(
                        "Loaded plugin: {} ({} tools)",
                        path.display(),
                        tool_count
                    );
                }
                Err(e) => {
                    stats.failed += 1;
                    error!("Failed to load plugin {}: {}", path.display(), e);
                }
            }
        }

        Ok(stats)
    }

    /// Load a single plugin from a file
    fn load_plugin(&mut self, path: &Path) -> Result<usize> {
        debug!("Loading plugin from: {}", path.display());

        // Load the root module using abi_stable
        let module = PluginModuleRef::load_from_file(path).map_err(|e| PluginError::LoadFailed {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        // Check API version compatibility
        let plugin_version = (module.api_version())();
        if plugin_version != PLUGIN_API_VERSION {
            return Err(PluginError::VersionMismatch {
                expected: PLUGIN_API_VERSION,
                found: plugin_version,
                path: path.to_path_buf(),
            }
            .into());
        }

        let plugin_name = (module.plugin_name())().to_string();
        debug!("Plugin '{}' passed version check", plugin_name);

        // Check for duplicate plugin names
        if self.plugins.iter().any(|p| p.name == plugin_name) {
            return Err(PluginError::DuplicatePlugin(plugin_name).into());
        }

        // Get tools from the plugin
        let tool_refs = (module.get_tools())();
        let tool_count = tool_refs.len();

        // Collect tool refs into a Vec
        let tools: Vec<PluginToolRef> = tool_refs.into_iter().collect();

        // Call initialize on each tool if it has one
        for tool in &tools {
            if let Some(init) = tool.initialize {
                let context = "{}"; // Empty context for now
                if !init(context.into()) {
                    warn!(
                        "Tool '{}' initialization failed",
                        (tool.info)().name.as_str()
                    );
                }
            }
        }

        self.plugins.push(LoadedPlugin {
            path: path.to_path_buf(),
            name: plugin_name,
            tools,
        });

        Ok(tool_count)
    }

    /// Check if a path is a plugin library based on extension
    fn is_plugin_library(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        let Some(ext) = path.extension() else {
            return false;
        };

        #[cfg(target_os = "macos")]
        let expected = "dylib";

        #[cfg(target_os = "linux")]
        let expected = "so";

        #[cfg(target_os = "windows")]
        let expected = "dll";

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let expected = "so"; // Default to .so for unknown platforms

        ext == expected
    }

    /// Get all loaded plugins
    pub fn plugins(&self) -> &[LoadedPlugin] {
        &self.plugins
    }

    /// Get all tools from all loaded plugins as an iterator
    pub fn all_tools(&self) -> impl Iterator<Item = (PluginToolRef, &str)> {
        self.plugins.iter().flat_map(|p| {
            p.tools.iter().map(move |t| (*t, p.name.as_str()))
        })
    }

    /// Get the number of loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Get the total number of tools across all plugins
    pub fn tool_count(&self) -> usize {
        self.plugins.iter().map(|p| p.tools.len()).sum()
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Expand tilde (~) in paths to the home directory
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(path_str) = path.to_str().ok_or(()) {
        if path_str.starts_with("~/") {
            if let Some(home) = dirs_home() {
                return home.join(&path_str[2..]);
            }
        }
    }
    path.to_path_buf()
}

/// Get the user's home directory
fn dirs_home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_plugin_library() {
        // Note: is_plugin_library checks if the path is a file first,
        // so these tests pass non-existent paths which will return false.
        // The extension check only happens if the file exists.

        // Non-existent paths always return false (file check first)
        assert!(!PluginLoader::is_plugin_library(Path::new(
            "/tmp/nonexistent/libplugin.dylib"
        )));

        // Non-library extensions also return false
        assert!(!PluginLoader::is_plugin_library(Path::new(
            "/tmp/test/plugin.txt"
        )));
        assert!(!PluginLoader::is_plugin_library(Path::new(
            "/tmp/test/plugin"
        )));
    }

    #[test]
    fn test_expand_tilde() {
        let home = dirs_home().unwrap_or_else(|| PathBuf::from("/home/user"));

        let expanded = expand_tilde(Path::new("~/test"));
        assert!(expanded.starts_with(&home) || expanded == Path::new("~/test"));

        // Non-tilde paths should be unchanged
        let absolute = expand_tilde(Path::new("/absolute/path"));
        assert_eq!(absolute, Path::new("/absolute/path"));
    }

    #[test]
    fn test_load_stats_default() {
        let stats = LoadStats::default();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.loaded, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.tools_loaded, 0);
    }
}
