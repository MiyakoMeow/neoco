//! Input pane widget for user message entry.
//!
//! This module provides the [`InputPane`] widget for handling user text input
//! with cursor management and key event handling.

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget as RatatuiWidget;
use ratatui::widgets::Wrap;

use crate::widget::TuiWidget;

/// Result of handling a key event in the input pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputResult {
    /// No action needed.
    None,
    /// A message should be sent.
    Submit(String),
}

/// The input pane for entering messages.
#[derive(Debug)]
pub struct InputPane {
    /// The current input text.
    text: String,
    /// Cursor position (byte index).
    cursor: usize,
}

impl InputPane {
    /// Create a new `InputPane`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    /// Handle a key event.
    #[must_use]
    pub fn handle_key_event(&mut self, key: KeyEvent) -> InputResult {
        match key.code {
            KeyCode::Enter => self.handle_enter(key.modifiers),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+C is handled at the app level
                InputResult::None
            },
            KeyCode::Char(c) => {
                self.insert_char(c);
                InputResult::None
            },
            KeyCode::Backspace => {
                self.delete_backward();
                InputResult::None
            },
            KeyCode::Delete => {
                self.delete_forward();
                InputResult::None
            },
            KeyCode::Left => {
                self.move_cursor_left();
                InputResult::None
            },
            KeyCode::Right => {
                self.move_cursor_right();
                InputResult::None
            },
            KeyCode::Home => {
                self.cursor = 0;
                InputResult::None
            },
            KeyCode::End => {
                self.cursor = self.text.len();
                InputResult::None
            },
            _ => InputResult::None,
        }
    }

    /// Handle Enter key (submit or newline).
    fn handle_enter(&mut self, modifiers: KeyModifiers) -> InputResult {
        if modifiers.contains(KeyModifiers::SHIFT) {
            // Shift+Enter: insert newline
            self.insert_char('\n');
            InputResult::None
        } else {
            // Enter: submit
            let text = std::mem::take(&mut self.text);
            self.cursor = 0;
            InputResult::Submit(text)
        }
    }

    /// Handle pasted text.
    pub fn handle_paste(&mut self, text: &str) {
        // Normalize line endings
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        for c in normalized.chars() {
            self.insert_char(c);
        }
    }

    /// Insert a character at the cursor position.
    fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the character before the cursor.
    fn delete_backward(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            self.cursor -= prev;
            self.text.remove(self.cursor);
        }
    }

    /// Delete the character after the cursor.
    fn delete_forward(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    /// Move cursor left by one character.
    fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            self.cursor -= prev;
        }
    }

    /// Move cursor right by one character.
    fn move_cursor_right(&mut self) {
        if self.cursor < self.text.len() {
            let next = self.text[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            self.cursor += next;
        }
    }

    /// Get the current text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the cursor position.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }
}

impl TuiWidget for InputPane {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let prompt = "> ";
        let display_text = format!("{}{}█", prompt, self.text);

        let paragraph = Paragraph::new(display_text)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });

        paragraph.render(area, buf);
    }
}

impl Default for InputPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_event_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_new_input_pane_is_empty() {
        let pane = InputPane::new();
        assert!(pane.text().is_empty());
        assert_eq!(pane.cursor(), 0);
    }

    #[test]
    fn test_insert_char() {
        let mut pane = InputPane::new();
        let result = pane.handle_key_event(key_event(KeyCode::Char('a')));
        assert_eq!(result, InputResult::None);
        assert_eq!(pane.text(), "a");
        assert_eq!(pane.cursor(), 1);
    }

    #[test]
    fn test_insert_multiple_chars() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('h')));
        let _ = pane.handle_key_event(key_event(KeyCode::Char('i')));
        assert_eq!(pane.text(), "hi");
        assert_eq!(pane.cursor(), 2);
    }

    #[test]
    fn test_submit_on_enter() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('h')));
        let _ = pane.handle_key_event(key_event(KeyCode::Char('i')));
        let result = pane.handle_key_event(key_event(KeyCode::Enter));
        assert_eq!(result, InputResult::Submit("hi".to_string()));
        assert!(pane.text().is_empty());
        assert_eq!(pane.cursor(), 0);
    }

    #[test]
    fn test_shift_enter_inserts_newline() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('a')));
        let result = pane.handle_key_event(key_event_with_mod(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(result, InputResult::None);
        assert_eq!(pane.text(), "a\n");
    }

    #[test]
    fn test_backspace_deletes_char() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('a')));
        let _ = pane.handle_key_event(key_event(KeyCode::Char('b')));
        let _ = pane.handle_key_event(key_event(KeyCode::Backspace));
        assert_eq!(pane.text(), "a");
        assert_eq!(pane.cursor(), 1);
    }

    #[test]
    fn test_backspace_at_start_is_noop() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Backspace));
        assert!(pane.text().is_empty());
    }

    #[test]
    fn test_delete_forward() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('a')));
        let _ = pane.handle_key_event(key_event(KeyCode::Char('b')));
        pane.cursor = 0;
        let _ = pane.handle_key_event(key_event(KeyCode::Delete));
        assert_eq!(pane.text(), "b");
    }

    #[test]
    fn test_cursor_movement() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('a')));
        let _ = pane.handle_key_event(key_event(KeyCode::Char('b')));
        let _ = pane.handle_key_event(key_event(KeyCode::Left));
        assert_eq!(pane.cursor(), 1);
        let _ = pane.handle_key_event(key_event(KeyCode::Right));
        assert_eq!(pane.cursor(), 2);
    }

    #[test]
    fn test_home_end_keys() {
        let mut pane = InputPane::new();
        let _ = pane.handle_key_event(key_event(KeyCode::Char('a')));
        let _ = pane.handle_key_event(key_event(KeyCode::Char('b')));
        let _ = pane.handle_key_event(key_event(KeyCode::Home));
        assert_eq!(pane.cursor(), 0);
        let _ = pane.handle_key_event(key_event(KeyCode::End));
        assert_eq!(pane.cursor(), 2);
    }

    #[test]
    fn test_paste_text() {
        let mut pane = InputPane::new();
        pane.handle_paste("hello\nworld");
        assert_eq!(pane.text(), "hello\nworld");
    }

    #[test]
    fn test_paste_normalizes_line_endings() {
        let mut pane = InputPane::new();
        pane.handle_paste("line1\r\nline2\rline3");
        assert_eq!(pane.text(), "line1\nline2\nline3");
    }

    #[test]
    fn test_ctrl_c_returns_none() {
        let mut pane = InputPane::new();
        let result = pane.handle_key_event(key_event_with_mod(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(result, InputResult::None);
    }
}
