use clap::{Args, Subcommand};

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub operation: AuthOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum AuthOperation {
    /// Authenticate with a newly supplied trusted device and password/SMS OTP.
    #[command(
        after_long_help = "Always prompts interactively for the account identifier, hidden password, and a newly supplied trusted v_id/device ID, followed by hidden SMS OTP only when Venmo challenges. A validated login may replace an existing credential; failure before storage leaves the existing entry untouched."
    )]
    Login,

    /// Remove the local credential without revoking the remote token.
    #[command(
        after_long_help = "Deletes only the local keyring entry. It does not contact Venmo or revoke the remote bearer token."
    )]
    Logout,

    /// Validate the credential and show the active account.
    Status,
}
