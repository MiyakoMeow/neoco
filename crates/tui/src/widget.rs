//! TUI widget components with trait-based architecture.
//!
//! This module provides a common [`TuiWidget`] trait for all UI components,
//! enabling consistent rendering and event handling.

mod chat;
mod highlight;
mod input;

pub use chat::ChatPane;
pub use chat::HistoryCell;
pub use input::InputPane;
pub use input::InputResult;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// Common trait for all TUI widgets.
///
/// All UI components implement this trait to provide consistent rendering
/// and event handling interfaces.
pub trait TuiWidget {
    /// Render the widget to the buffer at the given area.
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Get the desired size of the widget.
    ///
    /// Returns `(width, height)` or `None` if the widget has no preference.
    fn size(&self) -> Option<(u16, u16)> {
        None
    }
}
