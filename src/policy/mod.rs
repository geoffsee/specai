use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::persistence::Persistence;

/// Represents the effect of a policy rule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

/// A single policy rule matching (agent, action, resource) tuples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Agent name pattern (supports wildcards: "*")
    pub agent: String,
    /// Action pattern (e.g., "tool_call", "file_write", "bash")
    pub action: String,
    /// Resource pattern (e.g., tool name, file path - supports wildcards: "*")
    pub resource: String,
    /// Effect to apply when rule matches
    pub effect: PolicyEffect,
}

impl PolicyRule {
    /// Check if this rule matches the given agent, action, and resource
    pub fn matches(&self, agent: &str, action: &str, resource: &str) -> bool {
        wildcard_match(&self.agent, agent)
            && wildcard_match(&self.action, action)
            && wildcard_match(&self.resource, resource)
    }
}

/// Container for all policy rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySet {
    pub rules: Vec<PolicyRule>,
}

impl Default for PolicySet {
    fn default() -> Self {
        Self { rules: Vec::new() }
    }
}

/// Result of policy evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Action is allowed
    Allow,
    /// Action is denied with a reason
    Deny(String),
}

/// Policy engine that evaluates actions against stored rules
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policy_set: PolicySet,
}

impl PolicyEngine {
    /// Create a new policy engine with an empty policy set
    pub fn new() -> Self {
        Self {
            policy_set: PolicySet::default(),
        }
    }

    /// Create a policy engine with the given policy set
    pub fn with_policy_set(policy_set: PolicySet) -> Self {
        Self { policy_set }
    }

    /// Load policies from persistence layer
    /// Policies are stored in the policy_cache table with key "policies"
    pub fn load_from_persistence(persistence: &Persistence) -> Result<Self> {
        match persistence.policy_get("policies")? {
            Some(entry) => {
                let policy_set: PolicySet = serde_json::from_value(entry.value)
                    .context("deserializing policy set from cache")?;
                Ok(Self::with_policy_set(policy_set))
            }
            None => {
                // No policies stored yet, return empty engine
                Ok(Self::new())
            }
        }
    }

    /// Save current policy set to persistence
    pub fn save_to_persistence(&self, persistence: &Persistence) -> Result<()> {
        let value = serde_json::to_value(&self.policy_set).context("serializing policy set")?;
        persistence.policy_upsert("policies", &value)?;
        Ok(())
    }

    /// Reload policies from persistence
    pub fn reload(&mut self, persistence: &Persistence) -> Result<()> {
        let engine = Self::load_from_persistence(persistence)?;
        self.policy_set = engine.policy_set;
        Ok(())
    }

    /// Evaluate a policy decision for the given agent, action, and resource
    /// Rules are evaluated in order, and the first matching rule determines the decision
    /// If no rules match, the default is to deny with a reason
    pub fn check(&self, agent: &str, action: &str, resource: &str) -> PolicyDecision {
        for rule in &self.policy_set.rules {
            if rule.matches(agent, action, resource) {
                return match rule.effect {
                    PolicyEffect::Allow => PolicyDecision::Allow,
                    PolicyEffect::Deny => PolicyDecision::Deny(format!(
                        "Policy denies {} action {} on resource {}",
                        agent, action, resource
                    )),
                };
            }
        }

        // Default: deny if no rule matches
        PolicyDecision::Deny(format!(
            "No policy rule matches agent '{}', action '{}', resource '{}' (default deny)",
            agent, action, resource
        ))
    }

    /// Get the number of rules in the policy set
    pub fn rule_count(&self) -> usize {
        self.policy_set.rules.len()
    }

