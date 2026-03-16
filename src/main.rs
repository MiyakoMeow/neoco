//! Neoco CLI - Simple chat with LLM using rig-core and ratatui

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::{Terminal, TerminalOptions, Viewport, prelude::*, widgets::Paragraph};
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::anthropic::Client as AnthropicClient;
use rig::providers::openai::Client as OpenAIClient;

mod config;
use config::{Config, ProviderType};

/// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "neoco")]
#[command(about = "Chat with LLM from command line")]
struct Cli {
    /// Message to send to the model
    #[arg(short = 'M', long = "message", required = true)]
    message: String,

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

    // Load configuration
    let config = Config::load_default()?;

    // Determine which model to use
    let model_string = if let Some(group) = &args.model_group {
        config
            .get_model_from_group(group)
            .with_context(|| format!("Unknown model group: {}", group))?
    } else if let Some(model) = &args.model {
        model.clone()
    } else if let Some(group) = &config.model_group {
        config
            .get_model_from_group(group)
            .with_context(|| format!("Unknown model group: {}", group))?
    } else if let Some(model) = &config.model {
        model.clone()
    } else {
        anyhow::bail!(
            "No model specified. Use --model or --model_group, or configure in neoco.toml"
        );
    };

    // Extract provider from model string
    let provider_config = config
        .extract_provider(&model_string)
        .with_context(|| format!("Unknown provider for model: {}", model_string))?;

    // Get API key
    let api_key = config.get_api_key(provider_config)?;

    // Parse model name (remove provider prefix)
    let model_name = model_string
        .split('/')
        .nth(1)
        .map(|s| s.split('?').next().unwrap_or(s))
        .unwrap_or(&model_string)
        .to_string();

    // Create client based on provider type
    let response: String = match provider_config.r#type {
        ProviderType::OpenAI | ProviderType::OpenAIResponses => {
            let client = OpenAIClient::builder()
                .api_key(&api_key)
                .base_url(&provider_config.base_url)
                .build()
                .context("Failed to create OpenAI client")?;
            let agent = client.agent(&model_name).build();

            eprintln!(
                "Using provider: {} ({})",
                provider_config.name, provider_config.base_url
            );

            agent.prompt(&args.message).await?
        },
        ProviderType::Anthropic => {
            let client = AnthropicClient::new(api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create Anthropic client: {}", e))?;
            let agent = client.agent(&model_name).build();

            eprintln!(
                "Using provider: {} ({})",
                provider_config.name, provider_config.base_url
            );

            agent.prompt(&args.message).await?
        },
    };

    // Output using ratatui Viewport::Inline
    let line_count = response.lines().count() as u16 + 1;
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(line_count),
        },
    )?;

    // Render the response directly without borders
    terminal.insert_before(0, |buf| {
        let para = Paragraph::new(response.as_str());
        para.render(buf.area, buf);
    })?;

    // Keep the output visible briefly then exit
    std::thread::sleep(std::time::Duration::from_millis(100));

    Ok(())
}
