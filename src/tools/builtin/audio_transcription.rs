use crate::persistence::Persistence;
use crate::tools::{Tool, ToolResult};
use crate::types::MessageRole;
use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;

/// Mock transcription event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptionEvent {
    /// Regular speech transcription
    Speech {
        text: String,
        confidence: f32,
        speaker: Option<String>,
    },
    /// Background noise or non-speech audio
    Noise { description: String, intensity: f32 },
    /// Emotional or tonal context
    Tone { emotion: String, text: String },
    /// Partial/incomplete transcription
    Partial { text: String, is_final: bool },
    /// System events (start/stop listening)
    System { message: String },
}

/// Predefined mock scenarios for testing
#[derive(Debug, Clone)]
pub struct MockScenario {
    pub name: String,
    pub description: String,
    pub events: Vec<(TranscriptionEvent, Duration)>, // Event and delay before next
}

impl MockScenario {
    fn simple_conversation() -> Self {
        Self {
            name: "simple_conversation".to_string(),
            description: "A simple back-and-forth conversation".to_string(),
            events: vec![
                (
                    TranscriptionEvent::System {
                        message: "Audio transcription started".to_string(),
                    },
                    Duration::from_millis(500),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Hello, how are you today?".to_string(),
                        confidence: 0.95,
                        speaker: Some("User".to_string()),
                    },
                    Duration::from_millis(1000),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "I'm doing well, thank you for asking.".to_string(),
                        confidence: 0.92,
                        speaker: Some("Assistant".to_string()),
                    },
                    Duration::from_millis(800),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "What's the weather like outside?".to_string(),
                        confidence: 0.88,
                        speaker: Some("User".to_string()),
                    },
                    Duration::from_millis(1200),
                ),
            ],
        }
    }

    fn command_sequence() -> Self {
        Self {
            name: "command_sequence".to_string(),
            description: "A series of voice commands".to_string(),
            events: vec![
                (
                    TranscriptionEvent::System {
                        message: "Voice command mode activated".to_string(),
                    },
                    Duration::from_millis(300),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Create a new file called test.txt".to_string(),
                        confidence: 0.90,
                        speaker: None,
                    },
                    Duration::from_millis(1500),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Write hello world to the file".to_string(),
                        confidence: 0.87,
                        speaker: None,
                    },
                    Duration::from_millis(1000),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Save and close the file".to_string(),
                        confidence: 0.93,
                        speaker: None,
                    },
                    Duration::from_millis(800),
                ),
                (
                    TranscriptionEvent::System {
                        message: "Commands executed successfully".to_string(),
                    },
                    Duration::from_millis(200),
                ),
            ],
        }
    }

    fn noisy_environment() -> Self {
        Self {
            name: "noisy_environment".to_string(),
            description: "Transcription with background noise".to_string(),
            events: vec![
                (
                    TranscriptionEvent::Noise {
                        description: "Background chatter".to_string(),
                        intensity: 0.3,
                    },
                    Duration::from_millis(500),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Can you hear me clearly?".to_string(),
                        confidence: 0.75,
                        speaker: Some("User".to_string()),
                    },
                    Duration::from_millis(800),
                ),
                (
                    TranscriptionEvent::Noise {
                        description: "Door closing".to_string(),
                        intensity: 0.7,
                    },
                    Duration::from_millis(300),
                ),
                (
                    TranscriptionEvent::Partial {
                        text: "I need to...".to_string(),
                        is_final: false,
                    },
                    Duration::from_millis(500),
                ),
                (
                    TranscriptionEvent::Partial {
                        text: "I need to schedule a meeting".to_string(),
                        is_final: true,
                    },
                    Duration::from_millis(1000),
                ),
            ],
        }
    }

    fn emotional_context() -> Self {
        Self {
            name: "emotional_context".to_string(),
            description: "Transcription with emotional tone markers".to_string(),
            events: vec![
                (
                    TranscriptionEvent::Tone {
                        emotion: "excited".to_string(),
                        text: "That's amazing news!".to_string(),
                    },
                    Duration::from_millis(1000),
                ),
                (
                    TranscriptionEvent::Tone {
                        emotion: "concerned".to_string(),
                        text: "Are you sure that's the right approach?".to_string(),
                    },
                    Duration::from_millis(1200),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Let me think about it.".to_string(),
                        confidence: 0.85,
                        speaker: None,
                    },
                    Duration::from_millis(800),
                ),
                (
                    TranscriptionEvent::Tone {
                        emotion: "confident".to_string(),
                        text: "Yes, I'm certain this will work.".to_string(),
                    },
                    Duration::from_millis(1000),
                ),
            ],
        }
    }

    fn multi_speaker() -> Self {
        Self {
            name: "multi_speaker".to_string(),
            description: "Multiple speakers in a meeting".to_string(),
            events: vec![
                (
                    TranscriptionEvent::System {
                        message: "Meeting transcription started".to_string(),
                    },
                    Duration::from_millis(500),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Welcome everyone to today's standup.".to_string(),
                        confidence: 0.92,
                        speaker: Some("Alice".to_string()),
                    },
                    Duration::from_millis(1000),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "I finished the authentication module yesterday.".to_string(),
                        confidence: 0.88,
                        speaker: Some("Bob".to_string()),
                    },
                    Duration::from_millis(1200),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Great work Bob. Charlie, how about you?".to_string(),
                        confidence: 0.90,
                        speaker: Some("Alice".to_string()),
                    },
                    Duration::from_millis(800),
                ),
                (
                    TranscriptionEvent::Speech {
                        text: "Still working on the database migrations.".to_string(),
                        confidence: 0.85,
                        speaker: Some("Charlie".to_string()),
                    },
                    Duration::from_millis(1000),
                ),
            ],
        }
    }
}

