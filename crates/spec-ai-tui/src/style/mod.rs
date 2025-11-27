//! Styling system for terminal text

mod color;
mod modifier;
mod style;
mod styled;
pub mod text_utils;

pub use color::Color;
pub use modifier::Modifier;
pub use style::Style;
pub use styled::{Line, Span, Text};
pub use text_utils::{truncate, wrap_text};
