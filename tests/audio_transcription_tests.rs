use serde_json::json;
use spec_ai::agent::AgentBuilder;
use spec_ai::config::{AgentProfile, AppConfig, AudioConfig};
use spec_ai::persistence::Persistence;
use spec_ai::tools::builtin::AudioTranscriptionTool;
use spec_ai::tools::{Tool, ToolRegistry};
use spec_ai::types::MessageRole;
use std::sync::Arc;
use tempfile::tempdir;

/// Create a test persistence instance with a temporary database
fn create_test_persistence() -> (Arc<Persistence>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Arc::new(Persistence::new(&db_path).unwrap());
    (persistence, dir)
}

/// Create a test persistence instance without Arc for registry
fn create_test_persistence_for_registry() -> (Persistence, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.duckdb");
    let persistence = Persistence::new(&db_path).unwrap();
    (persistence, dir)
}

#[tokio::test]
async fn test_audio_tool_registration() {
    let (persistence, _dir) = create_test_persistence();

    // Create tool registry with persistence
    let registry = ToolRegistry::with_builtin_tools(Some(persistence));

    // Check that audio_transcribe tool is registered
    assert!(registry.has("audio_transcribe"));

    let tool = registry.get("audio_transcribe").unwrap();
    assert_eq!(tool.name(), "audio_transcribe");
    assert!(tool.description().contains("audio"));
}

