//! Styling system for terminal text

mod color;
mod modifier;
mod style;
mod styled;

pub use color::Color;
pub use modifier::Modifier;
pub use style::Style;
pub use styled::{Span, Line, Text};
