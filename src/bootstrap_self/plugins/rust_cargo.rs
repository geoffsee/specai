use crate::bootstrap_self::plugin::{BootstrapPlugin, PluginContext, PluginOutcome};
use crate::types::{EdgeType, NodeType};
use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const IGNORED_TOP_LEVEL: &[&str] = &[
    ".git",
    ".github",
    ".idea",
    ".vscode",
    "target",
    "tmp",
    "node_modules",
];
const MAX_COMPONENTS: usize = 12;
const MAX_DOCUMENTS: usize = 8;
const COMPONENT_SCAN_LIMIT: usize = 400;
const DOCUMENT_PREVIEW_BYTES: usize = 2048;
const SAMPLE_FILES_PER_COMPONENT: usize = 5;

static BOOTSTRAP_PHASES: &[&str] = &[
    "Survey the repository layout and capture component stats",
    "Index canonical documents and specs for semantic recall",
    "Extract dependency and build surfaces from Cargo manifests",
    "Link every artifact into the session knowledge graph",
];

pub struct RustCargoPlugin;

impl BootstrapPlugin for RustCargoPlugin {
    fn name(&self) -> &'static str {
        "rust-cargo"
    }

    fn phases(&self) -> Vec<&'static str> {
        BOOTSTRAP_PHASES.to_vec()
    }

    fn should_activate(&self, repo_root: &PathBuf) -> bool {
        repo_root.join("Cargo.toml").exists()
    }

    fn run(&self, context: PluginContext) -> Result<PluginOutcome> {
        let mut outcome = PluginOutcome::new("rust-cargo");
        outcome.phases = self.phases().iter().map(|s| s.to_string()).collect();

        let metadata = self.collect_repo_metadata(context.repo_root)?;
        let components = self.collect_components(context.repo_root)?;
        let documents = self.collect_documents(context.repo_root);
        let manifest = metadata.manifest.clone();

        let repo_props = json!({
            "name": metadata.name,
            "version": metadata.version,
            "description": metadata.description,
            "edition": metadata.edition,
            "path": context.repo_root.display().to_string(),
            "component_count": components.len(),
            "document_count": documents.len(),
            "dependency_groups": {
                "runtime": manifest.dependencies.len(),
                "dev": manifest.dev_dependencies.len(),
                "build": manifest.build_dependencies.len()
            },
            "component_catalog": components.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "document_catalog": documents.iter().map(|d| d.relative_path.clone()).collect::<Vec<_>>(),
            "phases": outcome.phases.clone(),
            "bootstrap_source": "rust-cargo-plugin",
            "captured_at": Utc::now().to_rfc3339(),
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

        for component in &components {
            let component_props = json!({
                "name": component.name,
                "path": component.relative_path,
                "component_type": component.kind.as_str(),
                "stats": {
                    "files_indexed": component.stats.total_files,
                    "code_files": component.stats.code_files,
                    "doc_files": component.stats.doc_files,
                    "test_files": component.stats.test_files,
                    "depth": component.stats.max_depth,
                    "samples": component.stats.sample_files,
                    "truncated": component.stats.truncated,
                },
                "bootstrap_source": "rust-cargo-plugin",
            });

            let node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Entity,
                "Component",
                &component_props,
                None,
            )?;
            outcome.nodes_created += 1;

            context.persistence.insert_graph_edge(
                context.session_id,
                node_id,
                repo_node_id,
                EdgeType::PartOf,
                Some("component_of"),
                Some(&json!({"bootstrap_source": "rust-cargo-plugin"})),
                0.95,
            )?;
            outcome.edges_created += 1;
        }

        for document in &documents {
            let doc_props = json!({
                "title": document.title.clone(),
                "path": &document.relative_path,
                "preview": document.preview.clone(),
                "line_count": document.line_count,
                "bytes_captured": document.bytes_captured,
                "truncated": document.truncated,
                "bootstrap_source": "rust-cargo-plugin",
            });

            let doc_node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Fact,
                "RepositoryDocument",
                &doc_props,
                None,
            )?;
            outcome.nodes_created += 1;

            context.persistence.insert_graph_edge(
                context.session_id,
                doc_node_id,
                repo_node_id,
                EdgeType::RelatesTo,
                Some("documents"),
                Some(&json!({"bootstrap_source": "rust-cargo-plugin"})),
                0.85,
            )?;
            outcome.edges_created += 1;
        }

        if !manifest.is_empty() {
            let manifest_props = json!({
                "dependencies": manifest.dependencies,
                "dev_dependencies": manifest.dev_dependencies,
                "build_dependencies": manifest.build_dependencies,
                "features": manifest.features,
                "bootstrap_source": "rust-cargo-plugin",
            });

            let manifest_node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Concept,
                "CargoManifest",
                &manifest_props,
                None,
            )?;
            outcome.nodes_created += 1;

            context.persistence.insert_graph_edge(
                context.session_id,
                manifest_node_id,
                repo_node_id,
                EdgeType::DependsOn,
                Some("builds"),
                Some(&json!({"bootstrap_source": "rust-cargo-plugin"})),
                0.9,
            )?;
            outcome.edges_created += 1;
        }

        outcome.metadata = json!({
            "repository_name": metadata.name,
            "component_count": components.len(),
            "document_count": documents.len(),
        });

        Ok(outcome)
    }
}

