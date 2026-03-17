use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use std::env;
use std::sync::LazyLock;
use tokio::process::Command;
use tokio::time::timeout;

const COMMAND_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Deserialize)]
pub struct CommandArgs {
    command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to execute command: {0}")]
    ExecuteError(#[from] std::io::Error),
    #[error("Command timed out after {0} seconds", COMMAND_TIMEOUT_SECS)]
    Timeout,
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i32, String),
}

static SHELL_CONFIG: LazyLock<ShellConfig> = LazyLock::new(ShellConfig::detect);

struct ShellConfig {
    name: String,
    args_prefix: Vec<&'static str>,
}

impl ShellConfig {
    fn detect() -> Self {
        if let Some(shell) = env::var_os("SHELL") {
            let shell_name = shell
                .to_string_lossy()
                .rsplit('/')
                .next()
                .unwrap_or(&shell.to_string_lossy())
                .to_string();
            if !shell_name.is_empty() {
                return Self {
                    name: shell_name,
                    args_prefix: vec!["-c"],
                };
            }
        }

        for var in ["PSModulePath", "PSExecutionPolicyPreference"] {
            if env::var(var).is_ok() {
                return Self {
                    name: "powershell".to_string(),
                    args_prefix: vec!["-Command"],
                };
            }
        }

        let shells = ["pwsh", "bash", "zsh", "fish", "sh", "cmd"];
        for shell in shells {
            if std::process::Command::new(shell)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                let args_prefix = if shell == "cmd" {
                    vec!["/C"]
                } else {
                    vec!["-c"]
                };
                return Self {
                    name: shell.to_string(),
                    args_prefix,
                };
            }
        }

        Self {
            name: "sh".to_string(),
            args_prefix: vec!["-c"],
        }
    }
}

pub struct ShellTool;

impl ShellTool {
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
    const NAME: &'static str = "shell";

    type Error = CommandError;
    type Args = CommandArgs;
    type Output = String;

    fn name(&self) -> String {
        SHELL_CONFIG.name.clone()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: SHELL_CONFIG.name.clone(),
            description: format!(
                "Execute a {} command and return the output. Use this tool to run shell commands, scripts, or system operations.",
                SHELL_CONFIG.name
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
        let mut cmd_args = SHELL_CONFIG.args_prefix.clone();
        cmd_args.push(&args.command);

        let output = timeout(
            tokio::time::Duration::from_secs(COMMAND_TIMEOUT_SECS),
            Command::new(&SHELL_CONFIG.name).args(cmd_args).output(),
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
            Err(_) => Err(CommandError::Timeout),
        }
    }
}
