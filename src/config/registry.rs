use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::agent::AgentProfile;
use crate::persistence::Persistence;

const ACTIVE_AGENT_KEY: &str = "active_agent";

/// Registry for managing agent profiles and tracking the active agent
#[derive(Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, AgentProfile>>>,
    active_agent: Arc<RwLock<Option<String>>>,
    persistence: Persistence,
}

impl AgentRegistry {
    /// Create a new AgentRegistry with the given agents and persistence
    pub fn new(agents: HashMap<String, AgentProfile>, persistence: Persistence) -> Self {
        Self {
            agents: Arc::new(RwLock::new(agents)),
            active_agent: Arc::new(RwLock::new(None)),
            persistence,
        }
    }

    /// Initialize the registry by loading the active agent from persistence
    pub fn init(&self) -> Result<()> {
        // Load the active agent from persistence if it exists
        if let Some(entry) = self.persistence.policy_get(ACTIVE_AGENT_KEY)? {
            if let Some(agent_name) = entry.value.as_str() {
                // Validate that this agent still exists in the registry
                let agents = self.agents.read().unwrap();
                if agents.contains_key(agent_name) {
                    drop(agents);
                    let mut active = self.active_agent.write().unwrap();
                    *active = Some(agent_name.to_string());
                }
                // If the persisted agent doesn't exist, we'll leave active as None
                // and let the caller set a new default
            }
        }
        Ok(())
    }

    /// Set the active agent profile by name
    pub fn set_active(&self, name: &str) -> Result<()> {
        // Verify the agent exists
        let agents = self.agents.read().unwrap();
        if !agents.contains_key(name) {
            return Err(anyhow!(
                "Agent '{}' not found. Available agents: {}",
                name,
                if agents.is_empty() {
                    "none".to_string()
                } else {
                    agents
                        .keys()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ));
        }
        drop(agents);

        // Update in-memory state
        {
            let mut active = self.active_agent.write().unwrap();
            *active = Some(name.to_string());
        }

        // Persist to database
        let value = serde_json::json!(name);
        self.persistence
            .policy_upsert(ACTIVE_AGENT_KEY, &value)
            .context("persisting active agent")?;

        Ok(())
    }

    /// Get the currently active agent profile
    pub fn active(&self) -> Result<Option<(String, AgentProfile)>> {
        let active_name = {
            let active = self.active_agent.read().unwrap();
            active.clone()
        };

        if let Some(name) = active_name {
            let agents = self.agents.read().unwrap();
            if let Some(profile) = agents.get(&name) {
                Ok(Some((name.clone(), profile.clone())))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get the name of the currently active agent (if any)
    pub fn active_name(&self) -> Option<String> {
        let active = self.active_agent.read().unwrap();
        active.clone()
    }

    /// List all available agent profiles
    pub fn list(&self) -> Vec<String> {
        let agents = self.agents.read().unwrap();
        let mut names: Vec<_> = agents.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get a specific agent profile by name
    pub fn get(&self, name: &str) -> Option<AgentProfile> {
        let agents = self.agents.read().unwrap();
        agents.get(name).cloned()
    }

    /// Add or update an agent profile
    pub fn upsert(&self, name: String, profile: AgentProfile) -> Result<()> {
        profile
            .validate()
            .with_context(|| format!("validating agent profile '{}'", name))?;

        let mut agents = self.agents.write().unwrap();
        agents.insert(name, profile);
        Ok(())
    }

    /// Remove an agent profile
    pub fn remove(&self, name: &str) -> Result<()> {
        // Check if this is the active agent
        let active_name = self.active_name();
        if active_name.as_deref() == Some(name) {
            return Err(anyhow!(
                "Cannot remove '{}' because it is the currently active agent. \
                 Please switch to a different agent first.",
                name
            ));
        }

        let mut agents = self.agents.write().unwrap();
        if agents.remove(name).is_none() {
            return Err(anyhow!("Agent '{}' not found", name));
        }

        Ok(())
    }

    /// Check if an agent exists
    pub fn exists(&self, name: &str) -> bool {
        let agents = self.agents.read().unwrap();
        agents.contains_key(name)
    }

    /// Get the number of registered agents
    pub fn count(&self) -> usize {
        let agents = self.agents.read().unwrap();
        agents.len()
    }

    /// Get the shared persistence layer
    pub fn persistence(&self) -> &Persistence {
        &self.persistence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_profile() -> AgentProfile {
        AgentProfile {
            prompt: Some("Test prompt".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_new_registry() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let mut agents = HashMap::new();
        agents.insert("agent1".to_string(), create_test_profile());

        let registry = AgentRegistry::new(agents, persistence);
        assert_eq!(registry.count(), 1);
        assert!(registry.exists("agent1"));
    }

    #[test]
    fn test_set_and_get_active() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let mut agents = HashMap::new();
        agents.insert("agent1".to_string(), create_test_profile());
        agents.insert("agent2".to_string(), create_test_profile());

        let registry = AgentRegistry::new(agents, persistence);
        registry.init().unwrap();

        // Initially no active agent
        assert!(registry.active().unwrap().is_none());

        // Set active agent
        registry.set_active("agent1").unwrap();
        let active = registry.active().unwrap();
        assert!(active.is_some());
        assert_eq!(active.unwrap().0, "agent1");

        // Verify it's persisted
        assert_eq!(registry.active_name(), Some("agent1".to_string()));
    }

    #[test]
    fn test_set_active_nonexistent_agent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let agents = HashMap::new();
        let registry = AgentRegistry::new(agents, persistence);
        registry.init().unwrap();

        let result = registry.set_active("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_agents() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let mut agents = HashMap::new();
        agents.insert("zebra".to_string(), create_test_profile());
        agents.insert("alpha".to_string(), create_test_profile());
        agents.insert("beta".to_string(), create_test_profile());

        let registry = AgentRegistry::new(agents, persistence);
        let list = registry.list();

        // Should be sorted
        assert_eq!(list, vec!["alpha", "beta", "zebra"]);
    }

    #[test]
    fn test_upsert_and_remove() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let agents = HashMap::new();
        let registry = AgentRegistry::new(agents, persistence);
        registry.init().unwrap();

        // Add an agent
        registry
            .upsert("new_agent".to_string(), create_test_profile())
            .unwrap();
        assert!(registry.exists("new_agent"));
        assert_eq!(registry.count(), 1);

        // Remove the agent
        registry.remove("new_agent").unwrap();
        assert!(!registry.exists("new_agent"));
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_cannot_remove_active_agent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let mut agents = HashMap::new();
        agents.insert("agent1".to_string(), create_test_profile());

        let registry = AgentRegistry::new(agents, persistence);
        registry.init().unwrap();
        registry.set_active("agent1").unwrap();

        // Should not be able to remove the active agent
        let result = registry.remove("agent1");
        assert!(result.is_err());
    }

    #[test]
    fn test_persistence_across_restarts() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        // First session: set active agent
        {
            let persistence = Persistence::new(&db_path).unwrap();
            let mut agents = HashMap::new();
            agents.insert("agent1".to_string(), create_test_profile());

            let registry = AgentRegistry::new(agents, persistence);
            registry.init().unwrap();
            registry.set_active("agent1").unwrap();
        }

        // Second session: verify active agent is still set
        {
            let persistence = Persistence::new(&db_path).unwrap();
            let mut agents = HashMap::new();
            agents.insert("agent1".to_string(), create_test_profile());

            let registry = AgentRegistry::new(agents, persistence);
            registry.init().unwrap();

            assert_eq!(registry.active_name(), Some("agent1".to_string()));
        }
    }
}
