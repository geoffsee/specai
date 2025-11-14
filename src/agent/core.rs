//! Agent Core Execution Loop
//!
//! The heart of the agent system - orchestrates reasoning, memory, and model interaction.

use crate::agent::model::{GenerationConfig, ModelProvider};
use crate::config::AgentProfile;
use crate::embeddings::EmbeddingsClient;
use crate::persistence::Persistence;
use crate::policy::{PolicyDecision, PolicyEngine};
use crate::tools::{ToolRegistry, ToolResult};
use crate::types::{EdgeType, Message, MessageRole, NodeType, TraversalDirection};
use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Output from an agent execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    /// The response text
    pub response: String,
    /// Message identifier for the persisted assistant response
    pub response_message_id: Option<i64>,
    /// Token usage information
    pub token_usage: Option<crate::agent::model::TokenUsage>,
    /// Detailed tool invocations performed during this turn
    pub tool_invocations: Vec<ToolInvocation>,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Semantic memory recall statistics for this turn (if embeddings enabled)
    pub recall_stats: Option<MemoryRecallStats>,
    /// Unique identifier for correlating this run with logs/telemetry
    pub run_id: String,
}

/// A single tool invocation, including arguments and outcome metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub name: String,
    pub arguments: Value,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolInvocation {
    pub fn from_result(name: &str, arguments: Value, result: &ToolResult) -> Self {
        let output = if result.output.trim().is_empty() {
            None
        } else {
            Some(result.output.clone())
        };

        Self {
            name: name.to_string(),
            arguments,
            success: result.success,
            output,
            error: result.error.clone(),
        }
    }
}

/// Telemetry about memory recall for a single turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecallStats {
    pub strategy: MemoryRecallStrategy,
    pub matches: Vec<MemoryRecallMatch>,
}

/// Strategy used for memory recall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryRecallStrategy {
    Semantic { requested: usize, returned: usize },
    RecentContext { limit: usize },
}

/// Summary of an individual recalled memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecallMatch {
    pub message_id: Option<i64>,
    pub score: f32,
    pub role: MessageRole,
    pub preview: String,
}

struct RecallResult {
    messages: Vec<Message>,
    stats: Option<MemoryRecallStats>,
}

// Entity extracted from text
struct ExtractedEntity {
    name: String,
    entity_type: String,
    confidence: f32,
}

// Concept extracted from text
struct ExtractedConcept {
    name: String,
    relevance: f32,
}

#[derive(Debug, Clone)]
struct GoalContext {
    message_id: i64,
    text: String,
    requires_tool: bool,
    satisfied: bool,
    node_id: Option<i64>,
}

impl GoalContext {
    fn new(message_id: i64, text: &str, requires_tool: bool, node_id: Option<i64>) -> Self {
        Self {
            message_id,
            text: text.to_string(),
            requires_tool,
            satisfied: !requires_tool,
            node_id,
        }
    }
}

/// Core agent execution engine
pub struct AgentCore {
    /// Agent profile with configuration
    profile: AgentProfile,
    /// Model provider
    provider: Arc<dyn ModelProvider>,
    /// Optional fast model provider for hierarchical reasoning
    fast_provider: Option<Arc<dyn ModelProvider>>,
    /// Optional embeddings client for semantic recall
    embeddings_client: Option<EmbeddingsClient>,
    /// Persistence layer
    persistence: Persistence,
    /// Current session ID
    session_id: String,
    /// Optional logical agent name from the registry
    agent_name: Option<String>,
    /// Conversation history (in-memory cache)
    conversation_history: Vec<Message>,
    /// Tool registry for executing tools
    tool_registry: Arc<ToolRegistry>,
    /// Policy engine for permission checks
    policy_engine: Arc<PolicyEngine>,
}

impl AgentCore {
    /// Create a new agent core
    pub fn new(
        profile: AgentProfile,
        provider: Arc<dyn ModelProvider>,
        embeddings_client: Option<EmbeddingsClient>,
        persistence: Persistence,
        session_id: String,
        agent_name: Option<String>,
        tool_registry: Arc<ToolRegistry>,
        policy_engine: Arc<PolicyEngine>,
    ) -> Self {
        Self {
            profile,
            provider,
            fast_provider: None,
            embeddings_client,
            persistence,
            session_id,
            agent_name,
            conversation_history: Vec::new(),
            tool_registry,
            policy_engine,
        }
    }

    /// Set the fast model provider for hierarchical reasoning
    pub fn with_fast_provider(mut self, fast_provider: Arc<dyn ModelProvider>) -> Self {
        self.fast_provider = Some(fast_provider);
        self
    }

