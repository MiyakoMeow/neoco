//! Neoco types library - events and errors.

pub mod errors;
pub mod events;

use events::ChatEvent as Event;

/// Trait for handling chat events.
pub trait EventHandler {
    /// Handle a chat event.
    fn handle(&self, event: Event);
}
