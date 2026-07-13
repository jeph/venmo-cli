use std::collections::HashSet;
use std::future::Future;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde_json::Value;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, PrimitiveDateTime};

use super::dto::{
    AccountEnvelope, AuthorizationDto, BalanceEnvelope, CreatePaymentRequest, CreateRequestRequest,
    FriendsEnvelope, PasswordLoginRequest, PaymentEligibilityEnvelope, PaymentEligibilityRequest,
    PaymentMethodDto, PaymentMethodsEnvelope, PaymentRecordDto, PaymentsEnvelope, SmsOtpRequest,
    StoriesEnvelope, StoryDto, StoryEnvelope, TransferDto, UpdatePaymentRequest, UserDto,
    UserEnvelope, UsersEnvelope,
};
use super::transport::{
    ApiSession, HttpRequest, HttpResponse, TransportBuildError, TransportError, VenmoHttpTransport,
};
use crate::application::ports::{
    ActivityApi, ActivityPage, ActivityPageRequest, ActivityPageToken, ApiFailure, ApiFailureKind,
    AuthenticationApi, BalanceApi, DoctorApi, FriendsApi, FriendsPage, FriendsPageRequest,
    FriendsPageToken, PasswordLoginApi, PaymentCreationApi, PaymentEligibility,
    PaymentEligibilityApi, PaymentMethodsApi, PeerFundingApi, PendingRequestsPage,
    PendingRequestsPageRequest, PendingRequestsPageToken, RequestAcceptanceApi, RequestCreationApi,
    RequestDeclineApi, RequestLookupApi, RequestsApi, RequiredShape, ShapeProbeOutcome,
    UserSearchPage, UserSearchPageRequest, UserSearchPageToken, UsersApi,
};
use crate::domain::{
    AcceptRequestPlan, AcceptedRequest, AccessToken, Account, AccountPassword, Activity,
    ActivityAction, ActivityBeforeId, ActivityCounterparty, ActivityDirection, ActivityId,
    ActivityStatus, Balance, CreateRequestPlan, CreatedPayment, DeclineRequestPlan,
    DeclinedRequest, DeviceId, EligibilityToken, FinancialStatus, LoginIdentifier, Money, OtpCode,
    OtpSecret, PasswordLoginStart, PayPlan, PaymentId, PaymentMethod, PaymentMethodId,
    PeerFundingFee, PeerFundingMethod, PeerFundingRole, PendingRequest, PendingRequestAction,
    RequestDirection, RequestId, RequestStatus, RequestsBefore, SignedUsdAmount, User, UserId,
    UserProfileKind, UserSearchQuery, Username,
};

const ACTIVITY_DETAIL_OPERATION: &str = "activity detail";
const ACTIVITY_LIST_OPERATION: &str = "activity listing";
const BALANCE_OPERATION: &str = "wallet balance";
const CURRENT_ACCOUNT_OPERATION: &str = "current account";
const DEVICE_TRUST_OPERATION: &str = "device trust";
const FRIENDS_OPERATION: &str = "friend listing";
const OTP_COMPLETION_OPERATION: &str = "OTP login completion";
const OTP_REQUEST_OPERATION: &str = "SMS OTP request";
const PASSWORD_LOGIN_OPERATION: &str = "password login";
#[allow(dead_code)]
const PAYMENT_CREATION_OPERATION: &str = "payment creation";
#[allow(dead_code)]
const PAYMENT_ELIGIBILITY_OPERATION: &str = "payment eligibility";
const PAYMENT_METHODS_OPERATION: &str = "payment-method listing";
#[allow(dead_code)]
const PEER_FUNDING_OPERATION: &str = "peer funding-method listing";
const REVOKE_TOKEN_OPERATION: &str = "token revocation";
#[allow(dead_code)]
const REQUEST_CREATION_OPERATION: &str = "request creation";
const REQUEST_ACCEPTANCE_OPERATION: &str = "request acceptance";
const REQUEST_DECLINE_OPERATION: &str = "request decline";
const REQUEST_DETAIL_OPERATION: &str = "pending-request detail";
const REQUEST_LIST_OPERATION: &str = "pending-request listing";
const USER_LOOKUP_OPERATION: &str = "user lookup";
const USER_SEARCH_OPERATION: &str = "user search";
const MAX_REMOTE_TEXT_BYTES: usize = 64 * 1024;

pub struct VenmoApiClient {
    transport: VenmoHttpTransport,
}

impl VenmoApiClient {
    pub fn production() -> Result<Self, TransportBuildError> {
        VenmoHttpTransport::production().map(Self::new)
    }

    #[must_use]
    pub(super) fn new(transport: VenmoHttpTransport) -> Self {
        Self { transport }
    }

    async fn start_password_login(
        &self,
        identifier: &LoginIdentifier,
        password: &AccountPassword,
        device_id: &DeviceId,
    ) -> Result<PasswordLoginStart, VenmoApiError> {
        let body = serde_json::to_vec(&PasswordLoginRequest {
            phone_email_or_username: identifier.expose(),
            client_id: "1",
            password: password.expose(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: PASSWORD_LOGIN_OPERATION,
        })?;
        let response = self
            .transport
            .send_with_device_id(
                device_id,
                HttpRequest::password_login_json_post(
                    "/oauth/access_token",
                    &["oauth", "access_token"],
                    &[],
                    body,
                ),
            )
            .await?;
        let error_code = parse_response_json(&response)
            .as_ref()
            .and_then(extract_error_code);
        if error_code.as_deref() == Some("81109") {
            let raw_secret = response.otp_secret().ok_or(VenmoApiError::Contract {
                operation: PASSWORD_LOGIN_OPERATION,
                problem: "the OTP challenge omitted the required OTP secret",
            })?;
            let secret = std::str::from_utf8(raw_secret).map_err(|_| VenmoApiError::Contract {
                operation: PASSWORD_LOGIN_OPERATION,
                problem: "the OTP challenge returned an invalid OTP secret",
            })?;
            let secret =
                OtpSecret::parse_owned(secret.to_owned()).map_err(|_| VenmoApiError::Contract {
                    operation: PASSWORD_LOGIN_OPERATION,
                    problem: "the OTP challenge returned an invalid OTP secret",
                })?;
            return Ok(PasswordLoginStart::OtpRequired(secret));
        }

        let value = require_issued_token_json(PASSWORD_LOGIN_OPERATION, response)?;
        extract_access_token(PASSWORD_LOGIN_OPERATION, &value)
            .map(PasswordLoginStart::Authenticated)
    }

    async fn send_sms_otp(
        &self,
        otp_secret: &OtpSecret,
        device_id: &DeviceId,
    ) -> Result<(), VenmoApiError> {
        let body = serde_json::to_vec(&SmsOtpRequest { via: "sms" }).map_err(|_| {
            VenmoApiError::RequestEncoding {
                operation: OTP_REQUEST_OPERATION,
            }
        })?;
        let response = self
            .transport
            .send_with_otp_secret(
                device_id,
                otp_secret,
                HttpRequest::non_financial_json_post(
                    "/account/two-factor/token",
                    &["account", "two-factor", "token"],
                    &[],
                    body,
                ),
            )
            .await?;
        require_success(OTP_REQUEST_OPERATION, response)
    }

    async fn finish_otp_login(
        &self,
        otp_code: &OtpCode,
        otp_secret: &OtpSecret,
        device_id: &DeviceId,
    ) -> Result<AccessToken, VenmoApiError> {
        let response = self
            .transport
            .send_with_otp_code(
                device_id,
                otp_secret,
                otp_code,
                HttpRequest::authentication_post(
                    "/oauth/access_token",
                    &["oauth", "access_token"],
                    &[("client_id", "1")],
                ),
            )
            .await?;
        let value = require_issued_token_json(OTP_COMPLETION_OPERATION, response)?;
        extract_access_token(OTP_COMPLETION_OPERATION, &value)
    }

    async fn mark_device_trusted(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<(), VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::non_financial_post("/users/devices", &["users", "devices"], &[]),
            )
            .await?;
        require_success(DEVICE_TRUST_OPERATION, response)
    }

