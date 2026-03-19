use anyhow::{Context, Result};
use futures::StreamExt;
use rig::agent::{Agent, MultiTurnStreamItem, PromptHook};
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, GetTokenUsage, Message, Usage};
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use tracing::info;

use crate::config::{Config, ProviderType};
use crate::output::OutputCallback;
use crate::tools::ShellTool;

enum AnyAgent {
    OpenAICompletions(Agent<rig::providers::openai::CompletionModel>),
    OpenAIResponses(Agent<rig::providers::openai::responses_api::ResponsesCompletionModel>),
    Anthropic(Agent<rig::providers::anthropic::completion::CompletionModel>),
}

impl AnyAgent {
    async fn chat(
        &mut self,
        message: &str,
        history: &[Message],
        output_callback: Option<&OutputCallback<'_>>,
    ) -> Result<(String, Option<Usage>)> {
        match self {
            Self::OpenAICompletions(agent) => {
                chat_with_agent(agent, message, history, output_callback).await
            },
            Self::OpenAIResponses(agent) => {
                chat_with_agent(agent, message, history, output_callback).await
            },
            Self::Anthropic(agent) => {
                chat_with_agent(agent, message, history, output_callback).await
            },
        }
    }
}

async fn chat_with_agent<M, P>(
    agent: &Agent<M, P>,
    message: &str,
    history: &[Message],
    output_callback: Option<&OutputCallback<'_>>,
) -> Result<(String, Option<Usage>)>
where
    M: CompletionModel + 'static,
    P: PromptHook<M> + 'static,
{
    let mut stream = agent.stream_chat(message, history.to_vec()).await;

    let mut full_response = String::new();
    let mut token_usage: Option<Usage> = None;

    while let Some(item) = stream.next().await {
        match item {
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text))) => {
                if let Some(cb) = output_callback {
                    cb(&text.text);
                }
                full_response.push_str(&text.text);
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Reasoning(
                reasoning,
            ))) => {
                if let Some(cb) = output_callback {
                    cb(&format!("[思考] {:?}", reasoning.content));
                }
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Final(
                response,
            ))) => {
                token_usage = response.token_usage();
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::ReasoningDelta { reasoning, .. },
            )) => {
                if let Some(cb) = output_callback {
                    cb(&format!("[思考] {reasoning}"));
                }
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id: _,
            })) => {
                if let Some(cb) = output_callback {
                    cb(&format!("[工具调用] {}: ", tool_call.function.name));
                    cb(&tool_call.function.arguments.to_string());
                }
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::ToolCallDelta { content, .. },
            )) => {
                if let Some(cb) = output_callback {
                    cb(&format!("[工具调用] {content:?}"));
                }
            },
            Ok(MultiTurnStreamItem::StreamUserItem(content)) => {
                use rig::streaming::StreamedUserContent;
                match content {
                    StreamedUserContent::ToolResult { tool_result, .. } => {
                        if let Some(cb) = output_callback {
                            cb(&format!("[工具结果] {:#?}\n", tool_result.content));
                        }
                    },
                }
            },
            Err(e) => {
                anyhow::bail!("Stream error: {e}");
            },
            _ => {},
        }
    }

    if let Some(cb) = output_callback {
        cb("\n");
    }

    if let Some(usage) = token_usage {
        info!(
            "Token usage - Input: {}, Output: {}, Total: {}",
            usage.input_tokens, usage.output_tokens, usage.total_tokens
        );
    }

    Ok((full_response, token_usage))
}

/// Send a chat message to the model and get responses.
///
/// # Errors
///
/// Returns an error if provider extraction, API key retrieval, or the chat request fails.
pub async fn chat(
    config: &Config,
    model_string: &str,
    messages: &[String],
    output_callback: Option<&OutputCallback<'_>>,
) -> Result<Vec<(String, Option<Usage>)>> {
    let provider_config = config
        .extract_provider(model_string)
        .with_context(|| format!("Unknown provider for model: {model_string}"))?
        .clone();

    let api_key = Config::get_api_key(&provider_config)?;

    let model_name = match model_string.split('/').nth(1) {
        Some(s) => s.split('?').next().unwrap_or(s).to_string(),
        None => model_string.to_string(),
    };

    if messages.is_empty() {
        anyhow::bail!("No message provided. Use -M/--message to send a message.");
    }

    let mut agent = match provider_config.r#type {
        ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(&api_key)
                .base_url(&provider_config.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;
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
                .context("Failed to create OpenAI Responses client")?;
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
                .context("Failed to create Anthropic client")?;
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
        let (response, usage) = agent.chat(msg, &history, output_callback).await?;

        history.push(Message::user(msg));
        history.push(Message::assistant(&response));

        results.push((response, usage));
    }

    Ok(results)
}
