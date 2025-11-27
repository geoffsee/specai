//! Slash command menu widget
//!
//! A popup menu that appears when the user types "/" to show available commands.

use crate::buffer::Buffer;
use crate::geometry::Rect;
use crate::style::{Color, Modifier, Style};
use crate::widget::StatefulWidget;

/// A slash command entry
#[derive(Debug, Clone)]
pub struct SlashCommand {
    /// Command name (without the leading /)
    pub name: String,
    /// Description of what the command does
    pub description: String,
    /// Optional keyboard shortcut hint
    pub shortcut: Option<String>,
}

impl SlashCommand {
    /// Create a new slash command
    pub fn new<S: Into<String>>(name: S, description: S) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            shortcut: None,
        }
    }

    /// Add a keyboard shortcut hint
    pub fn shortcut<S: Into<String>>(mut self, shortcut: S) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Check if this command matches the query
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(&query.to_lowercase())
    }
}

/// State for the slash menu
#[derive(Debug, Clone, Default)]
pub struct SlashMenuState {
    /// Currently selected index
    pub selected: usize,
    /// Scroll offset for long lists
    pub scroll: usize,
    /// Whether the menu is visible
    pub visible: bool,
}

impl SlashMenuState {
    /// Create a new menu state
    pub fn new() -> Self {
        Self::default()
    }

    /// Show the menu
    pub fn show(&mut self) {
        self.visible = true;
        self.selected = 0;
        self.scroll = 0;
    }

    /// Hide the menu
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Select the next item
    pub fn next(&mut self, item_count: usize) {
        if item_count > 0 {
            self.selected = (self.selected + 1) % item_count;
        }
    }

    /// Select the previous item
    pub fn prev(&mut self, item_count: usize) {
        if item_count > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(item_count - 1);
        }
    }

    /// Get the selected index
    pub fn selected_index(&self) -> usize {
        self.selected
    }
}

/// Slash menu widget
#[derive(Debug, Clone)]
pub struct SlashMenu {
    /// Available commands
    commands: Vec<SlashCommand>,
    /// Current filter query
    query: String,
    /// Style for the menu border
    border_style: Style,
    /// Style for unselected items
    item_style: Style,
    /// Style for the selected item
    selected_style: Style,
    /// Style for the command name
    name_style: Style,
    /// Style for the description
    desc_style: Style,
    /// Style for the shortcut
    shortcut_style: Style,
    /// Maximum visible items
    max_visible: usize,
}

impl Default for SlashMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl SlashMenu {
    /// Create a new slash menu
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            query: String::new(),
            border_style: Style::new().fg(Color::DarkGrey),
            item_style: Style::default(),
            selected_style: Style::new().bg(Color::Blue).fg(Color::White),
            name_style: Style::new().fg(Color::Cyan).modifier(Modifier::BOLD),
            desc_style: Style::new().fg(Color::Grey),
            shortcut_style: Style::new().fg(Color::DarkGrey),
            max_visible: 8,
        }
    }

    /// Set the available commands
    pub fn commands(mut self, commands: Vec<SlashCommand>) -> Self {
        self.commands = commands;
        self
    }

    /// Set the filter query
    pub fn query<S: Into<String>>(mut self, query: S) -> Self {
        self.query = query.into();
        self
    }

    /// Set border style
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Set item style
    pub fn item_style(mut self, style: Style) -> Self {
        self.item_style = style;
        self
    }

    /// Set selected item style
    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    /// Set maximum visible items
    pub fn max_visible(mut self, count: usize) -> Self {
        self.max_visible = count;
        self
    }

    /// Get filtered commands based on query
    pub fn filtered_commands(&self) -> Vec<&SlashCommand> {
        self.commands
            .iter()
            .filter(|cmd| cmd.matches(&self.query))
            .collect()
    }

    /// Get the selected command name (if any)
    pub fn selected_command(&self, state: &SlashMenuState) -> Option<&str> {
        let filtered = self.filtered_commands();
        filtered.get(state.selected).map(|cmd| cmd.name.as_str())
    }
}

impl StatefulWidget for SlashMenu {
    type State = SlashMenuState;

