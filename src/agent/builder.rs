//! Agent Builder
//!
//! Provides a fluent API for constructing agent instances.

use crate::agent::core::AgentCore;
use crate::agent::factory::{create_provider, resolve_api_key};
use crate::agent::model::ModelProvider;
use crate::config::{AgentProfile, AgentRegistry, AppConfig};
use crate::embeddings::EmbeddingsClient;
use crate::persistence::Persistence;
use crate::policy::PolicyEngine;
use crate::tools::ToolRegistry;
use anyhow::{Context, Result, anyhow};
use std::sync::Arc;

/// Builder for constructing AgentCore instances
pub struct AgentBuilder {
    profile: Option<AgentProfile>,
    provider: Option<Arc<dyn ModelProvider>>,
    embeddings_client: Option<EmbeddingsClient>,
    persistence: Option<Persistence>,
    session_id: Option<String>,
    config: Option<AppConfig>,
    tool_registry: Option<Arc<ToolRegistry>>,
    policy_engine: Option<Arc<PolicyEngine>>,
    agent_name: Option<String>,
}

impl AgentBuilder {
    /// Create a new agent builder
    pub fn new() -> Self {
        Self {
            profile: None,
            provider: None,
            embeddings_client: None,
            persistence: None,
            session_id: None,
            config: None,
            tool_registry: None,
            policy_engine: None,
            agent_name: None,
        }
    }

    /// Create an agent from the registry with the active profile
    /// This is a convenience method for CLI use
    pub fn new_with_registry(
        registry: &AgentRegistry,
        config: &AppConfig,
        session_id: Option<String>,
    ) -> Result<AgentCore> {
        create_agent_from_registry(registry, config, session_id)
    }