impl RustCargoPlugin {
    fn collect_repo_metadata(&self, repo_root: &Path) -> Result<RepoMetadata> {
        let manifest_path = repo_root.join("Cargo.toml");
        let manifest_raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest_value: toml::Value =
            toml::from_str(&manifest_raw).context("parsing Cargo.toml")?;

        let package = manifest_value
            .get("package")
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();

        let name = package
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let version = package
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();
        let description = package
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let edition = package
            .get("edition")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let dependencies = extract_dependency_section(manifest_value.get("dependencies"));
        let dev_dependencies = extract_dependency_section(manifest_value.get("dev-dependencies"));
        let build_dependencies =
            extract_dependency_section(manifest_value.get("build-dependencies"));

        let features = manifest_value
            .get("features")
            .and_then(|v| v.as_table())
            .map(|table| table.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(RepoMetadata {
            name,
            version,
            description,
            edition,
            manifest: ManifestDigest {
                dependencies,
                dev_dependencies,
                build_dependencies,
                features,
            },
        })
    }

    fn collect_components(&self, repo_root: &Path) -> Result<Vec<RepoComponent>> {
        let mut components = Vec::new();
        let entries =
            fs::read_dir(repo_root).with_context(|| format!("reading {}", repo_root.display()))?;

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if IGNORED_TOP_LEVEL.contains(&name.as_str()) {
                continue;
            }
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                let stats = self.summarize_directory(&path, repo_root)?;
                let relative_path = to_relative_string(&path, repo_root);
                let kind = ComponentKind::classify(&name);
                components.push(RepoComponent {
                    name,
                    relative_path,
                    kind,
                    stats,
                });
            }
        }

        components.sort_by(|a, b| {
            b.stats
                .total_files
                .cmp(&a.stats.total_files)
                .then_with(|| a.name.cmp(&b.name))
        });
        components.truncate(MAX_COMPONENTS);
        Ok(components)
    }

    fn summarize_directory(&self, path: &Path, repo_root: &Path) -> Result<ComponentStats> {
        let mut stats = ComponentStats::default();
        for entry in WalkDir::new(path).min_depth(1).into_iter() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().is_dir() {
                continue;
            }

            stats.total_files += 1;
            let rel = to_relative_string(entry.path(), repo_root);
            if stats.sample_files.len() < SAMPLE_FILES_PER_COMPONENT {
                stats.sample_files.push(rel.clone());
            }
            stats.max_depth = stats.max_depth.max(entry.depth());

            let ext = entry
                .path()
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "rb" | "go" | "c" | "cpp" | "java"
            ) {
                stats.code_files += 1;
            }
            if matches!(ext.as_str(), "md" | "txt" | "rst" | "adoc") {
                stats.doc_files += 1;
            }
            if rel.contains("test") || rel.contains("spec") {
                stats.test_files += 1;
            }

            if stats.total_files >= COMPONENT_SCAN_LIMIT {
                stats.truncated = true;
                break;
            }
        }
        Ok(stats)
    }

    fn collect_documents(&self, repo_root: &Path) -> Vec<DocumentDigest> {
        let mut documents = Vec::new();
        let mut push_if_present = |relative: &str| {
            let path = repo_root.join(relative);
            if path.is_file() {
                if let Some(digest) = digest_document(&path, repo_root) {
                    documents.push(digest);
                }
            }
        };

        for candidate in ["README.md", "SETUP.md", "VERIFY.md", "CONTRIBUTING.md"] {
            push_if_present(candidate);
        }

        let docs_dir = repo_root.join("docs");
        if docs_dir.is_dir() {
            for entry in WalkDir::new(&docs_dir)
                .max_depth(2)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file()
                    && matches!(
                        entry
                            .path()
                            .extension()
                            .and_then(OsStr::to_str)
                            .unwrap_or(""),
                        "md" | "txt"
                    )
                {
                    if let Some(digest) = digest_document(entry.path(), repo_root) {
                        documents.push(digest);
                    }
                }
                if documents.len() >= MAX_DOCUMENTS {
                    break;
                }
            }
        }

        let specs_dir = repo_root.join("specs");
        if documents.len() < MAX_DOCUMENTS && specs_dir.is_dir() {
            for entry in WalkDir::new(&specs_dir)
                .max_depth(1)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .and_then(OsStr::to_str)
                        .map(|ext| ext.eq_ignore_ascii_case("spec"))
                        .unwrap_or(false)
                {
                    if let Some(digest) = digest_document(entry.path(), repo_root) {
                        documents.push(digest);
                    }
                }
                if documents.len() >= MAX_DOCUMENTS {
                    break;
                }
            }
        }

        documents.truncate(MAX_DOCUMENTS);
        documents
    }
}

