use std::collections::HashSet;

/// Default whitelist of allowed bash commands
pub const DEFAULT_WHITELIST: &[&str] = &[
    // File operations
    "cat", "ls", "cp", "mv", "rm", "touch", "mkdir", "rmdir", "find", "grep", "head", "tail", "wc",
    "chmod", "chown", // Text processing
    "echo", "printf", "sed", "awk", "cut", "sort", "uniq", "tr", "xargs", "diff", "comm",
    // System info
    "pwd", "uname", "date", "id", "whoami", "hostname", "which", "type", // Compression
    "tar", "gzip", "gunzip", "zip", "unzip", "bzip2", "bunzip2", // Development tools
    "git", "cargo", "rustc", "npm", "yarn", "node", "python", "pip", "go", "javac", "java",
    // Utilities
    "tee", "basename", "dirname", "readlink", "realpath", "test", "[",
];

/// Command whitelist validator
#[derive(Debug, Clone)]
pub struct Whitelist {
    commands: HashSet<String>,
}

impl Whitelist {
    /// Create whitelist with default commands plus extras
    #[must_use]
    pub fn new(extra_commands: Vec<String>) -> Self {
        let mut commands: HashSet<String> = DEFAULT_WHITELIST
            .iter()
            .copied()
            .map(std::string::ToString::to_string)
            .collect();

        for cmd in extra_commands {
            commands.insert(cmd);
        }

        Self { commands }
    }

    /// Check if a command is allowed
    #[must_use]
    pub fn is_allowed(&self, command: &str) -> bool {
        self.commands.contains(command)
    }

    /// Get the list of allowed commands
    #[must_use]
    pub fn allowed_commands(&self) -> Vec<String> {
        self.commands.iter().cloned().collect()
    }
}

impl Default for Whitelist {
    fn default() -> Self {
        Self::new(vec![])
    }
}

/// Extract the main command from a command string
/// Handles cases like "git status" -> "git", "cargo build" -> "cargo"
#[must_use]
pub fn extract_command(command_str: &str) -> Option<String> {
    command_str
        .split_whitespace()
        .next()
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_whitelist() {
        let whitelist = Whitelist::default();
        assert!(whitelist.is_allowed("git"));
        assert!(whitelist.is_allowed("cargo"));
        assert!(whitelist.is_allowed("ls"));
        assert!(!whitelist.is_allowed("unknown-cmd"));
    }

    #[test]
    fn test_extra_commands() {
        let whitelist = Whitelist::new(vec!["custom-tool".to_string()]);
        assert!(whitelist.is_allowed("custom-tool"));
        assert!(whitelist.is_allowed("git"));
    }

    #[test]
    fn test_extract_command() {
        assert_eq!(extract_command("git status"), Some("git".to_string()));
        assert_eq!(
            extract_command("cargo build --release"),
            Some("cargo".to_string())
        );
        assert_eq!(extract_command("  ls -la  "), Some("ls".to_string()));
    }
}
