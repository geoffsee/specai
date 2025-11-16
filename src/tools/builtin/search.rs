use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const DEFAULT_MAX_RESULTS: usize = 20;
const HARD_MAX_RESULTS: usize = 100;
const DEFAULT_CONTEXT_LINES: usize = 2;
const DEFAULT_MAX_FILE_BYTES: usize = 512 * 1024; // 512 KiB

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    root: Option<String>,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case_sensitive: bool,
    file_extensions: Option<Vec<String>>,
    max_results: Option<usize>,
    context_lines: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SearchResultEntry {
    path: String,
    line: usize,
    snippet: String,
    score: f32,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    query: String,
    results: Vec<SearchResultEntry>,
}

/// Tool that searches local files for literal or regex matches
pub struct SearchTool {
    root: PathBuf,
    max_file_bytes: usize,
}

impl SearchTool {
    pub fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            root,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        }
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    pub fn with_max_file_bytes(mut self, max_file_bytes: usize) -> Self {
        self.max_file_bytes = max_file_bytes;
        self
    }

    fn resolve_root(&self, override_root: &Option<String>) -> PathBuf {
        override_root
            .as_ref()
            .map(|r| PathBuf::from(r))
            .unwrap_or_else(|| self.root.clone())
    }

    fn filter_extension(&self, path: &Path, allowed: &Option<Vec<String>>) -> bool {
        match allowed {
            None => true,
            Some(list) if list.is_empty() => true,
            Some(list) => {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext = ext.trim_start_matches('.');
                    list.iter().any(|allowed_ext| {
                        allowed_ext
                            .trim_start_matches('.')
                            .eq_ignore_ascii_case(ext)
                    })
                } else {
                    false
                }
            }
        }
    }

    fn literal_match(
        &self,
        query: &str,
        line: &str,
        case_sensitive: bool,
    ) -> Option<(usize, usize)> {
        if case_sensitive {
            line.find(query).map(|start| (start, start + query.len()))
        } else {
            let lower_line = line.to_lowercase();
            let lower_query = query.to_lowercase();
            lower_line
                .find(&lower_query)
                .map(|start| (start, start + lower_query.len()))
        }
    }

    fn build_snippet(lines: &[String], idx: usize, context_lines: usize) -> String {
        let start = idx.saturating_sub(context_lines);
        let end = (idx + context_lines).min(lines.len().saturating_sub(1));
        lines[start..=end].join("\n")
    }
}

impl Default for SearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Searches local files using literal or regex queries"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query string or regex pattern"
                },
                "root": {
                    "type": "string",
                    "description": "Directory to search (defaults to current workspace)"
                },
                "regex": {
                    "type": "boolean",
                    "description": "Interpret query as regular expression",
                    "default": false
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Case sensitive search (default false for literal matches)",
                    "default": false
                },
                "file_extensions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Limit search to specific file extensions"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (max 100)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of lines of context around matches",
                    "default": 2
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: SearchArgs =
            serde_json::from_value(args).context("Failed to parse search arguments")?;

        if args.query.trim().is_empty() {
            return Err(anyhow!("search query cannot be empty"));
        }

        let root = self.resolve_root(&args.root);
        if !root.exists() {
            return Err(anyhow!("Search root {} does not exist", root.display()));
        }

        let max_results = args
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, HARD_MAX_RESULTS);
        let context_lines = args.context_lines.unwrap_or(DEFAULT_CONTEXT_LINES);

        let regex = if args.regex {
            Some(
                RegexBuilder::new(&args.query)
                    .case_insensitive(!args.case_sensitive)
                    .build()
                    .context("Invalid regular expression for search")?,
            )
        } else {
            None
        };

        let mut results = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if results.len() >= max_results {
                break;
            }

            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }

            if !self.filter_extension(path, &args.file_extensions) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };

            if metadata.len() as usize > self.max_file_bytes {
                continue;
            }

            let data = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };

            let content = match String::from_utf8(data) {
                Ok(text) => text,
                Err(_) => continue,
            };

            let lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();

            for (idx, line) in lines.iter().enumerate() {
                if results.len() >= max_results {
                    break;
                }

                let maybe_span = if let Some(regex) = &regex {
                    regex.find(line).map(|m| (m.start(), m.end()))
                } else {
                    self.literal_match(&args.query, line, args.case_sensitive)
                };

                if maybe_span.is_none() {
                    continue;
                }

                let snippet = Self::build_snippet(&lines, idx, context_lines);
                let score = 1.0 / (1.0 + idx as f32);

                results.push(SearchResultEntry {
                    path: path.display().to_string(),
                    line: idx + 1,
                    snippet,
                    score,
                });
            }
        }

        let response = SearchResponse {
            query: args.query,
            results,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&response).context("Failed to serialize search results")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_literal_search() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sample.txt");
        fs::write(&file_path, "hello search tool\nsecond line\nhello again").unwrap();

        let tool = SearchTool::new().with_root(dir.path());
        let args = serde_json::json!({
            "query": "hello",
            "root": dir.path().to_string_lossy(),
            "max_results": 5
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let payload: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(payload["results"].as_array().unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn test_regex_search() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("module.rs");
        fs::write(&file_path, "fn test_case() {}\nfn demo_case() {}\n").unwrap();

        let tool = SearchTool::new().with_root(dir.path());
        let args = serde_json::json!({
            "query": "fn\\s+test_\\w+",
            "regex": true,
            "root": dir.path().to_string_lossy(),
            "file_extensions": ["rs"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let payload: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(payload["results"].as_array().unwrap().len(), 1);
    }
}
