use anyhow::Result;
use spec_ai::cli::CliState;
use std::env;
use std::path::PathBuf;

fn print_usage() {
    eprintln!("Usage: spec-ai [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -c, --config <PATH>    Path to config file (default: ./spec-ai.config.toml or ~/.spec-ai/spec-ai.config.toml)");
    eprintln!("  -h, --help             Print this help message");
}

fn parse_args() -> Result<Option<PathBuf>> {
    let args: Vec<String> = env::args().collect();

    for i in 1..args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-c" | "--config" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("--config requires a path argument");
                }
                return Ok(Some(PathBuf::from(&args[i + 1])));
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unknown argument: {}", arg);
                print_usage();
                std::process::exit(1);
            }
            _ => {
                // Skip non-flag arguments (like the value after --config)
                continue;
            }
        }
    }

    Ok(None)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let config_path = parse_args()?;

    // Initialize CLI state (loads config and persistence)
    let mut cli = match CliState::initialize_with_path(config_path) {
        Ok(cli) => cli,
        Err(e) => {
            // Check if this is a database lock error (another instance running)
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
            // For other errors, return the original error
            return Err(e);
        }
    };

    // Initialize logging based on config
    let log_level = cli.config.logging.level.to_uppercase();
    let default_directive = format!("spec_ai={}", log_level.to_lowercase());
    let env_override = env::var("RUST_LOG").unwrap_or_default();
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
    cli.run_repl().await?;
    Ok(())
}
