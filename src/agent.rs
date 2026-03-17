use anyhow::Result;
use futures::StreamExt;
use rig::agent::{Agent, MultiTurnStreamItem, PromptHook};
use rig::completion::{CompletionModel, GetTokenUsage, Message, Usage};
use rig::streaming::{StreamedAssistantContent, StreamingChat};

pub async fn send_message<M, P>(
    agent: &Agent<M, P>,
    message: &str,
    history: &[Message],
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
                print!("{}", text.text);
                full_response.push_str(&text.text);
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Reasoning(
                reasoning,
            ))) => {
                print!("[思考] {:?}", reasoning.content);
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Final(
                response,
            ))) => {
                token_usage = response.token_usage();
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::ReasoningDelta { reasoning, .. },
            )) => {
                print!("[思考] {reasoning}");
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id: _,
            })) => {
                print!("[工具调用] {}: ", tool_call.function.name);
                print!("{}", tool_call.function.arguments);
            },
            Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::ToolCallDelta { content, .. },
            )) => {
                print!("[工具调用] {content:?}");
            },
            Err(e) => {
                anyhow::bail!("Stream error: {e}");
            },
            _ => {},
        }
    }

    println!();

    if let Some(usage) = token_usage {
        eprintln!(
            "Token usage - Input: {}, Output: {}, Total: {}",
            usage.input_tokens, usage.output_tokens, usage.total_tokens
        );
    }

    Ok((full_response, token_usage))
}

pub async fn send_messages<M, P>(
    agent: &Agent<M, P>,
    messages: &[String],
    provider_name: &str,
    provider_base_url: &str,
) -> Result<Vec<(String, Option<Usage>)>>
where
    M: CompletionModel + 'static,
    P: PromptHook<M> + 'static,
{
    eprintln!("Using provider: {provider_name} ({provider_base_url})");

    let mut history: Vec<Message> = Vec::new();
    let mut results = Vec::new();

    for msg in messages {
        let (response, usage) = send_message(agent, msg, &history).await?;

        history.push(Message::user(msg));
        history.push(Message::assistant(&response));

        results.push((response, usage));
    }

    Ok(results)
}
