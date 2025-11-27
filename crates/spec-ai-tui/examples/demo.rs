//! Demo application showcasing spec-ai-tui features
//!
//! Run with: cargo run -p spec-ai-tui --example demo

#[path = "demo/handlers.rs"]
mod handlers;
#[path = "demo/models.rs"]
mod models;
#[path = "demo/state.rs"]
mod state;
#[path = "demo/ui.rs"]
mod ui;

use handlers::{handle_event, on_tick};
use spec_ai_tui::{
    app::{App, AppRunner},
    buffer::Buffer,
    event::Event,
    geometry::Rect,
};
use state::DemoState;

/// Demo application
struct DemoApp;

impl App for DemoApp {
    type State = DemoState;

    fn init(&self) -> Self::State {
        let mut state = DemoState::default();
        state.editor.focused = true;
        state
    }

    fn handle_event(&mut self, event: Event, state: &mut Self::State) -> bool {
        handle_event(event, state)
    }

    fn on_tick(&mut self, state: &mut Self::State) {
        on_tick(state);
    }

    fn render(&self, state: &Self::State, area: Rect, buf: &mut Buffer) {
        ui::render(state, area, buf);
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = DemoApp;
    let mut runner = AppRunner::new(app)?;

    runner.run().await
}