#[tokio::test]
async fn test_audio_tool_batch_mode() {
    let (persistence, _dir) = create_test_persistence();
    let tool = AudioTranscriptionTool::with_persistence(persistence.clone());

    let args = json!({
        "scenario": "simple_conversation",
        "duration": 1,
        "mode": "batch",
        "speed_multiplier": 100.0,
        "persist": true
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.success);

    let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
    assert_eq!(output["scenario"], "simple_conversation");
    assert_eq!(output["mode"], "batch");
    assert!(output["transcriptions"].is_array());

    // Check that transcriptions are not empty
    let transcriptions = output["transcriptions"].as_array().unwrap();
    assert!(!transcriptions.is_empty());
}

#[tokio::test]
async fn test_audio_tool_stream_mode() {
    let (persistence, _dir) = create_test_persistence();
    let tool = AudioTranscriptionTool::with_persistence(persistence);

    let args = json!({
        "scenario": "command_sequence",
        "duration": 2,
        "mode": "stream",
        "speed_multiplier": 50.0,
        "persist": false
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.success);

    let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
    assert_eq!(output["scenario"], "command_sequence");
    assert_eq!(output["mode"], "stream");
    assert_eq!(output["status"], "listening");
    assert!(output["sample_transcriptions"].is_array());
}

#[tokio::test]
async fn test_audio_persistence() {
    let (persistence, _dir) = create_test_persistence();
    let tool = AudioTranscriptionTool::with_persistence(persistence.clone());

    let _session_id = "test-session-audio";

    let args = json!({
        "scenario": "simple_conversation",
        "duration": 1,
        "mode": "batch",
        "speed_multiplier": 100.0,
        "persist": true
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.success);

    let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
    let tool_session_id = output["session_id"].as_str().unwrap();

    // Check that messages were persisted
    // Note: The tool creates its own session_id, so we use that to query
    let messages = persistence.list_messages(tool_session_id, 100).unwrap();
    assert!(!messages.is_empty(), "Should have persisted messages");

    // Verify all messages are from User role (as transcriptions)
    for msg in &messages {
        assert_eq!(msg.role, MessageRole::User);
        assert!(!msg.content.is_empty());
    }
}

#[tokio::test]
async fn test_different_scenarios() {
    let tool = AudioTranscriptionTool::new();

    let scenarios = vec![
        "simple_conversation",
        "command_sequence",
        "noisy_environment",
        "emotional_context",
        "multi_speaker",
    ];

    for scenario in scenarios {
        let args = json!({
            "scenario": scenario,
            "duration": 1,
            "mode": "batch",
            "speed_multiplier": 1000.0,
            "persist": false
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success, "Failed for scenario: {}", scenario);

        let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(output["scenario"], scenario);

        let transcriptions = output["transcriptions"].as_array().unwrap();
        assert!(
            !transcriptions.is_empty(),
            "No transcriptions for scenario: {}",
            scenario
        );
    }
}

#[tokio::test]
async fn test_audio_config_loading() {
    let config = AppConfig {
        audio: AudioConfig {
            enabled: true,
            mock_scenario: "emotional_context".to_string(),
            event_delay_ms: 100,
            auto_respond: true,
            default_duration: 60,
        },
        ..Default::default()
    };

    assert_eq!(config.audio.enabled, true);
    assert_eq!(config.audio.mock_scenario, "emotional_context");
    assert_eq!(config.audio.event_delay_ms, 100);
    assert_eq!(config.audio.auto_respond, true);
    assert_eq!(config.audio.default_duration, 60);
}

#[tokio::test]
async fn test_agent_profile_audio_config() {
    let profile = AgentProfile {
        enable_audio_transcription: true,
        audio_response_mode: "batch".to_string(),
        audio_scenario: Some("multi_speaker".to_string()),
        ..Default::default()
    };

    assert_eq!(profile.enable_audio_transcription, true);
    assert_eq!(profile.audio_response_mode, "batch");
    assert_eq!(profile.audio_scenario, Some("multi_speaker".to_string()));

    // Test default values
    let default_profile = AgentProfile::default();
    assert_eq!(default_profile.enable_audio_transcription, false);
    assert_eq!(default_profile.audio_response_mode, "immediate");
    assert_eq!(default_profile.audio_scenario, None);
}

#[tokio::test]
async fn test_audio_with_agent_integration() {
    let (persistence, _dir) = create_test_persistence_for_registry();

    // Create config with audio enabled
    let mut config = AppConfig::default();
    config.audio.enabled = true;

    // Create agent profile with audio transcription enabled
    let mut profile = AgentProfile::default();
    profile.enable_audio_transcription = true;
    profile.allowed_tools = Some(vec!["audio_transcribe".to_string()]);

    // Add profile to config
    config.agents.insert("audio_agent".to_string(), profile);
    config.default_agent = Some("audio_agent".to_string());

    // Create agent with audio-enabled profile
    let registry = spec_ai::config::AgentRegistry::new(config.agents.clone(), persistence);
    registry.init().unwrap();
    registry.set_active("audio_agent").unwrap();

    // Create agent using the builder
    let agent =
        AgentBuilder::new_with_registry(&registry, &config, Some("test-audio-session".to_string()))
            .unwrap();

    // Verify the agent has access to audio_transcribe tool
    let tools = agent.tool_registry();
    assert!(tools.has("audio_transcribe"));
}

#[tokio::test]
async fn test_transcription_event_formatting() {
    let tool = AudioTranscriptionTool::new();

    // Test that different event types are formatted correctly
    let args = json!({
        "scenario": "noisy_environment",
        "duration": 2,
        "mode": "batch",
        "speed_multiplier": 100.0,
        "persist": false
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.success);

    let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
    let transcriptions = output["transcriptions"].as_array().unwrap();

    // Check for different event type markers
    let transcription_text: Vec<String> = transcriptions
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    let full_text = transcription_text.join(" ");

    // Noisy environment should have noise markers
    assert!(
        full_text.contains("[NOISE:")
            || full_text.contains("[PARTIAL]")
            || full_text.contains("[FINAL]"),
        "Should contain noise or partial transcription markers"
    );
}

#[tokio::test]
async fn test_speed_multiplier() {
    let tool = AudioTranscriptionTool::new();

    // Test with very fast speed
    let start = std::time::Instant::now();

    let args = json!({
        "scenario": "simple_conversation",
        "duration": 2, // Would normally take 2 seconds
        "mode": "batch",
        "speed_multiplier": 10000.0, // Make it 10000x faster
        "persist": false
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.success);

    let elapsed = start.elapsed();

    // With 10000x speed multiplier, 2 seconds should complete in well under 1 second
    assert!(
        elapsed.as_secs() < 1,
        "Speed multiplier didn't work as expected, took {:?}",
        elapsed
    );
}
