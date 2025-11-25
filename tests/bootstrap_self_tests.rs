use anyhow::Result;
use serde_json::json;
use spec_ai::bootstrap_self::BootstrapSelf;
use spec_ai::persistence::Persistence;
use spec_ai::types::NodeType;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn create_sample_repo(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("docs"))?;
    fs::create_dir_all(root.join("spec"))?;

    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "bootstrap-fixture"
version = "0.2.0"
edition = "2021"
description = "Fixture repo"

[dependencies]
serde = "1.0"
tokio = { version = "1", features = ["rt"] }
"#,
    )?;

    fs::write(root.join("README.md"), "# Fixture Repo\n\nPriming test.")?;
    fs::write(root.join("src/lib.rs"), "pub fn demo() -> usize { 42 }\n")?;
    fs::write(root.join("docs/overview.md"), "Docs overview line")?;
    fs::write(root.join("spec/sample.spec"), "name = \"demo\"\n")?;
    Ok(())
}

#[test]
fn test_bootstrap_self_creates_graph_artifacts() -> Result<()> {
    let temp = tempdir()?;
    let repo_root = temp.path().join("repo");
    create_sample_repo(&repo_root)?;

    let db_path = temp.path().join("bootstrap.db");
    let persistence = Persistence::new(&db_path)?;
    let session = "bootstrap-session";

    let bootstrapper = BootstrapSelf::new(&persistence, session, repo_root.clone());
    let outcome = bootstrapper.run()?;

    assert!(outcome.nodes_created >= 3);
    assert!(outcome.edges_created >= 2);
    assert_eq!(outcome.repository_name, "bootstrap-fixture");
    assert!(outcome.component_count >= 1);
    assert!(outcome.document_count >= 1);

    let nodes = persistence.list_graph_nodes(session, None, None)?;
    let repo_node = nodes
        .iter()
        .find(|n| n.label == "Repository")
        .expect("repository node missing");
    assert_eq!(repo_node.properties["name"], "bootstrap-fixture");
    assert_eq!(repo_node.node_type, NodeType::Entity);

    let doc_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| n.label == "RepositoryDocument")
        .collect();
    assert!(!doc_nodes.is_empty());

    let manifest_node = nodes
        .iter()
        .find(|n| n.label == "CargoManifest")
        .expect("manifest node missing");
    assert!(manifest_node.properties["dependencies"].is_array());
    assert!(manifest_node.properties["bootstrap_source"] == json!("rust-cargo-plugin"));

    Ok(())
}