    /// Add a rule to the policy set
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.policy_set.rules.push(rule);
    }

    /// Get a reference to the policy set
    pub fn policy_set(&self) -> &PolicySet {
        &self.policy_set
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple wildcard matching
/// Supports "*" as a wildcard that matches any string
fn wildcard_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // If pattern contains wildcards, do more complex matching
    if pattern.contains('*') {
        // Split pattern by '*' and check if text contains all parts in order
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut text_pos = 0;

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if let Some(pos) = text[text_pos..].find(part) {
                text_pos += pos + part.len();
            } else {
                return false;
            }

            // If this is not the last part and not followed by wildcard at pattern end,
            // ensure part is found
            if i == parts.len() - 1 && !pattern.ends_with('*') {
                // Last part must be at the end
                return text.ends_with(part);
            }
        }

        // If pattern starts with '*', first part can be anywhere
        // If pattern ends with '*', last part can be anywhere (already handled)
        if !pattern.starts_with('*') && !parts.is_empty() && !parts[0].is_empty() {
            return text.starts_with(parts[0]);
        }

        true
    } else {
        // No wildcards, exact match
        pattern == text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match_exact() {
        assert!(wildcard_match("hello", "hello"));
        assert!(!wildcard_match("hello", "world"));
        assert!(!wildcard_match("hello", "hello_world"));
    }

    #[test]
    fn test_wildcard_match_star() {
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("*", "foo/bar/baz"));
    }

    #[test]
    fn test_wildcard_match_prefix() {
        assert!(wildcard_match("hello*", "hello"));
        assert!(wildcard_match("hello*", "hello_world"));
        assert!(wildcard_match("hello*", "hello123"));
        assert!(!wildcard_match("hello*", "hi_world"));
    }

    #[test]
    fn test_wildcard_match_suffix() {
        assert!(wildcard_match("*world", "world"));
        assert!(wildcard_match("*world", "hello_world"));
        assert!(!wildcard_match("*world", "world_hello"));
    }

    #[test]
    fn test_wildcard_match_middle() {
        assert!(wildcard_match("hello*world", "helloworld"));
        assert!(wildcard_match("hello*world", "hello_beautiful_world"));
        assert!(!wildcard_match("hello*world", "hello"));
        assert!(!wildcard_match("hello*world", "world"));
    }

    #[test]
    fn test_wildcard_match_multiple() {
        assert!(wildcard_match("/etc/*/*.conf", "/etc/nginx/nginx.conf"));
        assert!(wildcard_match("/etc/*/*.conf", "/etc/apache2/apache2.conf"));
        assert!(!wildcard_match(
            "/etc/*/*.conf",
            "/etc/nginx/sites-available/default"
        ));
    }

    #[test]
    fn test_policy_rule_matches() {
        let rule = PolicyRule {
            agent: "coder".to_string(),
            action: "tool_call".to_string(),
            resource: "echo".to_string(),
            effect: PolicyEffect::Allow,
        };

        assert!(rule.matches("coder", "tool_call", "echo"));
        assert!(!rule.matches("assistant", "tool_call", "echo"));
        assert!(!rule.matches("coder", "file_write", "echo"));
        assert!(!rule.matches("coder", "tool_call", "calculator"));
    }

    #[test]
    fn test_policy_rule_wildcard_agent() {
        let rule = PolicyRule {
            agent: "*".to_string(),
            action: "tool_call".to_string(),
            resource: "echo".to_string(),
            effect: PolicyEffect::Allow,
        };

        assert!(rule.matches("coder", "tool_call", "echo"));
        assert!(rule.matches("assistant", "tool_call", "echo"));
        assert!(rule.matches("any_agent", "tool_call", "echo"));
    }

    #[test]
    fn test_policy_rule_wildcard_resource() {
        let rule = PolicyRule {
            agent: "coder".to_string(),
            action: "tool_call".to_string(),
            resource: "*".to_string(),
            effect: PolicyEffect::Allow,
        };

        assert!(rule.matches("coder", "tool_call", "echo"));
        assert!(rule.matches("coder", "tool_call", "calculator"));
        assert!(rule.matches("coder", "tool_call", "any_tool"));
    }

    #[test]
    fn test_policy_engine_allow() {
        let mut engine = PolicyEngine::new();
        engine.add_rule(PolicyRule {
            agent: "coder".to_string(),
            action: "tool_call".to_string(),
            resource: "echo".to_string(),
            effect: PolicyEffect::Allow,
        });

        assert_eq!(
            engine.check("coder", "tool_call", "echo"),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn test_policy_engine_deny() {
        let mut engine = PolicyEngine::new();
        engine.add_rule(PolicyRule {
            agent: "coder".to_string(),
            action: "bash".to_string(),
            resource: "/etc/*".to_string(),
            effect: PolicyEffect::Deny,
        });

        match engine.check("coder", "bash", "/etc/passwd") {
            PolicyDecision::Deny(_) => {}
            _ => panic!("Expected deny decision"),
        }
    }

    #[test]
    fn test_policy_engine_first_match_wins() {
        let mut engine = PolicyEngine::new();
        // First rule: deny all bash
        engine.add_rule(PolicyRule {
            agent: "*".to_string(),
            action: "bash".to_string(),
            resource: "*".to_string(),
            effect: PolicyEffect::Deny,
        });
        // Second rule: allow bash for coder (should never be reached)
        engine.add_rule(PolicyRule {
            agent: "coder".to_string(),
            action: "bash".to_string(),
            resource: "*".to_string(),
            effect: PolicyEffect::Allow,
        });

        // First rule should win
        match engine.check("coder", "bash", "/tmp/test.sh") {
            PolicyDecision::Deny(_) => {}
            _ => panic!("Expected deny decision from first rule"),
        }
    }

    #[test]
    fn test_policy_engine_default_deny() {
        let engine = PolicyEngine::new();

        // No rules, should deny by default
        match engine.check("agent", "action", "resource") {
            PolicyDecision::Deny(reason) => {
                assert!(reason.contains("No policy rule matches"));
            }
            _ => panic!("Expected default deny"),
        }
    }

    #[test]
    fn test_policy_engine_rule_count() {
        let mut engine = PolicyEngine::new();
        assert_eq!(engine.rule_count(), 0);

        engine.add_rule(PolicyRule {
            agent: "*".to_string(),
            action: "*".to_string(),
            resource: "*".to_string(),
            effect: PolicyEffect::Allow,
        });
        assert_eq!(engine.rule_count(), 1);
    }

    #[test]
    fn test_policy_serialization() {
        let policy_set = PolicySet {
            rules: vec![
                PolicyRule {
                    agent: "coder".to_string(),
                    action: "tool_call".to_string(),
                    resource: "echo".to_string(),
                    effect: PolicyEffect::Allow,
                },
                PolicyRule {
                    agent: "*".to_string(),
                    action: "bash".to_string(),
                    resource: "/etc/*".to_string(),
                    effect: PolicyEffect::Deny,
                },
            ],
        };

        // Serialize and deserialize
        let json = serde_json::to_value(&policy_set).unwrap();
        let deserialized: PolicySet = serde_json::from_value(json).unwrap();

        assert_eq!(deserialized.rules.len(), 2);
        assert_eq!(deserialized.rules[0].agent, "coder");
        assert_eq!(deserialized.rules[1].effect, PolicyEffect::Deny);
    }

    #[test]
    fn test_policy_persistence() {
        use crate::test_utils::create_test_db;

        let persistence = create_test_db();

        // Create engine with some rules
        let mut engine = PolicyEngine::new();
        engine.add_rule(PolicyRule {
            agent: "coder".to_string(),
            action: "tool_call".to_string(),
            resource: "echo".to_string(),
            effect: PolicyEffect::Allow,
        });
        engine.add_rule(PolicyRule {
            agent: "*".to_string(),
            action: "bash".to_string(),
            resource: "*".to_string(),
            effect: PolicyEffect::Deny,
        });

        // Save to persistence
        engine.save_to_persistence(&persistence).unwrap();

        // Load from persistence
        let loaded = PolicyEngine::load_from_persistence(&persistence).unwrap();
        assert_eq!(loaded.rule_count(), 2);

        // Verify rules work
        assert_eq!(
            loaded.check("coder", "tool_call", "echo"),
            PolicyDecision::Allow
        );
        match loaded.check("coder", "bash", "/tmp/test.sh") {
            PolicyDecision::Deny(_) => {}
            _ => panic!("Expected deny"),
        }
    }

    #[test]
    fn test_policy_reload() {
        use crate::test_utils::create_test_db;

        let persistence = create_test_db();

        // Create and save initial engine
        let mut engine = PolicyEngine::new();
        engine.add_rule(PolicyRule {
            agent: "coder".to_string(),
            action: "tool_call".to_string(),
            resource: "echo".to_string(),
            effect: PolicyEffect::Allow,
        });
        engine.save_to_persistence(&persistence).unwrap();

        // Create new engine with different rules
        let mut engine2 = PolicyEngine::new();
        engine2.add_rule(PolicyRule {
            agent: "*".to_string(),
            action: "*".to_string(),
            resource: "*".to_string(),
            effect: PolicyEffect::Deny,
        });
        engine2.save_to_persistence(&persistence).unwrap();

        // Reload first engine - should get new rules
        engine.reload(&persistence).unwrap();
        assert_eq!(engine.rule_count(), 1);

        // Should have the deny-all rule now
        match engine.check("coder", "tool_call", "echo") {
            PolicyDecision::Deny(_) => {}
            _ => panic!("Expected deny after reload"),
        }
    }

    #[test]
    fn test_load_empty_persistence() {
        use crate::test_utils::create_test_db;

        let persistence = create_test_db();

        // Load from empty persistence - should get empty engine
        let engine = PolicyEngine::load_from_persistence(&persistence).unwrap();
        assert_eq!(engine.rule_count(), 0);

        // Should deny by default
        match engine.check("agent", "action", "resource") {
            PolicyDecision::Deny(_) => {}
            _ => panic!("Expected default deny"),
        }
    }
}