/// Configuration for audio transcription session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// Mock scenario to use
    pub scenario: String,
    /// Duration to listen (in seconds), None for continuous
    pub duration: Option<u64>,
    /// Whether to loop the scenario
    pub loop_scenario: bool,
    /// Base delay multiplier (1.0 = normal speed)
    pub speed_multiplier: f32,
    /// Whether to persist to database
    pub persist: bool,
    /// Session ID for persistence
    pub session_id: Option<String>,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            scenario: "simple_conversation".to_string(),
            duration: Some(30),
            loop_scenario: false,
            speed_multiplier: 1.0,
            persist: true,
            session_id: None,
        }
    }
}

/// Mock audio transcription tool
pub struct AudioTranscriptionTool {
    scenarios: Vec<MockScenario>,
    active_sessions: Arc<Mutex<Vec<String>>>,
    persistence: Option<Arc<Persistence>>,
}

impl AudioTranscriptionTool {
    pub fn new() -> Self {
        Self {
            scenarios: vec![
                MockScenario::simple_conversation(),
                MockScenario::command_sequence(),
                MockScenario::noisy_environment(),
                MockScenario::emotional_context(),
                MockScenario::multi_speaker(),
            ],
            active_sessions: Arc::new(Mutex::new(Vec::new())),
            persistence: None,
        }
    }

    pub fn with_persistence(persistence: Arc<Persistence>) -> Self {
        let mut tool = Self::new();
        tool.persistence = Some(persistence);
        tool
    }

    /// Get a scenario by name
    fn get_scenario(&self, name: &str) -> Option<&MockScenario> {
        self.scenarios.iter().find(|s| s.name == name)
    }

    /// Create a stream of transcription events
    pub fn create_event_stream(
        &self,
        config: TranscriptionConfig,
    ) -> Pin<Box<dyn Stream<Item = TranscriptionEvent> + Send>> {
        let scenario = self
            .get_scenario(&config.scenario)
            .cloned()
            .unwrap_or_else(MockScenario::simple_conversation);

        let speed_multiplier = config.speed_multiplier;
        let loop_scenario = config.loop_scenario;
        let duration = config.duration;

        Box::pin(stream::unfold(
            (scenario, 0usize, time::Instant::now(), duration),
            move |(scenario, mut index, start_time, duration)| async move {
                // Check duration limit
                if let Some(max_duration) = duration {
                    if start_time.elapsed() >= Duration::from_secs(max_duration) {
                        return None;
                    }
                }

                // Get current event or loop/end
                if index >= scenario.events.len() {
                    if loop_scenario {
                        index = 0;
                    } else {
                        return None;
                    }
                }

                let (event, delay) = scenario.events[index].clone();

                // Apply speed multiplier to delay
                let adjusted_delay =
                    Duration::from_millis((delay.as_millis() as f32 * speed_multiplier) as u64);

                // Wait before emitting the event
                time::sleep(adjusted_delay).await;

                Some((event, (scenario, index + 1, start_time, duration)))
            },
        ))
    }

