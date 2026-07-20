use std::future::Future;
use std::str::FromStr;

use serde_json::Value;
use time::OffsetDateTime;

use crate::features::auth::OtpCode;
use crate::features::payments::{
    BlankSourceEligibility, BlankSourceEligibilityApi, CreatedPayment, EligibilityToken,
    FinancialStatus, PayPlan, PaymentCreationApi, PaymentCreationOutcome, PaymentId,
    PaymentOtpVerification, PaymentStepUpApi, PaymentVerification,
};
use crate::features::people::User;
use crate::features::requests::{CreatedRequest, RequestId, RequestStatus};
use crate::shared::{AccessToken, Account, DeviceId, Money, Note, Visibility};

use super::super::dto::{
    BlankSourceEligibilityEnvelope, BlankSourceEligibilityRequest, CreatePaymentMetadata,
    CreatePaymentRequest, CreatedPaymentEnvelope, IssuePaymentOtpAction, IssuePaymentOtpEnvelope,
    PaymentOtpGraphQlRequest, PaymentOtpGraphQlVariables, PaymentOtpInput, PaymentRecordDto,
    VerifyPaymentOtpAction, VerifyPaymentOtpEnvelope,
};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest, JsonBody};
use super::error::{ApiCodeSuffix, VenmoApiError};
use super::response::{
    decode_success, is_payment_otp_step_up_required, require_financial_success_json,
};
use super::support::parse_timestamp_value;
use super::{
    BLANK_SOURCE_ELIGIBILITY_OPERATION, PAYMENT_CREATION_OPERATION, PAYMENT_OTP_ISSUE_OPERATION,
    PAYMENT_OTP_VERIFICATION_OPERATION, REQUEST_CREATION_OPERATION, VenmoApiClient,
};

const ISSUE_PAYMENT_OTP_QUERY: &str =
    "mutation SendOtp($input: SendOtpRequest!) { sendOtp(input: $input) { success } }";
const VERIFY_PAYMENT_OTP_QUERY: &str = "mutation validateOtp($input: ValidateOtpRequest!) { validateOtp(input: $input) { validated reasonCode } }";
const PAYMENT_OTP_FLOW_TYPE: &str = "P2P";
const SMS_OTP_METHOD: &str = "sms_otp";
const SMS_OTP_VERIFIED_STATUS: &str = "sms_otp_verified";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PeerCreation {
    Payment,
    Request,
}

enum PeerCreationStatus {
    Payment(FinancialStatus),
    Request(RequestStatus),
}

impl PeerCreation {
    pub(super) const fn operation(self) -> &'static str {
        match self {
            Self::Payment => PAYMENT_CREATION_OPERATION,
            Self::Request => REQUEST_CREATION_OPERATION,
        }
    }

    const fn expected_action(self) -> &'static str {
        match self {
            Self::Payment => "pay",
            Self::Request => "charge",
        }
    }

    fn amount_string(self, amount: Money) -> String {
        match self {
            Self::Payment => amount.to_string(),
            Self::Request => format!("-{amount}"),
        }
    }

    fn validate_status(self, status: &str) -> Result<PeerCreationStatus, VenmoApiError> {
        match self {
            Self::Payment => {
                let status = match status {
                    "settled" => FinancialStatus::Settled,
                    "pending" => FinancialStatus::Pending,
                    "held" => FinancialStatus::Held,
                    _ => {
                        return financial_contract_unknown(
                            self.operation(),
                            "the response contained an unsupported financial status",
                        );
                    }
                };
                Ok(PeerCreationStatus::Payment(status))
            }
            Self::Request => {
                let status = RequestStatus::from_str(status).map_err(|_| {
                    VenmoApiError::FinancialOutcomeUnknown {
                        operation: self.operation(),
                        problem: "the response contained an invalid request status",
                    }
                })?;
                if status.as_str() != "pending" {
                    return financial_contract_unknown(
                        self.operation(),
                        "the response contained an unsupported request status",
                    );
                }
                Ok(PeerCreationStatus::Request(status))
            }
        }
    }
}

pub(super) struct TimestampedPaymentRecord {
    payment: PaymentRecordDto,
    created_at: OffsetDateTime,
}

impl TimestampedPaymentRecord {
    pub(super) const fn new(payment: PaymentRecordDto, created_at: OffsetDateTime) -> Self {
        Self {
            payment,
            created_at,
        }
    }

    pub(super) const fn payment(&self) -> &PaymentRecordDto {
        &self.payment
    }

