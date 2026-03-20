//! Rendering abstraction layer.

pub mod cli;
pub mod tui;

use crate::errors::RenderError;
use crate::events::ChatEvent;

/// Trait for rendering chat events to different backends.
pub trait Renderer {
    /// Render a single chat event.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    fn render_event(&mut self, event: &ChatEvent) -> Result<(), RenderError>;

    /// Render a streaming chunk of text.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    fn render_chunk(&mut self, chunk: &str, is_thinking: bool) -> Result<(), RenderError>;

    /// Clear the output area (used for TUI redraws).
    ///
    /// # Errors
    ///
    /// Returns an error if clearing fails.
    fn clear(&mut self) -> Result<(), RenderError>;

    /// Flush the output.
    ///
    /// # Errors
    ///
    /// Returns an error if flushing fails.
    fn flush(&mut self) -> Result<(), RenderError>;

    /// Shutdown the renderer and clean up resources.
    ///
    /// # Errors
    ///
    /// Returns an error if shutdown fails.
    fn shutdown(mut self) -> Result<(), RenderError>
    where
        Self: Sized,
    {
        self.flush()
    }
}
