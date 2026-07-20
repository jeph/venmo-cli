use std::future::Future;
use std::str::FromStr;

use serde_json::Value;

use crate::features::payments::{EligibilityToken, FinancialStatus, PaymentId, PeerFundingSource};
use crate::features::people::User;
use crate::features::requests::{
    AcceptRequestPlan, AcceptedRequest, CreateRequestPlan, CreatedRequest, DeclineRequestPlan,
    DeclinedRequest, PendingRequestsPage, PendingRequestsPageRequest, RequestAcceptanceApi,
    RequestAction, RequestApprovalEligibility, RequestApprovalEligibilityApi, RequestApprovalFee,
    RequestApprovalFees, RequestApprovalNotificationApi, RequestCreationApi, RequestDeclineApi,
    RequestDirection, RequestId, RequestLookupApi, RequestNotificationId, RequestRecord,
    RequestStatus, RequestsApi, RequestsBefore,
};
use crate::shared::{AccessToken, DeviceId, Limit, Money, UserId};

use super::super::dto::{
    CreateRequestRequest, FundedRequestApproval, FundedRequestApprovalFee, PaymentEnvelope,
    PaymentRecordDto, PaymentsEnvelope, RequestApprovalEligibilityEnvelope, RequestApprovalFeeDto,
    RequestApprovalMetadata, RequestApprovalNotificationsEnvelope, UpdatePaymentRequest,
};
use super::super::transport::{ApiSession, ApiTransport, FormBody, HttpRequest, JsonBody};
use super::error::VenmoApiError;
use super::payments::{
    PeerCreation, TimestampedPaymentRecord, financial_contract_unknown, money_json_number,
    parse_created_record, require_financial_timestamp, validate_created_request,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestMutation {
    Accept,
    Decline,
}

enum RequestMutationStatus {
    Accepted(FinancialStatus),
    Declined(RequestStatus),
}

const MAX_REQUEST_APPROVAL_FEES: usize = 16;
const MAX_REQUEST_APPROVAL_FEE_FIELD_BYTES: usize = 4096;

fn map_request_approval_fees(
    fees: Option<Vec<RequestApprovalFeeDto>>,
) -> Result<RequestApprovalFees, VenmoApiError> {
    let Some(fees) = fees else {
        return Ok(RequestApprovalFees::omitted());
    };
    if fees.len() > MAX_REQUEST_APPROVAL_FEES {
        return request_approval_fee_contract_error();
    }
    let mut total_cents = 0_u64;
    let mut mapped = Vec::with_capacity(fees.len());
    for fee in fees {
        let product_uri = validate_request_approval_fee_field(fee.product_uri, false)?;
        let applied_to = validate_request_approval_fee_field(fee.applied_to, true)?;
        let fee_token = validate_request_approval_fee_field(fee.fee_token, false)?;
        let fee_percentage = fee.fee_percentage.map(|value| value.to_string());
        if fee_percentage
            .as_deref()
            .is_some_and(|value| value.starts_with('-'))
        {
            return request_approval_fee_contract_error();
        }
        total_cents = total_cents
            .checked_add(fee.calculated_fee_amount_in_cents)
            .ok_or(VenmoApiError::Contract {
                operation: REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
                problem: "the request-approval eligibility fee total overflowed",
            })?;
        mapped.push(RequestApprovalFee::new(
            product_uri,
            applied_to,
            fee_token,
            fee.base_fee_amount,
            fee_percentage,
            fee.calculated_fee_amount_in_cents,
        ));
    }
    Ok(RequestApprovalFees::present(mapped, total_cents))
}

fn validate_request_approval_fee_field(
    value: String,
    allow_whitespace: bool,
) -> Result<String, VenmoApiError> {
    if value.is_empty()
        || value.len() > MAX_REQUEST_APPROVAL_FEE_FIELD_BYTES
        || value.chars().any(char::is_control)
        || (!allow_whitespace && value.chars().any(char::is_whitespace))
    {
        return request_approval_fee_contract_error();
    }
    Ok(value)
}

fn request_approval_fee_contract_error<T>() -> Result<T, VenmoApiError> {
    Err(VenmoApiError::Contract {
        operation: REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
        problem: "the request-approval eligibility response contained invalid fee details",
    })
}

impl RequestMutation {
    const fn operation(self) -> &'static str {
        match self {
            Self::Accept => REQUEST_ACCEPTANCE_OPERATION,
            Self::Decline => REQUEST_DECLINE_OPERATION,
        }
    }

    const fn wire_action(self) -> &'static str {
        match self {
            Self::Accept => "approve",
            Self::Decline => "deny",
        }
    }

    const fn allows_response_action(self, action: RequestAction) -> bool {
        match self {
            Self::Accept => matches!(action, RequestAction::Pay | RequestAction::Charge),
            Self::Decline => matches!(action, RequestAction::Charge),
        }
    }

    fn expected_parties<'a>(
        response_action: RequestAction,
        request: &'a RequestRecord,
        account_id: &'a UserId,
    ) -> (&'a UserId, &'a UserId) {
        match response_action {
            RequestAction::Pay => (account_id, request.counterparty().user_id()),
            RequestAction::Charge => (request.counterparty().user_id(), account_id),
        }
    }

    fn validate_status(
        self,
        status: RequestStatus,
    ) -> Result<RequestMutationStatus, VenmoApiError> {
        match self {
            Self::Accept => {
                let status = match status.as_str() {
                    "settled" => FinancialStatus::Settled,
                    "pending" => FinancialStatus::Pending,
                    "held" => FinancialStatus::Held,
                    _ => {
                        return financial_contract_unknown(
                            self.operation(),
                            "the response contained an unsupported accepted-payment status",
                        );
                    }
                };
                Ok(RequestMutationStatus::Accepted(status))
            }
            Self::Decline => {
                if status.as_str() != "cancelled" {
                    return financial_contract_unknown(
                        self.operation(),
                        "the response did not prove the request reached the supported terminal state",
                    );
                }
                Ok(RequestMutationStatus::Declined(status))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestMappingContext {
    PendingList,
    Detail,
}

impl RequestMappingContext {
    const fn operation(self) -> &'static str {
        match self {
            Self::PendingList => REQUEST_LIST_OPERATION,
            Self::Detail => REQUEST_DETAIL_OPERATION,
        }
    }

    const fn allows_action(self, action: RequestAction) -> bool {
        match self {
            Self::PendingList => matches!(action, RequestAction::Charge),
            Self::Detail => true,
        }
    }

    const fn requires_pending_status(self) -> bool {
        matches!(self, Self::PendingList)
    }
}
use super::response::{decode_success, require_financial_success_json};
use super::support::{
    bounded_optional_text, map_user, parse_timestamp, require_query_value, validate_page_count,
    validate_query_keys,
};
use super::{
    REQUEST_ACCEPTANCE_OPERATION, REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
    REQUEST_APPROVAL_NOTIFICATION_OPERATION, REQUEST_CREATION_OPERATION, REQUEST_DECLINE_OPERATION,
    REQUEST_DETAIL_OPERATION, REQUEST_LIST_OPERATION, VenmoApiClient,
};

impl<T: ApiTransport> VenmoApiClient<T> {
    async fn create_peer_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &CreateRequestPlan,
    ) -> Result<CreatedRequest, VenmoApiError> {
        let creation = PeerCreation::Request;
        let request_id = plan.request_id().to_string();
        let amount = money_json_number(plan.amount(), creation)?;
        let body = JsonBody::encode(&CreateRequestRequest {
            uuid: &request_id,
            user_id: plan.recipient().user_id().as_str(),
            audience: plan.visibility().as_str(),
            amount: &amount,
            note: plan.note().as_str(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: REQUEST_CREATION_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::financial_json_post("/payments", &["payments"], &[], body),
            )
            .await?;
        let value = require_financial_success_json(creation.operation(), response)?;
        let payment = parse_created_record(creation, value)?;
        validate_created_request(
            payment,
            plan.account(),
            plan.recipient(),
            plan.amount(),
            plan.note(),
            plan.visibility(),
        )
    }

    async fn accept_incoming_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &AcceptRequestPlan,
    ) -> Result<AcceptedRequest, VenmoApiError> {
        if plan.uses_modern_funding() {
            return self
                .accept_source_funded_request(access_token, device_id, plan)
                .await;
        }
        let payment = self
            .update_incoming_request(
                access_token,
                device_id,
                plan.request().id(),
                RequestMutation::Accept,
            )
            .await?;
        validate_accepted_request(payment, plan)
    }

    async fn fetch_request_approval_notification_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        request_id: &RequestId,
    ) -> Result<RequestNotificationId, VenmoApiError> {
        const MAX_NOTIFICATIONS: usize = 200;

        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read(
                    "/notifications",
                    &["notifications"],
                    &[("acknowledged", "false")],
                ),
            )
            .await?;
        let envelope: RequestApprovalNotificationsEnvelope = decode_success(
            REQUEST_APPROVAL_NOTIFICATION_OPERATION,
            response,
            "the request-approval notification response did not match the supported envelope",
        )?;
        if envelope.data.len() > MAX_NOTIFICATIONS {
            return Err(VenmoApiError::Contract {
                operation: REQUEST_APPROVAL_NOTIFICATION_OPERATION,
                problem: "the request-approval notification response exceeded its record limit",
            });
        }
        let mut matches = envelope.data.into_iter().filter_map(|notification| {
            let payment = notification.payment?;
            (payment.id.into_string() == request_id.as_str()).then_some(notification.id)
        });
        let notification_id = match (matches.next(), matches.next()) {
            (Some(notification_id), None) => notification_id,
            (None, _) => {
                return Err(VenmoApiError::Contract {
                    operation: REQUEST_APPROVAL_NOTIFICATION_OPERATION,
                    problem: "the pending request had no matching approval notification",
                });
            }
            (Some(_), Some(_)) => {
                return Err(VenmoApiError::Contract {
                    operation: REQUEST_APPROVAL_NOTIFICATION_OPERATION,
                    problem: "the pending request had multiple matching approval notifications",
                });
            }
        };
        RequestNotificationId::from_str(&notification_id.into_string()).map_err(|_| {
            VenmoApiError::Contract {
                operation: REQUEST_APPROVAL_NOTIFICATION_OPERATION,
                problem: "the matching approval notification had an invalid ID",
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_request_approval_eligibility(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        requester: &User,
        amount_cents: u64,
        note: &str,
        funding: &PeerFundingSource,
    ) -> Result<RequestApprovalEligibility, VenmoApiError> {
        let amount = amount_cents.to_string();
        let body = FormBody::pairs(&[
            ("target_type", "user_id"),
            ("target_id", requester.user_id().as_str()),
            ("country_code", "1"),
            ("amount", amount.as_str()),
            ("note", note),
            ("funding_source_id", funding.method().id().as_str()),
        ])
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::non_financial_form_post(
                    "/protection/eligibility",
                    &["protection", "eligibility"],
                    &[],
                    body,
                ),
            )
            .await?;
        let envelope: RequestApprovalEligibilityEnvelope = decode_success(
            REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
            response,
            "the request-approval eligibility response did not match the supported envelope",
        )?;
        let eligibility = envelope.data;
        if !eligibility.eligible {
            return Err(VenmoApiError::RequestApprovalEligibilityDenied);
        }
        let fees = map_request_approval_fees(eligibility.fees)?;
        let token = eligibility
            .eligibility_token
            .ok_or(VenmoApiError::Contract {
                operation: REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
                problem: "the request-approval eligibility response omitted its token",
            })?;
        let token = EligibilityToken::parse_owned(token).map_err(|_| VenmoApiError::Contract {
            operation: REQUEST_APPROVAL_ELIGIBILITY_OPERATION,
            problem: "the request-approval eligibility response contained an invalid token",
        })?;
        Ok(RequestApprovalEligibility::new(token, fees))
    }

    async fn accept_source_funded_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &AcceptRequestPlan,
    ) -> Result<AcceptedRequest, VenmoApiError> {
        let funding = plan
            .funding_source()
            .ok_or(VenmoApiError::RequestEncoding {
                operation: REQUEST_ACCEPTANCE_OPERATION,
            })?;
        let token = plan
            .eligibility_token()
            .ok_or(VenmoApiError::RequestEncoding {
                operation: REQUEST_ACCEPTANCE_OPERATION,
            })?;
        let fees = plan.approval_fees().ok_or(VenmoApiError::RequestEncoding {
            operation: REQUEST_ACCEPTANCE_OPERATION,
        })?;
        let notification_id =
            plan.approval_notification_id()
                .ok_or(VenmoApiError::RequestEncoding {
                    operation: REQUEST_ACCEPTANCE_OPERATION,
                })?;
        let wire_fees = fees
            .entries()
            .map(|fees| {
                fees.iter()
                    .map(|fee| {
                        let fee_percentage = fee
                            .fee_percentage()
                            .map(serde_json::Number::from_str)
                            .transpose()
                            .map_err(|_| VenmoApiError::RequestEncoding {
                                operation: REQUEST_ACCEPTANCE_OPERATION,
                            })?;
                        Ok(FundedRequestApprovalFee {
                            product_uri: fee.product_uri(),
                            applied_to: fee.applied_to(),
                            fee_token: fee.fee_token(),
                            base_fee_amount: fee.base_fee_amount(),
                            fee_percentage,
                            calculated_fee_amount_in_cents: fee.calculated_fee_amount_in_cents(),
                        })
                    })
                    .collect::<Result<Vec<_>, VenmoApiError>>()
            })
            .transpose()?;
        let body = JsonBody::encode(&FundedRequestApproval {
            funding_source_id: funding.method().id().as_str(),
            eligibility_token: token.expose(),
            fees: wire_fees.as_deref(),
            metadata: RequestApprovalMetadata {
                quasi_cash_disclaimer_viewed: false,
            },
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: REQUEST_ACCEPTANCE_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::financial_json_put(
                    "/requests/{request-id}",
                    &["requests", notification_id.as_str()],
                    &[],
                    body,
                ),
            )
            .await?;
        let value = require_financial_success_json(REQUEST_ACCEPTANCE_OPERATION, response)?;
        if !value.is_object() {
            return financial_contract_unknown(
                REQUEST_ACCEPTANCE_OPERATION,
                "the successful source-funded approval response was not an object",
            );
        }
        if value.pointer("/data/url").is_some_and(|url| !url.is_null()) {
            return financial_contract_unknown(
                REQUEST_ACCEPTANCE_OPERATION,
                "the source-funded approval requires an unsupported continuation",
            );
        }
        Ok(AcceptedRequest::source_funded())
    }

    async fn decline_incoming_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &DeclineRequestPlan,
    ) -> Result<DeclinedRequest, VenmoApiError> {
        let payment = self
            .update_incoming_request(
                access_token,
                device_id,
                plan.request().id(),
                RequestMutation::Decline,
            )
            .await?;
        validate_declined_request(payment, plan)
    }

    async fn update_incoming_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        request_id: &RequestId,
        mutation: RequestMutation,
    ) -> Result<TimestampedPaymentRecord, VenmoApiError> {
        let operation = mutation.operation();
        let body = JsonBody::encode(&UpdatePaymentRequest {
            action: mutation.wire_action(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding { operation })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::financial_json_put(
                    "/payments/{payment-id}",
                    &["payments", request_id.as_str()],
                    &[],
                    body,
                ),
            )
            .await?;
        let value = require_financial_success_json(operation, response)?;
        parse_updated_payment(operation, value)
    }

    pub(super) async fn fetch_pending_requests_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        page: PendingRequestsPageRequest,
    ) -> Result<PendingRequestsPage, VenmoApiError> {
        let limit_value = page.page_size().get().to_string();
        let query = [
            ("action", "charge"),
            ("status", "pending,held"),
            ("limit", limit_value.as_str()),
        ]
        .into_iter()
        .chain(page.before().map(|before| ("before", before.as_str())))
        .collect::<Vec<_>>();
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/payments", &["payments"], &query),
            )
            .await?;
        let envelope: PaymentsEnvelope = decode_success(
            REQUEST_LIST_OPERATION,
            response,
            "the pending-request response did not match the supported envelope",
        )?;
        let requests = envelope
            .data
            .into_iter()
            .map(|payment| {
                map_request_record(payment, current_user_id, RequestMappingContext::PendingList)
            })
            .collect::<Result<Vec<_>, _>>()?;
        validate_page_count(REQUEST_LIST_OPERATION, requests.len(), page.page_size())?;
        let next_token =
            self.parse_requests_next_link(envelope.pagination.next.as_deref(), page.page_size())?;
        Ok(PendingRequestsPage::new(requests, next_token))
    }

    fn parse_requests_next_link(
        &self,
        raw: Option<&str>,
        page_size: Limit,
    ) -> Result<Option<RequestsBefore>, VenmoApiError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let pairs = self.transport.parse_trusted_next_link(raw, &["payments"])?;
        let query = validate_query_keys(
            REQUEST_LIST_OPERATION,
            &pairs,
            &["action", "before", "limit", "status"],
        )?;
        require_query_value(REQUEST_LIST_OPERATION, &query, "action", "charge")?;
        require_query_value(REQUEST_LIST_OPERATION, &query, "status", "pending,held")?;
        require_query_value(
            REQUEST_LIST_OPERATION,
            &query,
            "limit",
            &page_size.get().to_string(),
        )?;
        let before = query.get("before").ok_or(VenmoApiError::Contract {
            operation: REQUEST_LIST_OPERATION,
            problem: "the pending-request continuation omitted its before value",
        })?;
        let before = RequestsBefore::from_str(before).map_err(|_| VenmoApiError::Contract {
            operation: REQUEST_LIST_OPERATION,
            problem: "the pending-request continuation contained an invalid before value",
        })?;
        Ok(Some(before))
    }

    async fn fetch_request_by_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        request_id: &RequestId,
    ) -> Result<RequestRecord, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read(
                    "/payments/{payment-id}",
                    &["payments", request_id.as_str()],
                    &[],
                ),
            )
            .await?;
        let envelope: PaymentEnvelope = decode_success(
            REQUEST_DETAIL_OPERATION,
            response,
            "the request detail did not match the supported envelope",
        )?;
        let request = map_request_record(
            envelope.data.into_payment(),
            current_user_id,
            RequestMappingContext::Detail,
        )?;
        if request.id() != request_id {
            return Err(VenmoApiError::Contract {
                operation: REQUEST_DETAIL_OPERATION,
                problem: "the request detail returned a different request ID",
            });
        }
        Ok(request)
    }
}

