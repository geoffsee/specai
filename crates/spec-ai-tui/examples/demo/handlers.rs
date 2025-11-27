//! Event handling and tick logic for the demo app.

use crate::models::{ChatMessage, ProcessStatus, ToolExecution, ToolStatus};
use crate::state::{DemoState, Panel};
use spec_ai_tui::{
    event::{Event, KeyCode, KeyModifiers},
    style::truncate,
    widget::builtin::{EditorAction, Selection},
};

pub fn handle_event(event: Event, state: &mut DemoState) -> bool {
    // Handle quit (Ctrl+C)
    if event.is_quit() {
        if state.pending_quit {
            // Second Ctrl+C - actually quit
            state.quit = true;
            return false;
        } else {
            // First Ctrl+C - show warning
            state.pending_quit = true;
            state.status = "Press Ctrl+C again to exit".to_string();
            return true;
        }
    }

    // Reset pending_quit on any key input (not tick events)
    if state.pending_quit {
        if let Event::Key(_) = event {
            state.pending_quit = false;
            state.status = "Ready".to_string();
        }
    }

    match event {
        Event::Key(key) => {
            // Handle log viewing overlay (highest priority - sub-overlay of process panel)
            if let Some(proc_idx) = state.viewing_logs {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        state.viewing_logs = None;
                        return true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.log_scroll = state.log_scroll.saturating_add(1);
                        return true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.log_scroll = state.log_scroll.saturating_sub(1);
                        return true;
                    }
                    KeyCode::PageUp => {
                        state.log_scroll = state.log_scroll.saturating_add(10);
                        return true;
                    }
                    KeyCode::PageDown => {
                        state.log_scroll = state.log_scroll.saturating_sub(10);
                        return true;
                    }
                    KeyCode::Char('g') => {
                        // Jump to top (oldest logs)
                        if let Some(proc) = state.processes.get(proc_idx) {
                            state.log_scroll = proc.output_lines.len().saturating_sub(1);
                        }
                        return true;
                    }
                    KeyCode::Char('G') => {
                        // Jump to bottom (newest logs)
                        state.log_scroll = 0;
                        return true;
                    }
                    _ => return true, // Consume all keys when log view is open
                }
            }

            // Handle overlay panels first (Escape to close)
            if state.show_process_panel || state.show_history {
                match key.code {
                    KeyCode::Esc => {
                        state.show_process_panel = false;
                        state.show_history = false;
                        return true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.show_process_panel {
                            state.selected_process = state.selected_process.saturating_sub(1);
                        } else if state.show_history {
                            state.selected_session = state.selected_session.saturating_sub(1);
                        }
                        return true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.show_process_panel {
                            let max = state.processes.len().saturating_sub(1);
                            state.selected_process = (state.selected_process + 1).min(max);
                        } else if state.show_history {
                            let max = state.sessions.len().saturating_sub(1);
                            state.selected_session = (state.selected_session + 1).min(max);
                        }
                        return true;
                    }
                    KeyCode::Enter => {
                        if state.show_history && state.selected_session < state.sessions.len() {
                            // Switch to selected session
                            if state.selected_session != state.current_session {
                                // Save current messages to current session
                                if state.current_session == 0 {
                                    state.sessions[0].messages = state.messages.clone();
                                    state.sessions[0].message_count = state.messages.len();
                                }
                                // Load selected session
                                state.current_session = state.selected_session;
                                if state.selected_session == 0 {
                                    // Restore saved messages or keep current
                                    state.messages = state.sessions[0].messages.clone();
                                } else {
                                    state.messages =
                                        state.sessions[state.selected_session].messages.clone();
                                }
                                state.status = format!(
                                    "Switched to: {}",
                                    state.sessions[state.selected_session].title
                                );
                                state.scroll_offset = 0;
                            }
                            state.show_history = false;
                        } else if state.show_process_panel
                            && state.selected_process < state.processes.len()
                        {
                            // Open log view for selected process
                            state.viewing_logs = Some(state.selected_process);
                            state.log_scroll = 0;
                            if let Some(proc) = state.processes.get(state.selected_process) {
                                state.status = format!(
                                    "Logs: PID {} (↑↓ scroll, g/G top/bottom, Esc close)",
                                    proc.pid
                                );
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('s') if state.show_process_panel => {
                        // Toggle stop/continue for selected process
                        if state.selected_process < state.processes.len() {
                            let proc = &mut state.processes[state.selected_process];
                            match proc.status {
                                ProcessStatus::Running => {
                                    proc.status = ProcessStatus::Stopped;
                                    state.status = format!(
                                        "Stopped PID {}: {}",
                                        proc.pid,
                                        truncate(&proc.command, 30)
                                    );
                                }
                                ProcessStatus::Stopped => {
                                    proc.status = ProcessStatus::Running;
                                    state.status = format!("Continued PID {}", proc.pid);
                                }
                                _ => {}
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('x') if state.show_process_panel => {
                        // Kill selected process (x for terminate)
                        if state.selected_process < state.processes.len() {
                            let proc = &mut state.processes[state.selected_process];
                            if proc.status == ProcessStatus::Running
                                || proc.status == ProcessStatus::Stopped
                            {
                                proc.status = ProcessStatus::Failed;
                                proc.exit_code = Some(-9); // SIGKILL
                                state.status = format!("Killed PID {} (SIGKILL)", proc.pid);
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('d') if state.show_process_panel => {
                        // Remove completed/failed process from list
                        if state.selected_process < state.processes.len() {
                            let proc = &state.processes[state.selected_process];
                            if proc.status == ProcessStatus::Completed
                                || proc.status == ProcessStatus::Failed
                            {
                                let pid = proc.pid;
                                state.processes.remove(state.selected_process);
                                if state.selected_process > 0
                                    && state.selected_process >= state.processes.len()
                                {
                                    state.selected_process -= 1;
                                }
                                state.status = format!("Removed PID {} from list", pid);
                            }
                        }
                        return true;
                    }
                    _ => return true, // Consume all keys when overlay is open
                }
            }

            // Global shortcuts (when not in slash menu)
            if !state.editor.show_slash_menu && key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('t') => {
                        // Toggle process manager
                        state.show_process_panel = !state.show_process_panel;
                        state.show_history = false;
                        if state.show_process_panel {
                            state.status =
                                "Processes (↑↓ nav, Enter stop/cont, x kill, d remove, Esc close)"
                                    .to_string();
                        }
                        return true;
                    }
                    KeyCode::Char('h') => {
                        // Toggle history panel
                        state.show_history = !state.show_history;
                        state.show_process_panel = false;
                        if state.show_history {
                            state.status =
                                "Session history (↑↓ select, Enter switch, Esc close)".to_string();
                        }
                        return true;
                    }
                    KeyCode::Char('l') => {
                        // Clear chat
                        state.messages.clear();
                        state.status = "Chat cleared".to_string();
                        return true;
                    }
                    KeyCode::Char('a') => {
                        // Toggle mock listening mode
                        toggle_listening(state);
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
                    KeyCode::Char('p') => {
                        // Simulate model prompting for user input
                        let timestamp = format!(
                            "{}:{:02}",
                            10 + state.messages.len() / 60,
                            state.messages.len() % 60
                        );
                        state.messages.push(ChatMessage::prompt(
                            "I found multiple matching files. Which one would you like me to read?\n\n\
                             1. src/main.rs (entry point)\n\
                             2. src/lib.rs (library root)\n\
                             3. src/config.rs (configuration)",
                            &timestamp,
                        ));
                        state.status = "Awaiting user response...".to_string();
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
                    KeyCode::Tab => {
                        if complete_slash_command(state) {
                            return true;
                        }
                        let count = filtered_command_count(state);
                        state.slash_menu.next(count);
                        return true;
                    }
                    KeyCode::BackTab => {
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
                            // If input is a slash command, execute it directly
                            let trimmed = text.trim();
                            if let Some(cmd) = trimmed.strip_prefix('/') {
                                let cmd_name = cmd.split_whitespace().next().unwrap_or("");
                                if !cmd_name.is_empty() {
                                    execute_slash_command(cmd_name, state);
                                    state.editor.clear();
                                    state.slash_menu.hide();
                                    return true;
                                }
                            }

                            if !text.is_empty() {
                                // Add user message
                                let timestamp = format!(
                                    "{}:{:02}",
                                    10 + state.messages.len() / 60,
                                    state.messages.len() % 60
                                );
                                state
                                    .messages
                                    .push(ChatMessage::new("user", &text, &timestamp));

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
                                    state.focus = Panel::Agent;
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
                Panel::Agent => {
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

pub fn on_tick(state: &mut DemoState) {
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
            state.reasoning[2] = format!(
                "  Progress: {}/{} chunks",
                state.stream_index,
                state.stream_buffer.len()
            );
        } else {
            // Streaming complete
            let response = state.streaming.take().unwrap();
            let timestamp = format!(
                "{}:{:02}",
                10 + state.messages.len() / 60,
                state.messages.len() % 60
            );
            state
                .messages
                .push(ChatMessage::new("assistant", &response, &timestamp));
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

                // Emit condensed tool result into chat
                let timestamp = format!(
                    "{}:{:02}",
                    10 + state.messages.len() / 60,
                    state.messages.len() % 60
                );
                let content = match tool.name.as_str() {
                    "code_search" => {
                        "Searching for: \"fn main\" in workspace\n\
                         Found 3 results:\n\
                         → crates/spec-ai-tui/examples/demo.rs: async fn main()\n\
                         → crates/spec-ai-cli/src/main.rs: fn main()\n\
                         → crates/spec-ai-api/src/lib.rs: pub fn run()\n\
                         Showing top matches..."
                    }
                    "file_read" => {
                        "Reading: crates/spec-ai-tui/src/lib.rs (first 40 lines)\n\
                         use spec_ai_tui::app::App;\n\
                         use spec_ai_tui::buffer::Buffer;\n\
                         use spec_ai_tui::geometry::Rect;\n\
                         pub struct DemoApp; ..."
                    }
                    _ => "Tool execution finished successfully.",
                };
                state
                    .messages
                    .push(ChatMessage::tool(&tool.name, content, &timestamp));
            } else {
                state.reasoning[0] = format!("{} Running {}...", spin_char, tool.name);
                state.reasoning[1] = format!("  Elapsed: {}ms", tool.duration_ms.unwrap());
                state.reasoning[2] = "  Waiting for results".to_string();
            }
        }
    }

    // Simulated listening (mock audio transcription)
    if state.listening {
        any_running = true;

        if state.tick % 5 == 0 && state.listen_index < state.listen_buffer.len() {
            let transcript = state.listen_buffer[state.listen_index];
            state.listen_index += 1;

            state.listen_log.push(transcript.to_string());
            if state.listen_log.len() > 6 {
                state.listen_log.remove(0);
            }
            state.status = "Listening (mock mic input)".to_string();
            state.reasoning[0] = format!("{} Listening (mock)...", spin_char);
            state.reasoning[1] = format!("  Segments captured: {}", state.listen_index);
            state.reasoning[2] = "  /listen to stop".to_string();
        } else if state.listen_index >= state.listen_buffer.len() {
            state
                .listen_log
                .push("Listening complete. Saved mock transcripts.".to_string());
            if state.listen_log.len() > 6 {
                state.listen_log.remove(0);
            }
            stop_listening(state, "Listening complete (mock)");
        } else {
            state.reasoning[0] = format!("{} Listening (mock)...", spin_char);
            state.reasoning[1] = format!("  Segments captured: {}", state.listen_index);
            state.reasoning[2] = "  /listen to stop".to_string();
        }
    }

    // Update running process elapsed times
    let mut running_count = 0;
    for proc in &mut state.processes {
        if proc.status == ProcessStatus::Running {
            running_count += 1;
            proc.elapsed_ms += 100;
        }
    }

    // Idle state animation
    if state.streaming.is_none() && !any_running {
        if state.tick % 50 == 0 {
            // Occasionally update idle reasoning
            let proc_info = if running_count > 0 {
                format!("  {} processes running", running_count)
            } else {
                "  Type / for commands".to_string()
            };

            let shortcuts_hint = "  Ctrl+T for processes, Ctrl+H for history";
            let waiting_hint = "  Waiting for input";
            let context_hint = "  Context loaded";
            let messages_hint = format!("  {} messages in history", state.messages.len());
            let tools_hint = format!("  {} tools available", 8);

            let idx = ((state.tick / 50) as usize) % 3;
            match idx {
                0 => {
                    state.reasoning[0] = "◇ Ready".to_string();
                    state.reasoning[1] = waiting_hint.to_string();
                    state.reasoning[2] = proc_info;
                }
                1 => {
                    state.reasoning[0] = "◇ Idle".to_string();
                    state.reasoning[1] = context_hint.to_string();
                    state.reasoning[2] = messages_hint;
                }
                _ => {
                    state.reasoning[0] = "◇ Ready".to_string();
                    state.reasoning[1] = tools_hint;
                    state.reasoning[2] = shortcuts_hint.to_string();
                }
            }
        }
    }
}

fn filtered_command_count(state: &DemoState) -> usize {
    state
        .slash_commands
        .iter()
        .filter(|cmd| cmd.matches(&state.editor.slash_query))
        .count()
}

fn selected_slash_command(state: &DemoState) -> Option<String> {
    let filtered: Vec<_> = state
        .slash_commands
        .iter()
        .filter(|c| c.matches(&state.editor.slash_query))
        .collect();

    filtered
        .get(state.slash_menu.selected_index())
        .map(|c| c.name.clone())
}

fn complete_slash_command(state: &mut DemoState) -> bool {
    if let Some(cmd) = selected_slash_command(state) {
        let text = format!("/{cmd}");
        state.editor.text = text.clone();
        state.editor.selection = Selection::cursor(text.len());
        state.editor.show_slash_menu = false;
        state.editor.slash_query.clear();
        state.slash_menu.hide();
        state.status = format!("Prepared /{} (Enter to run, add args manually)", cmd);
        true
    } else {
        false
    }
}

fn toggle_listening(state: &mut DemoState) {
    if state.listening {
        stop_listening(state, "Listening stopped (mock)");
    } else {
        start_listening(state);
    }
}

fn start_listening(state: &mut DemoState) {
    state.listening = true;
    state.listen_index = 0;
    state.listen_log.clear();
    state.status = "Listening (mock mic input)...".to_string();
    state.reasoning[0] = "◇ Listening (mock mic)".to_string();
    state.reasoning[1] = "  Capturing microphone input".to_string();
    state.reasoning[2] = "  /listen to stop".to_string();

    state
        .listen_log
        .push("Started mock listening session (demo only)".to_string());
}

fn stop_listening(state: &mut DemoState, status: &str) {
    if !state.listening {
        return;
    }
    state.listening = false;
    state.listen_index = 0;
    state.status = status.to_string();
    state.reasoning[0] = "✓ Listening stopped".to_string();
    state.reasoning[1] = "  Transcriptions paused".to_string();
    state.reasoning[2] = "  Use /listen to start again".to_string();
}

fn execute_slash_command(cmd: &str, state: &mut DemoState) {
    // Find the command that matches
    let selected_cmd = selected_slash_command(state)
        .as_deref()
        .unwrap_or(cmd)
        .to_string();

    let timestamp = format!(
        "{}:{:02}",
        10 + state.messages.len() / 60,
        state.messages.len() % 60
    );

    match selected_cmd.as_str() {
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
                 │  Ctrl+T     Open agent processes panel          │\n\
                 │  Ctrl+H     Open session history                │\n\
                 │  Ctrl+R     Simulate tool execution             │\n\
                 │  Ctrl+S     Simulate streaming response         │\n\
                 │  Ctrl+A     Toggle mock listening               │\n\
                 │                                                 │\n\
                 │  SLASH COMMANDS                                 │\n\
                 │  /help /clear /model /system /export            │\n\
                 │  /settings /theme /tools /listen                │\n\
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
        "listen" => {
            toggle_listening(state);
        }
        _ => {
            state.status = format!("Unknown command: /{}", selected_cmd);
        }
    }
}
