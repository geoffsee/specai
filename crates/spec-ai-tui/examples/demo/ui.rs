//! Rendering routines for the demo application.

use crate::models::ProcessStatus;
use crate::state::{DemoState, DisplayMode, Panel};
use spec_ai_tui::{
    buffer::Buffer,
    geometry::Rect,
    layout::{Constraint, Layout},
    style::{truncate, wrap_text, Color, Line, Span, Style},
    widget::{
        builtin::{Block, Editor, Overlay, SlashCommand, SlashMenu, StatusBar, StatusSection},
        StatefulWidget, Widget,
    },
};

pub fn render(state: &DemoState, area: Rect, buf: &mut Buffer) {
    if state.display_mode == DisplayMode::GlassesHud {
        render_glass_mode(state, area, buf);

        // Keep overlays available even in HUD mode for debugging/demo parity.
        if state.show_process_panel {
            render_process_overlay(state, area, buf);
        }
        if state.show_history {
            render_history_overlay(state, area, buf);
        }
        if state.viewing_logs.is_some() {
            render_log_overlay(state, area, buf);
        }
        return;
    }

    // Main layout: content + status
    let main_chunks = Layout::vertical()
        .constraints([
            Constraint::Fill(1),  // Main content
            Constraint::Fixed(3), // Reasoning panel
            Constraint::Fixed(1), // Status bar
        ])
        .split(area);

    // Content layout: chat + input
    let content_chunks = Layout::vertical()
        .constraints([
            Constraint::Fill(1),  // Chat
            Constraint::Fixed(8), // Input area (taller for wrapped text)
        ])
        .split(main_chunks[0]);

    render_chat(state, content_chunks[0], buf);
    render_input(state, content_chunks[1], buf);
    render_reasoning(state, main_chunks[1], buf);
    render_status(state, main_chunks[2], buf);
    render_listen_overlay(state, content_chunks[0], buf);

    if state.show_process_panel {
        render_process_overlay(state, area, buf);
    }
    if state.show_history {
        render_history_overlay(state, area, buf);
    }
    if state.viewing_logs.is_some() {
        render_log_overlay(state, area, buf);
    }
}

fn render_glass_mode(state: &DemoState, area: Rect, buf: &mut Buffer) {
    // Keep the background consistent and low-noise for a small FOV.
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buf.get_mut(x, y) {
                cell.bg = Color::Rgb(5, 7, 12);
            }
        }
    }

    let chunks = Layout::vertical()
        .constraints([
            Constraint::Fixed(1), // Status bar
            Constraint::Fill(1),  // Focus card
            Constraint::Fixed(3), // Quick controls
        ])
        .split(area);

    render_glass_status_bar(state, chunks[0], buf);
    render_glass_focus_card(state, chunks[1], buf);
    render_glass_footer(state, chunks[2], buf);

    // Torus as ambient presence indicator in bottom-right corner
    render_glass_torus_overlay(state, area, buf);
}

fn render_glass_status_bar(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let mic = if state.listening { "mic:on" } else { "mic:off" };
    let tools_active = state
        .tools
        .iter()
        .filter(|t| t.status == crate::models::ToolStatus::Running)
        .count();
    let tools_label = if tools_active > 0 {
        format!("tools:{}*", tools_active)
    } else {
        "tools:idle".to_string()
    };

    let status_text = truncate(&state.status, area.width.saturating_sub(24) as usize);

    let bar = StatusBar::new()
        .style(Style::new().bg(Color::Rgb(8, 8, 14)).fg(Color::White))
        .left([
            StatusSection::new("spec-ai").style(Style::new().fg(Color::Cyan).bold()),
            StatusSection::new("GLASS").style(Style::new().fg(Color::Magenta)),
        ])
        .center([StatusSection::new(status_text)])
        .right([
            StatusSection::new(mic).style(Style::new().fg(Color::Yellow)),
            StatusSection::new(tools_label).style(Style::new().fg(Color::DarkGrey)),
            StatusSection::new(format!("msgs:{}", state.messages.len()))
                .style(Style::new().fg(Color::DarkGrey)),
        ]);

    Widget::render(&bar, area, buf);
}

