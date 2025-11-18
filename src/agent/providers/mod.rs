pub mod mock;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "mlx")]
pub mod mlx;

#[cfg(feature = "lmstudio")]
pub mod lmstudio;

pub use mock::MockProvider;

#[cfg(feature = "openai")]
pub use openai::OpenAIProvider;

#[cfg(feature = "mlx")]
pub use mlx::MLXProvider;

#[cfg(feature = "lmstudio")]
pub use lmstudio::LMStudioProvider;