    /// Set a new session ID and clear conversation history
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = session_id;
        self.conversation_history.clear();
        self
    }

    /// Execute a single interaction step
    pub async fn run_step(&mut self, input: &str) -> Result<AgentOutput> {
        let run_id = format!("run-{}", Utc::now().timestamp_micros());

        // Step 1: Recall relevant memories
        let recall_result = self.recall_memories(input).await?;
        let recalled_messages = recall_result.messages;
        let recall_stats = recall_result.stats;

        // Step 2: Build prompt with context
        let mut prompt = self.build_prompt(input, &recalled_messages)?;

        // Step 3: Store user message
        let user_message_id = self.store_message(MessageRole::User, input).await?;

        // Track user goal context (graph-driven planning)
        let mut goal_context =
            Some(self.create_goal_context(user_message_id, input, self.profile.enable_graph)?);

        // Step 4: Agent loop with tool execution
        let mut tool_invocations = Vec::new();
        let mut final_response = String::new();
        let mut token_usage = None;
        let mut finish_reason = None;
        let mut auto_response: Option<String> = None;

        // Attempt to auto-satisfy simple goals before invoking the model
        if let Some(goal) = goal_context.as_mut() {
            if goal.requires_tool {
                if let Some((tool_name, tool_args)) =
                    Self::infer_goal_tool_action(goal.text.as_str())
                {
                    if self.is_tool_allowed(&tool_name) {
                        match self.execute_tool(&run_id, &tool_name, &tool_args).await {
                            Ok(result) => {
                                let invocation = ToolInvocation::from_result(
                                    &tool_name,
                                    tool_args.clone(),
                                    &result,
                                );
                                if let Err(err) = self
                                    .record_goal_tool_result(goal, &tool_name, &tool_args, &result)
                                {
                                    warn!("Failed to record goal progress: {}", err);
                                }
                                if result.success {
                                    if let Err(err) =
                                        self.update_goal_status(goal, "completed", true, None)
                                    {
                                        warn!("Failed to update goal status: {}", err);
                                    } else {
                                        goal.satisfied = true;
                                    }
                                }
                                auto_response = Some(Self::format_auto_tool_response(
                                    &tool_name,
                                    invocation.output.as_deref(),
                                ));
                                tool_invocations.push(invocation);
                            }
                            Err(err) => {
                                warn!("Auto tool execution '{}' failed: {}", tool_name, err);
                            }
                        }
                    }
                }
            }
        }

        let skip_model = goal_context
            .as_ref()
            .map(|goal| goal.requires_tool && goal.satisfied && auto_response.is_some())
            .unwrap_or(false);

        if skip_model {
            final_response = auto_response.unwrap_or_else(|| "Task completed.".to_string());
            finish_reason = Some("auto_tool".to_string());
        } else {
            // Allow up to 5 iterations to handle tool calls
            for _iteration in 0..5 {
                // Generate response using model
                let generation_config = self.build_generation_config();
                let response = self
                    .provider
                    .generate(&prompt, &generation_config)
                    .await
                    .context("Failed to generate response from model")?;

                token_usage = response.usage;
                finish_reason = response.finish_reason.clone();
                final_response = response.content.clone();

                // Check for tool calls in the response
                if let Some(tool_call) = self.parse_tool_call(&response.content) {
                    let tool_name = tool_call.0;
                    let tool_args = tool_call.1;

                    // Check if tool is allowed
                    if !self.is_tool_allowed(&tool_name) {
                        let error_msg =
                            format!("Tool '{}' is not allowed by agent policy", tool_name);
                        prompt.push_str(&format!(
                            "\n\nTOOL_ERROR: {}\n\nPlease continue without using this tool.",
                            error_msg
                        ));
                        continue;
                    }

                    // Execute tool
                    match self.execute_tool(&run_id, &tool_name, &tool_args).await {
                        Ok(result) => {
                            let invocation =
                                ToolInvocation::from_result(&tool_name, tool_args.clone(), &result);
                            let tool_output = invocation.output.clone().unwrap_or_default();
                            let was_success = invocation.success;
                            let error_message = invocation
                                .error
                                .clone()
                                .unwrap_or_else(|| "Tool execution failed".to_string());
                            tool_invocations.push(invocation);

                            if let Some(goal) = goal_context.as_mut() {
                                if let Err(err) = self
                                    .record_goal_tool_result(goal, &tool_name, &tool_args, &result)
                                {
                                    warn!("Failed to record goal progress: {}", err);
                                }
                                if result.success && goal.requires_tool && !goal.satisfied {
                                    if let Err(err) =
                                        self.update_goal_status(goal, "in_progress", true, None)
                                    {
                                        warn!("Failed to update goal status: {}", err);
                                    }
                                }
                            }

                            if was_success {
                                // Add tool result to prompt for next iteration
                                prompt.push_str(&format!(
                                    "\n\nTOOL_RESULT from {}:\n{}\n\nBased on this result, please continue.",
                                    tool_name, tool_output
                                ));
                            } else {
                                prompt.push_str(&format!(
                                    "\n\nTOOL_ERROR: {}\n\nPlease continue without this tool.",
                                    error_message
                                ));
                                continue;
                            }

                            // If the model response contains only the tool call, continue loop
                            if response.content.trim().starts_with("TOOL_CALL:") {
                                continue;
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Error executing tool '{}': {}", tool_name, e);
                            prompt.push_str(&format!(
                                "\n\nTOOL_ERROR: {}\n\nPlease continue without this tool.",
                                error_msg
                            ));
                            tool_invocations.push(ToolInvocation {
                                name: tool_name,
                                arguments: tool_args,
                                success: false,
                                output: None,
                                error: Some(error_msg),
                            });
                            continue;
                        }
                    }
                }

                if let Some(goal) = goal_context.as_ref() {
                    if goal.requires_tool && !goal.satisfied {
                        prompt.push_str(
                            "\n\nGOAL_STATUS: pending. The user request requires executing an allowed tool. Emit a TOOL_CALL now.",
                        );
                        continue;
                    }
                }

                // No tool call found or response includes final answer, break
                break;
            }
        }

        // Step 5: Store assistant response
        let response_message_id = self
            .store_message(MessageRole::Assistant, &final_response)
            .await?;

        if let Some(goal) = goal_context.as_mut() {
            if goal.requires_tool {
                if goal.satisfied {
                    if let Err(err) =
                        self.update_goal_status(goal, "completed", true, Some(response_message_id))
                    {
                        warn!("Failed to finalize goal status: {}", err);
                    }
                } else if let Err(err) =
                    self.update_goal_status(goal, "blocked", false, Some(response_message_id))
                {
                    warn!("Failed to record blocked goal status: {}", err);
                }
            } else if let Err(err) =
                self.update_goal_status(goal, "completed", true, Some(response_message_id))
            {
                warn!("Failed to finalize goal status: {}", err);
            }
        }

        // Step 6: Update conversation history
        self.conversation_history.push(Message {
            id: user_message_id,
            session_id: self.session_id.clone(),
            role: MessageRole::User,
            content: input.to_string(),
            created_at: Utc::now(),
        });

        self.conversation_history.push(Message {
            id: response_message_id,
            session_id: self.session_id.clone(),
            role: MessageRole::Assistant,
            content: final_response.clone(),
            created_at: Utc::now(),
        });

        Ok(AgentOutput {
            response: final_response,
            response_message_id: Some(response_message_id),
            token_usage,
            tool_invocations,
            finish_reason,
            recall_stats,
            run_id,
        })
    }

    /// Build generation configuration from profile
    fn build_generation_config(&self) -> GenerationConfig {
        GenerationConfig {
            temperature: self.profile.temperature,
            max_tokens: self.profile.max_context_tokens.map(|t| t as u32),
            stop_sequences: None,
            top_p: Some(self.profile.top_p),
            frequency_penalty: None,
            presence_penalty: None,
        }
    }

    /// Recall relevant memories for the given input
    async fn recall_memories(&self, query: &str) -> Result<RecallResult> {
        const RECENT_CONTEXT: i64 = 2;
        let mut context = Vec::new();
        let mut seen_ids = HashSet::new();

        let recent_messages = self
            .persistence
            .list_messages(&self.session_id, RECENT_CONTEXT)?;

        for message in recent_messages {
            seen_ids.insert(message.id);
            context.push(message);
        }

        // If graph memory is enabled, expand context with graph-connected nodes
        if self.profile.enable_graph && self.profile.graph_memory {
            let mut graph_messages = Vec::new();

            // For each recent message, find related nodes in the graph
            for msg in &context {
                // Check if this message has a corresponding node in the graph
                let nodes = self.persistence.list_graph_nodes(
                    &self.session_id,
                    Some(NodeType::Message),
                    Some(10),
                )?;

                for node in nodes {
                    if let Some(msg_id) = node.properties["message_id"].as_i64() {
                        if msg_id == msg.id {
                            // Traverse graph to find related nodes
                            let neighbors = self.persistence.traverse_neighbors(
                                &self.session_id,
                                node.id,
                                TraversalDirection::Both,
                                self.profile.graph_depth,
                            )?;

                            // Add messages from related nodes
                            for neighbor in neighbors {
                                if neighbor.node_type == NodeType::Message {
                                    if let Some(related_msg_id) =
                                        neighbor.properties["message_id"].as_i64()
                                    {
                                        if !seen_ids.contains(&related_msg_id) {
                                            if let Some(related_msg) =
                                                self.persistence.get_message(related_msg_id)?
                                            {
                                                seen_ids.insert(related_msg.id);
                                                graph_messages.push(related_msg);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add graph-expanded messages to context
            context.extend(graph_messages);
        }

        if let Some(client) = &self.embeddings_client {
            if self.profile.memory_k == 0 || query.trim().is_empty() {
                return Ok(RecallResult {
                    messages: context,
                    stats: None,
                });
            }

            match client.embed(query).await {
                Ok(query_embedding) if !query_embedding.is_empty() => {
                    let recalled = self.persistence.recall_top_k(
                        &self.session_id,
                        &query_embedding,
                        self.profile.memory_k,
                    )?;

                    let mut matches = Vec::new();
                    let mut semantic_context = Vec::new();

                    for (memory, score) in recalled {
                        if let Some(message_id) = memory.message_id {
                            if seen_ids.contains(&message_id) {
                                continue;
                            }

                            if let Some(message) = self.persistence.get_message(message_id)? {
                                seen_ids.insert(message.id);
                                matches.push(MemoryRecallMatch {
                                    message_id: Some(message.id),
                                    score,
                                    role: message.role,
                                    preview: preview_text(&message.content),
                                });
                                semantic_context.push(message);
                            }
                        }
                    }

                    // If graph memory enabled, expand semantic matches with graph connections
                    if self.profile.enable_graph && self.profile.graph_memory {
                        let mut graph_expanded = Vec::new();

                        for msg in &semantic_context {
                            // Find message node in graph
                            let nodes = self.persistence.list_graph_nodes(
                                &self.session_id,
                                Some(NodeType::Message),
                                Some(100),
                            )?;

                            for node in nodes {
                                if let Some(msg_id) = node.properties["message_id"].as_i64() {
                                    if msg_id == msg.id {
                                        // Traverse to find related information
                                        let neighbors = self.persistence.traverse_neighbors(
                                            &self.session_id,
                                            node.id,
                                            TraversalDirection::Both,
                                            self.profile.graph_depth,
                                        )?;

                                        for neighbor in neighbors {
                                            // Include related facts, concepts, and entities
                                            if matches!(
                                                neighbor.node_type,
                                                NodeType::Fact
                                                    | NodeType::Concept
                                                    | NodeType::Entity
                                            ) {
                                                // Create a synthetic message for graph context
                                                let graph_content = format!(
                                                    "[Graph Context - {} {}]: {}",
                                                    neighbor.node_type.as_str(),
                                                    neighbor.label,
                                                    neighbor.properties.to_string()
                                                );

                                                // Add as system message for context
                                                let graph_msg = Message {
                                                    id: -1, // Synthetic ID
                                                    session_id: self.session_id.clone(),
                                                    role: MessageRole::System,
                                                    content: graph_content,
                                                    created_at: Utc::now(),
                                                };

                                                graph_expanded.push(graph_msg);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Combine semantic and graph-expanded context
                        context.extend(semantic_context);
                        context.extend(graph_expanded);

                        // Apply graph weight to balance semantic vs graph relevance
                        let _total_messages = context.len();
                        let _semantic_weight = 1.0 - self.profile.graph_weight;
                        let _graph_weight = self.profile.graph_weight;

                        // TODO: Implement graph-semantic hybrid ranking
                        // Re-rank based on combined score (simplified for now)
                        // In production, this would use a more sophisticated ranking algorithm
                    } else {
                        context.extend(semantic_context);
                    }

                    return Ok(RecallResult {
                        messages: context,
                        stats: Some(MemoryRecallStats {
                            strategy: MemoryRecallStrategy::Semantic {
                                requested: self.profile.memory_k,
                                returned: matches.len(),
                            },
                            matches,
                        }),
                    });
                }
                Ok(_) => {
                    return Ok(RecallResult {
                        messages: context,
                        stats: Some(MemoryRecallStats {
                            strategy: MemoryRecallStrategy::Semantic {
                                requested: self.profile.memory_k,
                                returned: 0,
                            },
                            matches: Vec::new(),
                        }),
                    });
                }
                Err(err) => {
                    warn!("Failed to embed recall query: {}", err);
                }
            }

            return Ok(RecallResult {
                messages: context,
                stats: None,
            });
        }

        // Fallback when embeddings are unavailable
        let limit = self.profile.memory_k as i64;
        let messages = self.persistence.list_messages(&self.session_id, limit)?;

        let stats = if self.profile.memory_k > 0 {
            Some(MemoryRecallStats {
                strategy: MemoryRecallStrategy::RecentContext {
                    limit: self.profile.memory_k,
                },
                matches: Vec::new(),
            })
        } else {
            None
        };

        Ok(RecallResult { messages, stats })
    }

    /// Build the prompt from system prompt, context, and user input
    fn build_prompt(&self, input: &str, context_messages: &[Message]) -> Result<String> {
        let mut prompt = String::new();

        // Add system prompt if configured
        if let Some(ref system_prompt) = self.profile.prompt {
            prompt.push_str("System: ");
            prompt.push_str(system_prompt);
            prompt.push_str("\n\n");
        }

        // Add tool instructions
        let available_tools = self.tool_registry.list();
        info!("Tool registry has {} tools", available_tools.len());
        if !available_tools.is_empty() {
            prompt.push_str("Available tools:\n");
            for tool_name in &available_tools {
                info!(
                    "Checking tool: {} - allowed: {}",
                    tool_name,
                    self.is_tool_allowed(tool_name)
                );
                if self.is_tool_allowed(tool_name) {
                    if let Some(tool) = self.tool_registry.get(tool_name) {
                        prompt.push_str(&format!("- {}: {}\n", tool_name, tool.description()));
                    }
                }
            }
            prompt.push_str("\n");
            prompt.push_str("To use a tool, respond with:\n");
            prompt.push_str("TOOL_CALL: tool_name\n");
            prompt.push_str("ARGS: {\"param\": \"value\"}\n\n");
        }

        // Add conversation context
        if !context_messages.is_empty() {
            prompt.push_str("Previous conversation:\n");
            for msg in context_messages {
                prompt.push_str(&format!("{}: {}\n", msg.role.as_str(), msg.content));
            }
            prompt.push_str("\n");
        }

        // Add current user input
        prompt.push_str(&format!("user: {}\n", input));

        prompt.push_str("assistant:");

        Ok(prompt)
    }

    /// Store a message in persistence
    async fn store_message(&self, role: MessageRole, content: &str) -> Result<i64> {
        let message_id = self
            .persistence
            .insert_message(&self.session_id, role, content)
            .context("Failed to store message")?;

        let mut embedding_id = None;

        if let Some(client) = &self.embeddings_client {
            if !content.trim().is_empty() {
                match client.embed(content).await {
                    Ok(embedding) if !embedding.is_empty() => {
                        match self.persistence.insert_memory_vector(
                            &self.session_id,
                            Some(message_id),
                            &embedding,
                        ) {
                            Ok(emb_id) => {
                                embedding_id = Some(emb_id);
                            }
                            Err(err) => {
                                warn!(
                                    "Failed to persist embedding for message {}: {}",
                                    message_id, err
                                );
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        warn!(
                            "Failed to create embedding for message {}: {}",
                            message_id, err
                        );
                    }
                }
            }
        }

        // If auto_graph is enabled, create graph nodes and edges
        if self.profile.enable_graph && self.profile.auto_graph {
            self.build_graph_for_message(message_id, role, content, embedding_id)?;
        }

        Ok(message_id)
    }

    /// Build graph nodes and edges for a new message
    fn build_graph_for_message(
        &self,
        message_id: i64,
        role: MessageRole,
        content: &str,
        embedding_id: Option<i64>,
    ) -> Result<()> {
        use serde_json::json;

        // Create a node for the message
        let message_node_id = self.persistence.insert_graph_node(
            &self.session_id,
            NodeType::Message,
            &format!("{:?}Message", role),
            &json!({
                "message_id": message_id,
                "role": role.as_str(),
                "content_preview": preview_text(content),
                "timestamp": Utc::now().to_rfc3339(),
            }),
            embedding_id,
        )?;

        // Extract entities and concepts from the message (simplified for now)
        // In production, this would use NER and entity extraction
        let entities = self.extract_entities_from_text(content);
        let concepts = self.extract_concepts_from_text(content);

        // Create nodes for entities
        for entity in entities {
            let entity_node_id = self.persistence.insert_graph_node(
                &self.session_id,
                NodeType::Entity,
                &entity.entity_type,
                &json!({
                    "name": entity.name,
                    "type": entity.entity_type,
                    "extracted_from": message_id,
                }),
                None,
            )?;

            // Create edge from message to entity
            self.persistence.insert_graph_edge(
                &self.session_id,
                message_node_id,
                entity_node_id,
                EdgeType::Mentions,
                Some("mentions"),
                Some(&json!({"confidence": entity.confidence})),
                entity.confidence,
            )?;
        }

        // Create nodes for concepts
        for concept in concepts {
            let concept_node_id = self.persistence.insert_graph_node(
                &self.session_id,
                NodeType::Concept,
                "Concept",
                &json!({
                    "name": concept.name,
                    "extracted_from": message_id,
                }),
                None,
            )?;

            // Create edge from message to concept
            self.persistence.insert_graph_edge(
                &self.session_id,
                message_node_id,
                concept_node_id,
                EdgeType::RelatesTo,
                Some("discusses"),
                Some(&json!({"relevance": concept.relevance})),
                concept.relevance,
            )?;
        }

        // Link to previous message in conversation flow
        let recent_messages = self.persistence.list_messages(&self.session_id, 2)?;
        if recent_messages.len() > 1 {
            // Find the previous message node
            let nodes = self.persistence.list_graph_nodes(
                &self.session_id,
                Some(NodeType::Message),
                Some(10),
            )?;

            for node in nodes {
                if let Some(prev_msg_id) = node.properties["message_id"].as_i64() {
                    if prev_msg_id != message_id && prev_msg_id == recent_messages[0].id {
                        // Create conversation flow edge
                        self.persistence.insert_graph_edge(
                            &self.session_id,
                            node.id,
                            message_node_id,
                            EdgeType::FollowsFrom,
                            Some("conversation_flow"),
                            None,
                            1.0,
                        )?;
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn create_goal_context(
        &self,
        message_id: i64,
        input: &str,
        persist: bool,
    ) -> Result<GoalContext> {
        let requires_tool = Self::goal_requires_tool(input);
        let node_id = if self.profile.enable_graph {
            if persist {
                let properties = json!({
                    "message_id": message_id,
                    "goal_text": input,
                    "status": "pending",
                    "requires_tool": requires_tool,
                    "satisfied": false,
                    "created_at": Utc::now().to_rfc3339(),
                });
                Some(self.persistence.insert_graph_node(
                    &self.session_id,
                    NodeType::Goal,
                    "Goal",
                    &properties,
                    None,
                )?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(GoalContext::new(message_id, input, requires_tool, node_id))
    }

    fn update_goal_status(
        &self,
        goal: &mut GoalContext,
        status: &str,
        satisfied: bool,
        response_message_id: Option<i64>,
    ) -> Result<()> {
        goal.satisfied = satisfied;
        if let Some(node_id) = goal.node_id {
            let properties = json!({
                "message_id": goal.message_id,
                "goal_text": goal.text,
                "status": status,
                "requires_tool": goal.requires_tool,
                "satisfied": satisfied,
                "response_message_id": response_message_id,
                "updated_at": Utc::now().to_rfc3339(),
            });
            self.persistence.update_graph_node(node_id, &properties)?;
        }
        Ok(())
    }

    fn record_goal_tool_result(
        &self,
        goal: &GoalContext,
        tool_name: &str,
        args: &Value,
        result: &ToolResult,
    ) -> Result<()> {
        if let Some(goal_node_id) = goal.node_id {
            let properties = json!({
                "tool": tool_name,
                "arguments": args,
                "success": result.success,
                "output_preview": preview_text(&result.output),
                "error": result.error,
                "timestamp": Utc::now().to_rfc3339(),
            });
            let tool_node_id = self.persistence.insert_graph_node(
                &self.session_id,
                NodeType::ToolResult,
                tool_name,
                &properties,
                None,
            )?;
            self.persistence.insert_graph_edge(
                &self.session_id,
                tool_node_id,
                goal_node_id,
                EdgeType::Produces,
                Some("satisfies"),
                None,
                if result.success { 1.0 } else { 0.1 },
            )?;
        }
        Ok(())
    }

    fn goal_requires_tool(input: &str) -> bool {
        let normalized = input.to_lowercase();
        const ACTION_VERBS: [&str; 18] = [
            "list", "show", "read", "write", "create", "update", "delete", "run", "execute",
            "open", "search", "fetch", "download", "scan", "compile", "test", "build", "inspect",
        ];

        ACTION_VERBS
            .iter()
            .any(|verb| normalized.contains(verb) && normalized.contains(' '))
    }

    fn infer_goal_tool_action(goal_text: &str) -> Option<(String, Value)> {
        let text = goal_text.to_lowercase();

        // Handle directory listing requests
        if text.contains("list")
            && (text.contains("directory") || text.contains("files") || text.contains("folder"))
        {
            return Some((
                "shell".to_string(),
                json!({
                    "command": "ls -a"
                }),
            ));
        }

        if text.contains("show") && text.contains("current directory") {
            return Some((
                "shell".to_string(),
                json!({
                    "command": "ls -a"
                }),
            ));
        }

        // For code generation requests, return None to let the agent handle it
        // The agent should use its normal reasoning to generate appropriate code
        // based on the user's request, not use hardcoded snippets
        None
    }

    fn format_auto_tool_response(tool_name: &str, output: Option<&str>) -> String {
        let sanitized = output.unwrap_or("").trim();
        if sanitized.is_empty() {
            return format!("Executed `{}` successfully.", tool_name);
        }

        if tool_name == "shell" || tool_name == "bash" {
            if let Ok(value) = serde_json::from_str::<Value>(sanitized) {
                let std_out = value
                    .get("stdout")
                    .and_then(|v| v.as_str())
                    .unwrap_or(sanitized);
                let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let stderr = value
                    .get("stderr")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("");
                let mut response = String::new();
                if !command.is_empty() {
                    response.push_str(&format!("Command `{}` output:\n", command));
                }
                response.push_str(std_out.trim_end());
                if !stderr.is_empty() {
                    response.push_str("\n\nstderr:\n");
                    response.push_str(stderr);
                }
                if response.trim().is_empty() {
                    return "Command completed without output.".to_string();
                }
                return response;
            }
        }

        sanitized.to_string()
    }

    // Entity extraction - can use fast model if configured
    fn extract_entities_from_text(&self, text: &str) -> Vec<ExtractedEntity> {
        // If fast reasoning is enabled and task is delegated to fast model, use it
        if self.profile.fast_reasoning
            && self.fast_provider.is_some()
            && self
                .profile
                .fast_model_tasks
                .contains(&"entity_extraction".to_string())
        {
            // Use fast model for entity extraction (would be async in production)
            debug!("Using fast model for entity extraction");
            // For now, fall back to simple extraction
            // In production, this would call the fast model async
        }

        let mut entities = Vec::new();

        // Simple pattern matching for demonstration
        // In production, use a proper NER model or fast LLM

        // Extract URLs
        let url_regex = regex::Regex::new(r"https?://[^\s]+").unwrap();
        for mat in url_regex.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: "URL".to_string(),
                confidence: 0.9,
            });
        }

        // Extract email addresses
        let email_regex =
            regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap();
        for mat in email_regex.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: "Email".to_string(),
                confidence: 0.9,
            });
        }

        // Extract quoted text as potential entities
        let quote_regex = regex::Regex::new(r#""([^"]+)""#).unwrap();
        for cap in quote_regex.captures_iter(text) {
            if let Some(quoted) = cap.get(1) {
                entities.push(ExtractedEntity {
                    name: quoted.as_str().to_string(),
                    entity_type: "Quote".to_string(),
                    confidence: 0.7,
                });
            }
        }

        entities
    }

    /// Use fast model for preliminary reasoning tasks
    #[allow(dead_code)]
    async fn _fast_reasoning(&self, task: &str, input: &str) -> Result<(String, f32)> {
        if let Some(ref fast_provider) = self.fast_provider {
            let prompt = format!(
                "Task: {}\nInput: {}\n\nProvide a concise response and confidence score (0-1):",
                task, input
            );

            let config = GenerationConfig {
                temperature: Some(self.profile.fast_model_temperature),
                max_tokens: Some(256), // Keep responses short for speed
                stop_sequences: None,
                top_p: Some(0.9),
                frequency_penalty: None,
                presence_penalty: None,
            };

            let response = fast_provider.generate(&prompt, &config).await?;

            // Parse confidence from response (simplified)
            let confidence = 0.7; // In production, extract from response

            Ok((response.content, confidence))
        } else {
            // No fast model configured
            Ok((String::new(), 0.0))
        }
    }

    /// Decide whether to use fast or main model based on task complexity
    #[allow(dead_code)]
    async fn _route_to_model(&self, task_type: &str, complexity_score: f32) -> bool {
        // Check if fast reasoning is enabled
        if !self.profile.fast_reasoning || self.fast_provider.is_none() {
            return false; // Use main model
        }

        // Check if task type is delegated to fast model
        if !self
            .profile
            .fast_model_tasks
            .contains(&task_type.to_string())
        {
            return false; // Use main model
        }

        // Check complexity threshold
        if complexity_score > self.profile.escalation_threshold {
            info!(
                "Task complexity {} exceeds threshold {}, using main model",
                complexity_score, self.profile.escalation_threshold
            );
            return false; // Use main model
        }

        true // Use fast model
    }

    // Concept extraction (simplified - in production use topic modeling)
    fn extract_concepts_from_text(&self, text: &str) -> Vec<ExtractedConcept> {
        let mut concepts = Vec::new();

        // Keywords that indicate concepts (simplified)
        let concept_keywords = vec![
            ("graph", "Knowledge Graph"),
            ("memory", "Memory System"),
            ("embedding", "Embeddings"),
            ("tool", "Tool Usage"),
            ("agent", "Agent System"),
            ("database", "Database"),
            ("query", "Query Processing"),
            ("node", "Graph Node"),
            ("edge", "Graph Edge"),
        ];

        let text_lower = text.to_lowercase();
        for (keyword, concept_name) in concept_keywords {
            if text_lower.contains(keyword) {
                concepts.push(ExtractedConcept {
                    name: concept_name.to_string(),
                    relevance: 0.6,
                });
            }
        }

        concepts
    }

    /// Get the current session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the agent profile
    pub fn profile(&self) -> &AgentProfile {
        &self.profile
    }

    /// Get the logical agent name (if provided)
    pub fn agent_name(&self) -> Option<&str> {
        self.agent_name.as_deref()
    }

    /// Get conversation history
    pub fn conversation_history(&self) -> &[Message] {
        &self.conversation_history
    }

    /// Load conversation history from persistence
    pub fn load_history(&mut self, limit: i64) -> Result<()> {
        self.conversation_history = self.persistence.list_messages(&self.session_id, limit)?;
        Ok(())
    }

    /// Parse tool call from model response
    /// Expected formats:
    /// 1. TOOL_CALL: tool_name\nARGS: {"arg1": "value1"}
    /// 2. tool_name: tool_name\ntool_input: {"arg1": "value1"}
    fn parse_tool_call(&self, response: &str) -> Option<(String, Value)> {
        // Try the expected format first
        let re1 = Regex::new(r"TOOL_CALL:\s*(\w+)\s*\nARGS:\s*(\{.*?\})").ok()?;
        if let Some(captures) = re1.captures(response) {
            let tool_name = captures.get(1)?.as_str().to_string();
            let args_str = captures.get(2)?.as_str();
            if let Ok(args) = serde_json::from_str(args_str) {
                return Some((tool_name, args));
            }
        }

        // Try the alternative format (what the model actually outputs)
        let re2 = Regex::new(r"tool_name:\s*(\w+)\s*\ntool_input:\s*(\{.*?\})").ok()?;
        if let Some(captures) = re2.captures(response) {
            let tool_name = captures.get(1)?.as_str().to_string();
            let args_str = captures.get(2)?.as_str();
            if let Ok(args) = serde_json::from_str(args_str) {
                return Some((tool_name, args));
            }
        }

        None
    }

    /// Check if a tool is allowed by the agent profile and policy engine
    fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // First check profile-level permissions (backward compatibility)
        let profile_allowed = self.profile.is_tool_allowed(tool_name);
        info!(
            "Profile check for tool '{}': allowed={}, allowed_tools={:?}, denied_tools={:?}",
            tool_name, profile_allowed, self.profile.allowed_tools, self.profile.denied_tools
        );
        if !profile_allowed {
            return false;
        }

        // Then check policy engine
        let agent_name = "agent"; // Could be enhanced to use profile name
        let decision = self.policy_engine.check(agent_name, "tool_call", tool_name);
        info!(
            "Policy check for tool '{}': decision={:?}",
            tool_name, decision
        );

        matches!(decision, PolicyDecision::Allow)
    }

    /// Execute a tool and log the result
    async fn execute_tool(
        &self,
        run_id: &str,
        tool_name: &str,
        args: &Value,
    ) -> Result<ToolResult> {
        // Execute the tool (convert execution failures into ToolResult failures)
        let exec_result = self.tool_registry.execute(tool_name, args.clone()).await;
        let result = match exec_result {
            Ok(res) => res,
            Err(err) => ToolResult::failure(err.to_string()),
        };

        // Log to persistence
        let result_json = serde_json::json!({
            "output": result.output,
            "success": result.success,
            "error": result.error,
        });

        let error_str = result.error.as_deref();
        self.persistence
            .log_tool(
                &self.session_id,
                self.agent_name.as_deref().unwrap_or("unknown"),
                run_id,
                tool_name,
                args,
                &result_json,
                result.success,
                error_str,
            )
            .context("Failed to log tool execution")?;

        Ok(result)
    }

    /// Get the tool registry
    pub fn tool_registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    /// Get the policy engine
    pub fn policy_engine(&self) -> &PolicyEngine {
        &self.policy_engine
    }

    /// Set a new policy engine (useful for reloading policies)
    pub fn set_policy_engine(&mut self, policy_engine: Arc<PolicyEngine>) {
        self.policy_engine = policy_engine;
    }
}

fn preview_text(content: &str) -> String {
    const MAX_CHARS: usize = 80;
    let trimmed = content.trim();
    let mut preview = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= MAX_CHARS {
            preview.push_str("...");
            break;
        }
        preview.push(ch);
    }
    if preview.is_empty() {
        trimmed.to_string()
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::providers::MockProvider;
    use crate::config::AgentProfile;
    use crate::embeddings::{EmbeddingsClient, EmbeddingsService};
    use async_trait::async_trait;
    use tempfile::tempdir;

    fn create_test_agent(session_id: &str) -> (AgentCore, tempfile::TempDir) {
        create_test_agent_with_embeddings(session_id, None)
    }

    fn create_test_agent_with_embeddings(
        session_id: &str,
        embeddings_client: Option<EmbeddingsClient>,
    ) -> (AgentCore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let profile = AgentProfile {
            prompt: Some("You are a helpful assistant.".to_string()),
            style: None,
            temperature: Some(0.7),
            model_provider: None,
            model_name: None,
            allowed_tools: None,
            denied_tools: None,
            memory_k: 5,
            top_p: 0.9,
            max_context_tokens: Some(2048),
            enable_graph: false,
            graph_memory: false,
            auto_graph: false,
            graph_steering: false,
            graph_depth: 3,
            graph_weight: 0.5,
            graph_threshold: 0.7,
            fast_reasoning: false,
            fast_model_provider: None,
            fast_model_name: None,
            fast_model_temperature: 0.3,
            fast_model_tasks: vec![],
            escalation_threshold: 0.6,
        };

        let provider = Arc::new(MockProvider::new("This is a test response."));
        let tool_registry = Arc::new(crate::tools::ToolRegistry::new());
        let policy_engine = Arc::new(PolicyEngine::new());

        (
            AgentCore::new(
                profile,
                provider,
                embeddings_client,
                persistence,
                session_id.to_string(),
                Some(session_id.to_string()),
                tool_registry,
                policy_engine,
            ),
            dir,
        )
    }

    #[derive(Clone)]
    struct KeywordEmbeddingsService;

    #[async_trait]
    impl EmbeddingsService for KeywordEmbeddingsService {
        async fn create_embeddings(&self, _model: &str, input: &str) -> Result<Vec<f32>> {
            Ok(keyword_embedding(input))
        }
    }

    fn keyword_embedding(input: &str) -> Vec<f32> {
        let lower = input.to_ascii_lowercase();
        let alpha = if lower.contains("alpha") { 1.0 } else { 0.0 };
        let beta = if lower.contains("beta") { 1.0 } else { 0.0 };
        vec![alpha, beta]
    }

    fn test_embeddings_client() -> EmbeddingsClient {
        EmbeddingsClient::with_service(
            "test",
            Arc::new(KeywordEmbeddingsService) as Arc<dyn EmbeddingsService>,
        )
    }

    #[tokio::test]
    async fn test_agent_core_run_step() {
        let (mut agent, _dir) = create_test_agent("test-session-1");

        let output = agent.run_step("Hello, how are you?").await.unwrap();

        assert!(!output.response.is_empty());
        assert!(output.token_usage.is_some());
        assert_eq!(output.tool_invocations.len(), 0);
    }

    #[tokio::test]
    async fn test_agent_core_conversation_history() {
        let (mut agent, _dir) = create_test_agent("test-session-2");

        agent.run_step("First message").await.unwrap();
        agent.run_step("Second message").await.unwrap();

        let history = agent.conversation_history();
        assert_eq!(history.len(), 4); // 2 user + 2 assistant
        assert_eq!(history[0].role, MessageRole::User);
        assert_eq!(history[1].role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn test_agent_core_session_switch() {
        let (mut agent, _dir) = create_test_agent("session-1");

        agent.run_step("Message in session 1").await.unwrap();
        assert_eq!(agent.session_id(), "session-1");

        agent = agent.with_session("session-2".to_string());
        assert_eq!(agent.session_id(), "session-2");
        assert_eq!(agent.conversation_history().len(), 0);
    }

    #[tokio::test]
    async fn test_agent_core_build_prompt() {
        let (agent, _dir) = create_test_agent("test-session-3");

        let context = vec![
            Message {
                id: 1,
                session_id: "test-session-3".to_string(),
                role: MessageRole::User,
                content: "Previous question".to_string(),
                created_at: Utc::now(),
            },
            Message {
                id: 2,
                session_id: "test-session-3".to_string(),
                role: MessageRole::Assistant,
                content: "Previous answer".to_string(),
                created_at: Utc::now(),
            },
        ];

        let prompt = agent.build_prompt("Current question", &context).unwrap();

        assert!(prompt.contains("You are a helpful assistant"));
        assert!(prompt.contains("Previous conversation"));
        assert!(prompt.contains("user: Previous question"));
        assert!(prompt.contains("assistant: Previous answer"));
        assert!(prompt.contains("user: Current question"));
    }

    #[tokio::test]
    async fn test_agent_core_persistence() {
        let (mut agent, _dir) = create_test_agent("persist-test");

        agent.run_step("Test message").await.unwrap();

        // Load messages from DB
        let messages = agent
            .persistence
            .list_messages("persist-test", 100)
            .unwrap();

        assert_eq!(messages.len(), 2); // user + assistant
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "Test message");
    }

    #[tokio::test]
    async fn store_message_records_embeddings() {
        let (agent, _dir) =
            create_test_agent_with_embeddings("embedding-store", Some(test_embeddings_client()));

        let message_id = agent
            .store_message(MessageRole::User, "Alpha detail")
            .await
            .unwrap();

        let query = vec![1.0f32, 0.0];
        let recalled = agent
            .persistence
            .recall_top_k("embedding-store", &query, 1)
            .unwrap();

        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].0.message_id, Some(message_id));
    }

    #[tokio::test]
    async fn recall_memories_appends_semantic_matches() {
        let (agent, _dir) =
            create_test_agent_with_embeddings("semantic-recall", Some(test_embeddings_client()));

        agent
            .store_message(MessageRole::User, "Alpha question")
            .await
            .unwrap();
        agent
            .store_message(MessageRole::Assistant, "Alpha answer")
            .await
            .unwrap();
        agent
            .store_message(MessageRole::User, "Beta prompt")
            .await
            .unwrap();
        agent
            .store_message(MessageRole::Assistant, "Beta reply")
            .await
            .unwrap();

        let recall = agent.recall_memories("alpha follow up").await.unwrap();
        assert!(matches!(
            recall.stats.as_ref().map(|s| &s.strategy),
            Some(MemoryRecallStrategy::Semantic { .. })
        ));
        assert_eq!(
            recall
                .stats
                .as_ref()
                .map(|s| s.matches.len())
                .unwrap_or_default(),
            2
        );

        let recalled = recall.messages;
        assert_eq!(recalled.len(), 4);
        assert_eq!(recalled[0].content, "Beta prompt");
        assert_eq!(recalled[1].content, "Beta reply");

        let tail: Vec<_> = recalled[2..].iter().map(|m| m.content.as_str()).collect();
        assert!(tail.contains(&"Alpha question"));
        assert!(tail.contains(&"Alpha answer"));
    }

    #[tokio::test]
    async fn test_agent_tool_call_parsing() {
        let (agent, _dir) = create_test_agent("tool-parse-test");

        // Valid tool call
        let response = "TOOL_CALL: echo\nARGS: {\"message\": \"hello\"}";
        let parsed = agent.parse_tool_call(response);
        assert!(parsed.is_some());
        let (name, args) = parsed.unwrap();
        assert_eq!(name, "echo");
        assert_eq!(args["message"], "hello");

        // No tool call
        let response = "Just a regular response";
        let parsed = agent.parse_tool_call(response);
        assert!(parsed.is_none());

        // Malformed tool call
        let response = "TOOL_CALL: echo\nARGS: invalid json";
        let parsed = agent.parse_tool_call(response);
        assert!(parsed.is_none());
    }

    #[tokio::test]
    async fn test_agent_tool_permission_allowed() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let mut profile = AgentProfile {
            prompt: Some("Test".to_string()),
            style: None,
            temperature: Some(0.7),
            model_provider: None,
            model_name: None,
            allowed_tools: Some(vec!["echo".to_string()]),
            denied_tools: None,
            memory_k: 5,
            top_p: 0.9,
            max_context_tokens: Some(2048),
            enable_graph: false,
            graph_memory: false,
            auto_graph: false,
            graph_steering: false,
            graph_depth: 3,
            graph_weight: 0.5,
            graph_threshold: 0.7,
            fast_reasoning: false,
            fast_model_provider: None,
            fast_model_name: None,
            fast_model_temperature: 0.3,
            fast_model_tasks: vec![],
            escalation_threshold: 0.6,
        };

        let provider = Arc::new(MockProvider::new("Test"));
        let tool_registry = Arc::new(crate::tools::ToolRegistry::new());

        // Create policy engine with permissive rule for testing
        let mut policy_engine = PolicyEngine::new();
        policy_engine.add_rule(crate::policy::PolicyRule {
            agent: "*".to_string(),
            action: "tool_call".to_string(),
            resource: "*".to_string(),
            effect: crate::policy::PolicyEffect::Allow,
        });
        let policy_engine = Arc::new(policy_engine);

        let agent = AgentCore::new(
            profile.clone(),
            provider.clone(),
            None,
            persistence.clone(),
            "test-session".to_string(),
            Some("policy-test".to_string()),
            tool_registry.clone(),
            policy_engine.clone(),
        );

        assert!(agent.is_tool_allowed("echo"));
        assert!(!agent.is_tool_allowed("math"));

        // Test with denied list
        profile.allowed_tools = None;
        profile.denied_tools = Some(vec!["math".to_string()]);

        let agent = AgentCore::new(
            profile,
            provider,
            None,
            persistence,
            "test-session-2".to_string(),
            Some("policy-test-2".to_string()),
            tool_registry,
            policy_engine,
        );

        assert!(agent.is_tool_allowed("echo"));
        assert!(!agent.is_tool_allowed("math"));
    }

    #[tokio::test]
    async fn test_agent_tool_execution_with_logging() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let persistence = Persistence::new(&db_path).unwrap();

        let profile = AgentProfile {
            prompt: Some("Test".to_string()),
            style: None,
            temperature: Some(0.7),
            model_provider: None,
            model_name: None,
            allowed_tools: Some(vec!["echo".to_string()]),
            denied_tools: None,
            memory_k: 5,
            top_p: 0.9,
            max_context_tokens: Some(2048),
            enable_graph: false,
            graph_memory: false,
            auto_graph: false,
            graph_steering: false,
            graph_depth: 3,
            graph_weight: 0.5,
            graph_threshold: 0.7,
            fast_reasoning: false,
            fast_model_provider: None,
            fast_model_name: None,
            fast_model_temperature: 0.3,
            fast_model_tasks: vec![],
            escalation_threshold: 0.6,
        };

        let provider = Arc::new(MockProvider::new("Test"));

        // Create tool registry and register echo tool
        let mut tool_registry = crate::tools::ToolRegistry::new();
        tool_registry.register(Arc::new(crate::tools::builtin::EchoTool::new()));

        let policy_engine = Arc::new(PolicyEngine::new());

        let agent = AgentCore::new(
            profile,
            provider,
            None,
            persistence.clone(),
            "tool-exec-test".to_string(),
            Some("tool-agent".to_string()),
            Arc::new(tool_registry),
            policy_engine,
        );

        // Execute tool directly
        let args = serde_json::json!({"message": "test message"});
        let result = agent
            .execute_tool("run-tool-test", "echo", &args)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "test message");

        // Verify tool execution was logged (we can't easily check DB here without more setup)
    }

    #[tokio::test]
    async fn test_agent_tool_registry_access() {
        let (agent, _dir) = create_test_agent("registry-test");

        let registry = agent.tool_registry();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_goal_requires_tool_detection() {
        assert!(AgentCore::goal_requires_tool(
            "List the files in this directory"
        ));
        assert!(AgentCore::goal_requires_tool("Run the tests"));
        assert!(!AgentCore::goal_requires_tool("Explain recursion"));
    }
}
