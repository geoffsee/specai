use crate::api::handlers::AppState;
use crate::api::sync::{GraphSyncPayload, SyncType};
use crate::sync::{SyncEngine, VectorClock};
use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

/// Request to initiate a sync
#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub session_id: String,
    pub graph_name: Option<String>,
    pub requesting_instance: String,
    pub vector_clock: Option<String>,
}

/// Response from a sync request
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub success: bool,
    pub message: String,
    pub payload: Option<GraphSyncPayload>,
}

/// Status of sync for a graph
#[derive(Debug, Serialize)]
pub struct SyncStatus {
    pub session_id: String,
    pub graph_name: String,
    pub sync_enabled: bool,
    pub vector_clock: String,
    pub last_sync_at: Option<String>,
    pub pending_changes: usize,
}

/// Request to enable/disable sync
#[derive(Debug, Deserialize)]
pub struct SyncToggleRequest {
    pub enabled: bool,
}

/// Conflict information
#[derive(Debug, Serialize)]
pub struct ConflictInfo {
    pub session_id: String,
    pub entity_type: String,
    pub entity_id: i64,
    pub local_version: String,
    pub remote_version: String,
    pub detected_at: String,
}

/// Handle sync request from a peer
pub async fn handle_sync_request(
    State(state): State<AppState>,
    Json(request): Json<SyncRequest>,
) -> impl IntoResponse {
    let persistence = state.persistence.clone();
    let instance_id = crate::api::mesh::MeshClient::generate_instance_id();
    let sync_engine = SyncEngine::new(persistence.clone(), instance_id);

    // Parse their vector clock
    let their_vc = if let Some(ref vc_str) = request.vector_clock {
        match VectorClock::from_json(vc_str) {
            Ok(vc) => vc,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(SyncResponse {
                        success: false,
                        message: format!("Invalid vector clock: {}", e),
                        payload: None,
                    }),
                )
            }
        }
    } else {
        VectorClock::new()
    };

    // Decide sync strategy
    let sync_type = match sync_engine
        .decide_sync_strategy(
            &request.session_id,
            request.graph_name.as_deref().unwrap_or("default"),
            &their_vc,
        )
        .await
    {
        Ok(st) => st,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SyncResponse {
                    success: false,
                    message: format!("Failed to determine sync strategy: {}", e),
                    payload: None,
                }),
            )
        }
    };

    // Perform sync based on strategy
    let payload = match sync_type {
        SyncType::Full => {
            match sync_engine
                .sync_full(
                    &request.session_id,
                    request.graph_name.as_deref().unwrap_or("default"),
                )
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(SyncResponse {
                            success: false,
                            message: format!("Full sync failed: {}", e),
                            payload: None,
                        }),
                    )
                }
            }
        }
        SyncType::Incremental => {
            match sync_engine
                .sync_incremental(
                    &request.session_id,
                    request.graph_name.as_deref().unwrap_or("default"),
                    &their_vc,
                )
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(SyncResponse {
                            success: false,
                            message: format!("Incremental sync failed: {}", e),
                            payload: None,
                        }),
                    )
                }
            }
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(SyncResponse {
                    success: false,
                    message: "Unsupported sync type".to_string(),
                    payload: None,
                }),
            )
        }
    };

    (
        StatusCode::OK,
        Json(SyncResponse {
            success: true,
            message: format!("{:?} sync completed", sync_type),
            payload: Some(payload),
        }),
    )
}

/// Apply incoming sync data
pub async fn handle_sync_apply(
    State(state): State<AppState>,
    Json(payload): Json<GraphSyncPayload>,
) -> impl IntoResponse {
    let persistence = state.persistence.clone();
    let instance_id = crate::api::mesh::MeshClient::generate_instance_id();
    let sync_engine = SyncEngine::new(persistence.clone(), instance_id);

    let graph_name = payload.graph_name.as_deref().unwrap_or("default");

    match sync_engine.apply_sync(&payload, graph_name).await {
        Ok(stats) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": "Sync applied successfully",
                "stats": {
                    "nodes_applied": stats.nodes_applied,
                    "edges_applied": stats.edges_applied,
                    "tombstones_applied": stats.tombstones_applied,
                    "conflicts_detected": stats.conflicts_detected,
                    "conflicts_resolved": stats.conflicts_resolved,
                    "sync_type": stats.sync_type
                }
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Failed to apply sync: {}", e)
            })),
        ),
    }
}

