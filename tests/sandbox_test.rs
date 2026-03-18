//! Integration tests for sandbox module

use neoco::tools::sandbox::network::NetworkWhitelist;
use neoco::tools::sandbox::{Sandbox, SandboxConfig, Whitelist};
use tempfile::TempDir;

#[test]
fn test_whitelist_basic_commands() {
    let whitelist = Whitelist::default();

    // Allowed commands
    assert!(whitelist.is_allowed("git"));
    assert!(whitelist.is_allowed("cargo"));
    assert!(whitelist.is_allowed("ls"));
    assert!(whitelist.is_allowed("cat"));
    assert!(whitelist.is_allowed("echo"));

    // Disallowed commands
    assert!(!whitelist.is_allowed("unknown-command"));
    assert!(!whitelist.is_allowed("evil"));
}

#[test]
fn test_sandbox_command_validation() {
    let sandbox = Sandbox::default();

    // Valid commands
    assert!(sandbox.validate_command("git status").is_ok());
    assert!(sandbox.validate_command("ls -la").is_ok());
    assert!(sandbox.validate_command("cargo build").is_ok());

    // Invalid commands
    assert!(sandbox.validate_command("unknown-cmd").is_err());
    assert!(sandbox.validate_command("hack-the-planet").is_err());
}

#[test]
fn test_sandbox_path_traversal_blocking() {
    let sandbox = Sandbox::default();

    // Path traversal should be blocked
    assert!(sandbox.validate_command("cat ../secret.txt").is_err());
    assert!(sandbox.validate_command("ls ../../etc").is_err());
    assert!(sandbox.validate_command("cat foo/../../../passwd").is_err());
}

#[test]
fn test_sandbox_absolute_path_blocking() {
    let sandbox = Sandbox::default();

    // Absolute paths outside workspace should be blocked
    assert!(sandbox.validate_command("cat /etc/passwd").is_err());
    assert!(sandbox.validate_command("ls /home").is_err());
    assert!(sandbox.validate_command("cat /root/.ssh/id_rsa").is_err());
}

#[test]
fn test_sandbox_with_custom_workspace() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;

    let temp_path = temp_dir.path().to_path_buf();
    let config = SandboxConfig {
        workspace_dir: temp_dir.path().to_path_buf(),
        allowed_paths: vec![temp_path.clone()],
        ..Default::default()
    };

    let sandbox = Sandbox::new(config);

    // Should allow accessing files in workspace
    std::fs::write(temp_dir.path().join("test.txt"), "content")?;
    assert!(sandbox.validate_command("cat test.txt").is_ok());

    // Should allow accessing allowed paths
    assert!(
        sandbox
            .validate_path(&format!("{}/test", temp_path.display()))
            .is_ok()
    );

    // Should block other absolute paths
    assert!(sandbox.validate_path("/etc/passwd").is_err());

    Ok(())
}

#[test]
fn test_sandbox_with_extra_commands() {
    let config = SandboxConfig {
        extra_whitelist: vec!["my-custom-tool".to_string()],
        ..Default::default()
    };

    let sandbox = Sandbox::new(config);

    // Default commands still work
    assert!(sandbox.validate_command("git status").is_ok());

    // Extra command also works
    assert!(sandbox.validate_command("my-custom-tool arg1").is_ok());
}

#[test]
fn test_sandbox_edge_cases() {
    let sandbox = Sandbox::default();

    // Empty command
    assert!(sandbox.validate_command("").is_err());

    // Command with only whitespace
    assert!(sandbox.validate_command("   ").is_err());

    // Path with null byte
    assert!(sandbox.validate_path("file\0.txt").is_err());

    // Multiple path traversal
    assert!(sandbox.validate_path("a/../b/../c/../../../d").is_err());
}

#[test]
fn test_network_whitelist_default_disabled() {
    let whitelist = NetworkWhitelist::default();

    // By default, network is not restricted
    assert!(whitelist.is_host_allowed("any.host.com").is_ok());
    assert!(whitelist.validate_url("https://evil.com").is_ok());
}

#[test]
fn test_network_whitelist_when_enabled() -> Result<(), regex::Error> {
    let whitelist = NetworkWhitelist::new(
        true,
        vec![
            "github.com".to_string(),
            "*.github.com".to_string(),
            "crates.io".to_string(),
        ],
    )?;

    // Allowed hosts
    assert!(whitelist.is_host_allowed("github.com").is_ok());
    assert!(whitelist.is_host_allowed("api.github.com").is_ok());
    assert!(whitelist.is_host_allowed("crates.io").is_ok());

    // Blocked hosts
    assert!(whitelist.is_host_allowed("evil.com").is_err());
    assert!(whitelist.is_host_allowed("google.com").is_err());

    // URL validation
    assert!(
        whitelist
            .validate_url("https://github.com/user/repo")
            .is_ok()
    );
    assert!(whitelist.validate_url("https://evil.com/steal").is_err());

    Ok(())
}

#[test]
fn test_network_command_detection() {
    let whitelist = NetworkWhitelist::new(true, vec![]).unwrap();

    // Commands that typically need network
    assert!(whitelist.is_command_allowed("git clone https://github.com/..."));
    assert!(whitelist.is_command_allowed("curl https://api.example.com"));
    assert!(whitelist.is_command_allowed("go build"));
    assert!(whitelist.is_command_allowed("cargo build"));

    // Commands that don't need network
    assert!(!whitelist.is_command_allowed("ls -la"));
    assert!(!whitelist.is_command_allowed("cat file.txt"));
    assert!(!whitelist.is_command_allowed("echo hello"));
}

#[test]
fn test_sandbox_complex_commands() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let config = SandboxConfig {
        workspace_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let sandbox = Sandbox::new(config);

    // Create test files
    std::fs::write(temp_dir.path().join("file1.txt"), "content1").expect("Failed to write file1");
    std::fs::write(temp_dir.path().join("file2.txt"), "content2").expect("Failed to write file2");

    // Complex valid commands
    assert!(sandbox.validate_command("cat file1.txt file2.txt").is_ok());
    assert!(sandbox.validate_command("grep pattern file1.txt").is_ok());
    assert!(sandbox.validate_command("ls -la | grep txt").is_ok());

    // Commands with path traversal in args
    assert!(sandbox.validate_command("cat ../file1.txt").is_err());
    assert!(
        sandbox
            .validate_command("grep pattern ../../secret")
            .is_err()
    );
}

#[test]
fn test_sandbox_directory_operations() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let config = SandboxConfig {
        workspace_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let sandbox = Sandbox::new(config);

    // Create test directory
    std::fs::create_dir(temp_dir.path().join("subdir")).expect("Failed to create subdir");

    // Valid directory operations
    assert!(sandbox.validate_command("mkdir newdir").is_ok());
    assert!(sandbox.validate_command("ls subdir").is_ok());
    assert!(sandbox.validate_command("find . -name '*.txt'").is_ok());

    // Directory traversal
    assert!(sandbox.validate_command("ls ../..").is_err());
    assert!(sandbox.validate_command("mkdir /etc/newdir").is_err());
}
