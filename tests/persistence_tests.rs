use serde_json::json;
use spec_ai::persistence::Persistence;
use spec_ai::types::MessageRole;
use tempfile::tempdir;

fn temp_db_path() -> std::path::PathBuf {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.duckdb");
    // Keep directory alive by leaking it for test duration to avoid drop before use
    Box::leak(Box::new(dir));
    path
}

#[test]
fn db_initializes_and_tables_exist() {
    let path = temp_db_path();
    let p = Persistence::new(&path).expect("init db");
    let conn = p.conn();
    // Should be able to query each table
    for table in ["messages", "memory_vectors", "tool_log", "policy_cache"].iter() {
        let sql = format!("SELECT COUNT(*) FROM {}", table);
        let mut stmt = conn.prepare(&sql).unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "table {} should start empty", table);
    }
}

#[test]
fn messages_insert_list_and_prune() {
    let path = temp_db_path();
    let p = Persistence::new(&path).unwrap();

    for i in 0..5 {
        let role = if i % 2 == 0 {
            MessageRole::User
        } else {
            MessageRole::Assistant
        };
        let _id = p.insert_message("s1", role, &format!("msg{}", i)).unwrap();
    }

    let last_two = p.list_messages("s1", 2).unwrap();
    assert_eq!(last_two.len(), 2);
    assert_eq!(last_two[0].content, "msg3");
    assert_eq!(last_two[1].content, "msg4");

    let deleted = p.prune_messages("s1", 3).unwrap();
    assert_eq!(deleted, 2);

    let remaining = p.list_messages("s1", 10).unwrap();
    assert_eq!(remaining.len(), 3);
    assert_eq!(remaining[0].content, "msg2");
    assert_eq!(remaining[2].content, "msg4");
}

#[test]
fn get_message_by_id() {
    let path = temp_db_path();
    let p = Persistence::new(&path).unwrap();

    let id = p
        .insert_message("sess", MessageRole::User, "stored")
        .unwrap();

    let fetched = p.get_message(id).unwrap().expect("message present");
    assert_eq!(fetched.content, "stored");
    assert_eq!(fetched.session_id, "sess");

    let missing = p.get_message(id + 1).unwrap();
    assert!(missing.is_none());
}

#[test]
fn memory_vectors_insert_and_recall() {
    let path = temp_db_path();
    let p = Persistence::new(&path).unwrap();
    // two vectors in different sessions and same
    let v1 = vec![1.0f32, 0.0, 0.0];
    let v2 = vec![0.0f32, 1.0, 0.0];
    let _m1 = p
        .insert_message("sess", MessageRole::User, "hello")
        .unwrap();
    let _id1 = p.insert_memory_vector("sess", None, &v1).unwrap();
    let _id2 = p.insert_memory_vector("sess", None, &v2).unwrap();

    let q = vec![1.0f32, 0.0, 0.0];
    let recalled = p.recall_top_k("sess", &q, 2).unwrap();
    assert_eq!(recalled.len(), 2);
    assert!(recalled[0].1 >= recalled[1].1);
    assert!(recalled[0]
        .0
        .embedding
        .iter()
        .zip(v1.iter())
        .all(|(a, b)| (*a - *b).abs() < 1e-6));
}

#[test]
fn tool_log_insert() {
    let path = temp_db_path();
    let p = Persistence::new(&path).unwrap();
    let args = json!({"path":"/tmp/a.txt"});
    let result = json!({"ok":true});
    let id = p
        .log_tool(
            "sess-1", "tester", "run-1", "FileTool", &args, &result, true, None,
        )
        .unwrap();
    assert!(id > 0);

    let conn = p.conn();
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM tool_log").unwrap();
    let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn policy_upsert_and_get() {
    let path = temp_db_path();
    let p = Persistence::new(&path).unwrap();
    let v1 = json!({"allow":["FileTool"]});
    p.policy_upsert("policies", &v1).unwrap();
    let got = p.policy_get("policies").unwrap().expect("exists");
    assert_eq!(got.value, v1);

    let v2 = json!({"allow":["FileTool","SearchTool"]});
    p.policy_upsert("policies", &v2).unwrap();
    let got2 = p.policy_get("policies").unwrap().expect("exists");
    assert_eq!(got2.value, v2);
}
