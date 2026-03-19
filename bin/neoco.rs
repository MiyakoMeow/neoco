//! Neoco CLI - Simple chat with LLM using rig-core

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use neoco::{Config, OutputHandler, chat, check_bash_available};

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

    check_bash_available().context("bash is required but not available")?;

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

    let output_handler = OutputHandler::new(1);

    chat(&config, &model_string, &args.messages, &output_handler).await?;

    Ok(())
}