impl<T: ApiTransport> RequestCreationApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn create_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a CreateRequestPlan,
    ) -> impl Future<Output = Result<CreatedRequest, Self::Error>> + Send + 'a {
        self.create_peer_request(access_token, device_id, plan)
    }
}

impl<T: ApiTransport> RequestAcceptanceApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn accept_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a AcceptRequestPlan,
    ) -> impl Future<Output = Result<AcceptedRequest, Self::Error>> + Send + 'a {
        self.accept_incoming_request(access_token, device_id, plan)
    }
}

impl<T: ApiTransport> RequestApprovalEligibilityApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn request_approval_eligibility<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        requester: &'a User,
        amount_cents: u64,
        note: &'a str,
        funding: &'a PeerFundingSource,
    ) -> impl Future<Output = Result<RequestApprovalEligibility, Self::Error>> + Send + 'a {
        self.fetch_request_approval_eligibility(
            access_token,
            device_id,
            requester,
            amount_cents,
            note,
            funding,
        )
    }
}

impl<T: ApiTransport> RequestApprovalNotificationApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn request_approval_notification_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestNotificationId, Self::Error>> + Send + 'a {
        self.fetch_request_approval_notification_id(access_token, device_id, request_id)
    }
}

impl<T: ApiTransport> RequestDeclineApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn decline_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a DeclineRequestPlan,
    ) -> impl Future<Output = Result<DeclinedRequest, Self::Error>> + Send + 'a {
        self.decline_incoming_request(access_token, device_id, plan)
    }
}

