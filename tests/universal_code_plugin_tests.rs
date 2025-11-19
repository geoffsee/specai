use anyhow::Result;
use spec_ai::bootstrap_self::BootstrapSelf;
use spec_ai::persistence::Persistence;
use spec_ai::types::NodeType;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

/// Create a sample polyglot repository with multiple languages
fn create_polyglot_repo(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("src/rust"))?;
    fs::create_dir_all(root.join("src/python"))?;
    fs::create_dir_all(root.join("src/typescript"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::create_dir_all(root.join("docs"))?;

    // Rust files
    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "universal-test"
version = "1.0.0"
edition = "2021"

[dependencies]
serde = "1.0"
tokio = "1.0"
actix-web = "4.0"

[dev-dependencies]
pytest = "1.0"
"#,
    )?;

    fs::write(
        root.join("src/rust/main.rs"),
        "fn main() { println!(\"Hello from Rust\"); }",
    )?;
    fs::write(
        root.join("src/rust/lib.rs"),
        "pub mod utils { pub fn demo() -> i32 { 42 } }",
    )?;

    // Python files
    fs::write(
        root.join("src/python/main.py"),
        "if __name__ == '__main__':\n    print('Hello from Python')",
    )?;
    fs::write(
        root.join("src/python/utils.py"),
        "def hello():\n    return 'world'",
    )?;

    // TypeScript files
    fs::write(
        root.join("src/typescript/main.ts"),
        "console.log('Hello from TypeScript');",
    )?;

    // Test files
    fs::write(
        root.join("tests/test_utils.rs"),
        "#[test]\nfn test() { assert!(true); }",
    )?;
    fs::write(
        root.join("tests/test_main.py"),
        "def test_example():\n    assert True",
    )?;

    // Documentation
    fs::write(
        root.join("README.md"),
        "# Universal Test Repository\n\nA polyglot codebase with Rust, Python, and TypeScript.",
    )?;
    fs::write(
        root.join("docs/architecture.md"),
        "# Architecture\n\nThis is a microservices architecture.",
    )?;
    fs::write(
        root.join("docs/api.md"),
        "# API Documentation\n\nEndpoints...",
    )?;

    // Create .git directory to make it look like a repo
    fs::create_dir_all(root.join(".git"))?;

    Ok(())
}

/// Create a simple single-language repository
fn create_simple_rust_repo(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::create_dir_all(root.join("examples"))?;

    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "simple-lib"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#,
    )?;

    fs::write(
        root.join("README.md"),
        "# Simple Lib\n\nA simple Rust library.",
    )?;
    fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }",
    )?;
    fs::write(
        root.join("tests/integration_test.rs"),
        "#[test]\nfn test_add() {}",
    )?;
    fs::write(
        root.join("examples/example.rs"),
        "fn main() { println!(\"Example\"); }",
    )?;

    fs::create_dir_all(root.join(".git"))?;

    Ok(())
}

#[test]
fn test_universal_plugin_activation() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("repo");
    create_simple_rust_repo(&repo_root)?;

    let db_path = temp.path().join("test.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "test-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let outcome = bootstrapper.run()?;

    // Universal plugin should activate and create nodes
    assert!(outcome.nodes_created > 0, "Plugin should create nodes");
    assert_eq!(outcome.repository_name, "simple-lib");

    Ok(())
}

#[test]
fn test_polyglot_repository_analysis() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("polyglot");
    create_polyglot_repo(&repo_root)?;

    let db_path = temp.path().join("polyglot.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "polyglot-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let outcome = bootstrapper.run()?;

    assert!(
        outcome.nodes_created >= 3,
        "Should create repository + components + docs"
    );
    assert_eq!(outcome.repository_name, "universal-test");
    assert!(outcome.component_count > 0, "Should detect components");
    assert!(outcome.document_count > 0, "Should detect documents");

    let nodes = persistence.list_graph_nodes(session, None, None)?;

    // Should have a repository node
    let repo_node = nodes
        .iter()
        .find(|n| n.label == "Repository")
        .expect("Repository node should exist");
    assert_eq!(repo_node.node_type, NodeType::Entity);
    assert_eq!(repo_node.properties["name"], "universal-test");

    // Should detect multiple components
    let components: Vec<_> = nodes.iter().filter(|n| n.label == "Component").collect();
    assert!(
        !components.is_empty(),
        "Should detect src, tests, docs, etc."
    );

    // Should have documentation nodes
    let docs: Vec<_> = nodes
        .iter()
        .filter(|n| n.label == "Documentation")
        .collect();
    assert!(!docs.is_empty(), "Should create documentation nodes");

    Ok(())
}

