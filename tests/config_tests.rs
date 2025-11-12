use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use spec_ai::config::{AppConfig, AgentProfile};
use spec_ai::test_utils::env_lock;

#[test]
fn test_load_valid_basic_config() {
    let fixture_path = PathBuf::from("tests/fixtures/config/valid_basic.toml");
    let config = AppConfig::load_from_file(&fixture_path).unwrap();

    assert_eq!(config.model.provider, "openai");
    assert_eq!(config.model.temperature, 0.8);
    assert_eq!(config.logging.level, "debug");
    assert!(config.validate().is_ok());
}

#[test]
fn test_load_config_with_agents() {
    let fixture_path = PathBuf::from("tests/fixtures/config/valid_with_agents.toml");
    let config = AppConfig::load_from_file(&fixture_path).unwrap();

    assert_eq!(config.model.provider, "anthropic");
    assert_eq!(config.model.model_name, Some("claude-3-opus".to_string()));
    assert_eq!(config.agents.len(), 2);
    assert!(config.agents.contains_key("coder"));
    assert!(config.agents.contains_key("researcher"));
    assert_eq!(config.default_agent, Some("coder".to_string()));

    // Verify coder agent
    let coder = config.agents.get("coder").unwrap();
    assert_eq!(coder.temperature, Some(0.3));
    assert_eq!(
        coder.allowed_tools,
        Some(vec!["file_read".to_string(), "file_write".to_string(), "bash".to_string()])
    );
    assert!(coder.is_tool_allowed("file_read"));
    assert!(!coder.is_tool_allowed("unknown_tool"));

    // Verify researcher agent
    let researcher = config.agents.get("researcher").unwrap();
    assert_eq!(researcher.temperature, Some(0.9));
    assert_eq!(researcher.memory_k, 20);
    assert!(!researcher.is_tool_allowed("bash"));
    assert!(!researcher.is_tool_allowed("file_write"));

    assert!(config.validate().is_ok());
}

#[test]
fn test_load_invalid_provider() {
    let fixture_path = PathBuf::from("tests/fixtures/config/invalid_provider.toml");
    let config = AppConfig::load_from_file(&fixture_path).unwrap();
    assert!(config.validate().is_err());
}

#[test]
fn test_load_invalid_temperature() {
    let fixture_path = PathBuf::from("tests/fixtures/config/invalid_temperature.toml");
    let config = AppConfig::load_from_file(&fixture_path).unwrap();
    assert!(config.validate().is_err());
}

#[test]
fn test_env_override_precedence() {
    let _guard = env_lock().lock().unwrap();
    // Ensure clean environment first
    unsafe {
        env::remove_var("AGENT_MODEL_PROVIDER");
        env::remove_var("AGENT_MODEL_TEMPERATURE");
        env::remove_var("AGENT_LOG_LEVEL");
    }

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[model]
provider = "openai"
temperature = 0.8

[logging]
level = "info"
"#;

    fs::write(&config_path, config_content).unwrap();

    // Set environment overrides
    unsafe {
        env::set_var("AGENT_MODEL_PROVIDER", "anthropic");
        env::set_var("AGENT_MODEL_TEMPERATURE", "0.5");
        env::set_var("AGENT_LOG_LEVEL", "debug");
    }

    let mut config = AppConfig::load_from_file(&config_path).unwrap();
    config.apply_env_overrides();

    assert_eq!(config.model.provider, "anthropic");
    assert_eq!(config.model.temperature, 0.5);
    assert_eq!(config.logging.level, "debug");

    // Cleanup
    unsafe {
        env::remove_var("AGENT_MODEL_PROVIDER");
        env::remove_var("AGENT_MODEL_TEMPERATURE");
        env::remove_var("AGENT_LOG_LEVEL");
    }
}

#[test]
fn test_defaults_fill_missing_fields() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Minimal config with only one field
    let config_content = r#"
