use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use spec_ai::cli::CliState;
use spec_ai::spec::AgentSpec;
use std::path::PathBuf;
use walkdir::WalkDir;

#[cfg(feature = "api")]
use {
    spec_ai::api::server::{ApiConfig, ApiServer},
    spec_ai::config::AgentRegistry,
    spec_ai::embeddings::EmbeddingsClient,
    spec_ai::persistence::Persistence,
    spec_ai::tools::ToolRegistry,
    std::sync::Arc,
};

#[derive(Parser)]
#[command(name = "spec-ai")]
#[command(about = "SpecAI - AI agent framework with spec execution", long_about = None)]
struct Cli {
    /// Path to config file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run one or more spec files
    Run {
        /// Spec files or directories to run. If not provided, uses specs/smoke.spec
        #[arg(value_name = "SPEC_OR_DIR")]
        specs: Vec<PathBuf>,
    },
    /// Start the API server for agent mesh functionality
    Server {
        /// Port to bind the server to
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Host address to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Join existing mesh at specified address
        #[arg(long)]
        join: Option<String>,
    },
}

fn collect_spec_files(path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut specs = Vec::new();

    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("spec") {
            specs.push(path.clone());
        } else {
            eprintln!(
                "Warning: Skipping '{}' (expected .spec extension)",
                path.display()
            );
        }
    } else if path.is_dir() {
        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "spec" {
                        specs.push(entry.path().to_path_buf());
                    }
                }
            }
        }
        specs.sort();
    } else {
        anyhow::bail!("Path '{}' does not exist", path.display());
    }

    Ok(specs)
}

async fn run_spec_file(cli: &mut CliState, spec_path: &PathBuf) -> Result<bool> {
    if !spec_path.exists() {
        eprintln!("Error: Spec file '{}' not found", spec_path.display());
        return Ok(false);
    }

    let abs_path = spec_path.canonicalize().with_context(|| {
        format!(
            "Failed to resolve absolute path for '{}'",
            spec_path.display()
        )
    })?;

    println!("=== Running spec: {} ===", abs_path.display());

    let spec = AgentSpec::from_file(&abs_path)?;
    let output = cli.agent.run_spec(&spec).await?;

    // Print the response
    println!("{}", output.response);

    // If execution completes without throwing an error, consider it successful
    // The agent will handle reporting any issues in the response
    Ok(true)
}

