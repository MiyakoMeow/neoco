//! Sandbox module for secure command execution
//!
//! This module provides filesystem sandboxing and command whitelisting
//! to safely execute shell commands with restricted access.

/// Configuration structures for sandbox
pub mod config;
/// Network access control
pub mod network;
/// Command whitelist definitions
pub mod whitelist;

use std::path::Path;
use thiserror::Error;

pub use config::{NetworkConfig, SandboxConfig};
pub use whitelist::{Whitelist, extract_command};

/// Sandbox validation errors
#[derive(Debug, Error)]
pub enum SandboxError {
    /// Command not in whitelist
    #[error("Command not in whitelist: {0}")]
    CommandNotAllowed(String),

    /// Path outside workspace
    #[error("Path outside workspace: {0}")]
    PathOutsideWorkspace(String),

    /// Invalid path
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Path traversal detected
    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    /// Symlink escape detected
    #[error("Symlink escape detected: {0}")]
    SymlinkEscape(String),
}

/// Bash command sandbox
#[derive(Debug, Clone)]
pub struct Sandbox {
    config: SandboxConfig,
    whitelist: Whitelist,
}

impl Sandbox {
    /// Create new sandbox with configuration
    #[must_use]
    pub fn new(config: SandboxConfig) -> Self {
        let whitelist = Whitelist::new(config.extra_whitelist.clone());
        Self { config, whitelist }
    }

    /// Validate a command string before execution
    ///
    /// # Errors
    /// Returns `SandboxError::CommandNotAllowed` if command is not in whitelist
    /// Returns `SandboxError::InvalidPath` if command is empty or contains null bytes
    /// Returns `SandboxError::PathTraversal` if command contains path traversal sequences
    /// Returns `SandboxError::PathOutsideWorkspace` if command references paths outside workspace
    pub fn validate_command(&self, command: &str) -> Result<(), SandboxError> {
        // Extract main command
        let main_cmd = extract_command(command)
            .ok_or_else(|| SandboxError::InvalidPath("Empty command".to_string()))?;

        // Check whitelist
        if !self.whitelist.is_allowed(&main_cmd) {
            return Err(SandboxError::CommandNotAllowed(main_cmd));
        }

        // Validate all paths in command
        self.validate_paths_in_command(command)?;

        Ok(())
    }

    /// Validate that all paths in a command are within allowed directories
    fn validate_paths_in_command(&self, command: &str) -> Result<(), SandboxError> {
        // Simple parsing: look for potential file paths
        // This is a basic implementation - in production, consider using a proper shell parser
        for word in command.split_whitespace() {
            // Skip options (start with -)
            if word.starts_with('-') {
                continue;
            }

            // Skip command name itself
            if self.whitelist.is_allowed(word) {
                continue;
            }

            // Check if it looks like a file path
            if looks_like_path(word) {
                self.validate_path(word)?;
            }
        }

        Ok(())
    }

    /// Validate a single path
    ///
    /// # Errors
    /// Returns `SandboxError::InvalidPath` if path contains null bytes
    /// Returns `SandboxError::PathTraversal` if path contains path traversal sequences
    /// Returns `SandboxError::PathOutsideWorkspace` if path resolves outside workspace
    /// Returns `SandboxError::SymlinkEscape` if path resolves to a location outside workspace via symlink
    pub fn validate_path(&self, path_str: &str) -> Result<(), SandboxError> {
        // Reject paths with null bytes
        if path_str.contains('\0') {
            return Err(SandboxError::InvalidPath("Null byte in path".to_string()));
        }

        // Check for path traversal attempts
        if path_str.contains("..") {
            return Err(SandboxError::PathTraversal(path_str.to_string()));
        }

        // Parse the path
        let path = Path::new(path_str);

        // Reject absolute paths that aren't in allowed list
        if path.is_absolute() {
            // Check if it's in allowed paths
            for allowed in &self.config.allowed_paths {
                if path.starts_with(allowed) {
                    return Ok(());
                }
            }
            return Err(SandboxError::PathOutsideWorkspace(path_str.to_string()));
        }

        // For relative paths, resolve and check
        let resolved = self.config.workspace_dir.join(path);
        self.validate_resolved_path(&resolved, path_str)
    }

    /// Validate a resolved (absolute) path
    fn validate_resolved_path(&self, resolved: &Path, original: &str) -> Result<(), SandboxError> {
        // Try to canonicalize (follows symlinks)
        let canonical = resolved
            .canonicalize()
            .unwrap_or_else(|_| resolved.to_path_buf());

        // Check workspace directory
        let workspace_canonical = self
            .config
            .workspace_dir
            .canonicalize()
            .unwrap_or_else(|_| self.config.workspace_dir.clone());

        if !canonical.starts_with(&workspace_canonical) {
            return Err(SandboxError::PathOutsideWorkspace(original.to_string()));
        }

        // Check for symlink escape
        if canonical != *resolved {
            // Path was resolved through symlink - verify it's still in workspace
            if !canonical.starts_with(&workspace_canonical) {
                return Err(SandboxError::SymlinkEscape(original.to_string()));
            }
        }

        Ok(())
    }

    /// Get the workspace directory
    #[must_use]
    pub fn workspace_dir(&self) -> &Path {
        &self.config.workspace_dir
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(SandboxConfig::default())
    }
}

/// Check if a string looks like a file path
fn looks_like_path(s: &str) -> bool {
    // Skip quoted strings (likely patterns, not paths)
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return false;
    }

    // Simple heuristic: contains / or . or is a known file extension
    s.contains('/')
        || s.contains('.')
        || std::path::Path::new(s)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        || std::path::Path::new(s)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
        || std::path::Path::new(s)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        || std::path::Path::new(s)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        || std::path::Path::new(s)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_validate_allowed_command() {
        let sandbox = Sandbox::default();
        assert!(sandbox.validate_command("git status").is_ok());
        assert!(sandbox.validate_command("ls -la").is_ok());
    }

    #[test]
    fn test_validate_disallowed_command() {
        let sandbox = Sandbox::default();
        assert!(sandbox.validate_command("unknown-cmd").is_err());
        assert!(sandbox.validate_command("evil-script").is_err());
    }

    #[test]
    fn test_validate_path_traversal() {
        let sandbox = Sandbox::default();
        assert!(sandbox.validate_path("../secret.txt").is_err());
        assert!(sandbox.validate_path("foo/../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_absolute_path_outside_workspace() {
        let sandbox = Sandbox::default();
        assert!(sandbox.validate_path("/etc/passwd").is_err());
        assert!(sandbox.validate_path("/home/user/.ssh/id_rsa").is_err());
    }

    #[test]
    fn test_validate_relative_path_inside_workspace() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let config = SandboxConfig {
            workspace_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let sandbox = Sandbox::new(config);

        // Create test file
        fs::write(temp_dir.path().join("test.txt"), "hello").expect("Failed to write file");

        assert!(sandbox.validate_path("test.txt").is_ok());
        assert!(sandbox.validate_path("./test.txt").is_ok());
    }

    #[test]
    fn test_validate_allowed_paths() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_path_buf();
        let config = SandboxConfig {
            workspace_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![temp_path.clone()],
            ..Default::default()
        };
        let sandbox = Sandbox::new(config);

        assert!(
            sandbox
                .validate_path(&format!("{}/file.txt", temp_path.display()))
                .is_ok()
        );
        assert!(sandbox.validate_path("/etc/passwd").is_err());
    }
}
