//! Filesystem sandboxing utilities
//!
//! Provides path validation and resolution to ensure all filesystem
//! operations stay within a designated workspace directory.

use super::error::SandboxError;
use std::path::{Component, Path, PathBuf};

/// Validates and resolves a path within a workspace
///
/// # Arguments
/// * `user_path` - The path provided by the user/WASM module
/// * `workspace_root` - The allowed workspace directory
///
/// # Returns
/// The canonicalized, verified path within the workspace
///
/// # Errors
/// Returns `SandboxError` if:
/// - Path contains `..` components (traversal attempt)
/// - Path resolves outside the workspace
/// - Path is invalid
pub fn resolve_sandbox_path(
    user_path: &str,
    workspace_root: &Path,
) -> Result<PathBuf, SandboxError> {
    // Reject null bytes
    if user_path.contains('\0') {
        return Err(SandboxError::InvalidPath(
            "Path contains null bytes".to_string(),
        ));
    }

    let path = Path::new(user_path);

    // Phase 1: Reject any `..` components
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(SandboxError::PathTraversal(user_path.to_string()));
        }
    }

    // Phase 2: Build the candidate path
    let candidate = if path.is_absolute() {
        // For absolute paths, check if they're within workspace
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    // Phase 3: Canonicalize to resolve symlinks and normalize
    let canon_candidate = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|e| SandboxError::Other(format!("Cannot resolve path '{user_path}': {e}")))?
    } else {
        // For non-existent paths, canonicalize the parent and join the filename
        let parent = candidate
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                SandboxError::InvalidPath(format!("Path has no parent directory: {user_path}"))
            })?;

        let canon_parent = parent.canonicalize().map_err(|e| {
            SandboxError::Other(format!(
                "Cannot resolve parent directory of '{user_path}': {e}"
            ))
        })?;

        let file_name = candidate.file_name().ok_or_else(|| {
            SandboxError::InvalidPath(format!("Invalid path (no filename): {user_path}"))
        })?;

        canon_parent.join(file_name)
    };

    // Phase 4: Verify the canonical path is inside the workspace
    let canon_workspace = workspace_root
        .canonicalize()
        .map_err(|e| SandboxError::Other(format!("Cannot canonicalize workspace root: {e}")))?;

    if !canon_candidate.starts_with(&canon_workspace) {
        return Err(SandboxError::OutsideWorkspace {
            path: user_path.to_string(),
            workspace: workspace_root.display().to_string(),
        });
    }

    Ok(canon_candidate)
}

/// Check if a command string contains shell metacharacters that could
/// be used to bypass sandbox restrictions
pub fn contains_dangerous_metacharacters(command: &str) -> Option<String> {
    // Command substitution (could execute arbitrary commands)
    if command.contains('`') {
        return Some("backtick command substitution".to_string());
    }
    if command.contains("$(") {
        return Some("$() command substitution".to_string());
    }

    // Variable expansion that could contain path traversal
    if command.contains("${") {
        return Some("${} variable expansion".to_string());
    }

    // Path-like patterns that might indicate escape attempts
    if command.contains("../") || command.contains("..\\") {
        return Some("path traversal sequence".to_string());
    }

    // Null byte injection
    if command.contains('\0') {
        return Some("null byte".to_string());
    }

    None
}

/// Validates that a shell command is safe to execute
///
/// Checks for:
/// - Dangerous metacharacters
/// - Path traversal attempts in the command itself
pub fn validate_shell_command(command: &str) -> Result<(), SandboxError> {
    // Check for dangerous metacharacters
    if let Some(reason) = contains_dangerous_metacharacters(command) {
        return Err(SandboxError::DangerousCommand(format!(
            "Command contains dangerous pattern: {reason}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_valid_path() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create a test file
        let test_file = workspace.join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        let result = resolve_sandbox_path("test.txt", workspace);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_file.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_traversal_denied() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        let result = resolve_sandbox_path("../secret.txt", workspace);
        assert!(matches!(result, Err(SandboxError::PathTraversal(_))));
    }

    #[test]
    fn test_resolve_outside_workspace() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create a directory outside the workspace
        let outside_dir = TempDir::new().unwrap();
        let outside_path = outside_dir.path().join("test.txt");
        std::fs::write(&outside_path, "test").unwrap();

        // Try to access a file outside the workspace using absolute path
        let result = resolve_sandbox_path(outside_path.to_str().unwrap(), workspace);
        assert!(matches!(result, Err(SandboxError::OutsideWorkspace { .. })));
    }

    #[test]
    fn test_dangerous_metacharacters() {
        assert!(contains_dangerous_metacharacters("`whoami`").is_some());
        assert!(contains_dangerous_metacharacters("$(whoami)").is_some());
        assert!(contains_dangerous_metacharacters("${HOME}").is_some());
        assert!(contains_dangerous_metacharacters("cat ../secret").is_some());
        assert!(contains_dangerous_metacharacters("echo hello").is_none());
    }
}
