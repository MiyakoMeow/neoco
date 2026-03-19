//! Neoco library crate.

mod agent;
mod config;
mod output;
mod tools;

pub use agent::chat;
pub use config::Config;
pub use output::OutputHandler;
pub use tools::check_bash_available;
