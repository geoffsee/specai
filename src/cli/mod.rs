//! CLI module for Epic 4 — minimal REPL and command parser

pub mod formatting;

use anyhow::{Context, Result};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::agent::{AgentBuilder, AgentCore};
use crate::config::{AgentProfile, AgentRegistry, AppConfig};
use crate::persistence::Persistence;
use crate::policy::PolicyEngine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Quit,
    ConfigReload,
    ConfigShow,
    PolicyReload,
    SwitchAgent(String),
    ListAgents,
    MemoryShow(Option<usize>),
    SessionNew(Option<String>),
    SessionList,
    SessionSwitch(String),
    Message(String),
    Empty,
}

pub fn parse_command(input: &str) -> Command {
    let line = input.trim();
    if line.is_empty() {
        return Command::Empty;
    }

    if let Some(rest) = line.strip_prefix('/') {
        let mut parts = rest.split_whitespace();
        let cmd = parts.next().unwrap_or("").to_lowercase();
        match cmd.as_str() {
            "help" | "h" | "?" => Command::Help,
            "quit" | "q" | "exit" => Command::Quit,
            "config" => match parts.next() {
                Some("reload") => Command::ConfigReload,
                Some("show") => Command::ConfigShow,
                _ => Command::Help,
            },
            "policy" => match parts.next() {
                Some("reload") => Command::PolicyReload,
                _ => Command::Help,
            },
            "agents" | "list" => Command::ListAgents,
            "switch" => {
                let name = parts.next().unwrap_or("").to_string();
                if name.is_empty() {
                    Command::Help
                } else {
                    Command::SwitchAgent(name)
                }
            }
            "memory" => match parts.next() {
                Some("show") => {
                    let n = parts.next().and_then(|s| s.parse::<usize>().ok());
                    Command::MemoryShow(n)
                }
                _ => Command::Help,
            },
            "session" => match parts.next() {
                Some("new") => {
                    let id = parts.next().map(|s| s.to_string());
                    Command::SessionNew(id)
                }
                Some("list") => Command::SessionList,
                Some("switch") => {
                    let id = parts.next().unwrap_or("").to_string();
                    if id.is_empty() {
                        Command::Help
                    } else {
                        Command::SessionSwitch(id)
                    }
                }
                _ => Command::Help,
            },
            _ => Command::Help,
        }
    } else {
        Command::Message(line.to_string())
    }
}

pub struct CliState {
    pub config: AppConfig,
    pub persistence: Persistence,
    pub registry: AgentRegistry,
    pub agent: AgentCore,
}

impl CliState {
    /// Initialize from loaded config (AppConfig::load)
    pub fn initialize() -> Result<Self> {
        let config = AppConfig::load()?;
        Self::new_with_config(config)
    }

    /// Create a CLI state from a provided config
    pub fn new_with_config(config: AppConfig) -> Result<Self> {
        let persistence =
            Persistence::new(&config.database.path).context("initializing persistence")?;

        // Build registry and ensure an active agent exists
        let initial_agents = config.agents.clone();
        let registry = AgentRegistry::new(initial_agents.clone(), persistence.clone());
        registry.init()?;

        // Ensure we have an active agent
        if registry.active_name().is_none() {
            if let Some(default_name) = &config.default_agent {
                if registry.get(default_name).is_some() {
                    registry.set_active(default_name)?;
                }
            }
        }
        if registry.active_name().is_none() {
            // If still none, create or pick a default profile
            if initial_agents.is_empty() {
                let default_profile = AgentProfile::default();
                registry.upsert("default".to_string(), default_profile)?;
                registry.set_active("default")?;
            } else {
                // Pick first agent by name
                if let Some(first) = registry.list().first().cloned() {
                    registry.set_active(&first)?;
                }
            }
        }

        // Create the AgentCore from registry + config
        let agent = AgentBuilder::new_with_registry(&registry, &config, None)?;

        Ok(Self {
            config,
            persistence,
            registry,
            agent,
        })
    }