#[cfg(feature = "api")]
async fn start_server(
    config_path: Option<PathBuf>,
    host: String,
    port: u16,
    join: Option<String>,
) -> Result<()> {
    use spec_ai::api::mesh::MeshClient;
    use spec_ai::config::AppConfig;
    use spec_ai::embeddings::EmbeddingsClient;
    use std::net::TcpListener;

    // Generate unique instance ID
    let instance_id = MeshClient::generate_instance_id();
    println!("Instance ID: {}", instance_id);

    // Determine if we should join an existing mesh or start as leader
    if let Some(ref registry_addr) = join {
        // Explicit join - find an available port for ourselves
        let mut test_port = port;
        let max_attempts = 100;
        for _ in 0..max_attempts {
            if TcpListener::bind(format!("{}:{}", host, test_port)).is_ok() {
                println!("Joining mesh at {} on port {}", registry_addr, test_port);
                return start_mesh_member(
                    config_path,
                    host,
                    test_port,
                    registry_addr.clone(),
                    instance_id,
                )
                .await;
            }
            test_port += 1;
        }
        anyhow::bail!("Could not find available port after {} attempts", max_attempts);
    }

    // Check if port is available
    match TcpListener::bind(format!("{}:{}", host, port)) {
        Ok(_listener) => {
            // Port is available, we'll be the mesh leader/registry
            println!("Starting spec-ai server as mesh leader on {}:{}", host, port);
            drop(_listener); // Release the port before starting the actual server
        }
        Err(_) => {
            // Port is in use - try to detect and join existing mesh
            println!("Port {} is in use. Checking for existing mesh registry...", port);
            let health_url = format!("http://{}:{}/health", host, port);
            match reqwest::get(&health_url).await {
                Ok(response) if response.status().is_success() => {
                    println!("Found existing spec-ai mesh registry at {}:{}", host, port);
                    // Find an available port for ourselves
                    let mut test_port = port + 1;
                    let max_attempts = 100;
                    for _ in 0..max_attempts {
                        if TcpListener::bind(format!("{}:{}", host, test_port)).is_ok() {
                            println!("Joining mesh on port {}", test_port);
                            let registry_url = format!("{}:{}", host, port);
                            return start_mesh_member(
                                config_path,
                                host,
                                test_port,
                                registry_url,
                                instance_id,
                            )
                            .await;
                        }
                        test_port += 1;
                    }
                    anyhow::bail!("Could not find available port after {} attempts", max_attempts);
                }
                _ => {
                    eprintln!("Error: Port {} is in use by another process", port);
                    eprintln!("Please specify a different port with --port");
                    std::process::exit(1);
                }
            }
        }
    }

    // Load configuration
    let app_config = if let Some(path) = config_path {
        AppConfig::load_from_file(&path)?
    } else {
        AppConfig::load()?
    };

    // Initialize persistence
    let persistence = Persistence::new(&app_config.database.path)?;

    // Initialize embeddings client if configured
    let embeddings = if let Some(embeddings_model) = &app_config.model.embeddings_model {
        if let Some(api_key_source) = &app_config.model.api_key_source {
            // Resolve API key from environment or file
            let api_key = if api_key_source.starts_with("ENV:") {
                std::env::var(&api_key_source[4..]).ok()
            } else {
                std::fs::read_to_string(api_key_source).ok()
            };
            if let Some(key) = api_key {
                Some(EmbeddingsClient::with_api_key(embeddings_model.clone(), key))
            } else {
                Some(EmbeddingsClient::new(embeddings_model.clone()))
            }
        } else {
            Some(EmbeddingsClient::new(embeddings_model.clone()))
        }
    } else {
        None
    };

    // Create registries
    let agent_registry = Arc::new(AgentRegistry::new(
        app_config.agents.clone(),
        persistence.clone(),
    ));
    let tool_registry = Arc::new(ToolRegistry::with_builtin_tools(
        Some(Arc::new(persistence.clone())),
        embeddings,
    ));

    // Configure and start API server
    let api_config = ApiConfig::new()
        .with_host(host.clone())
        .with_port(port)
        .with_cors(true);

    let server = ApiServer::new(
        api_config.clone(),
        persistence.clone(),
        agent_registry.clone(),
        tool_registry.clone(),
        app_config.clone(),
    );

    println!("Server running at http://{}", api_config.bind_address());
    println!("Health check: http://{}/health", api_config.bind_address());
    println!("Press Ctrl+C to stop the server");

    // Self-register as leader in the mesh registry
    let mesh_registry = server.mesh_registry();
    let self_instance = spec_ai::api::mesh::MeshInstance {
        instance_id: instance_id.clone(),
        hostname: host.clone(),
        port,
        capabilities: vec!["registry".to_string(), "query".to_string()],
        is_leader: true,
        last_heartbeat: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        agent_profiles: agent_registry.list(),
    };
    mesh_registry.register(self_instance).await;

    // Start background heartbeat for self (keeps our own timestamp fresh)
    let heartbeat_instance_id = instance_id.clone();
    let heartbeat_registry = mesh_registry.clone();
    let heartbeat_interval = app_config.mesh.heartbeat_interval_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(heartbeat_interval));
        loop {
            interval.tick().await;
            let _ = heartbeat_registry.heartbeat(&heartbeat_instance_id).await;
        }
    });

    // Start stale instance cleanup task
    let cleanup_registry = mesh_registry.clone();
    let cleanup_timeout = app_config.mesh.leader_timeout_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(cleanup_timeout / 2));
        loop {
            interval.tick().await;
            cleanup_registry.cleanup_stale(cleanup_timeout).await;
        }
    });

    // Setup shutdown signal
    let shutdown_instance_id = instance_id.clone();
    let shutdown_registry = mesh_registry.clone();
    let shutdown = async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        println!("\nShutting down server...");
        // Deregister from mesh
        let _ = shutdown_registry.deregister(&shutdown_instance_id).await;
    };

    // Run server with graceful shutdown
    server.run_with_shutdown(shutdown).await?;

    println!("Server stopped");
    Ok(())
}

