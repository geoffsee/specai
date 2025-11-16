use spec_ai::agent::providers::MockProvider;
use spec_ai::agent::AgentCore;
use spec_ai::config::AgentProfile;
use spec_ai::persistence::Persistence;
use spec_ai::policy::{PolicyDecision, PolicyEffect, PolicyEngine, PolicyRule};
use spec_ai::tools::{
    builtin::{EchoTool, MathTool},
    ToolRegistry,
};
use std::sync::Arc;
use tempfile::tempdir;

/// Create a test agent with the given policy engine
fn create_test_agent_with_policy(
    policy_engine: Arc<PolicyEngine>,
    profile: AgentProfile,
) -> (AgentCore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    let provider = Arc::new(MockProvider::new(
        "TOOL_CALL: echo\nARGS: {\"message\": \"test\"}",
    ));

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(EchoTool::new()));
    tool_registry.register(Arc::new(MathTool::new()));

    let agent = AgentCore::new(
        profile,
        provider,
        None,
        persistence,
        "test-session".to_string(),
        Some("policy-test".to_string()),
        Arc::new(tool_registry),
        policy_engine,
    );

    (agent, dir)
}

#[test]
fn test_policy_denies_tool_by_default() {
    // Empty policy engine denies everything by default
    let policy_engine = Arc::new(PolicyEngine::new());

    let profile = AgentProfile {
        prompt: Some("Test".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: Some(vec!["echo".to_string()]), // Profile allows echo
        denied_tools: None,
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    let (agent, _dir) = create_test_agent_with_policy(policy_engine, profile);

    // Profile allows echo, but policy denies it (no rule = default deny)
    assert!(!agent.policy_engine().policy_set().rules.is_empty() == false); // Empty policy

    // Check that policy decision is deny
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    match decision {
        PolicyDecision::Deny(_) => {} // Expected
        PolicyDecision::Allow => panic!("Expected deny, got allow"),
    }
}

#[test]
fn test_policy_allows_specific_tool() {
    let mut policy_engine = PolicyEngine::new();

    // Add rule to allow echo
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "echo".to_string(),
        effect: PolicyEffect::Allow,
    });

    let profile = AgentProfile {
        prompt: Some("Test".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: Some(vec!["echo".to_string(), "math".to_string()]),
        denied_tools: None,
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    let (agent, _dir) = create_test_agent_with_policy(Arc::new(policy_engine), profile);

    // Policy allows echo
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    assert_eq!(decision, PolicyDecision::Allow);

    // Policy doesn't have rule for math, so it's denied by default
    let decision = agent.policy_engine().check("agent", "tool_call", "math");
    match decision {
        PolicyDecision::Deny(_) => {}
        PolicyDecision::Allow => panic!("Expected deny for math"),
    }
}

#[test]
fn test_policy_denies_specific_tool() {
    let mut policy_engine = PolicyEngine::new();

    // Add rule to deny math explicitly
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "math".to_string(),
        effect: PolicyEffect::Deny,
    });

    let profile = AgentProfile::default();

    let (agent, _dir) = create_test_agent_with_policy(Arc::new(policy_engine), profile);

    // Policy explicitly denies math
    let decision = agent.policy_engine().check("agent", "tool_call", "math");
    match decision {
        PolicyDecision::Deny(reason) => {
            assert!(reason.contains("math"));
        }
        PolicyDecision::Allow => panic!("Expected deny for math"),
    }
}

#[test]
fn test_policy_wildcard_allows_all_tools() {
    let mut policy_engine = PolicyEngine::new();

    // Add permissive wildcard rule
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "*".to_string(),
        effect: PolicyEffect::Allow,
    });

    let profile = AgentProfile::default();

    let (agent, _dir) = create_test_agent_with_policy(Arc::new(policy_engine), profile);

    // Policy allows everything with wildcard
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    assert_eq!(decision, PolicyDecision::Allow);

    let decision = agent.policy_engine().check("agent", "tool_call", "math");
    assert_eq!(decision, PolicyDecision::Allow);

    let decision = agent
        .policy_engine()
        .check("agent", "tool_call", "any_tool");
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn test_policy_first_match_wins() {
    let mut policy_engine = PolicyEngine::new();

    // First rule: deny all
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "*".to_string(),
        effect: PolicyEffect::Deny,
    });

    // Second rule: allow echo (this should never be reached)
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "echo".to_string(),
        effect: PolicyEffect::Allow,
    });

    let profile = AgentProfile::default();

    let (agent, _dir) = create_test_agent_with_policy(Arc::new(policy_engine), profile);

    // First rule should win - deny all
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    match decision {
        PolicyDecision::Deny(_) => {}
        PolicyDecision::Allow => panic!("Expected deny from first rule"),
    }
}

