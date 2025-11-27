//! Core widget traits

use crate::buffer::Buffer;
use crate::geometry::Rect;

/// Core widget trait for stateless rendering
///
/// Widgets are responsible for rendering themselves to a buffer within
/// a given area. They should not modify any state outside of the buffer.
pub trait Widget {
    /// Render the widget to a buffer within the given area
    ///
    /// The widget should clip its output to the provided area and not
    /// write outside of it.
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Stateful widget with associated state type
///
/// Use this for widgets that need to maintain state across renders,
/// such as input fields with cursor position, lists with selection state,
/// or scrollable views with scroll offset.
pub trait StatefulWidget {
    /// The state type for this widget
    type State;

    /// Render with mutable state access
    ///
    /// The state may be modified during rendering (e.g., to adjust
    /// scroll positions or update internal caches).
    fn render(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State);
}

/// Blanket implementation: any Widget can be used where StatefulWidget<State=()> is expected
impl<W: Widget> StatefulWidget for W {
    type State = ();

    fn render(&self, area: Rect, buf: &mut Buffer, _state: &mut Self::State) {
        Widget::render(self, area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Style;

    struct TestWidget {
        text: String,
    }

    impl Widget for TestWidget {
        fn render(&self, area: Rect, buf: &mut Buffer) {
            buf.set_string(area.x, area.y, &self.text, Style::default());
        }
    }

    #[test]
    fn test_widget_render() {
        let widget = TestWidget {
            text: "Hello".to_string(),
        };
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::new(area);

        Widget::render(&widget, area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().symbol, "H");
        assert_eq!(buf.get(4, 0).unwrap().symbol, "o");
    }

    struct CounterWidget;

    impl StatefulWidget for CounterWidget {
        type State = u32;

        fn render(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
            let text = format!("Count: {}", state);
            buf.set_string(area.x, area.y, &text, Style::default());
            *state += 1;
        }
    }

    #[test]
    fn test_stateful_widget_render() {
        let widget = CounterWidget;
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::new(area);
        let mut state = 0u32;

        widget.render(area, &mut buf, &mut state);
        assert_eq!(state, 1);

        widget.render(area, &mut buf, &mut state);
        assert_eq!(state, 2);
    }
}