impl<T: ApiTransport> RequestsApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn pending_requests<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: PendingRequestsPageRequest,
    ) -> impl Future<Output = Result<PendingRequestsPage, Self::Error>> + Send + 'a {
        self.fetch_pending_requests_page(access_token, device_id, current_user_id, page)
    }
}

impl<T: ApiTransport> RequestLookupApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn request_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
        self.fetch_request_by_id(access_token, device_id, current_user_id, request_id)
    }
}

fn parse_updated_payment(
    operation: &'static str,
    value: Value,
) -> Result<TimestampedPaymentRecord, VenmoApiError> {
    let envelope: PaymentEnvelope =
        serde_json::from_value(value).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the successful response did not match the supported updated-payment envelope",
        })?;
    let payment = envelope.data.into_payment();
    let created_at = require_financial_timestamp(
        operation,
        payment.date_created.as_deref(),
        "the successful response omitted the original creation timestamp",
        "the successful response contained an invalid creation timestamp",
    )?;
    Ok(TimestampedPaymentRecord::new(payment, created_at))
}

fn validate_accepted_request(
    payment: TimestampedPaymentRecord,
    plan: &AcceptRequestPlan,
) -> Result<AcceptedRequest, VenmoApiError> {
    let mutation = RequestMutation::Accept;
    let operation = mutation.operation();
    let status =
        validate_updated_record(mutation, &payment, plan.request(), plan.account().user_id())?;
    let status = match status {
        RequestMutationStatus::Accepted(status) => status,
        RequestMutationStatus::Declined(_) => {
            return financial_contract_unknown(
                operation,
                "request acceptance produced a decline-status validation result",
            );
        }
    };
    let payment_id = PaymentId::from_str(&payment.payment().id.as_str()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid accepted-payment ID",
        }
    })?;
    Ok(AcceptedRequest::new(payment_id, status))
}

