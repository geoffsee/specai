use anyhow::Result;

#[cfg(feature = "cli")]
fn main() -> Result<()> {
    spec_ai_cli::run()
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("Error: CLI functionality requires the 'cli' feature");
    eprintln!("Please rebuild with: cargo build --features cli");
    std::process::exit(1);
}
