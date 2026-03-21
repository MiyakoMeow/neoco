//! Bash availability checking.

use thiserror::Error;

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
#[must_use]
pub fn get_bash_path() -> Option<String> {
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
