mod activity;
mod auth;
mod doctor;
mod error;
mod people;
mod requests;
mod shared;
mod wallet;
mod writes;

pub(super) use activity::{write_activity_list, write_activity_show};
pub(super) use auth::{
    write_auth_status, write_login_result, write_logout_report, write_password_login_report,
    write_reauthentication_report,
};
pub(super) use doctor::write_doctor;
pub use error::write_error;
pub(super) use people::{write_friends, write_user_search};
pub(super) use requests::write_requests;
pub(super) use shared::sanitize_terminal_text;
pub(super) use wallet::{write_balance, write_payment_methods};
pub(super) use writes::{
    write_accept_preflight, write_accept_result, write_decline_preflight, write_decline_result,
    write_pay_preflight, write_pay_result, write_request_create_result,
};

#[cfg(test)]
mod tests;
