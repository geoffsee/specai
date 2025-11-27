//! Built-in widgets

mod paragraph;
mod input;
mod block;
mod status;
mod editor;
mod slash_menu;

pub use paragraph::{Paragraph, Alignment, Wrap};
pub use input::{Input, InputState};
pub use block::{Block, BorderType};
pub use status::{StatusBar, StatusSection};
pub use editor::{Editor, EditorState, EditorAction, Selection};
pub use slash_menu::{SlashMenu, SlashMenuState, SlashCommand};
