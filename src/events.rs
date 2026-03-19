use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
#[doc = "Events produced during a chat stream."]
pub enum ChatEvent {
    #[doc = "Plain text content from the model."]
    Text(String),
    #[doc = "Complete reasoning content from the model."]
    Reasoning(String),
    #[doc = "Incremental reasoning delta from the model."]
    ReasoningDelta(String),
    #[doc = "Tool call request from the model."]
    ToolCall {
        #[doc = "Name of the tool being called."]
        name: String,
        #[doc = "Arguments passed to the tool."]
        arguments: String,
    },
    #[doc = "Incremental tool call delta from the model."]
    ToolCallDelta(String),
    #[doc = "Result from a tool execution."]
    ToolResult {
        #[doc = "Content of the tool result."]
        content: String,
    },
    #[doc = "Token usage statistics from the model."]
    Usage {
        #[doc = "Number of input tokens used."]
        input_tokens: u64,
        #[doc = "Number of output tokens generated."]
        output_tokens: u64,
        #[doc = "Total number of tokens used."]
        total_tokens: u64,
    },
    #[doc = "Stream has completed."]
    Done,
}
