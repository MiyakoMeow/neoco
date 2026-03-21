//! Unified error types for neoco.
//!
//! This crate provides centralized error handling for all neoco components,
//! ensuring consistent error types and propagation across the codebase.

pub mod chat;
pub mod event;
pub mod io;

pub use chat::ChatError;
pub use event::EventError;
pub use io::IoError;

/// Unified neoco error type.
///
/// This enum encompasses all possible errors that can occur across the neoco codebase.
#[derive(Debug, thiserror::Error)]
pub enum NeocoError {
    /// Chat-related errors.
    #[error("Chat error: {0}")]
    Chat(#[from] ChatError),

    /// Event-related errors.
    #[error("Event error: {0}")]
    Event(#[from] EventError),

    /// I/O-related errors.
    #[error("I/O error: {0}")]
    Io(#[from] IoError),
}

/// Result type alias for neoco operations.
pub type Result<T> = std::result::Result<T, NeocoError>;