/// Renders a small torus as an ambient presence indicator in the bottom-right corner.
fn render_glass_torus_overlay(state: &DemoState, area: Rect, buf: &mut Buffer) {
    // Larger size for better braille resolution
    let torus_width: u16 = 28;
    let torus_height: u16 = 12;

    // Position: bottom-right corner with small margin
    let margin = 1_u16;
    let x = area.right().saturating_sub(torus_width + margin);
    let y = area.bottom().saturating_sub(torus_height + margin + 3); // Above footer

    if x < area.x || y < area.y {
        return;
    }

    let torus_area = Rect::new(x, y, torus_width, torus_height);
    render_torus(state, torus_area, buf);
}

/// Core torus rendering using braille characters for 8x resolution.
/// Each braille character is a 2x4 grid of dots, giving much smoother curves.
fn render_torus(state: &DemoState, area: Rect, buf: &mut Buffer) {
    if area.width < 8 || area.height < 4 {
        return;
    }

    let char_width = area.width as usize;
    let char_height = area.height as usize;

    // Braille gives us 2x horizontal and 4x vertical resolution
    let pixel_width = char_width * 2;
    let pixel_height = char_height * 4;

    // Torus parameters
    let r1: f32 = 1.0; // Tube radius
    let r2: f32 = 2.0; // Torus radius
    let k1: f32 = (pixel_height as f32) * 0.7; // Scale factor

    // Animation: rotation based on tick
    let speed_mult = if state.streaming.is_some() { 0.15 } else { 0.03 };
    let a = (state.tick as f32) * speed_mult;
    let b = (state.tick as f32) * speed_mult * 0.6;

    let (sin_a, cos_a) = a.sin_cos();
    let (sin_b, cos_b) = b.sin_cos();

    // High-res z-buffer and luminance buffer
    let mut z_buffer = vec![0.0_f32; pixel_width * pixel_height];
    let mut lum_buffer = vec![-2.0_f32; pixel_width * pixel_height]; // -2 = empty

    // Sample the torus surface densely for solid braille fill
    let theta_steps = 314;
    let phi_steps = 157;

    for i in 0..theta_steps {
        let theta = (i as f32) * 2.0 * std::f32::consts::PI / (theta_steps as f32);
        let (sin_theta, cos_theta) = theta.sin_cos();

        for j in 0..phi_steps {
            let phi = (j as f32) * 2.0 * std::f32::consts::PI / (phi_steps as f32);
            let (sin_phi, cos_phi) = phi.sin_cos();

            let circle_x = r2 + r1 * cos_theta;
            let circle_y = r1 * sin_theta;

            let x = circle_x * (cos_b * cos_phi + sin_a * sin_b * sin_phi)
                - circle_y * cos_a * sin_b;
            let y = circle_x * (sin_b * cos_phi - sin_a * cos_b * sin_phi)
                + circle_y * cos_a * cos_b;
            let z = cos_a * circle_x * sin_phi + circle_y * sin_a;

            let distance = 5.0;
            let ooz = 1.0 / (z + distance);

            // Project to pixel coordinates
            let xp = (pixel_width as f32 / 2.0 + k1 * ooz * x * 0.5) as i32;
            let yp = (pixel_height as f32 / 2.0 - k1 * ooz * y * 0.5) as i32;

            if xp >= 0 && xp < pixel_width as i32 && yp >= 0 && yp < pixel_height as i32 {
                let idx = yp as usize * pixel_width + xp as usize;

                let luminance = cos_phi * cos_theta * sin_b
                    - cos_a * cos_theta * sin_phi
                    - sin_a * sin_theta
                    + cos_b * (cos_a * sin_theta - cos_theta * sin_a * sin_phi);

                if ooz > z_buffer[idx] {
                    z_buffer[idx] = ooz;
                    lum_buffer[idx] = luminance;
                }
            }
        }
    }

    // Convert pixel buffer to braille characters
    // Braille dot positions in a 2x4 cell:
    //   0 3
    //   1 4
    //   2 5
    //   6 7
    let dot_bits: [u8; 8] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80];

    let is_active = state.streaming.is_some();

    for cy in 0..char_height {
        for cx in 0..char_width {
            let mut braille: u8 = 0;
            let mut max_lum: f32 = -2.0;
            let mut has_pixel = false;

            // Check each of the 8 dots in this braille cell
            for dot in 0..8 {
                let (dx, dy) = match dot {
                    0 => (0, 0),
                    1 => (0, 1),
                    2 => (0, 2),
                    3 => (1, 0),
                    4 => (1, 1),
                    5 => (1, 2),
                    6 => (0, 3),
                    7 => (1, 3),
                    _ => unreachable!(),
                };

                let px = cx * 2 + dx;
                let py = cy * 4 + dy;

                if px < pixel_width && py < pixel_height {
                    let idx = py * pixel_width + px;
                    let lum = lum_buffer[idx];
                    if lum > -1.5 {
                        // Has a pixel here
                        braille |= dot_bits[dot];
                        has_pixel = true;
                        if lum > max_lum {
                            max_lum = lum;
                        }
                    }
                }
            }

            if has_pixel {
                // Convert to braille unicode (U+2800 + pattern)
                let braille_char = char::from_u32(0x2800 + braille as u32).unwrap_or(' ');

                // Color based on luminance and activity
                let color = if is_active {
                    if max_lum > 0.4 {
                        Color::Rgb(255, 230, 180) // Warm bright
                    } else if max_lum > 0.0 {
                        Color::Rgb(220, 180, 120) // Warm mid
                    } else {
                        Color::Rgb(140, 100, 60) // Warm dark
                    }
                } else if max_lum > 0.4 {
                    Color::Rgb(160, 220, 240) // Cool bright
                } else if max_lum > 0.0 {
                    Color::Rgb(80, 150, 180) // Cool mid
                } else {
                    Color::Rgb(40, 80, 110) // Cool dark
                };

                buf.set_string(
                    area.x + cx as u16,
                    area.y + cy as u16,
                    &braille_char.to_string(),
                    Style::new().fg(color),
                );
            }
        }
    }
}

