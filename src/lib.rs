//! Neoco - A CLI tool for chatting with LLMs
//!
//! This crate provides the core functionality for the neoco CLI application,
//! including agent management, configuration, output handling, and tool execution.

/// Agent module for LLM interactions
pub mod agent;
/// Configuration module for managing settings
pub mod config;
/// Output module for handling console output
pub mod output;
/// Tools module for executing shell commands with sandbox security
pub mod tools;
