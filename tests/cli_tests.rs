use spec_ai::cli::CliState;
use spec_ai::config::{
    AgentProfile, AppConfig, AudioConfig, DatabaseConfig, LoggingConfig, ModelConfig, UiConfig,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Import the formatting module to access set_plain_text_mode
use spec_ai::cli::formatting;

fn build_repo_fixture(path: &Path) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("docs")).unwrap();
    fs::create_dir_all(path.join("specs")).unwrap();
    fs::write(
        path.join("Cargo.toml"),
        r#"
[package]
name = "cli-bootstrap"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#,
    )
    .unwrap();
    fs::write(path.join("README.md"), "# CLI Fixture\n").unwrap();
    fs::write(path.join("src/lib.rs"), "pub fn sample() {}\n").unwrap();
    fs::write(path.join("docs/guide.md"), "Docs\n").unwrap();
    fs::write(path.join("specs/basic.spec"), "name = \"sample\"\n").unwrap();
}

struct EnvVarGuard {
    key: &'static str,
}

impl EnvVarGuard {
    fn set_path(key: &'static str, value: &Path) -> Self {
        std::env::set_var(key, value);
        Self { key }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        std::env::remove_var(self.key);
    }
}

/// Integration test for full CLI workflow across multiple commands
#[tokio::test]
async fn test_full_cli_workflow() {
    // Force plain text mode for consistent test output
    formatting::set_plain_text_mode(true);

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("workflow.duckdb");

    // Create config with multiple agents
    let mut agents = HashMap::new();

    let mut coder_profile = AgentProfile::default();
    coder_profile.temperature = Some(0.3);
    coder_profile.prompt = Some("You are a coding assistant.".to_string());
    agents.insert("coder".to_string(), coder_profile);

    let mut researcher_profile = AgentProfile::default();
    researcher_profile.temperature = Some(0.9);
    researcher_profile.prompt = Some("You are a research assistant.".to_string());
    agents.insert("researcher".to_string(), researcher_profile);

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: Some("test-model".into()),
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("coder".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    // 1. Check initial help
    let help = cli.handle_line("/help").await.unwrap().unwrap();
    assert!(help.contains("SpecAI Commands"));
    assert!(help.contains("/config show"));
    assert!(help.contains("/agents"));

    // 2. List agents - should show coder and researcher with coder active
    let agents_list = cli.handle_line("/agents").await.unwrap().unwrap();
    assert!(agents_list.contains("coder"));
    assert!(agents_list.contains("researcher"));
    assert!(agents_list.contains("(active)"));

    // 3. Send a message in default session
    let response1 = cli.handle_line("Hello, world!").await.unwrap().unwrap();
    assert!(!response1.is_empty());

    // 4. Check memory shows the conversation
    let memory = cli.handle_line("/memory show 5").await.unwrap().unwrap();
    assert!(memory.contains("user:"));
    assert!(memory.contains("assistant:"));
    assert!(memory.contains("Hello, world!"));

    // 5. Create a new session
    let new_session = cli
        .handle_line("/session new research-project")
        .await
        .unwrap()
        .unwrap();
    assert!(new_session.contains("research-project"));

    // 6. In new session, memory should be empty initially
    let empty_memory = cli.handle_line("/memory show").await.unwrap().unwrap();
    assert!(empty_memory.contains("No messages in this session."));

    // 7. Switch agent while in new session
    let switch = cli
        .handle_line("/switch researcher")
        .await
        .unwrap()
        .unwrap();
    assert!(switch.contains("researcher"));

    // 8. Verify active agent changed
    let agents_after_switch = cli.handle_line("/agents").await.unwrap().unwrap();
    assert!(
        agents_after_switch.contains("researcher (active)")
            || agents_after_switch.contains("researcher")
                && agents_after_switch.contains("(active)")
    );

    // 9. Send message with new agent
    let response2 = cli
        .handle_line("Research this topic")
        .await
        .unwrap()
        .unwrap();
    assert!(!response2.is_empty());

    // 10. List sessions - should show both
    let sessions = cli.handle_line("/session list").await.unwrap().unwrap();
    assert!(sessions.contains("research-project"));

    // 11. Show current config
    let config_display = cli.handle_line("/config show").await.unwrap().unwrap();
    assert!(config_display.contains("Configuration loaded:"));
    assert!(config_display.contains("Model Provider: mock"));
    assert!(config_display.contains("Temperature: 0.7"));
    assert!(config_display.contains("Agents: 2"));
}

/// Test executing a spec file
#[tokio::test]
async fn test_spec_run_command() {
    formatting::set_plain_text_mode(true);

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("spec_run.duckdb");
    let spec_path = dir.path().join("feature.spec");

    let spec_contents = r#"
name = "Add CLI spec command"
goal = "Validate that the CLI loads .spec files"

tasks = [
    "Load the file",
    "Send it to the agent"
]

deliverables = [
    "Mock response referencing the spec"
]
    "#;
    fs::write(&spec_path, spec_contents).unwrap();

    let mut default_profile = AgentProfile::default();
    default_profile.fast_reasoning = false;
    default_profile.enable_graph = false;
    default_profile.graph_memory = false;
    default_profile.graph_steering = false;
    default_profile.auto_graph = false;

    let mut agents = HashMap::new();
    agents.insert("default".to_string(), default_profile);

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("default".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    let output = cli
        .handle_line(&format!("/spec run {}", spec_path.display()))
        .await
        .unwrap()
        .unwrap();
    assert!(output.contains("Executing spec"));
    assert!(output.contains("Add CLI spec command"));

    let shorthand_output = cli
        .handle_line(&format!("/spec {}", spec_path.display()))
        .await
        .unwrap()
        .unwrap();
    assert!(shorthand_output.contains("Executing spec"));
}

/// Test session isolation - messages in one session don't appear in another
#[tokio::test]
async fn test_session_isolation() {
    // Force plain text mode for consistent test output
    formatting::set_plain_text_mode(true);

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("isolation.duckdb");

    let mut agents = HashMap::new();
    agents.insert("test".to_string(), AgentProfile::default());

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("test".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    // Session 1: Send unique message
    let _ = cli.handle_line("Message in session 1").await.unwrap();
    let memory1 = cli.handle_line("/memory show").await.unwrap().unwrap();
    assert!(memory1.contains("Message in session 1"));

    // Switch to new session
    let _ = cli.handle_line("/session new session2").await.unwrap();

    // Session 2 should not have session 1's messages
    let memory2 = cli.handle_line("/memory show").await.unwrap().unwrap();
    assert!(!memory2.contains("Message in session 1"));
    assert!(memory2.contains("No messages in this session."));

    // Send message in session 2
    let _ = cli.handle_line("Message in session 2").await.unwrap();
    let memory2b = cli.handle_line("/memory show").await.unwrap().unwrap();
    assert!(memory2b.contains("Message in session 2"));
    assert!(!memory2b.contains("Message in session 1"));
}

#[tokio::test]
async fn test_init_command_gating() {
    formatting::set_plain_text_mode(true);

    let repo_temp = TempDir::new().unwrap();
    build_repo_fixture(repo_temp.path());
    let _env_guard = EnvVarGuard::set_path("SPEC_AI_BOOTSTRAP_ROOT", repo_temp.path());

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("init_gate.duckdb");

    let mut agents = HashMap::new();
    let mut profile = AgentProfile::default();
    profile.fast_reasoning = false;
    profile.fast_model_provider = None;
    profile.fast_model_name = None;
    agents.insert("default".to_string(), profile);

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("default".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    let first = cli.handle_line("/init").await.unwrap().unwrap();
    assert!(first.contains("Knowledge graph bootstrap complete"));

    let denied = cli.handle_line("/init").await.unwrap().unwrap();
    assert!(denied.contains("must be the first action"));

    let _ = cli.handle_line("/session new skip-init").await.unwrap();
    let _ = cli.handle_line("Hello agent").await.unwrap();
    let denied_after_message = cli.handle_line("/init").await.unwrap().unwrap();
    assert!(denied_after_message.contains("must be the first action"));

    let _ = cli.handle_line("/session new fresh").await.unwrap();
    let second = cli.handle_line("/init").await.unwrap().unwrap();
    assert!(second.contains("Knowledge graph bootstrap complete"));
}

/// Test agent switching preserves session but changes agent context
#[tokio::test]
async fn test_agent_switching_preserves_session() {
    // Force plain text mode for consistent test output
    formatting::set_plain_text_mode(true);

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("agent_switch.duckdb");

    let mut agents = HashMap::new();
    agents.insert("agent1".to_string(), AgentProfile::default());
    agents.insert("agent2".to_string(), AgentProfile::default());

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("agent1".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    // Send message with agent1
    let _ = cli.handle_line("First message").await.unwrap();

    // Switch to agent2 (should preserve session history)
    let _ = cli.handle_line("/switch agent2").await.unwrap();

    // Send message with agent2
    let _ = cli.handle_line("Second message").await.unwrap();

    // Memory should show both messages in the same session
    let memory = cli.handle_line("/memory show 10").await.unwrap().unwrap();
    assert!(memory.contains("First message"));
    assert!(memory.contains("Second message"));
}

/// Test config reload functionality
#[tokio::test]
async fn test_config_reload() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("reload.duckdb");

    let mut agents = HashMap::new();
    agents.insert("test".to_string(), AgentProfile::default());

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("test".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    // Send a message
    let _ = cli.handle_line("Before reload").await.unwrap();

    // Note: This tests that reload doesn't crash, not that it actually reloads from file
    // In real usage, this would reload from spec-ai.config.toml
    let reload_result = cli.handle_line("/config reload").await;

    // Reload should succeed (loads default config since we don't have spec-ai.config.toml in test)
    // or fail gracefully
    match reload_result {
        Ok(Some(msg)) => {
            // Either successfully reloaded or got an error message
            assert!(!msg.is_empty());
        }
        Err(_) => {
            // Config reload failed (expected in test environment without spec-ai.config.toml)
            // This is acceptable behavior
        }
        _ => {}
    }
}

/// Test empty command handling
#[tokio::test]
async fn test_empty_commands() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("empty.duckdb");

    let mut agents = HashMap::new();
    agents.insert("test".to_string(), AgentProfile::default());

    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents,
        default_agent: Some("test".into()),
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    // Empty input should return None
    let result = cli.handle_line("").await.unwrap();
    assert!(result.is_none());

    // Whitespace-only input should return None
    let result2 = cli.handle_line("   ").await.unwrap();
    assert!(result2.is_none());
}

/// Test listing agents when no agents configured
#[tokio::test]
async fn test_list_agents_empty() {
    // Force plain text mode for consistent test output
    formatting::set_plain_text_mode(true);

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("no_agents.duckdb");

    // Config with no agents (CLI will create a default one)
    let config = AppConfig {
        database: DatabaseConfig { path: db_path },
        model: ModelConfig {
            provider: "mock".into(),
            model_name: None,
            embeddings_model: None,
            api_key_source: None,
            temperature: 0.7,
        },
        ui: UiConfig {
            prompt: "> ".into(),
            theme: "default".into(),
        },
        logging: LoggingConfig {
            level: "info".into(),
        },
        audio: AudioConfig::default(),
        agents: HashMap::new(),
        default_agent: None,
    };

    let mut cli = CliState::new_with_config(config).unwrap();

    // Should show at least the default agent that was auto-created
    let agents = cli.handle_line("/agents").await.unwrap().unwrap();
    assert!(agents.contains("Available agents:") || agents.contains("default"));
}
