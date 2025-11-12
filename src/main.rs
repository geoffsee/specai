use anyhow::Result;
use spec_ai::cli::CliState;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize CLI state (loads config and persistence)
    let mut cli = CliState::initialize()?;

    // Initialize logging based on config
    let log_level = cli.config.logging.level.to_uppercase();
    let env_filter = format!("specai={}", log_level.to_lowercase());
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    // Run REPL
    cli.run_repl().await?;
    Ok(())
}
