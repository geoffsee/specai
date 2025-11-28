//! Demo application state and defaults.

use crate::models::{AgentProcess, ChatMessage, Session, ToolExecution, ToolStatus};
use spec_ai_tui::widget::builtin::{EditorState, SlashCommand, SlashMenuState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Input,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Standard,
    GlassesHud,
}

/// Demo application state
pub struct DemoState {
    /// Editor field state
    pub editor: EditorState,
    /// Slash menu state
    pub slash_menu: SlashMenuState,
    /// Available slash commands
    pub slash_commands: Vec<SlashCommand>,
    /// Chat messages
    pub messages: Vec<ChatMessage>,
    /// Current streaming response (simulated)
    pub streaming: Option<String>,
    /// Scroll offset for chat
    pub scroll_offset: u16,
    /// Status message
    pub status: String,
    /// Active tools
    pub tools: Vec<ToolExecution>,
    /// Reasoning messages
    pub reasoning: Vec<String>,
    /// Should quit
    pub quit: bool,
    /// Current panel focus
    pub focus: Panel,
    /// Tick counter for animations
    pub tick: u64,
    /// Simulated streaming state
    pub stream_buffer: Vec<&'static str>,
    pub stream_index: usize,
    /// Mock listening mode active
    pub listening: bool,
    /// Simulated listening transcript buffer
    pub listen_buffer: Vec<&'static str>,
    /// Current index in listening buffer
    pub listen_index: usize,
    /// Display buffer for recent listening lines
    pub listen_log: Vec<String>,
    /// Agent-spawned processes
    pub processes: Vec<AgentProcess>,
    /// Show process manager overlay
    pub show_process_panel: bool,
    /// Selected process in panel
    pub selected_process: usize,
    /// Viewing logs for process (index)
    pub viewing_logs: Option<usize>,
    /// Log scroll offset
    pub log_scroll: usize,
    /// Session history
    pub sessions: Vec<Session>,
    /// Current session index
    pub current_session: usize,
    /// Show history overlay
    pub show_history: bool,
    /// Selected session in history
    pub selected_session: usize,
    /// Pending quit (first Ctrl+C pressed)
    pub pending_quit: bool,
    /// Display mode (standard vs glasses HUD)
    pub display_mode: DisplayMode,
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
                SlashCommand::new("listen", "Toggle mock audio listening"),
                SlashCommand::new("glass", "Toggle glasses-optimized HUD"),
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
            listening: false,
            listen_buffer: vec![
                "[mic] Calibrating input gain... (ok)",
                "[mic] User: \"hey there, can you summarize the last deploy?\"",
                "[mic] Assistant: \"Sure, checking the deploy logs now.\"",
                "[mic] User: \"focus on API changes and DB migrations\"",
                "[mic] (silence) listening...",
            ],
            listen_index: 0,
            listen_log: Vec::new(),
            // Mock agent-spawned processes with full log output
            processes: vec![
                {
                    let mut proc = AgentProcess::new(48291, "cargo test --all --no-fail-fast", "test-runner");
                    proc.elapsed_ms = 45200;
                    proc.output_lines = vec![
                        "   Compiling spec-ai-tui v0.4.16".to_string(),
                        "   Compiling spec-ai v0.4.16".to_string(),
                        "    Finished test [unoptimized + debuginfo] target(s) in 12.34s".to_string(),
                        "     Running unittests src/lib.rs".to_string(),
                        "".to_string(),
                        "running 47 tests".to_string(),
                        "test buffer::tests::test_cell_default ... ok".to_string(),
                        "test buffer::tests::test_buffer_creation ... ok".to_string(),
                        "test buffer::tests::test_set_string ... ok".to_string(),
                        "test geometry::tests::test_rect_new ... ok".to_string(),
                        "test geometry::tests::test_rect_intersection ... ok".to_string(),
                        "test geometry::tests::test_rect_union ... ok".to_string(),
                        "test layout::tests::test_vertical_split ... ok".to_string(),
                        "test layout::tests::test_horizontal_split ... ok".to_string(),
                        "test style::tests::test_color_rgb ... ok".to_string(),
                        "test style::tests::test_modifier_combine ... ok".to_string(),
                        "test widget::tests::test_block_render ... ok".to_string(),
                        "test widget::tests::test_editor_insert ... ok".to_string(),
                        "...".to_string(),
                        "test result: ok. 47 passed; 0 failed; 0 ignored".to_string(),
                    ];
                    proc
                },
                {
                    let mut proc = AgentProcess::new(48156, "npm run dev -- --port 3000", "dev-server");
                    proc.elapsed_ms = 182000;
                    proc.output_lines = vec![
                        "> frontend@0.1.0 dev".to_string(),
                        "> next dev --port 3000".to_string(),
                        "".to_string(),
                        "  ▲ Next.js 14.0.4".to_string(),
                        "  - Local:        http://localhost:3000".to_string(),
                        "  - Environments: .env.local".to_string(),
                        "".to_string(),
                        " ✓ Ready in 2.1s".to_string(),
                        " ○ Compiling /page ...".to_string(),
                        " ✓ Compiled /page in 892ms (512 modules)".to_string(),
                        " ○ Compiling /api/auth ...".to_string(),
                        " ✓ Compiled /api/auth in 234ms (89 modules)".to_string(),
                        " GET / 200 in 45ms".to_string(),
                        " GET /api/auth/session 200 in 12ms".to_string(),
                        " GET /_next/static/chunks/main.js 200 in 3ms".to_string(),
                    ];
                    proc
                },
                {
                    let mut proc = AgentProcess::new(47892, "cargo watch -x check", "file-watcher");
                    proc.elapsed_ms = 892000;
                    proc.output_lines = vec![
                        "[cargo-watch] Watching /Users/dev/project".to_string(),
                        "[cargo-watch] Waiting for changes...".to_string(),
                        "[cargo-watch] Change detected: src/lib.rs".to_string(),
                        "[Running 'cargo check']".to_string(),
                        "    Checking spec-ai-tui v0.4.16".to_string(),
                        "    Checking spec-ai v0.4.16".to_string(),
                        "    Finished dev [unoptimized + debuginfo] target(s) in 4.21s".to_string(),
                        "[cargo-watch] Waiting for changes...".to_string(),
                        "[cargo-watch] Change detected: src/widget/mod.rs".to_string(),
                        "[Running 'cargo check']".to_string(),
                        "    Checking spec-ai-tui v0.4.16".to_string(),
                        "    Finished dev [unoptimized + debuginfo] target(s) in 1.89s".to_string(),
                        "[cargo-watch] Waiting for changes...".to_string(),
                    ];
                    proc
                },
                {
                    let mut proc = AgentProcess::new(48302, "docker compose up db redis", "infra");
                    proc.elapsed_ms = 120000;
                    proc.output_lines = vec![
                        "[+] Running 2/2".to_string(),
                        " ⠿ Container project-db-1     Created".to_string(),
                        " ⠿ Container project-redis-1  Created".to_string(),
                        "Attaching to db-1, redis-1".to_string(),
                        "db-1    | PostgreSQL Database directory appears to contain a database".to_string(),
                        "db-1    | 2024-01-15 10:00:01.234 UTC [1] LOG:  starting PostgreSQL 15.4".to_string(),
                        "db-1    | 2024-01-15 10:00:01.456 UTC [1] LOG:  listening on IPv4 address \"0.0.0.0\", port 5432".to_string(),
                        "db-1    | 2024-01-15 10:00:01.567 UTC [1] LOG:  database system is ready to accept connections".to_string(),
                        "redis-1 | 1:C 15 Jan 2024 10:00:01.123 # oO0OoO0OoO0Oo Redis is starting".to_string(),
                        "redis-1 | 1:M 15 Jan 2024 10:00:01.234 * Ready to accept connections".to_string(),
                        "db-1    | 2024-01-15 10:02:15.789 UTC [47] LOG:  connection received: host=172.18.0.1 port=54321".to_string(),
                        "db-1    | 2024-01-15 10:02:15.812 UTC [47] LOG:  connection authorized: user=app database=myapp".to_string(),
                    ];
                    proc
                },
            ],
            show_process_panel: false,
            selected_process: 0,
            viewing_logs: None,
            log_scroll: 0,
            // Mock sessions with history
            sessions: vec![
                Session {
                    id: 1,
                    title: "Current session".to_string(),
                    preview: "TUI framework development".to_string(),
                    timestamp: "Today 10:00".to_string(),
                    message_count: 0, // Will be updated from messages
                    messages: vec![], // Current session uses state.messages
                },
                Session {
                    id: 2,
                    title: "API refactoring".to_string(),
                    preview: "Discussed REST → GraphQL migration".to_string(),
                    timestamp: "Yesterday".to_string(),
                    message_count: 24,
                    messages: vec![
                        ChatMessage::new("user", "How should we approach the API migration?", "14:30"),
                        ChatMessage::new("assistant", "I recommend a phased approach: 1) Create GraphQL schema, 2) Implement resolvers, 3) Add Apollo client, 4) Deprecate REST endpoints.", "14:31"),
                        ChatMessage::new("user", "What about backwards compatibility?", "14:35"),
                        ChatMessage::new("assistant", "Keep REST endpoints during transition. Add deprecation headers and document timeline.", "14:36"),
                    ],
                },
                Session {
                    id: 3,
                    title: "Bug investigation".to_string(),
                    preview: "Memory leak in connection pool".to_string(),
                    timestamp: "2 days ago".to_string(),
                    message_count: 18,
                    messages: vec![
                        ChatMessage::new("user", "There's a memory leak when connections aren't returned to pool", "09:15"),
                        ChatMessage::tool("grep", "Searching for: 'pool.acquire' in src/\nFound 12 occurrences", "09:15"),
                        ChatMessage::new("assistant", "Found the issue - connections acquired in error paths aren't being released. Adding drop guards.", "09:16"),
                    ],
                },
                Session {
                    id: 4,
                    title: "Performance optimization".to_string(),
                    preview: "Reduced latency by 40%".to_string(),
                    timestamp: "Last week".to_string(),
                    message_count: 42,
                    messages: vec![
                        ChatMessage::new("user", "The dashboard is loading slowly", "16:00"),
                        ChatMessage::tool("profiler", "Flame graph generated: database queries take 800ms", "16:01"),
                        ChatMessage::new("assistant", "Main bottleneck is N+1 queries. Adding eager loading and query batching.", "16:02"),
                    ],
                },
                Session {
                    id: 5,
                    title: "Documentation sprint".to_string(),
                    preview: "Updated API docs and README".to_string(),
                    timestamp: "2 weeks ago".to_string(),
                    message_count: 15,
                    messages: vec![
                        ChatMessage::new("user", "Help me document the authentication flow", "11:00"),
                        ChatMessage::new("assistant", "I'll create a sequence diagram and update the API reference.", "11:01"),
                    ],
                },
            ],
            current_session: 0,
            show_history: false,
            selected_session: 0,
            pending_quit: false,
            display_mode: DisplayMode::Standard,
        }
    }
}