    fn render(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if !state.visible || area.is_empty() {
            return;
        }

        let filtered = self.filtered_commands();
        if filtered.is_empty() {
            return;
        }

        // Clamp selected index
        if state.selected >= filtered.len() {
            state.selected = filtered.len().saturating_sub(1);
        }

        // Calculate menu dimensions
        let item_count = filtered.len().min(self.max_visible);
        let menu_height = item_count as u16 + 2; // +2 for borders

        // Find maximum width needed
        let max_name_len = filtered.iter().map(|c| c.name.len()).max().unwrap_or(0);
        let max_desc_len = filtered
            .iter()
            .map(|c| c.description.len())
            .max()
            .unwrap_or(0);
        let menu_width = (max_name_len + max_desc_len + 6).min(area.width as usize - 2) as u16 + 2;

        // Position menu above the input area
        let menu_y = area.y.saturating_sub(menu_height);
        let menu_x = area.x;
        let menu_area = Rect::new(menu_x, menu_y, menu_width.min(area.width), menu_height);

        // Adjust scroll to keep selected item visible
        if state.selected < state.scroll {
            state.scroll = state.selected;
        } else if state.selected >= state.scroll + self.max_visible {
            state.scroll = state.selected - self.max_visible + 1;
        }

        // Draw border
        let chars = BorderChars::rounded();

        // Top border
        buf.set_string(
            menu_area.x,
            menu_area.y,
            &chars.top_left.to_string(),
            self.border_style,
        );
        for x in (menu_area.x + 1)..(menu_area.right() - 1) {
            buf.set_string(x, menu_area.y, &chars.top.to_string(), self.border_style);
        }
        buf.set_string(
            menu_area.right() - 1,
            menu_area.y,
            &chars.top_right.to_string(),
            self.border_style,
        );

        // Bottom border
        buf.set_string(
            menu_area.x,
            menu_area.bottom() - 1,
            &chars.bottom_left.to_string(),
            self.border_style,
        );
        for x in (menu_area.x + 1)..(menu_area.right() - 1) {
            buf.set_string(
                x,
                menu_area.bottom() - 1,
                &chars.bottom.to_string(),
                self.border_style,
            );
        }
        buf.set_string(
            menu_area.right() - 1,
            menu_area.bottom() - 1,
            &chars.bottom_right.to_string(),
            self.border_style,
        );

        // Side borders and items
        let inner_width = menu_area.width.saturating_sub(2) as usize;

        for (i, cmd) in filtered
            .iter()
            .enumerate()
            .skip(state.scroll)
            .take(self.max_visible)
        {
            let y = menu_area.y + 1 + (i - state.scroll) as u16;

            // Side borders
            buf.set_string(menu_area.x, y, &chars.left.to_string(), self.border_style);
            buf.set_string(
                menu_area.right() - 1,
                y,
                &chars.right.to_string(),
                self.border_style,
            );

            // Item background
            let is_selected = i == state.selected;
            let bg_style = if is_selected {
                self.selected_style
            } else {
                self.item_style
            };

            // Clear the row first
            for x in (menu_area.x + 1)..(menu_area.right() - 1) {
                if let Some(cell) = buf.get_mut(x, y) {
                    cell.symbol = " ".to_string();
                    cell.bg = bg_style.bg;
                    cell.fg = bg_style.fg;
                }
            }

            // Draw command name
            let name = format!("/{}", cmd.name);
            let name_style = if is_selected {
                self.selected_style
            } else {
                self.name_style
            };
            buf.set_string(menu_area.x + 1, y, &name, name_style);

            // Draw description
            let desc_x = menu_area.x + 1 + name.len() as u16 + 1;
            let remaining = inner_width.saturating_sub(name.len() + 1);
            if remaining > 0 {
                let desc: String = cmd.description.chars().take(remaining).collect();
                let desc_style = if is_selected {
                    self.selected_style.fg(Color::White)
                } else {
                    self.desc_style
                };
                buf.set_string(desc_x, y, &desc, desc_style);
            }
        }

        // Draw scroll indicators if needed
        if state.scroll > 0 {
            buf.set_string(
                menu_area.right() - 2,
                menu_area.y + 1,
                "▲",
                self.border_style,
            );
        }
        if state.scroll + self.max_visible < filtered.len() {
            buf.set_string(
                menu_area.right() - 2,
                menu_area.bottom() - 2,
                "▼",
                self.border_style,
            );
        }
    }
}

/// Border characters for the menu
struct BorderChars {
    top: char,
    bottom: char,
    left: char,
    right: char,
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
}

impl BorderChars {
    fn rounded() -> Self {
        Self {
            top: '─',
            bottom: '─',
            left: '│',
            right: '│',
            top_left: '╭',
            top_right: '╮',
            bottom_left: '╰',
            bottom_right: '╯',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_command_matches() {
        let cmd = SlashCommand::new("help", "Show help");
        assert!(cmd.matches(""));
        assert!(cmd.matches("hel"));
        assert!(cmd.matches("HELP"));
        assert!(!cmd.matches("xyz"));
    }

    #[test]
    fn test_slash_menu_state() {
        let mut state = SlashMenuState::new();
        state.show();
        assert!(state.visible);
        assert_eq!(state.selected, 0);

        state.next(5);
        assert_eq!(state.selected, 1);

        state.next(5);
        assert_eq!(state.selected, 2);

        state.prev(5);
        assert_eq!(state.selected, 1);

        // Wrap around
        state.selected = 4;
        state.next(5);
        assert_eq!(state.selected, 0);

        state.prev(5);
        assert_eq!(state.selected, 4);
    }

    #[test]
    fn test_filtered_commands() {
        let menu = SlashMenu::new()
            .commands(vec![
                SlashCommand::new("help", "Show help"),
                SlashCommand::new("clear", "Clear screen"),
                SlashCommand::new("history", "Show history"),
            ])
            .query("h");

        let filtered = menu.filtered_commands();
        assert_eq!(filtered.len(), 2); // help and history
    }
}
