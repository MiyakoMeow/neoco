//! Tools and utilities.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

const COMMAND_TIMEOUT_SECS: u64 = 60;

/// Errors that can occur when checking for or executing bash commands.
#[derive(Debug, Error)]
pub enum BashError {
    /// Failed to execute the bash process.
    #[error("Failed to execute bash at: {0}")]
    Execute(String),

    /// The bash version check command failed.
    #[error("bash --version failed at: {0}")]
    VersionCheck(String),

    /// The default bash version check returned a non-zero exit status.
    #[error("default bash --version returned non-zero exit status")]
    DefaultBashFailed,
}

impl From<std::io::Error> for BashError {
    fn from(e: std::io::Error) -> Self {
        BashError::Execute(e.to_string())
    }
}

/// Locates bash path from environment variables.
///
/// Only checks if the path is set and non-empty. The actual executable
/// validation is performed by `check_bash_available()` (synchronous, no timeout)
/// and `ShellTool::call()` (async with timeout).
fn get_bash_path() -> Option<String> {
    let candidates = [
        "NEOCO_GIT_BASH_PATH",
        "CLAUDE_CODE_GIT_BASH_PATH",
        "OPENCODE_GIT_BASH_PATH",
    ];

    for env_name in &candidates {
        if let Ok(path) = std::env::var(env_name)
            && !path.is_empty()
        {
            return Some(path);
        }
    }
    None
}

/// Arguments for shell command execution
#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    command: String,
    #[serde(default)]
    timeout: Option<u64>,
}

/// Result of shell command execution.
///
/// This struct captures all relevant information from a shell command execution,
/// including the command itself, its output streams, and exit status.
#[derive(Debug, Serialize, Clone)]
pub struct CommandResult {
    /// The command that was executed.
    command: String,
    /// Standard output from the command.
    stdout: String,
    /// Standard error output from the command.
    stderr: String,
    /// Exit code (0 for success, non-zero for failure).
    ///
    /// Uses i64 for consistency with external error reporting systems.
    exit_code: i64,
}

impl CommandResult {
    /// Get the command that was executed.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Get the standard output from the command.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Get the standard error output from the command.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Get the exit code.
    #[must_use]
    pub fn exit_code(&self) -> i64 {
        self.exit_code
    }

    /// Check if the command succeeded (exit code == 0).
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Errors that can occur during shell command execution.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// Failed to execute the command.
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    /// Command execution timed out.
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
    /// Command exited with a non-zero status code.
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i64, String),
}

/// Check if bash is available in the system.
///
/// # Errors
///
/// Returns an error if bash cannot be found or fails to execute.
pub fn check_bash_available() -> std::result::Result<(), BashError> {
    if let Some(path) = get_bash_path() {
        let output = std::process::Command::new(&path)
            .arg("--version")
            .output()?;
        if !output.status.success() {
            return Err(BashError::VersionCheck(path));
        }
        return Ok(());
    }
    let output = std::process::Command::new("bash")
        .arg("--version")
        .output()?;
    if !output.status.success() {
        return Err(BashError::DefaultBashFailed);
    }
    Ok(())
}

/// Shell command execution tool.
pub struct ShellTool;

impl ShellTool {
    /// Creates a new shell tool instance.
    #[must_use]
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

    type Error = CommandError;
    type Args = CommandArgs;
    type Output = CommandResult;

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
        let bash_path = get_bash_path().unwrap_or_else(|| "bash".to_string());
        let output = timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            Command::new(&bash_path)
                .kill_on_drop(true)
                .args(cmd_args)
                .output(),
        )
        .await;

        match output {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = i64::from(output.status.code().unwrap_or(-1));

                if !output.status.success() {
                    return Err(CommandError::ExitError(
                        exit_code,
                        format!("{stdout}{stderr}"),
                    ));
                }

                Ok(CommandResult {
                    command: args.command.clone(),
                    stdout,
                    stderr,
                    exit_code,
                })
            },
            Ok(Err(e)) => Err(CommandError::ExecuteError(e)),
            Err(_) => Err(CommandError::Timeout(timeout_secs)),
        }
    }
}
