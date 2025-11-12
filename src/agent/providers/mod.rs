pub mod mock;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "mlx")]
pub mod mlx;

pub use mock::MockProvider;

#[cfg(feature = "openai")]
pub use openai::OpenAIProvider;

#[cfg(feature = "mlx")]
pub use mlx::MLXProvider;
