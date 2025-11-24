#[cfg(feature = "api")]
pub mod handlers;
#[cfg(feature = "api")]
pub mod mesh;
#[cfg(feature = "api")]
pub mod sync;
#[cfg(feature = "api")]
pub mod sync_handlers;
#[cfg(feature = "api")]
pub mod middleware;
#[cfg(feature = "api")]
pub mod models;
/// REST API and WebSocket server for programmatic agent access
///
/// This module provides:
/// - REST endpoints for agent interaction
/// - WebSocket streaming for real-time responses
/// - API key authentication
/// - JSON request/response format

#[cfg(feature = "api")]
pub mod server;

#[cfg(feature = "api")]
pub use models::{ErrorResponse, QueryRequest, QueryResponse, StreamChunk};
#[cfg(feature = "api")]
pub use server::{ApiConfig, ApiServer};