    pub(super) const fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
}

impl<T: ApiTransport> VenmoApiClient<T> {
    async fn fetch_blank_source_eligibility(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        recipient: &User,
        amount: Money,
        note: &Note,
    ) -> Result<BlankSourceEligibility, VenmoApiError> {
        let body = JsonBody::encode(&BlankSourceEligibilityRequest {
            funding_source_id: "",
            action: "pay",
            country_code: "1",
            target_type: "user_id",
            note: note.as_str(),
            target_id: recipient.user_id().as_str(),
            amount: amount.cents(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: BLANK_SOURCE_ELIGIBILITY_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::non_financial_json_post(
                    "/protection/eligibility",
                    &["protection", "eligibility"],
                    &[],
                    body,
                ),
            )
            .await?;
        let envelope: BlankSourceEligibilityEnvelope = decode_success(
            BLANK_SOURCE_ELIGIBILITY_OPERATION,
            response,
            "the payment-eligibility response did not match the supported envelope",
        )?;
        let eligibility = envelope.data;
        let _ = (&eligibility.fee_disclaimer, &eligibility.ineligible_reason);
        if !eligibility.eligible {
            return Err(VenmoApiError::EligibilityDenied);
        }
        let fee_cents = eligibility.fees.into_iter().try_fold(0_u64, |total, fee| {
            let cents = fee.calculated_cents().ok_or(VenmoApiError::Contract {
                operation: BLANK_SOURCE_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility response contained an unknown fee shape",
            })?;
            total.checked_add(cents).ok_or(VenmoApiError::Contract {
                operation: BLANK_SOURCE_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility fee total overflowed",
            })
        })?;
        let token = EligibilityToken::parse_owned(eligibility.eligibility_token).map_err(|_| {
            VenmoApiError::Contract {
                operation: BLANK_SOURCE_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility response contained an invalid token",
            }
        })?;
        Ok(BlankSourceEligibility::new(token, fee_cents))
    }

    async fn create_peer_payment(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &PayPlan,
        verification: PaymentVerification,
    ) -> Result<PaymentCreationOutcome, VenmoApiError> {
        let request_id = plan.request_id().to_string();
        let creation = PeerCreation::Payment;
        let amount = money_json_number(plan.amount(), creation)?;
        let body = JsonBody::encode(&CreatePaymentRequest {
            uuid: &request_id,
            user_id: plan.recipient().user_id().as_str(),
            audience: plan.visibility().as_str(),
            amount: &amount,
            note: plan.note().as_str(),
            eligibility_token: plan.eligibility_token().expose(),
            funding_source_id: plan.funding_source().method().id().as_str(),
            metadata: match verification {
                PaymentVerification::Unverified => None,
                PaymentVerification::SmsOtpVerified => Some(CreatePaymentMetadata {
                    verification_method: &[SMS_OTP_METHOD],
                    verification_status: SMS_OTP_VERIFIED_STATUS,
                }),
            },
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: PAYMENT_CREATION_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::financial_json_post("/payments", &["payments"], &[], body),
            )
            .await?;
        if is_payment_otp_step_up_required(&response) {
            return Ok(PaymentCreationOutcome::OtpStepUpRequired);
        }
        let value = require_financial_success_json(creation.operation(), response)?;
        let payment = parse_created_record(creation, value)?;
        validate_created_payment(
            payment,
            plan.account(),
            plan.recipient(),
            plan.amount(),
            plan.note(),
            plan.visibility(),
        )
        .map(PaymentCreationOutcome::Created)
    }

    async fn send_payment_otp(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        request_id: &crate::shared::ClientRequestId,
    ) -> Result<(), VenmoApiError> {
        let request_id = request_id.to_string();
        let body = JsonBody::encode(&PaymentOtpGraphQlRequest {
            query: ISSUE_PAYMENT_OTP_QUERY,
            variables: PaymentOtpGraphQlVariables {
                input: PaymentOtpInput {
                    flow_type: PAYMENT_OTP_FLOW_TYPE,
                    action: IssuePaymentOtpAction {
                        delivery_method: "sms",
                    },
                    uuid: &request_id,
                },
            },
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: PAYMENT_OTP_ISSUE_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::orchestration_json_post("/graphql", body),
            )
            .await?;
        let envelope: IssuePaymentOtpEnvelope = decode_success(
            PAYMENT_OTP_ISSUE_OPERATION,
            response,
            "the payment OTP issuance response did not match the supported envelope",
        )?;
        if envelope.errors.is_some() {
            return Err(VenmoApiError::ApiFailure {
                operation: PAYMENT_OTP_ISSUE_OPERATION,
                code_suffix: ApiCodeSuffix::default(),
            });
        }
        if envelope.data.is_some_and(|data| data.send_otp.success) {
            Ok(())
        } else {
            Err(VenmoApiError::Contract {
                operation: PAYMENT_OTP_ISSUE_OPERATION,
                problem: "the response did not prove that an SMS code was issued",
            })
        }
    }

    async fn validate_payment_otp(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        request_id: &crate::shared::ClientRequestId,
        otp: &OtpCode,
    ) -> Result<PaymentOtpVerification, VenmoApiError> {
        let request_id = request_id.to_string();
        let body = JsonBody::encode(&PaymentOtpGraphQlRequest {
            query: VERIFY_PAYMENT_OTP_QUERY,
            variables: PaymentOtpGraphQlVariables {
                input: PaymentOtpInput {
                    flow_type: PAYMENT_OTP_FLOW_TYPE,
                    action: VerifyPaymentOtpAction { otp: otp.expose() },
                    uuid: &request_id,
                },
            },
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: PAYMENT_OTP_VERIFICATION_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::orchestration_json_post("/graphql", body),
            )
            .await?;
        let envelope: VerifyPaymentOtpEnvelope = decode_success(
            PAYMENT_OTP_VERIFICATION_OPERATION,
            response,
            "the payment OTP verification response did not match the supported envelope",
        )?;
        if envelope.errors.is_some() {
            return Err(VenmoApiError::ApiFailure {
                operation: PAYMENT_OTP_VERIFICATION_OPERATION,
                code_suffix: ApiCodeSuffix::default(),
            });
        }
        let result =
            envelope
                .data
                .and_then(|data| data.validate_otp)
                .ok_or(VenmoApiError::Contract {
                    operation: PAYMENT_OTP_VERIFICATION_OPERATION,
                    problem: "the response omitted the OTP validation result",
                })?;
        match (result.validated, result.reason_code.as_deref()) {
            (true, _) => Ok(PaymentOtpVerification::Verified),
            (false, Some("otpIncorrect")) => Ok(PaymentOtpVerification::Incorrect),
            (false, Some("otpExpired")) => Ok(PaymentOtpVerification::Expired),
            (false, Some("otpUnexpected")) => Ok(PaymentOtpVerification::Unexpected),
            (false, Some("tooManyIncorrectAttempts")) => {
                Ok(PaymentOtpVerification::TooManyIncorrectAttempts)
            }
            _ => Err(VenmoApiError::Contract {
                operation: PAYMENT_OTP_VERIFICATION_OPERATION,
                problem: "the response contained an unsupported OTP validation result",
            }),
        }
    }
}

impl<T: ApiTransport> BlankSourceEligibilityApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn blank_source_eligibility<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        recipient: &'a User,
        amount: Money,
        note: &'a Note,
    ) -> impl Future<Output = Result<BlankSourceEligibility, Self::Error>> + Send + 'a {
        self.fetch_blank_source_eligibility(access_token, device_id, recipient, amount, note)
    }
}

impl<T: ApiTransport> PaymentCreationApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn create_payment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a PayPlan,
        verification: PaymentVerification,
    ) -> impl Future<Output = Result<PaymentCreationOutcome, Self::Error>> + Send + 'a {
        self.create_peer_payment(access_token, device_id, plan, verification)
    }
}

