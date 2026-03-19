//! Chat event types.

use rig::completion::Usage;
use serde::Serialize;

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
        /// Name of the tool being called.
        name: String,
        /// Arguments passed to the tool.
        arguments: String,
    },
    /// Incremental tool call delta from the model.
    ToolCallDelta(String),
    /// Result from a tool execution.
    ToolResult {
        /// Content of the tool result.
        content: String,
    },
    /// Token usage statistics from the model.
    Usage(Usage),
    /// Stream has completed.
    Done,
}
