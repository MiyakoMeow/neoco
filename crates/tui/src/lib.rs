//! Neoco TUI - Terminal user interface for neoco.
//!
//! This crate provides a text-based user interface for interacting with
//! the neoco AI assistant. It supports:
//! - Chat interaction with syntax highlighting
//! - Tool call display
//! - Event-driven architecture with centralized state management

// Suppress unused crate dependency warnings
#[cfg(test)]
use insta as _;
#[allow(unused_imports)]
use neoco_agent as _;
#[allow(unused_imports)]
use neoco_errors as _;
#[allow(unused_imports)]
use neoco_event as _;

pub mod app;
pub mod event;
pub mod event_stream;
pub mod frame_rate_limiter;
pub mod frame_requester;
pub mod state;
pub mod tui;
pub mod widget;

use neoco_config::Config;

// Re-export ChatMessage from neoco-event for convenience
pub use neoco_event::ChatMessage;

use crate::tui::init;

/// Run the TUI application.
///
/// This function initializes the terminal, runs the main event loop, and
/// restores the terminal on exit.
///
/// # Arguments
///
/// * `_config` - Application configuration (currently unused).
/// * `_model_string` - Model identifier string (currently unused).
///
/// # Errors
///
/// Returns an error if terminal initialization fails or if an I/O error occurs
/// during the event loop.
pub async fn run_tui(_config: &Config, _model_string: &str) -> std::io::Result<()> {
    let terminal = init()?;
    let mut tui = tui::Tui::new(terminal);

    let mut app = app::App::new();

    // Run the app
    let result = app.run(&mut tui).await;

    // Restore terminal
    let _ = tui::restore();

    result
}