fn validate_declined_request(
    payment: TimestampedPaymentRecord,
    plan: &DeclineRequestPlan,
) -> Result<DeclinedRequest, VenmoApiError> {
    let mutation = RequestMutation::Decline;
    let operation = mutation.operation();
    let status =
        validate_updated_record(mutation, &payment, plan.request(), plan.account().user_id())?;
    let status = match status {
        RequestMutationStatus::Declined(status) => status,
        RequestMutationStatus::Accepted(_) => {
            return financial_contract_unknown(
                operation,
                "request decline produced an acceptance-status validation result",
            );
        }
    };
    Ok(DeclinedRequest::new(plan.request().id().clone(), status))
}

fn validate_updated_record(
    mutation: RequestMutation,
    payment: &TimestampedPaymentRecord,
    request: &RequestRecord,
    account_id: &UserId,
) -> Result<RequestMutationStatus, VenmoApiError> {
    let operation = mutation.operation();
    let record = payment.payment();
    if mutation == RequestMutation::Decline && record.id.as_str() != request.id().as_str() {
        return financial_contract_unknown(
            operation,
            "the response returned a different request ID",
        );
    }
    let response_action = match record.action.as_str() {
        "charge" => RequestAction::Charge,
        "pay" => RequestAction::Pay,
        _ => {
            return financial_contract_unknown(
                operation,
                "the response contained an unsupported request action",
            );
        }
    };
    if !mutation.allows_response_action(response_action) {
        return financial_contract_unknown(operation, "the response returned a different action");
    }
    let (expected_actor, expected_target) =
        RequestMutation::expected_parties(response_action, request, account_id);
    let amount = Money::from_str(&record.amount.as_str()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid amount",
        }
    })?;
    if amount != request.amount() {
        return financial_contract_unknown(operation, "the response returned a different amount");
    }
    if record.actor.id.as_str() != expected_actor.as_str() {
        return financial_contract_unknown(operation, "the response returned a different actor");
    }
    if record.target.user.id.as_str() != expected_target.as_str() {
        return financial_contract_unknown(operation, "the response returned a different target");
    }
    if record.note.as_deref() != request.note() {
        return financial_contract_unknown(operation, "the response returned a different note");
    }
    if record.audience.as_deref() != request.audience() {
        return financial_contract_unknown(operation, "the response returned a different audience");
    }
    let response_created_at = payment.created_at();
    let expected_created_at =
        request
            .created_at()
            .ok_or(VenmoApiError::FinancialOutcomeUnknown {
                operation,
                problem: "the mutation plan omitted the original creation timestamp",
            })?;
    match mutation {
        RequestMutation::Accept
            if response_created_at.unix_timestamp_nanos()
                < expected_created_at.unix_timestamp_nanos() =>
        {
            return financial_contract_unknown(
                operation,
                "the accepted payment predates the original request",
            );
        }
        RequestMutation::Decline
            if response_created_at.unix_timestamp_nanos()
                != expected_created_at.unix_timestamp_nanos() =>
        {
            return financial_contract_unknown(
                operation,
                "the response returned a different creation timestamp",
            );
        }
        RequestMutation::Accept | RequestMutation::Decline => {}
    }
    let status = RequestStatus::from_str(&record.status).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid request status",
        }
    })?;
    mutation.validate_status(status)
}

