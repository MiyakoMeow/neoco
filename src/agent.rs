//! Agent and chat functionality.

use futures::{Stream, StreamExt};
use rig::agent::{Agent, MultiTurnStreamItem, PromptHook};
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, GetTokenUsage, Message, Usage};
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use tracing::info;

use crate::config::{Config, ProviderType};
use crate::errors::ChatError;
use crate::events::ChatEvent;
use crate::output::EventHandler;
use crate::tools::ShellTool;

type Result<T> = std::result::Result<T, ChatError>;

/// Extract structured data from tool result content.
///
/// Tool results from the LLM are wrapped in a specific JSON structure:
/// ```json
/// [
///   {
///     "type": "tool_result",
///     "content": "{\"text\": \"{...escaped JSON...}\"}"
///   }
/// ]
/// ```
///
/// This function:
/// 1. Expects the content to be an array
/// 2. Takes the first element
/// 3. Extracts the "text" field as a string
/// 4. Parses that string as JSON
///
/// Returns `None` if the content cannot be parsed or doesn't match the expected format.
fn extract_structured_data(content_value: &serde_json::Value) -> Option<serde_json::Value> {
    content_value
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
}

enum AnyAgent {
    OpenAICompletions(Agent<rig::providers::openai::CompletionModel>),
    OpenAIResponses(Agent<rig::providers::openai::responses_api::ResponsesCompletionModel>),
    Anthropic(Agent<rig::providers::anthropic::completion::CompletionModel>),
}

trait ChatStreamer {
    fn stream_chat_events(
        &self,
        message: &str,
        history: Vec<Message>,
    ) -> impl Stream<Item = Result<ChatEvent>> + Send;
}

impl<M, P> ChatStreamer for Agent<M, P>
where
    M: CompletionModel + 'static,
    P: PromptHook<M> + 'static,
{
    fn stream_chat_events(
        &self,
        message: &str,
        history: Vec<Message>,
    ) -> impl Stream<Item = Result<ChatEvent>> + Send {
        async_stream::stream! {
            let mut stream = self.stream_chat(message, history).await;
            let mut token_usage: Option<Usage> = None;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Text(text),
                    )) => {
                        yield Ok(ChatEvent::Text(text.text));
                    },
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Reasoning(reasoning),
                    )) => {
                        let content = serde_json::to_string(&reasoning.content)?;
                        yield Ok(ChatEvent::Reasoning(content));
                    },
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Final(response),
                    )) => {
                        token_usage = response.token_usage();
                    },
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ReasoningDelta { reasoning, .. },
                    )) => {
                        yield Ok(ChatEvent::ReasoningDelta(reasoning));
                    },
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ToolCall {
                            tool_call,
                            internal_call_id: _,
                        },
                    )) => {
                        yield Ok(ChatEvent::ToolCall {
                            arguments: tool_call.function.arguments.to_string(),
                        });
                    },
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ToolCallDelta { content, .. },
                    )) => {
                        yield Ok(ChatEvent::ToolCallDelta(format!("{content:?}")));
                    },
                    Ok(MultiTurnStreamItem::StreamUserItem(content)) => {
                        use rig::streaming::StreamedUserContent;
                        let StreamedUserContent::ToolResult { tool_result, .. } = content;

                        let content_value = serde_json::to_value(&tool_result.content).ok();
                        let structured = content_value.as_ref().and_then(extract_structured_data);
                        let content_str = content_value
                            .as_ref()
                            .and_then(|v| v.as_str())
                            .unwrap_or(&serde_json::to_string(&tool_result.content)?)
                            .to_string();

                        yield Ok(ChatEvent::ToolResult {
                            content: content_str,
                            structured,
                        });
                    },
                    Err(e) => {
                        yield Err(ChatError::Stream(e.to_string()));
                    },
                    _ => {},
                }
            }

            if let Some(usage) = token_usage {
                info!(
                    "Token usage - Input: {}, Output: {}, Total: {}",
                    usage.input_tokens, usage.output_tokens, usage.total_tokens
                );
                yield Ok(ChatEvent::Usage(usage));
            }

            yield Ok(ChatEvent::Done);
        }
    }
}