[model]
provider = "openai"
"#;

    fs::write(&config_path, config_content).unwrap();

    let config = AppConfig::load_from_file(&config_path).unwrap();

    // Verify defaults are applied
    assert_eq!(config.model.provider, "openai");
    assert_eq!(config.model.temperature, 0.7); // default
    assert_eq!(config.logging.level, "info"); // default
    assert_eq!(config.ui.theme, "default"); // default
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_default_agent_not_found() {
    let mut config = AppConfig::default();
    config.model.provider = "openai".to_string();
    config.default_agent = Some("nonexistent".to_string());

    // Should fail validation because 'nonexistent' agent doesn't exist in agents map
    let result = config.validate();
    assert!(result.is_err(), "Expected validation error but got: {:?}", result);
}

#[test]
fn test_agent_profile_tool_restrictions() {
    let mut profile = AgentProfile::default();

    // Test with no restrictions - all tools allowed
    assert!(profile.is_tool_allowed("any_tool"));

    // Test with allowlist
    profile.allowed_tools = Some(vec!["tool1".to_string(), "tool2".to_string()]);
    assert!(profile.is_tool_allowed("tool1"));
    assert!(profile.is_tool_allowed("tool2"));
    assert!(!profile.is_tool_allowed("tool3"));

    // Test with denylist
    profile.allowed_tools = None;
    profile.denied_tools = Some(vec!["dangerous_tool".to_string()]);
    assert!(!profile.is_tool_allowed("dangerous_tool"));
    assert!(profile.is_tool_allowed("safe_tool"));
}

#[test]
fn test_config_summary() {
    let config = AppConfig::default();
    let summary = config.summary();

    assert!(summary.contains("Configuration loaded:"));
    assert!(summary.contains("Database:"));
    assert!(summary.contains("Model Provider: mock"));
    assert!(summary.contains("Temperature: 0.7"));
    assert!(summary.contains("Logging Level: info"));
}

#[test]
fn test_multiple_env_overrides() {
    let _guard = env_lock().lock().unwrap();
    // Clean environment first
    unsafe {
        env::remove_var("AGENT_MODEL_PROVIDER");
        env::remove_var("AGENT_MODEL_NAME");
        env::remove_var("AGENT_API_KEY_SOURCE");
        env::remove_var("AGENT_DB_PATH");
        env::remove_var("AGENT_UI_THEME");
        env::remove_var("AGENT_DEFAULT_AGENT");
    }

    unsafe {
        env::set_var("AGENT_MODEL_PROVIDER", "ollama");
        env::set_var("AGENT_MODEL_NAME", "llama3");
        env::set_var("AGENT_API_KEY_SOURCE", "env:OLLAMA_KEY");
        env::set_var("AGENT_DB_PATH", "/tmp/test.duckdb");
        env::set_var("AGENT_UI_THEME", "dark");
        env::set_var("AGENT_DEFAULT_AGENT", "test_agent");
    }

    let mut config = AppConfig::default();
    config.apply_env_overrides();

    assert_eq!(config.model.provider, "ollama");
    assert_eq!(config.model.model_name, Some("llama3".to_string()));
    assert_eq!(config.model.api_key_source, Some("env:OLLAMA_KEY".to_string()));
    assert_eq!(config.database.path, PathBuf::from("/tmp/test.duckdb"));
    assert_eq!(config.ui.theme, "dark");
    assert_eq!(config.default_agent, Some("test_agent".to_string()));

    // Cleanup
    unsafe {
        env::remove_var("AGENT_MODEL_PROVIDER");
        env::remove_var("AGENT_MODEL_NAME");
        env::remove_var("AGENT_API_KEY_SOURCE");
        env::remove_var("AGENT_DB_PATH");
        env::remove_var("AGENT_UI_THEME");
        env::remove_var("AGENT_DEFAULT_AGENT");
    }
}

#[test]
fn test_agent_profile_effective_values() {
    let mut profile = AgentProfile::default();

    // Test defaults
    assert_eq!(profile.effective_temperature(0.7), 0.7);
    assert_eq!(profile.effective_provider("mock"), "mock");
    assert_eq!(profile.effective_model_name(Some("default-model")), Some("default-model"));

    // Test overrides
    profile.temperature = Some(0.3);
    profile.model_provider = Some("openai".to_string());
    profile.model_name = Some("gpt-4".to_string());

    assert_eq!(profile.effective_temperature(0.7), 0.3);
    assert_eq!(profile.effective_provider("mock"), "openai");
    assert_eq!(profile.effective_model_name(Some("default-model")), Some("gpt-4"));
}
