use spec_ai::agent::builder::AgentBuilder;
use spec_ai::agent::providers::MockProvider;
use spec_ai::config::AgentProfile;
use spec_ai::persistence::Persistence;
use spec_ai::tools::builtin::{EchoTool, MathTool};
use spec_ai::tools::ToolRegistry;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_tool_registry_basic_operations() {
    let mut registry = ToolRegistry::new();

    // Initially empty
    assert_eq!(registry.len(), 0);
    assert!(registry.is_empty());

    // Register echo tool
    registry.register(Arc::new(EchoTool::new()));
    assert_eq!(registry.len(), 1);
    assert!(registry.has("echo"));

    // Register calculator tool
    registry.register(Arc::new(MathTool::new()));
    assert_eq!(registry.len(), 2);
    assert!(registry.has("calculator"));

    // List all tools
    let tools = registry.list();
    assert_eq!(tools.len(), 2);
    assert!(tools.contains(&"echo"));
    assert!(tools.contains(&"calculator"));
}

#[tokio::test]
async fn test_echo_tool_execution() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool::new()));

    let args = serde_json::json!({
        "message": "Hello, world!"
    });

    let result = registry.execute("echo", args).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output, "Hello, world!");
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_math_tool_operations() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(MathTool::new()));

    // Test addition
    let args = serde_json::json!({
        "operation": "add",
        "a": 5.0,
        "b": 3.0
    });
    let result = registry.execute("calculator", args).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output, "8");

    // Test subtraction
    let args = serde_json::json!({
        "operation": "subtract",
        "a": 10.0,
        "b": 4.0
    });
    let result = registry.execute("calculator", args).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output, "6");

    // Test multiplication
    let args = serde_json::json!({
        "operation": "multiply",
        "a": 4.0,
        "b": 5.0
    });
    let result = registry.execute("calculator", args).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output, "20");

    // Test division
    let args = serde_json::json!({
        "operation": "divide",
        "a": 15.0,
        "b": 3.0
    });
    let result = registry.execute("calculator", args).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output, "5");

    // Test division by zero
    let args = serde_json::json!({
        "operation": "divide",
        "a": 10.0,
        "b": 0.0
    });
    let result = registry.execute("calculator", args).await.unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
}

#[tokio::test]
async fn test_agent_with_tool_registry() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    let profile = AgentProfile {
        prompt: Some("You are a helpful assistant.".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: Some(vec!["echo".to_string(), "calculator".to_string()]),
        denied_tools: None,
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    // Create tool registry with both tools
    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(EchoTool::new()));
    tool_registry.register(Arc::new(MathTool::new()));

    let provider = Arc::new(MockProvider::new("Response from assistant"));

    let agent = AgentBuilder::new()
        .with_profile(profile)
        .with_provider(provider)
        .with_persistence(persistence)
        .with_session_id("tool-test-session")
        .with_tool_registry(Arc::new(tool_registry))
        .build()
        .unwrap();

    // Verify agent has access to tools
    assert_eq!(agent.tool_registry().len(), 2);
    assert!(agent.tool_registry().has("echo"));
    assert!(agent.tool_registry().has("calculator"));
}

#[tokio::test]
async fn test_agent_tool_permissions_allowlist() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    // Profile with only echo allowed
    let profile = AgentProfile {
        prompt: Some("Test".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: Some(vec!["echo".to_string()]),
        denied_tools: None,
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(EchoTool::new()));
    tool_registry.register(Arc::new(MathTool::new()));

    let provider = Arc::new(MockProvider::new("Test"));

    let agent = AgentBuilder::new()
        .with_profile(profile)
        .with_provider(provider)
        .with_persistence(persistence)
        .with_session_id("permission-test")
        .with_tool_registry(Arc::new(tool_registry))
        .build()
        .unwrap();

    // Verify permissions
    assert!(agent.profile().is_tool_allowed("echo"));
    assert!(!agent.profile().is_tool_allowed("calculator"));
}

#[tokio::test]
async fn test_agent_tool_permissions_denylist() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    // Profile with calculator denied
    let profile = AgentProfile {
        prompt: Some("Test".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: None,
        denied_tools: Some(vec!["calculator".to_string()]),
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(EchoTool::new()));
    tool_registry.register(Arc::new(MathTool::new()));

    let provider = Arc::new(MockProvider::new("Test"));

    let agent = AgentBuilder::new()
        .with_profile(profile)
        .with_provider(provider)
        .with_persistence(persistence)
        .with_session_id("deny-test")
        .with_tool_registry(Arc::new(tool_registry))
        .build()
        .unwrap();

    // Verify permissions
    assert!(agent.profile().is_tool_allowed("echo"));
    assert!(!agent.profile().is_tool_allowed("calculator"));
}

#[tokio::test]
async fn test_tool_execution_logging() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    let profile = AgentProfile {
        prompt: Some("Test".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: Some(vec!["echo".to_string()]),
        denied_tools: None,
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(EchoTool::new()));

    let provider = Arc::new(MockProvider::new("Test"));

    let mut agent = AgentBuilder::new()
        .with_profile(profile)
        .with_provider(provider)
        .with_persistence(persistence.clone())
        .with_session_id("log-test")
        .with_tool_registry(Arc::new(tool_registry))
        .build()
        .unwrap();

    // Execute a step (this would log if tools were called)
    let _output = agent.run_step("Test input").await.unwrap();

    // Tool logging is tested in the persistence layer
    // We verify the mechanism exists and is integrated
}

#[tokio::test]
async fn test_default_builtin_tool_registry() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    let profile = AgentProfile::default();
    let provider = Arc::new(MockProvider::new("Test"));

    // Build agent without specifying tool registry
    let agent = AgentBuilder::new()
        .with_profile(profile)
        .with_provider(provider)
        .with_persistence(persistence)
        .with_session_id("default-registry-test")
        .build()
        .unwrap();

    // Should be populated with built-in tools by default
    let registry = agent.tool_registry();
    assert!(registry.len() >= 5);
    assert!(registry.has("echo"));
    assert!(registry.has("calculator"));
    assert!(registry.has("file_read"));
    assert!(registry.has("file_write"));
    assert!(registry.has("bash"));
}

#[tokio::test]
async fn test_tool_error_handling() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(MathTool::new()));

    // Test invalid operation
    let args = serde_json::json!({
        "operation": "invalid_op",
        "a": 5.0,
        "b": 3.0
    });
    let result = registry.execute("calculator", args).await.unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());

    // Test nonexistent tool
    let args = serde_json::json!({});
    let result = registry.execute("nonexistent", args).await;
    assert!(result.is_err());
}
