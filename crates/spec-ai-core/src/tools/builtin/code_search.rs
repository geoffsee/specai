use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use toak_rs::{JsonDatabaseGenerator, JsonDatabaseOptions, SemanticSearch};

const DEFAULT_TOP_N: usize = 3;
const MAX_TOP_N: usize = 25;

#[derive(Debug, Deserialize)]
struct CodeSearchArgs {
    query: String,
    top_n: Option<usize>,
    root: Option<String>,
    refresh: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CodeSearchResult {
    path: String,
    similarity: f32,
    snippet: String,
}

#[derive(Debug, Serialize)]
struct CodeSearchResponse {
    query: String,
    root: String,
    top_n: usize,
    results: Vec<CodeSearchResult>,
}

/// Simple semantic code search powered by toak-rs embeddings.
pub struct CodeSearchTool {
    root: PathBuf,
}

impl CodeSearchTool {
    pub fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { root }
    }

    fn resolve_root(&self, override_root: &Option<String>) -> PathBuf {
        override_root
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.root.clone())
    }

    fn cache_path(root: &Path) -> PathBuf {
        root.join(".spec-ai").join("code_search_embeddings.json")
    }

    async fn ensure_embeddings(&self, root: &Path, refresh: bool, top_n: usize) -> Result<PathBuf> {
        let embeddings_path = Self::cache_path(root);
        if embeddings_path.exists() && !refresh {
            return Ok(embeddings_path);
        }

        if let Some(parent) = embeddings_path.parent() {
            std::fs::create_dir_all(parent).context("creating code-search cache dir")?;
        }

        let options = JsonDatabaseOptions {
            dir: root.to_path_buf(),
            output_file_path: embeddings_path.clone(),
            verbose: false,
            chunker_config: Default::default(),
            max_concurrent_files: 4,
            file_type_exclusions: Default::default(),
            file_exclusions: Default::default(),
        };

        let generator = JsonDatabaseGenerator::new(options)
            .context("initializing toak embeddings generator")?;

        generator
            .generate_database()
            .await
            .with_context(|| format!("generating embeddings database (top_n={})", top_n))?;

        Ok(embeddings_path)
    }
}

impl Default for CodeSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CodeSearchTool {
    fn name(&self) -> &str {
        "code_search"
    }

    fn description(&self) -> &str {
        "Semantic code search using toak-rs embeddings"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query text"
                },
                "top_n": {
                    "type": "integer",
                    "description": "Number of results to return (default 3, max 25)"
                },
                "root": {
                    "type": "string",
                    "description": "Repository root to search (defaults to current dir)"
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Force re-generation of embeddings (default false)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: CodeSearchArgs =
            serde_json::from_value(args).context("Failed to parse code_search arguments")?;

        if args.query.trim().is_empty() {
            return Err(anyhow!("query cannot be empty"));
        }

        let top_n = args.top_n.unwrap_or(DEFAULT_TOP_N).clamp(1, MAX_TOP_N);

        let root = self.resolve_root(&args.root);
        if !root.exists() {
            return Err(anyhow!("Search root {} does not exist", root.display()));
        }

        let refresh = args.refresh.unwrap_or(false);
        let embeddings_path = self
            .ensure_embeddings(&root, refresh, top_n)
            .await
            .context("building embeddings database")?;

        let mut searcher =
            SemanticSearch::new(&embeddings_path).context("loading embeddings database")?;
        let hits = searcher
            .search(&args.query, top_n)
            .context("running semantic search")?;

        let results = hits
            .into_iter()
            .map(|hit| {
                let mut snippet = hit.content;
                if snippet.len() > 480 {
                    snippet.truncate(480);
                    snippet.push_str("...[truncated]");
                }
                CodeSearchResult {
                    path: hit.file_path,
                    similarity: hit.similarity,
                    snippet,
                }
            })
            .collect();

        let response = CodeSearchResponse {
            query: args.query,
            root: root.display().to_string(),
            top_n,
            results,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&response).context("serializing search response")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runs_search_with_generated_embeddings() {
        if std::env::var("RUN_TOAK_SEARCH_TEST").is_err() {
            // Skip in environments without fastembed model access
            return;
        }

        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("a.rs"), "fn alpha() {}\n// comment\n").unwrap();
        fs::write(root.join("b.rs"), "fn beta_thing() { let x = 1; }\n").unwrap();

        // Initialize git so toak can discover files
        Command::new("git")
            .arg("init")
            .current_dir(root)
            .output()
            .expect("git init failed");
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .expect("git add failed");

        let tool = CodeSearchTool::new();
        let args = serde_json::json!({
            "query": "beta",
            "root": root.to_string_lossy(),
            "top_n": 2,
            "refresh": true
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let payload: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        let hits = payload["results"].as_array().unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0]["path"].as_str().unwrap().contains("b.rs"));
    }
}
