/// Mesh registry handlers and models
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use crate::persistence::Persistence;

/// Agent instance information in the mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshInstance {
    pub instance_id: String,
    pub hostname: String,
    pub port: u16,
    pub capabilities: Vec<String>,
    pub is_leader: bool,
    pub last_heartbeat: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub agent_profiles: Vec<String>,
}

/// Request to register a new instance
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub instance_id: String,
    pub hostname: String,
    pub port: u16,
    pub capabilities: Vec<String>,
    pub agent_profiles: Vec<String>,
}

/// Response from registration
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub success: bool,
    pub instance_id: String,
    pub is_leader: bool,
    pub leader_id: Option<String>,
    pub peers: Vec<MeshInstance>,
}

/// List of registered instances
#[derive(Debug, Serialize, Deserialize)]
pub struct InstancesResponse {
    pub instances: Vec<MeshInstance>,
    pub leader_id: Option<String>,
}

/// Heartbeat request
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub status: String,
    pub metrics: Option<HashMap<String, serde_json::Value>>,
}

/// Heartbeat response
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub acknowledged: bool,
    pub leader_id: Option<String>,
    pub should_sync: bool,
}

/// Message types for inter-agent communication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    Query,          // Request information from another agent
    Response,       // Response to a query
    Notification,   // One-way notification
    TaskDelegation, // Delegate a task to another agent
    TaskResult,     // Result of a delegated task
    GraphSync,      // Knowledge graph synchronization
    Custom(String), // Custom message type
}

impl MessageType {
    pub fn as_str(&self) -> String {
        match self {
            MessageType::Query => "query".to_string(),
            MessageType::Response => "response".to_string(),
            MessageType::Notification => "notification".to_string(),
            MessageType::TaskDelegation => "task_delegation".to_string(),
            MessageType::TaskResult => "task_result".to_string(),
            MessageType::GraphSync => "graph_sync".to_string(),
            MessageType::Custom(s) => s.clone(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "query" => MessageType::Query,
            "response" => MessageType::Response,
            "notification" => MessageType::Notification,
            "task_delegation" => MessageType::TaskDelegation,
            "task_result" => MessageType::TaskResult,
            "graph_sync" => MessageType::GraphSync,
            custom => MessageType::Custom(custom.to_string()),
        }
    }
}

/// Inter-agent message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub message_id: String,
    pub source_instance: String,
    pub target_instance: Option<String>, // None for broadcast
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>, // For request/response correlation
    pub created_at: DateTime<Utc>,
}

/// Message send request
#[derive(Debug, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub target_instance: Option<String>, // None for broadcast
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
}

/// Message send response
#[derive(Debug, Serialize, Deserialize)]
pub struct SendMessageResponse {
    pub message_id: String,
    pub status: String,
    pub delivered_to: Vec<String>,
}

/// Pending messages response
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingMessagesResponse {
    pub messages: Vec<AgentMessage>,
}

/// Mesh registry state
#[derive(Clone)]
pub struct MeshRegistry {
    instances: Arc<RwLock<HashMap<String, MeshInstance>>>,
    leader_id: Arc<RwLock<Option<String>>>,
    message_queue: Arc<RwLock<Vec<AgentMessage>>>,
    persistence: Option<Persistence>,
}