    async fn fetch_current_account(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Account, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/account", &["account"], &[]),
            )
            .await?;
        let value = require_success_json(CURRENT_ACCOUNT_OPERATION, response)?;
        let envelope: AccountEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: CURRENT_ACCOUNT_OPERATION,
                problem: "the account response did not match the supported envelope",
            })?;
        let user = envelope.data.into_user();
        let user_id =
            UserId::from_str(&user.id.into_string()).map_err(|_| VenmoApiError::Contract {
                operation: CURRENT_ACCOUNT_OPERATION,
                problem: "the account response contained an invalid user ID",
            })?;
        let raw_username = user.username.ok_or(VenmoApiError::Contract {
            operation: CURRENT_ACCOUNT_OPERATION,
            problem: "the account response omitted the username",
        })?;
        let username = match raw_username.strip_prefix('@') {
            Some(bare) => bare.to_owned(),
            None => raw_username,
        };
        let username = Username::from_bare(username).map_err(|_| VenmoApiError::Contract {
            operation: CURRENT_ACCOUNT_OPERATION,
            problem: "the account response contained an invalid username",
        })?;

        Ok(Account::new(user_id, username, user.display_name))
    }

    async fn revoke_token(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<(), VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::non_financial_delete(
                    "/oauth/access_token",
                    &["oauth", "access_token"],
                    &[],
                ),
            )
            .await?;
        require_success(REVOKE_TOKEN_OPERATION, response)?;
        Ok(())
    }

    async fn fetch_payment_methods(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Vec<PaymentMethod>, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/payment-methods", &["payment-methods"], &[]),
            )
            .await?;
        let value = require_success_json(PAYMENT_METHODS_OPERATION, response)?;
        let envelope: PaymentMethodsEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: PAYMENT_METHODS_OPERATION,
                problem: "the payment-method response did not match the supported envelope",
            })?;
        map_payment_methods(envelope.data.into_methods())
    }

    async fn fetch_balance(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Balance, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/account", &["account"], &[]),
            )
            .await?;
        let value = require_success_json(BALANCE_OPERATION, response)?;
        let envelope: BalanceEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: BALANCE_OPERATION,
                problem: "the account response omitted the supported wallet-balance fields",
            })?;
        let available = SignedUsdAmount::from_str(&envelope.data.balance).map_err(|_| {
            VenmoApiError::Contract {
                operation: BALANCE_OPERATION,
                problem: "the account response contained an invalid available balance",
            }
        })?;
        let on_hold = SignedUsdAmount::from_str(&envelope.data.balance_on_hold).map_err(|_| {
            VenmoApiError::Contract {
                operation: BALANCE_OPERATION,
                problem: "the account response contained an invalid on-hold balance",
            }
        })?;
        Ok(Balance::new(available, on_hold))
    }

    #[allow(dead_code)]
    async fn fetch_peer_funding_methods(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Vec<PeerFundingMethod>, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/payment-methods", &["payment-methods"], &[]),
            )
            .await?;
        let value = require_success_json(PEER_FUNDING_OPERATION, response)?;
        let envelope: PaymentMethodsEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response did not match the supported envelope",
            })?;
        map_peer_funding_methods(envelope.data.into_methods())
    }

    #[allow(dead_code)]
    async fn fetch_payment_eligibility(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        recipient: &User,
        amount: Money,
        note: &crate::domain::Note,
    ) -> Result<PaymentEligibility, VenmoApiError> {
        let body = serde_json::to_vec(&PaymentEligibilityRequest {
            funding_source_id: "",
            action: "pay",
            country_code: "1",
            target_type: "user_id",
            note: note.as_str(),
            target_id: recipient.user_id().as_str(),
            amount: amount.cents(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: PAYMENT_ELIGIBILITY_OPERATION,
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
        let value = require_success_json(PAYMENT_ELIGIBILITY_OPERATION, response)?;
        let envelope: PaymentEligibilityEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: PAYMENT_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility response did not match the supported envelope",
            })?;
        let eligibility = envelope.data;
        let _ = (&eligibility.fee_disclaimer, &eligibility.ineligible_reason);
        if !eligibility.eligible {
            return Err(VenmoApiError::EligibilityDenied);
        }
        let fee_cents = eligibility.fees.into_iter().try_fold(0_u64, |total, fee| {
            let cents = fee.calculated_cents().ok_or(VenmoApiError::Contract {
                operation: PAYMENT_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility response contained an unknown fee shape",
            })?;
            total.checked_add(cents).ok_or(VenmoApiError::Contract {
                operation: PAYMENT_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility fee total overflowed",
            })
        })?;
        let token = EligibilityToken::parse_owned(eligibility.eligibility_token).map_err(|_| {
            VenmoApiError::Contract {
                operation: PAYMENT_ELIGIBILITY_OPERATION,
                problem: "the payment-eligibility response contained an invalid token",
            }
        })?;
        Ok(PaymentEligibility::new(token, fee_cents))
    }

    #[allow(dead_code)]
    async fn create_peer_payment(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &PayPlan,
    ) -> Result<CreatedPayment, VenmoApiError> {
        let request_id = plan.request_id().to_string();
        let amount = money_json_number(plan.amount(), false, PAYMENT_CREATION_OPERATION)?;
        let body = serde_json::to_vec(&CreatePaymentRequest {
            uuid: &request_id,
            user_id: plan.recipient().user_id().as_str(),
            audience: "private",
            amount: &amount,
            note: plan.note().as_str(),
            eligibility_token: plan.eligibility_token().expose(),
            funding_source_id: plan.backup_method().method().id().as_str(),
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
        let value = require_financial_success_json(PAYMENT_CREATION_OPERATION, response)?;
        let payment = parse_created_payment(PAYMENT_CREATION_OPERATION, value)?;
        validate_created_payment(PAYMENT_CREATION_OPERATION, payment, plan, false)
    }

    #[allow(dead_code)]
    async fn create_peer_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &CreateRequestPlan,
    ) -> Result<CreatedPayment, VenmoApiError> {
        let request_id = plan.request_id().to_string();
        let amount = money_json_number(plan.amount(), true, REQUEST_CREATION_OPERATION)?;
        let body = serde_json::to_vec(&CreateRequestRequest {
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
        let value = require_financial_success_json(REQUEST_CREATION_OPERATION, response)?;
        let payment = parse_created_payment(REQUEST_CREATION_OPERATION, value)?;
        validate_created_request(REQUEST_CREATION_OPERATION, payment, plan)
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
                "approve",
                REQUEST_ACCEPTANCE_OPERATION,
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
                "deny",
                REQUEST_DECLINE_OPERATION,
            )
            .await?;
        validate_declined_request(payment, plan)
    }

    async fn update_incoming_request(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        request_id: &RequestId,
        action: &'static str,
        operation: &'static str,
    ) -> Result<PaymentRecordDto, VenmoApiError> {
        let body = serde_json::to_vec(&UpdatePaymentRequest { action })
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

    async fn fetch_friends_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        page: FriendsPageRequest,
    ) -> Result<FriendsPage, VenmoApiError> {
        let offset = page.token().map_or(0, FriendsPageToken::offset);
        let offset_value = offset.to_string();
        let limit_value = page.page_size().get().to_string();
        let path_segments = ["users", current_user_id.as_str(), "friends"];
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read(
                    "/users/{user-id}/friends",
                    &path_segments,
                    &[
                        ("limit", limit_value.as_str()),
                        ("offset", offset_value.as_str()),
                    ],
                ),
            )
            .await?;
        let value = require_success_json(FRIENDS_OPERATION, response)?;
        let envelope: FriendsEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: FRIENDS_OPERATION,
                problem: "the friend-list response did not match the supported envelope",
            })?;
        let users = map_users(envelope.data, FRIENDS_OPERATION)?;
        validate_page_count(FRIENDS_OPERATION, users.len(), page.page_size())?;
        let next_token = self.parse_friends_next_link(
            envelope.pagination.next.as_deref(),
            &path_segments,
            page.page_size(),
        )?;
        Ok(FriendsPage::new(users, next_token))
    }

    fn parse_friends_next_link(
        &self,
        raw: Option<&str>,
        path_segments: &[&str],
        page_size: NonZeroU32,
    ) -> Result<Option<FriendsPageToken>, VenmoApiError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let pairs = self.transport.parse_trusted_next_link(raw, path_segments)?;
        validate_query_keys(FRIENDS_OPERATION, &pairs, &["limit", "offset"])?;
        require_query_value(
            FRIENDS_OPERATION,
            &pairs,
            "limit",
            &page_size.get().to_string(),
        )?;
        let offset = unique_query_value(FRIENDS_OPERATION, &pairs, "offset")?
            .ok_or(VenmoApiError::Contract {
                operation: FRIENDS_OPERATION,
                problem: "the friend-list continuation omitted its offset",
            })?
            .parse::<u32>()
            .map_err(|_| VenmoApiError::Contract {
                operation: FRIENDS_OPERATION,
                problem: "the friend-list continuation contained an invalid offset",
            })?;
        Ok(Some(FriendsPageToken::from_offset(offset)))
    }

    async fn fetch_activity_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        page: ActivityPageRequest,
    ) -> Result<ActivityPage, VenmoApiError> {
        let limit_value = page.page_size().get().to_string();
        let mut query = vec![("limit", limit_value.as_str()), ("social_only", "false")];
        if let Some(token) = page.token() {
            query.push(("before_id", token.as_str()));
        }
        let path_segments = ["stories", "target-or-actor", current_user_id.as_str()];
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/stories/target-or-actor/{user-id}", &path_segments, &query),
            )
            .await?;
        let value = require_success_json(ACTIVITY_LIST_OPERATION, response)?;
        let envelope: StoriesEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: ACTIVITY_LIST_OPERATION,
                problem: "the activity response did not match the supported envelope",
            })?;
        let activities = envelope
            .data
            .into_iter()
            .map(|story| map_activity(story, current_user_id, ACTIVITY_LIST_OPERATION))
            .collect::<Result<Vec<_>, _>>()?;
        validate_page_count(ACTIVITY_LIST_OPERATION, activities.len(), page.page_size())?;
        let next_token = self.parse_activity_next_link(
            envelope.pagination.next.as_deref(),
            &path_segments,
            page.page_size(),
        )?;
        Ok(ActivityPage::new(activities, next_token))
    }

    fn parse_activity_next_link(
        &self,
        raw: Option<&str>,
        path_segments: &[&str],
        page_size: NonZeroU32,
    ) -> Result<Option<ActivityPageToken>, VenmoApiError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let pairs = self.transport.parse_trusted_next_link(raw, path_segments)?;
        validate_query_keys(
            ACTIVITY_LIST_OPERATION,
            &pairs,
            &["before_id", "limit", "only_public_stories", "social_only"],
        )?;
        require_query_value(
            ACTIVITY_LIST_OPERATION,
            &pairs,
            "limit",
            &page_size.get().to_string(),
        )?;
        require_query_value_case_insensitive(
            ACTIVITY_LIST_OPERATION,
            &pairs,
            "social_only",
            "false",
        )?;
        if let Some(value) =
            unique_query_value(ACTIVITY_LIST_OPERATION, &pairs, "only_public_stories")?
            && !value.eq_ignore_ascii_case("false")
        {
            return Err(VenmoApiError::Contract {
                operation: ACTIVITY_LIST_OPERATION,
                problem: "the activity continuation changed the visibility filter",
            });
        }
        let before_id = unique_query_value(ACTIVITY_LIST_OPERATION, &pairs, "before_id")?.ok_or(
            VenmoApiError::Contract {
                operation: ACTIVITY_LIST_OPERATION,
                problem: "the activity continuation omitted its before-id value",
            },
        )?;
        ActivityBeforeId::from_str(&before_id).map_err(|_| VenmoApiError::Contract {
            operation: ACTIVITY_LIST_OPERATION,
            problem: "the activity continuation contained an invalid before-id value",
        })?;
        Ok(Some(ActivityPageToken::new(before_id)))
    }

    async fn fetch_activity_by_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        activity_id: &ActivityId,
    ) -> Result<Activity, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read(
                    "/stories/{story-id}",
                    &["stories", activity_id.as_str()],
                    &[],
                ),
            )
            .await?;
        let value = require_success_json(ACTIVITY_DETAIL_OPERATION, response)?;
        let envelope: StoryEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: ACTIVITY_DETAIL_OPERATION,
                problem: "the activity-detail response did not match the supported envelope",
            })?;
        let activity = map_activity(
            envelope.data.into_story(),
            current_user_id,
            ACTIVITY_DETAIL_OPERATION,
        )?;
        if activity.id() != activity_id {
            return Err(VenmoApiError::Contract {
                operation: ACTIVITY_DETAIL_OPERATION,
                problem: "the activity-detail response returned a different activity ID",
            });
        }
        Ok(activity)
    }

    async fn fetch_pending_requests_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        page: PendingRequestsPageRequest,
    ) -> Result<PendingRequestsPage, VenmoApiError> {
        let limit_value = page.page_size().get().to_string();
        let mut query = vec![
            ("action", "charge"),
            ("status", "pending,held"),
            ("limit", limit_value.as_str()),
        ];
        if let Some(token) = page.token() {
            query.push(("before", token.as_str()));
        }
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/payments", &["payments"], &query),
            )
            .await?;
        let value = require_success_json(REQUEST_LIST_OPERATION, response)?;
        let envelope: PaymentsEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: REQUEST_LIST_OPERATION,
                problem: "the pending-request response did not match the supported envelope",
            })?;
        let requests = envelope
            .data
            .into_iter()
            .map(|payment| {
                map_pending_request(payment, current_user_id, REQUEST_LIST_OPERATION, true)
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
        page_size: NonZeroU32,
    ) -> Result<Option<PendingRequestsPageToken>, VenmoApiError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let pairs = self.transport.parse_trusted_next_link(raw, &["payments"])?;
        validate_query_keys(
            REQUEST_LIST_OPERATION,
            &pairs,
            &["action", "before", "limit", "status"],
        )?;
        require_query_value(REQUEST_LIST_OPERATION, &pairs, "action", "charge")?;
        require_query_value(REQUEST_LIST_OPERATION, &pairs, "status", "pending,held")?;
        require_query_value(
            REQUEST_LIST_OPERATION,
            &pairs,
            "limit",
            &page_size.get().to_string(),
        )?;
        let before = unique_query_value(REQUEST_LIST_OPERATION, &pairs, "before")?.ok_or(
            VenmoApiError::Contract {
                operation: REQUEST_LIST_OPERATION,
                problem: "the pending-request continuation omitted its before value",
            },
        )?;
        RequestsBefore::from_str(&before).map_err(|_| VenmoApiError::Contract {
            operation: REQUEST_LIST_OPERATION,
            problem: "the pending-request continuation contained an invalid before value",
        })?;
        Ok(Some(PendingRequestsPageToken::new(before)))
    }

    async fn fetch_pending_request_by_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        request_id: &RequestId,
    ) -> Result<PendingRequest, VenmoApiError> {
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
        let value = require_success_json(REQUEST_DETAIL_OPERATION, response)?;
        let envelope: super::dto::PaymentEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: REQUEST_DETAIL_OPERATION,
                problem: "the pending-request detail did not match the supported envelope",
            })?;
        let request = map_pending_request(
            envelope.data.into_payment(),
            current_user_id,
            REQUEST_DETAIL_OPERATION,
            false,
        )?;
        if request.id() != request_id {
            return Err(VenmoApiError::Contract {
                operation: REQUEST_DETAIL_OPERATION,
                problem: "the pending-request detail returned a different request ID",
            });
        }
        Ok(request)
    }

    async fn fetch_user_search_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        query: &UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> Result<UserSearchPage, VenmoApiError> {
        let offset = page.token().map_or(0, UserSearchPageToken::offset);
        let offset_value = offset.to_string();
        let limit_value = page.page_size().get().to_string();
        let query_value = query.username_query().unwrap_or(query.as_str());
        let mut query_pairs = vec![
            ("query", query_value),
            ("limit", limit_value.as_str()),
            ("offset", offset_value.as_str()),
        ];
        if query.username_query().is_some() {
            query_pairs.push(("type", "username"));
        }

        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/users", &["users"], &query_pairs),
            )
            .await?;
        let value = require_success_json(USER_SEARCH_OPERATION, response)?;
        let envelope: UsersEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: USER_SEARCH_OPERATION,
                problem: "the user-search response did not match the supported envelope",
            })?;
        let users = map_users(envelope.data.into_users(), USER_SEARCH_OPERATION)?;
        let page_size =
            usize::try_from(page.page_size().get()).map_err(|_| VenmoApiError::Contract {
                operation: USER_SEARCH_OPERATION,
                problem: "the requested page size could not be represented safely",
            })?;
        if users.len() > page_size {
            return Err(VenmoApiError::Contract {
                operation: USER_SEARCH_OPERATION,
                problem: "the user-search response exceeded the requested page size",
            });
        }
        let next_token = if !users.is_empty() && users.len() == page_size {
            let returned = u32::try_from(users.len()).map_err(|_| VenmoApiError::Contract {
                operation: USER_SEARCH_OPERATION,
                problem: "the returned page size could not be represented safely",
            })?;
            let next_offset = offset
                .checked_add(returned)
                .ok_or(VenmoApiError::Contract {
                    operation: USER_SEARCH_OPERATION,
                    problem: "the user-search offset overflowed",
                })?;
            Some(UserSearchPageToken::from_offset(next_offset))
        } else {
            None
        };
        Ok(UserSearchPage::new(users, next_token))
    }

    async fn fetch_user_by_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        user_id: &UserId,
    ) -> Result<User, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/users/{user-id}", &["users", user_id.as_str()], &[]),
            )
            .await?;
        let value = require_success_json(USER_LOOKUP_OPERATION, response)?;
        let envelope: UserEnvelope =
            serde_json::from_value(value).map_err(|_| VenmoApiError::Contract {
                operation: USER_LOOKUP_OPERATION,
                problem: "the user-lookup response did not match the supported envelope",
            })?;
        let user = map_user(envelope.data.into_user(), USER_LOOKUP_OPERATION)?;
        if user.user_id() != user_id {
            return Err(VenmoApiError::Contract {
                operation: USER_LOOKUP_OPERATION,
                problem: "the user-lookup response returned a different user ID",
            });
        }
        Ok(user)
    }
}

impl AuthenticationApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.fetch_current_account(access_token, device_id)
    }

    fn revoke_access_token<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.revoke_token(access_token, device_id)
    }
}

impl PasswordLoginApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn begin_password_login<'a>(
        &'a self,
        identifier: &'a LoginIdentifier,
        password: &'a AccountPassword,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PasswordLoginStart, Self::Error>> + Send + 'a {
        self.start_password_login(identifier, password, device_id)
    }

    fn request_sms_otp<'a>(
        &'a self,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.send_sms_otp(otp_secret, device_id)
    }

    fn complete_otp_login<'a>(
        &'a self,
        otp_code: &'a OtpCode,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<AccessToken, Self::Error>> + Send + 'a {
        self.finish_otp_login(otp_code, otp_secret, device_id)
    }

    fn trust_device<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.mark_device_trusted(access_token, device_id)
    }
}

impl PaymentMethodsApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn payment_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a {
        self.fetch_payment_methods(access_token, device_id)
    }
}

impl PeerFundingApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn peer_funding_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PeerFundingMethod>, Self::Error>> + Send + 'a {
        self.fetch_peer_funding_methods(access_token, device_id)
    }
}

impl PaymentEligibilityApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn payment_eligibility<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        recipient: &'a User,
        amount: Money,
        note: &'a crate::domain::Note,
    ) -> impl Future<Output = Result<PaymentEligibility, Self::Error>> + Send + 'a {
        self.fetch_payment_eligibility(access_token, device_id, recipient, amount, note)
    }
}

impl PaymentCreationApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn create_payment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a PayPlan,
    ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a {
        self.create_peer_payment(access_token, device_id, plan)
    }
}

impl RequestCreationApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn create_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a CreateRequestPlan,
    ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a {
        self.create_peer_request(access_token, device_id, plan)
    }
}

impl RequestAcceptanceApi for VenmoApiClient {
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

impl RequestDeclineApi for VenmoApiClient {
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

impl UsersApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.fetch_user_by_id(access_token, device_id, user_id)
    }

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.fetch_user_search_page(access_token, device_id, query, page)
    }
}

impl FriendsApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn friends<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: FriendsPageRequest,
    ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a {
        self.fetch_friends_page(access_token, device_id, current_user_id, page)
    }
}

impl BalanceApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn balance<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.fetch_balance(access_token, device_id)
    }
}

