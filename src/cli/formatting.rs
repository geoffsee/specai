//! Terminal formatting utilities using termimad for rich markdown rendering

use crate::agent::core::{AgentOutput, MemoryRecallStrategy};
use serde_json::to_string;
use std::cell::Cell;
use termimad::*;

thread_local! {
    /// Override for terminal detection in tests
    static FORCE_PLAIN_TEXT: Cell<bool> = Cell::new(false);
}

/// Force plain text output (for testing)
/// Available for both unit and integration tests
pub fn set_plain_text_mode(enabled: bool) {
    FORCE_PLAIN_TEXT.with(|f| f.set(enabled));
}

/// Initialize a custom MadSkin with spec-ai color scheme
pub fn create_skin() -> MadSkin {
    let mut skin = MadSkin::default();

    // Headers - cyan with bold
    let mut header_style = CompoundStyle::with_fg(termimad::crossterm::style::Color::Cyan);
    header_style.add_attr(termimad::crossterm::style::Attribute::Bold);
    skin.headers[0].compound_style = header_style;
    skin.headers[1].compound_style =
        CompoundStyle::with_fg(termimad::crossterm::style::Color::Cyan);

    // Bold text - bright white
    skin.bold.set_fg(termimad::crossterm::style::Color::White);

    // Italic - dim white
    skin.italic.set_fg(termimad::crossterm::style::Color::Grey);

    // Inline code - yellow background
    skin.inline_code
        .set_fg(termimad::crossterm::style::Color::Yellow);

    // Code blocks - with border
    skin.code_block
        .set_fg(termimad::crossterm::style::Color::White);

    // Links - blue
    skin.paragraph.compound_style = CompoundStyle::default();

    // Lists - improved bullet points with better colors and symbol
    skin.bullet = StyledChar::from_fg_char(termimad::crossterm::style::Color::Green, '▸');

    // List item styling - make list items stand out
    skin.paragraph.compound_style =
        CompoundStyle::with_fg(termimad::crossterm::style::Color::White);

    // Quote styling for better visual hierarchy
    skin.quote_mark
        .set_fg(termimad::crossterm::style::Color::DarkCyan);
    skin.quote_mark.set_char('┃');

    skin
}

/// Check if we're in a TTY (terminal) or if output is piped/redirected
pub fn is_terminal() -> bool {
    // Check for test override first
    if FORCE_PLAIN_TEXT.with(|f| f.get()) {
        return false;
    }

    // Use terminal_size as a proxy for TTY detection
    terminal_size::terminal_size().is_some()
}

/// Render markdown text with the spec-ai skin
/// Falls back to plain text if not in a terminal
pub fn render_markdown(text: &str) -> String {
    if !is_terminal() {
        return text.to_string();
    }

    let skin = create_skin();
    let terminal_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);

    skin.text(text, Some(terminal_width)).to_string()
}

/// Render agent response with markdown formatting
pub fn render_agent_response(role: &str, content: &str) -> String {
    if !is_terminal() {
        return format!("{}: {}", role, content);
    }

    let skin = create_skin();
    let terminal_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);

    // Format with role header
    let formatted = format!("**{}:**\n\n{}", role, content);
    skin.text(&formatted, Some(terminal_width)).to_string()
}

/// Render run metadata (memory recall, tools, token usage)
pub fn render_run_stats(output: &AgentOutput) -> Option<String> {
    let mut sections = Vec::new();

    if let Some(stats) = &output.recall_stats {
        let mut section = String::from("## Memory Recall\n");
        match stats.strategy {
            MemoryRecallStrategy::Semantic {
                requested,
                returned,
            } => {
                section.push_str(&format!(
                    "- Strategy: semantic (requested top {}, returned {})\n",
                    requested, returned
                ));
            }
            MemoryRecallStrategy::RecentContext { limit } => {
                section.push_str(&format!(
                    "- Strategy: recent context window (last {} messages)\n",
                    limit
                ));
            }
        }

        if stats.matches.is_empty() {
            section.push_str("- No recalled vector matches this turn.\n");
        } else {
            section.push_str("- Matches:\n");
            for (idx, m) in stats.matches.iter().take(3).enumerate() {
                section.push_str(&format!(
                    "  {}. [{} | score {:.2}] {}\n",
                    idx + 1,
                    m.role.as_str(),
                    m.score,
                    m.preview
                ));
            }
            if stats.matches.len() > 3 {
                section.push_str(&format!(
                    "  ... {} additional matches omitted\n",
                    stats.matches.len() - 3
                ));
            }
        }
        sections.push(section);
    }

    if !output.tool_invocations.is_empty() {
        let mut section = String::from("## Tool Calls\n");
        for inv in &output.tool_invocations {
            section.push_str(&format!(
                "- **{}** [{}]",
                inv.name,
                if inv.success { "ok" } else { "error" }
            ));
            let args = to_string(&inv.arguments).unwrap_or_else(|_| "{}".to_string());
            section.push_str(&format!(" args: `{}`", args));

            if let Some(out) = &inv.output {
                if !out.is_empty() {
                    section.push_str(&format!(" → {}", out));
                }
            }

            if let Some(err) = &inv.error {
                section.push_str(&format!(" (error: {})", err));
            }

            section.push('\n');
        }
        sections.push(section);
    }

    if let Some(usage) = &output.token_usage {
        sections.push(format!(
            "## Tokens\n- Prompt: {}\n- Completion: {}\n- Total: {}\n",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        ));
    }

    if sections.is_empty() {
        return None;
    }

    let markdown = format!("---\n\n# Run Stats\n\n{}", sections.join("\n"));
    Some(render_markdown(&markdown))
}

