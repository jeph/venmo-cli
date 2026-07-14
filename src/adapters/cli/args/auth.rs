use clap::{Args, Subcommand};

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub operation: AuthOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum AuthOperation {
    /// Authenticate with a trusted device and password/SMS OTP, or import a bearer token.
    Login(LoginArgs),

    /// Replace an expired token using the stored trusted device and password/SMS OTP.
    #[command(
        after_long_help = "Uses the device ID from the one readable stored credential and never prompts for or prints it. Account identifier, password, and an SMS code for the exact recognized challenge are entered only through interactive prompts. This performs a full password login, not a token refresh."
    )]
    Reauthenticate,

    /// Remove the locally stored credential.
    Logout(LogoutArgs),

    /// Validate the credential and show the active account.
    Status,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Password login requires a v_id/device ID that Venmo already trusts. Every credential value is entered through an interactive prompt; never put a device ID, password, OTP, or bearer token in command arguments."
)]
pub struct LoginArgs {
    /// Import a bearer token and prompt for its matching device ID when needed.
    #[arg(long)]
    pub token: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct LogoutArgs {
    /// Attempt to revoke the bearer token before deleting it locally.
    #[arg(long)]
    pub revoke: bool,
}