fn render_glass_focus_card(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .title("Glasses HUD")
        .border_style(Style::new().fg(Color::Cyan));
    let inner = block.inner(area);
    Widget::render(&block, area, buf);

    if inner.is_empty() {
        return;
    }

    let width = inner.width.max(1) as usize;
    let mut y = inner.y;

    let focus_line = if let Some(ref streaming) = state.streaming {
        format!(
            "Responding… {}",
            truncate(streaming, width.saturating_sub(14))
        )
    } else if let Some(msg) = state.messages.iter().rev().find(|m| m.role == "assistant") {
        format!(
            "Assistant: {}",
            truncate(&msg.content.replace('\n', " "), width.saturating_sub(12))
        )
    } else {
        format!(
            "Status: {}",
            truncate(&state.status, width.saturating_sub(10))
        )
    };

    for line in wrap_text(&focus_line, width, "") {
        if y >= inner.bottom() {
            break;
        }
        buf.set_string(inner.x, y, &line, Style::new().fg(Color::White).bold());
        y += 1;
    }

    if y < inner.bottom() {
        if let Some(msg) = state.messages.iter().rev().find(|m| m.role == "user") {
            let user_line = format!(
                "You: {}",
                truncate(&msg.content.replace('\n', " "), width.saturating_sub(6))
            );
            for line in wrap_text(&user_line, width, "") {
                if y >= inner.bottom() {
                    break;
                }
                buf.set_string(
                    inner.x,
                    y,
                    &line,
                    Style::new().fg(Color::Rgb(170, 220, 170)),
                );
                y += 1;
            }
        }
    }

    let reasoning_lines: Vec<_> = state
        .reasoning
        .iter()
        .take(2)
        .map(|r| truncate(r, width))
        .collect();
    for line in reasoning_lines {
        if y >= inner.bottom() {
            break;
        }
        buf.set_string(
            inner.x,
            y,
            &line,
            Style::new().fg(Color::Rgb(120, 140, 170)),
        );
        y += 1;
    }

    if y < inner.bottom() {
        let mic_line = if state.listening {
            "Mic: live — listening for cues"
        } else {
            "Mic: off — tap Ctrl+A to monitor"
        };
        buf.set_string(
            inner.x,
            y,
            &truncate(mic_line, width),
            Style::new().fg(Color::Yellow),
        );
        y += 1;
    }

    if y < inner.bottom() {
        let quick_hint = "Glance-safe: big type, few lines, no clutter";
        buf.set_string(
            inner.x,
            y,
            &truncate(quick_hint, width),
            Style::new().fg(Color::DarkGrey),
        );
    }
}

