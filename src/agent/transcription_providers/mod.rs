//! Transcription Provider Implementations

pub mod mock;

#[cfg(feature = "vttrs")]
pub mod vttrs;

pub use mock::MockTranscriptionProvider;

#[cfg(feature = "vttrs")]
pub use vttrs::VttRsProvider;