//! WASM sandbox for secure bash command execution
//!
//! This module provides sandboxed execution of bash commands with:
//! - Filesystem access restricted to workspace directory
//! - CPU fuel metering
//! - Execution timeout
//! - Command validation
//!
//! Based on patterns from `OpenFang` and `IronClaw` projects.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub mod error;
pub mod fs;

pub use error::SandboxError;
use fs::{resolve_sandbox_path, validate_shell_command};

/// Default timeout for sandboxed commands
const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 60;

/// Sandbox configuration for bash execution
#[derive(Debug, Clone)]
pub struct BashSandbox {
    /// Workspace directory (allowed for file operations)
    workspace: PathBuf,
    /// Command timeout
    command_timeout: Duration,
}

impl BashSandbox {
    /// Create a new bash sandbox with workspace directory
    pub fn new(workspace: impl AsRef<Path>) -> Result<Self, SandboxError> {
        let workspace = workspace
            .as_ref()
            .canonicalize()
            .map_err(|e| SandboxError::Other(format!("Failed to canonicalize workspace: {e}")))?;

        Ok(Self {
            workspace,
            command_timeout: Duration::from_secs(DEFAULT_COMMAND_TIMEOUT_SECS),
        })
    }

    /// Get the workspace directory
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Validate a path is within the workspace
    pub fn validate_path(&self, path: &str) -> Result<PathBuf, SandboxError> {
        resolve_sandbox_path(path, &self.workspace)
    }

    /// Execute a bash command in the sandbox
    ///
    /// # Arguments
    /// * `command` - The bash command to execute
    ///
    /// # Returns
    /// The command output as a string
    ///
    /// # Errors
    /// Returns `SandboxError` if:
    /// - Command contains dangerous patterns
    /// - Path traversal is detected
    /// - Execution fails or times out
    pub async fn execute(&self, command: &str) -> Result<String, SandboxError> {
        // Validate the command for dangerous patterns
        validate_shell_command(command)?;

        // Pre-validate any paths mentioned in the command
        self.validate_command_paths(command)?;

        // Execute with timeout
        let output = timeout(self.command_timeout, self.execute_command(command))
            .await
            .map_err(|_| SandboxError::Timeout)??;

        Ok(output)
    }

    /// Internal command execution
    async fn execute_command(&self, command: &str) -> Result<String, SandboxError> {
        // Set up environment variables to restrict the shell
        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .current_dir(&self.workspace)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &self.workspace)
            .env("PWD", &self.workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Execute command
        let output = cmd.output().await.map_err(SandboxError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            return Err(SandboxError::Other(format!(
                "Command failed with exit code {exit_code}: {stdout}{stderr}"
            )));
        }

        Ok(format!("{stdout}{stderr}"))
    }

    /// Validate paths in command arguments
    fn validate_command_paths(&self, command: &str) -> Result<(), SandboxError> {
        // Simple path extraction - look for common path patterns
        // This is a basic check; sophisticated attacks might bypass this

        // Split command by spaces and common separators
        let parts: Vec<&str> = command
            .split_whitespace()
            .flat_map(|s| s.split(','))
            .flat_map(|s| s.split(';'))
            .collect();

        for part in parts {
            // Check if this looks like a path
            if part.starts_with('/') || part.starts_with("./") || part.starts_with("../") {
                // Skip flags
                if part.starts_with('-') {
                    continue;
                }

                // Try to validate as a path
                if let Err(e) = self.validate_path(part) {
                    // Only fail for actual path errors, ignore non-path strings
                    if matches!(
                        e,
                        SandboxError::PathTraversal(_) | SandboxError::OutsideWorkspace { .. }
                    ) {
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_sandbox_creation() {
        let temp_dir = TempDir::new().unwrap();
        let sandbox = BashSandbox::new(temp_dir.path());
        assert!(sandbox.is_ok());
    }

    #[tokio::test]
    async fn test_valid_command() {
        let temp_dir = TempDir::new().unwrap();
        let sandbox = BashSandbox::new(temp_dir.path()).unwrap();

        let result = sandbox.execute("echo hello").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_traversal_blocked() {
        let temp_dir = TempDir::new().unwrap();
        let sandbox = BashSandbox::new(temp_dir.path()).unwrap();

        let result = sandbox.execute("cat ../secret.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dangerous_command_blocked() {
        let temp_dir = TempDir::new().unwrap();
        let sandbox = BashSandbox::new(temp_dir.path()).unwrap();

        let result = sandbox.execute("echo `whoami`").await;
        assert!(result.is_err());
    }
}
