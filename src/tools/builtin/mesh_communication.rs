use crate::api::mesh::{MessageType, MeshClient};
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

/// Tool for sending messages to other agents in the mesh
pub struct SendMessageTool {
    instance_id: String,
    mesh_url: Option<String>, // URL of the mesh registry
}

impl SendMessageTool {
    pub fn new(instance_id: String, mesh_url: Option<String>) -> Self {
        Self {
            instance_id,
            mesh_url,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct SendMessageArgs {
    target_instance: Option<String>,
    message_type: String,
    payload: Value,
    correlation_id: Option<String>,
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "send_mesh_message"
    }

    fn description(&self) -> &str {
        "Send a message to another agent instance in the mesh. Can send to a specific instance or broadcast to all."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target_instance": {
                    "type": "string",
                    "description": "The instance ID to send to. Omit for broadcast.",
                },
                "message_type": {
                    "type": "string",
                    "enum": ["query", "response", "notification", "task_delegation", "task_result", "graph_sync"],
                    "description": "Type of message being sent",
                },
                "payload": {
                    "type": "object",
                    "description": "Message payload as JSON object",
                },
                "correlation_id": {
                    "type": "string",
                    "description": "Optional correlation ID for request/response matching",
                }
            },
            "required": ["message_type", "payload"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: SendMessageArgs = serde_json::from_value(args)?;

        // If no mesh URL configured, return error
        let Some(ref mesh_url) = self.mesh_url else {
            return Ok(ToolResult::failure(
                "Mesh communication not configured. No mesh registry URL available.",
            ));
        };

        // Parse mesh URL
        let parts: Vec<&str> = mesh_url.split(':').collect();
        if parts.len() != 2 {
            return Ok(ToolResult::failure(format!("Invalid mesh URL: {}", mesh_url)));
        }

        let host = parts[0];
        let port: u16 = parts[1].parse()?;

        let client = MeshClient::new(host, port);

        // Send the message
        let message_type = MessageType::from_str(&args.message_type);
        let response = client
            .send_message(
                self.instance_id.clone(),
                args.target_instance,
                message_type,
                args.payload,
                args.correlation_id,
            )
            .await?;

        Ok(ToolResult::success(format!(
            "Message sent successfully. ID: {}, Status: {}, Delivered to: {:?}",
            response.message_id, response.status, response.delivered_to
        )))
    }
}

/// Tool for querying mesh instances
pub struct QueryMeshTool {
    mesh_url: Option<String>,
}

impl QueryMeshTool {
    pub fn new(mesh_url: Option<String>) -> Self {
        Self { mesh_url }
    }
}

#[async_trait]
impl Tool for QueryMeshTool {
    fn name(&self) -> &str {
        "query_mesh"
    }

    fn description(&self) -> &str {
        "Query the mesh registry to see all available agent instances and their capabilities."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult> {
        let Some(ref mesh_url) = self.mesh_url else {
            return Ok(ToolResult::failure(
                "Mesh communication not configured. No mesh registry URL available.",
            ));
        };

        let parts: Vec<&str> = mesh_url.split(':').collect();
        if parts.len() != 2 {
            return Ok(ToolResult::failure(format!("Invalid mesh URL: {}", mesh_url)));
        }

        let host = parts[0];
        let port: u16 = parts[1].parse()?;

        let client = MeshClient::new(host, port);
        let instances = client.list_instances().await?;

        let output = serde_json::to_string_pretty(&instances)?;
        Ok(ToolResult::success(output))
    }
}

/// Tool for receiving pending messages
pub struct GetMessagesTool {
    instance_id: String,
    mesh_url: Option<String>,
}

impl GetMessagesTool {
    pub fn new(instance_id: String, mesh_url: Option<String>) -> Self {
        Self {
            instance_id,
            mesh_url,
        }
    }
}

#[async_trait]
impl Tool for GetMessagesTool {
    fn name(&self) -> &str {
        "get_mesh_messages"
    }

    fn description(&self) -> &str {
        "Retrieve pending messages sent to this agent instance from other agents."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult> {
        let Some(ref mesh_url) = self.mesh_url else {
            return Ok(ToolResult::failure(
                "Mesh communication not configured. No mesh registry URL available.",
            ));
        };

        let parts: Vec<&str> = mesh_url.split(':').collect();
        if parts.len() != 2 {
            return Ok(ToolResult::failure(format!("Invalid mesh URL: {}", mesh_url)));
        }

        let host = parts[0];
        let port: u16 = parts[1].parse()?;

        let client = MeshClient::new(host, port);
        let messages = client.get_messages(&self.instance_id).await?;

        let output = serde_json::to_string_pretty(&messages)?;
        Ok(ToolResult::success(output))
    }
}
