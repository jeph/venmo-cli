//! Public terminal-frontend facade.
//!
//! Concrete composition, prompting, rendering, and service adapters remain
//! crate-private under `adapters`; this module exposes only the process and
//! Clap-schema surface needed by the binary and frontend tests.
//!
//! Release gates, terminal-capability injection, and raw logging initialization are intentionally
//! not public API:
//!
//! ```compile_fail
//! use venmo_cli::cli::ReleaseGates;
//! ```
//!
//! ```compile_fail
//! use venmo_cli::cli::TerminalCapabilities;
//! ```
//!
//! ```compile_fail
//! use venmo_cli::cli::initialize_logging;
//! ```

pub use crate::adapters::cli::args::{
    AcceptArgs, ActivityArgs, ActivityListArgs, ActivityOperation, ActivityShowArgs, AuthArgs,
    AuthOperation, Cli, Command, CompletionShell, CompletionsArgs, DeclineArgs, FriendsArgs,
    FriendsListArgs, FriendsOperation, LoginArgs, LogoutArgs, PayArgs, PaymentMethodsArgs,
    PaymentMethodsOperation, RequestArgs, RequestDirectionArg, RequestsArgs, RequestsListArgs,
    RequestsOperation, UserSearchArgs, UsersArgs, UsersOperation,
};
pub use crate::adapters::cli::{
    AppError, ErrorCategory, handle_runtime_initialization_failure, run, write_error,
};
