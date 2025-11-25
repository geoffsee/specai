use std::sync::{Mutex, OnceLock};

/// Global test utilities
///
/// Provides a process-wide mutex to serialize tests that mutate process-wide
/// state (like environment variables). Use this to avoid flaky tests when
/// `cargo test` runs tests in parallel.
pub fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn create_test_db() -> crate::persistence::Persistence {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    // Leak the temp dir to keep it alive for the test duration
    std::mem::forget(dir);
    crate::persistence::Persistence::new(&db_path).unwrap()
}