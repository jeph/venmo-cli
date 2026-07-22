mod activity;
mod auth;
mod error;
mod json;
mod people;
mod requests;
mod shared;
mod timestamp;
mod transfers;
mod wallet;
mod writes;

pub(super) use activity::{
    write_activity_comment_removal_details, write_activity_comment_removal_result,
    write_activity_comments, write_activity_info, write_activity_list,
    write_activity_social_details, write_activity_social_result,
};
pub(super) use auth::{write_auth_status, write_logout_report, write_password_login_report};
pub use error::{write_cli_failure, write_error};
pub use json::write_parse_error_json;
pub(super) use people::{
    write_friends, write_friendship_details, write_friendship_result, write_user_info,
    write_user_search,
};
pub(super) use requests::{write_request_info, write_requests};
pub(super) use session::{OutputClass, OutputSession, PreflightMode};
pub(super) use shared::sanitize_terminal_text;
pub(super) use timestamp::TimestampFormatter;

mod session;
pub(super) use transfers::{
    write_transfer_options, write_transfer_out_details, write_transfer_out_result,
};
pub(super) use wallet::{write_balance, write_payment_methods};
pub(super) use writes::{
    write_accept_details, write_accept_result, write_cancel_details, write_cancel_result,
    write_decline_details, write_decline_result, write_dry_run_complete, write_pay_details,
    write_pay_result, write_request_create_details, write_request_create_result,
};

#[cfg(test)]
mod tests;
