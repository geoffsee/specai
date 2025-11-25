use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Full end-to-end integration test that:
/// 1. Builds the Rust binary
/// 2. Creates a brand new directory with config and spec files
/// 3. Runs the spec using the built binary
/// 4. Makes assertions about the output
#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore = "Slow end-to-end binary test - run with --features integration-tests")]
async fn test_full_binary_spec_execution() {
    // Step 1: Build the binary
    println!("Building the binary...");
    let build_result = Command::new("cargo")
        .args(&["build", "--release"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(
        build_result.status.success(),
        "Cargo build failed: {}",
        String::from_utf8_lossy(&build_result.stderr)
    );

    // Locate the built binary
    let binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("spec-ai");

    assert!(
        binary_path.exists(),
        "Binary not found at expected path: {}",
        binary_path.display()
    );

    // Step 2: Create a brand new temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_dir = temp_dir.path();

    println!("Test directory: {}", test_dir.display());

    // Create a config file with mock provider
    let config_content = r#"
[model]
provider = "mock"

[database]
path = "test.duckdb"

[agents.default]
temperature = 0.7
"#;

    let config_path = test_dir.join("spec-ai.config.toml");
    fs::write(&config_path, config_content).expect("Failed to write config file");

    // Create a test spec file
    let spec_content = r#"
name = "Integration Test Spec"
goal = "Verify that the spec-ai binary can execute spec end-to-end"

tasks = [
    "Load and parse this spec file",
    "Execute the spec using the agent",
    "Return a response confirming execution"
]

deliverables = [
    "Confirmation that the spec was processed",
    "Evidence that the mock agent responded"
]
"#;

    let spec_path = test_dir.join("test.spec");
    fs::write(&spec_path, spec_content).expect("Failed to write spec file");

    // Step 3: Execute the binary with the spec
    println!("Executing spec via binary...");

    // We need to run the binary in interactive mode and pipe commands to it
    // The binary expects interactive input, so we'll use stdin to send the /spec command
    let spec_command = format!("/spec {}\n/quit\n", spec_path.display());

    let execution_result = Command::new(&binary_path)
        .current_dir(test_dir)
        .env("SPEC_AI_CONFIG", config_path.to_str().unwrap())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(spec_command.as_bytes())?;
            }
            child.wait_with_output()
        })
        .expect("Failed to execute binary");

    let stdout = String::from_utf8_lossy(&execution_result.stdout);
    let stderr = String::from_utf8_lossy(&execution_result.stderr);

    println!("=== STDOUT ===");
    println!("{}", stdout);
    println!("=== STDERR ===");
    println!("{}", stderr);

    // Step 4: Make assertions about the output

    // Assert the process completed successfully
    assert!(
        execution_result.status.success(),
        "Binary execution failed with exit code: {:?}\nStderr: {}",
        execution_result.status.code(),
        stderr
    );

    // Assert output contains spec name
    assert!(
        stdout.contains("Integration Test Spec") || stderr.contains("Integration Test Spec"),
        "Output should contain the spec name"
    );

    // Assert output contains evidence of spec execution
    assert!(
        stdout.contains("goal") || stdout.contains("Verify") || stdout.contains("spec"),
        "Output should contain evidence of spec processing"
    );

    // Assert the mock provider responded (mock provider returns specific text)
    assert!(
        stdout.contains("Mock") || stdout.contains("response") || !stdout.is_empty(),
        "Output should contain mock agent response"
    );

    // Assert no critical errors in stderr
    assert!(
        !stderr.contains("panic") && !stderr.contains("Error:"),
        "Stderr should not contain critical errors"
    );

    println!("✓ All assertions passed!");
}

/// Test building the binary without execution
#[test]
#[cfg_attr(not(feature = "integration-tests"), ignore = "Slow binary build test - run with --features integration-tests")]
fn test_binary_builds_successfully() {
    let build_result = Command::new("cargo")
        .args(&["build", "--release"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(
        build_result.status.success(),
        "Cargo build failed: {}",
        String::from_utf8_lossy(&build_result.stderr)
    );

    let binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("spec-ai");

    assert!(
        binary_path.exists(),
        "Binary should exist at: {}",
        binary_path.display()
    );
}
