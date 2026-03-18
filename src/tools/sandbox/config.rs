use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sandbox configuration for bash tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Workspace directory - all file operations must be within this directory
    #[serde(default = "default_workspace_dir")]
    pub workspace_dir: PathBuf,

    /// Additional allowed paths outside workspace (e.g., /tmp, /dev/null)
    #[serde(default)]
    pub allowed_paths: Vec<PathBuf>,

    /// Network access settings
    #[serde(default)]
    pub network: NetworkConfig,

    /// Custom commands to add to whitelist
    #[serde(default)]
    pub extra_whitelist: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            workspace_dir: default_workspace_dir(),
            allowed_paths: vec![],
            network: NetworkConfig::default(),
            extra_whitelist: vec![],
        }
    }
}

fn default_workspace_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Network access configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Enable network whitelist (default: false)
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// Allowed hosts/patterns (e.g., "github.com", "*.example.com")
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

fn default_false() -> bool {
    false
}
