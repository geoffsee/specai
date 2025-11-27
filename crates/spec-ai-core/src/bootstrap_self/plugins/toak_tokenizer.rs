use crate::bootstrap_self::plugin::{BootstrapMode, BootstrapPlugin, PluginContext, PluginOutcome};
use crate::types::{EdgeType, NodeType};
use anyhow::{Context, Result};
use blake3::Hasher;
use serde_json::json;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use toak_rs::{clean_and_redact, count_tokens, JsonDatabaseGenerator, JsonDatabaseOptions};
use tokio::runtime::Builder as RuntimeBuilder;
use walkdir::WalkDir;

const MAX_FILES_ANALYZED: usize = 200;
const MAX_BYTES_PER_FILE: usize = 200_000;
const TOP_FILES: usize = 8;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".idea",
    ".vscode",
    "target",
    "dist",
    "build",
    "node_modules",
    "tmp",
    "temp",
];

const BINARY_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "svg", "webp", "tiff", "ico", "ttf", "woff", "woff2",
    "eot", "otf", "exe", "dll", "so", "dylib", "bin", "dat", "pyc", "pyo", "class", "jar", "zip",
    "tar", "gz", "rar", "7z", "mp3", "mp4", "avi", "mov", "wav", "db", "sqlite", "sqlite3", "lock",
    "lockb",
];

static BOOTSTRAP_PHASES: &[&str] = &[
    "Discover tracked files for tokenization",
    "Clean and redact content using toak-rs helpers",
    "Summarize token footprint and link graph nodes",
    "Generate embeddings database for semantic search",
];

pub struct ToakTokenizerPlugin;

impl BootstrapPlugin for ToakTokenizerPlugin {
    fn name(&self) -> &'static str {
        "toak-tokenizer"
    }

    fn phases(&self) -> Vec<&'static str> {
        BOOTSTRAP_PHASES.to_vec()
    }

    fn should_activate(&self, repo_root: &PathBuf) -> bool {
        repo_root.join(".git").exists()
    }

    fn run(&self, context: PluginContext) -> Result<PluginOutcome> {
        let mut outcome = PluginOutcome::new(self.name());
        outcome.phases = self.phases().iter().map(|s| s.to_string()).collect();

        let tracked_files = self.tracked_files(context.repo_root)?;
        let summary = self.analyze_files(&context, &tracked_files)?;

        let (embeddings_path, embeddings_cached) =
            match self.generate_embeddings_database(&context, &tracked_files) {
                Ok(res) => res,
                Err(err) => {
                    tracing::warn!("toak-tokenizer: embeddings generation skipped: {err:#}");
                    (None, false)
                }
            };

        let repository_name = context
            .repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repository")
            .to_string();

        let repo_props = json!({
            "name": repository_name,
            "path": context.repo_root.display().to_string(),
            "bootstrap_source": "toak-tokenizer-plugin",
            "token_profile": {
                "files_considered": tracked_files.len(),
                "files_analyzed": summary.files_analyzed(),
                "max_files": MAX_FILES_ANALYZED,
                "bytes_scanned": summary.total_bytes,
                "avg_tokens_per_file": summary.average_cleaned_tokens(),
                "raw_token_total": summary.total_raw_tokens,
                "cleaned_token_total": summary.total_cleaned_tokens,
                "cached_reused": summary.cached_hits,
            },
            "embeddings_path": embeddings_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            "embeddings_cached": embeddings_cached,
        });

        let repo_node_id = context.persistence.insert_graph_node(
            context.session_id,
            NodeType::Entity,
            "Repository",
            &repo_props,
            None,
        )?;

        outcome.root_node_id = Some(repo_node_id);
        outcome.nodes_created = 1;

        if summary.files_analyzed() > 0 {
            let top_files: Vec<_> = summary
                .top_files()
                .iter()
                .map(|f| {
                    json!({
                        "path": &f.path,
                        "raw_tokens": f.raw_tokens,
                        "cleaned_tokens": f.cleaned_tokens,
                        "bytes_captured": f.bytes_captured,
                        "truncated": f.truncated,
                    })
                })
                .collect();

            let footprint_props = json!({
                "bootstrap_source": "toak-tokenizer-plugin",
                "files_analyzed": summary.files_analyzed(),
                "raw_token_total": summary.total_raw_tokens,
                "cleaned_token_total": summary.total_cleaned_tokens,
                "bytes_captured": summary.total_bytes,
                "largest_files": top_files,
                "cached_reused": summary.cached_hits,
            });

            let token_node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Concept,
                "TokenFootprint",
                &footprint_props,
                None,
            )?;

            outcome.nodes_created += 1;

            context.persistence.insert_graph_edge(
                context.session_id,
                token_node_id,
                repo_node_id,
                EdgeType::RelatesTo,
                Some("tokenized_with"),
                Some(&json!({"bootstrap_source": "toak-tokenizer-plugin"})),
                0.82,
            )?;
            outcome.edges_created += 1;

            for entry in &summary.entries {
                let file_props = json!({
                    "path": &entry.path,
                    "raw_tokens": entry.raw_tokens,
                    "cleaned_tokens": entry.cleaned_tokens,
                    "bytes_captured": entry.bytes_captured,
                    "truncated": entry.truncated,
                    "cached": entry.cached,
                    "bootstrap_source": "toak-tokenizer-plugin",
                });

                let node_id = context.persistence.insert_graph_node(
                    context.session_id,
                    NodeType::Concept,
                    "TokenizedFile",
                    &file_props,
                    entry.embedding_id,
                )?;
                outcome.nodes_created += 1;

                context.persistence.insert_graph_edge(
                    context.session_id,
                    node_id,
                    repo_node_id,
                    EdgeType::RelatesTo,
                    Some("tokenized_file"),
                    Some(&json!({"bootstrap_source": "toak-tokenizer-plugin"})),
                    0.8,
                )?;
                outcome.edges_created += 1;

                context.persistence.insert_graph_edge(
                    context.session_id,
                    node_id,
                    token_node_id,
                    EdgeType::RelatesTo,
                    Some("summarized_in"),
                    Some(&json!({"bootstrap_source": "toak-tokenizer-plugin"})),
                    0.78,
                )?;
                outcome.edges_created += 1;
            }
        }

        outcome.metadata = json!({
            "repository_name": repository_name,
            "component_count": 0,
            "document_count": 0,
            "tokenized_files": summary.files_analyzed(),
            "cached_files": summary.cached_hits,
            "embeddings_path": embeddings_path.as_ref().map(|p| p.display().to_string()),
            "embeddings_cached": embeddings_cached,
        });

        Ok(outcome)
    }
}

