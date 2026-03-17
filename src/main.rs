//! Neoco CLI - Simple chat with LLM using rig-core and ratatui

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::{Terminal, TerminalOptions, Viewport, prelude::*, widgets::Paragraph};

mod agent;
mod config;
use agent::send_messages;
use config::{Config, ProviderType};

/// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "neoco")]
#[command(about = "Chat with LLM from command line")]
struct Cli {
    /// Message to send to the model (can be used multiple times for multi-turn)
    #[arg(short = 'M', long = "message")]
    messages: Vec<String>,

    /// Model to use (format: provider/model-name, e.g., deepseek/deepseek-chat)
    #[arg(short, long)]
    model: Option<String>,

    /// Model group to use (smart, fast, balanced)
    #[arg(short = 'g', long = "model_group")]
    model_group: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    let config = Config::load_default()?;

    let model_string = if let Some(group) = &args.model_group {
        config
            .get_model_from_group(group)
            .with_context(|| format!("Unknown model group: {group}"))?
    } else if let Some(model) = &args.model {
        model.clone()
    } else if let Some(group) = &config.model_group {
        config
            .get_model_from_group(group)
            .with_context(|| format!("Unknown model group: {group}"))?
    } else if let Some(model) = &config.model {
        model.clone()
    } else {
        anyhow::bail!(
            "No model specified. Use --model or --model_group, or configure in neoco.toml"
        );
    };

    let provider_config = config
        .extract_provider(&model_string)
        .with_context(|| format!("Unknown provider for model: {model_string}"))?;

    let api_key = Config::get_api_key(provider_config)?;

    let model_name = model_string
        .split('/')
        .nth(1)
        .map_or(model_string.as_str(), |s| s.split('?').next().unwrap_or(s))
        .to_string();

    if args.messages.is_empty() {
        anyhow::bail!("No message provided. Use -M/--message to send a message.");
    }

    match provider_config.r#type {
        ProviderType::OpenAICompletions => {
            use rig::client::CompletionClient;
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(&api_key)
                .base_url(&provider_config.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;
            let agent = client.agent(&model_name).build();
            let results = send_messages(
                &agent,
                &args.messages,
                &provider_config.name,
                &provider_config.base_url,
            )
            .await?;
            output_results(&results)?;
        },
        ProviderType::OpenAIResponses => {
            use rig::client::CompletionClient;
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(&api_key)
                .base_url(&provider_config.base_url)
                .build()
                .context("Failed to create OpenAI Responses client")?;
            let agent = client.agent(&model_name).build();
            let results = send_messages(
                &agent,
                &args.messages,
                &provider_config.name,
                &provider_config.base_url,
            )
            .await?;
            output_results(&results)?;
        },
        ProviderType::Anthropic => {
            use rig::client::CompletionClient;
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(api_key.as_str())
                .base_url(&provider_config.base_url)
                .anthropic_version("2023-06-01")
                .build()
                .context("Failed to create Anthropic client")?;
            let agent = client.agent(&model_name).build();
            let results = send_messages(
                &agent,
                &args.messages,
                &provider_config.name,
                &provider_config.base_url,
            )
            .await?;
            output_results(&results)?;
        },
    }

    Ok(())
}

fn output_results(results: &[(String, Option<rig::completion::Usage>)]) -> Result<()> {
    let empty_response = String::new();
    let last_response = results.last().map_or(&empty_response, |(r, _)| r);
    let line_count = u16::try_from(last_response.lines().count()).unwrap_or(0) + 1;
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(line_count),
        },
    )?;

    terminal.insert_before(0, |buf| {
        let para = Paragraph::new(last_response.as_str());
        para.render(buf.area, buf);
    })?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    Ok(())
}