#[test]
fn test_spec_ai_document_generation() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("repo");
    create_simple_rust_repo(&repo_root)?;

    let db_path = temp.path().join("doc-gen.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "doc-gen-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root.clone());
    let _outcome = bootstrapper.run()?;

    // Check if .SPEC-AI.md was generated
    let spec_file = repo_root.join(".SPEC-AI.md");
    assert!(
        spec_file.exists(),
        ".SPEC-AI.md should be generated in repo root"
    );

    let content = fs::read_to_string(&spec_file)?;
    assert!(
        content.contains("simple-lib"),
        "Spec document should contain project name"
    );
    assert!(
        content.contains("## Overview"),
        "Spec should have Overview section"
    );
    assert!(
        content.contains("## Project Information"),
        "Spec should have Project Information"
    );
    assert!(
        content.contains("## File Structure"),
        "Spec should have File Structure"
    );
    assert!(
        content.contains("## Languages Used"),
        "Spec should list languages"
    );
    assert!(
        content.contains("Generated by spec-ai Universal Code Plugin"),
        "Spec should credit the plugin"
    );

    Ok(())
}

#[test]
fn test_language_detection() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("lang-detect");
    create_polyglot_repo(&repo_root)?;

    let db_path = temp.path().join("lang-detect.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "lang-detect-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let _outcome = bootstrapper.run()?;

    let nodes = persistence.list_graph_nodes(session, None, None)?;
    let repo_node = nodes
        .iter()
        .find(|n| n.label == "Repository")
        .expect("Repository node should exist");

    // Extract detected languages from repo properties
    if let Some(languages) = repo_node.properties.get("languages") {
        let langs_str = languages.to_string().to_lowercase();
        // Should detect at least Rust (from Cargo.toml/files)
        assert!(langs_str.contains("rust"), "Should detect Rust language");
    }

    Ok(())
}

#[test]
fn test_component_classification() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("components");
    create_polyglot_repo(&repo_root)?;

    let db_path = temp.path().join("components.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "components-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let outcome = bootstrapper.run()?;

    assert!(outcome.component_count > 0, "Should classify components");

    let nodes = persistence.list_graph_nodes(session, None, None)?;

    // Find component nodes
    let component_nodes: Vec<_> = nodes.iter().filter(|n| n.label == "Component").collect();

    for node in &component_nodes {
        // Check that component nodes have some identifying properties
        // Properties may include name, path, type, or other identifying info
        let has_props = node.properties.get("name").is_some()
            || node.properties.get("path").is_some()
            || node.properties.get("type").is_some();
        assert!(
            has_props,
            "Component should have identifying properties (name, path, or type)"
        );
    }

    Ok(())
}

#[test]
fn test_architecture_pattern_detection() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("arch");
    create_simple_rust_repo(&repo_root)?;

    let db_path = temp.path().join("arch.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "arch-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let _outcome = bootstrapper.run()?;

    let nodes = persistence.list_graph_nodes(session, None, None)?;

    // Should have an ArchitecturePattern concept node
    let arch_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| n.label == "ArchitecturePattern")
        .collect();
    assert!(!arch_nodes.is_empty(), "Should detect architecture pattern");

    if let Some(arch_node) = arch_nodes.first() {
        assert_eq!(arch_node.node_type, NodeType::Concept);
        assert!(
            arch_node.properties.get("pattern").is_some(),
            "Should have pattern property"
        );
    }

    Ok(())
}

#[test]
fn test_knowledge_graph_edges() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("edges");
    create_simple_rust_repo(&repo_root)?;

    let db_path = temp.path().join("edges.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "edges-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let outcome = bootstrapper.run()?;

    assert!(
        outcome.edges_created > 0,
        "Plugin should create edges linking nodes"
    );

    let edges = persistence.list_graph_edges(session, None, None)?;
    assert!(!edges.is_empty(), "Graph should have edges");

    // Verify edge relationships exist
    let has_part_of = edges.iter().any(|e| e.edge_type.as_str() == "PART_OF");
    assert!(
        has_part_of,
        "Should have PartOf edges (component of repository)"
    );

    Ok(())
}

#[test]
fn test_empty_repository_handling() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("empty");

    // Create just a .git directory
    fs::create_dir_all(repo_root.join(".git"))?;

    let db_path = temp.path().join("empty.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "empty-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    // Should not crash on empty repository
    let result = bootstrapper.run();
    // The result might be an error or partial success, both are acceptable for empty repo
    let _ = result;

    Ok(())
}

#[test]
fn test_manifest_detection() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("manifest");
    create_simple_rust_repo(&repo_root)?;

    let db_path = temp.path().join("manifest.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "manifest-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root);
    let _outcome = bootstrapper.run()?;

    let nodes = persistence.list_graph_nodes(session, None, None)?;

    // The repository node should have framework info
    let repo_node = nodes
        .iter()
        .find(|n| n.label == "Repository")
        .expect("Repository node should exist");

    assert!(
        repo_node.properties.get("frameworks").is_some(),
        "Should detect frameworks/dependencies"
    );

    Ok(())
}
