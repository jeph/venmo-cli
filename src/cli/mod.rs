//! Public terminal-frontend facade.
//!
//! Concrete composition, prompting, rendering, and service adapters remain
//! crate-private under `adapters`; this module exposes only the process and
//! Clap-schema surface needed by the binary and frontend tests.
//!
//! Terminal-capability injection and raw logging initialization are intentionally not public API:
//!
//! ```compile_fail
//! use venmo_cli::cli::TerminalCapabilities;
//! ```
//!
//! ```compile_fail
//! use venmo_cli::cli::initialize_logging;
//! ```

pub use crate::adapters::cli::args::{
    AcceptArgs, ActivityArgs, ActivityInfoArgs, ActivityListArgs, ActivityOperation, AuthArgs,
    AuthOperation, Cli, Command, DeclineArgs, FriendAddArgs, FriendRemoveArgs, FriendsArgs,
    FriendsListArgs, FriendsOperation, PayArgs, PayOperation, PayUserArgs, RequestArgs,
    RequestDirectionArg, RequestInfoArgs, RequestsArgs, RequestsListArgs, RequestsOperation,
    TransferAmountArg, TransferArgs, TransferOperation, TransferOutArgs, TransferSpeedArg,
    UserInfoArgs, UserSearchArgs, UsersArgs, UsersOperation, VisibilityArg,
};
pub use crate::adapters::cli::{
    AppError, ErrorCategory, handle_runtime_initialization_failure, run, write_error,
};
