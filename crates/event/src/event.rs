//! Unified event types for neoco.

use crossterm::event::KeyEvent;
use rig::completion::Usage;
use serde::Serialize;

/// Terminal-level events from the terminal.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// A key was pressed.
    Key(KeyEvent),
    /// The terminal was resized.
    Resize(u16, u16),
    /// Text was pasted from clipboard.
    Paste(String),
}

/// Events produced during a chat stream.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum ChatEvent {
    /// Plain text content from the model.
    Text(String),
    /// Complete reasoning content from the model.
    Reasoning(String),
    /// Incremental reasoning delta from the model.
    ReasoningDelta(String),
    /// Tool call request from the model.
    ToolCall {
        /// Arguments passed to the tool.
        arguments: String,
    },
    /// Incremental tool call delta from the model.
    ToolCallDelta(String),
    /// Result from a tool execution.
    ToolResult {
        /// Serialized content of the tool result.
        content: String,
        /// Optional structured data (for tools that return structured results).
        structured: Option<serde_json::Value>,
    },
    /// Token usage statistics from the model.
    Usage(Usage),
    /// Stream has completed.
    Done,
}

/// UI-level events for user interaction.
#[derive(Debug, Clone)]
pub enum UIEvent {
    /// Scroll chat pane up by the given number of lines.
    ScrollUp(u16),
    /// Scroll chat pane down by the given number of lines.
    ScrollDown(u16),
    /// User submitted a message to send.
    SendMessage(String),
    /// Request to exit the application.
    Exit,
}

/// Unified event type that encompasses all event types.
#[derive(Debug, Clone)]
pub enum UnifiedEvent {
    /// Terminal event from the terminal.
    Terminal(TerminalEvent),
    /// Chat event from the agent.
    Chat(ChatEvent),
    /// UI event from user interaction.
    UI(UIEvent),
}