impl MeshRegistry {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            leader_id: Arc::new(RwLock::new(None)),
            message_queue: Arc::new(RwLock::new(Vec::new())),
            persistence: None,
        }
    }

    pub fn with_persistence(persistence: Persistence) -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            leader_id: Arc::new(RwLock::new(None)),
            message_queue: Arc::new(RwLock::new(Vec::new())),
            persistence: Some(persistence),
        }
    }

    /// Register a new instance
    pub async fn register(&self, instance: MeshInstance) -> RegisterResponse {
        let mut instances = self.instances.write().await;
        let mut leader = self.leader_id.write().await;

        // First instance becomes the leader
        let is_leader = instances.is_empty();
        let mut new_instance = instance.clone();
        new_instance.is_leader = is_leader;

        if is_leader {
            *leader = Some(instance.instance_id.clone());
        }

        instances.insert(instance.instance_id.clone(), new_instance);

        RegisterResponse {
            success: true,
            instance_id: instance.instance_id.clone(),
            is_leader,
            leader_id: leader.clone(),
            peers: instances.values().cloned().collect(),
        }
    }

    /// Update heartbeat timestamp
    pub async fn heartbeat(&self, instance_id: &str) -> HeartbeatResponse {
        let mut instances = self.instances.write().await;
        let leader = self.leader_id.read().await;

        if let Some(instance) = instances.get_mut(instance_id) {
            instance.last_heartbeat = Utc::now();
            HeartbeatResponse {
                acknowledged: true,
                leader_id: leader.clone(),
                should_sync: false,
            }
        } else {
            HeartbeatResponse {
                acknowledged: false,
                leader_id: leader.clone(),
                should_sync: false,
            }
        }
    }

    /// Remove an instance
    pub async fn deregister(&self, instance_id: &str) -> bool {
        let mut instances = self.instances.write().await;
        let mut leader = self.leader_id.write().await;

        if let Some(instance) = instances.remove(instance_id) {
            // If leader is leaving, elect a new one
            if instance.is_leader && !instances.is_empty() {
                // Simple election: first remaining instance becomes leader
                if let Some((new_leader_id, new_leader)) = instances.iter_mut().next() {
                    new_leader.is_leader = true;
                    *leader = Some(new_leader_id.clone());
                }
            } else if instances.is_empty() {
                *leader = None;
            }
            true
        } else {
            false
        }
    }

    /// Get all instances
    pub async fn list(&self) -> Vec<MeshInstance> {
        let instances = self.instances.read().await;
        instances.values().cloned().collect()
    }

    /// Check for stale instances and remove them
    pub async fn cleanup_stale(&self, timeout_secs: u64) {
        let now = Utc::now();
        let mut instances = self.instances.write().await;
        let mut leader = self.leader_id.write().await;

        let stale_ids: Vec<String> = instances
            .iter()
            .filter_map(|(id, instance)| {
                let elapsed = now.timestamp() - instance.last_heartbeat.timestamp();
                if elapsed > timeout_secs as i64 {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        for id in stale_ids {
            if let Some(instance) = instances.remove(&id) {
                // Handle leader failover if needed
                if instance.is_leader && !instances.is_empty() {
                    if let Some((new_leader_id, new_leader)) = instances.iter_mut().next() {
                        new_leader.is_leader = true;
                        *leader = Some(new_leader_id.clone());
                    }
                }
            }
        }

        if instances.is_empty() {
            *leader = None;
        }
    }

    /// Get the current leader ID
    pub async fn get_leader(&self) -> Option<String> {
        let leader = self.leader_id.read().await;
        leader.clone()
    }

    /// Send a message to an instance or broadcast
    pub async fn send_message(
        &self,
        source_instance: String,
        target_instance: Option<String>,
        message_type: MessageType,
        payload: serde_json::Value,
        correlation_id: Option<String>,
    ) -> Result<SendMessageResponse> {
        // Generate time-ordered UUID v7 for better database performance and distributed safety
        let message_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();

        let message = AgentMessage {
            message_id: message_id.clone(),
            source_instance,
            target_instance: target_instance.clone(),
            message_type,
            payload,
            correlation_id,
            created_at: Utc::now(),
        };

        // Persist to database if available
        if let Some(ref persistence) = self.persistence {
            let target_str = target_instance.as_deref();
            if let Err(e) = persistence.mesh_message_store(
                &message_id,
                &message.source_instance,
                target_str,
                &message.message_type.as_str(),
                &message.payload,
                "pending",
            ) {
                tracing::warn!("Failed to persist mesh message: {}", e);
            }
        }

        // Add to message queue
        let mut queue = self.message_queue.write().await;
        queue.push(message.clone());

        // GraphSync messages are handled when retrieved from the queue
        // to avoid recursion issues

        // Determine who received it
        let delivered_to = if let Some(ref target) = target_instance {
            let instances = self.instances.read().await;
            if instances.contains_key(target) {
                vec![target.clone()]
            } else {
                return Err(anyhow::anyhow!("Target instance '{}' not found", target));
            }
        } else {
            // Broadcast - delivered to all instances
            let instances = self.instances.read().await;
            instances.keys().cloned().collect()
        };

        Ok(SendMessageResponse {
            message_id,
            status: "queued".to_string(),
            delivered_to,
        })
    }

    /// Get pending messages for an instance
    pub async fn get_pending_messages(&self, instance_id: &str) -> Vec<AgentMessage> {
        let queue = self.message_queue.read().await;
        queue
            .iter()
            .filter(|msg| {
                // Return messages targeted at this instance or broadcasts (None)
                msg.target_instance.as_deref() == Some(instance_id)
                    || msg.target_instance.is_none()
            })
            .cloned()
            .collect()
    }

    /// Acknowledge/remove messages after delivery
    pub async fn acknowledge_messages(&self, message_ids: Vec<String>) {
        let mut queue = self.message_queue.write().await;
        queue.retain(|msg| !message_ids.contains(&msg.message_id));
    }

}

/// Client-side mesh operations
#[derive(Clone)]
pub struct MeshClient {
    base_url: String,
    client: reqwest::Client,
}

impl MeshClient {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            base_url: format!("http://{}:{}", host, port),
            client: reqwest::Client::new(),
        }
    }

    /// Generate a unique instance ID
    pub fn generate_instance_id() -> String {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());
        // Use UUID v7 for time-ordered, globally unique IDs with better collision resistance
        let uuid = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        format!("{}-{}", hostname, uuid)
    }

    /// Register this instance with a mesh registry
    pub async fn register(
        &self,
        instance_id: String,
        hostname: String,
        port: u16,
        capabilities: Vec<String>,
        agent_profiles: Vec<String>,
    ) -> Result<RegisterResponse> {
        let request = RegisterRequest {
            instance_id,
            hostname,
            port,
            capabilities,
            agent_profiles,
        };

        let response = self
            .client
            .post(format!("{}/registry/register", self.base_url))
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            anyhow::bail!("Registration failed: {}", response.status())
        }
    }

    /// Send heartbeat to registry
    pub async fn heartbeat(
        &self,
        instance_id: &str,
        metrics: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<HeartbeatResponse> {
        let request = HeartbeatRequest {
            status: "healthy".to_string(),
            metrics,
        };

        let response = self
            .client
            .post(format!("{}/registry/heartbeat/{}", self.base_url, instance_id))
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            anyhow::bail!("Heartbeat failed: {}", response.status())
        }
    }

    /// List all instances in the mesh
    pub async fn list_instances(&self) -> Result<InstancesResponse> {
        let response = self
            .client
            .get(format!("{}/registry/agents", self.base_url))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            anyhow::bail!("Failed to list instances: {}", response.status())
        }
    }

    /// Deregister from the mesh
    pub async fn deregister(&self, instance_id: &str) -> Result<()> {
        let response = self
            .client
            .delete(format!("{}/registry/deregister/{}", self.base_url, instance_id))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("Deregistration failed: {}", response.status())
        }
    }

    /// Send a message to another instance
    pub async fn send_message(
        &self,
        source_instance: String,
        target_instance: Option<String>,
        message_type: MessageType,
        payload: serde_json::Value,
        correlation_id: Option<String>,
    ) -> Result<SendMessageResponse> {
        let request = SendMessageRequest {
            target_instance,
            message_type,
            payload,
            correlation_id,
        };

        let response = self
            .client
            .post(format!("{}/messages/send/{}", self.base_url, source_instance))
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            anyhow::bail!("Send message failed: {}", response.status())
        }
    }

    /// Get pending messages for an instance
    pub async fn get_messages(&self, instance_id: &str) -> Result<PendingMessagesResponse> {
        let response = self
            .client
            .get(format!("{}/messages/{}", self.base_url, instance_id))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            anyhow::bail!("Get messages failed: {}", response.status())
        }
    }

    /// Acknowledge received messages
    pub async fn acknowledge_messages(&self, instance_id: &str, message_ids: Vec<String>) -> Result<()> {
        let request = AcknowledgeMessagesRequest { message_ids };

        let response = self
            .client
            .post(format!("{}/messages/ack/{}", self.base_url, instance_id))
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("Acknowledge failed: {}", response.status())
        }
    }
}