/// Render help text with rich markdown formatting
pub fn render_help() -> String {
    let help_text = r#"
# SpecAI Commands

## Agent Management
Manage your AI agent profiles and sessions:

- **`/agents`** or **`/list`** — List all available agent profiles
- **`/switch <name>`** — Switch to a different agent profile
- **`/new <name>`** — Create new conversation session

## Configuration
Control your SpecAI configuration:

- **`/config show`** — Display current configuration
  - Shows model provider, temperature, and other settings
- **`/config reload`** — Reload configuration from file
  - Useful after editing config.toml

## Memory & History
Access conversation memory:

- **`/memory show [N]`** — Show last N messages (default: 10)
  - Displays color-coded conversation history
- **`/memory clear`** — Clear conversation history

## Session Management
Manage multiple conversation sessions:

- **`/session list`** — List all conversation sessions
- **`/session load <id>`** — Load a specific session
- **`/session delete <id>`** — Delete a session

## General Commands
- **`/help`** — Show this help message
- **`/quit`** or **`/exit`** — Exit the REPL

---

**Usage:** Type your message to chat with the current agent. Use `/` prefix for commands.
"#;

    render_markdown(help_text)
}

/// Create a formatted table for agent list
pub fn render_agent_table(agents: Vec<(String, bool, Option<String>)>) -> String {
    if !is_terminal() {
        // Plain text fallback
        let mut output = String::from("Available agents:\n");
        for (name, is_active, description) in agents {
            let active_marker = if is_active { " (active)" } else { "" };
            let desc = description.unwrap_or_default();
            output.push_str(&format!("  - {}{}", name, active_marker));
            if !desc.is_empty() {
                output.push_str(&format!(" - {}", desc));
            }
            output.push('\n');
        }
        return output;
    }

    let skin = create_skin();
    let terminal_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);

    // Build markdown table
    let mut table = String::from("# Available Agents\n\n");
    table.push_str("| Agent | Status | Description |\n");
    table.push_str("|-------|--------|-------------|\n");

    for (name, is_active, description) in agents {
        let status = if is_active { "**active**" } else { "" };
        let desc = description.unwrap_or_default();
        table.push_str(&format!("| {} | {} | {} |\n", name, status, desc));
    }

    skin.text(&table, Some(terminal_width)).to_string()
}

/// Format memory/history display with role-based color coding
pub fn render_memory(messages: Vec<(String, String)>) -> String {
    if !is_terminal() {
        // Plain text fallback
        let mut output = String::new();
        for (role, content) in messages {
            output.push_str(&format!("{}: {}\n", role, content));
        }
        return output;
    }

    let skin = create_skin();
    let terminal_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);

    let mut formatted = String::from("# Conversation History\n\n");

    for (role, content) in messages {
        let role_formatted = match role.as_str() {
            "user" => "**👤 User:**",
            "assistant" => "**🤖 Assistant:**",
            "system" => "**⚙️  System:**",
            _ => &format!("**{}:**", role),
        };

        formatted.push_str(&format!("{}\n{}\n\n---\n\n", role_formatted, content));
    }

    skin.text(&formatted, Some(terminal_width)).to_string()
}

/// Format configuration display with sections
pub fn render_config(config_text: &str) -> String {
    if !is_terminal() {
        return config_text.to_string();
    }

    let skin = create_skin();
    let terminal_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);

    let formatted = format!("# Current Configuration\n\n```toml\n{}\n```", config_text);
    skin.text(&formatted, Some(terminal_width)).to_string()
}

/// Render a formatted list with custom bullet styling
pub fn render_list(title: &str, items: Vec<String>) -> String {
    if !is_terminal() {
        let mut output = format!("{}:\n", title);
        for item in items {
            output.push_str(&format!("  - {}\n", item));
        }
        return output;
    }

    let skin = create_skin();
    let terminal_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);

    let mut formatted = format!("## {}\n\n", title);
    for item in items {
        formatted.push_str(&format!("- {}\n", item));
    }

    skin.text(&formatted, Some(terminal_width)).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_markdown_basic() {
        let text = "**bold** and *italic*";
        let result = render_markdown(text);
        // Just ensure it doesn't panic
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_agent_table() {
        let agents = vec![
            (
                "default".to_string(),
                true,
                Some("Default agent".to_string()),
            ),
            ("researcher".to_string(), false, None),
        ];
        let result = render_agent_table(agents);
        assert!(result.contains("default"));
        assert!(result.contains("researcher"));
    }
}
