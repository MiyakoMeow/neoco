use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i32, String),
}

pub struct ShellTool(String);

impl ShellTool {
    pub fn new() -> Self {
        let name = Self::get_shell_name();
        Self(name)
    }

    fn get_shell_name() -> String {
        #[cfg(target_os = "windows")]
        {
            if std::env::var("PSModulePath").is_ok() {
                "powershell".to_string()
            } else {
                "cmd".to_string()
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::env::var("SHELL")
                .ok()
                .and_then(|s| s.split('/').last().map(String::from))
                .unwrap_or_else(|| "sh".to_string())
        }
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ShellTool {
    const NAME: &'static str = "shell";

    type Error = CommandError;
    type Args = CommandArgs;
    type Output = String;

    fn name(&self) -> String {
        self.0.clone()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let shell_name = &self.0;
        ToolDefinition {
            name: shell_name.clone(),
            description: format!(
                "Execute a {shell_name} command and return the output. Use this tool to run shell commands, scripts, or system operations."
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        #[cfg(target_os = "windows")]
        let output = if std::env::var("PSModulePath").is_ok() {
            Command::new("powershell")
                .args(["-Command", &args.command])
                .output()?
        } else {
            Command::new("cmd").args(["/C", &args.command]).output()?
        };

        #[cfg(not(target_os = "windows"))]
        let output = {
            let shell = std::env::var("SHELL")
                .ok()
                .and_then(|s| s.split('/').last().map(String::from))
                .unwrap_or_else(|| "sh".to_string());
            Command::new(shell).args(["-c", &args.command]).output()?
        };

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
    }
}
