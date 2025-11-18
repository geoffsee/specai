use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use extractous::Extractor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Arguments accepted by the file_extract tool
#[derive(Debug, Deserialize)]
struct FileExtractArgs {
    path: String,
    #[serde(default)]
    include_metadata: bool,
    #[serde(default)]
    xml_output: bool,
    #[serde(default)]
    max_chars: Option<i32>,
}

/// Output payload returned by the file_extract tool
#[derive(Debug, Serialize)]
struct FileExtractOutput {
    path: String,
    content: String,
    metadata: Option<HashMap<String, Vec<String>>>,
}

/// Tool that uses Extractous to read arbitrary files and return textual content
pub struct FileExtractTool;

impl FileExtractTool {
    pub fn new() -> Self {
        Self
    }

    fn normalize_path(&self, input: &str) -> Result<PathBuf> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("file_extract requires a valid path"));
        }
        Ok(PathBuf::from(trimmed))
    }
}

#[async_trait]
impl Tool for FileExtractTool {
    fn name(&self) -> &str {
        "file_extract"
    }

    fn description(&self) -> &str {
        "Extracts text metadata from files regardless of format (PDF, Office, HTML, etc.)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative or absolute path to the file that should be extracted"
                },
                "include_metadata": {
                    "type": "boolean",
                    "description": "Include metadata returned by Extractous",
                    "default": false
                },
                "xml_output": {
                    "type": "boolean",
                    "description": "Request XML formatted result instead of plain text",
                    "default": false
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Limit the number of characters returned (must be > 0 if provided)",
                    "minimum": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: FileExtractArgs =
            serde_json::from_value(args).context("Failed to parse file_extract arguments")?;

        let path = self.normalize_path(&args.path)?;
        let metadata =
            fs::metadata(&path).with_context(|| format!("File not found: {}", path.display()))?;

        if !metadata.is_file() {
            return Ok(ToolResult::failure(format!(
                "{} is not a regular file",
                path.display()
            )));
        }

        let mut extractor = Extractor::new();
        if let Some(max_chars) = args.max_chars {
            if max_chars <= 0 {
                return Ok(ToolResult::failure(
                    "max_chars must be greater than zero".to_string(),
                ));
            }
            extractor = extractor.set_extract_string_max_length(max_chars);
        }

        if args.xml_output {
            extractor = extractor.set_xml_output(true);
        }

        let display_path = path.to_string_lossy().into_owned();
        let (content, extracted_metadata) = extractor
            .extract_file_to_string(&display_path)
            .map_err(|err| anyhow!("Failed to extract {}: {}", display_path, err))?;

        let metadata = if args.include_metadata {
            Some(extracted_metadata)
        } else {
            None
        };

        let output = FileExtractOutput {
            path: display_path,
            content,
            metadata,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&output).context("Failed to serialize file_extract output")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn name_and_description() {
        let tool = FileExtractTool::new();
        assert_eq!(tool.name(), "file_extract");
        assert!(tool.description().contains("Extracts text"));
    }

    #[tokio::test]
    async fn parameters_require_path() {
        let tool = FileExtractTool::new();
        let params = tool.parameters();
        let required = params["required"].as_array().unwrap();
        assert!(required.iter().any(|value| value == "path"));
    }

    #[tokio::test]
    async fn invalid_max_chars_returns_failure() {
        let tool = FileExtractTool::new();
        let tmp = NamedTempFile::new().unwrap();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "max_chars": 0
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.unwrap(), "max_chars must be greater than zero");
    }
}