#[cfg(feature = "api")]
async fn start_mesh_member(
    config_path: Option<PathBuf>,
    host: String,
    port: u16,
    registry_url: String,
    instance_id: String,
) -> Result<()> {
    use spec_ai::api::mesh::MeshClient;
    use spec_ai::config::AppConfig;
    use spec_ai::embeddings::EmbeddingsClient;

    println!("Starting as mesh member on {}:{}", host, port);
    println!("Registry at: {}", registry_url);

    // Load configuration
    let app_config = if let Some(path) = config_path {
        AppConfig::load_from_file(&path)?
    } else {
        AppConfig::load()?
    };

    // Initialize persistence
    let persistence = Persistence::new(&app_config.database.path)?;

    // Initialize embeddings client if configured
    let embeddings = if let Some(embeddings_model) = &app_config.model.embeddings_model {
        if let Some(api_key_source) = &app_config.model.api_key_source {
            let api_key = if api_key_source.starts_with("ENV:") {
                std::env::var(&api_key_source[4..]).ok()
            } else {
                std::fs::read_to_string(api_key_source).ok()
            };
            if let Some(key) = api_key {
                Some(EmbeddingsClient::with_api_key(embeddings_model.clone(), key))
            } else {
                Some(EmbeddingsClient::new(embeddings_model.clone()))
            }
        } else {
            Some(EmbeddingsClient::new(embeddings_model.clone()))
        }
    } else {
        None
    };

    // Create registries
    let agent_registry = Arc::new(AgentRegistry::new(
        app_config.agents.clone(),
        persistence.clone(),
    ));
    let tool_registry = Arc::new(ToolRegistry::with_builtin_tools(
        Some(Arc::new(persistence.clone())),
        embeddings,
    ));

    // Get agent profiles for registration
    let agent_profiles: Vec<String> = agent_registry.list();

    // Register with the mesh
    let mesh_client = MeshClient::new(&registry_url.split(':').next().unwrap(),
        registry_url.split(':').nth(1).unwrap().parse()?);

    let register_response = mesh_client
        .register(
            instance_id.clone(),
            host.clone(),
            port,
            vec!["query".to_string()],
            agent_profiles,
        )
        .await?;

    println!("Registered with mesh:");
    println!("  Leader: {}", register_response.is_leader);
    println!("  Peers: {}", register_response.peers.len());

    // Start our API server
    let api_config = ApiConfig::new()
        .with_host(host.clone())
        .with_port(port)
        .with_cors(true);

    let server = ApiServer::new(
        api_config.clone(),
        persistence,
        agent_registry,
        tool_registry,
        app_config.clone(),
    );

    println!("Server running at http://{}", api_config.bind_address());

    // Start background heartbeat to registry
    let heartbeat_instance_id = instance_id.clone();
    let heartbeat_client = mesh_client.clone();
    let heartbeat_interval = app_config.mesh.heartbeat_interval_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(heartbeat_interval));
        loop {
            interval.tick().await;
            if let Err(e) = heartbeat_client.heartbeat(&heartbeat_instance_id, None).await {
                eprintln!("Heartbeat failed: {}", e);
            }
        }
    });

    // Setup shutdown signal with deregistration
    let shutdown_instance_id = instance_id.clone();
    let shutdown_client = mesh_client.clone();
    let shutdown = async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        println!("\nShutting down server...");
        // Deregister from mesh
        if let Err(e) = shutdown_client.deregister(&shutdown_instance_id).await {
            eprintln!("Failed to deregister: {}", e);
        }
    };

    // Run server with graceful shutdown
    server.run_with_shutdown(shutdown).await?;

    println!("Server stopped");
    Ok(())
}

