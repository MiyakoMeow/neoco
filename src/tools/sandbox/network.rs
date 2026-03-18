//! Network access whitelist for sandbox
//!
//! This module provides optional network access controls.
//! Disabled by default - only active when explicitly enabled.

#![allow(dead_code)]

use std::collections::HashSet;
use thiserror::Error;

/// Network access validation errors
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum NetworkError {
    /// Network access disabled
    #[error("Network access disabled")]
    AccessDisabled,

    /// Host not in whitelist
    #[error("Host not in whitelist: {0}")]
    HostNotAllowed(String),

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

/// Network whitelist validator
#[derive(Debug, Clone, Default)]
pub struct NetworkWhitelist {
    enabled: bool,
    allowed_hosts: Vec<regex::Regex>,
}

impl NetworkWhitelist {
    /// Create new network whitelist
    ///
    /// When enabled is false, all network access is allowed (default behavior)
    ///
    /// # Errors
    /// Returns `regex::Error` if any of the patterns are invalid
    pub fn new(enabled: bool, patterns: Vec<String>) -> Result<Self, regex::Error> {
        let allowed_hosts = if enabled {
            patterns
                .into_iter()
                .map(|p| regex::Regex::new(&glob_to_regex(&p)))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };

        Ok(Self {
            enabled,
            allowed_hosts,
        })
    }

    /// Check if network access is allowed for a given host
    ///
    /// # Errors
    /// Returns `NetworkError::AccessDisabled` if network whitelist is disabled
    /// Returns `NetworkError::HostNotAllowed` if host is not in whitelist
    pub fn is_host_allowed(&self, host: &str) -> Result<(), NetworkError> {
        if !self.enabled {
            // Whitelist not enabled - allow all
            return Ok(());
        }

        for pattern in &self.allowed_hosts {
            if pattern.is_match(host) {
                return Ok(());
            }
        }

        Err(NetworkError::HostNotAllowed(host.to_string()))
    }

    /// Check if a command is allowed network access
    #[must_use]
    pub fn is_command_allowed(&self, command: &str) -> bool {
        if !self.enabled {
            return true;
        }

        // Extract the main command
        let main_cmd = command.split_whitespace().next().unwrap_or("");

        // Commands that commonly need network access
        let network_commands: HashSet<&str> = [
            "curl", "wget", "git", "npm", "yarn", "cargo", "pip", "python", "node",
        ]
        .iter()
        .copied()
        .collect();

        network_commands.contains(main_cmd)
    }

    /// Validate a URL against the whitelist
    ///
    /// # Errors
    /// Returns `NetworkError::InvalidUrl` if the URL is malformed
    /// Returns `NetworkError::HostNotAllowed` if the host is not in whitelist
    pub fn validate_url(&self, url: &str) -> Result<(), NetworkError> {
        if !self.enabled {
            return Ok(());
        }

        // Simple URL parsing to extract host
        let host =
            extract_host_from_url(url).ok_or_else(|| NetworkError::InvalidUrl(url.to_string()))?;

        self.is_host_allowed(&host)
    }
}

/// Convert glob pattern to regex
/// e.g., "*.example.com" -> r"^.*\.example\.com$"
fn glob_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' => regex.push_str("\\."),
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
}

/// Extract host from URL
fn extract_host_from_url(url: &str) -> Option<String> {
    // Remove protocol prefix
    let without_protocol = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("ftp://"))
        .unwrap_or(url);

    // Extract host (everything before first / or :)
    without_protocol
        .split('/')
        .next()
        .and_then(|host_port| host_port.split(':').next().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_whitelist_disabled() {
        let whitelist = NetworkWhitelist::default();
        // When disabled, all hosts are allowed
        assert!(whitelist.is_host_allowed("any.host.com").is_ok());
        assert!(whitelist.validate_url("https://example.com").is_ok());
    }

    #[test]
    fn test_network_whitelist_enabled() -> Result<(), regex::Error> {
        let whitelist = NetworkWhitelist::new(
            true,
            vec!["github.com".to_string(), "*.example.com".to_string()],
        )?;

        assert!(whitelist.is_host_allowed("github.com").is_ok());
        assert!(whitelist.is_host_allowed("api.example.com").is_ok());
        assert!(whitelist.is_host_allowed("sub.example.com").is_ok());
        assert!(whitelist.is_host_allowed("other.com").is_err());

        Ok(())
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host_from_url("https://github.com/user/repo"),
            Some("github.com".to_string())
        );
        assert_eq!(
            extract_host_from_url("http://api.example.com:8080/path"),
            Some("api.example.com".to_string())
        );
        assert_eq!(
            extract_host_from_url("ftp://files.server.com"),
            Some("files.server.com".to_string())
        );
    }

    #[test]
    fn test_glob_to_regex() {
        assert_eq!(glob_to_regex("github.com"), "^github\\.com$");
        assert_eq!(glob_to_regex("*.example.com"), "^.*\\.example\\.com$");
        assert_eq!(glob_to_regex("api?.test.com"), "^api.\\.test\\.com$");
    }

    #[test]
    fn test_command_network_check() {
        let whitelist = NetworkWhitelist::new(true, vec![]).unwrap();

        assert!(whitelist.is_command_allowed("git clone ..."));
        assert!(whitelist.is_command_allowed("curl https://..."));
        assert!(whitelist.is_command_allowed("npm install"));
        assert!(!whitelist.is_command_allowed("ls -la"));
        assert!(!whitelist.is_command_allowed("cat file.txt"));
    }
}
