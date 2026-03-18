//! Tools module for neoco
//!
//! Provides shell command execution with sandbox security.

use anyhow::{Context, Result};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

/// Sandbox module for command security
pub mod sandbox;

use sandbox::{Sandbox, SandboxConfig};

const COMMAND_TIMEOUT_SECS: u64 = 60;

/// Arguments for shell command execution
#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    /// The command string to execute
    command: String,
    /// Optional timeout override in seconds
    #[serde(default)]
    timeout: Option<u64>,
}

/// Errors that can occur during command execution
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// Failed to execute command
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    /// Command timed out
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
    /// Command failed with non-zero exit code
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i32, String),
    /// Sandbox validation failed
    #[error("Sandbox validation failed: {0}")]
    SandboxError(String),
}

/// Check if bash is available on the system
///
/// # Errors
/// Returns an error if bash is not available or cannot be executed
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

/// Shell tool for executing bash commands with sandbox security
pub struct ShellTool {
    sandbox: Sandbox,
}

impl ShellTool {
    /// Create a new [`ShellTool`] with default sandbox configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            sandbox: Sandbox::default(),
        }
    }

    /// Create with custom sandbox configuration
    #[must_use]
    #[allow(dead_code)]
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