fn render_glass_footer(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .title("Quick controls")
        .border_style(Style::new().fg(Color::DarkGrey));
    let inner = block.inner(area);
    Widget::render(&block, area, buf);

    if inner.is_empty() {
        return;
    }

    let width = inner.width as usize;
    let hints = [
        "Ctrl+G or /glass toggles HUD",
        "Ctrl+A mic | Ctrl+S stream | Ctrl+T processes",
        "Designed for narrow viewports — short, high-contrast lines",
    ];

    for (i, hint) in hints.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        let fg = if i == 0 {
            Color::Cyan
        } else {
            Color::Rgb(120, 120, 140)
        };
        buf.set_string(inner.x, y, &truncate(hint, width), Style::new().fg(fg));
    }
}

fn render_chat(state: &DemoState, area: Rect, buf: &mut Buffer) {
    // Draw border
    let border_style = if state.focus == Panel::Agent {
        Style::new().fg(Color::Cyan)
    } else {
        Style::new().fg(Color::DarkGrey)
    };

    let block = Block::bordered().title("Agent").border_style(border_style);
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
            "user" => (Style::new().fg(Color::Green).bold(), "User".to_string()),
            "assistant" => (Style::new().fg(Color::Cyan).bold(), "Assistant".to_string()),
            "system" => (Style::new().fg(Color::Yellow).bold(), "System".to_string()),
            "tool" => {
                let name = msg.tool_name.as_deref().unwrap_or("tool");
                (
                    Style::new().fg(Color::Magenta).bold(),
                    format!("⚙ {}", name),
                )
            }
            _ => (Style::new().fg(Color::White), msg.role.clone()),
        };

        // Add prompt indicator if this is a prompt message
        if msg.is_prompt {
            lines.push(Line::from_spans([
                Span::styled(
                    format!("[{}] ", msg.timestamp),
                    Style::new().fg(Color::DarkGrey),
                ),
                Span::styled(format!("{}:", role_display), role_style),
                Span::styled(
                    " (waiting for input)",
                    Style::new().fg(Color::Yellow).italic(),
                ),
            ]));
        } else {
            lines.push(Line::from_spans([
                Span::styled(
                    format!("[{}] ", msg.timestamp),
                    Style::new().fg(Color::DarkGrey),
                ),
                Span::styled(format!("{}:", role_display), role_style),
            ]));
        }

        // Content lines with word wrapping
        // Tool messages get a special background indicator
        let is_tool = msg.role == "tool";
        let content_style = if is_tool {
            Style::new().fg(Color::DarkGrey)
        } else {
            Style::new()
        };
        let prefix = if is_tool { "  │ " } else { "  " };

        let content_to_render = if is_tool {
            condensed_tool_summary(&msg.content, content_width)
        } else {
            msg.content.clone()
        };

        for content_line in content_to_render.lines() {
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
            Span::styled("Assistant:", Style::new().fg(Color::Cyan).bold()),
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
        lines.push(Line::styled(
            format!("  {}", cursor_char),
            Style::new().fg(Color::Cyan),
        ));
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

fn render_input(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let border_style = if state.focus == Panel::Input {
        Style::new().fg(Color::Cyan)
    } else {
        Style::new().fg(Color::DarkGrey)
    };

    let block = Block::bordered().title("Input").border_style(border_style);
    Widget::render(&block, area, buf);

    let inner = block.inner(area);
    if inner.is_empty() {
        return;
    }

    // Help text at top
    let help_text = if state.editor.show_slash_menu {
        "↑/↓: select | Tab: autocomplete | Enter: execute | Esc: cancel"
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
    let editor_area = Rect::new(
        inner.x + 2,
        inner.y + 1,
        inner.width.saturating_sub(2),
        editor_height,
    );
    let editor = Editor::new()
        .placeholder("Type a message... (/ for commands)")
        .style(Style::new().fg(Color::White));

    let mut editor_state = state.editor.clone();
    editor.render(editor_area, buf, &mut editor_state);

    // Render slash menu if active
    if state.editor.show_slash_menu {
        // Create the menu widget
        let filtered_commands: Vec<SlashCommand> = state
            .slash_commands
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

fn condensed_tool_summary(content: &str, content_width: usize) -> String {
    let summary_width = content_width.max(40).min(200);
    let mut lines: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    if lines.is_empty() {
        return truncate(content, summary_width);
    }

    let extra_lines = lines.len().saturating_sub(3);
    lines.truncate(3);

    let mut summary = lines.join(" • ");
    if extra_lines > 0 {
        summary.push_str(&format!(" ... (+{} more)", extra_lines));
    }

    let condensed = truncate(&summary, summary_width);
    if condensed != summary {
        truncate(&format!("{} (condensed)", condensed), summary_width)
    } else {
        condensed
    }
}

fn render_reasoning(state: &DemoState, area: Rect, buf: &mut Buffer) {
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

fn render_status(state: &DemoState, area: Rect, buf: &mut Buffer) {
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
        .center([StatusSection::new(&state.status).style(status_style)])
        .right([
            StatusSection::new(format!("msgs: {}", state.messages.len())),
            StatusSection::new(format!("tick: {}", state.tick)),
        ]);

    Widget::render(&bar, area, buf);
}

fn render_listen_overlay(state: &DemoState, area: Rect, buf: &mut Buffer) {
    if !state.listening && state.listen_log.is_empty() {
        return;
    }

    let max_width = area.width.min(48);
    let min_width = 24.min(area.width);
    let width = max_width.max(min_width);

    let content_lines = state.listen_log.len().max(1);
    let max_height = area.height.min(8);
    let height = ((content_lines as u16) + 2).min(max_height);

    let x = area
        .right()
        .saturating_sub(width)
        .saturating_sub(1)
        .max(area.x);
    let y = area.bottom().saturating_sub(height).max(area.y);
    let overlay_area = Rect::new(x, y, width, height);

    let title = if state.listening {
        "Listening (mock)"
    } else {
        "Recent audio"
    };

    let block = Block::bordered()
        .title(title)
        .border_style(Style::new().fg(Color::Cyan));
    let inner = block.inner(overlay_area);
    Widget::render(&block, overlay_area, buf);

    if inner.is_empty() {
        return;
    }

    let available_lines = inner.height as usize;
    let start = state.listen_log.len().saturating_sub(available_lines);
    for (i, line) in state.listen_log[start..].iter().enumerate() {
        let y_pos = inner.y + i as u16;
        if y_pos >= inner.bottom() {
            break;
        }
        let truncated = truncate(line, inner.width as usize);
        buf.set_string(inner.x, y_pos, &truncated, Style::new().fg(Color::White));
    }
}

fn render_process_overlay(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let running = state
        .processes
        .iter()
        .filter(|p| p.status == ProcessStatus::Running)
        .count();
    let overlay = Overlay::new()
        .title(format!("Agent Processes ({} running)", running))
        .border_color(Color::Cyan)
        .bg_color(Color::Rgb(15, 15, 25))
        .help_text("Enter: logs │ s: stop/cont │ x: kill │ d: remove │ Esc: close")
        .dimensions(0.75, 0.65);

    let inner = overlay.render_frame(area, buf);
    let inner_x = inner.x;
    let inner_width = inner.width as usize;
    let mut y = inner.y;

    // Header row
    buf.set_string(inner_x, y, "PID", Style::new().fg(Color::DarkGrey).bold());
    buf.set_string(
        inner_x + 8,
        y,
        "AGENT",
        Style::new().fg(Color::DarkGrey).bold(),
    );
    buf.set_string(
        inner_x + 22,
        y,
        "COMMAND",
        Style::new().fg(Color::DarkGrey).bold(),
    );
    buf.set_string(
        inner.right() - 8,
        y,
        "TIME",
        Style::new().fg(Color::DarkGrey).bold(),
    );
    y += 1;

    // Separator
    for x in inner_x..inner.right() {
        buf.set_string(x, y, "─", Style::new().fg(Color::Rgb(40, 40, 50)));
    }
    y += 1;

    if state.processes.is_empty() {
        buf.set_string(
            inner_x,
            y,
            "No agent processes running",
            Style::new().fg(Color::DarkGrey),
        );
    } else {
        for (i, proc) in state.processes.iter().enumerate() {
            if y >= inner.bottom() {
                break;
            }
            let is_selected = i == state.selected_process;
            let bg = if is_selected {
                Color::Rgb(35, 35, 55)
            } else {
                Color::Rgb(15, 15, 25)
            };

            for x in inner_x..inner.right() {
                if let Some(cell) = buf.get_mut(x, y) {
                    cell.bg = bg;
                    cell.symbol = " ".to_string();
                }
            }

            let (icon, icon_color) = proc.status_icon();
            buf.set_string(inner_x, y, icon, Style::new().fg(icon_color));
            buf.set_string(
                inner_x + 2,
                y,
                &format!("{}", proc.pid),
                if is_selected {
                    Style::new().fg(Color::White).bold()
                } else {
                    Style::new().fg(Color::White)
                },
            );
            buf.set_string(
                inner_x + 8,
                y,
                &proc.agent.chars().take(12).collect::<String>(),
                Style::new().fg(Color::Magenta),
            );
            buf.set_string(
                inner_x + 22,
                y,
                &truncate(&proc.command, inner_width.saturating_sub(35)),
                if is_selected {
                    Style::new().fg(Color::Cyan)
                } else {
                    Style::new().fg(Color::White)
                },
            );
            let elapsed = proc.elapsed_display();
            buf.set_string(
                inner.right() - elapsed.len() as u16,
                y,
                &elapsed,
                Style::new().fg(Color::DarkGrey),
            );
            y += 1;

            if is_selected && !proc.output_lines.is_empty() {
                for x in inner_x..inner.right() {
                    if let Some(cell) = buf.get_mut(x, y) {
                        cell.bg = Color::Rgb(25, 25, 35);
                        cell.symbol = " ".to_string();
                    }
                }
                let last_line = proc.output_lines.last().unwrap();
                buf.set_string(
                    inner_x + 2,
                    y,
                    &format!(
                        "└─ {}",
                        &last_line.chars().take(inner_width - 4).collect::<String>()
                    ),
                    Style::new().fg(Color::DarkGrey),
                );
                y += 1;
            }
        }
    }
}

fn render_history_overlay(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let overlay = Overlay::new()
        .title("Session History")
        .border_color(Color::Magenta)
        .help_text("Enter: switch session | Esc: close")
        .dimensions(0.6, 0.6);

    let inner = overlay.render_frame(area, buf);
    let inner_x = inner.x;
    let inner_width = inner.width as usize;
    let mut y = inner.y;

    for (i, session) in state.sessions.iter().enumerate() {
        if y >= inner.bottom() {
            break;
        }
        let is_selected = i == state.selected_session;
        let is_current = i == state.current_session;
        let bg = if is_selected {
            Color::Rgb(40, 40, 60)
        } else {
            Color::Rgb(20, 20, 30)
        };

        for x in inner_x..inner.right() {
            if let Some(cell) = buf.get_mut(x, y) {
                cell.bg = bg;
                cell.symbol = " ".to_string();
            }
        }

        buf.set_string(
            inner_x,
            y,
            if is_current { "●" } else { "○" },
            if is_current {
                Style::new().fg(Color::Green)
            } else {
                Style::new().fg(Color::DarkGrey)
            },
        );
        buf.set_string(
            inner_x + 2,
            y,
            &session.title,
            if is_selected {
                Style::new().fg(Color::White).bold()
            } else if is_current {
                Style::new().fg(Color::Green)
            } else {
                Style::new().fg(Color::White)
            },
        );
        buf.set_string(
            inner.right() - session.timestamp.len() as u16,
            y,
            &session.timestamp,
            Style::new().fg(Color::DarkGrey),
        );
        y += 1;

        if y < inner.bottom() {
            for x in inner_x..inner.right() {
                if let Some(cell) = buf.get_mut(x, y) {
                    cell.bg = bg;
                    cell.symbol = " ".to_string();
                }
            }
            buf.set_string(
                inner_x + 2,
                y,
                &session
                    .preview
                    .chars()
                    .take(inner_width - 15)
                    .collect::<String>(),
                Style::new().fg(Color::DarkGrey),
            );
            let msg_count = if i == 0 {
                format!("{} msgs", state.messages.len())
            } else {
                format!("{} msgs", session.message_count)
            };
            buf.set_string(
                inner.right() - msg_count.len() as u16,
                y,
                &msg_count,
                Style::new().fg(Color::DarkGrey),
            );
            y += 2;
        }
    }
}

fn render_log_overlay(state: &DemoState, area: Rect, buf: &mut Buffer) {
    let proc_idx = match state.viewing_logs {
        Some(idx) => idx,
        None => return,
    };
    let proc = match state.processes.get(proc_idx) {
        Some(p) => p,
        None => return,
    };

    let (status_icon, status_color) = proc.status_icon();
    let overlay = Overlay::new()
        .title(format!(
            "{} PID {} │ {} │ {}",
            status_icon,
            proc.pid,
            proc.agent,
            truncate(&proc.command, 40)
        ))
        .border_color(Color::Green)
        .bg_color(Color::Rgb(10, 10, 15))
        .help_text("↑/k: up │ ↓/j: down │ g/G: top/bottom │ Esc/q: close")
        .dimensions(0.85, 0.80);

    let inner = overlay.render_frame(area, buf);
    let inner_x = inner.x;
    let inner_width = inner.width as usize;
    let inner_height = inner.height as usize;

    let total_lines = proc.output_lines.len();
    let visible_lines = inner_height;
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll = state.log_scroll.min(max_scroll);
    let start = total_lines.saturating_sub(visible_lines + scroll);
    let end = (start + visible_lines).min(total_lines);

    for (i, line_idx) in (start..end).enumerate() {
        let render_y = inner.y + i as u16;
        if render_y >= inner.bottom() {
            break;
        }
        let log_line = &proc.output_lines[line_idx];
        let style = if log_line.contains("error") || log_line.contains("Error") {
            Style::new().fg(Color::Red)
        } else if log_line.contains("warn") || log_line.contains("WARN") {
            Style::new().fg(Color::Yellow)
        } else if log_line.contains("✓") || log_line.contains("ok") || log_line.contains("Ready")
        {
            Style::new().fg(Color::Green)
        } else if log_line.starts_with("   ") {
            Style::new().fg(Color::DarkGrey)
        } else {
            Style::new().fg(Color::White)
        };
        buf.set_string(
            inner_x,
            render_y,
            &format!("{:>4} ", line_idx + 1),
            Style::new().fg(Color::Rgb(60, 60, 70)),
        );
        buf.set_string(
            inner_x + 5,
            render_y,
            &log_line
                .chars()
                .take(inner_width.saturating_sub(6))
                .collect::<String>(),
            style,
        );
    }

    if total_lines > visible_lines {
        let scrollbar_height = inner_height.saturating_sub(2) as u16;
        let thumb_pos = if scrollbar_height > 0 {
            ((start as u32 * scrollbar_height as u32) / total_lines.max(1) as u32) as u16
        } else {
            0
        };
        for i in 0..scrollbar_height {
            buf.set_string(
                inner.right() - 1,
                inner.y + i,
                if i == thumb_pos { "█" } else { "░" },
                Style::new().fg(Color::Rgb(60, 60, 70)),
            );
        }
    }

    let status_text = match proc.status {
        ProcessStatus::Running => "● LIVE",
        ProcessStatus::Stopped => "◉ PAUSED",
        ProcessStatus::Completed => "✓ DONE",
        ProcessStatus::Failed => "✗ FAILED",
    };
    let overlay_area = overlay.area(area);
    buf.set_string(
        overlay_area.right() - 3 - status_text.len() as u16,
        overlay_area.bottom() - 2,
        status_text,
        Style::new().fg(status_color),
    );
}