impl<T: ApiTransport> PaymentStepUpApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn issue_payment_otp<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        request_id: &'a crate::shared::ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.send_payment_otp(access_token, device_id, request_id)
    }

    fn verify_payment_otp<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        request_id: &'a crate::shared::ClientRequestId,
        otp: &'a OtpCode,
    ) -> impl Future<Output = Result<PaymentOtpVerification, Self::Error>> + Send + 'a {
        self.validate_payment_otp(access_token, device_id, request_id, otp)
    }
}

pub(super) fn money_json_number(
    amount: Money,
    creation: PeerCreation,
) -> Result<serde_json::Number, VenmoApiError> {
    serde_json::Number::from_str(&creation.amount_string(amount)).map_err(|_| {
        VenmoApiError::RequestEncoding {
            operation: creation.operation(),
        }
    })
}

pub(super) fn parse_created_record(
    creation: PeerCreation,
    value: Value,
) -> Result<TimestampedPaymentRecord, VenmoApiError> {
    let operation = creation.operation();
    let envelope: CreatedPaymentEnvelope =
        serde_json::from_value(value).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the successful response did not match the supported payment envelope",
        })?;
    let payment = envelope.data.payment;
    let created_at = require_financial_timestamp(
        operation,
        payment.date_created.as_deref(),
        "the successful response omitted the creation timestamp",
        "the successful response contained an invalid creation timestamp",
    )?;
    Ok(TimestampedPaymentRecord::new(payment, created_at))
}

