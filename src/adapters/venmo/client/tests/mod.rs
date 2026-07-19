use std::error::Error;
use std::io;
use std::str::FromStr;
use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use serde_json::Value;
use wiremock::matchers::{body_string, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::dto::StoryDto;
use super::super::transport::{
    AmbiguousWriteCause, ApiSession, ApiTransport, HttpRequest, HttpResponse, OperationClass,
    ResponseCapture, ScriptedCredentials, ScriptedRequest, ScriptedResultSnapshot,
    ScriptedTransport, ScriptedTransportSnapshot, TransportError, VenmoHttpTransport,
};
use super::activity::{ActivityMappingContext, map_activity};
use super::error::VenmoApiError;
use super::payments::{PeerCreation, money_json_number};
use super::response::sanitize_api_code;
use super::support::parse_timestamp_value;
use super::*;
use crate::features::activity::{
    ActivityBeforeId, ActivityDetailApi, ActivityDirection, ActivityId, ActivityListApi,
    ActivityPageRequest,
};
use crate::features::auth::{
    AccountPassword, CurrentAccountApi, LoginIdentifier, OtpCode, OtpSecret, PasswordLoginApi,
    PasswordLoginStart,
};
use crate::features::payments::{
    BlankSourceEligibilityApi, EligibilityToken, FinancialStatus, PayPlan, PaymentCreationApi,
    PeerFundingApi, PeerFundingFee, PeerFundingMethod, PeerFundingRole,
};
use crate::features::people::{
    FriendsApi, FriendsPageRequest, User, UserLookupApi, UserProfileKind, UserSearchApi,
    UserSearchPageRequest, UserSearchQuery,
};
use crate::features::requests::{
    AcceptRequestPlan, CreateRequestPlan, DeclineRequestPlan, PendingRequestsPageRequest,
    RequestAcceptanceApi, RequestAction, RequestCreationApi, RequestDeclineApi, RequestDirection,
    RequestId, RequestLookupApi, RequestRecord, RequestStatus, RequestsApi, RequestsBefore,
};
use crate::features::transfers::{
    TransferInstrument, TransferInstrumentId, TransferInstrumentSuffix, TransferOptionsApi,
    TransferOutCreationApi, TransferOutPlan, TransferSpeed,
};
use crate::features::wallet::{
    Balance, BalanceApi, PaymentMethod, PaymentMethodId, PaymentMethodsApi, SignedUsdAmount,
};
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, DeviceId, Limit, Money, Note, Offset, UserId,
    Username, Visibility,
};

type TestResult = Result<(), Box<dyn Error>>;

const PASSWORD_LOGIN_REQUEST_BODY: &str = concat!(
    r#"{"phone_email_or_username":"alice@example.com","client_id":"1","#,
    r#""password":"synthetic-password"}"#,
);
const SMS_OTP_REQUEST_BODY: &str = r#"{"via":"sms"}"#;
const BLANK_SOURCE_ELIGIBILITY_REQUEST_BODY: &str = concat!(
    r#"{"funding_source_id":"","action":"pay","country_code":"1","#,
    r#""target_type":"user_id","note":"Synthetic note","target_id":"456","amount":1}"#,
);
const PAYMENT_CREATION_REQUEST_BODY: &str = concat!(
    r#"{"uuid":"123e4567-e89b-12d3-a456-426614174000","user_id":"456","#,
    r#""audience":"private","amount":0.01,"note":"Synthetic note","#,
    r#""eligibility_token":"synthetic-eligibility-token","funding_source_id":"bank-1"}"#,
);
const PAYMENT_CREATION_FRIENDS_REQUEST_BODY: &str = concat!(
    r#"{"uuid":"123e4567-e89b-12d3-a456-426614174000","user_id":"456","#,
    r#""audience":"friends","amount":0.01,"note":"Synthetic note","#,
    r#""eligibility_token":"synthetic-eligibility-token","funding_source_id":"bank-1"}"#,
);
const PAYMENT_CREATION_PUBLIC_REQUEST_BODY: &str = concat!(
    r#"{"uuid":"123e4567-e89b-12d3-a456-426614174000","user_id":"456","#,
    r#""audience":"public","amount":0.01,"note":"Synthetic note","#,
    r#""eligibility_token":"synthetic-eligibility-token","funding_source_id":"bank-1"}"#,
);
const REQUEST_CREATION_REQUEST_BODY: &str = concat!(
    r#"{"uuid":"123e4567-e89b-12d3-a456-426614174000","user_id":"456","#,
    r#""audience":"private","amount":-0.01,"note":"Synthetic note"}"#,
);
const REQUEST_CREATION_FRIENDS_REQUEST_BODY: &str = concat!(
    r#"{"uuid":"123e4567-e89b-12d3-a456-426614174000","user_id":"456","#,
    r#""audience":"friends","amount":-0.01,"note":"Synthetic note"}"#,
);
const REQUEST_CREATION_PUBLIC_REQUEST_BODY: &str = concat!(
    r#"{"uuid":"123e4567-e89b-12d3-a456-426614174000","user_id":"456","#,
    r#""audience":"public","amount":-0.01,"note":"Synthetic note"}"#,
);
const SYNTHETIC_ACCESS_TOKEN: &str = "synthetic-token";
const SYNTHETIC_DEVICE_ID: &str = "synthetic-device";
const SYNTHETIC_ISSUED_TOKEN: &str = "synthetic-issued-token";
const SYNTHETIC_OTP_SECRET: &str = "synthetic-otp-secret";