#[test]
fn test_policy_persistence_round_trip() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    // Create policy engine with rules
    let mut engine = PolicyEngine::new();
    engine.add_rule(PolicyRule {
        agent: "coder".to_string(),
        action: "tool_call".to_string(),
        resource: "echo".to_string(),
        effect: PolicyEffect::Allow,
    });
    engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "math".to_string(),
        effect: PolicyEffect::Deny,
    });

    // Save to persistence
    engine.save_to_persistence(&persistence).unwrap();

    // Load from persistence
    let loaded_engine = PolicyEngine::load_from_persistence(&persistence).unwrap();

    assert_eq!(loaded_engine.rule_count(), 2);

    // Verify rules work correctly
    let decision = loaded_engine.check("coder", "tool_call", "echo");
    assert_eq!(decision, PolicyDecision::Allow);

    let decision = loaded_engine.check("anyone", "tool_call", "math");
    match decision {
        PolicyDecision::Deny(_) => {}
        PolicyDecision::Allow => panic!("Expected deny for math"),
    }
}

#[test]
fn test_policy_reload_updates_agent() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();

    // Create initial policy engine (permissive)
    let mut engine = PolicyEngine::new();
    engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "*".to_string(),
        effect: PolicyEffect::Allow,
    });
    engine.save_to_persistence(&persistence).unwrap();

    // Create agent with initial policy
    let profile = AgentProfile::default();
    let provider = Arc::new(MockProvider::new("Test"));
    let tool_registry = Arc::new(ToolRegistry::new());
    let policy_engine = PolicyEngine::load_from_persistence(&persistence).unwrap();

    let mut agent = AgentCore::new(
        profile,
        provider,
        None,
        persistence.clone(),
        "test-session".to_string(),
        Some("policy-reload".to_string()),
        tool_registry,
        Arc::new(policy_engine),
    );

    // Verify initial policy allows echo
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    assert_eq!(decision, PolicyDecision::Allow);

    // Update policy in persistence (now deny all)
    let mut new_engine = PolicyEngine::new();
    new_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "*".to_string(),
        effect: PolicyEffect::Deny,
    });
    new_engine.save_to_persistence(&persistence).unwrap();

    // Reload policy in agent
    let reloaded_engine = PolicyEngine::load_from_persistence(&persistence).unwrap();
    agent.set_policy_engine(Arc::new(reloaded_engine));

    // Verify updated policy denies echo
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    match decision {
        PolicyDecision::Deny(_) => {}
        PolicyDecision::Allow => panic!("Expected deny after reload"),
    }
}

#[test]
fn test_policy_combined_with_profile_permissions() {
    let mut policy_engine = PolicyEngine::new();

    // Policy allows echo and math
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "echo".to_string(),
        effect: PolicyEffect::Allow,
    });
    policy_engine.add_rule(PolicyRule {
        agent: "*".to_string(),
        action: "tool_call".to_string(),
        resource: "math".to_string(),
        effect: PolicyEffect::Allow,
    });

    // Profile only allows echo (denies math)
    let profile = AgentProfile {
        prompt: Some("Test".to_string()),
        style: None,
        temperature: Some(0.7),
        model_provider: None,
        model_name: None,
        allowed_tools: Some(vec!["echo".to_string()]), // Only echo allowed
        denied_tools: None,
        memory_k: 5,
        top_p: 0.9,
        max_context_tokens: Some(2048),
        ..AgentProfile::default()
    };

    let (agent, _dir) = create_test_agent_with_policy(Arc::new(policy_engine), profile);

    // Agent checks both profile and policy
    // Profile allows echo, policy allows echo -> allowed
    let decision = agent.policy_engine().check("agent", "tool_call", "echo");
    assert_eq!(decision, PolicyDecision::Allow);

    // Profile denies math (not in allowlist), policy allows math -> denied by profile
    let decision = agent.policy_engine().check("agent", "tool_call", "math");
    assert_eq!(decision, PolicyDecision::Allow); // Policy allows it

    // But is_tool_allowed checks both profile and policy
    // Profile denies math, so overall result is deny
    assert!(agent.profile().is_tool_allowed("echo"));
    assert!(!agent.profile().is_tool_allowed("math")); // Profile denies
}