impl ActivityApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
        self.fetch_activity_page(access_token, device_id, current_user_id, page)
    }

    fn activity_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<Activity, Self::Error>> + Send + 'a {
        self.fetch_activity_by_id(access_token, device_id, current_user_id, activity_id)
    }
}

impl RequestsApi for VenmoApiClient {
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

impl RequestLookupApi for VenmoApiClient {
    type Error = VenmoApiError;

    fn pending_request_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<PendingRequest, Self::Error>> + Send + 'a {
        self.fetch_pending_request_by_id(access_token, device_id, current_user_id, request_id)
    }
}

impl DoctorApi for VenmoApiClient {
    type Error = VenmoApiError;

    async fn connectivity(&self) -> Result<(), Self::Error> {
        self.transport
            .send_unauthenticated(HttpRequest::read("/account", &["account"], &[]))
            .await?;
        Ok(())
    }

    fn diagnostic_current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.fetch_current_account(access_token, device_id)
    }

    async fn required_shapes<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
    ) -> Vec<ShapeProbeOutcome> {
        let page_size = NonZeroU32::MIN;
        vec![
            shape_probe(
                RequiredShape::Balance,
                self.fetch_balance(access_token, device_id).await,
            ),
            shape_probe(
                RequiredShape::PaymentMethods,
                self.fetch_payment_methods(access_token, device_id).await,
            ),
            shape_probe(
                RequiredShape::Friends,
                self.fetch_friends_page(
                    access_token,
                    device_id,
                    current_user_id,
                    FriendsPageRequest::new(page_size, None),
                )
                .await,
            ),
            shape_probe(
                RequiredShape::Activity,
                self.fetch_activity_page(
                    access_token,
                    device_id,
                    current_user_id,
                    ActivityPageRequest::new(page_size, None),
                )
                .await,
            ),
            shape_probe(
                RequiredShape::PendingRequests,
                self.fetch_pending_requests_page(
                    access_token,
                    device_id,
                    current_user_id,
                    PendingRequestsPageRequest::new(page_size, None),
                )
                .await,
            ),
        ]
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum VenmoApiError {
    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error("Venmo rejected the {operation} request with HTTP {status}{code_suffix}")]
    Http {
        operation: &'static str,
        status: u16,
        code_suffix: ApiCodeSuffix,
    },

    #[error("Venmo reported that the {operation} request failed{code_suffix}")]
    ApiFailure {
        operation: &'static str,
        code_suffix: ApiCodeSuffix,
    },

    #[error("Venmo reported that this personal payment is not eligible")]
    EligibilityDenied,

    #[error("the successful {operation} response was not valid JSON")]
    MalformedJson { operation: &'static str },

    #[error("failed to encode the {operation} request")]
    RequestEncoding { operation: &'static str },

    #[error(
        "the successful {operation} response could not prove the issued token; authentication outcome is unknown and a remote token may remain active because {problem}"
    )]
    AuthenticationOutcomeUnknown {
        operation: &'static str,
        problem: &'static str,
    },

    #[error(
        "the {operation} outcome is unknown and must be reconciled before retrying because {problem}"
    )]
    FinancialOutcomeUnknown {
        operation: &'static str,
        problem: &'static str,
    },

    #[error("cannot use the {operation} response because {problem}")]
    Contract {
        operation: &'static str,
        problem: &'static str,
    },
}

impl ApiFailure for VenmoApiError {
    fn kind(&self) -> ApiFailureKind {
        match self {
            Self::Transport(TransportError::Timeout) => ApiFailureKind::Timeout,
            Self::Transport(
                TransportError::Network
                | TransportError::UnexpectedRedirect
                | TransportError::ResponseRead,
            ) => ApiFailureKind::Network,
            Self::Transport(TransportError::FinancialWriteOutcomeUnknown { .. }) => {
                ApiFailureKind::AmbiguousWrite
            }
            Self::FinancialOutcomeUnknown { .. } => ApiFailureKind::AmbiguousWrite,
            Self::Transport(TransportError::AuthenticationOutcomeUnknown { .. }) => {
                ApiFailureKind::Internal
            }
            Self::Transport(
                TransportError::ResponseTooLarge { .. }
                | TransportError::InvalidRoute
                | TransportError::InvalidQuery
                | TransportError::InvalidContinuationLink
                | TransportError::InvalidAuthenticationResponseHeader,
            )
            | Self::MalformedJson { .. }
            | Self::Contract { .. } => ApiFailureKind::Contract,
            Self::Transport(
                TransportError::InvalidAuthenticationHeader
                | TransportError::RequestConstruction
                | TransportError::ResourceExhaustion,
            )
            | Self::RequestEncoding { .. }
            | Self::AuthenticationOutcomeUnknown { .. } => ApiFailureKind::Internal,
            Self::Http { .. } | Self::ApiFailure { .. } | Self::EligibilityDenied => {
                ApiFailureKind::Rejected
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ApiCodeSuffix(Option<String>);

impl std::fmt::Display for ApiCodeSuffix {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(code) => write!(formatter, " (error code {code})"),
            None => Ok(()),
        }
    }
}

fn require_success_json(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Value, VenmoApiError> {
    require_success_value(operation, response)?.ok_or(VenmoApiError::Contract {
        operation,
        problem: "the response body was empty",
    })
}

#[allow(dead_code)]
fn require_financial_success_json(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Value, VenmoApiError> {
    let status = response.status();
    if response.body().is_empty() {
        return Err(VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response body was empty",
        });
    }
    let value = serde_json::from_slice::<Value>(response.body()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response body was not valid JSON",
        }
    })?;
    let error_code = extract_error_code(&value);
    let confirmed_error_code = extract_root_error_code(&value);
    let confirmed_rejection = confirmed_error_code
        .as_deref()
        .is_some_and(|code| is_confirmed_financial_rejection(operation, status.as_u16(), code));
    if !status.is_success() {
        if confirmed_rejection {
            return Err(VenmoApiError::Http {
                operation,
                status: status.as_u16(),
                code_suffix: ApiCodeSuffix::from_remote(error_code.as_deref()),
            });
        }
        return Err(VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the server response did not prove that no write occurred",
        });
    }
    if error_code.as_deref().is_some_and(is_failure_error_code) {
        if confirmed_rejection {
            return Err(VenmoApiError::ApiFailure {
                operation,
                code_suffix: ApiCodeSuffix::from_remote(error_code.as_deref()),
            });
        }
        return Err(VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the successful HTTP response contained an unverified API error",
        });
    }
    Ok(value)
}

#[allow(dead_code)]
fn is_confirmed_financial_rejection(operation: &str, status: u16, code: &str) -> bool {
    operation == PAYMENT_CREATION_OPERATION && status == 400 && matches!(code, "1396" | "13006")
}

fn require_issued_token_json(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Value, VenmoApiError> {
    require_success_json(operation, response).map_err(|error| match error {
        VenmoApiError::MalformedJson { .. } | VenmoApiError::Contract { .. } => {
            VenmoApiError::AuthenticationOutcomeUnknown {
                operation,
                problem: "the successful response did not contain a usable JSON token payload",
            }
        }
        other => other,
    })
}

fn parse_response_json(response: &HttpResponse) -> Option<Value> {
    if response.body().is_empty() {
        None
    } else {
        serde_json::from_slice(response.body()).ok()
    }
}

fn extract_access_token(
    operation: &'static str,
    value: &Value,
) -> Result<AccessToken, VenmoApiError> {
    let token = value
        .get("access_token")
        .or_else(|| value.get("data").and_then(|data| data.get("access_token")))
        .and_then(Value::as_str)
        .ok_or(VenmoApiError::AuthenticationOutcomeUnknown {
            operation,
            problem: "the response omitted the access token",
        })?;
    AccessToken::from_normalized_owned(token.to_owned()).map_err(|_| {
        VenmoApiError::AuthenticationOutcomeUnknown {
            operation,
            problem: "the response contained an invalid access token",
        }
    })
}

fn require_success(operation: &'static str, response: HttpResponse) -> Result<(), VenmoApiError> {
    require_success_value(operation, response).map(|_| ())
}

fn require_success_value(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Option<Value>, VenmoApiError> {
    let status = response.status();
    let value = if response.body().is_empty() {
        None
    } else {
        match serde_json::from_slice::<Value>(response.body()) {
            Ok(value) => Some(value),
            Err(_) if !status.is_success() => None,
            Err(_) => return Err(VenmoApiError::MalformedJson { operation }),
        }
    };
    let error_code = value.as_ref().and_then(extract_error_code);
    let code_suffix = ApiCodeSuffix::from_remote(error_code.as_deref());

    if !status.is_success() {
        return Err(VenmoApiError::Http {
            operation,
            status: status.as_u16(),
            code_suffix,
        });
    }
    if error_code.as_deref().is_some_and(is_failure_error_code) {
        return Err(VenmoApiError::ApiFailure {
            operation,
            code_suffix,
        });
    }
    Ok(value)
}

impl ApiCodeSuffix {
    fn from_remote(code: Option<&str>) -> Self {
        Self(code.and_then(sanitize_api_code))
    }
}

fn sanitize_api_code(code: &str) -> Option<String> {
    const MAX_CODE_BYTES: usize = 64;
    if code.is_empty()
        || code.len() > MAX_CODE_BYTES
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return None;
    }
    Some(code.to_owned())
}

fn is_failure_error_code(code: &str) -> bool {
    !matches!(code, "0" | "0.0")
}

fn extract_error_code(value: &Value) -> Option<String> {
    const PATHS: &[&[&str]] = &[
        &["error", "code"],
        &["error_code"],
        &["code"],
        &["data", "error_code"],
        &["data", "error", "code"],
    ];
    for path in PATHS {
        let mut current = value;
        let mut found = true;
        for segment in *path {
            let Some(next) = current.get(*segment) else {
                found = false;
                break;
            };
            current = next;
        }
        if found {
            if let Some(code) = current.as_str() {
                return Some(code.to_owned());
            }
            if current.is_number() {
                return Some(current.to_string());
            }
        }
    }
    None
}

#[allow(dead_code)]
fn extract_root_error_code(value: &Value) -> Option<String> {
    let code = value.get("error")?.as_object()?.get("code")?;
    if let Some(code) = code.as_str() {
        Some(code.to_owned())
    } else if code.is_number() {
        Some(code.to_string())
    } else {
        None
    }
}

fn map_payment_methods(
    methods: Vec<PaymentMethodDto>,
) -> Result<Vec<PaymentMethod>, VenmoApiError> {
    let mut mapped = Vec::with_capacity(methods.len());
    let mut ids = HashSet::with_capacity(methods.len());
    for method in methods {
        let is_default = method.is_default();
        let id = PaymentMethodId::from_str(&method.id.into_string()).map_err(|_| {
            VenmoApiError::Contract {
                operation: PAYMENT_METHODS_OPERATION,
                problem: "the payment-method response contained an invalid method ID",
            }
        })?;
        if !ids.insert(id.clone()) {
            return Err(VenmoApiError::Contract {
                operation: PAYMENT_METHODS_OPERATION,
                problem: "the payment-method response contained duplicate method IDs",
            });
        }
        mapped.push(PaymentMethod::new(
            id,
            method.name.map(|value| value.into_string()),
            method.method_type.map(|value| value.into_string()),
            method.last_four.map(|value| value.into_string()),
            is_default,
        ));
    }
    Ok(mapped)
}

#[allow(dead_code)]
fn map_peer_funding_methods(
    methods: Vec<PaymentMethodDto>,
) -> Result<Vec<PeerFundingMethod>, VenmoApiError> {
    let mut mapped = Vec::with_capacity(methods.len());
    let mut ids = HashSet::with_capacity(methods.len());
    for method in methods {
        let PaymentMethodDto {
            id,
            name,
            method_type,
            last_four,
            is_default: _,
            role: _,
            payment_method_role: _,
            peer_payment_role,
            merchant_payment_role: _,
            fee,
        } = method;
        let id =
            PaymentMethodId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response contained an invalid method ID",
            })?;
        if !ids.insert(id.clone()) {
            return Err(VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response contained duplicate method IDs",
            });
        }
        let role = peer_payment_role.ok_or(VenmoApiError::Contract {
            operation: PEER_FUNDING_OPERATION,
            problem: "the peer funding-method response omitted a peer-payment role",
        })?;
        let role = role.as_str().to_ascii_lowercase();
        let role = match role.as_str() {
            "default" => Some(PeerFundingRole::Default),
            "backup" => Some(PeerFundingRole::Backup),
            "none" => None,
            _ => {
                return Err(VenmoApiError::Contract {
                    operation: PEER_FUNDING_OPERATION,
                    problem: "the peer funding-method response contained an unknown peer-payment role",
                });
            }
        };
        let Some(role) = role else {
            continue;
        };
        let funding_kind = method_type
            .as_ref()
            .ok_or(VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response omitted the method type",
            })?
            .as_str()
            .to_ascii_lowercase();
        match funding_kind.as_str() {
            "balance" => continue,
            "bank" | "card" => {}
            _ => {
                return Err(VenmoApiError::Contract {
                    operation: PEER_FUNDING_OPERATION,
                    problem: "the peer funding-method response did not prove an external bank or card source",
                });
            }
        }
        let fee = match fee.as_ref().and_then(super::dto::FeeDto::calculated_cents) {
            None => PeerFundingFee::Unknown,
            Some(0) => PeerFundingFee::ProvenZero,
            Some(cents) => PeerFundingFee::NonZero { cents },
        };
        let payment_method = PaymentMethod::new(
            id,
            name.map(|value| value.into_string()),
            method_type.map(|value| value.into_string()),
            last_four.map(|value| value.into_string()),
            matches!(role, PeerFundingRole::Default),
        );
        mapped.push(PeerFundingMethod::new(payment_method, role, fee));
    }
    Ok(mapped)
}

#[allow(dead_code)]
fn money_json_number(
    amount: Money,
    negative: bool,
    operation: &'static str,
) -> Result<serde_json::Number, VenmoApiError> {
    let amount = if negative {
        format!("-{amount}")
    } else {
        amount.to_string()
    };
    serde_json::Number::from_str(&amount).map_err(|_| VenmoApiError::RequestEncoding { operation })
}

