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
    AcceptArgs, ActivityArgs, ActivityCommentAddArgs, ActivityCommentListArgs,
    ActivityCommentRemoveArgs, ActivityCommentsArgs, ActivityCommentsOperation, ActivityInfoArgs,
    ActivityLikeArgs, ActivityListArgs, ActivityOperation, ActivityUnlikeArgs, AuthArgs,
    AuthOperation, CancelArgs, Cli, Command, DeclineArgs, FriendAddArgs, FriendRemoveArgs,
    FriendsArgs, FriendsListArgs, FriendsOperation, PayArgs, PayOperation, PayUserArgs,
    RequestArgs, RequestDirectionArg, RequestInfoArgs, RequestsArgs, RequestsListArgs,
    RequestsOperation, TransferAmountArg, TransferArgs, TransferOperation, TransferOutArgs,
    TransferSpeedArg, UserInfoArgs, UserSearchArgs, UsersArgs, UsersOperation, VisibilityArg,
};
pub use crate::adapters::cli::{
    AppError, CliFailure, CommandId, ErrorCategory, OutputFormat,
    handle_runtime_initialization_failure, run, write_cli_failure, write_error,
    write_parse_error_json,
};
