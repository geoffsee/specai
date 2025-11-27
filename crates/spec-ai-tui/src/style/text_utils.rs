//! Text utility functions

use unicode_width::UnicodeWidthStr;

/// Truncate a string to fit within max_width, adding ellipsis if needed
pub fn truncate(s: &str, max_width: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        ".".repeat(max_width)
    } else {
        let mut result = String::new();
        let mut current_width = 0;
        for c in s.chars() {
            let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            if current_width + char_width + 3 > max_width {
                break;
            }
            result.push(c);
            current_width += char_width;
        }
        result.push_str("...");
        result
    }
}

/// Word-wrap text to fit within max_width
///
/// Returns a vector of lines. Continuation lines are prefixed with the given prefix.
pub fn wrap_text(text: &str, max_width: usize, prefix: &str) -> Vec<String> {
    if max_width == 0 {
        return vec![];
    }

    let prefix_width = UnicodeWidthStr::width(prefix);
    let continuation_width = max_width.saturating_sub(prefix_width);

    if continuation_width == 0 {
        return vec![text.chars().take(max_width).collect()];
    }

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0usize;
    let mut is_first_line = true;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        let line_max = if is_first_line {
            max_width
        } else {
            continuation_width
        };

        if current_width == 0 {
            if word_width <= line_max {
                current_line.push_str(word);
                current_width = word_width;
            } else {
                // Word too long, break it
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
                    current_width = UnicodeWidthStr::width(current_line.as_str());
                }
            }
        } else if current_width + 1 + word_width <= line_max {
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Wrap to new line
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_very_short_max() {
        assert_eq!(truncate("hello", 2), "..");
    }

    #[test]
    fn test_wrap_simple() {
        let lines = wrap_text("hello world", 20, "  ");
        assert_eq!(lines, vec!["hello world"]);
    }

    #[test]
    fn test_wrap_multiple_lines() {
        let lines = wrap_text("hello world test", 8, "  ");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "hello");
        assert_eq!(lines[1], "  world");
        assert_eq!(lines[2], "  test");
    }
}
