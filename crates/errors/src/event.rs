//! Event-related error types.

use thiserror::Error;

/// Errors that can occur in the event system.
#[derive(Debug, Error)]
pub enum EventError {
    /// Event bus send error.
    #[error("Event bus send error: {0}")]
    EventBusSend(String),

    /// Event bus receive error.
    #[error("Event bus receive error: {0}")]
    EventBusRecv(String),

    /// Terminal I/O error.
    #[error("Terminal I/O error: {0}")]
    Terminal(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_error_display() {
        let err = EventError::EventBusSend("test error".to_string());
        assert!(err.to_string().contains("test error"));
    }
}
