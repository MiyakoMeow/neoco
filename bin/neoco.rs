//! Neoco CLI - Simple chat with LLM using rig-core

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use neoco_cli::CliRenderer;
use neoco_core::errors::{ChatError, RenderError};
use neoco_core::renderer::Renderer;
use neoco_core::{
    agent::chat,
    config::{Config, ConfigError},
    tools::{BashError, check_bash_available},
};
use neoco_tui::TuiRenderer;

/// Errors that can occur during CLI execution.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Bash is not available or not working correctly.
    #[error("bash is required but not available: {0}")]
    BashNotAvailable(#[from] BashError),

    /// Failed to load the configuration file.
    #[error("Failed to load config: {0}")]
    Config(#[from] ConfigError),

    /// The specified model group does not exist in the configuration.
    #[error("Unknown model group: {0}")]
    UnknownModelGroup(String),

    /// No model was specified and no default is configured.
    #[error("No model specified. Use --model or --model_group, or configure in neoco.toml")]
    NoModel,

    /// An error occurred during chat interaction.
    #[error("Chat error: {0}")]
    Chat(#[from] ChatError),

    /// A rendering error occurred.
    #[error("Render error: {0}")]
    Render(#[from] RenderError),
}

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
async fn main() -> std::result::Result<(), CliError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Cli::parse();

    check_bash_available()?;

    let config = Config::load_default()?;

    let model_string = if let Some(group) = &args.model_group {
        config
            .get_model_from_group(group)
            .ok_or_else(|| CliError::UnknownModelGroup(group.clone()))?
    } else if let Some(model) = &args.model {
        model.clone()
    } else if let Some(group) = &config.model_group {
        config
            .get_model_from_group(group)
            .ok_or_else(|| CliError::UnknownModelGroup(group.clone()))?
    } else if let Some(model) = &config.model {
        model.clone()
    } else {
        return Err(CliError::NoModel);
    };

    // 模式选择：有-M参数使用CLI模式，否则使用TUI模式
    if args.messages.is_empty() {
        // TUI模式：交互式聊天
        run_tui_mode(&config, &model_string).await?;
    } else {
        // CLI模式：一次性发送消息
        let mut renderer = CliRenderer::new();
        chat(&config, &model_string, &args.messages, &mut renderer).await?;
        renderer.shutdown()?;
    }

    Ok(())
}

/// Run TUI interactive mode.
async fn run_tui_mode(config: &Config, model_string: &str) -> Result<(), CliError> {
    let mut renderer = TuiRenderer::new()?;

    loop {
        match renderer.run() {
            Ok(input) => {
                let messages = vec![input];
                if let Err(e) = chat(config, model_string, &messages, &mut renderer).await {
                    renderer
                        .render_event(&neoco_core::events::ChatEvent::Text(format!("错误: {e}")))?;
                }
            },
            Err(RenderError::RenderFailed(msg)) if msg == "User quit" => {
                renderer.shutdown()?;
                break;
            },
            Err(e) => {
                renderer.shutdown()?;
                return Err(CliError::Render(e));
            },
        }
    }

    Ok(())
}
