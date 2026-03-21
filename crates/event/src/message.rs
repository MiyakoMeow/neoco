//! Message types for chat and tool interactions.

use serde_json::Value;

/// A message in the chat history.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    /// A message from the user.
    User(String),
    /// A message from the assistant.
    Assistant(String),
    /// Reasoning content from the assistant.
    Reasoning(String),
    /// A tool call with command arguments.
    ToolCall {
        /// The command being executed.
        command: String,
    },
    /// A tool execution result.
    ToolResult {
        /// Standard output from the tool.
        stdout: String,
        /// Standard error from the tool.
        stderr: String,
        /// Exit code of the tool.
        exit_code: i64,
    },
}

impl ChatMessage {
    /// Parse tool call arguments and create appropriate message.
    #[must_use]
    pub fn parse_tool_call(arguments: &str) -> Self {
        if let Ok(cmd_obj) = serde_json::from_str::<Value>(arguments)
            && let Some(command) = cmd_obj.get("command").and_then(Value::as_str)
        {
            return Self::ToolCall {
                command: command.to_string(),
            };
        }
        Self::ToolCall {
            command: arguments.to_string(),
        }
    }

    /// Parse tool result and create appropriate message.
    #[must_use]
    pub fn parse_tool_result(content: &str, structured: &Option<Value>) -> Self {
        if let Some(data) = structured
            && let (Some(stdout), Some(stderr), Some(exit_code)) = (
                data.get("stdout").and_then(Value::as_str),
                data.get("stderr").and_then(Value::as_str),
                data.get("exit_code").and_then(Value::as_i64),
            )
        {
            return Self::ToolResult {
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                exit_code,
            };
        }
        Self::Assistant(content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_call_from_json() {
        let args = r#"{"command": "ls -la"}"#;
        let msg = ChatMessage::parse_tool_call(args);
        assert!(matches!(msg, ChatMessage::ToolCall { command } if command == "ls -la"));
    }

    #[test]
    fn test_parse_tool_call_from_plain_text() {
        let args = "plain text command";
        let msg = ChatMessage::parse_tool_call(args);
        assert!(
            matches!(msg, ChatMessage::ToolCall { command } if command == "plain text command")
        );
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
        assert!(matches!(msg, ChatMessage::ToolResult { stdout, .. } if stdout == "hello"));
    }

    #[test]
    fn test_parse_tool_result_without_structured() {
        let content = "raw output";
        let msg = ChatMessage::parse_tool_result(content, &None);
        assert!(matches!(msg, ChatMessage::Assistant(text) if text == "raw output"));
    }
}
