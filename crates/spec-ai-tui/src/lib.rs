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

pub mod geometry;
pub mod style;
pub mod buffer;
pub mod terminal;
pub mod layout;
pub mod widget;
pub mod event;
pub mod app;

// Re-export commonly used types
pub use geometry::{Point, Rect, Size};
pub use style::{Color, Modifier, Style, Span, Line, Text};
pub use buffer::{Cell, Buffer};
pub use terminal::Terminal;
pub use layout::{Constraint, Direction, Layout};
pub use widget::Widget;
pub use event::Event;
pub use app::App;
