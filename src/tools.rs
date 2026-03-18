//! Shell tool with sandbox integration

use anyhow::{Context, Result};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use std::env;

use crate::sandbox::{BashSandbox, SandboxError};

#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    #[error("Sandbox error: {0}")]
    SandboxError(String),
}

impl From<SandboxError> for CommandError {
    fn from(e: SandboxError) -> Self {
        CommandError::SandboxError(e.to_string())
    }
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
    /// Sandbox for command execution
    sandbox: BashSandbox,
}

impl ShellTool {
    pub fn new() -> Result<Self> {
        // Get current working directory as workspace
        let workspace = env::current_dir().context("Failed to get current directory")?;

        let sandbox = BashSandbox::new(&workspace).context("Failed to create bash sandbox")?;

        Ok(Self { sandbox })
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new().expect("Failed to create ShellTool")
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

        let workspace_str = self.sandbox.workspace().display().to_string();
        let mut description = format!(
            "Execute a bash command and return the output. \
            File system access is restricted to the current directory: {workspace_str}. \
            Path traversal (../) and dangerous shell patterns are blocked."
        );
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
        // First, validate and sanitize through sandbox
        self.sandbox
            .execute(&args.command)
            .await
            .map_err(CommandError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_tool_creation() {
        let tool = ShellTool::new();
        assert!(tool.is_ok());
    }
}
