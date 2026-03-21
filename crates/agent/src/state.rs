//! Agent state management.

use rig::completion::Usage;

// Re-export ChatMessage from neoco-event
pub use neoco_event::ChatMessage;

/// A tool call being executed.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// The command being executed.
    pub command: String,
    /// The arguments passed to the tool.
    pub arguments: String,
}

/// Agent state that manages chat history and tool calls.
#[derive(Debug, Clone)]
pub struct AgentState {
    /// Chat message history.
    pub chat_history: Vec<ChatMessage>,
    /// Current tool call being executed.
    pub current_tool_call: Option<ToolCall>,
    /// Token usage from the last response.
    pub token_usage: Option<Usage>,
    /// Whether the agent is waiting for a response.
    pub is_loading: bool,
}

impl AgentState {
    /// Create a new agent state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chat_history: Vec::new(),
            current_tool_call: None,
            token_usage: None,
            is_loading: false,
        }
    }

    /// Add a message to the chat history.
    pub fn push_message(&mut self, message: ChatMessage) {
        self.chat_history.push(message);
    }

    /// Update the current tool call.
    pub fn update_tool_call(&mut self, tool_call: ToolCall) {
        self.current_tool_call = Some(tool_call);
    }

    /// Clear the current tool call.
    pub fn clear_tool_call(&mut self) {
        self.current_tool_call = None;
    }

    /// Update token usage.
    pub fn update_usage(&mut self, usage: Usage) {
        self.token_usage = Some(usage);
    }

    /// Set loading state.
    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_new() {
        let state = AgentState::new();
        assert!(state.chat_history.is_empty());
        assert!(state.current_tool_call.is_none());
        assert!(state.token_usage.is_none());
        assert!(!state.is_loading);
    }

    #[test]
    fn test_push_message() {
        let mut state = AgentState::new();
        state.push_message(ChatMessage::User("hello".to_string()));
        assert_eq!(state.chat_history.len(), 1);
    }

    #[test]
    fn test_update_tool_call() {
        let mut state = AgentState::new();
        let tool_call = ToolCall {
            command: "ls".to_string(),
            arguments: r#"{"command":"ls -la"}"#.to_string(),
        };
        state.update_tool_call(tool_call);
        assert!(state.current_tool_call.is_some());
        assert_eq!(state.current_tool_call.as_ref().unwrap().command, "ls");
    }

    #[test]
    fn test_clear_tool_call() {
        let mut state = AgentState::new();
        let tool_call = ToolCall {
            command: "ls".to_string(),
            arguments: "{}".to_string(),
        };
        state.update_tool_call(tool_call);
        state.clear_tool_call();
        assert!(state.current_tool_call.is_none());
    }

    #[test]
    fn test_parse_tool_call_from_json() {
        let args = r#"{"command": "ls -la"}"#;
        let msg = ChatMessage::parse_tool_call(args);
        match msg {
            ChatMessage::ToolCall { command } => assert_eq!(command, "ls -la"),
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_parse_tool_call_from_plain_text() {
        let args = "plain text command";
        let msg = ChatMessage::parse_tool_call(args);
        match msg {
            ChatMessage::ToolCall { command } => assert_eq!(command, "plain text command"),
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_parse_tool_result_with_structured_data() {
        let content = "raw output";
        let structured = serde_json::json!({
            "stdout": "hello",
            "stderr": "",
            "exit_code": 0
        });
        let msg = ChatMessage::parse_tool_result(content, &Some(structured));
        match msg {
            ChatMessage::ToolResult {
                stdout,
                stderr,
                exit_code,
            } => {
                assert_eq!(stdout, "hello");
                assert!(stderr.is_empty());
                assert_eq!(exit_code, 0);
            },
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_parse_tool_result_without_structured() {
        let content = "raw output";
        let msg = ChatMessage::parse_tool_result(content, &None);
        match msg {
            ChatMessage::Assistant(text) => assert_eq!(text, "raw output"),
            _ => panic!("Expected Assistant message"),
        }
    }
}
