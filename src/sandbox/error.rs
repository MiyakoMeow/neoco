//! Sandbox error types

use std::fmt;

/// Errors that can occur in the sandbox
#[derive(Debug)]
pub enum SandboxError {
    /// Path traversal attempt detected
    PathTraversal(String),
    /// Access to path outside workspace
    OutsideWorkspace {
        /// The requested path
        path: String,
        /// The workspace root
        workspace: String,
    },
    /// Invalid path format
    InvalidPath(String),
    /// Execution timeout
    Timeout,
    /// Shell command contains dangerous metacharacters
    DangerousCommand(String),
    /// IO error
    Io(std::io::Error),
    /// Other errors
    Other(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::PathTraversal(path) => {
                write!(f, "Path traversal denied: '{path}' contains '..'")
            },
            SandboxError::OutsideWorkspace { path, workspace } => {
                write!(
                    f,
                    "Access denied: path '{path}' is outside workspace '{workspace}'"
                )
            },
            SandboxError::InvalidPath(msg) => write!(f, "Invalid path: {msg}"),
            SandboxError::Timeout => write!(f, "Execution timeout"),
            SandboxError::DangerousCommand(cmd) => {
                write!(f, "Dangerous shell command detected: {cmd}")
            },
            SandboxError::Io(e) => write!(f, "IO error: {e}"),
            SandboxError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SandboxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SandboxError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SandboxError {
    fn from(e: std::io::Error) -> Self {
        SandboxError::Io(e)
    }
}

impl From<String> for SandboxError {
    fn from(s: String) -> Self {
        SandboxError::Other(s)
    }
}

impl From<&str> for SandboxError {
    fn from(s: &str) -> Self {
        SandboxError::Other(s.to_string())
    }
}
