//! Application framework with App trait and runner

use crate::buffer::Buffer;
use crate::event::{Event, EventLoop};
use crate::geometry::Rect;
use crate::terminal::Terminal;
use std::io;
use std::time::Duration;

/// Application trait for implementing TUI apps
///
/// Implement this trait to create a TUI application with the Elm-inspired
/// architecture: init, update (via handle_event), and render.
pub trait App {
    /// Application state type
    type State;

    /// Initialize the application state
    fn init(&self) -> Self::State;

    /// Handle an event and update state
    ///
    /// Return `true` to continue running, `false` to quit.
    fn handle_event(&mut self, event: Event, state: &mut Self::State) -> bool;

    /// Render the UI to a buffer
    fn render(&self, state: &Self::State, area: Rect, buf: &mut Buffer);

    /// Called before each render (optional)
    fn on_tick(&mut self, _state: &mut Self::State) {}
}

/// Application runner that manages the terminal and event loop
pub struct AppRunner<A: App> {
    app: A,
    terminal: Terminal,
    event_loop: EventLoop,
    tick_rate: Duration,
}

impl<A: App> AppRunner<A> {
    /// Create a new app runner
    pub fn new(app: A) -> io::Result<Self> {
        let terminal = Terminal::new()?;
        let tick_rate = Duration::from_millis(100);
        let event_loop = EventLoop::new(tick_rate);

        Ok(Self {
            app,
            terminal,
            event_loop,
            tick_rate,
        })
    }

    /// Set the tick rate
    pub fn tick_rate(mut self, rate: Duration) -> Self {
        self.tick_rate = rate;
        self.event_loop = EventLoop::new(rate);
        self
    }

    /// Get a sender for custom events
    pub fn event_sender(&self) -> tokio::sync::mpsc::UnboundedSender<Event> {
        self.event_loop.sender()
    }

    /// Run the application
    pub async fn run(&mut self) -> io::Result<()> {
        // Enter raw mode
        let _raw_guard = self.terminal.enter_raw_mode()?;

        // Initialize state
        let mut state = self.app.init();

        // Initial render
        self.render(&state)?;

        // Main event loop
        loop {
            if let Some(event) = self.event_loop.next().await {
                // Handle resize
                if let Event::Resize { .. } = &event {
                    self.terminal.refresh_size()?;
                    self.terminal.invalidate();
                }

                // Handle tick
                if matches!(event, Event::Tick) {
                    self.app.on_tick(&mut state);
                }

                // Let app handle the event
                if !self.app.handle_event(event, &mut state) {
                    break;
                }

                // Render after each event
                self.render(&state)?;
            } else {
                // Event stream ended
                break;
            }
        }

        Ok(())
    }

    /// Render the current state
    fn render(&mut self, state: &A::State) -> io::Result<()> {
        let area = self.terminal.full_rect();
        let mut buf = Buffer::new(area);

        self.app.render(state, area, &mut buf);
        self.terminal.draw(&buf)
    }
}

/// Simple application builder for quick prototyping
pub struct SimpleApp<S, F, R>
where
    F: FnMut(Event, &mut S) -> bool,
    R: Fn(&S, Rect, &mut Buffer),
{
    init_fn: Box<dyn Fn() -> S>,
    handle_fn: F,
    render_fn: R,
}

impl<S, F, R> SimpleApp<S, F, R>
where
    F: FnMut(Event, &mut S) -> bool,
    R: Fn(&S, Rect, &mut Buffer),
{
    /// Create a simple app from closures
    pub fn new<I>(init: I, handle: F, render: R) -> Self
    where
        I: Fn() -> S + 'static,
    {
        Self {
            init_fn: Box::new(init),
            handle_fn: handle,
            render_fn: render,
        }
    }
}

impl<S, F, R> App for SimpleApp<S, F, R>
where
    F: FnMut(Event, &mut S) -> bool,
    R: Fn(&S, Rect, &mut Buffer),
{
    type State = S;

    fn init(&self) -> Self::State {
        (self.init_fn)()
    }

    fn handle_event(&mut self, event: Event, state: &mut Self::State) -> bool {
        (self.handle_fn)(event, state)
    }

    fn render(&self, state: &Self::State, area: Rect, buf: &mut Buffer) {
        (self.render_fn)(state, area, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Style;

    struct TestApp;

    impl App for TestApp {
        type State = u32;

        fn init(&self) -> Self::State {
            0
        }

        fn handle_event(&mut self, event: Event, state: &mut Self::State) -> bool {
            if event.is_quit() {
                return false;
            }
            *state += 1;
            true
        }

        fn render(&self, state: &Self::State, area: Rect, buf: &mut Buffer) {
            let text = format!("Count: {}", state);
            buf.set_string(area.x, area.y, &text, Style::default());
        }
    }

    #[test]
    fn test_app_init() {
        let app = TestApp;
        let state = app.init();
        assert_eq!(state, 0);
    }

    #[test]
    fn test_app_render() {
        let app = TestApp;
        let state = 42u32;
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::new(area);

        app.render(&state, area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().symbol, "C");
        assert_eq!(buf.get(7, 0).unwrap().symbol, "4");
    }
}
