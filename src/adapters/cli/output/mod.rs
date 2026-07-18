mod activity;
mod auth;
mod doctor;
mod error;
mod people;
mod requests;
mod shared;
mod wallet;
mod writes;

pub(super) use activity::{write_activity_info, write_activity_list};
pub(super) use auth::{write_auth_status, write_logout_report, write_password_login_report};
pub(super) use doctor::write_doctor;
pub use error::write_error;
pub(super) use people::{write_friends, write_user_info, write_user_search};
pub(super) use requests::{write_request_info, write_requests};
pub(super) use shared::sanitize_terminal_text;
pub(super) use wallet::{write_balance, write_payment_methods};
pub(super) use writes::{
    write_accept_preflight, write_accept_result, write_decline_preflight, write_decline_result,
    write_pay_preflight, write_pay_result, write_request_create_result,
};

#[cfg(test)]
mod tests;
