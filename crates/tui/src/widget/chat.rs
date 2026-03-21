//! Chat display pane widget.
//!
//! This module provides the [`ChatPane`] widget for displaying chat history
//! with support for syntax highlighting in code blocks.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget as RatatuiWidget;
use ratatui::widgets::Wrap;

use crate::widget::TuiWidget;
use crate::widget::highlight::CodeHighlighter;

/// A single cell in the chat history for rendering.
#[derive(Debug, Clone)]
pub enum HistoryCell {
    /// A message from the user.
    UserMessage(String),
    /// A message from the assistant.
    AssistantMessage(String),
    /// Reasoning content from the assistant.
    Reasoning(String),
    /// A tool call with command.
    ToolCall {
        /// The command being called.
        command: String,
    },
    /// A tool execution result.
    ToolResult {
        /// Standard output.
        stdout: String,
        /// Standard error.
        stderr: String,
        /// Exit code.
        exit_code: i64,
    },
}

impl HistoryCell {
    /// Convert this cell to lines for rendering.
    fn to_lines(&self, highlighter: &CodeHighlighter) -> Vec<Line<'static>> {
        match self {
            HistoryCell::UserMessage(text) => {
                vec![
                    Line::from(vec![
                        Span::styled(
                            "[User] ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(text.clone()),
                    ]),
                    Line::raw(""),
                ]
            },
            HistoryCell::AssistantMessage(text) => render_message_with_code(text, highlighter),
            HistoryCell::Reasoning(text) => {
                vec![
                    Line::from(vec![
                        Span::styled("[Thinking] ", Style::default().fg(Color::Yellow)),
                        Span::raw(text.clone()),
                    ]),
                    Line::raw(""),
                ]
            },
            HistoryCell::ToolCall { command } => {
                vec![
                    Line::from(vec![
                        Span::styled("[Bash] ", Style::default().fg(Color::Magenta)),
                        Span::raw(command.clone()),
                    ]),
                    Line::raw(""),
                ]
            },
            HistoryCell::ToolResult {
                stdout,
                stderr,
                exit_code,
            } => render_tool_result(stdout, stderr, *exit_code),
        }
    }
}

/// Render a message that may contain code blocks with syntax highlighting.
fn render_message_with_code(text: &str, highlighter: &CodeHighlighter) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_language: Option<String> = None;
    let mut code_buffer = String::new();

    for line in text.lines() {
        if line.starts_with("```") {
            if in_code_block {
                // End of code block - highlight and render
                let lang = code_language.as_deref();
                let styled_lines = highlighter.highlight(&code_buffer, lang);
                lines.extend(styled_lines);
                code_buffer.clear();
                code_language = None;
                in_code_block = false;
            } else {
                // Start of code block
                in_code_block = true;
                let lang = line.strip_prefix("```").map(str::to_string);
                code_language = lang.filter(|l| !l.is_empty());
            }
        } else if in_code_block {
            code_buffer.push_str(line);
            code_buffer.push('\n');
        } else {
            lines.push(Line::from(vec![Span::raw(line.to_string())]));
        }
    }

    // Handle unclosed code block
    if in_code_block && !code_buffer.is_empty() {
        let lang = code_language.as_deref();
        let styled_lines = highlighter.highlight(&code_buffer, lang);
        lines.extend(styled_lines);
    }

    lines.push(Line::raw(""));
    lines
}

/// Render tool result with stdout, stderr, and exit code.
fn render_tool_result(stdout: &str, stderr: &str, exit_code: i64) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if !stdout.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "Output:",
            Style::default().fg(Color::Green),
        )]));
        for line in stdout.lines().take(5) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(line.to_string()),
            ]));
        }
        let stdout_line_count = stdout.lines().count();
        if stdout_line_count > 5 {
            lines.push(Line::from(vec![Span::raw(format!(
                "  ... ({} more lines)",
                stdout_line_count - 5
            ))]));
        }
    }

    if !stderr.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "Error:",
            Style::default().fg(Color::Red),
        )]));
        for line in stderr.lines().take(5) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(line.to_string()),
            ]));
        }
        let stderr_line_count = stderr.lines().count();
        if stderr_line_count > 5 {
            lines.push(Line::from(vec![Span::raw(format!(
                "  ... ({} more lines)",
                stderr_line_count - 5
            ))]));
        }
    }

    if exit_code != 0 {
        lines.push(Line::from(vec![Span::styled(
            format!("Exit code: {exit_code}"),
            Style::default().fg(Color::Red),
        )]));
    }

    lines.push(Line::raw(""));
    lines
}

