//! Neoco events library - event system for neoco.
//!
//! This crate provides the event system for neoco, including:
//! - Unified event bus for all event types
//! - Terminal event handling
//! - Event types and error handling
//! - Message types for chat and tool interactions

// Suppress unused crate dependency warning for tokio_stream
#[allow(unused_imports)]
use tokio_stream as _;

pub mod event;
pub mod event_bus;
pub mod message;

pub use event::{ChatEvent, TerminalEvent, UIEvent, UnifiedEvent};
pub use event_bus::EventBus;
pub use message::ChatMessage;
