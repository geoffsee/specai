use anyhow::Result;
use serde_json::json;
use spec_ai::persistence::Persistence;
use spec_ai::types::{EdgeType, NodeType, TraversalDirection};
use tempfile::tempdir;

#[test]
fn test_graph_node_operations() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let persistence = Persistence::new(&db_path)?;
    let session_id = "test_session";

    // Create a node
    let node_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Person",
        &json!({"name": "Alice", "age": 30}),
        None,
    )?;
    assert!(node_id > 0);

    // Get the node
    let node = persistence.get_graph_node(node_id)?;
    assert!(node.is_some());
    let node = node.unwrap();
    assert_eq!(node.label, "Person");
    assert_eq!(node.node_type, NodeType::Entity);
    assert_eq!(node.properties["name"], "Alice");

    // Update the node
    persistence.update_graph_node(node_id, &json!({"name": "Alice", "age": 31}))?;
    let updated_node = persistence.get_graph_node(node_id)?.unwrap();
    assert_eq!(updated_node.properties["age"], 31);

    // List nodes
    let nodes = persistence.list_graph_nodes(session_id, None, Some(10))?;
    assert_eq!(nodes.len(), 1);

    // List nodes by type
    let entity_nodes =
        persistence.list_graph_nodes(session_id, Some(NodeType::Entity), Some(10))?;
    assert_eq!(entity_nodes.len(), 1);

    let concept_nodes =
        persistence.list_graph_nodes(session_id, Some(NodeType::Concept), Some(10))?;
    assert_eq!(concept_nodes.len(), 0);

    // Delete the node
    persistence.delete_graph_node(node_id)?;
    let deleted_node = persistence.get_graph_node(node_id)?;
    assert!(deleted_node.is_none());

    Ok(())
}

#[test]
fn test_graph_edge_operations() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let persistence = Persistence::new(&db_path)?;
    let session_id = "test_session";

    // Create two nodes
    let alice_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Person",
        &json!({"name": "Alice"}),
        None,
    )?;
    let bob_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Person",
        &json!({"name": "Bob"}),
        None,
    )?;

    // Create an edge
    let edge_id = persistence.insert_graph_edge(
        session_id,
        alice_id,
        bob_id,
        EdgeType::RelatesTo,
        Some("knows"),
        Some(&json!({"since": "2020"})),
        1.0,
    )?;
    assert!(edge_id > 0);

    // Get the edge
    let edge = persistence.get_graph_edge(edge_id)?;
    assert!(edge.is_some());
    let edge = edge.unwrap();
    assert_eq!(edge.source_id, alice_id);
    assert_eq!(edge.target_id, bob_id);
    assert_eq!(edge.edge_type, EdgeType::RelatesTo);
    assert_eq!(edge.predicate, Some("knows".to_string()));

    // List edges
    let edges = persistence.list_graph_edges(session_id, Some(alice_id), None)?;
    assert_eq!(edges.len(), 1);

    let edges_to_bob = persistence.list_graph_edges(session_id, None, Some(bob_id))?;
    assert_eq!(edges_to_bob.len(), 1);

    // Delete the edge
    persistence.delete_graph_edge(edge_id)?;
    let deleted_edge = persistence.get_graph_edge(edge_id)?;
    assert!(deleted_edge.is_none());

    Ok(())
}

