use crate::tools::{Tool, ToolResult};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time;
use tracing::info;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_COMMAND_LENGTH: usize = 4096;
const MAX_OUTPUT_CHARS: usize = 16_384;
const DENYLIST: &[&str] = &[
    "sudo", "rm -rf", "reboot", "shutdown", ":(){", "mkfs", "dd if=", ">|",
];

#[derive(Debug, Deserialize)]
struct BashArgs {
    command: String,
    timeout_ms: Option<u64>,
    env: Option<HashMap<String, String>>,
    working_dir: Option<String>,
}

#[derive(Debug, Serialize)]
struct CommandOutput {
    command: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
    duration_ms: u128,
}

fn truncate_output(input: &[u8]) -> String {
    let text = String::from_utf8_lossy(input);
    if text.len() <= MAX_OUTPUT_CHARS {
        text.to_string()
    } else {
        let mut truncated = text.chars().take(MAX_OUTPUT_CHARS).collect::<String>();
        truncated.push_str("...<truncated>");
        truncated
    }
}

fn validate_command(command: &str) -> Result<()> {
    if command.trim().is_empty() {
        return Err(anyhow!("Command cannot be empty"));
    }

    if command.len() > MAX_COMMAND_LENGTH {
        return Err(anyhow!(
            "Command exceeds maximum allowed length ({})",
            MAX_COMMAND_LENGTH
        ));
    }

    for forbidden in DENYLIST {
        if command.contains(forbidden) {
            return Err(anyhow!(format!(
                "Command contains forbidden pattern '{}'",
                forbidden
            )));
        }
    }

    Ok(())
}

async fn run_bash_command(args: &BashArgs, shell_path: &Path) -> Result<CommandOutput> {
    if !shell_path.exists() {
        return Err(anyhow!(format!(
            "Shell path {} does not exist",
            shell_path.display()
        )));
    }

    validate_command(&args.command)?;

    info!(
        target: "spec_ai::tools::bash",
        command = %args.command,
        shell = %shell_path.display(),
        "Executing bash command"
    );

    let timeout = args
        .timeout_ms
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_TIMEOUT);

    let mut command = Command::new(shell_path);
    command.arg("-c").arg(&args.command);
    command.kill_on_drop(true);

    if let Some(dir) = &args.working_dir {
        command.current_dir(dir);
    }

    if let Some(env) = &args.env {
        for (key, value) in env {
            command.env(key, value);
        }
    }

    let start = Instant::now();
    let output = match time::timeout(timeout, command.output()).await {
        Ok(result) => result.context("Failed to execute bash command")?,
        Err(_) => {
            return Err(anyhow!(format!(
                "Command timed out after {} ms",
                timeout.as_millis()
            )));
        }
    };

    let duration = start.elapsed().as_millis();
    let stdout = truncate_output(&output.stdout);
    let stderr = truncate_output(&output.stderr);
    let exit_code = output.status.code().unwrap_or_default();

    info!(
        target: "spec_ai::tools::bash",
        command = %args.command,
        exit_code,
        duration_ms = duration,
        "Bash command finished"
    );

    Ok(CommandOutput {
        command: args.command.clone(),
        stdout,
        stderr,
        exit_code,
        duration_ms: duration,
    })
}

/// Tool that executes bash commands with safety checks
pub struct BashTool {
    shell_path: String,
}

impl BashTool {
    pub fn new() -> Self {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        Self { shell_path: shell }
    }

    pub fn with_shell(mut self, path: impl Into<String>) -> Self {
        self.shell_path = path.into();
        self
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Executes bash commands with timeout, output capture, and denylisted operations"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Bash command to run"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Maximum execution time in milliseconds",
                    "minimum": 1000
                },
                "env": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "Environment variables for the process"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: BashArgs =
            serde_json::from_value(args).context("Failed to parse bash arguments")?;
        let shell_path = Path::new(&self.shell_path);

        let output = run_bash_command(&args, shell_path).await?;

        if output.exit_code == 0 {
            Ok(ToolResult::success(
                serde_json::to_string(&output).context("Failed to serialize bash output")?,
            ))
        } else {
            Ok(ToolResult::failure(
                serde_json::to_string(&output).context("Failed to serialize bash output")?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bash_success() {
        let tool = BashTool::new();
        let args = serde_json::json!({ "command": "echo test" });
        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let payload: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(payload["stdout"].as_str().unwrap().contains("test"));
    }

    #[tokio::test]
    async fn test_bash_failure() {
        let tool = BashTool::new();
        let args = serde_json::json!({ "command": "exit 5" });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_bash_timeout() {
        let tool = BashTool::new();
        let args = serde_json::json!({
            "command": "sleep 5",
            "timeout_ms": 1000
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }
}
