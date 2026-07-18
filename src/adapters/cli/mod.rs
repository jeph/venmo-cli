pub mod args;
mod dispatch;
mod error;
mod logging;
mod output;
mod prompt;

pub use dispatch::{handle_runtime_initialization_failure, run};
pub use error::{AppError, ErrorCategory};
pub use output::write_error;
