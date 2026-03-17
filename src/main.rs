//! Neoco CLI - Simple chat with LLM using rig-core and ratatui

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::{Terminal, TerminalOptions, Viewport, prelude::*, widgets::Paragraph};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent;
mod config;
use config::Config;

use agent::chat;

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
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

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

    let results = chat(&config, &model_string, &args.messages).await?;

    output_results(&results)?;

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
