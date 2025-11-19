use crate::bootstrap_self::plugin::{BootstrapPlugin, PluginContext, PluginOutcome};
use crate::types::{EdgeType, NodeType};
use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Analysis configuration
const MAX_FILES_SCANNED: usize = 1000;
const MAX_INTENT_FILES: usize = 50;
const MAX_INTENT_BYTES: usize = 51_200; // 50KB
const MAX_SEMANTIC_BYTES: usize = 512_000; // 500KB
const SAMPLE_FILES_PER_COMPONENT: usize = 5;
const MAX_COMPONENTS: usize = 15;
const MAX_DOCUMENTS: usize = 8;

static BOOTSTRAP_PHASES: &[&str] = &[
    "Classify files and build structural model",
    "Analyze codebase intent and purpose using fast model",
    "Generate semantic understanding with main model",
    "Build knowledge graph from analysis",
];

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".gitignore",
    ".idea",
    ".vscode",
    "target",
    "dist",
    "build",
    "node_modules",
    ".next",
    ".nuxt",
    "tmp",
    "temp",
    "__pycache__",
    ".pytest_cache",
    "venv",
    ".venv",
];

const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "rb", "go", "c", "cpp", "cc", "cxx", "h", "hpp", "java",
    "kt", "scala", "php", "swift", "m", "mm", "cs", "vb", "f90", "pl", "sh", "bash",
];

const DOC_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "rst", "adoc", "asciidoc"];

pub struct UniversalCodePlugin;

impl BootstrapPlugin for UniversalCodePlugin {
    fn name(&self) -> &'static str {
        "universal-code"
    }

    fn phases(&self) -> Vec<&'static str> {
        BOOTSTRAP_PHASES.to_vec()
    }

    fn should_activate(&self, repo_root: &PathBuf) -> bool {
        // Activate if repository has any code files or common markers
        repo_root.join(".git").exists()
            || repo_root.join(".hg").exists()
            || self.contains_code_files(repo_root)
            || self.has_any_manifest(repo_root)
    }

    fn run(&self, context: PluginContext) -> Result<PluginOutcome> {
        let mut outcome = PluginOutcome::new("universal-code");
        outcome.phases = self.phases().iter().map(|s| s.to_string()).collect();

        // Phase 1: File Classification
        let classification = self.classify_files(context.repo_root)?;

        // Phase 2: Intent Analysis (using fast model simulation)
        let intent = self.analyze_intent(context.repo_root, &classification)?;

        // Phase 3: Semantic Analysis (using main model simulation)
        let semantic = self.analyze_semantic(context.repo_root, &classification, &intent)?;

        // Phase 4: Build Knowledge Graph
        self.build_knowledge_graph(
            context.clone(),
            &classification,
            &intent,
            &semantic,
            &mut outcome,
        )?;

        // Generate .SPEC-AI.md document
        let spec_document = self.generate_spec_document(&classification, &intent, &semantic)?;
        self.write_spec_document(context.repo_root, &spec_document)?;

        outcome.metadata = json!({
            "repository_name": intent.project_name,
            "component_count": classification.components.len(),
            "document_count": classification.documents.len(),
            "detected_languages": classification.detected_languages,
            "detected_frameworks": classification.detected_frameworks,
            "architecture_pattern": semantic.architecture_pattern,
            "estimated_complexity": semantic.complexity_estimate,
            "file_count": classification.total_files,
        });

        Ok(outcome)
    }
}