impl AnyAgent {
    async fn chat<H>(
        &mut self,
        message: &str,
        history: &[Message],
        handler: &H,
    ) -> Result<(String, Option<Usage>)>
    where
        H: EventHandler + ?Sized,
    {
        use futures::Stream;
        use std::pin::Pin;

        let history_vec = history.to_vec();
        let stream: Pin<Box<dyn Stream<Item = Result<ChatEvent>> + Send>> = match self {
            Self::OpenAICompletions(agent) => {
                Box::pin(agent.stream_chat_events(message, history_vec))
            },
            Self::OpenAIResponses(agent) => {
                Box::pin(agent.stream_chat_events(message, history_vec))
            },
            Self::Anthropic(agent) => Box::pin(agent.stream_chat_events(message, history_vec)),
        };

        let mut full_response = String::new();
        let mut token_usage: Option<Usage> = None;

        tokio::pin!(stream);
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    if let ChatEvent::Text(ref text) = event {
                        full_response.push_str(text);
                    }
                    if let ChatEvent::Usage(usage) = event {
                        token_usage = Some(usage);
                    } else {
                        handler.handle(event);
                    }
                },
                Err(e) => {
                    return Err(e);
                },
            }
        }

        Ok((full_response, token_usage))
    }
}

/// Send a chat message to the model and get responses.
///
/// # Errors
///
/// Returns an error if provider extraction fails, API key retrieval fails,
/// or the chat request fails.
pub async fn chat<H>(
    config: &Config,
    model_string: &str,
    messages: &[String],
    handler: &H,
) -> Result<Vec<(String, Option<Usage>)>>
where
    H: EventHandler + ?Sized,
{
    let provider_config = config
        .extract_provider(model_string)
        .ok_or_else(|| ChatError::UnknownProvider(model_string.to_string()))?
        .clone();

    let api_key =
        Config::get_api_key(&provider_config).map_err(|e| ChatError::ApiKey(e.to_string()))?;

    let model_name = match model_string.split('/').nth(1) {
        Some(s) => s.split('?').next().unwrap_or(s).to_string(),
        None => model_string.to_string(),
    };

    if messages.is_empty() {
        return Err(ChatError::NoMessage);
    }

    let mut agent = match provider_config.r#type {
        ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(&api_key)
                .base_url(&provider_config.base_url)
                .build()
                .map_err(|e| ChatError::ClientCreation(e.to_string()))?;
            let ag = client
                .agent(&model_name)
                .tool(ShellTool::new())
                .default_max_turns(usize::MAX / 2)
                .build();
            AnyAgent::OpenAICompletions(ag)
        },
        ProviderType::OpenAIResponses => {
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(&api_key)
                .base_url(&provider_config.base_url)
                .build()
                .map_err(|e| ChatError::ClientCreation(e.to_string()))?;
            let ag = client
                .agent(&model_name)
                .tool(ShellTool::new())
                .default_max_turns(usize::MAX / 2)
                .build();
            AnyAgent::OpenAIResponses(ag)
        },
        ProviderType::Anthropic => {
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(api_key.as_str())
                .base_url(&provider_config.base_url)
                .anthropic_version("2023-06-01")
                .build()
                .map_err(|e| ChatError::ClientCreation(e.to_string()))?;
            let ag = client
                .agent(&model_name)
                .tool(ShellTool::new())
                .default_max_turns(usize::MAX / 2)
                .build();
            AnyAgent::Anthropic(ag)
        },
    };

    info!(
        "Using provider: {} ({})",
        provider_config.name, provider_config.base_url
    );

    let mut history: Vec<Message> = Vec::new();
    let mut results = Vec::new();

    for msg in messages {
        let (response, usage) = agent.chat(msg, &history, handler).await?;

        history.push(Message::user(msg));
        history.push(Message::assistant(&response));

        results.push((response, usage));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_structured_data_valid() {
        let json = r#"[{"text":"{\"key\":\"value\"}"}]"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let result = extract_structured_data(&value);
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert_eq!(extracted["key"], "value");
    }

    #[test]
    fn test_extract_structured_data_nested_json() {
        let json = r#"[{"text":"{\"stdout\":\"output\",\"stderr\":\"error\",\"exit_code\":0}"}]"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let result = extract_structured_data(&value);
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert_eq!(extracted["stdout"], "output");
        assert_eq!(extracted["stderr"], "error");
        assert_eq!(extracted["exit_code"], 0);
    }

    #[test]
    fn test_extract_structured_data_not_array() {
        let value = serde_json::json!({"key": "value"});
        assert!(extract_structured_data(&value).is_none());
    }

    #[test]
    fn test_extract_structured_data_empty_array() {
        let value = serde_json::json!([]);
        assert!(extract_structured_data(&value).is_none());
    }

    #[test]
    fn test_extract_structured_data_no_text_field() {
        let json = r#"[{"content":"{}"}]"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(extract_structured_data(&value).is_none());
    }

    #[test]
    fn test_extract_structured_data_invalid_inner_json() {
        let json = r#"[{"text":"not valid json"}]"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(extract_structured_data(&value).is_none());
    }
}
