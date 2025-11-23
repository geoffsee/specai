use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

const DEFAULT_MAX_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
enum FileReadFormat {
    #[default]
    Text,
    Base64,
}

#[derive(Debug, Deserialize)]
struct FileReadArgs {
    path: String,
    #[serde(default)]
    include_metadata: bool,
    #[serde(default)]
    format: FileReadFormat,
    max_bytes: Option<usize>,
    /// Read only the first N lines
    head: Option<usize>,
    /// Read only the last N lines
    tail: Option<usize>,
    /// Skip the first N lines (used with limit)
    offset: Option<usize>,
    /// Read at most N lines (used with offset)
    limit: Option<usize>,
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
                },
                "head": {
                    "type": "integer",
                    "description": "Read only the first N lines (text format only)",
                    "minimum": 1
                },
                "tail": {
                    "type": "integer",
                    "description": "Read only the last N lines (text format only)",
                    "minimum": 1
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip the first N lines (text format only, use with limit)",
                    "minimum": 0
                },
                "limit": {
                    "type": "integer",
                    "description": "Read at most N lines (text format only, use with offset)",
                    "minimum": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: FileReadArgs =
            serde_json::from_value(args).context("Failed to parse file_read arguments")?;

        let path = self.normalize_path(&args.path)?;
        let file_metadata =
            fs::metadata(&path).with_context(|| format!("File not found: {}", path.display()))?;

        if !file_metadata.is_file() {
            return Ok(ToolResult::failure(format!(
                "{} is not a regular file",
                path.display()
            )));
        }

        // Check if line-based operations are requested
        let use_line_mode = args.head.is_some()
            || args.tail.is_some()
            || args.offset.is_some()
            || args.limit.is_some();

        // Validate that line-based operations are only used with text format
        if use_line_mode && !matches!(args.format, FileReadFormat::Text) {
            return Ok(ToolResult::failure(
                "Line-based operations (head, tail, offset, limit) are only supported with text format".to_string()
            ));
        }

        // For line-based operations, we can bypass the byte limit check
        // as we'll only read specific lines
        let limit = self.ensure_within_limit(args.max_bytes);

        if !use_line_mode && file_metadata.len() as usize > limit {
            // Estimate lines for better error message
            let estimated_lines = (file_metadata.len() / 80).max(1); // Assume ~80 chars per line
            return Ok(ToolResult::failure(format!(
                "File exceeds maximum allowed size of {} bytes (file is {} bytes). \
                 Consider using line-based reading:\n\
                 - Use 'head: N' to read first N lines\n\
                 - Use 'tail: N' to read last N lines\n\
                 - Use 'offset: M' with 'limit: N' to read N lines starting from line M\n\
                 Estimated lines in file: ~{}",
                limit,
                file_metadata.len(),
                estimated_lines
            )));
        }

        let (encoding, content, actual_bytes) = if use_line_mode {
            // Handle line-based reading
            let file = fs::File::open(&path)
                .with_context(|| format!("Failed to open file {}", path.display()))?;
            let reader = BufReader::new(file);

            let processed_content = if let Some(n) = args.head {
                // Read first N lines
                reader
                    .lines()
                    .take(n)
                    .collect::<Result<Vec<_>, _>>()
                    .context("Failed to read lines")?
                    .join("\n")
            } else if let Some(n) = args.tail {
                // Read last N lines
                let all_lines: Vec<String> = reader
                    .lines()
                    .collect::<Result<Vec<_>, _>>()
                    .context("Failed to read lines")?;
                let start = all_lines.len().saturating_sub(n);
                all_lines[start..].join("\n")
            } else {
                // Handle offset and limit
                let offset = args.offset.unwrap_or(0);
                let limit = args.limit.unwrap_or(usize::MAX);

                reader
                    .lines()
                    .skip(offset)
                    .take(limit)
                    .collect::<Result<Vec<_>, _>>()
                    .context("Failed to read lines")?
                    .join("\n")
            };

            let bytes = processed_content.as_bytes().len();
            ("utf-8", processed_content, bytes)
        } else {
            // Read entire file (existing behavior)
            let bytes = fs::read(&path)
                .with_context(|| format!("Failed to read file {}", path.display()))?;
            let actual_bytes = bytes.len();

            match args.format {
                FileReadFormat::Text => {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    ("utf-8", text, actual_bytes)
                }
                FileReadFormat::Base64 => (
                    "base64",
                    general_purpose::STANDARD.encode(&bytes),
                    actual_bytes,
                ),
            }
        };

        let metadata = if args.include_metadata {
            Some(Self::serialize_metadata(&file_metadata))
        } else {
            None
        };

        let output = FileReadOutput {
            path: path.to_string_lossy().into_owned(),
            encoding,
            bytes: actual_bytes,
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
        assert!(result.error.is_some());
        assert!(result
            .error
            .unwrap()
            .contains("Consider using line-based reading"));
    }

    #[tokio::test]
    async fn test_file_read_head() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "line1").unwrap();
        writeln!(tmp, "line2").unwrap();
        writeln!(tmp, "line3").unwrap();
        writeln!(tmp, "line4").unwrap();
        writeln!(tmp, "line5").unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "head": 3
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let value: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        let content = value["content"].as_str().unwrap();
        assert_eq!(content, "line1\nline2\nline3");
    }

    #[tokio::test]
    async fn test_file_read_tail() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "line1").unwrap();
        writeln!(tmp, "line2").unwrap();
        writeln!(tmp, "line3").unwrap();
        writeln!(tmp, "line4").unwrap();
        writeln!(tmp, "line5").unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "tail": 2
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let value: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        let content = value["content"].as_str().unwrap();
        assert_eq!(content, "line4\nline5");
    }

    #[tokio::test]
    async fn test_file_read_offset_limit() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "line1").unwrap();
        writeln!(tmp, "line2").unwrap();
        writeln!(tmp, "line3").unwrap();
        writeln!(tmp, "line4").unwrap();
        writeln!(tmp, "line5").unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "offset": 1,
            "limit": 3
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let value: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        let content = value["content"].as_str().unwrap();
        assert_eq!(content, "line2\nline3\nline4");
    }

    #[tokio::test]
    async fn test_file_read_line_mode_with_base64_fails() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "test").unwrap();

        let tool = FileReadTool::new();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "format": "base64",
            "head": 10
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result
            .error
            .unwrap()
            .contains("only supported with text format"));
    }
}