    /// Format transcription event as string
    fn format_event(&self, event: &TranscriptionEvent) -> String {
        match event {
            TranscriptionEvent::Speech {
                text,
                confidence,
                speaker,
            } => {
                if let Some(speaker) = speaker {
                    format!(
                        "[{}] {} (confidence: {:.1}%)",
                        speaker,
                        text,
                        confidence * 100.0
                    )
                } else {
                    format!("{} (confidence: {:.1}%)", text, confidence * 100.0)
                }
            }
            TranscriptionEvent::Noise {
                description,
                intensity,
            } => {
                format!("[NOISE: {} (intensity: {:.1})]", description, intensity)
            }
            TranscriptionEvent::Tone { emotion, text } => {
                format!("[TONE: {}] {}", emotion, text)
            }
            TranscriptionEvent::Partial { text, is_final } => {
                if *is_final {
                    format!("[FINAL] {}", text)
                } else {
                    format!("[PARTIAL] {}...", text)
                }
            }
            TranscriptionEvent::System { message } => {
                format!("[SYSTEM] {}", message)
            }
        }
    }

    /// Store transcription event in database
    async fn persist_event(&self, session_id: &str, event: &TranscriptionEvent) -> Result<()> {
        if let Some(persistence) = &self.persistence {
            let formatted = self.format_event(event);

            // Store as a user message
            persistence.insert_message(session_id, MessageRole::User, &formatted)?;

            // Optionally store metadata as graph nodes
            if let TranscriptionEvent::Speech {
                text: _,
                speaker: _,
                confidence: _,
            } = event
            {
                // Could create graph nodes for entities, speakers, etc.
                // This is where we'd integrate with the knowledge graph
                // For now, we'll just log the transcription
            }
        }
        Ok(())
    }
}

impl Default for AudioTranscriptionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for AudioTranscriptionTool {
    fn name(&self) -> &str {
        "audio_transcribe"
    }