impl ToakTokenizerPlugin {
    fn tracked_files(&self, repo_root: &Path) -> Result<Vec<PathBuf>> {
        if repo_root.join(".git").exists() {
            let output = Command::new("git")
                .arg("ls-files")
                .current_dir(repo_root)
                .output()
                .context("running git ls-files for toak-tokenizer plugin")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let files = stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|line| repo_root.join(line))
                    .collect();
                return Ok(files);
            }
        }

        Ok(self.walk_repository(repo_root))
    }

    fn walk_repository(&self, repo_root: &Path) -> Vec<PathBuf> {
        WalkDir::new(repo_root)
            .into_iter()
            .filter_entry(|entry| !self.is_ignored_dir(entry.path()))
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.into_path())
            .collect()
    }

    fn is_ignored_dir(&self, path: &Path) -> bool {
        path.components()
            .any(|comp| IGNORED_DIRS.contains(&comp.as_os_str().to_string_lossy().as_ref()))
    }

    fn analyze_files(&self, context: &PluginContext, files: &[PathBuf]) -> Result<TokenSummary> {
        let mut summary = TokenSummary::default();

        for path in files
            .iter()
            .filter(|p| !self.should_skip(p))
            .take(MAX_FILES_ANALYZED)
        {
            if let Some(info) = self.process_file(context, path)? {
                summary.record(info);
            }
        }

        Ok(summary)
    }

    fn process_file(&self, context: &PluginContext, path: &Path) -> Result<Option<FileTokenInfo>> {
        let file_hash = match self.hash_file(path) {
            Ok(hash) => hash,
            Err(_) => return Ok(None),
        };

        let relative_path = self.relative_path(context.repo_root, path);

        if let Some(cached) = self.load_cached(context, &relative_path, &file_hash)? {
            return Ok(Some(cached));
        }

        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        if bytes.is_empty() {
            return Ok(None);
        }

        let truncated = bytes.len() > MAX_BYTES_PER_FILE;
        let slice_len = std::cmp::min(bytes.len(), MAX_BYTES_PER_FILE);
        let content = String::from_utf8_lossy(&bytes[..slice_len]);
        let cleaned = clean_and_redact(&content);

        let raw_tokens = count_tokens(&content);
        let cleaned_tokens = count_tokens(&cleaned);

        let embedding_id = Some(self.store_embedding(context, &cleaned)?);

        context.persistence.upsert_tokenized_file(
            context.session_id,
            &relative_path,
            &file_hash,
            raw_tokens,
            cleaned_tokens,
            slice_len,
            truncated,
            embedding_id,
        )?;

        Ok(Some(FileTokenInfo {
            path: relative_path,
            raw_tokens,
            cleaned_tokens,
            bytes_captured: slice_len,
            truncated,
            embedding_id,
            cached: false,
            file_hash,
        }))
    }

    fn should_skip(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let lower = ext.to_ascii_lowercase();
            if BINARY_EXTENSIONS.contains(&lower.as_str()) {
                return true;
            }
        }
        false
    }

    fn load_cached(
        &self,
        context: &PluginContext,
        relative_path: &str,
        file_hash: &str,
    ) -> Result<Option<FileTokenInfo>> {
        let cached = context
            .persistence
            .get_tokenized_file(context.session_id, relative_path)?;

        if let Some(record) = cached {
            if record.file_hash == file_hash {
                return Ok(Some(FileTokenInfo {
                    path: relative_path.to_string(),
                    raw_tokens: record.raw_tokens,
                    cleaned_tokens: record.cleaned_tokens,
                    bytes_captured: record.bytes_captured,
                    truncated: record.truncated,
                    embedding_id: record.embedding_id,
                    cached: record.file_hash == file_hash,
                    file_hash: record.file_hash,
                }));
            }
        }

        Ok(None)
    }

    fn hash_file(&self, path: &Path) -> Result<String> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = Hasher::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    fn store_embedding(&self, context: &PluginContext, cleaned: &str) -> Result<i64> {
        let embedding = self.hashed_embedding(cleaned);
        context
            .persistence
            .insert_memory_vector(context.session_id, None, &embedding)
    }

    fn hashed_embedding(&self, text: &str) -> Vec<f32> {
        let mut hasher = Hasher::new();
        hasher.update(text.as_bytes());
        let digest = hasher.finalize();
        let mut out = Vec::new();
        for chunk in digest.as_bytes().chunks(4).take(8) {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(chunk);
            out.push(u32::from_le_bytes(buf) as f32 / u32::MAX as f32);
        }
        if out.is_empty() {
            out.push(0.0);
        }
        out
    }

    fn relative_path(&self, repo_root: &Path, path: &Path) -> String {
        path.strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }

    fn generate_embeddings_database(
        &self,
        context: &PluginContext,
        tracked_files: &[PathBuf],
    ) -> Result<(Option<PathBuf>, bool)> {
        let root = context.repo_root;

        if !root.exists() || tracked_files.is_empty() {
            return Ok((None, false));
        }

        let embeddings_path = embeddings_output_path(root);
        let existing_valid = embeddings_path
            .metadata()
            .map(|m| m.len() > 0)
            .unwrap_or(false);

        if existing_valid && !matches!(context.mode, BootstrapMode::Refresh) {
            return Ok((Some(embeddings_path), true));
        }

        if let Some(parent) = embeddings_path.parent() {
            fs::create_dir_all(parent).context("creating embeddings cache directory")?;
        }

        let mut file_type_exclusions = HashSet::new();
        for ext in BINARY_EXTENSIONS {
            file_type_exclusions.insert(format!(".{}", ext));
        }

        let mut file_exclusions = Vec::new();
        for dir in IGNORED_DIRS {
            file_exclusions.push(format!("{dir}/**"));
            file_exclusions.push(format!("**/{dir}/**"));
        }

        let options = JsonDatabaseOptions {
            dir: root.clone(),
            output_file_path: embeddings_path.clone(),
            file_type_exclusions,
            file_exclusions,
            verbose: false,
            chunker_config: Default::default(),
            max_concurrent_files: 4,
        };

        let generator = JsonDatabaseGenerator::new(options)
            .context("initializing toak embeddings generator")?;

        let rt = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .context("creating tokio runtime for embeddings generation")?;

        rt.block_on(generator.generate_database())
            .context("generating embeddings database")?;

        Ok((Some(embeddings_path), false))
    }
}