async fn run_specs_command(config_path: Option<PathBuf>, spec_paths: Vec<PathBuf>) -> Result<i32> {
    // Determine which specs to run
    let specs_to_run = if spec_paths.is_empty() {
        let default_spec = PathBuf::from("specs/smoke.spec");
        if !default_spec.exists() {
            eprintln!("Error: Default spec not found at 'specs/smoke.spec'.");
            eprintln!("Please provide explicit spec files or create the default spec.");
            return Ok(1);
        }
        vec![default_spec]
    } else {
        let mut all_specs = Vec::new();
        for path in &spec_paths {
            let specs = collect_spec_files(path)?;
            all_specs.extend(specs);
        }

        if all_specs.is_empty() {
            eprintln!("Error: No .spec files found in provided paths.");
            return Ok(1);
        }

        all_specs
    };

    // Initialize CLI state
    let mut cli = match CliState::initialize_with_path(config_path) {
        Ok(cli) => cli,
        Err(e) => {
            let error_chain = format!("{:#}", e);
            if error_chain.contains("Could not set lock")
                || error_chain.contains("Conflicting lock")
            {
                eprintln!("Error: Another instance of spec-ai is already running.");
                eprintln!();
                eprintln!("Only one instance can access the database at a time.");
                eprintln!("Please close the other instance or wait for it to finish.");
                std::process::exit(1);
            }
            return Err(e);
        }
    };

    // Run each spec file
    let mut all_success = true;
    for spec_path in specs_to_run {
        match run_spec_file(&mut cli, &spec_path).await {
            Ok(success) => {
                if !success {
                    all_success = false;
                }
            }
            Err(e) => {
                eprintln!("Error running spec '{}': {}", spec_path.display(), e);
                all_success = false;
            }
        }
    }

    Ok(if all_success { 0 } else { 1 })
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run { specs }) => {
            let exit_code = run_specs_command(cli.config, specs).await?;
            std::process::exit(exit_code);
        }
        #[cfg(feature = "api")]
        Some(Commands::Server { port, host, join }) => {
            start_server(cli.config, host, port, join).await?;
            Ok(())
        }
        #[cfg(not(feature = "api"))]
        Some(Commands::Server { .. }) => {
            eprintln!("Error: Server functionality requires the 'api' feature");
            eprintln!("Please rebuild with: cargo build --features api");
            std::process::exit(1);
        }
        None => {
            // No subcommand - run the REPL
            let mut cli_state = match CliState::initialize_with_path(cli.config) {
                Ok(cli) => cli,
                Err(e) => {
                    let error_chain = format!("{:#}", e);
                    if error_chain.contains("Could not set lock")
                        || error_chain.contains("Conflicting lock")
                    {
                        eprintln!("Error: Another instance of spec-ai is already running.");
                        eprintln!();
                        eprintln!("Only one instance can access the database at a time.");
                        eprintln!("Please close the other instance or wait for it to finish.");
                        std::process::exit(1);
                    }
                    return Err(e);
                }
            };

            // Initialize logging based on config
            let log_level = cli_state.config.logging.level.to_uppercase();
            let default_directive = format!("spec_ai={}", log_level.to_lowercase());
            let env_override = std::env::var("RUST_LOG").unwrap_or_default();
            let combined_filter = if env_override.trim().is_empty() {
                default_directive.clone()
            } else if env_override.contains("spec_ai") {
                env_override
            } else {
                format!("{},{}", env_override, default_directive)
            };

            tracing_subscriber::fmt()
                .with_env_filter(combined_filter)
                .with_target(true)
                .init();

            // Run REPL
            cli_state.run_repl().await?;
            Ok(())
        }
    }
}
