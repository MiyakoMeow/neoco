//! Neoco library crate.

mod agent;
mod config;
mod errors;
mod events;
mod output;
mod tools;

pub use agent::chat;
pub use config::{Config, ConfigError};
pub use errors::ChatError;
pub use events::ChatEvent;
pub use output::{EventHandler, OutputHandler};
pub use tools::{BashError, check_bash_available};
