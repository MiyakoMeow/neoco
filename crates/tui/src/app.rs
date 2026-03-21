//! Application main loop and state management.
//!
//! This module provides the [`App`] struct that manages the application state
//! and event loop for the TUI.

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use neoco_agent::EventHandler;
use neoco_event::{ChatEvent, EventBus, UIEvent, UnifiedEvent};
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use tokio_stream::StreamExt;

use crate::event::EventResult;
use crate::state::AppState;
use crate::tui::Tui;
use crate::tui::TuiEvent;
use crate::widget::ChatPane;
use crate::widget::HistoryCell;
use crate::widget::InputPane;
use crate::widget::InputResult;
use crate::widget::TuiWidget;

// Re-export ChatMessage from neoco-event
use neoco_event::ChatMessage;

/// Event handler that sends events to the app.
struct AppEventHandler {
    event_bus: EventBus,
}

impl EventHandler for AppEventHandler {
    fn handle(&self, event: ChatEvent) {
        let _ = self.event_bus.send(UnifiedEvent::Chat(event));
    }
}

/// The main application state and event loop.
pub struct App {
    /// Centralized application state.
    state: AppState,
    /// The chat display pane.
    chat_pane: ChatPane,
    /// The input pane.
    input_pane: InputPane,
    /// Unified event bus.
    event_bus: EventBus,
    /// Event bus receiver for processing events.
    event_rx: tokio::sync::broadcast::Receiver<UnifiedEvent>,
}

impl App {
    /// Create a new `App`.
    #[must_use]
    pub fn new() -> Self {
        let event_bus = EventBus::default();
        let event_rx = event_bus.subscribe();
        Self {
            state: AppState::new(),
            chat_pane: ChatPane::new(),
            input_pane: InputPane::new(),
            event_bus,
            event_rx,
        }
    }

    /// Get an event handler that can be used to send events to this app.
    #[must_use]
    pub fn event_handler(&self) -> impl EventHandler {
        AppEventHandler {
            event_bus: self.event_bus.clone(),
        }
    }

    /// Get a clone of the event bus.
    #[must_use]
    pub fn event_bus(&self) -> EventBus {
        self.event_bus.clone()
    }

    /// Get a reference to the application state.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Run the application.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal I/O fails.
    pub async fn run(&mut self, tui: &mut Tui) -> std::io::Result<()> {
        let mut event_stream = tui.event_stream();

        loop {
            // Draw the UI
            self.draw(tui)?;

            // Handle events
            tokio::select! {
                Some(event) = event_stream.next() => {
                    if self.handle_terminal_event(event) == EventResult::Exit {
                        break;
                    }
                    tui.request_draw();
                }
            }

            // Handle app events (non-blocking)
            self.process_app_events();

            if self.state.should_exit {
                break;
            }
        }

        Ok(())
    }

    /// Draw the UI.
    fn draw(&mut self, tui: &mut Tui) -> std::io::Result<()> {
        tui.terminal.draw(|frame| {
            let area = frame.area();

            // Update state with terminal size
            self.state.resize(area.width, area.height);

            // Layout: chat area on top, input area at bottom
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),    // Chat area
                    Constraint::Length(1), // Input area (single line)
                ])
                .split(area);

            // Render chat pane
            if let Some(chat_area) = chunks.first() {
                self.chat_pane.render(*chat_area, frame.buffer_mut());
            }

