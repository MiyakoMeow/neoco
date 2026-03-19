use std::sync::Arc;

use anyhow::{Context, Result};
use rig::client::CompletionClient;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;
use ulid::Ulid;

use crate::agent::AnyAgent;
use crate::agent_tree::{InsertMode, QueuedMessage, SharedAgentTree};
use crate::config::{Config, ProviderType};

const DEFAULT_MAX_TURNS: usize = 1000;
const COMMAND_TIMEOUT_SECS: u64 = 60;

/// Arguments for the shell command tool.
#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    /// The command to execute.
    command: String,
    /// Optional timeout in seconds.
    #[serde(default)]
    timeout: Option<u64>,
}

/// Unified error type for all tool operations.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Shell command failed: {0}")]
    Command(#[from] CommandError),
    #[error("Agent spawn failed: {0}")]
    Spawn(#[from] SpawnError),
    #[error("Message send failed: {0}")]
    Send(#[from] SendError),
    #[error("General error: {0}")]
    General(#[from] anyhow::Error),
}

/// Errors that can occur when executing shell commands.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i32, String),
}

/// Checks if bash is available on the system.
pub fn check_bash_available() -> Result<()> {
    std::process::Command::new("bash")
        .arg("--version")
        .output()
        .context("Failed to execute bash")?
        .status
        .success()
        .then_some(())
        .context("bash --version returned non-zero exit status")
}

/// Tool for executing shell commands.
pub struct ShellTool;

impl ShellTool {
    /// Creates a new `ShellTool` instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self
    }
}

impl Tool for ShellTool {
    const NAME: &'static str = "bash";

    type Error = ToolError;
    type Args = CommandArgs;
    type Output = String;

    fn name(&self) -> String {
        "bash".to_string()
    }

    async fn definition(&self, prompt: String) -> ToolDefinition {
        use std::fmt::Write as _;

        let mut description = "Execute a bash command and return the output. Use this tool to run shell commands, scripts, or system operations.".to_string();
        if !prompt.is_empty() {
            let _ = writeln!(description);
            let _ = writeln!(description, "Additional instructions: {prompt}");
        }
        ToolDefinition {
            name: "bash".to_string(),
            description,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Timeout in seconds (default: 60)"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut cmd_args = vec!["-c"];
        cmd_args.push(&args.command);

        let timeout_secs = args.timeout.unwrap_or(COMMAND_TIMEOUT_SECS);
        let output = timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            Command::new("bash")
                .kill_on_drop(true)
                .args(cmd_args)
                .output(),
        )
        .await;

        match output {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if !output.status.success() {
                    let exit_code = output.status.code().unwrap_or(-1);
                    return Err(ToolError::Command(CommandError::ExitError(
                        exit_code,
                        format!("{stdout}{stderr}"),
                    )));
                }

                Ok(format!("{stdout}{stderr}"))
            },
            Ok(Err(e)) => Err(ToolError::Command(CommandError::ExecuteError(e))),
            Err(_) => Err(ToolError::Command(CommandError::Timeout(timeout_secs))),
        }
    }
}

/// Arguments for the spawn tool.
#[derive(Debug, Deserialize)]
pub struct SpawnArgs {
    /// Message to send to the child agent.
    message: String,
    /// Model group to use for the child agent.
    model_group: String,
}

/// Errors that can occur when spawning a child agent.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("Failed to create agent: {0}")]
    CreateError(#[from] anyhow::Error),
}

/// Tool for spawning child agents.
pub struct SpawnTool {
    config: Arc<Config>,
    agent_tree: SharedAgentTree,
    current_agent_id: Ulid,
}

impl SpawnTool {
    /// Creates a new `SpawnTool` instance.
    pub fn new(config: Arc<Config>, agent_tree: SharedAgentTree, current_agent_id: Ulid) -> Self {
        Self {
            config,
            agent_tree,
            current_agent_id,
        }
    }
}

impl Tool for SpawnTool {
    const NAME: &'static str = "spawn";

    type Error = ToolError;
    type Args = SpawnArgs;
    type Output = String;

