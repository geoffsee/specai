use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use spec_ai::cli::CliState;
use spec_ai::spec::AgentSpec;
use std::path::PathBuf;
use walkdir::WalkDir;

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