fn map_request_record(
    payment: PaymentRecordDto,
    current_user_id: &UserId,
    context: RequestMappingContext,
) -> Result<RequestRecord, VenmoApiError> {
    let operation = context.operation();
    let PaymentRecordDto {
        id,
        status,
        action,
        amount,
        actor,
        target,
        note,
        audience,
        date_created,
    } = payment;
    let mapped_action = match action.as_str() {
        "charge" => RequestAction::Charge,
        "pay" => RequestAction::Pay,
        _ => {
            return Err(VenmoApiError::Contract {
                operation,
                problem: "the request response contained an unsupported action",
            });
        }
    };
    if !context.allows_action(mapped_action) {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the pending-request response contained an unsupported action",
        });
    }
    let status = RequestStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: match context {
            RequestMappingContext::PendingList => {
                "the pending-request response contained an invalid status"
            }
            RequestMappingContext::Detail => {
                "the request-detail response contained an invalid status"
            }
        },
    })?;
    if context.requires_pending_status() && !status.is_pending_or_held() {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the pending-request response contained a non-pending record",
        });
    }
    let id = RequestId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: match context {
            RequestMappingContext::PendingList => {
                "the pending-request response contained an invalid request ID"
            }
            RequestMappingContext::Detail => {
                "the request-detail response contained an invalid request ID"
            }
        },
    })?;
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: match context {
            RequestMappingContext::PendingList => {
                "the pending-request response contained an invalid positive USD amount"
            }
            RequestMappingContext::Detail => {
                "the request-detail response contained an invalid positive USD amount"
            }
        },
    })?;
    let actor = map_user(actor, operation)?;
    let target = map_user(target.user, operation)?;
    let (direction, counterparty) = request_parties(actor, target, current_user_id, context)?;
    let note = bounded_optional_text(
        note,
        operation,
        match context {
            RequestMappingContext::PendingList => {
                "the pending-request response contained an oversized note"
            }
            RequestMappingContext::Detail => {
                "the request-detail response contained an oversized note"
            }
        },
    )?;
    let created_at = date_created
        .as_deref()
        .map(|value| {
            parse_timestamp(
                value,
                operation,
                match context {
                    RequestMappingContext::PendingList => {
                        "the pending-request response contained an invalid creation timestamp"
                    }
                    RequestMappingContext::Detail => {
                        "the request-detail response contained an invalid creation timestamp"
                    }
                },
            )
        })
        .transpose()?;
    Ok(RequestRecord::new(
        id,
        mapped_action,
        direction,
        counterparty,
        amount,
        note,
        created_at,
        status,
    )
    .with_audience(audience))
}

fn request_parties(
    actor: User,
    target: User,
    current_user_id: &UserId,
    context: RequestMappingContext,
) -> Result<(RequestDirection, User), VenmoApiError> {
    let actor_is_self = actor.user_id() == current_user_id;
    let target_is_self = target.user_id() == current_user_id;
    match (actor_is_self, target_is_self) {
        (true, false) => Ok((RequestDirection::Outgoing, target)),
        (false, true) => Ok((RequestDirection::Incoming, actor)),
        (true, true) | (false, false) => Err(VenmoApiError::Contract {
            operation: context.operation(),
            problem: match context {
                RequestMappingContext::PendingList => {
                    "the pending-request response did not identify exactly one active-account party"
                }
                RequestMappingContext::Detail => {
                    "the request-detail response did not identify exactly one active-account party"
                }
            },
        }),
    }
}
