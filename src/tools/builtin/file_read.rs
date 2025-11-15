use crate::tools::{Tool, ToolResult};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const DEFAULT_MAX_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileReadFormat {
    Text,
    Base64,
}

impl Default for FileReadFormat {
    fn default() -> Self {
        FileReadFormat::Text
    }
}

#[derive(Debug, Deserialize)]
struct FileReadArgs {
    path: String,
    #[serde(default)]
    include_metadata: bool,
    #[serde(default)]
    format: FileReadFormat,
    max_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FileMetadata {
    size_bytes: u64,
    modified: Option<String>,
    created: Option<String>,
}

#[derive(Debug, Serialize)]
struct FileReadOutput {
    path: String,
    encoding: &'static str,
    bytes: usize,
    content: String,
    metadata: Option<FileMetadata>,
}

/// Tool for safely reading files from disk
pub struct FileReadTool {
    max_bytes: usize,
}

impl FileReadTool {
    pub fn new() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    fn ensure_within_limit(&self, requested: Option<usize>) -> usize {
        requested
            .map(|req| req.min(self.max_bytes))
            .unwrap_or(self.max_bytes)
    }

    fn normalize_path(&self, input: &str) -> Result<PathBuf> {
        if input.trim().is_empty() {
            return Err(anyhow!("file_read requires a valid path"));
        }

        Ok(PathBuf::from(input))
    }

    fn serialize_metadata(metadata: &fs::Metadata) -> FileMetadata {
        let modified = metadata.modified().ok().map(|time| {
            let datetime: DateTime<Utc> = time.into();
            datetime.to_rfc3339()
        });
        let created = metadata.created().ok().map(|time| {
            let datetime: DateTime<Utc> = time.into();
            datetime.to_rfc3339()
        });

        FileMetadata {
            size_bytes: metadata.len(),
            modified,
            created,
        }
    }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Reads files from disk with optional metadata and size limits"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative or absolute file path to read"
                },
                "include_metadata": {
                    "type": "boolean",
                    "description": "Return file metadata (size, timestamps)",
                    "default": false
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "base64"],
                    "description": "Return format for file contents",
                    "default": "text"
                },
                "max_bytes": {
                    "type": "integer",
                    "description": "Override default read limit (bytes)",
                    "minimum": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: FileReadArgs =
            serde_json::from_value(args).context("Failed to parse file_read arguments")?;

        let limit = self.ensure_within_limit(args.max_bytes);
        let path = self.normalize_path(&args.path)?;
        let metadata =
            fs::metadata(&path).with_context(|| format!("File not found: {}", path.display()))?;

        if !metadata.is_file() {
            return Ok(ToolResult::failure(format!(
                "{} is not a regular file",
                path.display()
            )));
        }

        if metadata.len() as usize > limit {
            return Ok(ToolResult::failure(format!(
                "File exceeds maximum allowed size of {} bytes",
                limit
            )));
        }

        let bytes =
            fs::read(&path).with_context(|| format!("Failed to read file {}", path.display()))?;
        let (encoding, content) = match args.format {
            FileReadFormat::Text => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                ("utf-8", text)
            }
            FileReadFormat::Base64 => ("base64", general_purpose::STANDARD.encode(&bytes)),
        };

        let metadata = if args.include_metadata {
            Some(Self::serialize_metadata(&metadata))
        } else {
            None
        };

        let output = FileReadOutput {
            path: path.to_string_lossy().into_owned(),
            encoding,
            bytes: bytes.len(),
            content,
            metadata,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&output).context("Failed to serialize file_read output")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_file_read_text() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "hello world").unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "include_metadata": true
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let value: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(value["encoding"], "utf-8");
        assert!(value["metadata"]["size_bytes"].is_number());
    }

    #[tokio::test]
    async fn test_file_read_binary_base64() {
        let tmp = NamedTempFile::new().unwrap();
        fs::write(tmp.path(), vec![0, 159, 146, 150]).unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "format": "base64"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let value: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(value["encoding"], "base64");
        assert!(!value["content"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_file_read_too_large() {
        let tmp = NamedTempFile::new().unwrap();
        fs::write(tmp.path(), vec![1; DEFAULT_MAX_BYTES + 1]).unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }
}