#[test]
fn test_graph_traversal() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let persistence = Persistence::new(&db_path)?;
    let session_id = "test_session";

    // Create a simple graph: A -> B -> C
    //                            -> D
    let a_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Node",
        &json!({"name": "A"}),
        None,
    )?;
    let b_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Node",
        &json!({"name": "B"}),
        None,
    )?;
    let c_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Node",
        &json!({"name": "C"}),
        None,
    )?;
    let d_id = persistence.insert_graph_node(
        session_id,
        NodeType::Entity,
        "Node",
        &json!({"name": "D"}),
        None,
    )?;

    // Create edges
    persistence.insert_graph_edge(session_id, a_id, b_id, EdgeType::DependsOn, None, None, 1.0)?;
    persistence.insert_graph_edge(session_id, b_id, c_id, EdgeType::DependsOn, None, None, 1.0)?;
    persistence.insert_graph_edge(session_id, b_id, d_id, EdgeType::DependsOn, None, None, 1.0)?;

    // Test neighbor traversal from A (depth 1)
    let neighbors_1 =
        persistence.traverse_neighbors(session_id, a_id, TraversalDirection::Outgoing, 1)?;
    assert_eq!(neighbors_1.len(), 1); // Only B

    // Test neighbor traversal from A (depth 2)
    let neighbors_2 =
        persistence.traverse_neighbors(session_id, a_id, TraversalDirection::Outgoing, 2)?;
    assert_eq!(neighbors_2.len(), 3); // B, C, D

    // Test incoming traversal from C
    let incoming =
        persistence.traverse_neighbors(session_id, c_id, TraversalDirection::Incoming, 1)?;
    assert_eq!(incoming.len(), 1); // Only B

    // Test shortest path from A to C
    let path = persistence.find_shortest_path(session_id, a_id, c_id, None)?;
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path.length, 2); // 2 edges: A->B->C
    assert_eq!(path.nodes.len(), 3); // 3 nodes: A, B, C

    // Test shortest path from A to D
    let path_to_d = persistence.find_shortest_path(session_id, a_id, d_id, None)?;
    assert!(path_to_d.is_some());
    let path_to_d = path_to_d.unwrap();
    assert_eq!(path_to_d.length, 2); // 2 edges: A->B->D

    // Test no path exists (from C to A with outgoing edges only)
    let no_path = persistence.find_shortest_path(session_id, c_id, a_id, None)?;
    assert!(no_path.is_none());

    Ok(())
}

#[test]
fn test_edge_types() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let persistence = Persistence::new(&db_path)?;
    let session_id = "test_session";

    // Create nodes
    let node1 = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Concept",
        &json!({"name": "AI"}),
        None,
    )?;
    let node2 = persistence.insert_graph_node(
        session_id,
        NodeType::Concept,
        "Concept",
        &json!({"name": "Machine Learning"}),
        None,
    )?;

    // Test different edge types
    let edge_types = vec![
        EdgeType::RelatesTo,
        EdgeType::CausedBy,
        EdgeType::PartOf,
        EdgeType::Mentions,
        EdgeType::FollowsFrom,
        EdgeType::Uses,
        EdgeType::Produces,
        EdgeType::DependsOn,
        EdgeType::Custom("CUSTOM_TYPE".to_string()),
    ];

    for edge_type in edge_types {
        let edge_id = persistence.insert_graph_edge(
            session_id,
            node1,
            node2,
            edge_type.clone(),
            None,
            None,
            1.0,
        )?;

        let edge = persistence.get_graph_edge(edge_id)?.unwrap();
        assert_eq!(edge.edge_type, edge_type);

        // Clean up
        persistence.delete_graph_edge(edge_id)?;
    }

    Ok(())
}

#[test]
fn test_graph_isolation_by_session() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let persistence = Persistence::new(&db_path)?;

    // Create nodes in different sessions
    let session1 = "session1";
    let session2 = "session2";

    let node1_s1 = persistence.insert_graph_node(
        session1,
        NodeType::Entity,
        "Entity",
        &json!({"session": 1}),
        None,
    )?;
    let node2_s1 = persistence.insert_graph_node(
        session1,
        NodeType::Entity,
        "Entity",
        &json!({"session": 1}),
        None,
    )?;

    let _node1_s2 = persistence.insert_graph_node(
        session2,
        NodeType::Entity,
        "Entity",
        &json!({"session": 2}),
        None,
    )?;

    // Create edges in different sessions
    persistence.insert_graph_edge(
        session1,
        node1_s1,
        node2_s1,
        EdgeType::RelatesTo,
        None,
        None,
        1.0,
    )?;

    // List nodes - should be isolated by session
    let nodes_s1 = persistence.list_graph_nodes(session1, None, None)?;
    assert_eq!(nodes_s1.len(), 2);

    let nodes_s2 = persistence.list_graph_nodes(session2, None, None)?;
    assert_eq!(nodes_s2.len(), 1);

    // List edges - should be isolated by session
    let edges_s1 = persistence.list_graph_edges(session1, None, None)?;
    assert_eq!(edges_s1.len(), 1);

    let edges_s2 = persistence.list_graph_edges(session2, None, None)?;
    assert_eq!(edges_s2.len(), 0);

    Ok(())
}