/// Get sync status for a graph
pub async fn get_sync_status(
    State(state): State<AppState>,
    Path((session_id, graph_name)): Path<(String, String)>,
) -> impl IntoResponse {
    let persistence = &state.persistence;
    let instance_id = crate::api::mesh::MeshClient::generate_instance_id();

    // Check if sync is enabled
    let sync_enabled = match persistence.graph_get_sync_enabled(&session_id, &graph_name) {
        Ok(enabled) => enabled,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to get sync status: {}", e)
                })),
            ).into_response()
        }
    };

    // Get vector clock
    let vector_clock = match persistence.graph_sync_state_get(&instance_id, &session_id, &graph_name) {
        Ok(Some(vc)) => vc,
        Ok(None) => "{}".to_string(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to get vector clock: {}", e)
                })),
            ).into_response()
        }
    };

    // Count pending changes (approximate)
    let since_timestamp = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(1))
        .unwrap()
        .to_rfc3339();

    let pending_changes = match persistence.graph_changelog_get_since(&session_id, &since_timestamp) {
        Ok(entries) => entries.len(),
        Err(_) => 0,
    };

    (
        StatusCode::OK,
        Json(SyncStatus {
            session_id,
            graph_name,
            sync_enabled,
            vector_clock,
            last_sync_at: None, // TODO: Track this
            pending_changes,
        }),
    ).into_response()
}

/// Enable or disable sync for a graph
pub async fn toggle_sync(
    State(state): State<AppState>,
    Path((session_id, graph_name)): Path<(String, String)>,
    Json(request): Json<SyncToggleRequest>,
) -> impl IntoResponse {
    let persistence = &state.persistence;

    match persistence.graph_set_sync_enabled(&session_id, &graph_name, request.enabled) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": format!("Sync {} for graph {}/{}",
                    if request.enabled { "enabled" } else { "disabled" },
                    session_id, graph_name),
                "enabled": request.enabled
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Failed to toggle sync: {}", e)
            })),
        ),
    }
}

/// List all graphs with their sync status
pub async fn list_sync_configs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let persistence = &state.persistence;

    // Get all graphs for this session
    match persistence.graph_list(&session_id) {
        Ok(graphs) => {
            let mut configs = Vec::new();
            for graph_name in graphs {
                let sync_enabled = persistence
                    .graph_get_sync_enabled(&session_id, &graph_name)
                    .unwrap_or(false);

                configs.push(serde_json::json!({
                    "graph_name": graph_name,
                    "sync_enabled": sync_enabled,
                }));
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "session_id": session_id,
                    "graphs": configs
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Failed to list sync configs: {}", e)
            })),
        ),
    }
}

/// Bulk enable/disable sync for multiple graphs
#[derive(Debug, Deserialize)]
pub struct BulkSyncRequest {
    pub graphs: Vec<String>,
    pub enabled: bool,
}

pub async fn bulk_toggle_sync(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<BulkSyncRequest>,
) -> impl IntoResponse {
    let persistence = &state.persistence;
    let mut results = Vec::new();
    let mut failed = Vec::new();

    for graph_name in &request.graphs {
        match persistence.graph_set_sync_enabled(&session_id, graph_name, request.enabled) {
            Ok(_) => results.push(graph_name.clone()),
            Err(e) => failed.push(serde_json::json!({
                "graph": graph_name,
                "error": e.to_string()
            })),
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": failed.is_empty(),
            "message": format!("Sync {} for {} graphs",
                if request.enabled { "enabled" } else { "disabled" },
                results.len()),
            "updated": results,
            "failed": failed
        })),
    )
}

/// Configure sync parameters for a graph
#[derive(Debug, Deserialize)]
pub struct SyncConfig {
    pub sync_enabled: bool,
    pub conflict_resolution_strategy: Option<String>, // "vector_clock", "last_write_wins", "manual"
    pub sync_interval_seconds: Option<u64>,
}

pub async fn configure_sync(
    State(state): State<AppState>,
    Path((session_id, graph_name)): Path<(String, String)>,
    Json(config): Json<SyncConfig>,
) -> impl IntoResponse {
    let persistence = &state.persistence;

    // First set the enabled status
    match persistence.graph_set_sync_enabled(&session_id, &graph_name, config.sync_enabled) {
        Ok(_) => {
            // TODO: Store additional configuration parameters
            // For now, we'll just acknowledge them
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "message": format!("Sync configuration updated for graph {}/{}", session_id, graph_name),
                    "config": {
                        "sync_enabled": config.sync_enabled,
                        "conflict_resolution_strategy": config.conflict_resolution_strategy.unwrap_or_else(|| "vector_clock".to_string()),
                        "sync_interval_seconds": config.sync_interval_seconds.unwrap_or(60),
                    }
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "message": format!("Failed to configure sync: {}", e)
            })),
        ),
    }
}

/// List unresolved conflicts
pub async fn list_conflicts(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    // TODO: Implement conflict tracking
    // For now, return empty list
    let conflicts: Vec<ConflictInfo> = Vec::new();

    (StatusCode::OK, Json(conflicts))
}