/// The chat display pane.
#[derive(Debug)]
pub struct ChatPane {
    /// The history of messages.
    history: Vec<HistoryCell>,
    /// Vertical scroll offset (lines from bottom).
    scroll_offset: usize,
    /// Code highlighter for syntax highlighting.
    highlighter: CodeHighlighter,
}

impl ChatPane {
    /// Create a new `ChatPane`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            scroll_offset: 0,
            highlighter: CodeHighlighter::new(),
        }
    }

    /// Add a history cell to the chat.
    pub fn push(&mut self, cell: HistoryCell) {
        self.history.push(cell);
        // Auto-scroll to bottom when new content is added
        self.scroll_offset = 0;
    }

    /// Scroll up by the specified number of lines.
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    /// Scroll down by the specified number of lines.
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Get the number of messages in the history.
    #[must_use]
    pub fn message_count(&self) -> usize {
        self.history.len()
    }

    /// Get all lines for rendering.
    fn all_lines(&self) -> Vec<Line<'static>> {
        self.history
            .iter()
            .flat_map(|cell| cell.to_lines(&self.highlighter))
            .collect()
    }

    /// Calculate the visible line range for rendering.
    fn visible_range(&self, area_height: u16) -> (usize, usize) {
        let all_lines = self.all_lines();
        let total_lines = all_lines.len();
        let visible_lines = usize::from(area_height);

        if total_lines == 0 {
            return (0, 0);
        }

        let scroll_offset = self.scroll_offset.min(total_lines.saturating_sub(1));
        let end = total_lines.saturating_sub(scroll_offset);
        let start = end.saturating_sub(visible_lines.min(end));

        (start, end)
    }
}

impl TuiWidget for ChatPane {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let all_lines = self.all_lines();

        if all_lines.is_empty() {
            return;
        }

        let (start, end) = self.visible_range(area.height);

        let display_lines: Vec<Line<'static>> = all_lines
            .get(start..end)
            .map(<[Line<'static>]>::to_vec)
            .unwrap_or_default();

        let paragraph = Paragraph::new(display_lines).wrap(Wrap { trim: false });
        paragraph.render(area, buf);
    }
}

impl Default for ChatPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_pane_new_is_empty() {
        let pane = ChatPane::new();
        assert_eq!(pane.message_count(), 0);
    }

    #[test]
    fn test_chat_pane_push_increments_count() {
        let mut pane = ChatPane::new();
        pane.push(HistoryCell::UserMessage("hello".to_string()));
        assert_eq!(pane.message_count(), 1);
    }

    #[test]
    fn test_chat_pane_push_clears_scroll() {
        let mut pane = ChatPane::new();
        pane.scroll_offset = 10;
        pane.push(HistoryCell::UserMessage("hello".to_string()));
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn test_chat_pane_scroll_up() {
        let mut pane = ChatPane::new();
        pane.scroll_up(5);
        assert_eq!(pane.scroll_offset, 5);
        pane.scroll_up(3);
        assert_eq!(pane.scroll_offset, 8);
    }

    #[test]
    fn test_chat_pane_scroll_down_saturates() {
        let mut pane = ChatPane::new();
        pane.scroll_offset = 5;
        pane.scroll_down(10);
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn test_user_message_rendering() {
        let cell = HistoryCell::UserMessage("hello".to_string());
        let highlighter = CodeHighlighter::new();
        let lines = cell.to_lines(&highlighter);
        assert_eq!(lines.len(), 2); // message + empty line
    }

    #[test]
    fn test_tool_result_rendering() {
        let cell = HistoryCell::ToolResult {
            stdout: "output".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let highlighter = CodeHighlighter::new();
        let lines = cell.to_lines(&highlighter);
        assert!(lines.len() >= 2); // header + content + empty line
    }

    #[test]
    fn test_tool_result_with_error() {
        let cell = HistoryCell::ToolResult {
            stdout: String::new(),
            stderr: "error".to_string(),
            exit_code: 1,
        };
        let highlighter = CodeHighlighter::new();
        let lines = cell.to_lines(&highlighter);
        // Should have: "Error:" + error line + "Exit code: 1" + empty line
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_code_block_highlighting() {
        let mut pane = ChatPane::new();
        pane.push(HistoryCell::AssistantMessage(
            "Here is code:\n```rust\nfn main() {}\n```".to_string(),
        ));
        let lines = pane.all_lines();
        assert!(!lines.is_empty());
    }
}