#[allow(dead_code)]
fn parse_created_payment(
    operation: &'static str,
    value: Value,
) -> Result<PaymentRecordDto, VenmoApiError> {
    let envelope: super::dto::CreatedPaymentEnvelope =
        serde_json::from_value(value).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the successful response did not match the supported payment envelope",
        })?;
    let payment = envelope.data.payment;
    let created_at =
        payment
            .date_created
            .as_deref()
            .ok_or(VenmoApiError::FinancialOutcomeUnknown {
                operation,
                problem: "the successful response omitted the creation timestamp",
            })?;
    parse_timestamp_value(created_at).map_err(|()| VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: "the successful response contained an invalid creation timestamp",
    })?;
    Ok(payment)
}

fn parse_updated_payment(
    operation: &'static str,
    value: Value,
) -> Result<PaymentRecordDto, VenmoApiError> {
    let envelope: super::dto::PaymentEnvelope =
        serde_json::from_value(value).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the successful response did not match the supported updated-payment envelope",
        })?;
    let payment = envelope.data.into_payment();
    let created_at =
        payment
            .date_created
            .as_deref()
            .ok_or(VenmoApiError::FinancialOutcomeUnknown {
                operation,
                problem: "the successful response omitted the original creation timestamp",
            })?;
    parse_timestamp_value(created_at).map_err(|()| VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: "the successful response contained an invalid creation timestamp",
    })?;
    Ok(payment)
}

fn validate_accepted_request(
    payment: PaymentRecordDto,
    plan: &AcceptRequestPlan,
) -> Result<AcceptedRequest, VenmoApiError> {
    let operation = REQUEST_ACCEPTANCE_OPERATION;
    let status = validate_updated_record(
        operation,
        &payment,
        plan.request(),
        plan.account().user_id(),
        plan.request().counterparty().user_id(),
        "pay",
    )?;
    let status = match status.as_str() {
        "settled" => FinancialStatus::Settled,
        "pending" => FinancialStatus::Pending,
        "held" => FinancialStatus::Held,
        _ => {
            return financial_contract_unknown(
                operation,
                "the response contained an unsupported accepted-payment status",
            );
        }
    };
    let payment_id = PaymentId::from_str(&payment.id.into_string()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid accepted-payment ID",
        }
    })?;
    Ok(AcceptedRequest::new(payment_id, status))
}

fn validate_declined_request(
    payment: PaymentRecordDto,
    plan: &DeclineRequestPlan,
) -> Result<DeclinedRequest, VenmoApiError> {
    let operation = REQUEST_DECLINE_OPERATION;
    let status = validate_updated_record(
        operation,
        &payment,
        plan.request(),
        plan.request().counterparty().user_id(),
        plan.account().user_id(),
        "charge",
    )?;
    if status.as_str() != "cancelled" {
        return financial_contract_unknown(
            operation,
            "the response did not prove the request reached the supported terminal state",
        );
    }
    Ok(DeclinedRequest::new(plan.request().id().clone(), status))
}

fn validate_updated_record(
    operation: &'static str,
    payment: &PaymentRecordDto,
    request: &PendingRequest,
    expected_actor: &UserId,
    expected_target: &UserId,
    expected_action: &str,
) -> Result<RequestStatus, VenmoApiError> {
    if payment.id.as_str() != request.id().as_str() {
        return financial_contract_unknown(
            operation,
            "the response returned a different request ID",
        );
    }
    if payment.action != expected_action {
        return financial_contract_unknown(operation, "the response returned a different action");
    }
    let amount = Money::from_str(&payment.amount.as_str()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid amount",
        }
    })?;
    if amount != request.amount() {
        return financial_contract_unknown(operation, "the response returned a different amount");
    }
    if payment.actor.id.as_str() != expected_actor.as_str() {
        return financial_contract_unknown(operation, "the response returned a different actor");
    }
    if payment.target.user.id.as_str() != expected_target.as_str() {
        return financial_contract_unknown(operation, "the response returned a different target");
    }
    if payment.note.as_deref() != request.note() {
        return financial_contract_unknown(operation, "the response returned a different note");
    }
    if payment.audience.as_deref() != request.audience() {
        return financial_contract_unknown(operation, "the response returned a different audience");
    }
    let response_created_at = payment
        .date_created
        .as_deref()
        .ok_or(VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response omitted the original creation timestamp",
        })
        .and_then(|value| {
            parse_timestamp_value(value).map_err(|()| VenmoApiError::FinancialOutcomeUnknown {
                operation,
                problem: "the response contained an invalid creation timestamp",
            })
        })?;
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
    RequestStatus::from_str(&payment.status).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
        operation,
        problem: "the response contained an invalid request status",
    })
}

#[allow(dead_code)]
fn validate_created_payment(
    operation: &'static str,
    payment: PaymentRecordDto,
    plan: &PayPlan,
    request: bool,
) -> Result<CreatedPayment, VenmoApiError> {
    validate_created_record(
        operation,
        payment,
        plan.account(),
        plan.recipient(),
        plan.amount(),
        plan.note(),
        if request { "charge" } else { "pay" },
        request,
    )
}

#[allow(dead_code)]
fn validate_created_request(
    operation: &'static str,
    payment: PaymentRecordDto,
    plan: &CreateRequestPlan,
) -> Result<CreatedPayment, VenmoApiError> {
    validate_created_record(
        operation,
        payment,
        plan.account(),
        plan.recipient(),
        plan.amount(),
        plan.note(),
        "charge",
        true,
    )
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn validate_created_record(
    operation: &'static str,
    payment: PaymentRecordDto,
    account: &Account,
    recipient: &User,
    expected_amount: Money,
    expected_note: &crate::domain::Note,
    expected_action: &str,
    request: bool,
) -> Result<CreatedPayment, VenmoApiError> {
    let PaymentRecordDto {
        id,
        status,
        action,
        amount,
        actor,
        target,
        note,
        audience,
        date_created: _,
    } = payment;
    if action != expected_action {
        return financial_contract_unknown(operation, "the response returned a different action");
    }
    let amount = Money::from_str(&amount.into_string()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid amount",
        }
    })?;
    if amount != expected_amount {
        return financial_contract_unknown(operation, "the response returned a different amount");
    }
    if actor.id.into_string() != account.user_id().as_str() {
        return financial_contract_unknown(operation, "the response returned a different actor");
    }
    if target.user.id.into_string() != recipient.user_id().as_str() {
        return financial_contract_unknown(operation, "the response returned a different target");
    }
    if note.as_deref() != Some(expected_note.as_str()) {
        return financial_contract_unknown(operation, "the response returned a different note");
    }
    if audience.as_deref() != Some("private") {
        return financial_contract_unknown(
            operation,
            "the response did not prove a private audience",
        );
    }
    let status = match (request, status.as_str()) {
        (true, "pending") => FinancialStatus::Pending,
        (false, "settled") => FinancialStatus::Settled,
        (false, "pending") => FinancialStatus::Pending,
        (false, "held") => FinancialStatus::Held,
        _ => {
            return financial_contract_unknown(
                operation,
                "the response contained an unsupported financial status",
            );
        }
    };
    let id = PaymentId::from_str(&id.into_string()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response contained an invalid payment ID",
        }
    })?;
    Ok(CreatedPayment::new(id, status))
}

#[allow(dead_code)]
fn financial_contract_unknown<T>(
    operation: &'static str,
    problem: &'static str,
) -> Result<T, VenmoApiError> {
    Err(VenmoApiError::FinancialOutcomeUnknown { operation, problem })
}

fn map_users(users: Vec<UserDto>, operation: &'static str) -> Result<Vec<User>, VenmoApiError> {
    users
        .into_iter()
        .map(|user| map_user(user, operation))
        .collect()
}

fn map_user(user: UserDto, operation: &'static str) -> Result<User, VenmoApiError> {
    let profile_kind = user.identity_type.as_deref().map(|value| {
        if value.eq_ignore_ascii_case("personal") {
            UserProfileKind::Personal
        } else if value.eq_ignore_ascii_case("business") {
            UserProfileKind::Business
        } else if value.eq_ignore_ascii_case("charity") {
            UserProfileKind::Charity
        } else {
            UserProfileKind::Unknown
        }
    });
    let is_payable = user.is_payable;
    let user_id =
        UserId::from_str(&user.id.into_string()).map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the user response contained an invalid user ID",
        })?;
    let username = match user.username.filter(|value| !value.is_empty()) {
        Some(value) => {
            let bare = value.strip_prefix('@').unwrap_or(&value);
            Some(
                Username::from_bare(bare.to_owned()).map_err(|_| VenmoApiError::Contract {
                    operation,
                    problem: "the user response contained an invalid username",
                })?,
            )
        }
        None => None,
    };
    Ok(User::new(
        user_id,
        username,
        user.display_name.filter(|value| !value.is_empty()),
    )
    .with_optional_financial_attributes(profile_kind, is_payable))
}

fn map_activity(
    story: StoryDto,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let StoryDto {
        id,
        date_created: story_created,
        note: story_note,
        audience: story_audience,
        payment,
        transfer,
        authorization,
    } = story;
    let id = ActivityId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid activity ID",
    })?;
    match (payment, transfer, authorization) {
        (Some(payment), None, None) => map_payment_activity(
            id,
            story_created,
            story_note,
            story_audience,
            payment,
            current_user_id,
            operation,
        ),
        (None, Some(transfer), None) => map_transfer_activity(
            id,
            story_created,
            story_note,
            story_audience,
            transfer,
            operation,
        ),
        (None, None, Some(authorization)) => map_authorization_activity(
            id,
            story_created,
            story_note,
            story_audience,
            authorization,
            current_user_id,
            operation,
        ),
        _ => Err(VenmoApiError::Contract {
            operation,
            problem: "the activity response contained an unsupported or ambiguous record type",
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn map_authorization_activity(
    id: ActivityId,
    story_created: Option<String>,
    story_note: Option<String>,
    story_audience: Option<String>,
    authorization: AuthorizationDto,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let AuthorizationDto {
        id: authorization_id,
        status,
        amount,
        created_at,
        descriptor,
        merchant,
        user,
    } = authorization;
    ActivityId::from_str(&authorization_id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid authorization ID",
    })?;
    let user = map_user(user, operation)?;
    if user.user_id() != current_user_id {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the authorization activity belonged to a different account",
        });
    }
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid authorization amount",
    })?;
    let action =
        ActivityAction::from_str("authorization").map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the authorization activity action was invalid",
        })?;
    let status = ActivityStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid authorization status",
    })?;
    let occurred_at = parse_timestamp(
        created_at.or(story_created),
        operation,
        "the authorization activity omitted or contained an invalid creation timestamp",
    )?;
    let note = bounded_optional_text(
        story_note.or(descriptor),
        operation,
        "the authorization activity contained an oversized note",
    )?;
    let audience = bounded_optional_label(
        story_audience,
        operation,
        "the authorization activity contained an invalid audience",
    )?;
    let merchant_name = bounded_required_text(
        merchant.display_name,
        operation,
        "the authorization activity contained an invalid merchant name",
    )?;
    Ok(Activity::new(
        id,
        occurred_at,
        action,
        ActivityDirection::Outgoing,
        ActivityCounterparty::external(merchant_name, "merchant".to_owned(), None),
        amount,
        status,
        note,
        audience,
    ))
}

#[allow(clippy::too_many_arguments)]
fn map_payment_activity(
    id: ActivityId,
    story_created: Option<String>,
    story_note: Option<String>,
    story_audience: Option<String>,
    payment: PaymentRecordDto,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let PaymentRecordDto {
        id: _,
        status,
        action,
        amount,
        actor,
        target,
        note: payment_note,
        audience: payment_audience,
        date_created: payment_created,
    } = payment;
    let actor = map_user(actor, operation)?;
    let target = map_user(target.user, operation)?;
    let (direction, counterparty) = relative_parties(actor, target, current_user_id, operation)?;
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid positive USD amount",
    })?;
    let action = ActivityAction::from_str(&action).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid action",
    })?;
    let status = ActivityStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid status",
    })?;
    let occurred_at = parse_timestamp(
        payment_created.or(story_created),
        operation,
        "the activity response omitted or contained an invalid creation timestamp",
    )?;
    let note = bounded_optional_text(
        payment_note.or(story_note),
        operation,
        "the activity response contained an oversized note",
    )?;
    let audience = bounded_optional_label(
        payment_audience.or(story_audience),
        operation,
        "the activity response contained an invalid audience",
    )?;

    Ok(Activity::new(
        id,
        occurred_at,
        action,
        direction,
        counterparty,
        amount,
        status,
        note,
        audience,
    ))
}

fn map_transfer_activity(
    id: ActivityId,
    story_created: Option<String>,
    story_note: Option<String>,
    story_audience: Option<String>,
    transfer: TransferDto,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let TransferDto {
        id: transfer_id,
        status,
        transfer_type,
        amount,
        date_requested,
        destination,
        source,
    } = transfer;
    if let Some(transfer_id) = transfer_id {
        ActivityId::from_str(&transfer_id.into_string()).map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the activity response contained an invalid transfer ID",
        })?;
    }
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid transfer amount",
    })?;
    let action_value = format!("transfer:{transfer_type}");
    let action = ActivityAction::from_str(&action_value).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid transfer type",
    })?;
    let status = ActivityStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid transfer status",
    })?;
    let occurred_at = parse_timestamp(
        date_requested.or(story_created),
        operation,
        "the transfer activity omitted or contained an invalid request timestamp",
    )?;
    let note = bounded_optional_text(
        story_note,
        operation,
        "the transfer activity contained an oversized note",
    )?;
    let audience = bounded_optional_label(
        story_audience,
        operation,
        "the transfer activity contained an invalid audience",
    )?;
    let (direction, endpoint) = match (source, destination) {
        (Some(source), None) => (ActivityDirection::Incoming, source),
        (None, Some(destination)) => (ActivityDirection::Outgoing, destination),
        (Some(_), Some(_)) | (None, None) => {
            return Err(VenmoApiError::Contract {
                operation,
                problem: "the transfer activity did not identify exactly one source or destination",
            });
        }
    };
    let name = bounded_required_text(
        endpoint.name,
        operation,
        "the transfer activity contained an invalid external-account name",
    )?;
    let kind = bounded_required_label(
        endpoint.endpoint_type,
        operation,
        "the transfer activity contained an invalid external-account type",
    )?;
    let last_four = endpoint
        .last_four
        .map(|value| {
            bounded_required_label(
                value.into_string(),
                operation,
                "the transfer activity contained an invalid destination suffix",
            )
        })
        .transpose()?;
    let counterparty = ActivityCounterparty::external(name, kind, last_four);
    Ok(Activity::new(
        id,
        occurred_at,
        action,
        direction,
        counterparty,
        amount,
        status,
        note,
        audience,
    ))
}