    fn description(&self) -> &str {
        "Mock audio transcription tool that simulates live audio input and converts it to text. \
         Supports multiple scenarios including conversations, commands, noisy environments, \
         and multi-speaker sessions."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "scenario": {
                    "type": "string",
                    "description": "Mock scenario to use",
                    "enum": [
                        "simple_conversation",
                        "command_sequence",
                        "noisy_environment",
                        "emotional_context",
                        "multi_speaker"
                    ]
                },
                "duration": {
                    "type": "integer",
                    "description": "Duration to listen in seconds (default: 30)"
                },
                "mode": {
                    "type": "string",
                    "description": "Transcription mode: 'stream' for real-time or 'batch' for all at once",
                    "enum": ["stream", "batch"],
                    "default": "stream"
                },
                "speed_multiplier": {
                    "type": "number",
                    "description": "Speed multiplier for event dispatch (1.0 = normal, 0.5 = half speed, 2.0 = double speed)",
                    "default": 1.0
                },
                "persist": {
                    "type": "boolean",
                    "description": "Whether to persist transcriptions to database",
                    "default": true
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        // Parse configuration
        let scenario = args["scenario"]
            .as_str()
            .unwrap_or("simple_conversation")
            .to_string();

        let duration = args["duration"].as_u64().or(Some(30));

        let mode = args["mode"].as_str().unwrap_or("stream").to_string();

        let speed_multiplier = args["speed_multiplier"].as_f64().unwrap_or(1.0) as f32;

        let persist = args["persist"].as_bool().unwrap_or(true);

        // Generate session ID
        let session_id = format!("audio_{}", chrono::Utc::now().timestamp_millis());

        // Track active session
        {
            let mut sessions = self.active_sessions.lock().await;
            sessions.push(session_id.clone());
        }

        let config = TranscriptionConfig {
            scenario: scenario.clone(),
            duration,
            loop_scenario: false,
            speed_multiplier,
            persist,
            session_id: Some(session_id.clone()),
        };

        // Create event stream
        let mut event_stream = self.create_event_stream(config);
        let mut transcriptions = Vec::new();

        // Process events based on mode
        if mode == "batch" {
            // Collect all events at once
            while let Some(event) = event_stream.next().await {
                let formatted = self.format_event(&event);
                transcriptions.push(formatted.clone());

                if persist {
                    let _ = self.persist_event(&session_id, &event).await;
                }
            }

            // Remove from active sessions
            {
                let mut sessions = self.active_sessions.lock().await;
                sessions.retain(|s| s != &session_id);
            }

            let result = json!({
                "session_id": session_id,
                "scenario": scenario,
                "mode": "batch",
                "transcriptions": transcriptions,
                "count": transcriptions.len(),
                "duration": duration,
            });
            Ok(ToolResult::success(result.to_string()))
        } else {
            // Stream mode - return immediately with session info
            // In a real implementation, this would set up a background task

            // For mock, we'll process a few events to show it's working
            let mut sample_transcriptions = Vec::new();
            let mut count = 0;

            while let Some(event) = event_stream.next().await {
                let formatted = self.format_event(&event);
                sample_transcriptions.push(formatted.clone());

                if persist {
                    let _ = self.persist_event(&session_id, &event).await;
                }

                count += 1;
                if count >= 3 {
                    break; // Just show first 3 events as sample
                }
            }

            let result = json!({
                "session_id": session_id,
                "scenario": scenario,
                "mode": "stream",
                "status": "listening",
                "sample_transcriptions": sample_transcriptions,
                "message": format!(
                    "Audio transcription session {} started. Listening for {} seconds...",
                    session_id,
                    duration.unwrap_or(0)
                ),
            });
            Ok(ToolResult::success(result.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_metadata() {
        let tool = AudioTranscriptionTool::new();
        assert_eq!(tool.name(), "audio_transcribe");
        assert!(tool.description().contains("audio"));

        let params = tool.parameters();
        assert!(params["properties"]["scenario"].is_object());
        assert!(params["properties"]["duration"].is_object());
    }

    #[tokio::test]
    async fn test_scenario_loading() {
        let tool = AudioTranscriptionTool::new();

        assert!(tool.get_scenario("simple_conversation").is_some());
        assert!(tool.get_scenario("command_sequence").is_some());
        assert!(tool.get_scenario("noisy_environment").is_some());
        assert!(tool.get_scenario("emotional_context").is_some());
        assert!(tool.get_scenario("multi_speaker").is_some());
        assert!(tool.get_scenario("non_existent").is_none());
    }

    #[tokio::test]
    async fn test_event_stream_creation() {
        let tool = AudioTranscriptionTool::new();
        let config = TranscriptionConfig {
            scenario: "simple_conversation".to_string(),
            duration: Some(1), // 1 second for quick test
            loop_scenario: false,
            speed_multiplier: 10.0, // Speed up for testing
            persist: false,
            session_id: None,
        };

        let mut stream = tool.create_event_stream(config);
        let mut count = 0;

        while let Some(_event) = stream.next().await {
            count += 1;
            if count >= 3 {
                break; // Just test a few events
            }
        }

        assert!(count > 0);
    }

    #[tokio::test]
    async fn test_batch_execution() {
        let tool = AudioTranscriptionTool::new();
        let args = json!({
            "scenario": "simple_conversation",
            "duration": 1,
            "mode": "batch",
            "speed_multiplier": 100.0, // Very fast for testing
            "persist": false
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);

        let output: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(output["scenario"], "simple_conversation");
        assert_eq!(output["mode"], "batch");
        assert!(output["transcriptions"].is_array());
    }

    #[tokio::test]
    async fn test_stream_execution() {
        let tool = AudioTranscriptionTool::new();
        let args = json!({
            "scenario": "command_sequence",
            "duration": 5,
            "mode": "stream",
            "speed_multiplier": 10.0,
            "persist": false
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);

        let output: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(output["scenario"], "command_sequence");
        assert_eq!(output["mode"], "stream");
        assert_eq!(output["status"], "listening");
        assert!(output["sample_transcriptions"].is_array());
    }

    #[tokio::test]
    async fn test_event_formatting() {
        let tool = AudioTranscriptionTool::new();

        let speech_event = TranscriptionEvent::Speech {
            text: "Hello".to_string(),
            confidence: 0.95,
            speaker: Some("Alice".to_string()),
        };
        let formatted = tool.format_event(&speech_event);
        assert!(formatted.contains("Alice"));
        assert!(formatted.contains("Hello"));
        assert!(formatted.contains("95.0%"));

        let noise_event = TranscriptionEvent::Noise {
            description: "Door closing".to_string(),
            intensity: 0.7,
        };
        let formatted = tool.format_event(&noise_event);
        assert!(formatted.contains("NOISE"));
        assert!(formatted.contains("Door closing"));
    }

    #[tokio::test]
    async fn test_active_session_tracking() {
        let tool = AudioTranscriptionTool::new();

        let args = json!({
            "scenario": "simple_conversation",
            "duration": 1,
            "mode": "stream",
            "speed_multiplier": 100.0,
            "persist": false
        });

        let result = tool.execute(args).await.unwrap();
        let output: Value = serde_json::from_str(&result.output).unwrap();
        let session_id = output["session_id"].as_str().unwrap();

        {
            let sessions = tool.active_sessions.lock().await;
            assert!(sessions.iter().any(|s| s == session_id));
        }
    }
}