fn embeddings_output_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".spec-ai")
        .join("code_search_embeddings.json")
}

#[derive(Clone, Default)]
struct TokenSummary {
    entries: Vec<FileTokenInfo>,
    total_raw_tokens: usize,
    total_cleaned_tokens: usize,
    total_bytes: usize,
    cached_hits: usize,
}

impl TokenSummary {
    fn record(&mut self, info: FileTokenInfo) {
        self.total_raw_tokens += info.raw_tokens;
        self.total_cleaned_tokens += info.cleaned_tokens;
        self.total_bytes += info.bytes_captured;
        if info.cached {
            self.cached_hits += 1;
        }
        self.entries.push(info);
    }

    fn files_analyzed(&self) -> usize {
        self.entries.len()
    }

    fn average_cleaned_tokens(&self) -> f64 {
        if self.entries.is_empty() {
            0.0
        } else {
            self.total_cleaned_tokens as f64 / self.entries.len() as f64
        }
    }

    fn top_files(&self) -> Vec<FileTokenInfo> {
        let mut files = self.entries.clone();
        files.sort_by(|a, b| b.cleaned_tokens.cmp(&a.cleaned_tokens));
        files.truncate(TOP_FILES);
        files
    }
}

#[derive(Clone)]
struct FileTokenInfo {
    path: String,
    raw_tokens: usize,
    cleaned_tokens: usize,
    bytes_captured: usize,
    truncated: bool,
    embedding_id: Option<i64>,
    cached: bool,
    file_hash: String,
}
