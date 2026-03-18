use anyhow::{Context, Result};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use std::sync::LazyLock;
use tokio::process::Command;
use tokio::time::timeout;

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
}

static SHELL_CONFIG: LazyLock<ShellConfig> =
    LazyLock::new(|| ShellConfig::detect().expect("bash must be available"));

struct ShellConfig {
    name: String,
    args_prefix: Vec<&'static str>,
}

impl ShellConfig {
    fn detect() -> Result<Self> {
        std::process::Command::new("bash")
            .arg("--version")
            .output()
            .context("Failed to execute bash")?
            .status
            .success()
            .then_some(())
            .context("bash executable not found")?;

        Ok(Self {
            name: "bash".to_string(),
            args_prefix: vec!["-c"],
        })
    }
}

pub fn check_bash_available() -> Result<()> {
    ShellConfig::detect()?;
    Ok(())
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
    const NAME: &'static str = "bash";

    type Error = CommandError;
    type Args = CommandArgs;
    type Output = String;

    fn name(&self) -> String {
        SHELL_CONFIG.name.clone()
    }

    async fn definition(&self, prompt: String) -> ToolDefinition {
        use std::fmt::Write as _;

        let mut description = format!(
            "Execute a {} command and return the output. Use this tool to run shell commands, scripts, or system operations.",
            SHELL_CONFIG.name
        );
        if !prompt.is_empty() {
            let _ = writeln!(description);
            let _ = writeln!(description, "Additional instructions: {prompt}");
        }
        ToolDefinition {
            name: SHELL_CONFIG.name.clone(),
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
        let mut cmd_args = SHELL_CONFIG.args_prefix.clone();
        cmd_args.push(&args.command);

        let timeout_secs = args.timeout.unwrap_or(COMMAND_TIMEOUT_SECS);
        let output = timeout(
            tokio::time::Duration::from_secs(timeout_secs),
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
            Err(_) => Err(CommandError::Timeout(timeout_secs)),
        }
    }
}
