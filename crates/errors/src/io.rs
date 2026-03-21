//! I/O-related error types.

use thiserror::Error;

/// Errors that can occur during I/O operations.
#[derive(Debug, Error)]
pub enum IoError {
    /// Failed to read input.
    #[error("Failed to read input: {0}")]
    ReadInput(String),

    /// Failed to write output.
    #[error("Failed to write output: {0}")]
    WriteOutput(String),

    /// Terminal operation error.
    #[error("Terminal error: {0}")]
    Terminal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_display() {
        let err = IoError::ReadInput("test error".to_string());
        assert!(err.to_string().contains("test error"));
    }
}
