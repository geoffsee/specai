pub mod agent;
pub mod agent_config;
pub mod cache;
pub mod registry;

// Re-export common types for convenience
pub use agent::AgentProfile;
pub use agent_config::{
    AppConfig, AudioConfig, DatabaseConfig, LoggingConfig, ModelConfig, UiConfig,
};
pub use registry::AgentRegistry;
