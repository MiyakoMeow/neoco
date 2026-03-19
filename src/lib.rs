//! Neoco library crate.

mod agent;
mod config;
mod events;
mod output;
mod tools;

pub use agent::chat;
pub use config::Config;
pub use events::ChatEvent;
pub use output::{EventHandler, OutputHandler};
pub use tools::check_bash_available;
