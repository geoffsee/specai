//! Agent Core Execution Loop
//!
//! The heart of the agent system - orchestrates reasoning, memory, and model interaction.

use crate::agent::model::{GenerationConfig, ModelProvider};
pub use crate::agent::output::{
    AgentOutput, GraphDebugInfo, GraphDebugNode, MemoryRecallMatch, MemoryRecallStats,
    MemoryRecallStrategy, ToolInvocation,
};
use crate::config::agent::AgentProfile;
use crate::embeddings::EmbeddingsClient;
use crate::persistence::Persistence;
use crate::policy::{PolicyDecision, PolicyEngine};
use crate::spec::AgentSpec;
use crate::tools::{ToolRegistry, ToolResult};
use crate::types::{EdgeType, Message, MessageRole, NodeType, TraversalDirection};
use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

const DEFAULT_MAIN_TEMPERATURE: f32 = 0.7;
const DEFAULT_TOP_P: f32 = 0.9;
const DEFAULT_FAST_TEMPERATURE: f32 = 0.3;
const DEFAULT_ESCALATION_THRESHOLD: f32 = 0.6;

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
    /// Cache for tool permission checks to avoid repeated lookups
    tool_permission_cache: std::cell::RefCell<std::collections::HashMap<String, bool>>,
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
            tool_permission_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
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
        self.tool_permission_cache.borrow_mut().clear();
        self
    }

    /// Execute a single interaction step
    pub async fn run_step(&mut self, input: &str) -> Result<AgentOutput> {
        let run_id = format!("run-{}", Utc::now().timestamp_micros());
        let total_timer = Instant::now();

        // Step 1: Recall relevant memories
        let recall_timer = Instant::now();
        let recall_result = self.recall_memories(input).await?;
        self.log_timing("run_step.recall_memories", recall_timer);
        let recalled_messages = recall_result.messages;
        let recall_stats = recall_result.stats;

        // Step 2: Build prompt with context
        let prompt_timer = Instant::now();
        let mut prompt = self.build_prompt(input, &recalled_messages)?;
        self.log_timing("run_step.build_prompt", prompt_timer);

        // Step 3: Store user message
        let store_user_timer = Instant::now();
        let user_message_id = self.store_message(MessageRole::User, input).await?;
        self.log_timing("run_step.store_user_message", store_user_timer);

        // Track user goal context (graph-driven planning)
        let mut goal_context =
            Some(self.create_goal_context(user_message_id, input, self.profile.enable_graph)?);

        // Step 4: Agent loop with tool execution
        let mut tool_invocations = Vec::new();
        let mut final_response = String::new();
        let mut token_usage = None;
        let mut finish_reason = None;
        let mut auto_response: Option<String> = None;
        let mut reasoning: Option<String> = None;
        let mut reasoning_summary: Option<String> = None;

        // Attempt to auto-satisfy simple goals before invoking the model
        if let Some(goal) = goal_context.as_mut() {
            if goal.requires_tool {
                if let Some((tool_name, tool_args)) =
                    Self::infer_goal_tool_action(goal.text.as_str())
                {
                    if self.is_tool_allowed(&tool_name) {
                        let tool_timer = Instant::now();
                        let tool_result = self.execute_tool(&run_id, &tool_name, &tool_args).await;
                        self.log_timing("run_step.tool_execution.auto", tool_timer);
                        match tool_result {
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

        // Fast-model routing (when enabled) happens only if we still need a model response
        let mut fast_model_final: Option<(String, f32)> = None;
        if !skip_model {
            if let Some(task_type) = self.detect_task_type(input) {
                let complexity = self.estimate_task_complexity(input);
                if self.should_use_fast_model(&task_type, complexity) {
                    let fast_timer = Instant::now();
                    let fast_result = self.fast_reasoning(&task_type, input).await;
                    self.log_timing("run_step.fast_reasoning_attempt", fast_timer);
                    match fast_result {
                        Ok((fast_text, confidence)) => {
                            if confidence >= self.escalation_threshold() {
                                fast_model_final = Some((fast_text, confidence));
                            } else {
                                prompt.push_str(&format!(
                                    "\n\nFAST_MODEL_HINT (task={} confidence={:.0}%):\n{}\n\nRefine this hint and produce a complete answer.",
                                    task_type,
                                    (confidence * 100.0).round(),
                                    fast_text
                                ));
                            }
                        }
                        Err(err) => {
                            warn!("Fast reasoning failed for task {}: {}", task_type, err);
                        }
                    }
                }
            }
        }

        if skip_model {
            final_response = auto_response.unwrap_or_else(|| "Task completed.".to_string());
            finish_reason = Some("auto_tool".to_string());
        } else if let Some((fast_text, confidence)) = fast_model_final {
            final_response = fast_text;
            finish_reason = Some(format!("fast_model ({:.0}%)", (confidence * 100.0).round()));
        } else {
            // Allow up to 5 iterations to handle tool calls
            for _iteration in 0..5 {
                // Generate response using model
                let generation_config = self.build_generation_config();
                let model_timer = Instant::now();
                let response_result = self.provider.generate(&prompt, &generation_config).await;
                self.log_timing("run_step.main_model_call", model_timer);
                let response = response_result.context("Failed to generate response from model")?;

                token_usage = response.usage;
                finish_reason = response.finish_reason.clone();
                final_response = response.content.clone();
                reasoning = response.reasoning.clone();

                // Summarize reasoning if present
                if let Some(ref reasoning_text) = reasoning {
                    reasoning_summary = self.summarize_reasoning(reasoning_text).await;
                }

                // Check for SDK-native tool calls (function calling)
                let sdk_tool_calls = response.tool_calls.clone().unwrap_or_default();

                // Early termination: if no tool calls and response is complete, break immediately
                if sdk_tool_calls.is_empty() {
                    // Check if finish_reason indicates completion
                    let is_complete = finish_reason.as_ref().map_or(false, |reason| {
                        let reason_lower = reason.to_lowercase();
                        reason_lower.contains("stop")
                            || reason_lower.contains("end_turn")
                            || reason_lower.contains("complete")
                            || reason_lower == "length"
                    });

                    // If no goal constraint requires tools, terminate early
                    let goal_needs_tool = goal_context
                        .as_ref()
                        .map_or(false, |g| g.requires_tool && !g.satisfied);

                    if is_complete && !goal_needs_tool {
                        debug!("Early termination: response complete with no tool calls needed");
                        break;
                    }
                }

                if !sdk_tool_calls.is_empty() {
                    // Process all tool calls from SDK response
                    for tool_call in sdk_tool_calls {
                        let tool_name = &tool_call.function_name;
                        let tool_args = &tool_call.arguments;

                        // Check if tool is allowed
                        if !self.is_tool_allowed(tool_name) {
                            let error_msg =
                                format!("Tool '{}' is not allowed by agent policy", tool_name);
                            warn!("{}", error_msg);
                            tool_invocations.push(ToolInvocation {
                                name: tool_name.clone(),
                                arguments: tool_args.clone(),
                                success: false,
                                output: None,
                                error: Some(error_msg),
                            });
                            continue;
                        }

                        // Execute tool
                        let tool_timer = Instant::now();
                        let exec_result = self.execute_tool(&run_id, tool_name, tool_args).await;
                        self.log_timing("run_step.tool_execution.sdk", tool_timer);
                        match exec_result {
                            Ok(result) => {
                                let invocation = ToolInvocation::from_result(
                                    tool_name,
                                    tool_args.clone(),
                                    &result,
                                );
                                let tool_output = invocation.output.clone().unwrap_or_default();
                                let was_success = invocation.success;
                                let error_message = invocation
                                    .error
                                    .clone()
                                    .unwrap_or_else(|| "Tool execution failed".to_string());
                                tool_invocations.push(invocation);

                                if let Some(goal) = goal_context.as_mut() {
                                    if let Err(err) = self.record_goal_tool_result(
                                        goal, tool_name, tool_args, &result,
                                    ) {
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
                                }
                            }
                            Err(e) => {
                                let error_msg =
                                    format!("Error executing tool '{}': {}", tool_name, e);
                                warn!("{}", error_msg);
                                prompt.push_str(&format!(
                                    "\n\nTOOL_ERROR: {}\n\nPlease continue without this tool.",
                                    error_msg
                                ));
                                tool_invocations.push(ToolInvocation {
                                    name: tool_name.clone(),
                                    arguments: tool_args.clone(),
                                    success: false,
                                    output: None,
                                    error: Some(error_msg),
                                });
                            }
                        }
                    }

                    // Continue loop to process tool results
                    continue;
                }

                if let Some(goal) = goal_context.as_ref() {
                    if goal.requires_tool && !goal.satisfied {
                        prompt.push_str(
                            "\n\nGOAL_STATUS: pending. The user request requires executing an allowed tool. Please call an appropriate tool.",
                        );
                        continue;
                    }
                }

                // No tool calls found or response includes final answer, break
                break;
            }
        }

        // Step 5: Store assistant response with reasoning if available
        let store_assistant_timer = Instant::now();
        let response_message_id = self
            .store_message_with_reasoning(
                MessageRole::Assistant,
                &final_response,
                reasoning.as_deref(),
            )
            .await?;
        self.log_timing("run_step.store_assistant_message", store_assistant_timer);

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

        // Step 7: Re-evaluate knowledge graph to recommend next action
        // Skip graph evaluation for very short conversations (< 3 messages) as there's insufficient context
        let next_action_recommendation =
            if self.profile.enable_graph && self.conversation_history.len() >= 3 {
                let graph_timer = Instant::now();
                let recommendation = self.evaluate_graph_for_next_action(
                    user_message_id,
                    response_message_id,
                    &final_response,
                    &tool_invocations,
                )?;
                self.log_timing("run_step.evaluate_graph_for_next_action", graph_timer);
                recommendation
            } else {
                None
            };

        // Persist steering insight as a synthetic system message for future turns
        if let Some(ref recommendation) = next_action_recommendation {
            info!("Knowledge graph recommends next action: {}", recommendation);
            let system_content = format!("Graph recommendation: {}", recommendation);
            let system_store_timer = Instant::now();
            let system_message_id = self
                .store_message(MessageRole::System, &system_content)
                .await?;
            self.log_timing("run_step.store_system_message", system_store_timer);

            self.conversation_history.push(Message {
                id: system_message_id,
                session_id: self.session_id.clone(),
                role: MessageRole::System,
                content: system_content,
                created_at: Utc::now(),
            });
        }

        let graph_debug = match self.snapshot_graph_debug_info() {
            Ok(info) => Some(info),
            Err(err) => {
                warn!("Failed to capture graph debug info: {}", err);
                None
            }
        };

        self.log_timing("run_step.total", total_timer);

        Ok(AgentOutput {
            response: final_response,
            response_message_id: Some(response_message_id),
            token_usage,
            tool_invocations,
            finish_reason,
            recall_stats,
            run_id,
            next_action: next_action_recommendation,
            reasoning,
            reasoning_summary,
            graph_debug,
        })
    }

    /// Execute a structured spec by converting it into a single prompt.
    pub async fn run_spec(&mut self, spec: &AgentSpec) -> Result<AgentOutput> {
        debug!(
            "Executing structured spec '{}' (source: {:?})",
            spec.display_name(),
            spec.source_path()
        );
        let prompt = spec.to_prompt();
        self.run_step(&prompt).await
    }

    /// Build generation configuration from profile
    fn build_generation_config(&self) -> GenerationConfig {
        let temperature = match self.profile.temperature {
            Some(temp) if temp.is_finite() => Some(temp.clamp(0.0, 2.0)),
            Some(_) => {
                warn!(
                    "Ignoring invalid temperature for agent {:?}, falling back to {}",
                    self.agent_name, DEFAULT_MAIN_TEMPERATURE
                );
                Some(DEFAULT_MAIN_TEMPERATURE)
            }
            None => None,
        };

        let top_p = if self.profile.top_p.is_finite() {
            Some(self.profile.top_p.clamp(0.0, 1.0))
        } else {
            warn!(
                "Invalid top_p detected for agent {:?}, falling back to {}",
                self.agent_name, DEFAULT_TOP_P
            );
            Some(DEFAULT_TOP_P)
        };

        GenerationConfig {
            temperature,
            max_tokens: self.profile.max_context_tokens.map(|t| t as u32),
            stop_sequences: None,
            top_p,
            frequency_penalty: None,
            presence_penalty: None,
        }
    }

    fn snapshot_graph_debug_info(&self) -> Result<GraphDebugInfo> {
        let mut info = GraphDebugInfo {
            enabled: self.profile.enable_graph,
            graph_memory_enabled: self.profile.graph_memory,
            auto_graph_enabled: self.profile.auto_graph,
            graph_steering_enabled: self.profile.graph_steering,
            node_count: 0,
            edge_count: 0,
            recent_nodes: Vec::new(),
        };

        if !self.profile.enable_graph {
            return Ok(info);
        }

        info.node_count = self.persistence.count_graph_nodes(&self.session_id)?.max(0) as usize;
        info.edge_count = self.persistence.count_graph_edges(&self.session_id)?.max(0) as usize;

        let recent_nodes = self
            .persistence
            .list_graph_nodes(&self.session_id, None, Some(5))?;
        info.recent_nodes = recent_nodes
            .into_iter()
            .map(|node| GraphDebugNode {
                id: node.id,
                node_type: node.node_type.as_str().to_string(),
                label: node.label,
            })
            .collect();

        Ok(info)
    }

    /// Summarize reasoning using the fast model
    async fn summarize_reasoning(&self, reasoning: &str) -> Option<String> {
        // Only summarize if we have a fast provider and reasoning is substantial
        let fast_provider = self.fast_provider.as_ref()?;

        if reasoning.len() < 50 {
            // Too short to summarize, just return it as-is
            return Some(reasoning.to_string());
        }

        let summary_prompt = format!(
            "Summarize the following reasoning in 1-2 concise sentences that explain the thought process:\n\n{}\n\nSummary:",
            reasoning
        );

        let config = GenerationConfig {
            temperature: Some(0.3),
            max_tokens: Some(100),
            stop_sequences: None,
            top_p: Some(0.9),
            frequency_penalty: None,
            presence_penalty: None,
        };

        let timer = Instant::now();
        let response = fast_provider.generate(&summary_prompt, &config).await;
        self.log_timing("summarize_reasoning.generate", timer);
        match response {
            Ok(response) => {
                let summary = response.content.trim().to_string();
                if !summary.is_empty() {
                    debug!("Generated reasoning summary: {}", summary);
                    Some(summary)
                } else {
                    None
                }
            }
            Err(e) => {
                warn!("Failed to summarize reasoning: {}", e);
                None
            }
        }
    }

    /// Recall relevant memories for the given input
    async fn recall_memories(&self, query: &str) -> Result<RecallResult> {
        const RECENT_CONTEXT: i64 = 2;
        // const MIN_MESSAGES_FOR_SEMANTIC_RECALL: usize = 3;
        let mut context = Vec::new();
        let mut seen_ids = HashSet::new();

        let recent_messages = self
            .persistence
            .list_messages(&self.session_id, RECENT_CONTEXT)?;

        // Optimization: Skip semantic recall for very new sessions (first interaction only)
        // This saves embedding generation time when there's insufficient history
        if self.conversation_history.is_empty() && recent_messages.is_empty() {
            return Ok(RecallResult {
                messages: Vec::new(),
                stats: Some(MemoryRecallStats {
                    strategy: MemoryRecallStrategy::RecentContext {
                        limit: RECENT_CONTEXT as usize,
                    },
                    matches: Vec::new(),
                }),
            });
        }

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

            let embed_timer = Instant::now();
            let embed_result = client.embed_batch(&[query]).await;
            self.log_timing("recall_memories.embed_batch", embed_timer);
            match embed_result {
                Ok(mut embeddings) => match embeddings.pop() {
                    Some(query_embedding) if !query_embedding.is_empty() => {
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

                            // Combine semantic and graph-expanded context with weighted limits
                            let total_slots = self.profile.memory_k.max(1);
                            let mut graph_limit =
                                ((total_slots as f32) * self.profile.graph_weight).round() as usize;
                            graph_limit = graph_limit.min(total_slots);
                            if graph_limit == 0 && !graph_expanded.is_empty() {
                                graph_limit = 1;
                            }

                            let mut semantic_limit = total_slots.saturating_sub(graph_limit);
                            if semantic_limit == 0 && !semantic_context.is_empty() {
                                semantic_limit = 1;
                                graph_limit = graph_limit.saturating_sub(1);
                            }

                            let mut limited_semantic = semantic_context;
                            if limited_semantic.len() > semantic_limit && semantic_limit > 0 {
                                limited_semantic.truncate(semantic_limit);
                            }

                            let mut limited_graph = graph_expanded;
                            if limited_graph.len() > graph_limit && graph_limit > 0 {
                                limited_graph.truncate(graph_limit);
                            }

                            context.extend(limited_semantic);
                            context.extend(limited_graph);
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
                    _ => {
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
                },
                Err(err) => {
                    warn!("Failed to embed recall query: {}", err);
                    return Ok(RecallResult {
                        messages: context,
                        stats: None,
                    });
                }
            }
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
        if let Some(system_prompt) = &self.profile.prompt {
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
        self.store_message_with_reasoning(role, content, None).await
    }

    /// Store a message in persistence with optional reasoning metadata
    async fn store_message_with_reasoning(
        &self,
        role: MessageRole,
        content: &str,
        reasoning: Option<&str>,
    ) -> Result<i64> {
        let message_id = self
            .persistence
            .insert_message(&self.session_id, role, content)
            .context("Failed to store message")?;

        let mut embedding_id = None;

        if let Some(client) = &self.embeddings_client {
            if !content.trim().is_empty() {
                let embed_timer = Instant::now();
                let embed_result = client.embed_batch(&[content]).await;
                self.log_timing("embeddings.message_content", embed_timer);
                match embed_result {
                    Ok(mut embeddings) => {
                        if let Some(embedding) = embeddings.pop() {
                            if !embedding.is_empty() {
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
                        }
                    }
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
            self.build_graph_for_message(message_id, role, content, embedding_id, reasoning)?;
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
        reasoning: Option<&str>,
    ) -> Result<()> {
        use serde_json::json;

        // Create a node for the message
        let mut message_props = json!({
            "message_id": message_id,
            "role": role.as_str(),
            "content_preview": preview_text(content),
            "timestamp": Utc::now().to_rfc3339(),
        });

        // Add reasoning preview if available
        if let Some(reasoning_text) = reasoning {
            if !reasoning_text.is_empty() {
                message_props["has_reasoning"] = json!(true);
                message_props["reasoning_preview"] = json!(preview_text(reasoning_text));
            }
        }

        let message_node_id = self.persistence.insert_graph_node(
            &self.session_id,
            NodeType::Message,
            &format!("{:?}Message", role),
            &message_props,
            embedding_id,
        )?;

        // Extract entities and concepts from the message content
        let mut entities = self.extract_entities_from_text(content);
        let mut concepts = self.extract_concepts_from_text(content);

        // Also extract entities and concepts from reasoning if available
        // This provides richer context for the knowledge graph
        if let Some(reasoning_text) = reasoning {
            if !reasoning_text.is_empty() {
                debug!(
                    "Extracting entities/concepts from reasoning for message {}",
                    message_id
                );
                let reasoning_entities = self.extract_entities_from_text(reasoning_text);
                let reasoning_concepts = self.extract_concepts_from_text(reasoning_text);

                // Merge reasoning entities with content entities (boosting confidence for duplicates)
                for mut reasoning_entity in reasoning_entities {
                    // Check if this entity was already extracted from content
                    if let Some(existing) = entities.iter_mut().find(|e| {
                        e.name.to_lowercase() == reasoning_entity.name.to_lowercase()
                            && e.entity_type == reasoning_entity.entity_type
                    }) {
                        // Boost confidence if entity appears in both content and reasoning
                        existing.confidence =
                            (existing.confidence + reasoning_entity.confidence * 0.5).min(1.0);
                    } else {
                        // New entity from reasoning, add with slightly lower confidence
                        reasoning_entity.confidence *= 0.8;
                        entities.push(reasoning_entity);
                    }
                }

                // Merge reasoning concepts with content concepts
                for mut reasoning_concept in reasoning_concepts {
                    if let Some(existing) = concepts
                        .iter_mut()
                        .find(|c| c.name.to_lowercase() == reasoning_concept.name.to_lowercase())
                    {
                        existing.relevance =
                            (existing.relevance + reasoning_concept.relevance * 0.5).min(1.0);
                    } else {
                        reasoning_concept.relevance *= 0.8;
                        concepts.push(reasoning_concept);
                    }
                }
            }
        }

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
            let timestamp = Utc::now().to_rfc3339();
            let mut properties = json!({
                "tool": tool_name,
                "arguments": args,
                "success": result.success,
                "output_preview": preview_text(&result.output),
                "error": result.error,
                "timestamp": timestamp,
            });

            let mut prompt_payload: Option<Value> = None;
            if tool_name == "prompt_user" && result.success {
                match serde_json::from_str::<Value>(&result.output) {
                    Ok(payload) => {
                        if let Some(props) = properties.as_object_mut() {
                            props.insert("prompt_user_payload".to_string(), payload.clone());
                            if let Some(response) = payload.get("response") {
                                props.insert(
                                    "response_preview".to_string(),
                                    Value::String(preview_json_value(response)),
                                );
                            }
                        }
                        prompt_payload = Some(payload);
                    }
                    Err(err) => {
                        warn!("Failed to parse prompt_user payload for graph: {}", err);
                        if let Some(props) = properties.as_object_mut() {
                            props.insert(
                                "prompt_user_parse_error".to_string(),
                                Value::String(err.to_string()),
                            );
                        }
                    }
                }
            }

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

            if let Some(payload) = prompt_payload {
                let response_preview = payload
                    .get("response")
                    .map(|v| preview_json_value(v))
                    .unwrap_or_default();

                let response_properties = json!({
                    "prompt": payload_field(&payload, "prompt"),
                    "input_type": payload_field(&payload, "input_type"),
                    "response": payload_field(&payload, "response"),
                    "display_value": payload_field(&payload, "display_value"),
                    "selections": payload_field(&payload, "selections"),
                    "metadata": payload_field(&payload, "metadata"),
                    "used_default": payload_field(&payload, "used_default"),
                    "used_prefill": payload_field(&payload, "used_prefill"),
                    "response_preview": response_preview,
                    "timestamp": timestamp,
                });

                let response_node_id = self.persistence.insert_graph_node(
                    &self.session_id,
                    NodeType::Event,
                    "UserInput",
                    &response_properties,
                    None,
                )?;

                self.persistence.insert_graph_edge(
                    &self.session_id,
                    tool_node_id,
                    response_node_id,
                    EdgeType::Produces,
                    Some("captures_input"),
                    None,
                    1.0,
                )?;

                self.persistence.insert_graph_edge(
                    &self.session_id,
                    response_node_id,
                    goal_node_id,
                    EdgeType::RelatesTo,
                    Some("addresses_goal"),
                    None,
                    0.9,
                )?;
            }
        }
        Ok(())
    }

    fn goal_requires_tool(input: &str) -> bool {
        let normalized = input.to_lowercase();
        const ACTION_VERBS: [&str; 18] = [
            "list", "show", "read", "write", "create", "update", "delete", "run", "execute",
            "open", "search", "fetch", "download", "scan", "compile", "test", "build", "inspect",
        ];

        if ACTION_VERBS
            .iter()
            .any(|verb| normalized.contains(verb) && normalized.contains(' '))
        {
            return true;
        }

        // Treat common "what is the project here" style questions
        // as requiring a tool so the agent can inspect the local workspace.
        let mentions_local_context = normalized.contains("this directory")
            || normalized.contains("current directory")
            || normalized.contains("this folder")
            || normalized.contains("here");

        let mentions_project = normalized.contains("this project")
            || normalized.contains("this repo")
            || normalized.contains("this repository")
            || normalized.contains("this codebase")
            // e.g., "project in this directory", "repo in the current directory"
            || ((normalized.contains("project")
                || normalized.contains("repo")
                || normalized.contains("repository")
                || normalized.contains("codebase"))
                && mentions_local_context);

        let asks_about_project = normalized.contains("what can")
            || normalized.contains("what is")
            || normalized.contains("what does")
            || normalized.contains("tell me")
            || normalized.contains("describe")
            || normalized.contains("about");

        mentions_project && asks_about_project
    }

    fn escalation_threshold(&self) -> f32 {
        if self.profile.escalation_threshold.is_finite() {
            self.profile.escalation_threshold.clamp(0.0, 1.0)
        } else {
            warn!(
                "Invalid escalation_threshold for agent {:?}, defaulting to {}",
                self.agent_name, DEFAULT_ESCALATION_THRESHOLD
            );
            DEFAULT_ESCALATION_THRESHOLD
        }
    }

    fn detect_task_type(&self, input: &str) -> Option<String> {
        if !self.profile.fast_reasoning || self.fast_provider.is_none() {
            return None;
        }

        let text = input.to_lowercase();

        let candidates: [(&str, &[&str]); 6] = [
            ("entity_extraction", &["entity", "extract", "named"]),
            ("decision_routing", &["classify", "categorize", "route"]),
            (
                "tool_selection",
                &["which tool", "use which tool", "tool should"],
            ),
            ("confidence_scoring", &["confidence", "certainty"]),
            ("summarization", &["summarize", "summary"]),
            ("graph_analysis", &["graph", "connection", "relationships"]),
        ];

        for (task, keywords) in candidates {
            if keywords.iter().any(|kw| text.contains(kw)) {
                if self.profile.fast_model_tasks.iter().any(|t| t == task) {
                    return Some(task.to_string());
                }
            }
        }

        None
    }

    fn estimate_task_complexity(&self, input: &str) -> f32 {
        let words = input.split_whitespace().count() as f32;
        let clauses =
            input.matches(" and ").count() as f32 + input.matches(" then ").count() as f32;
        let newlines = input.matches('\n').count() as f32;

        let length_factor = (words / 120.0).min(1.0);
        let clause_factor = (clauses / 4.0).min(1.0);
        let structure_factor = (newlines / 5.0).min(1.0);

        (0.6 * length_factor + 0.3 * clause_factor + 0.1 * structure_factor).clamp(0.0, 1.0)
    }

    fn infer_goal_tool_action(goal_text: &str) -> Option<(String, Value)> {
        let text = goal_text.to_lowercase();

        // Handle project/repo description requests by reading the README when available
        let mentions_local_context = text.contains("this directory")
            || text.contains("current directory")
            || text.contains("this folder")
            || text.contains("here");

        let mentions_project = text.contains("this project")
            || text.contains("this repo")
            || text.contains("this repository")
            || text.contains("this codebase")
            || ((text.contains("project")
                || text.contains("repo")
                || text.contains("repository")
                || text.contains("codebase"))
                && mentions_local_context);

        let asks_about_project = text.contains("what can")
            || text.contains("what is")
            || text.contains("what does")
            || text.contains("tell me")
            || text.contains("describe")
            || text.contains("about");

        if mentions_project && asks_about_project {
            // Prefer a README file if present in the current directory
            for candidate in &["README.md", "Readme.md", "readme.md"] {
                if Path::new(candidate).exists() {
                    return Some((
                        "file_read".to_string(),
                        json!({
                            "path": candidate,
                            "max_bytes": 65536
                        }),
                    ));
                }
            }

            // Fallback: scan common manifest files to infer project type
            return Some((
                "search".to_string(),
                json!({
                    "query": "Cargo.toml|package.json|pyproject.toml|setup.py",
                    "regex": true,
                    "case_sensitive": false,
                    "max_results": 20
                }),
            ));
        }

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

    fn parse_confidence(text: &str) -> Option<f32> {
        for line in text.lines() {
            let lower = line.to_lowercase();
            if lower.contains("confidence") {
                let token = lower
                    .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                    .find(|chunk| !chunk.is_empty())?;
                if let Ok(value) = token.parse::<f32>() {
                    if (0.0..=1.0).contains(&value) {
                        return Some(value);
                    }
                }
            }
        }
        None
    }

    fn strip_fast_answer(text: &str) -> String {
        let mut answer = String::new();
        for line in text.lines() {
            if line.to_lowercase().starts_with("answer:") {
                answer.push_str(line.splitn(2, ':').nth(1).unwrap_or("").trim());
                break;
            }
        }
        if answer.is_empty() {
            text.trim().to_string()
        } else {
            answer
        }
    }

    fn format_auto_tool_response(tool_name: &str, output: Option<&str>) -> String {
        let sanitized = output.unwrap_or("").trim();
        if sanitized.is_empty() {
            return format!("Executed `{}` successfully.", tool_name);
        }

        if tool_name == "file_read" {
            if let Ok(value) = serde_json::from_str::<Value>(sanitized) {
                let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                let content = value.get("content").and_then(|v| v.as_str()).unwrap_or("");

                // Truncate very large files for display
                let max_chars = 4000;
                let display_content = if content.len() > max_chars {
                    let mut snippet = content[..max_chars].to_string();
                    snippet.push_str("\n...\n[truncated]");
                    snippet
                } else {
                    content.to_string()
                };

                return format!("Contents of {}:\n{}", path, display_content);
            }
        }

        if tool_name == "search" {
            if let Ok(value) = serde_json::from_str::<Value>(sanitized) {
                let query = value.get("query").and_then(|v| v.as_str()).unwrap_or("");

                if let Some(results) = value.get("results").and_then(|v| v.as_array()) {
                    if results.is_empty() {
                        return if query.is_empty() {
                            "Search returned no results.".to_string()
                        } else {
                            format!("Search for {:?} returned no results.", query)
                        };
                    }

                    let mut lines = Vec::new();
                    if query.is_empty() {
                        lines.push("Search results:".to_string());
                    } else {
                        lines.push(format!("Search results for {:?}:", query));
                    }

                    for entry in results.iter().take(5) {
                        let path = entry
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("<unknown>");
                        let line = entry.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                        let snippet = entry
                            .get("snippet")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .replace('\n', " ");

                        lines.push(format!("- {}:{} - {}", path, line, snippet));
                    }

                    return lines.join("\n");
                }
            }
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
    async fn fast_reasoning(&self, task: &str, input: &str) -> Result<(String, f32)> {
        let total_timer = Instant::now();
        let result = if let Some(ref fast_provider) = self.fast_provider {
            let prompt = format!(
                "You are a fast specialist model that assists a more capable agent.\nTask: {}\nInput: {}\n\nRespond with two lines:\nAnswer: <concise result>\nConfidence: <0-1 decimal>",
                task, input
            );

            let fast_temperature = if self.profile.fast_model_temperature.is_finite() {
                self.profile.fast_model_temperature.clamp(0.0, 2.0)
            } else {
                warn!(
                    "Invalid fast_model_temperature for agent {:?}, using {}",
                    self.agent_name, DEFAULT_FAST_TEMPERATURE
                );
                DEFAULT_FAST_TEMPERATURE
            };

            let config = GenerationConfig {
                temperature: Some(fast_temperature),
                max_tokens: Some(256), // Keep responses short for speed
                stop_sequences: None,
                top_p: Some(DEFAULT_TOP_P),
                frequency_penalty: None,
                presence_penalty: None,
            };

            let call_timer = Instant::now();
            let response_result = fast_provider.generate(&prompt, &config).await;
            self.log_timing("fast_reasoning.generate", call_timer);
            let response = response_result?;

            let confidence = Self::parse_confidence(&response.content).unwrap_or(0.7);
            let cleaned = Self::strip_fast_answer(&response.content);

            Ok((cleaned, confidence))
        } else {
            // No fast model configured
            Ok((String::new(), 0.0))
        };

        self.log_timing("fast_reasoning.total", total_timer);
        result
    }

    /// Decide whether to use fast or main model based on task complexity
    fn should_use_fast_model(&self, task_type: &str, complexity_score: f32) -> bool {
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
        let threshold = self.escalation_threshold();
        if complexity_score > threshold {
            info!(
                "Task complexity {} exceeds threshold {}, using main model",
                complexity_score, threshold
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

    /// Check if a tool is allowed by the agent profile and policy engine
    fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // Check cache first to avoid repeated permission lookups
        {
            let cache = self.tool_permission_cache.borrow();
            if let Some(&allowed) = cache.get(tool_name) {
                return allowed;
            }
        }

        // First check profile-level permissions (backward compatibility)
        let profile_allowed = self.profile.is_tool_allowed(tool_name);
        debug!(
            "Profile check for tool '{}': allowed={}, allowed_tools={:?}, denied_tools={:?}",
            tool_name, profile_allowed, self.profile.allowed_tools, self.profile.denied_tools
        );
        if !profile_allowed {
            self.tool_permission_cache
                .borrow_mut()
                .insert(tool_name.to_string(), false);
            return false;
        }

        // Then check policy engine
        let agent_name = self.agent_name.as_deref().unwrap_or("agent");
        let decision = self.policy_engine.check(agent_name, "tool_call", tool_name);
        debug!(
            "Policy check for tool '{}': decision={:?}",
            tool_name, decision
        );

        let allowed = matches!(decision, PolicyDecision::Allow);
        self.tool_permission_cache
            .borrow_mut()
            .insert(tool_name.to_string(), allowed);
        allowed
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

    /// Evaluate the knowledge graph to recommend a next action based on context
    fn evaluate_graph_for_next_action(
        &self,
        user_message_id: i64,
        assistant_message_id: i64,
        response_content: &str,
        tool_invocations: &[ToolInvocation],
    ) -> Result<Option<String>> {
        // Find the assistant message node in the graph
        let nodes = self.persistence.list_graph_nodes(
            &self.session_id,
            Some(NodeType::Message),
            Some(50),
        )?;

        let mut assistant_node_id = None;
        let mut _user_node_id = None;

        for node in &nodes {
            if let Some(msg_id) = node.properties["message_id"].as_i64() {
                if msg_id == assistant_message_id {
                    assistant_node_id = Some(node.id);
                } else if msg_id == user_message_id {
                    _user_node_id = Some(node.id);
                }
            }
        }

        if assistant_node_id.is_none() {
            debug!("Assistant message node not found in graph");
            return Ok(None);
        }

        let assistant_node_id = assistant_node_id.unwrap();

        // Analyze the graph context around the current conversation
        let neighbors = self.persistence.traverse_neighbors(
            &self.session_id,
            assistant_node_id,
            TraversalDirection::Both,
            2, // Look 2 hops away todo: dynamically adjust based on a complexity score
        )?;

        // Analyze goals in the graph
        let goal_nodes =
            self.persistence
                .list_graph_nodes(&self.session_id, Some(NodeType::Goal), Some(10))?;

        let mut pending_goals = Vec::new();
        let mut completed_goals = Vec::new();

        for goal in &goal_nodes {
            if let Some(status) = goal.properties["status"].as_str() {
                match status {
                    "pending" | "in_progress" => {
                        if let Some(goal_text) = goal.properties["goal_text"].as_str() {
                            pending_goals.push(goal_text.to_string());
                        }
                    }
                    "completed" => {
                        if let Some(goal_text) = goal.properties["goal_text"].as_str() {
                            completed_goals.push(goal_text.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Analyze tool results in the graph
        let tool_nodes = self.persistence.list_graph_nodes(
            &self.session_id,
            Some(NodeType::ToolResult),
            Some(10),
        )?;

        let mut recent_tool_failures = Vec::new();
        let mut recent_tool_successes = Vec::new();

        for tool_node in &tool_nodes {
            if let Some(success) = tool_node.properties["success"].as_bool() {
                let tool_name = tool_node.properties["tool"].as_str().unwrap_or("unknown");
                if success {
                    recent_tool_successes.push(tool_name.to_string());
                } else {
                    recent_tool_failures.push(tool_name.to_string());
                }
            }
        }

        // Analyze entities and concepts in the graph
        let mut key_entities = HashSet::new();
        let mut key_concepts = HashSet::new();

        for neighbor in &neighbors {
            match neighbor.node_type {
                NodeType::Entity => {
                    if let Some(name) = neighbor.properties["name"].as_str() {
                        key_entities.insert(name.to_string());
                    }
                }
                NodeType::Concept => {
                    if let Some(name) = neighbor.properties["name"].as_str() {
                        key_concepts.insert(name.to_string());
                    }
                }
                _ => {}
            }
        }

        // Generate recommendation based on graph analysis
        let recommendation = self.generate_action_recommendation(
            &pending_goals,
            &completed_goals,
            &recent_tool_failures,
            &recent_tool_successes,
            &key_entities,
            &key_concepts,
            response_content,
            tool_invocations,
        );

        // If we have a recommendation, create a node for it
        if let Some(ref rec) = recommendation {
            let properties = json!({
                "recommendation": rec,
                "user_message_id": user_message_id,
                "assistant_message_id": assistant_message_id,
                "pending_goals": pending_goals,
                "completed_goals": completed_goals,
                "tool_failures": recent_tool_failures,
                "tool_successes": recent_tool_successes,
                "key_entities": key_entities.into_iter().collect::<Vec<_>>(),
                "key_concepts": key_concepts.into_iter().collect::<Vec<_>>(),
                "timestamp": Utc::now().to_rfc3339(),
            });

            let rec_node_id = self.persistence.insert_graph_node(
                &self.session_id,
                NodeType::Event,
                "NextActionRecommendation",
                &properties,
                None,
            )?;

            // Link recommendation to assistant message
            self.persistence.insert_graph_edge(
                &self.session_id,
                assistant_node_id,
                rec_node_id,
                EdgeType::Produces,
                Some("recommends"),
                None,
                0.8,
            )?;
        }

        Ok(recommendation)
    }

    /// Generate an action recommendation based on graph analysis
    fn generate_action_recommendation(
        &self,
        pending_goals: &[String],
        completed_goals: &[String],
        recent_tool_failures: &[String],
        _recent_tool_successes: &[String],
        _key_entities: &HashSet<String>,
        key_concepts: &HashSet<String>,
        response_content: &str,
        tool_invocations: &[ToolInvocation],
    ) -> Option<String> {
        let mut recommendations = Vec::new();

        // Check for pending goals that need attention
        if !pending_goals.is_empty() {
            let goals_str = pending_goals.join(", ");
            recommendations.push(format!("Continue working on pending goals: {}", goals_str));
        }

        // Check for tool failures that might need retry or alternative approach
        if !recent_tool_failures.is_empty() {
            let unique_failures: HashSet<_> = recent_tool_failures.iter().collect();
            for tool in unique_failures {
                recommendations.push(format!(
                    "Consider alternative approach for failed tool: {}",
                    tool
                ));
            }
        }

        // Analyze response content for questions or uncertainties
        let response_lower = response_content.to_lowercase();
        if response_lower.contains("error") || response_lower.contains("failed") {
            recommendations.push("Investigate and resolve the reported error".to_string());
        }

        if response_lower.contains("?") || response_lower.contains("unclear") {
            recommendations.push("Clarify the uncertain aspects mentioned".to_string());
        }

        // Check if recent tools suggest a workflow pattern
        if tool_invocations.len() > 1 {
            let tool_sequence: Vec<_> = tool_invocations.iter().map(|t| t.name.as_str()).collect();

            // Common patterns
            if tool_sequence.contains(&"file_read") && !tool_sequence.contains(&"file_write") {
                recommendations
                    .push("Consider modifying the read files if changes are needed".to_string());
            }

            if tool_sequence.contains(&"search")
                && tool_invocations.last().map_or(false, |t| t.success)
            {
                recommendations
                    .push("Examine the search results for relevant information".to_string());
            }
        }

        // Analyze key concepts for domain-specific recommendations
        if key_concepts.contains("Knowledge Graph") || key_concepts.contains("Graph Node") {
            recommendations
                .push("Consider visualizing or querying the graph structure".to_string());
        }

        if key_concepts.contains("Database") || key_concepts.contains("Query Processing") {
            recommendations.push("Verify data integrity and query performance".to_string());
        }

        // If we have both completed and pending goals, suggest prioritization
        if !completed_goals.is_empty() && !pending_goals.is_empty() {
            recommendations.push(format!(
                "Build on completed work ({} done) to address remaining goals ({} pending)",
                completed_goals.len(),
                pending_goals.len()
            ));
        }

        // Select the most relevant recommendation
        if recommendations.is_empty() {
            // No specific recommendation - check if conversation seems complete
            if completed_goals.len() > pending_goals.len() && recent_tool_failures.is_empty() {
                Some(
                    "Current objectives appear satisfied. Ready for new tasks or refinements."
                        .to_string(),
                )
            } else {
                None
            }
        } else {
            // Return the first (highest priority) recommendation
            Some(recommendations[0].clone())
        }
    }

    fn log_timing(&self, stage: &str, start: Instant) {
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        let agent_label = self.agent_name.as_deref().unwrap_or("unnamed");
        info!(
            target: "agent_timing",
            "stage={} duration_ms={:.2} agent={} session_id={}",
            stage,
            duration_ms,
            agent_label,
            self.session_id
        );
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

fn preview_json_value(value: &Value) -> String {
    match value {
        Value::String(text) => preview_text(text),
        Value::Null => "null".to_string(),
        _ => {
            let raw = value.to_string();
            if raw.len() > 80 {
                format!("{}...", &raw[..77])
            } else {
                raw
            }
        }
    }
}

fn payload_field(payload: &Value, key: &str) -> Value {
    payload.get(key).cloned().unwrap_or(Value::Null)
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
            show_reasoning: false,
            enable_audio_transcription: false,
            audio_response_mode: "immediate".to_string(),
            audio_scenario: None,
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

    fn create_fast_reasoning_agent(
        session_id: &str,
        fast_output: &str,
    ) -> (AgentCore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("fast.duckdb");
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
            fast_reasoning: true,
            fast_model_provider: Some("mock".to_string()),
            fast_model_name: Some("mock-fast".to_string()),
            fast_model_temperature: 0.3,
            fast_model_tasks: vec!["entity_extraction".to_string()],
            escalation_threshold: 0.5,
            show_reasoning: false,
            enable_audio_transcription: false,
            audio_response_mode: "immediate".to_string(),
            audio_scenario: None,
        };

        profile.validate().unwrap();

        let provider = Arc::new(MockProvider::new("This is a test response."));
        let fast_provider = Arc::new(MockProvider::new(fast_output.to_string()));
        let tool_registry = Arc::new(crate::tools::ToolRegistry::new());
        let policy_engine = Arc::new(PolicyEngine::new());

        (
            AgentCore::new(
                profile,
                provider,
                None,
                persistence,
                session_id.to_string(),
                Some(session_id.to_string()),
                tool_registry,
                policy_engine,
            )
            .with_fast_provider(fast_provider),
            dir,
        )
    }

    #[derive(Clone)]
    struct KeywordEmbeddingsService;

    #[async_trait]
    impl EmbeddingsService for KeywordEmbeddingsService {
        async fn create_embeddings(
            &self,
            _model: &str,
            inputs: Vec<String>,
        ) -> Result<Vec<Vec<f32>>> {
            Ok(inputs
                .into_iter()
                .map(|input| keyword_embedding(&input))
                .collect())
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
    async fn fast_model_short_circuits_when_confident() {
        let (mut agent, _dir) = create_fast_reasoning_agent(
            "fast-confident",
            "Answer: Entities detected.\nConfidence: 0.9",
        );

        let output = agent
            .run_step("Extract the entities mentioned in this string.")
            .await
            .unwrap();

        assert!(output
            .finish_reason
            .unwrap_or_default()
            .contains("fast_model"));
        assert!(output.response.contains("Entities detected"));
    }

    #[tokio::test]
    async fn fast_model_only_hints_when_low_confidence() {
        let (mut agent, _dir) =
            create_fast_reasoning_agent("fast-hint", "Answer: Unsure.\nConfidence: 0.2");

        let output = agent
            .run_step("Extract the entities mentioned in this string.")
            .await
            .unwrap();

        assert_eq!(output.finish_reason.as_deref(), Some("stop"));
        assert_eq!(output.response, "This is a test response.");
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
            show_reasoning: false,
            enable_audio_transcription: false,
            audio_response_mode: "immediate".to_string(),
            audio_scenario: None,
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
        assert!(!agent.is_tool_allowed("calculator"));

        // Test with denied list
        profile.allowed_tools = None;
        profile.denied_tools = Some(vec!["calculator".to_string()]);

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
        assert!(!agent.is_tool_allowed("calculator"));
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
            show_reasoning: false,
            enable_audio_transcription: false,
            audio_response_mode: "immediate".to_string(),
            audio_scenario: None,
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
        assert!(AgentCore::goal_requires_tool(
            "Tell me about the project in this directory"
        ));
    }

    #[test]
    fn test_infer_goal_tool_action_project_description() {
        let query = "Tell me about the project in this directory";
        let inferred = AgentCore::infer_goal_tool_action(query)
            .expect("Should infer a tool for project description");
        let (tool, args) = inferred;
        assert!(
            tool == "file_read" || tool == "search",
            "unexpected tool: {}",
            tool
        );
        if tool == "file_read" {
            // Should include a path and max_bytes
            assert!(args.get("path").is_some());
            assert!(args.get("max_bytes").is_some());
        } else {
            // search path: should include regex query for common manifests
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            assert!(query.contains("Cargo.toml") || query.contains("package.json"));
        }
    }
}
