use std::future::Future;
use std::str::FromStr;

use serde_json::Value;

use crate::features::payments::{FinancialStatus, PaymentId};
use crate::features::people::User;
use crate::features::requests::{
    AcceptRequestPlan, AcceptedRequest, CreateRequestPlan, CreatedRequest, DeclineRequestPlan,
    DeclinedRequest, PendingRequestsPage, PendingRequestsPageRequest, RequestAcceptanceApi,
    RequestAction, RequestCreationApi, RequestDeclineApi, RequestDirection, RequestId,
    RequestLookupApi, RequestRecord, RequestStatus, RequestsApi, RequestsBefore,
};
use crate::shared::{AccessToken, DeviceId, Limit, Money, UserId};

use super::super::dto::{
    CreateRequestRequest, PaymentEnvelope, PaymentRecordDto, PaymentsEnvelope, UpdatePaymentRequest,
};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest, JsonBody};
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

    const fn expected_response_action(self) -> RequestAction {
        match self {
            Self::Accept => RequestAction::Pay,
            Self::Decline => RequestAction::Charge,
        }
    }

    fn expected_parties<'a>(
        self,
        request: &'a RequestRecord,
        account_id: &'a UserId,
    ) -> (&'a UserId, &'a UserId) {
        match self {
            Self::Accept => (account_id, request.counterparty().user_id()),
            Self::Decline => (request.counterparty().user_id(), account_id),
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
    REQUEST_ACCEPTANCE_OPERATION, REQUEST_CREATION_OPERATION, REQUEST_DECLINE_OPERATION,
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
            audience: "private",
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
        )
    }

    async fn accept_incoming_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &AcceptRequestPlan,
    ) -> Result<AcceptedRequest, VenmoApiError> {
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
    let (expected_actor, expected_target) = mutation.expected_parties(request, account_id);
    if record.id.as_str() != request.id().as_str() {
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
    if response_action != mutation.expected_response_action() {
        return financial_contract_unknown(operation, "the response returned a different action");
    }
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
    if response_created_at.unix_timestamp_nanos() != expected_created_at.unix_timestamp_nanos() {
        return financial_contract_unknown(
            operation,
            "the response returned a different creation timestamp",
        );
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
