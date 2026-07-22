pub mod args;
mod command;
mod dispatch;
mod error;
mod failure;
mod logging;
mod output;
mod prompt;
mod response;

pub use command::{CommandId, OutputFormat};
pub use dispatch::{handle_runtime_initialization_failure, run};
pub use error::{AppError, ErrorCategory};
pub use failure::CliFailure;
pub use output::{write_cli_failure, write_error};
