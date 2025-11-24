/// HTTP server implementation
use crate::api::handlers::{health_check, list_agents, query, stream_query, AppState};
use crate::api::mesh::{
    acknowledge_messages, deregister_instance, get_messages, heartbeat, list_instances,
    register_instance, send_message,
};
use crate::api::sync_handlers::{
    bulk_toggle_sync, configure_sync, get_sync_status, handle_sync_apply, handle_sync_request,
    list_conflicts, list_sync_configs, toggle_sync,
};
use crate::config::{AgentRegistry, AppConfig};
use crate::persistence::Persistence;
use crate::tools::ToolRegistry;
use anyhow::Result;
use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Server host address
    pub host: String,
    /// Server port
    pub port: u16,
    /// Optional API key for authentication
    pub api_key: Option<String>,
    /// Enable CORS
    pub enable_cors: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            api_key: None,
            enable_cors: true,
        }
    }
}

impl ApiConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_cors(mut self, enable: bool) -> Self {
        self.enable_cors = enable;
        self
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// API server
pub struct ApiServer {
    config: ApiConfig,
    state: AppState,
}

impl ApiServer {
    /// Create a new API server
    pub fn new(
        config: ApiConfig,
        persistence: Persistence,
        agent_registry: Arc<AgentRegistry>,
        tool_registry: Arc<ToolRegistry>,
        app_config: AppConfig,
    ) -> Self {
        let state = AppState::new(persistence, agent_registry, tool_registry, app_config);

        Self { config, state }
    }

    /// Get the mesh registry for self-registration
    pub fn mesh_registry(&self) -> &crate::api::mesh::MeshRegistry {
        &self.state.mesh_registry
    }

    /// Build the router with all routes
    fn build_router(&self) -> Router {
        let mut router = Router::new()
            // Health and info endpoints
            .route("/health", get(health_check))
            .route("/agents", get(list_agents))
            // Query endpoints
            .route("/query", post(query))
            .route("/stream", post(stream_query))
            // Mesh registry endpoints
            .route("/registry/register", post(register_instance::<AppState>))
            .route("/registry/agents", get(list_instances::<AppState>))
            .route("/registry/heartbeat/:instance_id", post(heartbeat::<AppState>))
            .route("/registry/deregister/:instance_id", delete(deregister_instance::<AppState>))
            // Message routing endpoints
            .route("/messages/send/:source_instance", post(send_message::<AppState>))
            .route("/messages/:instance_id", get(get_messages::<AppState>))
            .route("/messages/ack/:instance_id", post(acknowledge_messages::<AppState>))
            // Graph sync endpoints
            .route("/sync/request", post(handle_sync_request))
            .route("/sync/apply", post(handle_sync_apply))
            .route("/sync/status/:session_id/:graph_name", get(get_sync_status))
            .route("/sync/enable/:session_id/:graph_name", post(toggle_sync))
            .route("/sync/configs/:session_id", get(list_sync_configs))
            .route("/sync/bulk/:session_id", post(bulk_toggle_sync))
            .route("/sync/configure/:session_id/:graph_name", post(configure_sync))
            .route("/sync/conflicts", get(list_conflicts))
            // Add state
            .with_state(self.state.clone());

        // Add CORS if enabled
        if self.config.enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            router = router.layer(cors);
        }

        // Add tracing
        router = router.layer(TraceLayer::new_for_http());

        router
    }

    /// Run the server
    pub async fn run(self) -> Result<()> {
        let app = self.build_router();
        let bind_addr = self.config.bind_address();

        tracing::debug!("Starting API server on {}", bind_addr);

        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

        axum::serve(listener, app)
            .await
            .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

        Ok(())
    }

    /// Run the server with graceful shutdown
    pub async fn run_with_shutdown(
        self,
        shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<()> {
        let app = self.build_router();
        let bind_addr = self.config.bind_address();

        tracing::debug!("Starting API server on {}", bind_addr);

        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_config_default() {
        let config = ApiConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
        assert!(config.api_key.is_none());
        assert!(config.enable_cors);
    }

    #[test]
    fn test_api_config_builder() {
        let config = ApiConfig::new()
            .with_host("0.0.0.0")
            .with_port(8080)
            .with_api_key("secret123")
            .with_cors(false);

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.api_key, Some("secret123".to_string()));
        assert!(!config.enable_cors);
    }

    #[test]
    fn test_bind_address() {
        let config = ApiConfig::new().with_host("localhost").with_port(5000);

        assert_eq!(config.bind_address(), "localhost:5000");
    }
}
