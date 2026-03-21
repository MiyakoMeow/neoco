//! Application-level events with two-layer architecture.
//!
//! This module defines the event system for the TUI application:
//! - **Terminal Events**: Raw terminal events from crossterm (keyboard, resize, paste)
//! - **App Events**: Business logic events (messages, tool calls, state changes)

use crossterm::event::KeyEvent;
use neoco_event::ChatEvent;

/// Raw terminal events from crossterm.
///
/// These events represent low-level terminal interactions that need to be
/// translated into application-level events.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// A key was pressed.
    Key(KeyEvent),
    /// The terminal was resized.
    Resize(u16, u16),
    /// Text was pasted from clipboard.
    Paste(String),
}

/// Application-level events for business logic.
///
/// These events represent high-level application actions and state changes.
#[derive(Debug)]
pub enum AppEvent {
    /// A chat event from the agent.
    ChatEvent(ChatEvent),
    /// User submitted a message to send.
    SendMessage(String),
    /// Request to scroll the chat pane up.
    ScrollUp(u16),
    /// Request to scroll the chat pane down.
    ScrollDown(u16),
    /// Request to exit the application.
    Exit,
}

/// Result of processing an event by a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    /// The event was handled.
    Handled,
    /// The event was not handled (passed to next handler).
    Ignored,
    /// The application should exit.
    Exit,
}
