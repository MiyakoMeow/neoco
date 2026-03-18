use anyhow::{Context, Result};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

pub mod sandbox;

use sandbox::{Sandbox, SandboxConfig};

const COMMAND_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    command: String,
    #[serde(default)]
    timeout: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i32, String),
    #[error("Sandbox validation failed: {0}")]
    SandboxError(String),
}

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

pub struct ShellTool {
    sandbox: Sandbox,
}

impl ShellTool {
    pub fn new() -> Self {
        Self {
            sandbox: Sandbox::default(),
        }
    }

    /// Create with custom sandbox configuration
    pub fn with_config(config: SandboxConfig) -> Self {
        Self {
            sandbox: Sandbox::new(config),
        }
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ShellTool {
    const NAME: &'static str = "bash";

    type Error = CommandError;
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
        // Validate command through sandbox first
        if let Err(e) = self.sandbox.validate_command(&args.command) {
            return Err(CommandError::SandboxError(e.to_string()));
        }

        let mut cmd_args = vec!["-c"];
        cmd_args.push(&args.command);

        let timeout_secs = args.timeout.unwrap_or(COMMAND_TIMEOUT_SECS);

        // Execute in workspace directory
        let output = timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            Command::new("bash")
                .kill_on_drop(true)
                .current_dir(self.sandbox.workspace_dir())
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
                    return Err(CommandError::ExitError(
                        exit_code,
                        format!("{stdout}{stderr}"),
                    ));
                }

                Ok(format!("{stdout}{stderr}"))
            },
            Ok(Err(e)) => Err(CommandError::ExecuteError(e)),
            Err(_) => Err(CommandError::Timeout(timeout_secs)),
        }
    }
}
