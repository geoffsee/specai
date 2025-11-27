//! Demo application showcasing spec-ai-tui features
//!
//! Run with: cargo run -p spec-ai-tui --example demo

use spec_ai_tui::{
    app::{App, AppRunner},
    buffer::Buffer,
    event::{Event, KeyCode, KeyModifiers},
    geometry::Rect,
    layout::{Constraint, Layout},
    style::{Color, Line, Span, Style},
    widget::{
        builtin::{
            Block, Editor, EditorAction, EditorState,
            SlashCommand, SlashMenu, SlashMenuState,
            StatusBar, StatusSection,
        },
        Widget, StatefulWidget,
    },
};

/// Mock chat message
#[derive(Debug, Clone)]
struct ChatMessage {
    role: String,
    content: String,
    timestamp: String,
    /// Optional tool name (for tool role)
    tool_name: Option<String>,
}

impl ChatMessage {
    fn new(role: &str, content: &str, timestamp: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: timestamp.to_string(),
            tool_name: None,
        }
    }

    fn tool(name: &str, content: &str, timestamp: &str) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.to_string(),
            timestamp: timestamp.to_string(),
            tool_name: Some(name.to_string()),
        }
    }
}

/// Mock tool execution
#[derive(Debug, Clone)]
struct ToolExecution {
    name: String,
    status: ToolStatus,
    duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum ToolStatus {
    Running,
    Success,
    Failed,
}

/// Demo application state
struct DemoState {
    /// Editor field state
    editor: EditorState,
    /// Slash menu state
    slash_menu: SlashMenuState,
    /// Available slash commands
    slash_commands: Vec<SlashCommand>,
    /// Chat messages
    messages: Vec<ChatMessage>,
    /// Current streaming response (simulated)
    streaming: Option<String>,
    /// Scroll offset for chat
    scroll_offset: u16,
    /// Status message
    status: String,
    /// Active tools
    tools: Vec<ToolExecution>,
    /// Reasoning messages
    reasoning: Vec<String>,
    /// Should quit
    quit: bool,
    /// Current panel focus
    focus: Panel,
    /// Tick counter for animations
    tick: u64,
    /// Simulated streaming state
    stream_buffer: Vec<&'static str>,
    stream_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Panel {
    Input,
    Chat,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            editor: EditorState::new(),
            slash_menu: SlashMenuState::new(),
            slash_commands: vec![
                SlashCommand::new("help", "Show available commands"),
                SlashCommand::new("clear", "Clear the chat history"),
                SlashCommand::new("model", "Switch AI model"),
                SlashCommand::new("system", "Set system prompt"),
                SlashCommand::new("export", "Export conversation"),
                SlashCommand::new("settings", "Open settings"),
                SlashCommand::new("theme", "Change color theme"),
                SlashCommand::new("tools", "List available tools"),
            ],
            messages: vec![
                ChatMessage::new("system", "Welcome to spec-ai! I'm your AI assistant.", "10:00"),
                ChatMessage::new("user", "Can you find the main entry point of the TUI crate?", "10:01"),
                ChatMessage::new("assistant", "I'll search for the main entry point in the TUI crate.", "10:01"),
                ChatMessage::tool(
                    "code_search",
                    "Searching for: \"fn main\" in crates/spec-ai-tui/\n\
                     Found 2 results:\n\
                     → examples/demo.rs:704 - async fn main()\n\
                     → src/lib.rs - (no main, library crate)",
                    "10:01"
                ),
                ChatMessage::new(
                    "assistant",
                    "Found it! The main entry point is in `examples/demo.rs` at line 704. Let me read that file to show you the structure.",
                    "10:01"
                ),
                ChatMessage::tool(
                    "file_read",
                    "Reading: examples/demo.rs (lines 704-720)\n\
                     ```rust\n\
                     #[tokio::main]\n\
                     async fn main() -> std::io::Result<()> {\n\
                         let app = DemoApp;\n\
                         let mut runner = AppRunner::new(app)?;\n\
                         runner.run().await\n\
                     }\n\
                     ```",
                    "10:02"
                ),
                ChatMessage::new(
                    "assistant",
                    "The TUI uses an async main function with tokio. Here's how it works:\n\n\
                     • **DemoApp** - Implements the App trait with init(), handle_event(), render()\n\
                     • **AppRunner** - Manages the terminal, event loop, and rendering\n\
                     • **run()** - Starts the async event loop\n\n\
                     The App trait follows an Elm-like architecture for clean state management.",
                    "10:02"
                ),
                ChatMessage::new("user", "What about error handling?", "10:03"),
                ChatMessage::tool(
                    "grep",
                    "Searching for: \"Result<\" in src/\n\
                     Found 47 matches across 12 files\n\
                     Most common: io::Result<()> for terminal operations",
                    "10:03"
                ),
                ChatMessage::new(
                    "assistant",
                    "Error handling uses Rust's standard Result type:\n\n\
                     • Terminal operations return `io::Result<()>`\n\
                     • The `?` operator propagates errors up\n\
                     • RAII guards ensure cleanup even on panic\n\n\
                     Try typing a message or use / to see available commands!",
                    "10:03"
                ),
            ],
            streaming: None,
            scroll_offset: 0,
            status: "Ready".to_string(),
            tools: vec![
                ToolExecution { name: "code_search".to_string(), status: ToolStatus::Success, duration_ms: Some(45) },
                ToolExecution { name: "file_read".to_string(), status: ToolStatus::Success, duration_ms: Some(12) },
                ToolExecution { name: "grep".to_string(), status: ToolStatus::Success, duration_ms: Some(89) },
            ],
            reasoning: vec![
                "◆ Analyzing user query...".to_string(),
                "◆ Searching codebase for entry points".to_string(),
                "◆ Context: 3 tools used, 847 tokens".to_string(),
            ],
            quit: false,
            focus: Panel::Input,
            tick: 0,
            stream_buffer: vec![
                "I'm ",
                "simulating ",
                "a ",
                "streaming ",
                "response ",
                "to ",
                "demonstrate ",
                "the ",
                "real-time ",
                "token ",
                "rendering ",
                "capability ",
                "of ",
                "this ",
                "TUI. ",
                "\n\n",
                "Each ",
                "word ",
                "appears ",
                "progressively, ",
                "just ",
                "like ",
                "an ",
                "actual ",
                "LLM ",
                "response!",
            ],
            stream_index: 0,
        }
    }
}

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
        // Handle quit
        if event.is_quit() {
            state.quit = true;
            return false;
        }

        match event {
            Event::Key(key) => {
                // Global shortcuts (when not in slash menu)
                if !state.editor.show_slash_menu && key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('l') => {
                            // Clear chat
                            state.messages.clear();
                            state.status = "Chat cleared".to_string();
                            return true;
                        }
                        KeyCode::Char('r') => {
                            // Simulate tool running
                            state.tools.push(ToolExecution {
                                name: "code_search".to_string(),
                                status: ToolStatus::Running,
                                duration_ms: None,
                            });
                            state.reasoning[1] = "◆ Tools: code_search (running...)".to_string();
                            state.status = "Running tool...".to_string();
                            return true;
                        }
                        KeyCode::Char('s') => {
                            // Start streaming simulation
                            if state.streaming.is_none() {
                                state.streaming = Some(String::new());
                                state.stream_index = 0;
                                state.status = "Streaming...".to_string();
                            }
                            return true;
                        }
                        _ => {}
                    }
                }

                // Handle arrow keys for slash menu navigation
                if state.editor.show_slash_menu {
                    match key.code {
                        KeyCode::Down => {
                            let count = filtered_command_count(state);
                            state.slash_menu.next(count);
                            return true;
                        }
                        KeyCode::Up => {
                            let count = filtered_command_count(state);
                            state.slash_menu.prev(count);
                            return true;
                        }
                        _ => {}
                    }
                }

                // Panel-specific handling
                match state.focus {
                    Panel::Input => {
                        // Update slash menu visibility from editor state
                        let was_showing = state.editor.show_slash_menu;

                        // Let the editor handle the event
                        match state.editor.handle_event(&event) {
                            EditorAction::Handled => {
                                // Sync slash menu visibility
                                if state.editor.show_slash_menu && !was_showing {
                                    state.slash_menu.show();
                                } else if !state.editor.show_slash_menu && was_showing {
                                    state.slash_menu.hide();
                                }
                            }
                            EditorAction::Submit(text) => {
                                if !text.is_empty() {
                                    // Add user message
                                    let timestamp = format!("{}:{:02}",
                                        10 + state.messages.len() / 60,
                                        state.messages.len() % 60);
                                    state.messages.push(ChatMessage::new("user", &text, &timestamp));

                                    // Start streaming response
                                    state.streaming = Some(String::new());
                                    state.stream_index = 0;
                                    state.status = "Generating response...".to_string();
                                }
                            }
                            EditorAction::SlashCommand(cmd) => {
                                // Execute the slash command
                                execute_slash_command(&cmd, state);
                                state.editor.clear();
                                state.slash_menu.hide();
                            }
                            EditorAction::SlashMenuNext => {
                                let count = filtered_command_count(state);
                                state.slash_menu.next(count);
                            }
                            EditorAction::SlashMenuPrev => {
                                let count = filtered_command_count(state);
                                state.slash_menu.prev(count);
                            }
                            EditorAction::Escape => {
                                // Do nothing, slash menu already closed
                            }
                            EditorAction::Ignored => {
                                // Handle keys not handled by editor
                                match key.code {
                                    KeyCode::Up if !state.editor.show_slash_menu => {
                                        // Switch to chat panel
                                        state.focus = Panel::Chat;
                                        state.editor.focused = false;
                                    }
                                    KeyCode::PageUp => {
                                        state.scroll_offset = state.scroll_offset.saturating_add(5);
                                    }
                                    KeyCode::PageDown => {
                                        state.scroll_offset = state.scroll_offset.saturating_sub(5);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Panel::Chat => {
                        match key.code {
                            KeyCode::Tab => {
                                // Tab always switches to input
                                state.focus = Panel::Input;
                                state.editor.focused = true;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                // Scroll down, or switch to input if at bottom
                                if state.scroll_offset > 0 {
                                    state.scroll_offset = state.scroll_offset.saturating_sub(1);
                                } else {
                                    state.focus = Panel::Input;
                                    state.editor.focused = true;
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                state.scroll_offset = state.scroll_offset.saturating_add(1);
                            }
                            KeyCode::PageUp => {
                                state.scroll_offset = state.scroll_offset.saturating_add(10);
                            }
                            KeyCode::PageDown => {
                                state.scroll_offset = state.scroll_offset.saturating_sub(10);
                            }
                            KeyCode::Char('g') => {
                                state.scroll_offset = 100; // Top
                            }
                            KeyCode::Char('G') => {
                                state.scroll_offset = 0; // Bottom
                            }
                            _ => {}
                        }
                    }
                }
            }
            Event::Paste(_) => {
                // Handle paste when input is focused
                if state.focus == Panel::Input {
                    let was_showing = state.editor.show_slash_menu;
                    match state.editor.handle_event(&event) {
                        EditorAction::Handled => {
                            if state.editor.show_slash_menu && !was_showing {
                                state.slash_menu.show();
                            } else if !state.editor.show_slash_menu && was_showing {
                                state.slash_menu.hide();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Resize { .. } => {
                // Terminal will handle resize
            }
            Event::Tick => {
                // Handle tick for animations
            }
            _ => {}
        }

        true
    }

    fn on_tick(&mut self, state: &mut Self::State) {
        state.tick += 1;

        // Spinner frames for animation
        let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let spin_char = spinner[(state.tick / 2) as usize % spinner.len()];

        // Simulate streaming
        if let Some(ref mut streaming) = state.streaming {
            if state.stream_index < state.stream_buffer.len() {
                streaming.push_str(state.stream_buffer[state.stream_index]);
                state.stream_index += 1;

                // Update reasoning during streaming
                let tokens_out = streaming.split_whitespace().count();
                state.reasoning[0] = format!("{} Generating response...", spin_char);
                state.reasoning[1] = format!("  Tokens: ~{} output", tokens_out);
                state.reasoning[2] = format!("  Progress: {}/{} chunks",
                    state.stream_index, state.stream_buffer.len());
            } else {
                // Streaming complete
                let response = state.streaming.take().unwrap();
                let timestamp = format!("{}:{:02}",
                    10 + state.messages.len() / 60,
                    state.messages.len() % 60);
                state.messages.push(ChatMessage::new("assistant", &response, &timestamp));
                state.status = "Ready".to_string();
                state.stream_index = 0;

                // Update reasoning to show completion
                state.reasoning[0] = "✓ Response complete".to_string();
                state.reasoning[1] = format!("  Total messages: {}", state.messages.len());
                state.reasoning[2] = format!("  Tools used: {}", state.tools.len());
            }
        }

        // Simulate tool execution
        let mut any_running = false;
        for tool in &mut state.tools {
            if tool.status == ToolStatus::Running {
                any_running = true;
                if tool.duration_ms.is_none() {
                    tool.duration_ms = Some(0);
                }
                tool.duration_ms = Some(tool.duration_ms.unwrap() + 100);

                // Complete after ~3 seconds
                if tool.duration_ms.unwrap() >= 3000 {
                    tool.status = ToolStatus::Success;
                    state.reasoning[0] = format!("✓ {} completed", tool.name);
                    state.reasoning[1] = format!("  Duration: {}ms", tool.duration_ms.unwrap());
                    state.status = "Ready".to_string();
                } else {
                    state.reasoning[0] = format!("{} Running {}...", spin_char, tool.name);
                    state.reasoning[1] = format!("  Elapsed: {}ms", tool.duration_ms.unwrap());
                    state.reasoning[2] = "  Waiting for results".to_string();
                }
            }
        }

        // Idle state animation
        if state.streaming.is_none() && !any_running {
            if state.tick % 50 == 0 {
                // Occasionally update idle reasoning
                let idle_messages = [
                    ("◇ Ready", "  Waiting for input", "  Type / for commands"),
                    ("◇ Idle", "  Context loaded", &format!("  {} messages in history", state.messages.len())),
                    ("◇ Ready", &format!("  {} tools available", 8), "  Ctrl+R to simulate tool"),
                ];
                let idx = ((state.tick / 50) as usize) % idle_messages.len();
                state.reasoning[0] = idle_messages[idx].0.to_string();
                state.reasoning[1] = idle_messages[idx].1.to_string();
                state.reasoning[2] = idle_messages[idx].2.to_string();
            }
        }
    }

    fn render(&self, state: &Self::State, area: Rect, buf: &mut Buffer) {
        // Main layout: content + status
        let main_chunks = Layout::vertical()
            .constraints([
                Constraint::Fill(1),   // Main content
                Constraint::Fixed(3),  // Reasoning panel
                Constraint::Fixed(1),  // Status bar
            ])
            .split(area);

        // Content layout: chat + input
        let content_chunks = Layout::vertical()
            .constraints([
                Constraint::Fill(1),   // Chat
                Constraint::Fixed(8),  // Input area (taller for wrapped text)
            ])
            .split(main_chunks[0]);

        // Render chat area
        self.render_chat(state, content_chunks[0], buf);

        // Render input area (with slash menu)
        self.render_input(state, content_chunks[1], buf);

        // Render reasoning panel
        self.render_reasoning(state, main_chunks[1], buf);

        // Render status bar
        self.render_status(state, main_chunks[2], buf);
    }
}

fn filtered_command_count(state: &DemoState) -> usize {
    state.slash_commands
        .iter()
        .filter(|cmd| cmd.matches(&state.editor.slash_query))
        .count()
}

fn execute_slash_command(cmd: &str, state: &mut DemoState) {
    // Find the command that matches
    let filtered: Vec<_> = state.slash_commands
        .iter()
        .filter(|c| c.matches(&state.editor.slash_query))
        .collect();

    let selected_cmd = filtered.get(state.slash_menu.selected_index())
        .map(|c| c.name.as_str())
        .unwrap_or(cmd);

    let timestamp = format!("{}:{:02}",
        10 + state.messages.len() / 60,
        state.messages.len() % 60);

    match selected_cmd {
        "help" => {
            state.messages.push(ChatMessage::new(
                "system",
                "╭─ Keyboard Controls ─────────────────────────────╮\n\
                 │                                                 │\n\
                 │  INPUT PANEL                                    │\n\
                 │  Enter      Send message (triggers streaming)   │\n\
                 │  /          Open slash command menu             │\n\
                 │  Alt+b/f    Word navigation (back/forward)      │\n\
                 │  Ctrl+Z     Undo                                │\n\
                 │  ↑          Switch to Chat panel                │\n\
                 │                                                 │\n\
                 │  CHAT PANEL                                     │\n\
                 │  ↑/k        Scroll up                           │\n\
                 │  ↓/j        Scroll down                         │\n\
                 │  PageUp/Dn  Scroll by 10 lines                  │\n\
                 │  g/G        Jump to top/bottom                  │\n\
                 │  Tab        Return to Input panel               │\n\
                 │                                                 │\n\
                 │  GLOBAL                                         │\n\
                 │  Ctrl+C     Quit                                │\n\
                 │  Ctrl+L     Clear chat                          │\n\
                 │  Ctrl+R     Simulate tool execution             │\n\
                 │  Ctrl+S     Simulate streaming response         │\n\
                 │                                                 │\n\
                 │  SLASH COMMANDS                                 │\n\
                 │  /help /clear /model /system /export            │\n\
                 │  /settings /theme /tools                        │\n\
                 │                                                 │\n\
                 ╰─────────────────────────────────────────────────╯",
                &timestamp,
            ));
            state.status = "Help displayed".to_string();
        }
        "clear" => {
            state.messages.clear();
            state.status = "Chat cleared".to_string();
        }
        "model" => {
            state.status = "Model selection not implemented".to_string();
        }
        "theme" => {
            state.status = "Theme selection not implemented".to_string();
        }
        "tools" => {
            state.messages.push(ChatMessage::new(
                "system",
                "Available tools:\n\
                 • code_search - Search codebase\n\
                 • file_read - Read file contents\n\
                 • file_write - Write to files\n\
                 • bash - Execute shell commands",
                &timestamp,
            ));
            state.status = "Tools listed".to_string();
        }
        _ => {
            state.status = format!("Unknown command: /{}", selected_cmd);
        }
    }
}

/// Word-wrap a string to fit within max_width, preserving a prefix for continuation lines
fn wrap_text(text: &str, max_width: usize, prefix: &str) -> Vec<String> {
    if max_width == 0 {
        return vec![];
    }

    let mut result = Vec::new();
    let prefix_width = unicode_width::UnicodeWidthStr::width(prefix);
    let first_line_width = max_width;
    let continuation_width = max_width.saturating_sub(prefix_width);

    if continuation_width == 0 {
        return vec![text.chars().take(max_width).collect()];
    }

    let mut current_line = String::new();
    let mut current_width = 0usize;
    let mut is_first_line = true;

    for word in text.split_whitespace() {
        let word_width = unicode_width::UnicodeWidthStr::width(word);

        // Determine available width for this line
        let line_max = if is_first_line { first_line_width } else { continuation_width };

        if current_width == 0 {
            // First word on line
            if word_width <= line_max {
                current_line.push_str(word);
                current_width = word_width;
            } else {
                // Word is too long, need to break it
                let mut chars = word.chars().peekable();
                while chars.peek().is_some() {
                    let chunk: String = chars.by_ref().take(line_max).collect();
                    if !current_line.is_empty() {
                        if is_first_line {
                            result.push(current_line);
                        } else {
                            result.push(format!("{}{}", prefix, current_line));
                        }
                        is_first_line = false;
                    }
                    current_line = chunk;
                    current_width = unicode_width::UnicodeWidthStr::width(current_line.as_str());
                }
            }
        } else if current_width + 1 + word_width <= line_max {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Need to wrap
            if is_first_line {
                result.push(current_line);
            } else {
                result.push(format!("{}{}", prefix, current_line));
            }
            is_first_line = false;
            current_line = word.to_string();
            current_width = word_width;
        }
    }

    // Push remaining content
    if !current_line.is_empty() {
        if is_first_line {
            result.push(current_line);
        } else {
            result.push(format!("{}{}", prefix, current_line));
        }
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

impl DemoApp {
    fn render_chat(&self, state: &DemoState, area: Rect, buf: &mut Buffer) {
        // Draw border
        let border_style = if state.focus == Panel::Chat {
            Style::new().fg(Color::Cyan)
        } else {
            Style::new().fg(Color::DarkGrey)
        };

        let block = Block::bordered()
            .title("Chat")
            .border_style(border_style);
        Widget::render(&block, area, buf);

        let inner = block.inner(area);
        if inner.is_empty() {
            return;
        }

        // Reserve 1 char for scrollbar
        let content_width = inner.width.saturating_sub(1) as usize;

        // Build chat content with word wrapping
        let mut lines: Vec<Line> = Vec::new();

        for msg in &state.messages {
            // Role header
            let (role_style, role_display) = match msg.role.as_str() {
                "user" => (Style::new().fg(Color::Green).bold(), "user".to_string()),
                "assistant" => (Style::new().fg(Color::Cyan).bold(), "assistant".to_string()),
                "system" => (Style::new().fg(Color::Yellow).bold(), "system".to_string()),
                "tool" => {
                    let name = msg.tool_name.as_deref().unwrap_or("tool");
                    (Style::new().fg(Color::Magenta).bold(), format!("⚙ {}", name))
                }
                _ => (Style::new().fg(Color::White), msg.role.clone()),
            };

            lines.push(Line::from_spans([
                Span::styled(format!("[{}] ", msg.timestamp), Style::new().fg(Color::DarkGrey)),
                Span::styled(format!("{}:", role_display), role_style),
            ]));

            // Content lines with word wrapping
            // Tool messages get a special background indicator
            let is_tool = msg.role == "tool";
            let content_style = if is_tool {
                Style::new().fg(Color::DarkGrey)
            } else {
                Style::new()
            };
            let prefix = if is_tool { "  │ " } else { "  " };

            for content_line in msg.content.lines() {
                let prefixed = format!("{}{}", prefix, content_line);
                let wrapped = wrap_text(&prefixed, content_width, prefix);
                for wrapped_line in wrapped {
                    if is_tool {
                        lines.push(Line::styled(wrapped_line, content_style));
                    } else {
                        lines.push(Line::raw(wrapped_line));
                    }
                }
            }

            lines.push(Line::empty());
        }

        // Add streaming content
        if let Some(ref streaming) = state.streaming {
            lines.push(Line::from_spans([
                Span::styled("[--:--] ", Style::new().fg(Color::DarkGrey)),
                Span::styled("assistant:", Style::new().fg(Color::Cyan).bold()),
                Span::styled(" (streaming...)", Style::new().fg(Color::DarkGrey).italic()),
            ]));

            for content_line in streaming.lines() {
                let prefixed = format!("  {}", content_line);
                let wrapped = wrap_text(&prefixed, content_width, "  ");
                for wrapped_line in wrapped {
                    lines.push(Line::raw(wrapped_line));
                }
            }

            // Blinking cursor - always include line to prevent bobbing
            let cursor_char = if state.tick % 10 < 5 { "█" } else { " " };
            lines.push(Line::styled(format!("  {}", cursor_char), Style::new().fg(Color::Cyan)));
        }

        // Calculate visible lines
        let visible_height = inner.height as usize;
        let total_lines = lines.len();
        let scroll = state.scroll_offset as usize;

        // Scroll from bottom
        let start = if total_lines > visible_height + scroll {
            total_lines - visible_height - scroll
        } else {
            0
        };
        let end = (start + visible_height).min(total_lines);

        // Render visible lines
        for (i, line) in lines[start..end].iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.bottom() {
                break;
            }
            buf.set_line(inner.x, y, line);
        }

        // Scroll indicator
        if total_lines > visible_height {
            let scrollbar_height = inner.height.saturating_sub(2);
            let thumb_pos = if total_lines > 0 {
                ((start as u32 * scrollbar_height as u32) / total_lines as u32) as u16
            } else {
                0
            };

            for y in 0..scrollbar_height {
                let char = if y == thumb_pos { "█" } else { "░" };
                buf.set_string(
                    inner.right().saturating_sub(1),
                    inner.y + 1 + y,
                    char,
                    Style::new().fg(Color::DarkGrey),
                );
            }
        }
    }

    fn render_input(&self, state: &DemoState, area: Rect, buf: &mut Buffer) {
        let border_style = if state.focus == Panel::Input {
            Style::new().fg(Color::Cyan)
        } else {
            Style::new().fg(Color::DarkGrey)
        };

        let block = Block::bordered()
            .title("Input")
            .border_style(border_style);
        Widget::render(&block, area, buf);

        let inner = block.inner(area);
        if inner.is_empty() {
            return;
        }

        // Help text at top
        let help_text = if state.editor.show_slash_menu {
            "↑/↓: select | Enter: execute | Esc: cancel"
        } else {
            "Ctrl+C: quit | Ctrl+L: clear | / commands | Ctrl+Z: undo | Alt+b/f: word nav"
        };
        buf.set_string(
            inner.x,
            inner.y,
            help_text,
            Style::new().fg(Color::DarkGrey),
        );

        // Prompt on second line
        buf.set_string(inner.x, inner.y + 1, "▸ ", Style::new().fg(Color::Green));

        // Editor field - uses remaining height
        let editor_height = inner.height.saturating_sub(1); // Leave 1 line for help
        let editor_area = Rect::new(inner.x + 2, inner.y + 1, inner.width.saturating_sub(2), editor_height);
        let editor = Editor::new()
            .placeholder("Type a message... (/ for commands)")
            .style(Style::new().fg(Color::White));

        let mut editor_state = state.editor.clone();
        editor.render(editor_area, buf, &mut editor_state);

        // Render slash menu if active
        if state.editor.show_slash_menu {
            // Create the menu widget
            let filtered_commands: Vec<SlashCommand> = state.slash_commands
                .iter()
                .filter(|cmd| cmd.matches(&state.editor.slash_query))
                .cloned()
                .collect();

            if !filtered_commands.is_empty() {
                let menu = SlashMenu::new()
                    .commands(filtered_commands)
                    .query(&state.editor.slash_query);

                // Position the menu relative to the input area
                // Menu will render above the input
                let menu_area = Rect::new(
                    inner.x + 2,
                    area.y, // Use the full input area for positioning
                    inner.width.saturating_sub(2).min(50),
                    area.height,
                );

                let mut menu_state = state.slash_menu.clone();
                menu_state.visible = true;
                menu.render(menu_area, buf, &mut menu_state);
            }
        }
    }

    fn render_reasoning(&self, state: &DemoState, area: Rect, buf: &mut Buffer) {
        // Background
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if let Some(cell) = buf.get_mut(x, y) {
                    cell.bg = Color::Rgb(25, 25, 35);
                }
            }
        }

        // Left border accent
        for y in area.y..area.bottom() {
            if let Some(cell) = buf.get_mut(area.x, y) {
                cell.symbol = "│".to_string();
                cell.fg = Color::Rgb(60, 60, 80);
            }
        }

        // Render reasoning lines with dynamic styling
        for (i, line) in state.reasoning.iter().enumerate() {
            if area.y + i as u16 >= area.bottom() {
                break;
            }

            // Style based on content
            let style = if line.starts_with('✓') {
                Style::new().fg(Color::Green)
            } else if line.contains("Running") || line.contains("Generating") {
                Style::new().fg(Color::Yellow)
            } else if line.starts_with('◇') || line.starts_with('◆') {
                Style::new().fg(Color::Rgb(100, 100, 120))
            } else if line.starts_with("  ") {
                Style::new().fg(Color::Rgb(80, 80, 100))
            } else {
                // Spinner or other - use yellow for activity
                Style::new().fg(Color::Yellow)
            };

            buf.set_string(area.x + 2, area.y + i as u16, line, style);
        }
    }

    fn render_status(&self, state: &DemoState, area: Rect, buf: &mut Buffer) {
        let status_style = match state.status.as_str() {
            s if s.contains("Error") || s.contains("Unknown") => Style::new().fg(Color::Red),
            s if s.contains("Running") || s.contains("Streaming") || s.contains("Generating") => {
                Style::new().fg(Color::Yellow)
            }
            _ => Style::new().fg(Color::Green),
        };

        let bar = StatusBar::new()
            .style(Style::new().bg(Color::DarkGrey).fg(Color::White))
            .left([
                StatusSection::new("spec-ai").style(Style::new().fg(Color::Cyan).bold()),
                StatusSection::new("demo").style(Style::new().fg(Color::DarkGrey)),
            ])
            .center([
                StatusSection::new(&state.status).style(status_style),
            ])
            .right([
                StatusSection::new(format!("msgs: {}", state.messages.len())),
                StatusSection::new(format!("tick: {}", state.tick)),
            ]);

        Widget::render(&bar, area, buf);
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = DemoApp;
    let mut runner = AppRunner::new(app)?;

    runner.run().await
}