    fn name(&self) -> String {
        "spawn".to_string()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "spawn".to_string(),
            description: "Spawn a child agent to handle a subtask. The child agent runs in parallel and its response will be added to the parent's message queue.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to send to the child agent"
                    },
                    "model_group": {
                        "type": "string",
                        "description": "Model group to use for the child agent"
                    }
                },
                "required": ["message", "model_group"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let model_string = self
            .config
            .get_model_from_group(&args.model_group)
            .ok_or_else(|| anyhow::anyhow!("Unknown model group: {}", args.model_group))?;

        let provider_config = self
            .config
            .extract_provider(&model_string)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider for model: {model_string}"))?;

        let api_key = Config::get_api_key(provider_config)?;

        let model_name = match model_string.split('/').nth(1) {
            Some(s) => s.split('?').next().unwrap_or(s).to_string(),
            None => model_string.clone(),
        };

        let tree = self.agent_tree.clone();
        let depth = tree.lock().await.get_agent_depth(self.current_agent_id);
        if depth >= self.config.agent_limits.tree_depth {
            return Err(ToolError::Spawn(SpawnError::CreateError(anyhow::anyhow!(
                "Max tree depth {} exceeded (current: {})",
                self.config.agent_limits.tree_depth,
                depth
            ))));
        }

        let children_count = tree
            .lock()
            .await
            .get_children_count(self.current_agent_id)
            .await;
        if children_count >= self.config.agent_limits.children_per_parent {
            return Err(ToolError::Spawn(SpawnError::CreateError(anyhow::anyhow!(
                "Max children per parent {} exceeded (current: {})",
                self.config.agent_limits.children_per_parent,
                children_count
            ))));
        }

        let active_count = tree.lock().await.get_active_spawn_count().await;
        if active_count >= self.config.agent_limits.concurrent_spawns {
            return Err(ToolError::Spawn(SpawnError::CreateError(anyhow::anyhow!(
                "Max concurrent spawns {} exceeded (current: {})",
                self.config.agent_limits.concurrent_spawns,
                active_count
            ))));
        }

        let child_id = Ulid::new();
        let parent_id = self.current_agent_id;

        let full_child_agent: AnyAgent = match provider_config.r#type {
            ProviderType::OpenAICompletions => {
                use rig::providers::openai::CompletionsClient;
                let client = CompletionsClient::builder()
                    .api_key(&api_key)
                    .base_url(&provider_config.base_url)
                    .build()
                    .context("Failed to create OpenAI Completions client")?;
                let agent = client
                    .agent(&model_name)
                    .tool(ShellTool::new())
                    .tool(SpawnTool::new(self.config.clone(), tree.clone(), child_id))
                    .tool(SendTool::new(tree.clone(), child_id))
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build();
                AnyAgent::OpenAICompletions(agent)
            },
            ProviderType::OpenAIResponses => {
                use rig::providers::openai::Client as OpenAIClient;
                let client = OpenAIClient::builder()
                    .api_key(&api_key)
                    .base_url(&provider_config.base_url)
                    .build()
                    .context("Failed to create OpenAI Responses client")?;
                let agent = client
                    .agent(&model_name)
                    .tool(ShellTool::new())
                    .tool(SpawnTool::new(self.config.clone(), tree.clone(), child_id))
                    .tool(SendTool::new(tree.clone(), child_id))
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build();
                AnyAgent::OpenAIResponses(agent)
            },
            ProviderType::Anthropic => {
                use rig::providers::anthropic::Client;
                let client = Client::builder()
                    .api_key(api_key.as_str())
                    .base_url(&provider_config.base_url)
                    .anthropic_version(&provider_config.anthropic_version)
                    .build()
                    .context("Failed to create Anthropic client")?;
                let agent = client
                    .agent(&model_name)
                    .tool(ShellTool::new())
                    .tool(SpawnTool::new(self.config.clone(), tree.clone(), child_id))
                    .tool(SendTool::new(tree.clone(), child_id))
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build();
                AnyAgent::Anthropic(agent)
            },
        };

        {
            let mut tree_lock = tree.lock().await;
            tree_lock
                .add_child_with_id(parent_id, child_id, full_child_agent)
                .await;
            tree_lock
                .run_child_agent(child_id, args.message, InsertMode::Queue)
                .await;
        }

        Ok(format!("child_{child_id}"))
    }
}

/// Arguments for the send tool.
#[derive(Debug, Deserialize)]
pub struct SendArgs {
    /// Target agent ID.
    to_agent_id: String,
    /// Message to send.
    message: String,
    /// How to insert the message.
    #[serde(default)]
    insert_mode: InsertMode,
}

/// Errors that can occur when sending a message to another agent.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("Invalid agent ID: {0}")]
    InvalidId(String),
    #[error("Target agent not found")]
    NotFound,
}

/// Tool for sending messages to other agents in the tree.
pub struct SendTool {
    agent_tree: SharedAgentTree,
    current_agent_id: Ulid,
}

impl SendTool {
    /// Creates a new `SendTool` instance.
    pub fn new(agent_tree: SharedAgentTree, current_agent_id: Ulid) -> Self {
        Self {
            agent_tree,
            current_agent_id,
        }
    }
}

impl Tool for SendTool {
    const NAME: &'static str = "send";

    type Error = ToolError;
    type Args = SendArgs;
    type Output = String;

    fn name(&self) -> String {
        "send".to_string()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "send".to_string(),
            description: "Send a message to another agent in the agent tree.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "to_agent_id": {
                        "type": "string",
                        "description": "Target agent ID (e.g., child_01HYX7K8Z9ABCDEFGHJKMNPRQV)"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message to send"
                    },
                    "insert_mode": {
                        "type": "string",
                        "enum": ["queue", "interrupt", "append"],
                        "description": "How to insert the message: queue (default), interrupt, append"
                    }
                },
                "required": ["to_agent_id", "message"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let target_id_str = args.to_agent_id.trim_start_matches("child_");
        let target_id: Ulid = target_id_str
            .parse()
            .map_err(|_| SendError::InvalidId(args.to_agent_id.clone()))?;

        let tree = self.agent_tree.clone();
        let tree_lock = tree.lock().await;

        // Verify target agent exists
        if tree_lock.get_agent(target_id).is_none() {
            return Err(ToolError::Send(SendError::NotFound));
        }

        let pending_msg = QueuedMessage {
            content: args.message,
            mode: args.insert_mode,
            from_agent_id: self.current_agent_id,
        };

        // Use async version to avoid blocking in async context
        tree_lock.add_pending_message(target_id, pending_msg).await;

        Ok("Message sent".to_string())
    }
}