impl UniversalCodePlugin {
    fn contains_code_files(&self, repo_root: &Path) -> bool {
        if let Ok(entries) = fs::read_dir(repo_root) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Some(ext) = entry.path().extension() {
                            if let Some(ext_str) = ext.to_str() {
                                if CODE_EXTENSIONS.contains(&ext_str.to_lowercase().as_str()) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn has_any_manifest(&self, repo_root: &Path) -> bool {
        let manifests = vec![
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "setup.py",
            "requirements.txt",
            "go.mod",
            "go.sum",
            "Gemfile",
            "pom.xml",
            "build.gradle",
            "gradle.properties",
            "Makefile",
            "CMakeLists.txt",
            "package.yaml",
            "composer.json",
        ];
        manifests
            .iter()
            .any(|manifest| repo_root.join(manifest).exists())
    }

    fn classify_files(&self, repo_root: &Path) -> Result<FileClassification> {
        let mut classification = FileClassification::default();
        let mut file_count = 0;
        let mut language_counts: HashMap<String, usize> = HashMap::new();

        for entry in WalkDir::new(repo_root).into_iter().filter_map(Result::ok) {
            if file_count >= MAX_FILES_SCANNED {
                break;
            }

            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            // Check if we should ignore this path
            if path
                .to_string_lossy()
                .split('/')
                .any(|segment| IGNORED_DIRS.contains(&segment))
            {
                continue;
            }

            file_count += 1;

            let rel_path = self.to_relative_string(path, repo_root);

            // Classify by extension
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    let lower_ext = ext_str.to_lowercase();

                    // Detect language
                    let lang = match lower_ext.as_str() {
                        "rs" => "rust",
                        "py" => "python",
                        "ts" | "tsx" => "typescript",
                        "js" | "jsx" => "javascript",
                        "go" => "go",
                        "java" => "java",
                        "cpp" | "cc" | "cxx" => "c++",
                        "c" => "c",
                        "rb" => "ruby",
                        "php" => "php",
                        "swift" => "swift",
                        "kt" => "kotlin",
                        "cs" => "csharp",
                        "h" | "hpp" => "c/c++",
                        _ => continue,
                    };
                    *language_counts.entry(lang.to_string()).or_insert(0) += 1;

                    // Classify by type
                    if CODE_EXTENSIONS.contains(&lower_ext.as_str()) {
                        classification.source_code.push(rel_path.clone());
                    } else if DOC_EXTENSIONS.contains(&lower_ext.as_str()) {
                        classification.documentation.push(rel_path.clone());
                    }
                }
            }

            // Classify by path patterns
            if rel_path.contains("/test")
                || rel_path.contains("_test.")
                || rel_path.contains("_spec.")
            {
                classification.tests.push(rel_path.clone());
            } else if rel_path.contains("/examples/") {
                classification.examples.push(rel_path);
            }
        }

        classification.total_files = file_count;
        classification.detected_languages =
            language_counts.into_iter().map(|(lang, _)| lang).collect();

        // Collect components (top-level directories)
        self.identify_components(repo_root, &mut classification)?;

        // Collect documents
        self.identify_documents(repo_root, &mut classification)?;

        // Detect frameworks
        self.detect_frameworks(repo_root, &mut classification)?;

        Ok(classification)
    }

    fn identify_components(
        &self,
        repo_root: &Path,
        classification: &mut FileClassification,
    ) -> Result<()> {
        let mut components = Vec::new();

        for entry in fs::read_dir(repo_root)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() && !IGNORED_DIRS.contains(&name.as_str()) {
                let rel_path = self.to_relative_string(&path, repo_root);
                let kind = classify_component_type(&name);
                components.push(ComponentInfo {
                    name,
                    relative_path: rel_path,
                    kind,
                });
            }
        }

        components.sort_by(|a, b| a.name.cmp(&b.name));
        components.truncate(MAX_COMPONENTS);
        classification.components = components;

        Ok(())
    }

