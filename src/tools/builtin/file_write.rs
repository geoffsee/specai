use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const DEFAULT_MAX_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WriteMode {
    Overwrite,
    Append,
}

impl Default for WriteMode {
    fn default() -> Self {
        WriteMode::Overwrite
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ContentEncoding {
    Text,
    Base64,
}

impl Default for ContentEncoding {
    fn default() -> Self {
        ContentEncoding::Text
    }
}

#[derive(Debug, Deserialize)]
struct FileWriteArgs {
    path: String,
    content: String,
    #[serde(default)]
    mode: WriteMode,
    #[serde(default)]
    encoding: ContentEncoding,
    #[serde(default = "FileWriteArgs::default_create_dirs")]
    create_dirs: bool,
}

impl FileWriteArgs {
    fn default_create_dirs() -> bool {
        true
    }
}

#[derive(Debug, serde::Serialize)]
struct FileWriteOutput {
    path: String,
    mode: &'static str,
    bytes_written: usize,
    existed: bool,
    message: String,
}

/// Tool for writing files to disk with safeguards
pub struct FileWriteTool {
    max_bytes: usize,
}

impl FileWriteTool {
    pub fn new() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        if path.trim().is_empty() {
            return Err(anyhow!("file_write requires a valid path"));
        }
        Ok(PathBuf::from(path))
    }

    fn ensure_parent(&self, path: &Path, create_dirs: bool) -> Result<()> {
        if let Some(parent) = path.parent() {
            if parent.exists() {
                return Ok(());
            }
            if create_dirs {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directories for {}", path.display())
                })?;
            } else {
                return Err(anyhow!(
                    "Parent directory does not exist for {} (set create_dirs=true to create it)",
                    path.display()
                ));
            }
            return Ok(());
        }
        Ok(())
    }

    fn decode_content(&self, args: &FileWriteArgs) -> Result<Vec<u8>> {
        let bytes = match args.encoding {
            ContentEncoding::Text => args.content.clone().into_bytes(),
            ContentEncoding::Base64 => general_purpose::STANDARD
                .decode(&args.content)
                .context("Failed to decode base64 content for file_write")?,
        };

        if bytes.len() > self.max_bytes {
            return Err(anyhow!(
                "Content exceeds maximum allowed size of {} bytes",
                self.max_bytes
            ));
        }

        Ok(bytes)
    }

    fn write_overwrite(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(anyhow!(
                    "Parent directory {} must exist before writing",
                    parent.display()
                ));
            }
        }

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = NamedTempFile::new_in(parent)
            .with_context(|| format!("Failed to create temporary file near {}", path.display()))?;
        tmp.write_all(bytes)
            .with_context(|| format!("Failed to write temporary file for {}", path.display()))?;
        tmp.flush()?;
        tmp.as_file().sync_all().ok();

        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("Failed to remove existing file {}", path.display()))?;
        }

        let tmp_path = tmp.into_temp_path();
        tmp_path
            .persist(path)
            .map_err(|err| anyhow!("Failed to persist file {}: {}", path.display(), err))?;
        Ok(())
    }

    fn write_append(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open {} for appending", path.display()))?;

        file.write_all(bytes)
            .with_context(|| format!("Failed to append to {}", path.display()))?;
        file.flush().ok();
        file.sync_all().ok();
        Ok(())
    }
}

impl Default for FileWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Writes text or base64-decoded content to files with optional append support"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative or absolute file path to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "mode": {
                    "type": "string",
                    "enum": ["overwrite", "append"],
                    "description": "Overwrite (default) or append to existing files",
                    "default": "overwrite"
                },
                "encoding": {
                    "type": "string",
                    "enum": ["text", "base64"],
                    "description": "Encoding for the provided content",
                    "default": "text"
                },
                "create_dirs": {
                    "type": "boolean",
                    "description": "Create parent directories when needed",
                    "default": true
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: FileWriteArgs =
            serde_json::from_value(args).context("Failed to parse file_write arguments")?;

        let path = self.resolve_path(&args.path)?;
        self.ensure_parent(&path, args.create_dirs)?;
        let bytes = self.decode_content(&args)?;

        let existed = path.exists();

        match args.mode {
            WriteMode::Overwrite => self.write_overwrite(&path, &bytes)?,
            WriteMode::Append => self.write_append(&path, &bytes)?,
        };

        let message = match args.mode {
            WriteMode::Overwrite if existed => "File overwritten",
            WriteMode::Overwrite => "File created",
            WriteMode::Append if existed => "Content appended to existing file",
            WriteMode::Append => "Content appended to new file",
        }
        .to_string();

        let output = FileWriteOutput {
            path: path.to_string_lossy().into_owned(),
            mode: match args.mode {
                WriteMode::Overwrite => "overwrite",
                WriteMode::Append => "append",
            },
            bytes_written: bytes.len(),
            existed,
            message,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&output).context("Failed to serialize file_write output")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_write_overwrite() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("file.txt");
        let tool = FileWriteTool::new();

        let args = serde_json::json!({
            "path": path.to_string_lossy(),
            "content": "hello world",
            "create_dirs": true
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_file_write_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("append.txt");
        fs::write(&path, "line1\n").unwrap();
        let tool = FileWriteTool::new();

        let args = serde_json::json!({
            "path": path.to_string_lossy(),
            "content": "line2\n",
            "mode": "append"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("line2"));
    }

    #[tokio::test]
    async fn test_file_write_base64() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("binary.bin");
        let tool = FileWriteTool::new();

        let args = serde_json::json!({
            "path": path.to_string_lossy(),
            "content": "AQID",
            "encoding": "base64"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes, vec![1, 2, 3]);
    }
}