mod activity;
mod auth;
mod live;
mod payments;
mod people;
mod requests;
mod response;
mod transfers;
mod wallet;

fn scripted_client(
    results: impl IntoIterator<Item = Result<HttpResponse, TransportError>>,
) -> Result<(VenmoApiClient<ScriptedTransport>, ScriptedTransport), Box<dyn Error>> {
    let transport = ScriptedTransport::new(results)?;
    Ok((VenmoApiClient::new(transport.clone()), transport))
}

fn scripted_response(
    status: u16,
    body: impl Into<Vec<u8>>,
) -> Result<HttpResponse, Box<dyn Error>> {
    Ok(HttpResponse::for_test(StatusCode::from_u16(status)?, body))
}

fn scripted_json_response(status: u16, body: Value) -> Result<HttpResponse, Box<dyn Error>> {
    scripted_response(status, serde_json::to_vec(&body)?)
}

#[derive(Debug, Eq, PartialEq)]
struct ScriptedObservation<Outcome> {
    outcome: Outcome,
    transport: ScriptedTransportSnapshot,
}

impl<Outcome> ScriptedObservation<Outcome> {
    fn expected(outcome: Outcome, transcript: Vec<ScriptedRequest>) -> Self {
        Self {
            outcome,
            transport: ScriptedTransportSnapshot::for_test(
                transcript,
                Vec::<ScriptedResultSnapshot>::new(),
                false,
            ),
        }
    }