fn map_pending_request(
    payment: PaymentRecordDto,
    current_user_id: &UserId,
    operation: &'static str,
    require_pending: bool,
) -> Result<PendingRequest, VenmoApiError> {
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
    let supported_action = if require_pending {
        action == "charge"
    } else {
        matches!(action.as_str(), "charge" | "pay")
    };
    if !supported_action {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the request response contained an unsupported action",
        });
    }
    let mapped_action = match action.as_str() {
        "charge" => PendingRequestAction::Charge,
        "pay" => PendingRequestAction::Pay,
        _ => {
            return Err(VenmoApiError::Contract {
                operation,
                problem: "the request response contained an unsupported action",
            });
        }
    };
    let status = RequestStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the pending-request response contained an invalid status",
    })?;
    if require_pending && !status.is_pending_record() {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the pending-request response contained a non-pending record",
        });
    }
    let id = RequestId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the pending-request response contained an invalid request ID",
    })?;
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the pending-request response contained an invalid positive USD amount",
    })?;
    let actor = map_user(actor, operation)?;
    let target = map_user(target.user, operation)?;
    let (direction, counterparty) = request_parties(actor, target, current_user_id, operation)?;
    let note = bounded_optional_text(
        note,
        operation,
        "the pending-request response contained an oversized note",
    )?;
    let created_at = date_created
        .map(|value| {
            parse_timestamp_value(&value).map_err(|_| VenmoApiError::Contract {
                operation,
                problem: "the pending-request response contained an invalid creation timestamp",
            })
        })
        .transpose()?;
    Ok(PendingRequest::new(
        id,
        direction,
        counterparty,
        amount,
        note,
        created_at,
        status,
    )
    .with_action(mapped_action)
    .with_audience(audience))
}

fn relative_parties(
    actor: User,
    target: User,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<(ActivityDirection, ActivityCounterparty), VenmoApiError> {
    let actor_is_self = actor.user_id() == current_user_id;
    let target_is_self = target.user_id() == current_user_id;
    match (actor_is_self, target_is_self) {
        (true, false) => Ok((
            ActivityDirection::Outgoing,
            ActivityCounterparty::user(target),
        )),
        (false, true) => Ok((
            ActivityDirection::Incoming,
            ActivityCounterparty::user(actor),
        )),
        (true, true) | (false, false) => Err(VenmoApiError::Contract {
            operation,
            problem: "the activity response did not identify exactly one active-account party",
        }),
    }
}

fn request_parties(
    actor: User,
    target: User,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<(RequestDirection, User), VenmoApiError> {
    let actor_is_self = actor.user_id() == current_user_id;
    let target_is_self = target.user_id() == current_user_id;
    match (actor_is_self, target_is_self) {
        (true, false) => Ok((RequestDirection::Outgoing, target)),
        (false, true) => Ok((RequestDirection::Incoming, actor)),
        (true, true) | (false, false) => Err(VenmoApiError::Contract {
            operation,
            problem: "the pending request did not identify exactly one active-account party",
        }),
    }
}

fn parse_timestamp(
    value: Option<String>,
    operation: &'static str,
    problem: &'static str,
) -> Result<OffsetDateTime, VenmoApiError> {
    let value = value.ok_or(VenmoApiError::Contract { operation, problem })?;
    parse_timestamp_value(&value).map_err(|_| VenmoApiError::Contract { operation, problem })
}

fn parse_timestamp_value(value: &str) -> Result<OffsetDateTime, ()> {
    if let Ok(timestamp) = OffsetDateTime::parse(value, &Rfc3339) {
        return Ok(timestamp);
    }
    let format = time::format_description::parse_borrowed::<3>(
        "[year]-[month]-[day]T[hour]:[minute]:[second]",
    )
    .map_err(|_| ())?;
    PrimitiveDateTime::parse(value, &format)
        .map(PrimitiveDateTime::assume_utc)
        .map_err(|_| ())
}

fn bounded_optional_text(
    value: Option<String>,
    operation: &'static str,
    problem: &'static str,
) -> Result<Option<String>, VenmoApiError> {
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_REMOTE_TEXT_BYTES)
    {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

fn bounded_required_text(
    value: String,
    operation: &'static str,
    problem: &'static str,
) -> Result<String, VenmoApiError> {
    if value.is_empty() || value.len() > MAX_REMOTE_TEXT_BYTES {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

fn bounded_required_label(
    value: String,
    operation: &'static str,
    problem: &'static str,
) -> Result<String, VenmoApiError> {
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

fn bounded_optional_label(
    value: Option<String>,
    operation: &'static str,
    problem: &'static str,
) -> Result<Option<String>, VenmoApiError> {
    if value.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > 64 || value.chars().any(char::is_control)
    }) {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

fn validate_page_count(
    operation: &'static str,
    actual: usize,
    requested: NonZeroU32,
) -> Result<(), VenmoApiError> {
    let requested = usize::try_from(requested.get()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the requested page size could not be represented safely",
    })?;
    if actual > requested {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the API returned more records than requested",
        });
    }
    Ok(())
}

fn validate_query_keys(
    operation: &'static str,
    pairs: &[(String, String)],
    allowed: &[&str],
) -> Result<(), VenmoApiError> {
    let mut seen = HashSet::with_capacity(pairs.len());
    for (key, _) in pairs {
        if !allowed.contains(&key.as_str()) || !seen.insert(key.as_str()) {
            return Err(VenmoApiError::Contract {
                operation,
                problem: "the continuation link contained unexpected or duplicate query fields",
            });
        }
    }
    Ok(())
}

fn unique_query_value(
    operation: &'static str,
    pairs: &[(String, String)],
    name: &str,
) -> Result<Option<String>, VenmoApiError> {
    let mut values = pairs
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value);
    let first = values.next().cloned();
    if values.next().is_some() {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the continuation link repeated a query field",
        });
    }
    Ok(first)
}

fn require_query_value(
    operation: &'static str,
    pairs: &[(String, String)],
    name: &str,
    expected: &str,
) -> Result<(), VenmoApiError> {
    if unique_query_value(operation, pairs, name)?.as_deref() != Some(expected) {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the continuation link changed a required query field",
        });
    }
    Ok(())
}

fn require_query_value_case_insensitive(
    operation: &'static str,
    pairs: &[(String, String)],
    name: &str,
    expected: &str,
) -> Result<(), VenmoApiError> {
    if !unique_query_value(operation, pairs, name)?
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the continuation link changed a required query field",
        });
    }
    Ok(())
}