    /// Handle a single line of input. Returns an optional output string.
    pub async fn handle_line(&mut self, line: &str) -> Result<Option<String>> {
        match parse_command(line) {
            Command::Empty => Ok(None),
            Command::Help => Ok(Some(formatting::render_help())),
            Command::Quit => Ok(Some("__QUIT__".to_string())),
            Command::ConfigShow => {
                let summary = self.config.summary();
                Ok(Some(formatting::render_config(&summary)))
            }
            Command::ListAgents => {
                let agents = self.registry.list();
                let active = self.registry.active_name();
                if agents.is_empty() {
                    Ok(Some("No agents configured.".to_string()))
                } else {
                    let agent_data: Vec<(String, bool, Option<String>)> = agents
                        .into_iter()
                        .map(|name| {
                            let is_active = Some(&name) == active.as_ref();
                            let description =
                                self.registry.get(&name).and_then(|p| p.style.clone());
                            (name, is_active, description)
                        })
                        .collect();
                    Ok(Some(formatting::render_agent_table(agent_data)))
                }
            }
            Command::ConfigReload => {
                let current_session = self.agent.session_id().to_string();
                self.config = AppConfig::load()?;
                // rebuild persistence (path may have changed)
                self.persistence = Persistence::new(&self.config.database.path)?;
                // rebuild registry with new agents
                self.registry =
                    AgentRegistry::new(self.config.agents.clone(), self.persistence.clone());
                self.registry.init()?;
                if let Some(default_name) = &self.config.default_agent {
                    let _ = self.registry.set_active(default_name);
                }
                // Recreate agent preserving session
                self.agent = AgentBuilder::new_with_registry(
                    &self.registry,
                    &self.config,
                    Some(current_session),
                )?;
                Ok(Some("Configuration reloaded.".to_string()))
            }
            Command::PolicyReload => {
                // Load policies from persistence
                let policy_engine = PolicyEngine::load_from_persistence(&self.persistence)
                    .context("Failed to load policies from persistence")?;
                let rule_count = policy_engine.rule_count();

                // Update the agent's policy engine
                self.agent
                    .set_policy_engine(std::sync::Arc::new(policy_engine));

                Ok(Some(format!(
                    "Policies reloaded. {} rule(s) active.",
                    rule_count
                )))
            }
            Command::SwitchAgent(name) => {
                self.registry.set_active(&name)?;
                let session = self.agent.session_id().to_string();
                self.agent =
                    AgentBuilder::new_with_registry(&self.registry, &self.config, Some(session))?;
                Ok(Some(format!("Switched active agent to '{}'.", name)))
            }
            Command::MemoryShow(n) => {
                let limit = n.unwrap_or(10) as i64;
                let sid = self.agent.session_id().to_string();
                let msgs = self.persistence.list_messages(&sid, limit)?;
                if msgs.is_empty() {
                    Ok(Some("No messages in this session.".to_string()))
                } else {
                    let messages: Vec<(String, String)> = msgs
                        .into_iter()
                        .map(|m| (m.role.as_str().to_string(), m.content))
                        .collect();
                    Ok(Some(formatting::render_memory(messages)))
                }
            }
            Command::SessionNew(id_opt) => {
                let new_id = id_opt.unwrap_or_else(|| {
                    format!("session-{}", chrono::Utc::now().timestamp_millis())
                });
                self.agent = AgentBuilder::new_with_registry(
                    &self.registry,
                    &self.config,
                    Some(new_id.clone()),
                )?;
                Ok(Some(format!("Started new session '{}'.", new_id)))
            }
            Command::SessionList => {
                let sessions = self.persistence.list_sessions()?;
                if sessions.is_empty() {
                    return Ok(Some("No sessions yet.".to_string()));
                }
                Ok(Some(formatting::render_list(
                    "Sessions (most recent first)",
                    sessions,
                )))
            }
            Command::SessionSwitch(id) => {
                self.agent = AgentBuilder::new_with_registry(
                    &self.registry,
                    &self.config,
                    Some(id.clone()),
                )?;
                Ok(Some(format!("Switched to session '{}'.", id)))
            }
            Command::Message(text) => {
                let output = self.agent.run_step(&text).await?;
                let mut formatted =
                    formatting::render_agent_response("assistant", &output.response);
                if let Some(stats) = formatting::render_run_stats(&output) {
                    formatted.push('\n');
                    formatted.push_str(&stats);
                }
                Ok(Some(formatted))
            }
        }
    }