    fn observed(outcome: Outcome, transport: &ScriptedTransport) -> Self {
        Self {
            outcome,
            transport: transport.snapshot(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ApiErrorSnapshot {
    kind: ApiFailureKind,
    detail: ApiErrorDetail,
}

#[derive(Debug, Eq, PartialEq)]
enum ApiErrorDetail {
    Transport(TransportError),
    Http {
        operation: &'static str,
        status: u16,
        rendered: String,
    },
    ApiFailure {
        operation: &'static str,
        rendered: String,
    },
    EligibilityDenied,
    MalformedJson {
        operation: &'static str,
    },
    RequestEncoding {
        operation: &'static str,
    },
    AuthenticationOutcomeUnknown {
        operation: &'static str,
    },
    FinancialOutcomeUnknown {
        operation: &'static str,
    },
    Contract {
        operation: &'static str,
    },
}

impl ApiErrorSnapshot {
    fn from_error(error: VenmoApiError) -> Self {
        let kind = error.kind();
        let rendered = error.to_string();
        let detail = match error {
            VenmoApiError::Transport(error) => ApiErrorDetail::Transport(error),
            VenmoApiError::Http {
                operation, status, ..
            }
            | VenmoApiError::AuthenticatedHttp {
                operation, status, ..
            } => ApiErrorDetail::Http {
                operation,
                status,
                rendered,
            },
            VenmoApiError::ApiFailure { operation, .. } => ApiErrorDetail::ApiFailure {
                operation,
                rendered,
            },
            VenmoApiError::EligibilityDenied => ApiErrorDetail::EligibilityDenied,
            VenmoApiError::MalformedJson { operation } => {
                ApiErrorDetail::MalformedJson { operation }
            }
            VenmoApiError::RequestEncoding { operation } => {
                ApiErrorDetail::RequestEncoding { operation }
            }
            VenmoApiError::AuthenticationOutcomeUnknown { operation, .. } => {
                ApiErrorDetail::AuthenticationOutcomeUnknown { operation }
            }
            VenmoApiError::FinancialOutcomeUnknown { operation, .. } => {
                ApiErrorDetail::FinancialOutcomeUnknown { operation }
            }
            VenmoApiError::Contract { operation, .. } => ApiErrorDetail::Contract { operation },
        };
        Self { kind, detail }
    }

    const fn contract(operation: &'static str) -> Self {
        Self {
            kind: ApiFailureKind::Contract,
            detail: ApiErrorDetail::Contract { operation },
        }
    }

    const fn malformed_json(operation: &'static str) -> Self {
        Self {
            kind: ApiFailureKind::Contract,
            detail: ApiErrorDetail::MalformedJson { operation },
        }
    }

    const fn authentication_unknown(operation: &'static str) -> Self {
        Self {
            kind: ApiFailureKind::Internal,
            detail: ApiErrorDetail::AuthenticationOutcomeUnknown { operation },
        }
    }

    const fn financial_unknown(operation: &'static str) -> Self {
        Self {
            kind: ApiFailureKind::AmbiguousWrite,
            detail: ApiErrorDetail::FinancialOutcomeUnknown { operation },
        }
    }

    const fn transport(error: TransportError, kind: ApiFailureKind) -> Self {
        Self {
            kind,
            detail: ApiErrorDetail::Transport(error),
        }
    }

    fn http(operation: &'static str, status: u16, code: Option<&str>) -> Self {
        let code_suffix = code.map_or_else(String::new, |code| format!(" (error code {code})"));
        let authenticated = if status == 401 { "authenticated " } else { "" };
        Self {
            kind: if status == 401 {
                ApiFailureKind::Authentication
            } else {
                ApiFailureKind::Rejected
            },
            detail: ApiErrorDetail::Http {
                operation,
                status,
                rendered: format!(
                    "Venmo rejected the {authenticated}{operation} request with HTTP {status}{code_suffix}"
                ),
            },
        }
    }
}

fn project_result<Value, Projection>(
    result: Result<Value, VenmoApiError>,
    project: impl FnOnce(Value) -> Projection,
) -> Result<Projection, ApiErrorSnapshot> {
    result.map(project).map_err(ApiErrorSnapshot::from_error)
}

fn authenticated_request(
    method: Method,
    route_template: &'static str,
    path_segments: &[&str],
    query: &[(&str, &str)],
    json_body: Option<&[u8]>,
    operation: OperationClass,
) -> ScriptedRequest {
    ScriptedRequest::for_test(
        ScriptedCredentials::authenticated_for_test(SYNTHETIC_ACCESS_TOKEN, SYNTHETIC_DEVICE_ID),
        method,
        route_template,
        path_segments,
        query,
        json_body,
        operation,
        ResponseCapture::None,
    )
}

fn authenticated_read_request(
    route_template: &'static str,
    path_segments: &[&str],
    query: &[(&str, &str)],
) -> ScriptedRequest {
    authenticated_request(
        Method::GET,
        route_template,
        path_segments,
        query,
        None,
        OperationClass::Read,
    )
}

fn password_login_request() -> ScriptedRequest {
    ScriptedRequest::for_test(
        ScriptedCredentials::device_for_test(SYNTHETIC_DEVICE_ID),
        Method::POST,
        "/oauth/access_token",
        &["oauth", "access_token"],
        &[],
        Some(PASSWORD_LOGIN_REQUEST_BODY.as_bytes()),
        OperationClass::AuthenticationWrite,
        ResponseCapture::OtpSecret,
    )
}

fn payment_creation_request(body: &'static str) -> ScriptedRequest {
    authenticated_request(
        Method::POST,
        "/payments",
        &["payments"],
        &[],
        Some(body.as_bytes()),
        OperationClass::FinancialWrite,
    )
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SecretSnapshot {
    IssuedToken,
    OtpSecret,
    Other,
}

impl std::fmt::Debug for SecretSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

fn issued_token_snapshot(value: &str) -> SecretSnapshot {
    if value == SYNTHETIC_ISSUED_TOKEN {
        SecretSnapshot::IssuedToken
    } else {
        SecretSnapshot::Other
    }
}

fn otp_secret_snapshot(value: &str) -> SecretSnapshot {
    if value == SYNTHETIC_OTP_SECRET {
        SecretSnapshot::OtpSecret
    } else {
        SecretSnapshot::Other
    }
}

#[derive(Debug, Eq, PartialEq)]
enum PasswordStartSnapshot {
    Authenticated(SecretSnapshot),
    OtpRequired(SecretSnapshot),
}

impl From<PasswordLoginStart> for PasswordStartSnapshot {
    fn from(start: PasswordLoginStart) -> Self {
        match start {
            PasswordLoginStart::Authenticated(token) => {
                Self::Authenticated(issued_token_snapshot(token.expose_secret()))
            }
            PasswordLoginStart::OtpRequired(secret) => {
                Self::OtpRequired(otp_secret_snapshot(secret.expose()))
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct AccountSnapshot {
    user_id: String,
    username: String,
    display_name: Option<String>,
}

impl From<Account> for AccountSnapshot {
    fn from(account: Account) -> Self {
        Self {
            user_id: account.user_id().as_str().to_owned(),
            username: account.username().as_str().to_owned(),
            display_name: account.display_name().map(str::to_owned),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct UserSnapshot {
    user_id: String,
    username: Option<String>,
    display_name: Option<String>,
    profile_kind: Option<UserProfileKind>,
    is_payable: Option<bool>,
}

impl From<&User> for UserSnapshot {
    fn from(user: &User) -> Self {
        Self {
            user_id: user.user_id().as_str().to_owned(),
            username: user.username().map(Username::as_str).map(str::to_owned),
            display_name: user.display_name().map(str::to_owned),
            profile_kind: user.profile_kind(),
            is_payable: user.is_payable(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct PaymentMethodSnapshot {
    id: String,
    name: Option<String>,
    method_type: Option<String>,
    last_four: Option<String>,
    is_default: bool,
}

impl From<&PaymentMethod> for PaymentMethodSnapshot {
    fn from(method: &PaymentMethod) -> Self {
        Self {
            id: method.id().as_str().to_owned(),
            name: method.name().map(str::to_owned),
            method_type: method.method_type().map(str::to_owned),
            last_four: method.last_four().map(str::to_owned),
            is_default: method.is_default(),
        }
    }
}

fn test_client(server: &MockServer) -> Result<VenmoApiClient, Box<dyn Error>> {
    let base_url = Url::parse(&format!("{}/v1/", server.uri()))?;
    let transport = VenmoHttpTransport::for_test(base_url, Duration::from_secs(2), 1024)?;
    Ok(VenmoApiClient::new(transport))
}

fn activity_body(story_id: &str) -> Value {
    serde_json::json!({
        "data": {
            "id": story_id,
            "date_created": "2026-07-11T12:00:00Z",
            "payment": {
                "id": "payment-1",
                "status": "failed",
                "action": "pay",
                "amount": "1.00",
                "actor": {"id": "123", "username": "alice"},
                "target": {"user": {"id": "456", "username": "bob"}},
                "date_created": "2026-07-11T12:00:00Z"
            }
        }
    })
}

fn request_body(request_id: &str, actor_id: &str, target_id: &str, status: &str) -> Value {
    serde_json::json!({
        "id": request_id,
        "status": status,
        "action": "charge",
        "amount": "0.01",
        "actor": {"id": actor_id, "username": format!("user-{actor_id}")},
        "target": {"user": {"id": target_id, "username": format!("user-{target_id}")}},
        "note": "Synthetic request",
        "audience": "private",
        "date_created": "2026-07-11T12:00:00"
    })
}

fn updated_payment_body(action: &str, status: &str, actor_id: &str, target_id: &str) -> Value {
    serde_json::json!({
        "data": {
            "id": "request-1",
            "status": status,
            "action": action,
            "amount": "0.01",
            "actor": {"id": actor_id, "username": format!("user-{actor_id}")},
            "target": {"user": {"id": target_id, "username": format!("user-{target_id}")}},
            "note": "Synthetic request",
            "audience": "private",
            "date_created": "2026-07-11T12:00:00"
        }
    })
}

fn created_payment_body(
    id: &str,
    action: &str,
    status: &str,
    actor_id: &str,
    target_id: &str,
) -> Value {
    created_payment_body_with_visibility(
        id,
        action,
        status,
        actor_id,
        target_id,
        Visibility::Private,
    )
}

fn created_payment_body_with_visibility(
    id: &str,
    action: &str,
    status: &str,
    actor_id: &str,
    target_id: &str,
    visibility: Visibility,
) -> Value {
    serde_json::json!({
        "data": {
            "payment": {
                "id": id,
                "status": status,
                "action": action,
                "amount": "0.01",
                "actor": {"id": actor_id, "username": "owner"},
                "target": {"user": {"id": target_id, "username": "bob"}},
                "note": "Synthetic note",
                "audience": visibility.as_str(),
                "date_created": "2026-07-12T12:00:00"
            }
        }
    })
}

fn financial_user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        Some("Synthetic user".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true))
}

fn test_account() -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str("123")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
    ))
}

fn zero_fee_peer_method() -> Result<PeerFundingMethod, Box<dyn Error>> {
    let method = PaymentMethod::new(
        PaymentMethodId::from_str("bank-1")?,
        Some("Synthetic bank".to_owned()),
        Some("bank".to_owned()),
        Some("1234".to_owned()),
        true,
    );
    Ok(PeerFundingMethod::new(
        method,
        PeerFundingRole::Default,
        PeerFundingFee::ProvenZero,
    ))
}

fn pay_plan() -> Result<PayPlan, Box<dyn Error>> {
    pay_plan_with_visibility(Visibility::Private)
}

fn pay_plan_with_visibility(visibility: Visibility) -> Result<PayPlan, Box<dyn Error>> {
    Ok(PayPlan::new(
        crate::shared::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?,
        test_account()?,
        financial_user("456", "bob")?,
        Money::from_cents(1)?,
        Note::from_str("Synthetic note")?,
        Balance::new(
            SignedUsdAmount::from_cents(0),
            SignedUsdAmount::from_cents(0),
        ),
        zero_fee_peer_method()?,
        0,
        EligibilityToken::parse_owned("synthetic-eligibility-token".to_owned())?,
        visibility,
    ))
}

fn request_plan() -> Result<CreateRequestPlan, Box<dyn Error>> {
    request_plan_with_visibility(Visibility::Private)
}

fn request_plan_with_visibility(
    visibility: Visibility,
) -> Result<CreateRequestPlan, Box<dyn Error>> {
    Ok(CreateRequestPlan::new(
        crate::shared::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?,
        test_account()?,
        financial_user("456", "bob")?,
        Money::from_cents(1)?,
        Note::from_str("Synthetic note")?,
        visibility,
    ))
}

fn incoming_request() -> Result<RequestRecord, Box<dyn Error>> {
    let created_at = parse_timestamp_value("2026-07-11T12:00:00")
        .map_err(|()| io::Error::other("invalid synthetic request timestamp"))?;
    Ok(RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Incoming,
        financial_user("456", "requester")?,
        Money::from_cents(1)?,
        Some("Synthetic request".to_owned()),
        Some(created_at),
        RequestStatus::from_str("pending")?,
    )
    .with_audience(Some("private".to_owned())))
}

fn accept_plan() -> Result<AcceptRequestPlan, Box<dyn Error>> {
    Ok(AcceptRequestPlan::new(
        test_account()?,
        incoming_request()?,
        Balance::new(
            SignedUsdAmount::from_cents(1),
            SignedUsdAmount::from_cents(0),
        ),
    ))
}

fn decline_plan() -> Result<DeclineRequestPlan, Box<dyn Error>> {
    Ok(DeclineRequestPlan::new(
        test_account()?,
        incoming_request()?,
    ))
}

fn test_session() -> Result<(AccessToken, DeviceId), Box<dyn Error>> {
    Ok((
        AccessToken::from_str("synthetic-token")?,
        DeviceId::from_str("synthetic-device")?,
    ))
}

async fn assert_request_count(server: &MockServer, expected: usize) {
    let requests = server.received_requests().await;
    assert!(
        requests
            .as_ref()
            .is_some_and(|requests| requests.len() == expected),
        "expected {expected} captured request(s), got {}",
        requests.as_ref().map_or(0, Vec::len)
    );
}

async fn assert_requests_have_no_query(server: &MockServer) {
    let requests = server.received_requests().await;
    assert!(
        requests
            .as_ref()
            .is_some_and(|requests| requests.iter().all(|request| request.url.query().is_none())),
        "expected every captured request URL to omit a query"
    );
}