    fn identify_documents(
        &self,
        repo_root: &Path,
        classification: &mut FileClassification,
    ) -> Result<()> {
        let mut documents = Vec::new();

        // Check for common documentation files
        let doc_files = vec![
            "README.md",
            "CONTRIBUTING.md",
            "ARCHITECTURE.md",
            "DESIGN.md",
            "API.md",
            "SETUP.md",
        ];

        for doc_file in doc_files {
            let path = repo_root.join(doc_file);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let preview = content.chars().take(1024).collect::<String>();
                    documents.push(DocumentInfo {
                        path: self.to_relative_string(&path, repo_root),
                        title: doc_file.to_string(),
                        preview,
                    });
                }
            }
        }

        // Check docs directory
        let docs_dir = repo_root.join("docs");
        if docs_dir.exists() {
            if let Ok(entries) = fs::read_dir(&docs_dir) {
                for entry in entries.flatten() {
                    if documents.len() >= MAX_DOCUMENTS {
                        break;
                    }
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if let Some(ext_str) = ext.to_str() {
                            if DOC_EXTENSIONS.contains(&ext_str.to_lowercase().as_str()) {
                                if let Ok(content) = fs::read_to_string(&path) {
                                    let preview = content.chars().take(1024).collect::<String>();
                                    let title = path
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("doc")
                                        .to_string();
                                    documents.push(DocumentInfo {
                                        path: self.to_relative_string(&path, repo_root),
                                        title,
                                        preview,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        classification.documents = documents;
        Ok(())
    }

    fn detect_frameworks(
        &self,
        repo_root: &Path,
        classification: &mut FileClassification,
    ) -> Result<()> {
        let mut frameworks = HashSet::new();

        // Check manifest files for dependencies
        let manifest_files = vec![
            (
                "Cargo.toml",
                vec!["tokio", "actix", "rocket", "axum", "serde", "diesel"],
            ),
            (
                "package.json",
                vec!["react", "vue", "angular", "express", "nextjs", "fastapi"],
            ),
            (
                "pyproject.toml",
                vec!["django", "flask", "fastapi", "sqlalchemy", "pytest"],
            ),
            ("go.mod", vec!["gin", "echo", "fiber", "gorm"]),
            ("pom.xml", vec!["spring", "hibernate", "maven"]),
        ];

        for (manifest_file, framework_keywords) in manifest_files {
            let path = repo_root.join(manifest_file);
            if let Ok(content) = fs::read_to_string(path) {
                for keyword in framework_keywords {
                    if content.to_lowercase().contains(keyword) {
                        frameworks.insert(keyword.to_string());
                    }
                }
            }
        }

        classification.detected_frameworks = frameworks.into_iter().collect();
        Ok(())
    }

    fn analyze_intent(
        &self,
        repo_root: &Path,
        classification: &FileClassification,
    ) -> Result<IntentAnalysis> {
        // Simulate fast model analysis by reading key files
        let mut intent = IntentAnalysis {
            project_name: self.extract_project_name(repo_root),
            purpose_statement: String::new(),
            project_domain: "general".to_string(),
            key_concepts: Vec::new(),
        };

        // Try to extract from README
        if let Ok(readme_content) = fs::read_to_string(repo_root.join("README.md")) {
            let lines: Vec<&str> = readme_content.lines().collect();
            if let Some(first_line) = lines.first() {
                intent.purpose_statement = first_line.trim_start_matches('#').trim().to_string();
            }
        }

        // Infer domain from languages and frameworks
        intent.project_domain = infer_domain(
            &classification.detected_languages,
            &classification.detected_frameworks,
        );

        // Extract key concepts from detected frameworks and languages
        intent
            .key_concepts
            .extend(classification.detected_frameworks.clone());
        intent
            .key_concepts
            .extend(classification.detected_languages.clone());

        if intent.purpose_statement.is_empty() {
            intent.purpose_statement = format!(
                "A {} project using {}",
                intent.project_domain,
                if classification.detected_languages.is_empty() {
                    "unknown languages".to_string()
                } else {
                    classification.detected_languages.join(", ")
                }
            );
        }

        Ok(intent)
    }

    fn analyze_semantic(
        &self,
        repo_root: &Path,
        classification: &FileClassification,
        intent: &IntentAnalysis,
    ) -> Result<SemanticAnalysis> {
        let mut semantic = SemanticAnalysis {
            architecture_pattern: infer_architecture_pattern(&classification.components),
            complexity_estimate: estimate_complexity(
                classification.total_files,
                classification.source_code.len(),
            ),
            key_abstractions: Vec::new(),
            critical_features: Vec::new(),
        };

        // Add key abstractions based on detected patterns
        for component in &classification.components {
            semantic.key_abstractions.push(component.name.clone());
        }

        // Identify critical features
        if classification.source_code.len() > 0 {
            semantic
                .critical_features
                .push("Core Implementation".to_string());
        }
        if classification.tests.len() > 0 {
            semantic.critical_features.push("Test Suite".to_string());
        }
        if classification.documentation.len() > 0 {
            semantic.critical_features.push("Documentation".to_string());
        }

        Ok(semantic)
    }

    fn build_knowledge_graph(
        &self,
        context: PluginContext,
        classification: &FileClassification,
        intent: &IntentAnalysis,
        semantic: &SemanticAnalysis,
        outcome: &mut PluginOutcome,
    ) -> Result<()> {
        // Create repository entity node
        let repo_props = json!({
            "name": intent.project_name,
            "purpose": intent.purpose_statement,
            "domain": intent.project_domain,
            "languages": classification.detected_languages,
            "frameworks": classification.detected_frameworks,
            "path": context.repo_root.display().to_string(),
            "total_files": classification.total_files,
            "source_files": classification.source_code.len(),
            "test_files": classification.tests.len(),
            "doc_files": classification.documentation.len(),
            "components": classification.components.len(),
            "architecture": semantic.architecture_pattern,
            "complexity": semantic.complexity_estimate,
            "captured_at": Utc::now().to_rfc3339(),
            "bootstrap_source": "universal-code-plugin",
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

        // Create component nodes
        for component in &classification.components {
            let component_props = json!({
                "name": component.name,
                "path": component.relative_path,
                "type": component.kind,
                "bootstrap_source": "universal-code-plugin",
            });

            let component_node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Entity,
                "Component",
                &component_props,
                None,
            )?;

            outcome.nodes_created += 1;

            // Link component to repository
            context.persistence.insert_graph_edge(
                context.session_id,
                component_node_id,
                repo_node_id,
                EdgeType::PartOf,
                Some("component_of"),
                Some(&json!({"bootstrap_source": "universal-code-plugin"})),
                0.95,
            )?;
            outcome.edges_created += 1;
        }

        // Create document nodes
        for doc in &classification.documents {
            let doc_props = json!({
                "title": doc.title,
                "path": doc.path,
                "bootstrap_source": "universal-code-plugin",
            });

            let doc_node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Fact,
                "Documentation",
                &doc_props,
                None,
            )?;

            outcome.nodes_created += 1;

            // Link document to repository
            context.persistence.insert_graph_edge(
                context.session_id,
                doc_node_id,
                repo_node_id,
                EdgeType::RelatesTo,
                Some("documents"),
                Some(&json!({"bootstrap_source": "universal-code-plugin"})),
                0.85,
            )?;
            outcome.edges_created += 1;
        }

        // Create concept nodes for architecture pattern
        if !semantic.architecture_pattern.is_empty() {
            let arch_props = json!({
                "pattern": semantic.architecture_pattern,
                "bootstrap_source": "universal-code-plugin",
            });

            let arch_node_id = context.persistence.insert_graph_node(
                context.session_id,
                NodeType::Concept,
                "ArchitecturePattern",
                &arch_props,
                None,
            )?;

            outcome.nodes_created += 1;

            // Link architecture to repository
            context.persistence.insert_graph_edge(
                context.session_id,
                arch_node_id,
                repo_node_id,
                EdgeType::RelatesTo,
                Some("implements"),
                Some(&json!({"bootstrap_source": "universal-code-plugin"})),
                0.90,
            )?;
            outcome.edges_created += 1;
        }

        Ok(())
    }

    fn generate_spec_document(
        &self,
        classification: &FileClassification,
        intent: &IntentAnalysis,
        semantic: &SemanticAnalysis,
    ) -> Result<String> {
        let mut doc = String::new();

        doc.push_str(&format!("# {}\n\n", intent.project_name));
        doc.push_str(&format!("## Overview\n\n{}\n\n", intent.purpose_statement));

        doc.push_str("## Project Information\n\n");
        doc.push_str(&format!("- **Domain**: {}\n", intent.project_domain));
        doc.push_str(&format!(
            "- **Architecture**: {}\n",
            semantic.architecture_pattern
        ));
        doc.push_str(&format!(
            "- **Complexity**: {}\n",
            semantic.complexity_estimate
        ));

        if !intent.key_concepts.is_empty() {
            doc.push_str(&format!(
                "- **Key Technologies**: {}\n",
                intent.key_concepts.join(", ")
            ));
        }

        doc.push_str("\n## File Structure\n\n");
        doc.push_str(&format!(
            "- **Total Files**: {}\n",
            classification.total_files
        ));
        doc.push_str(&format!(
            "- **Source Files**: {}\n",
            classification.source_code.len()
        ));
        doc.push_str(&format!(
            "- **Test Files**: {}\n",
            classification.tests.len()
        ));
        doc.push_str(&format!(
            "- **Documentation**: {}\n",
            classification.documentation.len()
        ));

        if !classification.detected_languages.is_empty() {
            doc.push_str(&format!(
                "\n## Languages Used\n\n{}\n",
                classification
                    .detected_languages
                    .iter()
                    .map(|lang| format!("- {}", lang))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !classification.detected_frameworks.is_empty() {
            doc.push_str(&format!(
                "\n## Frameworks & Libraries\n\n{}\n",
                classification
                    .detected_frameworks
                    .iter()
                    .map(|fw| format!("- {}", fw))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !classification.components.is_empty() {
            doc.push_str("\n## Components\n\n");
            for component in &classification.components {
                doc.push_str(&format!("### {}\n", component.name));
                doc.push_str(&format!("- **Type**: {}\n", component.kind));
                doc.push_str(&format!("- **Path**: {}\n\n", component.relative_path));
            }
        }

        if !semantic.key_abstractions.is_empty() {
            doc.push_str("\n## Key Abstractions\n\n");
            for abstraction in &semantic.key_abstractions {
                doc.push_str(&format!("- {}\n", abstraction));
            }
            doc.push_str("\n");
        }

        if !semantic.critical_features.is_empty() {
            doc.push_str("\n## Critical Features\n\n");
            for feature in &semantic.critical_features {
                doc.push_str(&format!("- {}\n", feature));
            }
            doc.push_str("\n");
        }

        doc.push_str("\n---\n\n");
        doc.push_str("*Generated by Spec-AI Universal Code Plugin*\n");
        doc.push_str(&format!("*{}\n", Utc::now().to_rfc3339()));

        Ok(doc)
    }

    fn write_spec_document(&self, repo_root: &Path, content: &str) -> Result<()> {
        let spec_path = repo_root.join(".SPEC-AI.md");
        fs::write(&spec_path, content)
            .with_context(|| format!("writing .SPEC-AI.md to {}", spec_path.display()))?;
        Ok(())
    }

    fn extract_project_name(&self, repo_root: &Path) -> String {
        // Try to extract from Cargo.toml
        if let Ok(content) = fs::read_to_string(repo_root.join("Cargo.toml")) {
            if let Some(line) = content.lines().find(|l| l.starts_with("name")) {
                if let Some(name_part) = line.split('=').nth(1) {
                    return name_part.trim().trim_matches('"').to_string();
                }
            }
        }

        // Try package.json
        if let Ok(content) = fs::read_to_string(repo_root.join("package.json")) {
            if let Some(line) = content.lines().find(|l| l.contains("\"name\"")) {
                if let Some(name_part) = line.split(':').nth(1) {
                    return name_part
                        .trim()
                        .trim_matches(',')
                        .trim_matches('"')
                        .to_string();
                }
            }
        }

        // Fall back to directory name
        repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository")
            .to_string()
    }

    fn to_relative_string(&self, path: &Path, root: &Path) -> String {
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }
}

// ============ Helper Structures ============

#[derive(Debug, Clone, Default)]
struct FileClassification {
    total_files: usize,
    source_code: Vec<String>,
    documentation: Vec<String>,
    tests: Vec<String>,
    examples: Vec<String>,
    components: Vec<ComponentInfo>,
    documents: Vec<DocumentInfo>,
    detected_languages: Vec<String>,
    detected_frameworks: Vec<String>,
}

#[derive(Debug, Clone)]
struct ComponentInfo {
    name: String,
    relative_path: String,
    kind: String,
}

#[derive(Debug, Clone)]
struct DocumentInfo {
    path: String,
    title: String,
    preview: String,
}

#[derive(Debug, Clone)]
struct IntentAnalysis {
    project_name: String,
    purpose_statement: String,
    project_domain: String,
    key_concepts: Vec<String>,
}

#[derive(Debug, Clone)]
struct SemanticAnalysis {
    architecture_pattern: String,
    complexity_estimate: String,
    key_abstractions: Vec<String>,
    critical_features: Vec<String>,
}

// ============ Helper Functions ============

fn classify_component_type(name: &str) -> String {
    match name {
        "src" => "source",
        "lib" => "library",
        "tests" | "test" => "tests",
        "docs" => "documentation",
        "examples" => "examples",
        "specs" => "specifications",
        "scripts" | "bin" => "executables",
        "config" => "configuration",
        _ => "other",
    }
    .to_string()
}

fn infer_domain(languages: &[String], frameworks: &[String]) -> String {
    let all_tech = [languages, frameworks].concat();
    let tech_str = all_tech.join(" ");

    if tech_str.contains("react") || tech_str.contains("vue") || tech_str.contains("angular") {
        "frontend".to_string()
    } else if tech_str.contains("django")
        || tech_str.contains("flask")
        || tech_str.contains("express")
        || tech_str.contains("actix")
    {
        "backend".to_string()
    } else if tech_str.contains("cli") || tech_str.contains("command") {
        "cli".to_string()
    } else if languages.contains(&"rust".to_string()) {
        "systems".to_string()
    } else {
        "general".to_string()
    }
}

fn infer_architecture_pattern(components: &[ComponentInfo]) -> String {
    let component_names = components
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();

    if component_names.contains(&"microservices") {
        "microservices".to_string()
    } else if component_names
        .iter()
        .filter(|c| c.contains("service"))
        .count()
        > 2
    {
        "service-oriented".to_string()
    } else if component_names.contains(&"api") {
        "api-driven".to_string()
    } else if component_names.contains(&"lib") {
        "library".to_string()
    } else {
        "modular".to_string()
    }
}

fn estimate_complexity(total_files: usize, code_files: usize) -> String {
    match (total_files, code_files) {
        (0..=50, _) => "low".to_string(),
        (51..=200, 0..=50) => "low-medium".to_string(),
        (51..=200, 51..=150) => "medium".to_string(),
        (201..=500, _) => "medium-high".to_string(),
        _ => "high".to_string(),
    }
}