    /// Run interactive REPL on stdin/stdout
    pub async fn run_repl(&mut self) -> Result<()> {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        let mut stdout = tokio::io::stdout();

        // Print welcome and summary
        stdout.write_all(self.config.summary().as_bytes()).await?;
        stdout.write_all(b"\nType /help for commands.\n").await?;
        stdout.flush().await?;

        loop {
            stdout.write_all(b"> ").await?;
            stdout.flush().await?;
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                break;
            } // EOF
            if let Some(out) = self.handle_line(&line).await? {
                if out == "__QUIT__" {
                    break;
                }
                stdout.write_all(out.as_bytes()).await?;
                if !out.ends_with('\n') {
                    stdout.write_all(b"\n").await?;
                }
                stdout.flush().await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DatabaseConfig, LoggingConfig, ModelConfig, UiConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn test_parse_commands() {
        assert_eq!(parse_command("/help"), Command::Help);
        assert_eq!(parse_command("/quit"), Command::Quit);
        assert_eq!(parse_command("/config reload"), Command::ConfigReload);
        assert_eq!(parse_command("/config show"), Command::ConfigShow);
        assert_eq!(parse_command("/agents"), Command::ListAgents);
        assert_eq!(parse_command("/list"), Command::ListAgents);
        assert_eq!(
            parse_command("/switch coder"),
            Command::SwitchAgent("coder".into())
        );
        assert_eq!(
            parse_command("/memory show 5"),
            Command::MemoryShow(Some(5))
        );
        assert_eq!(parse_command("/session list"), Command::SessionList);
        assert_eq!(parse_command("/session new"), Command::SessionNew(None));
        assert_eq!(
            parse_command("/session new s2"),
            Command::SessionNew(Some("s2".into()))
        );
        assert_eq!(
            parse_command("/session switch abc"),
            Command::SessionSwitch("abc".into())
        );
        assert_eq!(parse_command("hello"), Command::Message("hello".into()));
        assert_eq!(parse_command("   "), Command::Empty);
    }

    #[tokio::test]
    async fn test_cli_smoke() {
        // Force plain text mode for consistent test output
        formatting::set_plain_text_mode(true);

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cli.duckdb");

        // Minimal config with one agent
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
            agents,
            default_agent: Some("test".into()),
        };

        let mut cli = CliState::new_with_config(config).unwrap();

        // Send a user message
        let out1 = cli.handle_line("hello").await.unwrap().unwrap();
        assert!(!out1.is_empty()); // mock response

        // Memory show should show the last two messages
        let out2 = cli.handle_line("/memory show 10").await.unwrap().unwrap();
        assert!(out2.contains("user:"));
        assert!(out2.contains("assistant:"));

        // Start a new session and ensure it switches
        let out3 = cli.handle_line("/session new s2").await.unwrap().unwrap();
        assert!(out3.contains("s2"));

        // Send another message in new session
        let _ = cli.handle_line("hi").await.unwrap().unwrap();

        // List sessions should include s2
        let out4 = cli.handle_line("/session list").await.unwrap().unwrap();
        assert!(out4.contains("s2"));
    }

    #[tokio::test]
    async fn test_list_agents_command() {
        // Force plain text mode for consistent test output
        formatting::set_plain_text_mode(true);

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cli_agents.duckdb");

        // Config with multiple agents
        let mut agents = HashMap::new();
        agents.insert("coder".to_string(), AgentProfile::default());
        agents.insert("researcher".to_string(), AgentProfile::default());

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
            agents,
            default_agent: Some("coder".into()),
        };

        let mut cli = CliState::new_with_config(config).unwrap();

        // Test /agents command
        let out = cli.handle_line("/agents").await.unwrap().unwrap();
        assert!(out.contains("Available agents:"));
        assert!(out.contains("coder"));
        assert!(out.contains("researcher"));
        assert!(out.contains("(active)")); // coder should be marked active

        // Test /list alias
        let out2 = cli.handle_line("/list").await.unwrap().unwrap();
        assert!(out2.contains("Available agents:"));
    }

    #[tokio::test]
    async fn test_config_show_command() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cli_config.duckdb");

        let mut agents = HashMap::new();
        agents.insert("test".to_string(), AgentProfile::default());

        let config = AppConfig {
            database: DatabaseConfig {
                path: db_path.clone(),
            },
            model: ModelConfig {
                provider: "mock".into(),
                model_name: Some("test-model".into()),
                embeddings_model: None,
                api_key_source: None,
                temperature: 0.8,
            },
            ui: UiConfig {
                prompt: "> ".into(),
                theme: "dark".into(),
            },
            logging: LoggingConfig {
                level: "debug".into(),
            },
            agents,
            default_agent: Some("test".into()),
        };

        let mut cli = CliState::new_with_config(config).unwrap();

        // Test /config show command
        let out = cli.handle_line("/config show").await.unwrap().unwrap();
        assert!(out.contains("Configuration loaded:"));
        assert!(out.contains("Model Provider: mock"));
        assert!(out.contains("Model Name: test-model"));
        assert!(out.contains("Temperature: 0.8"));
        assert!(out.contains("Logging Level: debug"));
        assert!(out.contains("UI Theme: dark"));
    }

    #[tokio::test]
    async fn test_help_command() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cli_help.duckdb");

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
            agents,
            default_agent: Some("test".into()),
        };

        let mut cli = CliState::new_with_config(config).unwrap();

        // Test /help command - output now includes markdown formatting
        let out = cli.handle_line("/help").await.unwrap().unwrap();
        assert!(out.contains("Commands") || out.contains("SpecAI"));
        assert!(out.contains("/config show") || out.contains("config"));
        assert!(out.contains("/agents") || out.contains("agents"));
        assert!(out.contains("/list") || out.contains("list"));
    }
}