/// Extension trait to add mesh registry to app state
pub trait MeshState {
    fn mesh_registry(&self) -> &MeshRegistry;
}

/// Handler: Register a new instance
pub async fn register_instance<S: MeshState>(
    State(state): State<S>,
    Json(request): Json<RegisterRequest>,
) -> impl IntoResponse {
    let instance = MeshInstance {
        instance_id: request.instance_id,
        hostname: request.hostname,
        port: request.port,
        capabilities: request.capabilities,
        is_leader: false, // Will be set by registry
        last_heartbeat: Utc::now(),
        created_at: Utc::now(),
        agent_profiles: request.agent_profiles,
    };

    let response = state.mesh_registry().register(instance).await;
    (StatusCode::OK, Json(response))
}

/// Handler: List all instances
pub async fn list_instances<S: MeshState>(State(state): State<S>) -> impl IntoResponse {
    let instances = state.mesh_registry().list().await;
    let leader_id = instances
        .iter()
        .find(|i| i.is_leader)
        .map(|i| i.instance_id.clone());

    Json(InstancesResponse {
        instances,
        leader_id,
    })
}

/// Handler: Heartbeat from an instance
pub async fn heartbeat<S: MeshState>(
    State(state): State<S>,
    Path(instance_id): Path<String>,
    Json(_request): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let response = state.mesh_registry().heartbeat(&instance_id).await;

    if response.acknowledged {
        (StatusCode::OK, Json(response))
    } else {
        (StatusCode::NOT_FOUND, Json(response))
    }
}

/// Handler: Deregister an instance
pub async fn deregister_instance<S: MeshState>(
    State(state): State<S>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let removed = state.mesh_registry().deregister(&instance_id).await;

    if removed {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// Handler: Send a message to another instance
pub async fn send_message<S: MeshState>(
    State(state): State<S>,
    Path(source_instance): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> impl IntoResponse {
    match state
        .mesh_registry()
        .send_message(
            source_instance,
            request.target_instance,
            request.message_type,
            request.payload,
            request.correlation_id,
        )
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Handler: Get pending messages for an instance
pub async fn get_messages<S: MeshState>(
    State(state): State<S>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let messages = state
        .mesh_registry()
        .get_pending_messages(&instance_id)
        .await;

    Json(PendingMessagesResponse { messages })
}

/// Acknowledge messages request
#[derive(Debug, Serialize, Deserialize)]
pub struct AcknowledgeMessagesRequest {
    pub message_ids: Vec<String>,
}

/// Handler: Acknowledge received messages
pub async fn acknowledge_messages<S: MeshState>(
    State(state): State<S>,
    Path(instance_id): Path<String>,
    Json(request): Json<AcknowledgeMessagesRequest>,
) -> impl IntoResponse {
    state
        .mesh_registry()
        .acknowledge_messages(request.message_ids)
        .await;

    StatusCode::NO_CONTENT
}