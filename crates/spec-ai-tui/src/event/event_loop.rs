//! Async event loop integrated with tokio

use super::Event;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

/// Async event loop that bridges crossterm events with tokio
pub struct EventLoop {
    /// Tick rate for periodic updates
    tick_rate: Duration,
    /// Receiver for custom events
    custom_rx: mpsc::UnboundedReceiver<Event>,
    /// Sender for custom events (cloneable)
    custom_tx: mpsc::UnboundedSender<Event>,
}

impl EventLoop {
    /// Create a new event loop with the given tick rate
    pub fn new(tick_rate: Duration) -> Self {
        let (custom_tx, custom_rx) = mpsc::unbounded_channel();
        Self {
            tick_rate,
            custom_rx,
            custom_tx,
        }
    }

    /// Create with default tick rate (100ms)
    pub fn default_rate() -> Self {
        Self::new(Duration::from_millis(100))
    }

    /// Get a sender for custom events
    ///
    /// Use this to send application-specific events into the loop
    /// from other async tasks.
    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.custom_tx.clone()
    }

    /// Get the next event
    ///
    /// This will return:
    /// - Keyboard/mouse/resize events from crossterm
    /// - Custom events sent via the sender
    /// - Tick events at the configured rate
    ///
    /// Returns None if the event stream ends.
    pub async fn next(&mut self) -> Option<Event> {
        let tick_delay = tokio::time::sleep(self.tick_rate);
        tokio::pin!(tick_delay);

        let mut event_stream = EventStream::new();

        tokio::select! {
            // Crossterm terminal events
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(event)) => Some(event.into()),
                    Some(Err(_)) => None,
                    None => None,
                }
            }
            // Custom events from other tasks
            Some(event) = self.custom_rx.recv() => {
                Some(event)
            }
            // Periodic tick
            _ = &mut tick_delay => {
                Some(Event::Tick)
            }
        }
    }

    /// Run the event loop with a handler function
    ///
    /// The handler is called for each event. Return `false` to stop the loop.
    pub async fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(Event) -> bool,
    {
        loop {
            if let Some(event) = self.next().await {
                if !handler(event) {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

/// Builder for EventLoop
pub struct EventLoopBuilder {
    tick_rate: Duration,
}

impl EventLoopBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            tick_rate: Duration::from_millis(100),
        }
    }

    /// Set the tick rate
    pub fn tick_rate(mut self, rate: Duration) -> Self {
        self.tick_rate = rate;
        self
    }

    /// Build the event loop
    pub fn build(self) -> EventLoop {
        EventLoop::new(self.tick_rate)
    }
}

impl Default for EventLoopBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires terminal"]
    async fn test_custom_event() {
        let mut event_loop = EventLoop::new(Duration::from_secs(10)); // Long tick so it doesn't interfere
        let sender = event_loop.sender();

        // Send a custom event
        sender.send(Event::Tick).unwrap();

        // We should get it back
        let event = tokio::time::timeout(Duration::from_millis(100), event_loop.next())
            .await
            .unwrap();

        assert!(matches!(event, Some(Event::Tick)));
    }
}
