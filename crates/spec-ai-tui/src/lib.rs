//! spec-ai-tui: A terminal user interface framework built from scratch on crossterm
//!
//! This crate provides a complete TUI framework with:
//! - Geometry primitives (`Rect`, `Point`, `Size`)
//! - Cell-based buffer system with diff rendering
//! - Terminal abstraction over crossterm
//! - Constraint-based layout engine
//! - Widget system with stateful and interactive traits
//! - Async event loop integrated with tokio
//! - Application framework with Elm-inspired architecture

pub mod app;
pub mod buffer;
pub mod event;
pub mod geometry;
pub mod layout;
pub mod style;
pub mod terminal;
pub mod widget;

// Re-export commonly used types
pub use app::App;
pub use buffer::{Buffer, Cell};
pub use event::Event;
pub use geometry::{Point, Rect, Size};
pub use layout::{Constraint, Direction, Layout};
pub use style::{truncate, wrap_text, Color, Line, Modifier, Span, Style, Text};
pub use terminal::Terminal;
pub use widget::Widget;
