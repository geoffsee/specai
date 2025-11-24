//! CLI module for Epic 4 — minimal REPL and command parser

pub mod formatting;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::agent::core::MemoryRecallStrategy;
use crate::agent::{
    create_transcription_provider, create_transcription_provider_simple, TranscriptionProvider,
};
use crate::agent::{AgentBuilder, AgentCore, AgentOutput};
use crate::bootstrap_self::BootstrapSelf;
use crate::config::{AgentProfile, AgentRegistry, AppConfig};
use crate::persistence::Persistence;
use crate::policy::PolicyEngine;
use crate::spec::AgentSpec;
use terminal_size::terminal_size;

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
    // Graph commands
    GraphEnable,
    GraphDisable,
    GraphStatus,
    GraphShow(Option<usize>),
    GraphClear,
    // Audio commands
    ListenStart(Option<u64>), // duration in seconds
    ListenStop,
    ListenStatus,
    Listen(Option<String>, Option<u64>), // Deprecated: kept for backward compatibility
    PasteStart,
    RunSpec(PathBuf),
    Init(Option<Vec<String>>),    // optional plugins list
    Refresh(Option<Vec<String>>), // rerun bootstrap with caching
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
            "graph" => match parts.next() {
                Some("enable") => Command::GraphEnable,
                Some("disable") => Command::GraphDisable,
                Some("status") => Command::GraphStatus,
                Some("show") => {
                    let n = parts.next().and_then(|s| s.parse::<usize>().ok());
                    Command::GraphShow(n)
                }
                Some("clear") => Command::GraphClear,
                _ => Command::Help,
            },
            "listen" => {
                match parts.next() {
                    Some("stop") => Command::ListenStop,
                    Some("status") => Command::ListenStatus,
                    Some("start") => {
                        let duration = parts.next().and_then(|s| s.parse::<u64>().ok());
                        Command::ListenStart(duration)
                    }
                    Some(duration_str) => {
                        // If it's a number, treat it as duration for backward compatibility
                        let duration = duration_str.parse::<u64>().ok();
                        Command::ListenStart(duration)
                    }
                    None => Command::ListenStart(None),
                }
            }
            "paste" => Command::PasteStart,
            "init" => {
                let plugins = if let Some(arg) = parts.next() {
                    if arg.starts_with("--plugins=") {
                        Some(
                            arg.strip_prefix("--plugins=")
                                .unwrap_or("")
                                .split(',')
                                .map(|p| p.trim().to_string())
                                .collect(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };
                Command::Init(plugins)
            }
            "refresh" => {
                let plugins = if let Some(arg) = parts.next() {
                    if arg.starts_with("--plugins=") {
                        Some(
                            arg.strip_prefix("--plugins=")
                                .unwrap_or("")
                                .split(',')
                                .map(|p| p.trim().to_string())
                                .collect(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };
                Command::Refresh(plugins)
            }
            "spec" => {
                let args: Vec<&str> = parts.collect();
                if args.is_empty() {
                    Command::Help
                } else {
                    let (path_parts, _explicit_run) = if args[0].eq_ignore_ascii_case("run") {
                        (args[1..].to_vec(), true)
                    } else {
                        (args, false)
                    };
                    if path_parts.is_empty() {
                        Command::Help
                    } else {
                        let path = path_parts.join(" ");
                        Command::RunSpec(PathBuf::from(path))
                    }
                }
            }
            _ => Command::Help,
        }
    } else {
        Command::Message(line.to_string())
    }
}

/// Transcription task handle for background listening
struct TranscriptionTask {
    handle: std::thread::JoinHandle<()>,
    stop_tx: mpsc::UnboundedSender<()>,
    started_at: std::time::SystemTime,
    duration_secs: Option<u64>,
    chunks_rx: mpsc::UnboundedReceiver<String>,
}

pub struct CliState {
    pub config: AppConfig,
    pub persistence: Persistence,
    pub registry: AgentRegistry,
    pub agent: AgentCore,
    pub transcription_provider: Arc<dyn TranscriptionProvider>,
    pub reasoning_messages: Vec<String>,
    pub status_message: String,
    paste_mode: bool,
    paste_buffer: String,
    init_allowed: bool,
    transcription_task: Option<TranscriptionTask>,
}

impl CliState {
    /// Initialize from loaded config (AppConfig::load)
    pub fn initialize() -> Result<Self> {
        let config = AppConfig::load()?;
        Self::new_with_config(config)
    }

    /// Initialize from a specific config file path
    pub fn initialize_with_path(path: Option<PathBuf>) -> Result<Self> {
        let config = if let Some(config_path) = path {
            AppConfig::load_from_file(&config_path)?
        } else {
            AppConfig::load()?
        };
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

        // Create transcription provider from config
        let transcription_provider = {
            use crate::agent::transcription_factory::TranscriptionProviderConfig;
            let provider_config = TranscriptionProviderConfig {
                provider: config.audio.provider.clone(),
                api_key_source: config.audio.api_key_source.clone(),
                endpoint: config.audio.endpoint.clone(),
                on_device: config.audio.on_device,
                settings: serde_json::Value::Null,
            };
            create_transcription_provider(&provider_config)
                .or_else(|_| create_transcription_provider_simple("mock"))
                .context("Failed to create transcription provider")?
        };

        let mut state = Self {
            config,
            persistence,
            registry,
            agent,
            transcription_provider,
            reasoning_messages: vec!["Reasoning: idle".to_string()],
            status_message: "Status: initializing".to_string(),
            paste_mode: false,
            paste_buffer: String::new(),
            init_allowed: true,
            transcription_task: None,
        };

        state.refresh_init_gate()?;

        Ok(state)
    }

    /// Save transcription chunks to database with embeddings
    async fn save_transcription_chunks(&self, chunks: &[String]) -> usize {
        let session_id = self.agent.session_id();
        let mut chunk_count = 0;
        for (idx, text) in chunks.iter().enumerate() {
            let timestamp = chrono::Utc::now();

            // Insert transcription
            match self
                .persistence
                .insert_transcription(session_id, idx as i64, text, timestamp)
            {
                Ok(transcription_id) => {
                    chunk_count += 1;

                    // Generate and link embedding
                    if let Some(embedding_id) = self.agent.generate_embedding(text).await {
                        if let Err(e) = self
                            .persistence
                            .update_transcription_embedding(transcription_id, embedding_id)
                        {
                            eprintln!(
                                "[Transcription] Failed to link embedding for chunk {}: {}",
                                idx, e
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Transcription] Failed to save chunk {}: {}", idx, e);
                }
            }
        }
        chunk_count
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
                self.refresh_init_gate()?;
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
                self.init_allowed = true;
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
                self.refresh_init_gate()?;
                Ok(Some(format!("Switched to session '{}'.", id)))
            }
            // Graph commands
            Command::GraphEnable => {
                // For now, just show instructions for enabling graph features
                // Since modifying the agent at runtime requires complex rebuilding
                Ok(Some(
                    "To enable knowledge graph features, update your spec-ai.config.toml:\n\n\
                    [agents.your_agent_name]\n\
                    enable_graph = true\n\
                    graph_memory = true\n\
                    auto_graph = true\n\
                    graph_steering = true\n\
                    graph_depth = 3\n\
                    graph_weight = 0.5\n\
                    graph_threshold = 0.7\n\n\
                    Then run: /config reload"
                        .to_string(),
                ))
            }
            Command::GraphDisable => {
                // For now, just show instructions for disabling graph features
                Ok(Some(
                    "To disable knowledge graph features, update your spec-ai.config.toml:\n\n\
                    [agents.your_agent_name]\n\
                    enable_graph = false\n\n\
                    Then run: /config reload"
                        .to_string(),
                ))
            }
            Command::GraphStatus => {
                let profile = self.agent.profile();
                let status = format!(
                    "Knowledge Graph Configuration:\n  \
                    Enabled: {}\n  \
                    Graph Memory: {}\n  \
                    Auto Build: {}\n  \
                    Graph Steering: {}\n  \
                    Traversal Depth: {}\n  \
                    Graph Weight: {:.2}\n  \
                    Tool Threshold: {:.2}",
                    profile.enable_graph,
                    profile.graph_memory,
                    profile.auto_graph,
                    profile.graph_steering,
                    profile.graph_depth,
                    profile.graph_weight,
                    profile.graph_threshold,
                );
                Ok(Some(status))
            }
            Command::GraphShow(limit) => {
                let limit_val = limit.unwrap_or(10) as i64;
                let session_id = self.agent.session_id();
                let nodes = self
                    .persistence
                    .list_graph_nodes(session_id, None, Some(limit_val))?;

                if nodes.is_empty() {
                    Ok(Some("No graph nodes in current session.".to_string()))
                } else {
                    let mut output = format!(
                        "Graph Nodes (showing {} of {}):\n",
                        nodes.len(),
                        nodes.len()
                    );
                    for node in &nodes {
                        output.push_str(&format!(
                            "  [{:?}] {} - {}\n",
                            node.node_type,
                            node.label,
                            node.properties["name"].as_str().unwrap_or("unnamed")
                        ));
                    }

                    // Also show edge count
                    let edges = self.persistence.list_graph_edges(session_id, None, None)?;
                    output.push_str(&format!("\nTotal edges: {}", edges.len()));

                    Ok(Some(output))
                }
            }
            Command::GraphClear => {
                let session_id = self.agent.session_id();

                // Get all nodes and delete them (edges will cascade)
                let nodes = self.persistence.list_graph_nodes(session_id, None, None)?;
                let count = nodes.len();

                for node in nodes {
                    self.persistence.delete_graph_node(node.id)?;
                }

                Ok(Some(format!(
                    "Cleared {} graph nodes for session '{}'",
                    count, session_id
                )))
            }
            Command::ListenStart(duration) => {
                use crate::agent::{TranscriptionConfig, TranscriptionEvent};
                use futures::StreamExt;

                // Check if already running
                if self.transcription_task.is_some() {
                    return Ok(Some(
                        "Transcription is already running. Use /listen stop to stop it first."
                            .to_string(),
                    ));
                }

                // Build transcription config from app config
                let config = TranscriptionConfig {
                    duration_secs: duration.or(Some(self.config.audio.default_duration_secs)),
                    chunk_duration_secs: self.config.audio.chunk_duration_secs,
                    model: self
                        .config
                        .audio
                        .model
                        .clone()
                        .unwrap_or_else(|| "whisper-1".to_string()),
                    out_file: self.config.audio.out_file.clone(),
                    language: self.config.audio.language.clone(),
                    endpoint: self.config.audio.endpoint.clone(),
                };

                // Create stop channel and chunks channel
                let (stop_tx, mut stop_rx) = mpsc::unbounded_channel::<()>();
                let (chunks_tx, chunks_rx) = mpsc::unbounded_channel::<String>();

                // Clone provider for background task
                let provider = Arc::clone(&self.transcription_provider);
                let provider_name = provider.metadata().name.clone();
                let provider_name_display = provider_name.clone(); // Clone for response message
                let started_at = std::time::SystemTime::now();

                // Spawn background thread with LocalSet for spawn_local support
                let handle = std::thread::spawn(move || {
                    // Create a current_thread runtime with LocalSet support
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("Failed to create runtime");

                    let local = tokio::task::LocalSet::new();

                    local.block_on(&rt, async move {
                        // Start transcription
                        let stream_result = provider.start_transcription(&config).await;

                        match stream_result {
                            Ok(mut stream) => {
                                println!("\n[Transcription] Started using {}", provider_name);

                                loop {
                                    tokio::select! {
                                        // Check for stop signal
                                        _ = stop_rx.recv() => {
                                            println!("\n[Transcription] Stopped by user");
                                            break;
                                        }
                                        // Process transcription events
                                        event = stream.next() => {
                                            match event {
                                                Some(Ok(TranscriptionEvent::Started { .. })) => {
                                                    // Already logged above
                                                }
                                                Some(Ok(TranscriptionEvent::Transcription { chunk_id, text, .. })) => {
                                                    println!("[Transcription] Chunk {}: {}", chunk_id, text);
                                                    let _ = chunks_tx.send(text);
                                                }
                                                Some(Ok(TranscriptionEvent::Error { chunk_id, message })) => {
                                                    eprintln!("[Transcription] Error in chunk {}: {}", chunk_id, message);
                                                }
                                                Some(Ok(TranscriptionEvent::Completed { total_chunks, .. })) => {
                                                    println!("[Transcription] Completed. Processed {} chunks.", total_chunks);
                                                    break;
                                                }
                                                Some(Err(e)) => {
                                                    eprintln!("[Transcription] Error: {}", e);
                                                    break;
                                                }
                                                None => {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[Transcription] Failed to start: {}", e);
                            }
                        }
                    })
                });

                // Store task info
                self.transcription_task = Some(TranscriptionTask {
                    handle,
                    stop_tx,
                    started_at,
                    duration_secs: duration.or(Some(self.config.audio.default_duration_secs)),
                    chunks_rx,
                });

                Ok(Some(format!(
                    "Started background transcription using {} (duration: {} seconds)\nUse /listen stop to stop, /listen status to check status.",
                    provider_name_display,
                    duration.or(Some(self.config.audio.default_duration_secs)).unwrap_or(30)
                )))
            }
            Command::ListenStop => {
                if let Some(mut task) = self.transcription_task.take() {
                    // Send stop signal
                    let _ = task.stop_tx.send(());

                    // Collect any remaining chunks
                    let mut chunks = Vec::new();
                    while let Ok(text) = task.chunks_rx.try_recv() {
                        chunks.push(text);
                    }

                    // Save to database
                    let chunk_count = self.save_transcription_chunks(&chunks).await;

                    let elapsed = task.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0);

                    Ok(Some(format!(
                        "Stopped transcription (ran for {} seconds, saved {} chunks to database)",
                        elapsed, chunk_count
                    )))
                } else {
                    Ok(Some("No transcription is currently running.".to_string()))
                }
            }
            Command::ListenStatus => {
                // Check if task is finished and save chunks if so
                if let Some(task) = self.transcription_task.take() {
                    if task.handle.is_finished() {
                        // Collect chunks
                        let mut chunks = Vec::new();
                        let mut chunks_rx = task.chunks_rx;
                        while let Ok(text) = chunks_rx.try_recv() {
                            chunks.push(text);
                        }

                        // Save to database
                        let chunk_count = self.save_transcription_chunks(&chunks).await;

                        let elapsed = task.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0);

                        return Ok(Some(format!(
                            "Transcription completed (ran for {} seconds, saved {} chunks to database)",
                            elapsed, chunk_count
                        )));
                    } else {
                        // Put it back since it's still running
                        self.transcription_task = Some(task);
                    }
                }

                if let Some(ref task) = self.transcription_task {
                    let elapsed = task.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0);

                    let duration_info = if let Some(dur) = task.duration_secs {
                        format!("/{} seconds", dur)
                    } else {
                        String::from(" (continuous)")
                    };

                    Ok(Some(format!(
                        "Transcription status: running\nElapsed: {}{}\nUse /listen stop to stop and save chunks.",
                        elapsed,
                        duration_info
                    )))
                } else {
                    Ok(Some("No transcription is currently running.\nUse /listen start [duration] to start.".to_string()))
                }
            }
            Command::Listen(_scenario, duration) => {
                // Redirect to new command
                Ok(Some(format!(
                    "The /listen command has been updated. Use:\n  /listen start [duration] - Start background transcription\n  /listen stop - Stop transcription\n  /listen status - Check status\n\nStarting transcription with {} seconds...",
                    duration.unwrap_or(self.config.audio.default_duration_secs)
                )))
            }
            Command::PasteStart => {
                // Paste mode is handled at the REPL loop level; this arm is mainly for tests
                Ok(Some(
                    "Entering paste mode. Paste your block and finish with /end on its own line."
                        .to_string(),
                ))
            }
            Command::RunSpec(path) => {
                let output = self.run_spec_command(&path).await?;
                Ok(Some(output))
            }
            Command::Init(plugins) => {
                if !self.init_allowed {
                    return Ok(Some(
                        "The /init command must be the first action in a session. Start a new session to run it again."
                            .to_string(),
                    ));
                }
                let bootstrapper =
                    BootstrapSelf::from_environment(&self.persistence, self.agent.session_id())?;
                let outcome = bootstrapper.run_with_plugins(plugins.clone())?;
                self.init_allowed = false;
                Ok(Some(format!(
                    "Knowledge graph bootstrap complete for '{}': {} nodes and {} edges captured ({} components, {} documents).",
                    outcome.repository_name,
                    outcome.nodes_created,
                    outcome.edges_created,
                    outcome.component_count,
                    outcome.document_count
                )))
            }
            Command::Refresh(plugins) => {
                let bootstrapper =
                    BootstrapSelf::from_environment(&self.persistence, self.agent.session_id())?;
                let outcome = bootstrapper.refresh_with_plugins(plugins.clone())?;
                self.init_allowed = false;
                Ok(Some(format!(
                    "Knowledge graph refresh complete for '{}': {} nodes and {} edges captured ({} components, {} documents).",
                    outcome.repository_name,
                    outcome.nodes_created,
                    outcome.edges_created,
                    outcome.component_count,
                    outcome.document_count
                )))
            }
            Command::Message(text) => {
                self.init_allowed = false;
                let output = self.agent.run_step(&text).await?;
                self.update_reasoning_messages(&output);
                let mut formatted =
                    formatting::render_agent_response("assistant", &output.response);
                let show_reasoning = self.agent.profile().show_reasoning;
                if let Some(stats) = formatting::render_run_stats(&output, show_reasoning) {
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

        self.set_status_idle();
        loop {
            self.render_reasoning_prompt(&mut stdout).await?;
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                break;
            } // EOF

            let trimmed = line.trim_end_matches(&['\n', '\r'][..]);

            // If we're currently in paste mode, accumulate lines until the user
            // types /end on its own line, then send the entire block as one
            // message.
            if self.paste_mode {
                if trimmed == "/end" {
                    // Leave paste mode and send the buffered block
                    self.paste_mode = false;
                    let full_input = std::mem::take(&mut self.paste_buffer);
                    let command_preview = Command::Message(full_input.clone());
                    self.update_status_for_command(&command_preview);
                    if !matches!(command_preview, Command::Empty) {
                        self.render_status_line(&mut stdout).await?;
                    }
                    if let Some(out) = self.handle_line(&full_input).await? {
                        if out == "__QUIT__" {
                            break;
                        }
                        stdout.write_all(out.as_bytes()).await?;
                        if !out.ends_with('\n') {
                            stdout.write_all(b"\n").await?;
                        }
                        stdout.flush().await?;
                    }
                    self.set_status_idle();
                } else if !trimmed.is_empty() {
                    if !self.paste_buffer.is_empty() {
                        self.paste_buffer.push('\n');
                    }
                    self.paste_buffer.push_str(trimmed);
                }
                continue;
            }

            // Normal mode: single-line commands and messages
            let command_preview = parse_command(&line);
            if matches!(command_preview, Command::PasteStart) {
                // Enter paste mode; UI hint
                self.paste_mode = true;
                self.paste_buffer.clear();
                self.status_message =
                    "Status: paste mode (end with /end on its own line)".to_string();
                self.render_status_line(&mut stdout).await?;
                continue;
            }

            self.update_status_for_command(&command_preview);
            if !matches!(command_preview, Command::Empty) {
                self.render_status_line(&mut stdout).await?;
            }
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
            self.set_status_idle();
        }

        // Checkpoint database before exiting to ensure all WAL data is written
        let _ = self.persistence.checkpoint();

        Ok(())
    }

    async fn run_spec_command(&mut self, path: &Path) -> Result<String> {
        let spec = AgentSpec::from_file(path)?;
        let mut intro = format!("Executing spec `{}`", spec.display_name());
        if let Some(source) = spec.source_path() {
            intro.push_str(&format!(" ({})", source.display()));
        }
        intro.push('\n');

        let preview = spec.preview();
        if !preview.is_empty() {
            intro.push('\n');
            intro.push_str(&preview);
            intro.push_str("\n\n");
        }

        let output = self.agent.run_spec(&spec).await?;
        self.update_reasoning_messages(&output);
        intro.push_str(&formatting::render_agent_response(
            "assistant",
            &output.response,
        ));
        let show_reasoning = self.agent.profile().show_reasoning;
        if let Some(stats) = formatting::render_run_stats(&output, show_reasoning) {
            intro.push('\n');
            intro.push_str(&stats);
        }

        Ok(intro)
    }

    fn update_reasoning_messages(&mut self, output: &AgentOutput) {
        self.reasoning_messages = Self::format_reasoning_messages(output);
    }

    fn format_reasoning_messages(output: &AgentOutput) -> Vec<String> {
        let mut lines = Vec::with_capacity(3);

        if let Some(stats) = &output.recall_stats {
            match &stats.strategy {
                MemoryRecallStrategy::Semantic {
                    requested,
                    returned,
                } => lines.push(format!(
                    "Recall: semantic (requested {}, returned {})",
                    requested, returned
                )),
                MemoryRecallStrategy::RecentContext { limit } => {
                    lines.push(format!("Recall: recent context (last {} messages)", limit))
                }
            }
        } else {
            lines.push("Recall: not used".to_string());
        }

        if let Some(invocation) = output.tool_invocations.last() {
            let status = if invocation.success { "ok" } else { "err" };
            lines.push(format!("Tool: {} ({})", invocation.name, status));
        } else {
            lines.push("Tool: idle".to_string());
        }

        if let Some(reason) = &output.finish_reason {
            lines.push(format!("Finish: {}", reason));
        } else if let Some(usage) = &output.token_usage {
            lines.push(format!(
                "Tokens: P {} C {} T {}",
                usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
            ));
        } else {
            lines.push("Finish: pending".to_string());
        }

        lines
    }

    fn set_status_idle(&mut self) {
        self.status_message = "Status: awaiting input".to_string();
    }

    fn update_status_for_command(&mut self, command: &Command) {
        self.status_message = Self::status_message_for_command(command);
    }

    fn status_message_for_command(command: &Command) -> String {
        match command {
            Command::Empty => "Status: awaiting input".to_string(),
            Command::Help => "Status: showing help".to_string(),
            Command::Quit => "Status: exiting".to_string(),
            Command::ConfigReload => "Status: reloading configuration".to_string(),
            Command::ConfigShow => "Status: displaying configuration".to_string(),
            Command::PolicyReload => "Status: reloading policies".to_string(),
            Command::SwitchAgent(name) => {
                format!("Status: switching to agent '{}'", name)
            }
            Command::ListAgents => "Status: listing agents".to_string(),
            Command::MemoryShow(Some(limit)) => {
                format!("Status: showing last {} messages", limit)
            }
            Command::MemoryShow(None) => "Status: showing recent messages".to_string(),
            Command::SessionNew(Some(id)) => {
                format!("Status: starting session '{}'", id)
            }
            Command::SessionNew(None) => "Status: starting new session".to_string(),
            Command::SessionList => "Status: listing sessions".to_string(),
            Command::SessionSwitch(id) => {
                format!("Status: switching to session '{}'", id)
            }
            Command::GraphEnable => "Status: showing graph enable instructions".to_string(),
            Command::GraphDisable => "Status: showing graph disable instructions".to_string(),
            Command::GraphStatus => "Status: showing graph status".to_string(),
            Command::GraphShow(Some(limit)) => {
                format!("Status: inspecting graph (limit {})", limit)
            }
            Command::GraphShow(None) => "Status: inspecting graph".to_string(),
            Command::GraphClear => "Status: clearing session graph".to_string(),
            Command::Init(_) => "Status: bootstrapping repository graph".to_string(),
            Command::ListenStart(duration) => {
                let mut status = "Status: starting background transcription".to_string();
                if let Some(d) = duration {
                    status.push_str(&format!(" for {} seconds", d));
                }
                status
            }
            Command::ListenStop => "Status: stopping transcription".to_string(),
            Command::ListenStatus => "Status: checking transcription status".to_string(),
            Command::Listen(scenario, duration) => {
                let mut status = "Status: starting audio transcription".to_string();
                if let Some(s) = scenario {
                    status.push_str(&format!(" (scenario: {})", s));
                }
                if let Some(d) = duration {
                    status.push_str(&format!(" for {} seconds", d));
                }
                status
            }
            Command::RunSpec(path) => {
                format!("Status: executing spec '{}'", path.display())
            }
            Command::PasteStart => {
                "Status: entering paste mode (end with /end on its own line)".to_string()
            }
            Command::Message(_) => "Status: running agent step".to_string(),
            Command::Refresh(_) => {
                "Status: refreshing internal knowledge graph".to_string()
            }
        }
    }

    fn pad_line_to_width(line: &str, width: usize) -> String {
        if width == 0 {
            return String::new();
        }
        let truncated: String = line.chars().take(width).collect();
        let truncated_len = truncated.chars().count();
        if truncated_len >= width {
            return truncated;
        }
        let mut padded = truncated;
        padded.push_str(&" ".repeat(width - truncated_len));
        padded
    }

    fn reasoning_display_lines(&self, width: usize) -> Vec<String> {
        (0..3)
            .map(|idx| {
                let content = self
                    .reasoning_messages
                    .get(idx)
                    .map(String::as_str)
                    .unwrap_or("");
                Self::pad_line_to_width(content, width)
            })
            .collect()
    }

    fn status_display_line(&self, width: usize) -> String {
        Self::pad_line_to_width(&self.status_message, width)
    }

    fn input_display_width(&self) -> usize {
        let terminal_width = terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(80);
        let prompt_len = self.config.ui.prompt.chars().count();
        if terminal_width <= prompt_len {
            1
        } else {
            terminal_width - prompt_len
        }
    }

    async fn render_reasoning_prompt(&self, stdout: &mut io::Stdout) -> Result<()> {
        let width = self.input_display_width();
        for line in self.reasoning_display_lines(width) {
            stdout.write_all(line.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
        }
        stdout.write_all(b"\n").await?;
        let status_line = self.status_display_line(width);
        stdout.write_all(status_line.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.write_all(self.config.ui.prompt.as_bytes()).await?;
        stdout.flush().await?;
        Ok(())
    }

    async fn render_status_line(&self, stdout: &mut io::Stdout) -> Result<()> {
        let width = self.input_display_width();
        let status_line = self.status_display_line(width);
        stdout.write_all(status_line.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        Ok(())
    }

    fn refresh_init_gate(&mut self) -> Result<()> {
        let messages = self.persistence.list_messages(self.agent.session_id(), 1)?;
        self.init_allowed = messages.is_empty();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::core::{MemoryRecallStats, MemoryRecallStrategy, ToolInvocation};
    use crate::agent::model::TokenUsage;
    use crate::agent::AgentOutput;
    use crate::config::{AudioConfig, DatabaseConfig, LoggingConfig, ModelConfig, UiConfig};
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_parse_commands() {
        assert_eq!(parse_command("/help"), Command::Help);
        assert_eq!(parse_command("/quit"), Command::Quit);
        assert_eq!(parse_command("/config reload"), Command::ConfigReload);
        assert_eq!(parse_command("/config show"), Command::ConfigShow);
        assert_eq!(parse_command("/agents"), Command::ListAgents);
        assert_eq!(parse_command("/list"), Command::ListAgents);
        assert_eq!(parse_command("/init"), Command::Init(None));
        assert_eq!(
            parse_command("/init --plugins=rust-cargo"),
            Command::Init(Some(vec!["rust-cargo".to_string()]))
        );
        assert_eq!(
            parse_command("/init --plugins=rust-cargo,python"),
            Command::Init(Some(vec!["rust-cargo".to_string(), "python".to_string()]))
        );
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
        assert_eq!(
            parse_command("/spec run plan.spec"),
            Command::RunSpec(PathBuf::from("plan.spec"))
        );
        assert_eq!(
            parse_command("/spec nested/path/my.spec"),
            Command::RunSpec(PathBuf::from("nested/path/my.spec"))
        );
        assert_eq!(parse_command("hello"), Command::Message("hello".into()));
        assert_eq!(parse_command("   "), Command::Empty);
    }

    #[test]
    fn reasoning_messages_default() {
        let output = AgentOutput {
            response: String::new(),
            response_message_id: None,
            token_usage: None,
            tool_invocations: Vec::new(),
            finish_reason: None,
            recall_stats: None,
            run_id: "run-default".to_string(),
            next_action: None,
            reasoning: None,
            reasoning_summary: None,
            graph_debug: None,
        };
        let lines = CliState::format_reasoning_messages(&output);
        assert_eq!(
            lines,
            vec![
                "Recall: not used".to_string(),
                "Tool: idle".to_string(),
                "Finish: pending".to_string()
            ]
        );
    }

    #[test]
    fn reasoning_messages_with_details() {
        let stats = MemoryRecallStats {
            strategy: MemoryRecallStrategy::Semantic {
                requested: 5,
                returned: 2,
            },
            matches: Vec::new(),
        };
        let invocation = ToolInvocation {
            name: "search".to_string(),
            arguments: json!({}),
            success: true,
            output: Some("ok".to_string()),
            error: None,
        };
        let output = AgentOutput {
            response: String::new(),
            response_message_id: None,
            token_usage: None,
            tool_invocations: vec![invocation],
            finish_reason: Some("stop".to_string()),
            recall_stats: Some(stats),
            run_id: "run-details".to_string(),
            next_action: None,
            reasoning: None,
            reasoning_summary: None,
            graph_debug: None,
        };
        let lines = CliState::format_reasoning_messages(&output);
        assert!(lines[0].starts_with("Recall: semantic"));
        assert!(lines[1].contains("search"));
        assert_eq!(lines[2], "Finish: stop");
    }

    #[test]
    fn reasoning_messages_tokens() {
        let usage = TokenUsage {
            prompt_tokens: 4,
            completion_tokens: 6,
            total_tokens: 10,
        };
        let output = AgentOutput {
            response: String::new(),
            response_message_id: None,
            token_usage: Some(usage),
            tool_invocations: Vec::new(),
            finish_reason: None,
            recall_stats: None,
            run_id: "run-tokens".to_string(),
            next_action: None,
            reasoning: None,
            reasoning_summary: None,
            graph_debug: None,
        };
        let lines = CliState::format_reasoning_messages(&output);
        assert_eq!(lines[2], "Tokens: P 4 C 6 T 10");
    }

    // #[tokio::test]
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
            audio: AudioConfig::default(),
            mesh: crate::config::MeshConfig::default(),
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
            audio: AudioConfig::default(),
            mesh: crate::config::MeshConfig::default(),
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
            audio: AudioConfig::default(),
            mesh: crate::config::MeshConfig::default(),
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
            audio: AudioConfig::default(),
            mesh: crate::config::MeshConfig::default(),
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
