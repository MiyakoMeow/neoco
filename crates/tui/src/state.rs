//! Centralized application state management.
//!
//! This module provides the [`AppState`] struct that holds UI-only state,
//! following the "Agent drives TUI" architecture where business state
//! (chat history) is managed by the Agent and received via events.

/// Centralized UI-only application state.
///
/// This struct manages only UI-related state (input, scrolling, terminal size),
/// while business state (chat history) is received from the Agent via events.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current input text.
    pub input_text: String,
    /// Cursor position in input (byte index).
    pub input_cursor: usize,
    /// Scroll offset for chat pane (lines from bottom).
    pub scroll_offset: usize,
    /// Whether the application should exit.
    pub should_exit: bool,
    /// Terminal width.
    pub terminal_width: u16,
    /// Terminal height.
    pub terminal_height: u16,
}

impl AppState {
    /// Create a new application state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            input_text: String::new(),
            input_cursor: 0,
            scroll_offset: 0,
            should_exit: false,
            terminal_width: 80,
            terminal_height: 24,
        }
    }

    /// Update terminal size.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    /// Clear the input field.
    pub fn clear_input(&mut self) {
        self.input_text.clear();
        self.input_cursor = 0;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert_eq!(state.input_text, "");
        assert_eq!(state.input_cursor, 0);
        assert_eq!(state.scroll_offset, 0);
        assert!(!state.should_exit);
        assert_eq!(state.terminal_width, 80);
        assert_eq!(state.terminal_height, 24);
    }

    #[test]
    fn test_resize() {
        let mut state = AppState::new();
        state.resize(100, 50);
        assert_eq!(state.terminal_width, 100);
        assert_eq!(state.terminal_height, 50);
    }

    #[test]
    fn test_clear_input() {
        let mut state = AppState::new();
        state.input_text = "test".to_string();
        state.input_cursor = 4;
        state.clear_input();
        assert_eq!(state.input_text, "");
        assert_eq!(state.input_cursor, 0);
    }
}