fn shape_probe<T>(shape: RequiredShape, result: Result<T, VenmoApiError>) -> ShapeProbeOutcome {
    match result {
        Ok(_) => ShapeProbeOutcome::passed(shape),
        Err(error) => ShapeProbeOutcome::failed(shape, error.kind()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::error::Error;
    use std::io;
    use std::num::NonZeroU32;
    use std::str::FromStr;
    use std::time::Duration;

    use reqwest::Url;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::application::ports::CredentialStore;
    use crate::infrastructure::credentials::NativeCredentialStore;

    type TestResult = Result<(), Box<dyn Error>>;

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "manually probes production read-only response shapes with the active keychain credential"]
    async fn live_read_only_schema_probe() -> TestResult {
        let loaded = NativeCredentialStore::new().load()?.ok_or_else(|| {
            io::Error::other("the live schema probe requires a stored credential")
        })?;
        let client = VenmoApiClient::production()?;
        let session = ApiSession::from(&loaded.envelope);
        let user_id = loaded.envelope.user_id().as_str();

        let _ = probe_shape(
            &client,
            session,
            "account",
            HttpRequest::read("/account", &["account"], &[]),
        )
        .await?;
        let friends = probe_shape(
            &client,
            session,
            "friends",
            HttpRequest::read(
                "/users/{user-id}/friends",
                &["users", user_id, "friends"],
                &[("limit", "2"), ("offset", "0")],
            ),
        )
        .await?;
        summarize_next_link("friends", friends.as_ref());
        let activity = probe_shape(
            &client,
            session,
            "activity",
            HttpRequest::read(
                "/stories/target-or-actor/{user-id}",
                &["stories", "target-or-actor", user_id],
                &[("limit", "2"), ("social_only", "false")],
            ),
        )
        .await?;
        summarize_next_link("activity", activity.as_ref());
        summarize_timestamp_shapes("activity", activity.as_ref());
        if let Some(value) = activity.as_ref() {
            if let Some(story_id) = value.pointer("/data/0/id").and_then(Value::as_str) {
                let _ = probe_shape(
                    &client,
                    session,
                    "story-detail",
                    HttpRequest::read("/stories/{story-id}", &["stories", story_id], &[]),
                )
                .await?;
            }
            if let Some(payment_id) = value.pointer("/data/0/payment/id").and_then(Value::as_str) {
                let _ = probe_shape(
                    &client,
                    session,
                    "activity-payment-detail",
                    HttpRequest::read("/payments/{payment-id}", &["payments", payment_id], &[]),
                )
                .await?;
            }
        }
        let pending = probe_shape(
            &client,
            session,
            "pending-requests",
            HttpRequest::read(
                "/payments",
                &["payments"],
                &[
                    ("action", "charge"),
                    ("status", "pending,held"),
                    ("limit", "1"),
                ],
            ),
        )
        .await?;
        summarize_next_link("pending-requests", pending.as_ref());
        summarize_request_directions(pending.as_ref(), user_id);
        if let Some(request_id) = pending
            .as_ref()
            .and_then(|value| value.pointer("/data/0/id"))
            .and_then(Value::as_str)
        {
            let _ = probe_shape(
                &client,
                session,
                "pending-request-detail",
                HttpRequest::read("/payments/{payment-id}", &["payments", request_id], &[]),
            )
            .await?;
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "manually probes production activity continuation semantics with the active credential"]
    async fn live_activity_continuation_probe() -> TestResult {
        let loaded = NativeCredentialStore::new().load()?.ok_or_else(|| {
            io::Error::other("the live continuation probe requires a stored credential")
        })?;
        let client = VenmoApiClient::production()?;
        let limit = "11";
        let response = client
            .transport
            .send_authenticated(
                ApiSession::from(&loaded.envelope),
                HttpRequest::read(
                    "/stories/target-or-actor/{user-id}",
                    &[
                        "stories",
                        "target-or-actor",
                        loaded.envelope.user_id().as_str(),
                    ],
                    &[("limit", limit), ("social_only", "false")],
                ),
            )
            .await?;
        let value: Value = serde_json::from_slice(response.body())?;
        summarize_next_link("activity-continuation", Some(&value));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "manually probes non-payment activity shapes with the active credential"]
    async fn live_non_payment_activity_probe() -> TestResult {
        let loaded = NativeCredentialStore::new().load()?.ok_or_else(|| {
            io::Error::other("the live activity-shape probe requires a stored credential")
        })?;
        let client = VenmoApiClient::production()?;
        let response = client
            .transport
            .send_authenticated(
                ApiSession::from(&loaded.envelope),
                HttpRequest::read(
                    "/stories/target-or-actor/{user-id}",
                    &[
                        "stories",
                        "target-or-actor",
                        loaded.envelope.user_id().as_str(),
                    ],
                    &[("limit", "50"), ("social_only", "false")],
                ),
            )
            .await?;
        let value: Value = serde_json::from_slice(response.body())?;
        let records = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| io::Error::other("activity probe did not return an array"))?;
        let mut types = std::collections::BTreeMap::<String, u32>::new();
        let mut first_non_payment = None;
        for record in records {
            let story_type = record
                .get("type")
                .and_then(Value::as_str)
                .and_then(safe_enum_value)
                .unwrap_or("unknown");
            *types.entry(story_type.to_owned()).or_default() += 1;
            if story_type != "payment" && first_non_payment.is_none() {
                first_non_payment = Some(record);
            }
        }
        eprintln!("schema-probe activity type-counts: {types:?}");
        if let Some(record) = first_non_payment {
            let mut shape = BTreeSet::new();
            collect_json_shape(record, "$.data[]", None, 0, &mut shape);
            for line in shape {
                eprintln!("  {line}");
            }
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "manually locates unsupported activity structures without emitting values"]
    async fn live_activity_contract_failure_probe() -> TestResult {
        let loaded = NativeCredentialStore::new().load()?.ok_or_else(|| {
            io::Error::other("the live activity-contract probe requires a stored credential")
        })?;
        let client = VenmoApiClient::production()?;
        let page_size = NonZeroU32::new(50)
            .ok_or_else(|| io::Error::other("probe page size must be nonzero"))?;
        let mut token: Option<ActivityPageToken> = None;
        for page_index in 0_u8..4 {
            let mut query = vec![("limit", "50"), ("social_only", "false")];
            if let Some(before_id) = token.as_ref() {
                query.push(("before_id", before_id.as_str()));
            }
            let path_segments = [
                "stories",
                "target-or-actor",
                loaded.envelope.user_id().as_str(),
            ];
            let response = client
                .transport
                .send_authenticated(
                    ApiSession::from(&loaded.envelope),
                    HttpRequest::read("/stories/target-or-actor/{user-id}", &path_segments, &query),
                )
                .await?;
            let value: Value = serde_json::from_slice(response.body())?;
            let records = value
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| io::Error::other("activity probe did not return an array"))?;
            for record in records {
                let supported = serde_json::from_value::<StoryDto>(record.clone())
                    .ok()
                    .is_some_and(|story| {
                        map_activity(story, loaded.envelope.user_id(), ACTIVITY_LIST_OPERATION)
                            .is_ok()
                    });
                if !supported {
                    eprintln!(
                        "schema-probe unsupported activity record on bounded page {}:",
                        page_index + 1
                    );
                    let mut shape = BTreeSet::new();
                    collect_json_shape(record, "$.data[]", None, 0, &mut shape);
                    for line in shape {
                        eprintln!("  {line}");
                    }
                    return Ok(());
                }
            }
            token = client.parse_activity_next_link(
                value.pointer("/pagination/next").and_then(Value::as_str),
                &path_segments,
                page_size,
            )?;
            if token.is_none() {
                eprintln!("schema-probe activity contract: all bounded records were supported");
                return Ok(());
            }
        }
        eprintln!("schema-probe activity contract: no unsupported record in four bounded pages");
        Ok(())
    }

    async fn probe_shape(
        client: &VenmoApiClient,
        session: ApiSession<'_>,
        label: &str,
        request: HttpRequest<'_>,
    ) -> Result<Option<Value>, Box<dyn Error>> {
        let response = client
            .transport
            .send_authenticated(session, request)
            .await?;
        eprintln!("schema-probe {label}: HTTP {}", response.status().as_u16());
        if response.body().is_empty() {
            eprintln!("  $: empty-body");
            return Ok(None);
        }
        let value: Value = match serde_json::from_slice(response.body()) {
            Ok(value) => value,
            Err(_) => {
                eprintln!("  $: non-json-body");
                return Ok(None);
            }
        };
        let mut shape = BTreeSet::new();
        collect_json_shape(&value, "$", None, 0, &mut shape);
        for line in shape {
            eprintln!("  {line}");
        }
        Ok(Some(value))
    }

    fn summarize_next_link(label: &str, value: Option<&Value>) {
        let Some(next) = value
            .and_then(|value| value.pointer("/pagination/next"))
            .and_then(Value::as_str)
        else {
            eprintln!("schema-probe {label} pagination: no-next-link");
            return;
        };
        let Ok(url) = reqwest::Url::parse(next) else {
            eprintln!("schema-probe {label} pagination: unparseable-next-link");
            return;
        };
        let trusted_origin = url.scheme() == "https"
            && url.host_str() == Some("api.venmo.com")
            && url.port_or_known_default() == Some(443);
        let query_keys = url
            .query_pairs()
            .map(|(key, _)| key.into_owned())
            .collect::<BTreeSet<_>>();
        let safe_values = url
            .query_pairs()
            .filter_map(|(key, value)| {
                matches!(
                    key.as_ref(),
                    "action"
                        | "limit"
                        | "offset"
                        | "only_public_stories"
                        | "social_only"
                        | "status"
                )
                .then(|| format!("{key}={value}"))
            })
            .collect::<BTreeSet<_>>();
        eprintln!(
            "schema-probe {label} pagination: trusted-origin={trusted_origin} query-keys={query_keys:?} safe-values={safe_values:?}"
        );
    }

    fn summarize_request_directions(value: Option<&Value>, user_id: &str) {
        let Some(records) = value
            .and_then(|value| value.get("data"))
            .and_then(Value::as_array)
        else {
            eprintln!("schema-probe pending-requests directions: unavailable");
            return;
        };
        let mut incoming = 0_u32;
        let mut outgoing = 0_u32;
        let mut unknown = 0_u32;
        for record in records {
            let actor = record.pointer("/actor/id").and_then(Value::as_str);
            let target = record.pointer("/target/user/id").and_then(Value::as_str);
            if actor == Some(user_id) {
                outgoing = outgoing.saturating_add(1);
            } else if target == Some(user_id) {
                incoming = incoming.saturating_add(1);
            } else {
                unknown = unknown.saturating_add(1);
            }
        }
        eprintln!(
            "schema-probe pending-requests directions: incoming={incoming} outgoing={outgoing} unknown={unknown}"
        );
    }

    fn summarize_timestamp_shapes(label: &str, value: Option<&Value>) {
        let Some(records) = value
            .and_then(|value| value.get("data"))
            .and_then(Value::as_array)
        else {
            eprintln!("schema-probe {label} timestamps: unavailable");
            return;
        };
        let mut shapes = BTreeSet::new();
        for record in records {
            for (field, candidate) in [
                ("story.date_created", record.get("date_created")),
                (
                    "payment.date_created",
                    record.pointer("/payment/date_created"),
                ),
            ] {
                match candidate.and_then(Value::as_str) {
                    Some(value) => {
                        shapes.insert(format!("{field}: {}", timestamp_shape(value)));
                    }
                    None => {
                        shapes.insert(format!("{field}: absent-or-non-string"));
                    }
                }
            }
        }
        eprintln!("schema-probe {label} timestamps: {shapes:?}");
    }

    fn timestamp_shape(value: &str) -> String {
        let timezone = if value.ends_with('Z') {
            "zulu"
        } else if value
            .get(10..)
            .is_some_and(|suffix| suffix.contains('+') || suffix.rfind('-').is_some())
        {
            "numeric-offset"
        } else {
            "no-offset"
        };
        let fractional_digits = value.split_once('.').map_or(0, |(_, suffix)| {
            suffix.bytes().take_while(u8::is_ascii_digit).count()
        });
        format!(
            "bytes={} has-T={} timezone={timezone} fractional-digits={fractional_digits} rfc3339={}",
            value.len(),
            value.contains('T'),
            OffsetDateTime::parse(value, &Rfc3339).is_ok()
        )
    }

    fn collect_json_shape(
        value: &Value,
        path: &str,
        field: Option<&str>,
        depth: usize,
        shape: &mut BTreeSet<String>,
    ) {
        const MAX_DEPTH: usize = 12;
        if depth > MAX_DEPTH {
            shape.insert(format!("{path}: depth-limit"));
            return;
        }
        match value {
            Value::Null => {
                shape.insert(format!("{path}: null"));
            }
            Value::Bool(_) => {
                shape.insert(format!("{path}: bool"));
            }
            Value::Number(_) => {
                shape.insert(format!("{path}: number"));
            }
            Value::String(value) => {
                let enum_value = field
                    .filter(|field| is_allowlisted_enum_field(field))
                    .and_then(|_| safe_enum_value(value));
                match enum_value {
                    Some(value) => {
                        shape.insert(format!("{path}: string enum={value}"));
                    }
                    None => {
                        shape.insert(format!("{path}: string"));
                    }
                }
            }
            Value::Array(values) => {
                shape.insert(format!("{path}: array length={}", values.len()));
                for value in values {
                    collect_json_shape(value, &format!("{path}[]"), field, depth + 1, shape);
                }
            }
            Value::Object(fields) => {
                shape.insert(format!("{path}: object"));
                for (key, value) in fields {
                    let safe_key = safe_schema_key(key).unwrap_or("[dynamic-key]");
                    collect_json_shape(
                        value,
                        &format!("{path}.{safe_key}"),
                        Some(safe_key),
                        depth + 1,
                        shape,
                    );
                }
            }
        }
    }

    fn safe_schema_key(value: &str) -> Option<&str> {
        const MAX_KEY_BYTES: usize = 64;
        (!value.is_empty()
            && value.len() <= MAX_KEY_BYTES
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
        .then_some(value)
    }

    fn is_allowlisted_enum_field(value: &str) -> bool {
        matches!(value, "action" | "audience" | "status" | "type")
    }

    fn safe_enum_value(value: &str) -> Option<&str> {
        const MAX_ENUM_BYTES: usize = 32;
        (!value.is_empty()
            && value.len() <= MAX_ENUM_BYTES
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
        .then_some(value)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn password_login_uses_device_only_and_returns_issued_token() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/oauth/access_token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_json(serde_json::json!({
                "phone_email_or_username": "alice@example.com",
                "client_id": "1",
                "password": "synthetic-password"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "synthetic-issued-token"
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
        let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
        let device_id = DeviceId::from_str("synthetic-device")?;

        let result = client
            .begin_password_login(&identifier, &password, &device_id)
            .await?;

        match result {
            PasswordLoginStart::Authenticated(token) => {
                assert_eq!(token.expose_secret(), "synthetic-issued-token");
            }
            PasswordLoginStart::OtpRequired(_) => {
                return Err(io::Error::other("unexpected OTP challenge").into());
            }
        }
        let requests = server.received_requests().await;
        assert!(requests.as_ref().is_some_and(|requests| {
            requests.len() == 1 && requests[0].headers.get("authorization").is_none()
        }));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn password_login_returns_a_redacted_otp_challenge() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/oauth/access_token"))
            .respond_with(
                ResponseTemplate::new(401)
                    .insert_header("venmo-otp-secret", "synthetic-otp-secret")
                    .set_body_json(serde_json::json!({
                        "error": {"code": 81109, "message": "private remote message"}
                    })),
            )
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
        let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
        let device_id = DeviceId::from_str("synthetic-device")?;

        let result = client
            .begin_password_login(&identifier, &password, &device_id)
            .await?;

        match result {
            PasswordLoginStart::OtpRequired(secret) => {
                assert_eq!(secret.expose(), "synthetic-otp-secret");
                assert!(!format!("{secret:?}").contains("synthetic-otp-secret"));
            }
            PasswordLoginStart::Authenticated(_) => {
                return Err(io::Error::other("expected OTP challenge").into());
            }
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn otp_completion_and_device_trust_use_exact_sensitive_headers() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/account/two-factor/token"))
            .and(header("device-id", "synthetic-device"))
            .and(header("venmo-otp-secret", "synthetic-otp-secret"))
            .and(body_json(serde_json::json!({"via": "sms"})))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/oauth/access_token"))
            .and(query_param("client_id", "1"))
            .and(header("device-id", "synthetic-device"))
            .and(header("venmo-otp-secret", "synthetic-otp-secret"))
            .and(header("venmo-otp", "123456"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "synthetic-issued-token"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/users/devices"))
            .and(header("authorization", "Bearer synthetic-issued-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let device_id = DeviceId::from_str("synthetic-device")?;
        let otp_secret = OtpSecret::parse_owned("synthetic-otp-secret".to_owned())?;
        let otp_code = OtpCode::parse_owned("123456".to_owned())?;

        client.request_sms_otp(&otp_secret, &device_id).await?;
        let token = client
            .complete_otp_login(&otp_code, &otp_secret, &device_id)
            .await?;
        client.trust_device(&token, &device_id).await?;

        assert_eq!(token.expose_secret(), "synthetic-issued-token");
        assert_request_count(&server, 3).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_otp_challenge_fails_without_exposing_remote_values() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/oauth/access_token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {"code": 81109, "message": "private remote message"}
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
        let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
        let device_id = DeviceId::from_str("synthetic-device")?;

        let result = client
            .begin_password_login(&identifier, &password, &device_id)
            .await;

        assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        if let Err(error) = result {
            let rendered = error.to_string();
            assert!(!rendered.contains("private remote message"));
            assert!(!rendered.contains("synthetic-password"));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn current_account_maps_wrapped_and_direct_envelopes() -> TestResult {
        for body in [
            r#"{"data":{"user":{"id":123,"username":"alice","display_name":"Alice"}}}"#,
            r#"{"data":{"id":"123","username":"alice","displayName":"Alice"}}"#,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/account"))
                .and(header("authorization", "Bearer synthetic-token"))
                .and(header("device-id", "synthetic-device"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let account = client.current_account(&token, &device_id).await?;

            assert_eq!(account.user_id().as_str(), "123");
            assert_eq!(account.username().as_str(), "alice");
            assert_eq!(account.display_name(), Some("Alice"));
            assert_request_count(&server, 1).await;
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn revocation_uses_delete_and_accepts_an_empty_success() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/oauth/access_token"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;

        client.revoke_access_token(&token, &device_id).await?;

        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn payment_methods_map_supported_envelopes_and_default_roles() -> TestResult {
        for body in [
            r#"{"data":[{"id":"balance-1","type":"balance","name":"Venmo balance","last_four":null,"peer_payment_role":"default"},{"id":"bank-1","payment_method_type":"bank","display_name":"Bank","lastFour":1234,"isDefault":false}]}"#,
            r#"{"data":{"payment_methods":[{"id":123,"label":"Card","type":"card","merchant_payment_role":"backup"}]}}"#,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/payment-methods"))
                .and(header("authorization", "Bearer synthetic-token"))
                .and(header("device-id", "synthetic-device"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let methods = client.payment_methods(&token, &device_id).await?;

            assert!(!methods.is_empty());
            assert_request_count(&server, 1).await;
            match methods.as_slice() {
                [first, second] => {
                    assert_eq!(first.id().as_str(), "balance-1");
                    assert_eq!(first.name(), Some("Venmo balance"));
                    assert_eq!(first.method_type(), Some("balance"));
                    assert!(first.is_default());
                    assert_eq!(second.last_four(), Some("1234"));
                    assert!(!second.is_default());
                }
                [only] => {
                    assert_eq!(only.id().as_str(), "123");
                    assert_eq!(only.name(), Some("Card"));
                }
                _ => {
                    return Err(io::Error::other("unexpected payment-method count").into());
                }
            }
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn payment_methods_reject_duplicate_or_invalid_ids() -> TestResult {
        for body in [
            r#"{"data":[{"id":"same"},{"id":"same"}]}"#,
            r#"{"data":[{"id":"bad id"}]}"#,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/payment-methods"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client.payment_methods(&token, &device_id).await;

            assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn peer_funding_uses_only_peer_roles_and_explicit_fee_evidence() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payment-methods"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {
                        "id": "balance-1",
                        "type": "balance",
                        "name": "Venmo balance",
                        "peer_payment_role": "default",
                        "fee": {"calculated_fee_amount_in_cents": 0}
                    },
                    {
                        "id": "bank-1",
                        "type": "bank",
                        "name": "Bank",
                        "peer_payment_role": "backup",
                        "merchant_payment_role": "none",
                        "fee": {"calculated_fee_amount_in_cents": 0}
                    },
                    {
                        "id": "card-1",
                        "type": "card",
                        "name": "Card",
                        "peer_payment_role": "backup",
                        "merchant_payment_role": "default",
                        "fee": {"calculated_fee_amount_in_cents": 3}
                    },
                    {
                        "id": "excluded-1",
                        "peer_payment_role": "none",
                        "merchant_payment_role": "default"
                    }
                ]
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;

        let methods = client.peer_funding_methods(&token, &device_id).await?;

        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].role(), PeerFundingRole::Backup);
        assert_eq!(methods[0].fee(), PeerFundingFee::ProvenZero);
        assert_eq!(methods[1].role(), PeerFundingRole::Backup);
        assert_eq!(methods[1].fee(), PeerFundingFee::NonZero { cents: 3 });
        assert!(
            !methods
                .iter()
                .any(|method| matches!(method.method().id().as_str(), "balance-1" | "excluded-1"))
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn peer_funding_rejects_missing_unknown_or_duplicate_peer_contracts() -> TestResult {
        for body in [
            serde_json::json!({"data": [{"id": "one"}]}),
            serde_json::json!({"data": [{"id": "one", "peer_payment_role": "surprise"}]}),
            serde_json::json!({"data": [{
                "id": "one",
                "type": "mystery",
                "peer_payment_role": "backup"
            }]}),
            serde_json::json!({"data": [
                {"id": "one", "peer_payment_role": "none"},
                {"id": "one", "peer_payment_role": "backup"}
            ]}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/payment-methods"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client.peer_funding_methods(&token, &device_id).await;

            assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn peer_funding_preserves_an_unrecognized_method_fee_as_unknown() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payment-methods"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "bank-1",
                    "type": "bank",
                    "name": "Bank",
                    "peer_payment_role": "default",
                    "fee": 0
                }]
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;

        let methods = client.peer_funding_methods(&token, &device_id).await?;

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fee(), PeerFundingFee::Unknown);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn payment_eligibility_uses_integer_cents_and_returns_a_redacted_token() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/protection/eligibility"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_json(serde_json::json!({
                "funding_source_id": "",
                "action": "pay",
                "country_code": "1",
                "target_type": "user_id",
                "note": "Synthetic note",
                "target_id": "456",
                "amount": 1
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "eligibility_token": "synthetic-eligibility-token",
                    "eligible": true,
                    "fees": [{"calculated_fee_amount_in_cents": 0}],
                    "fee_disclaimer": "Synthetic zero fee",
                    "ineligible_reason": null
                }
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let recipient = financial_user("456", "bob")?;
        let amount = Money::from_cents(1)?;
        let note = crate::domain::Note::from_str("Synthetic note")?;

        let eligibility = client
            .payment_eligibility(&token, &device_id, &recipient, amount, &note)
            .await?;

        assert_eq!(eligibility.fee_cents(), 0);
        assert_eq!(eligibility.token().expose(), "synthetic-eligibility-token");
        assert!(!format!("{:?}", eligibility.token()).contains("synthetic-eligibility-token"));
        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ineligible_payment_is_a_confirmed_prewrite_rejection() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/protection/eligibility"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "eligibility_token": "synthetic-eligibility-token",
                    "eligible": false,
                    "fees": [],
                    "fee_disclaimer": "Not eligible",
                    "ineligible_reason": "synthetic_reason"
                }
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let recipient = financial_user("456", "bob")?;
        let note = crate::domain::Note::from_str("Synthetic note")?;

        let result = client
            .payment_eligibility(&token, &device_id, &recipient, Money::from_cents(1)?, &note)
            .await;

        assert!(matches!(result, Err(VenmoApiError::EligibilityDenied)));
        assert_eq!(
            result.as_ref().err().map(ApiFailure::kind),
            Some(ApiFailureKind::Rejected)
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn payment_creation_sends_exact_candidate_body_and_validates_success() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_json(serde_json::json!({
                "uuid": "123e4567-e89b-12d3-a456-426614174000",
                "user_id": "456",
                "audience": "private",
                "amount": 0.01,
                "note": "Synthetic note",
                "eligibility_token": "synthetic-eligibility-token",
                "funding_source_id": "bank-1"
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(created_payment_body(
                    "payment-1",
                    "pay",
                    "settled",
                    "123",
                    "456",
                )),
            )
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let plan = pay_plan()?;

        let created = client.create_payment(&token, &device_id, &plan).await?;

        assert_eq!(created.id().as_str(), "payment-1");
        assert_eq!(created.status(), FinancialStatus::Settled);
        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn request_creation_sends_negative_amount_without_payment_only_fields() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_json(serde_json::json!({
                "uuid": "123e4567-e89b-12d3-a456-426614174000",
                "user_id": "456",
                "audience": "private",
                "amount": -0.01,
                "note": "Synthetic note"
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(created_payment_body(
                    "request-1",
                    "charge",
                    "pending",
                    "123",
                    "456",
                )),
            )
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let plan = request_plan()?;

        let created = client.create_request(&token, &device_id, &plan).await?;

        assert_eq!(created.id().as_str(), "request-1");
        assert_eq!(created.status(), FinancialStatus::Pending);
        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_mismatched_and_unverified_write_responses_are_ambiguous() -> TestResult {
        let direct_payment =
            created_payment_body("payment-1", "pay", "settled", "123", "456")["data"]["payment"]
                .clone();
        let mut missing_timestamp =
            created_payment_body("payment-1", "pay", "settled", "123", "456");
        missing_timestamp["data"]["payment"]["date_created"] = Value::Null;
        let mut invalid_timestamp = missing_timestamp.clone();
        invalid_timestamp["data"]["payment"]["date_created"] = Value::String("invalid".to_owned());
        let bodies = [
            (200_u16, "not-json".to_owned()),
            (200, serde_json::json!({"data": direct_payment}).to_string()),
            (200, missing_timestamp.to_string()),
            (200, invalid_timestamp.to_string()),
            (
                200,
                created_payment_body("payment-1", "pay", "settled", "123", "999").to_string(),
            ),
            (
                500,
                serde_json::json!({"error": {"code": "unknown"}}).to_string(),
            ),
            (
                500,
                serde_json::json!({"error": {"code": "1396"}}).to_string(),
            ),
            (400, serde_json::json!({"error_code": "1396"}).to_string()),
            (
                200,
                serde_json::json!({"error": {"code": "1396"}}).to_string(),
            ),
        ];
        for (status, body) in bodies {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/payments"))
                .respond_with(ResponseTemplate::new(status).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client
                .create_payment(&token, &device_id, &pay_plan()?)
                .await;

            assert!(matches!(
                result,
                Err(VenmoApiError::FinancialOutcomeUnknown { .. })
            ));
            assert_eq!(
                result.as_ref().err().map(ApiFailure::kind),
                Some(ApiFailureKind::AmbiguousWrite)
            );
        }
        Ok(())
    }

    #[test]
    fn financial_json_numbers_preserve_every_cent_exactly() -> TestResult {
        let largest = Money::from_cents(u64::MAX)?;

        let payment = money_json_number(largest, false, PAYMENT_CREATION_OPERATION)?;
        let request = money_json_number(largest, true, REQUEST_CREATION_OPERATION)?;

        assert_eq!(payment.to_string(), "184467440737095516.15");
        assert_eq!(request.to_string(), "-184467440737095516.15");
        assert_eq!(serde_json::to_string(&payment)?, "184467440737095516.15");
        assert_eq!(serde_json::to_string(&request)?, "-184467440737095516.15");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn only_dossier_known_payment_errors_are_confirmed_rejections() -> TestResult {
        for code in ["1396", "13006"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/payments"))
                .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": {"code": code}
                })))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client
                .create_payment(&token, &device_id, &pay_plan()?)
                .await;

            assert!(matches!(result, Err(VenmoApiError::Http { .. })));
            assert_eq!(
                result.as_ref().err().map(ApiFailure::kind),
                Some(ApiFailureKind::Rejected)
            );
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {"code": "13006"}
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let request_result = client
            .create_request(&token, &device_id, &request_plan()?)
            .await;
        assert_eq!(
            request_result.as_ref().err().map(ApiFailure::kind),
            Some(ApiFailureKind::AmbiguousWrite)
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_search_maps_users_and_uses_bounded_offset_queries() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/users"))
            .and(query_param("query", "alice"))
            .and(query_param("type", "username"))
            .and(query_param("limit", "2"))
            .and(query_param("offset", "50"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"data":{"users":[{"id":51,"username":"alice","display_name":"Alice"},{"id":"52","username":"@alice2","name":"Alice Two"}]}}"#,
                "application/json",
            ))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let query = UserSearchQuery::from_str("@alice")?;
        let page_size = NonZeroU32::new(2)
            .ok_or_else(|| io::Error::other("synthetic page size must be nonzero"))?;

        let page = client
            .search_users(
                &token,
                &device_id,
                &query,
                UserSearchPageRequest::new(page_size, Some(UserSearchPageToken::from_offset(50))),
            )
            .await?;
        let (users, next) = page.into_parts();

        assert_eq!(users.len(), 2);
        assert_eq!(
            users.first().and_then(User::username).map(Username::as_str),
            Some("alice")
        );
        assert_eq!(users.last().and_then(User::display_name), Some("Alice Two"));
        assert_eq!(next.map(UserSearchPageToken::offset), Some(52));
        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_search_rejects_invalid_records_and_oversized_pages() -> TestResult {
        for body in [
            r#"{"data":[{"id":"not-numeric","username":"alice"}]}"#,
            r#"{"data":[{"id":"1"},{"id":"2"}]}"#,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/users"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;
            let query = UserSearchQuery::from_str("alice")?;
            let page_size = NonZeroU32::new(1)
                .ok_or_else(|| io::Error::other("synthetic page size must be nonzero"))?;

            let result = client
                .search_users(
                    &token,
                    &device_id,
                    &query,
                    UserSearchPageRequest::new(page_size, None),
                )
                .await;

            assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_lookup_maps_supported_envelopes_and_exact_id() -> TestResult {
        for body in [
            r#"{"data":{"user":{"id":123,"username":"alice","display_name":"Alice","identity_type":"personal","is_payable":true}}}"#,
            r#"{"data":{"id":"123","username":"@alice","name":"Alice","identity_type":"personal","is_payable":true}}"#,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/users/123"))
                .and(header("authorization", "Bearer synthetic-token"))
                .and(header("device-id", "synthetic-device"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;
            let user_id = UserId::from_str("123")?;

            let user = client.user_by_id(&token, &device_id, &user_id).await?;

            assert_eq!(user.user_id(), &user_id);
            assert_eq!(user.username().map(Username::as_str), Some("alice"));
            assert_eq!(user.display_name(), Some("Alice"));
            assert_eq!(user.profile_kind(), Some(UserProfileKind::Personal));
            assert_eq!(user.is_payable(), Some(true));
            assert_request_count(&server, 1).await;
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_lookup_rejects_mismatched_or_invalid_ids() -> TestResult {
        for body in [
            r#"{"data":{"user":{"id":"124","username":"alice"}}}"#,
            r#"{"data":{"id":"not-numeric","username":"alice"}}"#,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/users/123"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;
            let user_id = UserId::from_str("123")?;

            let result = client.user_by_id(&token, &device_id, &user_id).await;

            assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn balance_maps_exact_signed_available_and_on_hold_fields() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/account"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "balance": "12.34",
                    "balance_on_hold": "-0.05",
                    "user": {"id": "123", "username": "alice"}
                }
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;

        let balance = client.balance(&token, &device_id).await?;

        assert_eq!(balance.available().cents(), 1_234);
        assert_eq!(balance.on_hold().cents(), -5);
        assert_eq!(balance.available().to_string(), "$12.34");
        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn balance_rejects_missing_or_lossy_values() -> TestResult {
        for body in [
            serde_json::json!({"data": {"balance": "1.00"}}),
            serde_json::json!({"data": {"balance": "1.001", "balance_on_hold": "0.00"}}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/account"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client.balance(&token, &device_id).await;

            assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn friends_map_records_and_validate_offset_continuations() -> TestResult {
        let server = MockServer::start().await;
        let next = format!("{}/v1/users/123/friends?limit=2&offset=4", server.uri());
        Mock::given(method("GET"))
            .and(path("/v1/users/123/friends"))
            .and(query_param("limit", "2"))
            .and(query_param("offset", "2"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "40", "username": "friend1", "display_name": "Friend One"},
                    {"id": 41, "username": "@friend2", "display_name": "Friend Two"}
                ],
                "pagination": {"next": next, "previous": null}
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;
        let size = NonZeroU32::new(2).ok_or_else(|| io::Error::other("nonzero test size"))?;

        let page = client
            .friends(
                &token,
                &device_id,
                &user_id,
                FriendsPageRequest::new(size, Some(FriendsPageToken::from_offset(2))),
            )
            .await?;
        let (users, next) = page.into_parts();

        assert_eq!(users.len(), 2);
        assert_eq!(
            users.first().and_then(User::display_name),
            Some("Friend One")
        );
        assert_eq!(next.map(FriendsPageToken::offset), Some(4));
        assert_request_count(&server, 1).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn friends_reject_untrusted_continuation_origins() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/users/123/friends"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [],
                "pagination": {
                    "next": "https://untrusted.example/v1/users/123/friends?limit=1&offset=1"
                }
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;

        let result = client
            .friends(
                &token,
                &device_id,
                &user_id,
                FriendsPageRequest::new(NonZeroU32::MIN, None),
            )
            .await;

        assert!(matches!(
            result,
            Err(VenmoApiError::Transport(
                TransportError::InvalidContinuationLink
            ))
        ));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activity_list_and_detail_use_story_ids_and_verified_party_direction() -> TestResult {
        let server = MockServer::start().await;
        let next = format!(
            "{}/v1/stories/target-or-actor/123?before_id=story-2&limit=1&only_public_stories=False&social_only=False",
            server.uri()
        );
        let story = serde_json::json!({
            "id": "story-1",
            "date_created": "2026-07-11T12:00:00",
            "note": "Dinner",
            "audience": "private",
            "payment": {
                "id": "payment-1",
                "status": "settled",
                "action": "pay",
                "amount": 1.25,
                "actor": {"id": "123", "username": "alice"},
                "target": {"user": {"id": "456", "username": "bob", "display_name": "Bob"}},
                "audience": "private",
                "date_created": "2026-07-11T12:00:00"
            }
        });
        Mock::given(method("GET"))
            .and(path("/v1/stories/target-or-actor/123"))
            .and(query_param("limit", "1"))
            .and(query_param("social_only", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [story.clone()],
                "pagination": {"next": next}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/stories/story-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": story
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;
        let activity_id = ActivityId::from_str("story-1")?;

        let page = client
            .activity(
                &token,
                &device_id,
                &user_id,
                ActivityPageRequest::new(NonZeroU32::MIN, None),
            )
            .await?;
        let (activities, next) = page.into_parts();
        let detail = client
            .activity_by_id(&token, &device_id, &user_id, &activity_id)
            .await?;

        assert_eq!(activities.len(), 1);
        assert_eq!(detail.id(), &activity_id);
        assert_eq!(detail.direction(), ActivityDirection::Outgoing);
        assert_eq!(detail.amount().cents(), 125);
        assert_eq!(
            detail
                .counterparty()
                .as_user()
                .map(|user| user.user_id().as_str()),
            Some("456")
        );
        assert_eq!(
            next.as_ref().map(ActivityPageToken::as_str),
            Some("story-2")
        );
        assert_request_count(&server, 2).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activity_detail_rejects_mismatched_story_ids() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/stories/story-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(activity_body("story-2")))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;
        let activity_id = ActivityId::from_str("story-1")?;

        let result = client
            .activity_by_id(&token, &device_id, &user_id, &activity_id)
            .await;

        assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activity_list_maps_external_transfer_records() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/stories/target-or-actor/123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "story-transfer",
                    "date_created": "2026-07-11T12:00:00",
                    "note": null,
                    "audience": "private",
                    "payment": null,
                    "transfer": {
                        "id": 789,
                        "status": "issued",
                        "type": "standard",
                        "amount": 12.34,
                        "date_requested": "2026-07-11T12:00:00",
                        "destination": {
                            "name": "Synthetic bank",
                            "type": "bank",
                            "last_four": "1234"
                        }
                    }
                }, {
                    "id": "story-add-funds",
                    "date_created": "2026-07-11T13:00:00",
                    "note": null,
                    "audience": "private",
                    "payment": null,
                    "transfer": {
                        "id": 790,
                        "status": "complete",
                        "type": "add_funds",
                        "amount": "5.00",
                        "date_requested": "2026-07-11T13:00:00",
                        "source": {
                            "name": "Synthetic source",
                            "type": "bank",
                            "last_four": 5678
                        }
                    }
                }, {
                    "id": "story-authorization",
                    "date_created": "2026-07-11T14:00:00",
                    "note": "Synthetic purchase",
                    "audience": "private",
                    "payment": null,
                    "transfer": null,
                    "authorization": {
                        "id": "authorization-1",
                        "status": "captured",
                        "amount": "2.50",
                        "created_at": "2026-07-11T14:00:00",
                        "descriptor": "Synthetic descriptor",
                        "merchant": {"display_name": "Synthetic merchant"},
                        "user": {"id": "123", "username": "alice"}
                    }
                }],
                "pagination": {"next": null}
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;

        let page_size = NonZeroU32::new(3)
            .ok_or_else(|| io::Error::other("transfer test page size must be nonzero"))?;
        let page = client
            .activity(
                &token,
                &device_id,
                &user_id,
                ActivityPageRequest::new(page_size, None),
            )
            .await?;
        let (activities, next) = page.into_parts();
        let activity = activities
            .first()
            .ok_or_else(|| io::Error::other("missing mapped transfer activity"))?;
        let add_funds = activities
            .get(1)
            .ok_or_else(|| io::Error::other("missing mapped add-funds activity"))?;
        let authorization = activities
            .last()
            .ok_or_else(|| io::Error::other("missing mapped authorization activity"))?;

        assert!(next.is_none());
        assert_eq!(activities.len(), 3);
        assert_eq!(activity.action().as_str(), "transfer:standard");
        assert_eq!(activity.status().as_str(), "issued");
        assert_eq!(activity.direction(), ActivityDirection::Outgoing);
        assert_eq!(activity.amount().cents(), 1_234);
        assert_eq!(
            activity.counterparty().external_parts(),
            Some(("Synthetic bank", "bank", Some("1234")))
        );
        assert_eq!(add_funds.action().as_str(), "transfer:add_funds");
        assert_eq!(add_funds.direction(), ActivityDirection::Incoming);
        assert_eq!(
            add_funds.counterparty().external_parts(),
            Some(("Synthetic source", "bank", Some("5678")))
        );
        assert_eq!(authorization.action().as_str(), "authorization");
        assert_eq!(authorization.status().as_str(), "captured");
        assert_eq!(authorization.direction(), ActivityDirection::Outgoing);
        assert_eq!(
            authorization.counterparty().external_parts(),
            Some(("Synthetic merchant", "merchant", None))
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_requests_map_both_directions_and_validate_detail_ids() -> TestResult {
        let server = MockServer::start().await;
        let next = format!(
            "{}/v1/payments?action=charge&before=request-3&limit=2&status=pending%2Cheld",
            server.uri()
        );
        let outgoing = request_body("request-1", "123", "456", "pending");
        let incoming = request_body("request-2", "789", "123", "held");
        Mock::given(method("GET"))
            .and(path("/v1/payments"))
            .and(query_param("action", "charge"))
            .and(query_param("status", "pending,held"))
            .and(query_param("limit", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [outgoing.clone(), incoming],
                "pagination": {"next": next}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/payments/request-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": outgoing
            })))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;
        let request_id = RequestId::from_str("request-1")?;
        let size = NonZeroU32::new(2).ok_or_else(|| io::Error::other("nonzero test size"))?;

        let page = client
            .pending_requests(
                &token,
                &device_id,
                &user_id,
                PendingRequestsPageRequest::new(size, None),
            )
            .await?;
        let (requests, next) = page.into_parts();
        let detail = client
            .pending_request_by_id(&token, &device_id, &user_id, &request_id)
            .await?;

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].direction(), RequestDirection::Outgoing);
        assert_eq!(requests[1].direction(), RequestDirection::Incoming);
        assert_eq!(detail.id(), &request_id);
        assert_eq!(
            next.as_ref().map(PendingRequestsPageToken::as_str),
            Some("request-3")
        );
        assert_request_count(&server, 2).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_requests_reject_non_charge_or_non_pending_records() -> TestResult {
        for (action, status) in [("pay", "pending"), ("charge", "settled")] {
            let server = MockServer::start().await;
            let mut body = request_body("request-1", "123", "456", status);
            if let Some(object) = body.as_object_mut() {
                object.insert("action".to_owned(), Value::String(action.to_owned()));
            }
            Mock::given(method("GET"))
                .and(path("/v1/payments"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": [body],
                    "pagination": {"next": null}
                })))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;
            let user_id = UserId::from_str("123")?;

            let result = client
                .pending_requests(
                    &token,
                    &device_id,
                    &user_id,
                    PendingRequestsPageRequest::new(NonZeroU32::MIN, None),
                )
                .await;

            assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_request_detail_preserves_terminal_state_for_mutation_preflight() -> TestResult
    {
        for (action, status, actor, target, direction) in [
            (
                "charge",
                "cancelled",
                "456",
                "123",
                RequestDirection::Incoming,
            ),
            ("pay", "settled", "123", "456", RequestDirection::Outgoing),
        ] {
            let server = MockServer::start().await;
            let mut body = request_body("request-1", actor, target, status);
            body["action"] = Value::String(action.to_owned());
            Mock::given(method("GET"))
                .and(path("/v1/payments/request-1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": body
                })))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;
            let user_id = UserId::from_str("123")?;
            let request_id = RequestId::from_str("request-1")?;

            let detail = client
                .pending_request_by_id(&token, &device_id, &user_id, &request_id)
                .await?;

            assert_eq!(detail.status().as_str(), status);
            assert_eq!(detail.direction(), direction);
            assert_eq!(
                detail.action(),
                if action == "charge" {
                    PendingRequestAction::Charge
                } else {
                    PendingRequestAction::Pay
                }
            );
            assert_request_count(&server, 1).await;
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn doctor_connectivity_probe_is_read_only_and_unauthenticated() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/account"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let client = test_client(&server)?;

        client.connectivity().await?;

        let requests = server.received_requests().await;
        assert!(requests.as_ref().is_some_and(|requests| {
            requests.len() == 1
                && requests[0].headers.get("authorization").is_none()
                && requests[0].headers.get("device-id").is_none()
        }));
        Ok(())
    }

    #[test]
    fn api_failure_kinds_preserve_operational_categories() {
        assert_eq!(
            VenmoApiError::Transport(TransportError::Timeout).kind(),
            ApiFailureKind::Timeout
        );
        assert_eq!(
            VenmoApiError::Transport(TransportError::Network).kind(),
            ApiFailureKind::Network
        );
        assert_eq!(
            VenmoApiError::Contract {
                operation: CURRENT_ACCOUNT_OPERATION,
                problem: "synthetic contract failure",
            }
            .kind(),
            ApiFailureKind::Contract
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn errors_expose_only_safe_status_and_code() -> TestResult {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "error": {
                "code": "AUTH-1",
                "message": "secret\u{1b}[31mtext",
            }
        })
        .to_string();
        Mock::given(method("GET"))
            .and(path("/v1/account"))
            .respond_with(ResponseTemplate::new(401).set_body_raw(body, "application/json"))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;

        let error = client.current_account(&token, &device_id).await;

        assert!(matches!(
            error,
            Err(VenmoApiError::Http { status: 401, .. })
        ));
        if let Err(error) = error {
            let rendered = error.to_string();
            assert!(rendered.contains("AUTH-1"));
            assert!(!rendered.contains("secret"));
            assert!(!rendered.contains('\u{1b}'));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_or_incomplete_success_is_a_contract_error() -> TestResult {
        for body in ["not-json", r#"{"data":{"user":{"id":"123"}}}"#] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/account"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client.current_account(&token, &device_id).await;

            assert!(matches!(
                result,
                Err(VenmoApiError::MalformedJson { .. } | VenmoApiError::Contract { .. })
            ));
        }
        Ok(())
    }

    #[test]
    fn unsafe_error_codes_are_not_rendered() {
        assert_eq!(sanitize_api_code("AUTH-1"), Some("AUTH-1".to_owned()));
        assert_eq!(sanitize_api_code("bad\ncode"), None);
        assert_eq!(sanitize_api_code(&"x".repeat(65)), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn request_acceptance_uses_exact_approve_update_and_validates_settlement() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/payments/request-1"))
            .and(header("accept", "application/json"))
            .and(header("content-type", "application/json"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_json(serde_json::json!({"action": "approve"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(updated_payment_body("pay", "settled", "123", "456")),
            )
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let plan = accept_plan()?;

        let accepted = client.accept_request(&token, &device_id, &plan).await?;

        assert_eq!(accepted.payment_id().as_str(), "request-1");
        assert_eq!(accepted.status(), FinancialStatus::Settled);
        assert_request_count(&server, 1).await;
        assert_requests_have_no_query(&server).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn request_decline_uses_deny_not_cancel_and_requires_terminal_response() -> TestResult {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/payments/request-1"))
            .and(header("accept", "application/json"))
            .and(header("content-type", "application/json"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_json(serde_json::json!({"action": "deny"})))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(updated_payment_body(
                    "charge",
                    "cancelled",
                    "456",
                    "123",
                )),
            )
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let plan = decline_plan()?;

        let declined = client.decline_request(&token, &device_id, &plan).await?;

        assert_eq!(declined.request_id().as_str(), "request-1");
        assert_eq!(declined.status().as_str(), "cancelled");
        assert_request_count(&server, 1).await;
        assert_requests_have_no_query(&server).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn request_update_mismatches_and_unverified_errors_are_ambiguous() -> TestResult {
        for (status, body) in [
            (200, updated_payment_body("pay", "settled", "456", "123")),
            (
                200,
                serde_json::json!({"data": {"id": "request-1", "status": "settled"}}),
            ),
            (400, serde_json::json!({"error": {"code": 2901}})),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("PUT"))
                .and(path("/v1/payments/request-1"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client
                .accept_request(&token, &device_id, &accept_plan()?)
                .await;

            assert!(matches!(
                result,
                Err(VenmoApiError::FinancialOutcomeUnknown { .. })
            ));
            assert_eq!(
                result.as_ref().err().map(ApiFailure::kind),
                Some(ApiFailureKind::AmbiguousWrite)
            );
            assert_request_count(&server, 1).await;
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn decline_rejects_every_unproven_terminal_response_as_ambiguous() -> TestResult {
        let mut wrong_id = updated_payment_body("charge", "cancelled", "456", "123");
        wrong_id["data"]["id"] = Value::String("request-2".to_owned());
        let mut wrong_amount = updated_payment_body("charge", "cancelled", "456", "123");
        wrong_amount["data"]["amount"] = Value::String("0.02".to_owned());
        let mut wrong_note = updated_payment_body("charge", "cancelled", "456", "123");
        wrong_note["data"]["note"] = Value::String("Different note".to_owned());
        let mut wrong_audience = updated_payment_body("charge", "cancelled", "456", "123");
        wrong_audience["data"]["audience"] = Value::String("public".to_owned());
        let mut wrong_created_at = updated_payment_body("charge", "cancelled", "456", "123");
        wrong_created_at["data"]["date_created"] = Value::String("2026-07-11T12:00:01".to_owned());
        for (status, body) in [
            (200, updated_payment_body("charge", "pending", "456", "123")),
            (200, updated_payment_body("pay", "cancelled", "456", "123")),
            (
                200,
                updated_payment_body("charge", "cancelled", "123", "456"),
            ),
            (200, wrong_id),
            (200, wrong_amount),
            (200, wrong_note),
            (200, wrong_audience),
            (200, wrong_created_at),
            (400, serde_json::json!({"error": {"code": 2901}})),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("PUT"))
                .and(path("/v1/payments/request-1"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .mount(&server)
                .await;
            let client = test_client(&server)?;
            let (token, device_id) = test_session()?;

            let result = client
                .decline_request(&token, &device_id, &decline_plan()?)
                .await;

            assert!(matches!(
                result,
                Err(VenmoApiError::FinancialOutcomeUnknown { .. })
            ));
            assert_eq!(
                result.as_ref().err().map(ApiFailure::kind),
                Some(ApiFailureKind::AmbiguousWrite)
            );
            assert_request_count(&server, 1).await;
        }
        Ok(())
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
                    "audience": "private",
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
        Ok(PayPlan::new(
            crate::domain::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?,
            test_account()?,
            financial_user("456", "bob")?,
            Money::from_cents(1)?,
            crate::domain::Note::from_str("Synthetic note")?,
            Balance::new(
                SignedUsdAmount::from_cents(0),
                SignedUsdAmount::from_cents(0),
            ),
            zero_fee_peer_method()?,
            EligibilityToken::parse_owned("synthetic-eligibility-token".to_owned())?,
        ))
    }

    fn request_plan() -> Result<CreateRequestPlan, Box<dyn Error>> {
        Ok(CreateRequestPlan::new(
            crate::domain::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?,
            test_account()?,
            financial_user("456", "bob")?,
            Money::from_cents(1)?,
            crate::domain::Note::from_str("Synthetic note")?,
        ))
    }

    fn incoming_request() -> Result<PendingRequest, Box<dyn Error>> {
        let created_at = parse_timestamp_value("2026-07-11T12:00:00")
            .map_err(|()| io::Error::other("invalid synthetic request timestamp"))?;
        Ok(PendingRequest::new(
            RequestId::from_str("request-1")?,
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
            requests.as_ref().is_some_and(|requests| requests
                .iter()
                .all(|request| request.url.query().is_none())),
            "expected every captured request URL to omit a query"
        );
    }
}
