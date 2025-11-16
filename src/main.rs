use anyhow::Result;
use spec_ai::cli::CliState;
use std::env;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize CLI state (loads config and persistence)
    let mut cli = CliState::initialize()?;

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