            // Render input pane
            if let Some(input_area) = chunks.get(1) {
                self.input_pane.render(*input_area, frame.buffer_mut());
            }
        })?;

        Ok(())
    }

    /// Handle a terminal event. Returns whether the app should exit.
    fn handle_terminal_event(&mut self, event: TuiEvent) -> EventResult {
        match event {
            TuiEvent::Key(key) => self.handle_key_event(key),
            TuiEvent::Paste(text) => {
                self.input_pane.handle_paste(&text);
                EventResult::Handled
            },
            TuiEvent::Draw => EventResult::Handled,
        }
    }

    /// Handle a key event. Returns whether the app should exit.
    fn handle_key_event(&mut self, key: KeyEvent) -> EventResult {
        // Global shortcuts
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.state.should_exit = true;
            return EventResult::Exit;
        }

        // Page Up/Down for scrolling
        match key.code {
            KeyCode::PageUp => {
                self.chat_pane.scroll_up(10);
                return EventResult::Handled;
            },
            KeyCode::PageDown => {
                self.chat_pane.scroll_down(10);
                return EventResult::Handled;
            },
            _ => {},
        }

        // Pass to input pane
        match self.input_pane.handle_key_event(key) {
            InputResult::None => EventResult::Handled,
            InputResult::Submit(text) => self.handle_submit(&text),
        }
    }

    /// Handle message submission.
    fn handle_submit(&mut self, text: &str) -> EventResult {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            self.chat_pane
                .push(HistoryCell::UserMessage(trimmed.to_string()));
            let _ = self
                .event_bus
                .send(UnifiedEvent::UI(UIEvent::SendMessage(trimmed.to_string())));
            self.state.clear_input();
        }
        EventResult::Handled
    }

    /// Process all pending app events.
    fn process_app_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_unified_event(event);
        }
    }

    /// Handle a unified event from the event bus.
    fn handle_unified_event(&mut self, event: UnifiedEvent) {
        match event {
            UnifiedEvent::Chat(chat_event) => self.handle_chat_event(chat_event),
            UnifiedEvent::UI(ref ui_event) => self.handle_ui_event(ui_event),
            UnifiedEvent::Terminal(_) => {
                // Terminal events are handled by the event stream
            },
        }
    }

    /// Handle a UI event.
    fn handle_ui_event(&mut self, event: &UIEvent) {
        match event {
            UIEvent::SendMessage(_) => {
                // SendMessage is handled by the caller
            },
            UIEvent::Exit => {
                self.state.should_exit = true;
            },
            UIEvent::ScrollUp(lines) => {
                self.chat_pane.scroll_up(usize::from(*lines));
            },
            UIEvent::ScrollDown(lines) => {
                self.chat_pane.scroll_down(usize::from(*lines));
            },
        }
    }

    /// Handle a chat event.
    fn handle_chat_event(&mut self, event: ChatEvent) {
        match event {
            ChatEvent::Text(text) => {
                self.chat_pane.push(HistoryCell::AssistantMessage(text));
            },
            ChatEvent::Reasoning(content) | ChatEvent::ReasoningDelta(content) => {
                self.chat_pane.push(HistoryCell::Reasoning(content));
            },
            ChatEvent::ToolCall { arguments } => {
                let message = ChatMessage::parse_tool_call(&arguments);
                let command = match &message {
                    ChatMessage::ToolCall { command } => command.clone(),
                    _ => arguments.clone(),
                };
                self.chat_pane.push(HistoryCell::ToolCall { command });
            },
            ChatEvent::ToolResult {
                content,
                structured,
            } => {
                let message = ChatMessage::parse_tool_result(&content, &structured);
                match &message {
                    ChatMessage::ToolResult {
                        stdout,
                        stderr,
                        exit_code,
                    } => {
                        self.chat_pane.push(HistoryCell::ToolResult {
                            stdout: stdout.clone(),
                            stderr: stderr.clone(),
                            exit_code: *exit_code,
                        });
                    },
                    ChatMessage::Assistant(text) => {
                        self.chat_pane
                            .push(HistoryCell::AssistantMessage(text.clone()));
                    },
                    _ => {},
                }
            },
            _ => {},
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new_creates_empty_state() {
        let app = App::new();
        assert_eq!(app.state().scroll_offset, 0);
    }

    #[test]
    fn test_app_event_handler() {
        let app = App::new();
        let handler = app.event_handler();
        drop(handler);
    }

    #[test]
    fn test_handle_submit_adds_message() {
        let mut app = App::new();
        let result = app.handle_submit("hello");
        assert_eq!(result, EventResult::Handled);
        assert_eq!(app.chat_pane.message_count(), 1);
    }

    #[test]
    fn test_handle_submit_ignores_empty() {
        let mut app = App::new();
        let result = app.handle_submit("   ");
        assert_eq!(result, EventResult::Handled);
        assert_eq!(app.chat_pane.message_count(), 0);
    }

    #[test]
    fn test_handle_chat_text_event() {
        let mut app = App::new();
        app.handle_chat_event(ChatEvent::Text("response".to_string()));
        assert_eq!(app.chat_pane.message_count(), 1);
    }

    #[test]
    fn test_handle_chat_reasoning_event() {
        let mut app = App::new();
        app.handle_chat_event(ChatEvent::Reasoning("thinking".to_string()));
        assert_eq!(app.chat_pane.message_count(), 1);
    }

    #[test]
    fn test_key_ctrl_c_exits() {
        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = app.handle_key_event(key);
        assert_eq!(result, EventResult::Exit);
        assert!(app.state().should_exit);
    }

    #[test]
    fn test_key_page_up_scrolls() {
        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        let result = app.handle_key_event(key);
        assert_eq!(result, EventResult::Handled);
    }

    #[test]
    fn test_key_page_down_scrolls() {
        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        let result = app.handle_key_event(key);
        assert_eq!(result, EventResult::Handled);
    }
}