    /// Set the agent profile
    pub fn with_profile(mut self, profile: AgentProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Set the model provider
    pub fn with_provider(mut self, provider: Arc<dyn ModelProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set a custom embeddings client
    pub fn with_embeddings_client(mut self, embeddings_client: EmbeddingsClient) -> Self {
        self.embeddings_client = Some(embeddings_client);
        self
    }

    /// Set the persistence layer
    pub fn with_persistence(mut self, persistence: Persistence) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Set the session ID
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the application configuration (used to derive defaults)
    pub fn with_config(mut self, config: AppConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the tool registry
    pub fn with_tool_registry(mut self, tool_registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = Some(tool_registry);
        self
    }

    /// Set the policy engine
    pub fn with_policy_engine(mut self, policy_engine: Arc<PolicyEngine>) -> Self {
        self.policy_engine = Some(policy_engine);
        self
    }

    /// Set the logical agent name (used for telemetry/logging)
    pub fn with_agent_name(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = Some(agent_name.into());
        self
    }

    /// Build the agent, validating all required fields
    pub fn build(self) -> Result<AgentCore> {
        // Get profile (required)
        let profile = self
            .profile
            .ok_or_else(|| anyhow!("Agent profile is required"))?;

        // Get or create provider
        let provider = if let Some(provider) = self.provider {
            provider
        } else if let Some(ref config) = self.config {
            create_provider(&config.model).context("Failed to create provider from config")?
        } else {
            return Err(anyhow!(
                "Either provider or config must be provided to build agent"
            ));
        };

        // Get or create persistence
        let persistence = if let Some(persistence) = self.persistence {
            persistence
        } else if let Some(ref config) = self.config {
            Persistence::new(&config.database.path).context("Failed to create persistence layer")?
        } else {
            return Err(anyhow!(
                "Either persistence or config must be provided to build agent"
            ));
        };

        // Get or create embeddings client
        let embeddings_client = if let Some(client) = self.embeddings_client {
            Some(client)
        } else if let Some(ref config) = self.config {
            create_embeddings_client_from_config(config)?
        } else {
            None
        };

        // Get or generate session ID
        let session_id = self
            .session_id
            .unwrap_or_else(|| format!("session-{}", chrono::Utc::now().timestamp_millis()));

        // Get or create tool registry (defaults to empty registry)
        let tool_registry = self
            .tool_registry
            .unwrap_or_else(|| Arc::new(ToolRegistry::new()));

        // Get or create policy engine (defaults to empty policy engine, or load from persistence)
        let policy_engine = if let Some(engine) = self.policy_engine {
            engine
        } else {
            // Try to load from persistence, or create empty engine
            let engine = PolicyEngine::load_from_persistence(&persistence)
                .unwrap_or_else(|_| PolicyEngine::new());
            Arc::new(engine)
        };

        Ok(AgentCore::new(
            profile,
            provider,
            embeddings_client,
            persistence,
            session_id,
            self.agent_name,
            tool_registry,
            policy_engine,
        ))
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create an agent from the active profile in the registry
pub fn create_agent_from_registry(
    registry: &AgentRegistry,
    config: &AppConfig,
    session_id: Option<String>,
) -> Result<AgentCore> {
    let (agent_name, profile) = registry
        .active()
        .context("No active agent profile in registry")?
        .ok_or_else(|| anyhow!("No active agent set in registry"))?;

    let mut builder = AgentBuilder::new()
        .with_profile(profile)
        .with_config(config.clone())
        .with_agent_name(agent_name.clone());

    if let Some(sid) = session_id {
        builder = builder.with_session_id(sid);
    }

    builder.build()
}

fn create_embeddings_client_from_config(config: &AppConfig) -> Result<Option<EmbeddingsClient>> {
    let model = &config.model;
    let Some(model_name) = &model.embeddings_model else {
        return Ok(None);
    };

    let client = if let Some(source) = &model.api_key_source {
        let api_key = resolve_api_key(source)?;
        EmbeddingsClient::with_api_key(model_name.clone(), api_key)
    } else {
        EmbeddingsClient::new(model_name.clone())
    };

    Ok(Some(client))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::providers::MockProvider;
    use crate::config::{AgentProfile, DatabaseConfig, LoggingConfig, ModelConfig, UiConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn create_test_config() -> AppConfig {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");

        AppConfig {
            database: DatabaseConfig { path: db_path },
            model: ModelConfig {
                provider: "mock".to_string(),
                model_name: Some("test-model".to_string()),
                embeddings_model: None,
                api_key_source: None,
                temperature: 0.7,
            },
            ui: UiConfig {
                prompt: "> ".to_string(),
                theme: "default".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            agents: HashMap::new(),
            default_agent: None,
        }
    }

    fn create_test_profile() -> AgentProfile {
        AgentProfile {
            prompt: Some("Test system prompt".to_string()),
            style: None,
            temperature: Some(0.8),
            model_provider: None,
            model_name: None,
            allowed_tools: None,
            denied_tools: None,
            memory_k: 10,
            top_p: 0.95,
            max_context_tokens: Some(4096),
        }
    }

    #[test]
    fn test_builder_with_all_fields() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let profile = create_test_profile();
        let provider = Arc::new(MockProvider::default());

        let agent = AgentBuilder::new()
            .with_profile(profile)
            .with_provider(provider)
            .with_persistence(persistence)
            .with_session_id("test-session")
            .build()
            .unwrap();

        assert_eq!(agent.session_id(), "test-session");
        assert_eq!(
            agent.profile().prompt,
            Some("Test system prompt".to_string())
        );
    }

    #[test]
    fn test_builder_with_config() {
        let config = create_test_config();
        let profile = create_test_profile();

        let agent = AgentBuilder::new()
            .with_profile(profile)
            .with_config(config)
            .build()
            .unwrap();

        // Should auto-generate session ID with timestamp
        assert!(agent.session_id().starts_with("session-"));
    }

    #[test]
    fn test_builder_missing_profile() {
        let config = create_test_config();

        let result = AgentBuilder::new().with_config(config).build();

        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("profile"));
    }

    #[test]
    fn test_builder_missing_provider_and_config() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let profile = create_test_profile();

        let result = AgentBuilder::new()
            .with_profile(profile)
            .with_persistence(persistence)
            .build();

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("provider or config")
        );
    }

    #[test]
    fn test_builder_auto_session_id() {
        let config = create_test_config();
        let profile = create_test_profile();

        let agent = AgentBuilder::new()
            .with_profile(profile)
            .with_config(config)
            .build()
            .unwrap();

        // Should auto-generate session ID with timestamp
        assert!(!agent.session_id().is_empty());
    }

    #[test]
    fn test_create_agent_from_registry() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let config = create_test_config();
        let profile = create_test_profile();

        let mut agents = HashMap::new();
        agents.insert("test-agent".to_string(), profile.clone());

        let registry = AgentRegistry::new(agents, persistence.clone());
        registry.set_active("test-agent").unwrap();

        let agent =
            create_agent_from_registry(&registry, &config, Some("custom-session".to_string()))
                .unwrap();

        assert_eq!(agent.session_id(), "custom-session");
        assert_eq!(
            agent.profile().prompt,
            Some("Test system prompt".to_string())
        );
    }

    #[test]
    fn test_create_agent_from_registry_no_active() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let config = create_test_config();
        let registry = AgentRegistry::new(HashMap::new(), persistence);

        let result = create_agent_from_registry(&registry, &config, None);

        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("No active") || err_msg.contains("active agent"));
    }
}
