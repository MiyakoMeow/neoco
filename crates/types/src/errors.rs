//! Error types for the chat module.

use thiserror::Error;

/// Errors that can occur during chat operations.
#[derive(Debug, Error)]
pub enum ChatError {
    /// No message was provided to send.
    #[error("No message provided. Use -M/--message to send a message.")]
    NoMessage,

    /// The provider for the given model is not recognized.
    #[error("Unknown provider for model: {0}")]
    UnknownProvider(String),

    /// Failed to retrieve or validate the API key.
    #[error("API key error: {0}")]
    ApiKey(String),

    /// Failed to create a client for the provider.
    #[error("Failed to create client: {0}")]
    ClientCreation(String),

    /// JSON serialization or deserialization failed.
    #[error("JSON serialization failed: {0}")]
    JsonSerialization(#[from] serde_json::Error),

    /// An error occurred while streaming chat responses.
    #[error("Stream error: {0}")]
    Stream(String),
}
