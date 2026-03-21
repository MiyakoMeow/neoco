//! Agent and chat functionality.

pub mod chat;
pub mod state;

// Re-export error types from neoco-errors for convenience
pub use chat::chat;
pub use neoco_errors::ChatError;
pub use state::{AgentState, ToolCall};

// Re-export ChatMessage and ChatEvent from neoco-event for convenience
pub use neoco_event::{ChatEvent, ChatMessage};

/// Trait for handling chat events.
pub trait EventHandler {
    /// Handle a chat event.
    fn handle(&self, event: ChatEvent);
}