fn digest_document(path: &Path, repo_root: &Path) -> Option<DocumentDigest> {
    let contents = fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    let preview = trimmed
        .chars()
        .take(DOCUMENT_PREVIEW_BYTES)
        .collect::<String>();
    let truncated = trimmed.len() > preview.len();
    let title = trimmed
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("document")
                .to_string()
        });

    let preview_len = preview.len();

    Some(DocumentDigest {
        relative_path: to_relative_string(path, repo_root),
        title,
        preview,
        line_count: trimmed.lines().count(),
        bytes_captured: preview_len,
        truncated,
    })
}

#[derive(Debug, Clone)]
struct RepoMetadata {
    name: String,
    version: String,
    description: Option<String>,
    edition: Option<String>,
    manifest: ManifestDigest,
}

#[derive(Debug, Clone)]
struct ManifestDigest {
    dependencies: Vec<String>,
    dev_dependencies: Vec<String>,
    build_dependencies: Vec<String>,
    features: Vec<String>,
}

impl ManifestDigest {
    fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
            && self.dev_dependencies.is_empty()
            && self.build_dependencies.is_empty()
            && self.features.is_empty()
    }
}

#[derive(Debug, Clone)]
struct RepoComponent {
    name: String,
    relative_path: String,
    kind: ComponentKind,
    stats: ComponentStats,
}

#[derive(Debug, Clone, Default)]
struct ComponentStats {
    total_files: usize,
    code_files: usize,
    doc_files: usize,
    test_files: usize,
    sample_files: Vec<String>,
    max_depth: usize,
    truncated: bool,
}

#[derive(Debug)]
struct DocumentDigest {
    relative_path: String,
    title: String,
    preview: String,
    line_count: usize,
    bytes_captured: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Copy)]
enum ComponentKind {
    Source,
    Tests,
    Docs,
    Specs,
    Examples,
    Scripts,
    Config,
    Data,
    Other,
}

impl ComponentKind {
    fn classify(name: &str) -> Self {
        match name {
            "src" => ComponentKind::Source,
            "tests" | "test" => ComponentKind::Tests,
            "docs" => ComponentKind::Docs,
            "specs" => ComponentKind::Specs,
            "examples" => ComponentKind::Examples,
            "scripts" | "bin" => ComponentKind::Scripts,
            "config" | "configurations" => ComponentKind::Config,
            "data" | "datasets" => ComponentKind::Data,
            _ => ComponentKind::Other,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            ComponentKind::Source => "source",
            ComponentKind::Tests => "tests",
            ComponentKind::Docs => "docs",
            ComponentKind::Specs => "specs",
            ComponentKind::Examples => "examples",
            ComponentKind::Scripts => "scripts",
            ComponentKind::Config => "config",
            ComponentKind::Data => "data",
            ComponentKind::Other => "other",
        }
    }
}

fn extract_dependency_section(section: Option<&toml::Value>) -> Vec<String> {
    let mut deps = Vec::new();
    if let Some(toml::Value::Table(table)) = section {
        for (name, value) in table {
            if let Some(version) = value.as_str() {
                deps.push(format!("{name} = {version}"));
            } else if let Some(inner) = value.as_table() {
                let mut parts = Vec::new();
                if let Some(version) = inner.get("version").and_then(|v| v.as_str()) {
                    parts.push(format!("version:{version}"));
                }
                if let Some(path) = inner.get("path").and_then(|v| v.as_str()) {
                    parts.push(format!("path:{path}"));
                }
                if let Some(git) = inner.get("git").and_then(|v| v.as_str()) {
                    parts.push(format!("git:{git}"));
                }
                if let Some(optional) = inner.get("optional").and_then(|v| v.as_bool()) {
                    parts.push(format!("optional:{optional}"));
                }
                if let Some(features) = inner.get("features").and_then(|v| v.as_array()) {
                    let feature_list = features
                        .iter()
                        .filter_map(|f| f.as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    if !feature_list.is_empty() {
                        parts.push(format!("features:{feature_list}"));
                    }
                }
                let detail = if parts.is_empty() {
                    "custom".to_string()
                } else {
                    parts.join(" | ")
                };
                deps.push(format!("{name} ({detail})"));
            }
        }
    }
    deps
}

fn to_relative_string(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
