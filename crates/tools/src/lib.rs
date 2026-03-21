//! Tools and utilities.

pub mod bash;
pub mod shell;

pub use bash::{BashError, check_bash_available};
pub use shell::{CommandArgs, CommandError, CommandResult, ShellTool};