pub(super) fn require_financial_timestamp(
    operation: &'static str,
    value: Option<&str>,
    missing_problem: &'static str,
    invalid_problem: &'static str,
) -> Result<time::OffsetDateTime, VenmoApiError> {
    let value = value.ok_or(VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: missing_problem,
    })?;
    parse_timestamp_value(value).map_err(|()| VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: invalid_problem,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_created_contract(
    creation: PeerCreation,
    payment: TimestampedPaymentRecord,
    account: &Account,
    recipient: &User,
    expected_amount: Money,
    expected_note: &Note,
    expected_visibility: Visibility,
) -> Result<(String, PeerCreationStatus), VenmoApiError> {
    let operation = creation.operation();
    let record = payment.payment();
    if record.action != creation.expected_action() {
        return financial_contract_unknown(operation, "the response returned a different action");
    }
    let amount = Money::from_str(record.amount.as_str().as_ref()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid amount",
        }
    })?;
    if amount != expected_amount {
        return financial_contract_unknown(operation, "the response returned a different amount");
    }
    if record.actor.id.as_str().as_ref() != account.user_id().as_str() {
        return financial_contract_unknown(operation, "the response returned a different actor");
    }
    if record.target.user.id.as_str().as_ref() != recipient.user_id().as_str() {
        return financial_contract_unknown(operation, "the response returned a different target");
    }
    if record.note.as_deref() != Some(expected_note.as_str()) {
        return financial_contract_unknown(operation, "the response returned a different note");
    }
    let response_visibility = record
        .audience
        .as_deref()
        .and_then(|audience| audience.parse::<Visibility>().ok());
    if !response_visibility
        .is_some_and(|visibility| visibility.is_at_least_as_restrictive_as(expected_visibility))
    {
        return financial_contract_unknown(
            operation,
            "the response did not provide a supported audience no more public than requested",
        );
    }
    Ok((
        record.id.as_str().into_owned(),
        creation.validate_status(&record.status)?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_created_payment(
    payment: TimestampedPaymentRecord,
    account: &Account,
    recipient: &User,
    expected_amount: Money,
    expected_note: &Note,
    expected_visibility: Visibility,
) -> Result<CreatedPayment, VenmoApiError> {
    let creation = PeerCreation::Payment;
    let operation = creation.operation();
    let (id, status) = validate_created_contract(
        creation,
        payment,
        account,
        recipient,
        expected_amount,
        expected_note,
        expected_visibility,
    )?;
    let status = match status {
        PeerCreationStatus::Payment(status) => status,
        PeerCreationStatus::Request(_) => {
            return financial_contract_unknown(
                operation,
                "payment creation produced a request-status validation result",
            );
        }
    };
    let id = PaymentId::from_str(&id).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: "the response contained an invalid payment ID",
    })?;
    Ok(CreatedPayment::new(id, status))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_created_request(
    payment: TimestampedPaymentRecord,
    account: &Account,
    recipient: &User,
    expected_amount: Money,
    expected_note: &Note,
    expected_visibility: Visibility,
) -> Result<CreatedRequest, VenmoApiError> {
    let creation = PeerCreation::Request;
    let operation = creation.operation();
    let (id, status) = validate_created_contract(
        creation,
        payment,
        account,
        recipient,
        expected_amount,
        expected_note,
        expected_visibility,
    )?;
    let status = match status {
        PeerCreationStatus::Request(status) => status,
        PeerCreationStatus::Payment(_) => {
            return financial_contract_unknown(
                operation,
                "request creation produced a payment-status validation result",
            );
        }
    };
    let id = RequestId::from_str(&id).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: "the response contained an invalid request ID",
    })?;
    Ok(CreatedRequest::new(id, status))
}

pub(super) fn financial_contract_unknown<T>(
    operation: &'static str,
    problem: &'static str,
) -> Result<T, VenmoApiError> {
    Err(VenmoApiError::FinancialOutcomeUnknown { operation, problem })
}
