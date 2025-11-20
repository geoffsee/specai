use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time;
use tracing::info;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_CHARS: usize = 16_384;

#[derive(Debug, Deserialize)]
struct ShellArgs {
    command: String,
    shell: Option<String>,
    shell_args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    working_dir: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ShellOutput {
    command: String,
    shell: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
    duration_ms: u128,
}

fn default_shell() -> (String, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        ("cmd.exe".to_string(), vec!["/C".to_string()])
    }
    #[cfg(not(target_os = "windows"))]
    {
        (
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            vec!["-c".to_string()],
        )
    }
}

fn truncate_output(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    if text.len() <= MAX_OUTPUT_CHARS {
        text.to_string()
    } else {
        let mut truncated = text.chars().take(MAX_OUTPUT_CHARS).collect::<String>();
        truncated.push_str("...<truncated>");
        truncated
    }
}

async fn execute_shell_command(args: &ShellArgs) -> Result<ShellOutput> {
    if args.command.trim().is_empty() {
        return Err(anyhow!("shell command cannot be empty"));
    }

    let (default_shell, default_args) = default_shell();
    let shell_binary = args.shell.clone().unwrap_or(default_shell);
    let mut shell_args = args.shell_args.clone().unwrap_or(default_args);
    if shell_args.is_empty() {
        shell_args = if cfg!(windows) {
            vec!["/C".into()]
        } else {
            vec!["-c".into()]
        };
    }

    let shell_path = PathBuf::from(&shell_binary);
    if (shell_path.is_absolute() || shell_binary.contains(std::path::MAIN_SEPARATOR))
        && !shell_path.exists()
    {
        return Err(anyhow!(
            "Shell binary {} does not exist",
            shell_path.display()
        ));
    }

    let timeout = args
        .timeout_ms
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_TIMEOUT);

    let mut command = Command::new(&shell_binary);
    for arg in &shell_args {
        command.arg(arg);
    }
    command.arg(&args.command);
    command.kill_on_drop(true);

    if let Some(dir) = &args.working_dir {
        command.current_dir(dir);
    }

    if let Some(ref env) = args.env {
        for (key, value) in env {
            command.env(key, value);
        }
    }

    info!(
        target: "spec_ai::tools::shell",
        command = %args.command,
        shell = %shell_binary,
        "Executing shell command"
    );

    let start = Instant::now();
    let output = match time::timeout(timeout, command.output()).await {
        Ok(result) => result.context("Failed to execute shell command")?,
        Err(_) => {
            return Err(anyhow!(format!(
                "Shell command timed out after {} ms",
                timeout.as_millis()
            )));
        }
    };

    let duration = start.elapsed().as_millis();
    let stdout = truncate_output(&output.stdout);
    let stderr = truncate_output(&output.stderr);
    let exit_code = output.status.code().unwrap_or_default();

    info!(
        target: "spec_ai::tools::shell",
        command = %args.command,
        shell = %shell_binary,
        exit_code,
        duration_ms = duration,
        "Shell command finished"
    );

    Ok(ShellOutput {
        command: args.command.clone(),
        shell: shell_binary,
        stdout,
        stderr,
        exit_code,
        duration_ms: duration,
    })
}

/// Cross-platform shell execution tool
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Executes commands using the system shell with cross-platform support"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command to execute"
                },
                "shell": {
                    "type": "string",
                    "description": "Shell binary to use (defaults to system shell)"
                },
                "shell_args": {
                    "type": "array",
                    "description": "Custom shell arguments (default -c or /C)",
                    "items": {"type": "string"}
                },
                "env": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "Environment variables for the shell"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Maximum execution time in milliseconds"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: ShellArgs =
            serde_json::from_value(args).context("Failed to parse shell arguments")?;

        let output = execute_shell_command(&args).await?;

        if output.exit_code == 0 {
            Ok(ToolResult::success(
                serde_json::to_string(&output).context("Failed to serialize shell output")?,
            ))
        } else {
            Ok(ToolResult::failure(
                serde_json::to_string(&output).context("Failed to serialize shell output")?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shell_default() {
        let tool = ShellTool::new();
        let args = serde_json::json!({
            "command": "echo shell-tool"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let payload: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(payload["stdout"].as_str().unwrap().contains("shell-tool"));
    }

    #[tokio::test]
    async fn test_shell_nonzero_exit() {
        let tool = ShellTool::new();
        let args = serde_json::json!({ "command": "exit 42" });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }
}
