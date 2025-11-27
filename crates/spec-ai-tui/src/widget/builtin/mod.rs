//! Built-in widgets

mod block;
mod editor;
mod input;
mod overlay;
mod paragraph;
mod slash_menu;
mod status;

pub use block::{Block, BorderType};
pub use editor::{Editor, EditorAction, EditorState, Selection};
pub use input::{Input, InputState};
pub use overlay::Overlay;
pub use paragraph::{Alignment, Paragraph, Wrap};
pub use slash_menu::{SlashCommand, SlashMenu, SlashMenuState};
pub use status::{StatusBar, StatusSection};
