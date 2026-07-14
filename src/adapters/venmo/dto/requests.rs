use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct UpdatePaymentRequest {
    pub action: &'static str,
}

#[derive(Serialize)]
pub(crate) struct CreateRequestRequest<'a> {
    pub uuid: &'a str,
    pub user_id: &'a str,
    pub audience: &'static str,
    pub amount: &'a serde_json::Number,
    pub note: &'a str,
}
