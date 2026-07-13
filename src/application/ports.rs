use std::error::Error;
use std::future::Future;
use std::io;
use std::num::NonZeroU32;

use thiserror::Error;
use time::OffsetDateTime;

use crate::domain::{
    AcceptRequestPlan, AcceptedRequest, AccessToken, AccessTokenParseError, Account,
    AccountPassword, AccountPasswordParseError, Activity, ActivityId, Balance, ClientRequestId,
    CreateRequestPlan, CreatedPayment, CredentialEnvelope, DeclineRequestPlan, DeclinedRequest,
    DeviceId, DeviceIdParseError, EligibilityToken, LoginIdentifier, LoginIdentifierParseError,
    OtpCode, OtpCodeParseError, OtpSecret, PasswordLoginStart, PayPlan, PaymentMethod,
    PeerFundingMethod, PendingRequest, RequestId, User, UserId, UserSearchQuery,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiFailureKind {
    Network,
    Timeout,
    Rejected,
    Contract,
    AmbiguousWrite,
    Internal,
}

pub trait ApiFailure: Error + Send + Sync + 'static {
    fn kind(&self) -> ApiFailureKind;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialFormat {
    Version1,
    LegacyTypeScript,
}

#[derive(Debug)]
pub struct LoadedCredential {
    pub envelope: CredentialEnvelope,
    pub format: CredentialFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialDeleteOutcome {
    Deleted,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialFailureKind {
    Unavailable,
    Corrupt,
    Invalid,
    UnsupportedVersion,
    TooLarge,
    Ambiguous,
    Platform,
    Internal,
}

impl CredentialFailureKind {
    #[must_use]
    pub const fn permits_validated_replacement(self) -> bool {
        matches!(
            self,
            Self::Corrupt | Self::Invalid | Self::UnsupportedVersion | Self::TooLarge
        )
    }
}

pub trait CredentialStoreFailure: Error + Send + Sync + 'static {
    fn kind(&self) -> CredentialFailureKind;
}

pub trait CredentialStore {
    type Error: CredentialStoreFailure;

    fn load(&self) -> Result<Option<LoadedCredential>, Self::Error>;
    fn save(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error>;
    fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error>;
}

/// A deliberately read-only credential capability used by diagnostics.
pub trait CredentialReader {
    type Error: CredentialStoreFailure;

    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error>;
}

impl<T> CredentialReader for T
where
    T: CredentialStore,
{
    type Error = T::Error;

    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.load()
    }
}

pub trait PromptPort {
    fn can_prompt(&self) -> bool;
    fn read_login_identifier(&self, prompt: &str) -> Result<LoginIdentifier, PromptError>;
    fn read_account_password(&self, prompt: &str) -> Result<AccountPassword, PromptError>;
    fn read_otp_code(&self, prompt: &str) -> Result<OtpCode, PromptError>;
    fn read_access_token(&self, prompt: &str) -> Result<AccessToken, PromptError>;
    fn read_device_id(&self, prompt: &str) -> Result<DeviceId, PromptError>;
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError>;
    fn select(&self, prompt: &str, items: &[String]) -> Result<usize, PromptError>;
}

pub trait AuthenticationApi {
    type Error: Error + Send + Sync + 'static;

    fn current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a;

    fn revoke_access_token<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

pub trait PasswordLoginApi {
    type Error: Error + Send + Sync + 'static;

    fn begin_password_login<'a>(
        &'a self,
        identifier: &'a LoginIdentifier,
        password: &'a AccountPassword,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PasswordLoginStart, Self::Error>> + Send + 'a;

    fn request_sms_otp<'a>(
        &'a self,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn complete_otp_login<'a>(
        &'a self,
        otp_code: &'a OtpCode,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<AccessToken, Self::Error>> + Send + 'a;

    fn trust_device<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

pub trait PaymentMethodsApi {
    type Error: ApiFailure;

    fn payment_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a;
}

#[allow(dead_code)]
pub(crate) trait ClientRequestIdGenerator {
    fn generate(&self) -> ClientRequestId;
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct PaymentEligibility {
    token: EligibilityToken,
    fee_cents: u64,
}

#[allow(dead_code)]
impl PaymentEligibility {
    #[must_use]
    pub const fn new(token: EligibilityToken, fee_cents: u64) -> Self {
        Self { token, fee_cents }
    }

    #[must_use]
    pub const fn token(&self) -> &EligibilityToken {
        &self.token
    }

    #[must_use]
    pub const fn fee_cents(&self) -> u64 {
        self.fee_cents
    }

    #[must_use]
    pub fn into_token(self) -> EligibilityToken {
        self.token
    }
}

#[allow(dead_code)]
pub(crate) trait PeerFundingApi {
    type Error: ApiFailure;

    fn peer_funding_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PeerFundingMethod>, Self::Error>> + Send + 'a;
}

#[allow(dead_code)]
pub(crate) trait PaymentEligibilityApi {
    type Error: ApiFailure;

    fn payment_eligibility<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        recipient: &'a User,
        amount: crate::domain::Money,
        note: &'a crate::domain::Note,
    ) -> impl Future<Output = Result<PaymentEligibility, Self::Error>> + Send + 'a;
}

#[allow(dead_code)]
pub(crate) trait PaymentCreationApi {
    type Error: ApiFailure;

    fn create_payment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a PayPlan,
    ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a;
}

#[allow(dead_code)]
pub(crate) trait RequestCreationApi {
    type Error: ApiFailure;

    fn create_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a CreateRequestPlan,
    ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestAcceptanceApi {
    type Error: ApiFailure;

    fn accept_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a AcceptRequestPlan,
    ) -> impl Future<Output = Result<AcceptedRequest, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestDeclineApi {
    type Error: ApiFailure;

    fn decline_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a DeclineRequestPlan,
    ) -> impl Future<Output = Result<DeclinedRequest, Self::Error>> + Send + 'a;
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct UserSearchPageToken(u32);

impl UserSearchPageToken {
    #[must_use]
    pub(crate) const fn from_offset(offset: u32) -> Self {
        Self(offset)
    }

    #[must_use]
    pub(crate) const fn offset(self) -> u32 {
        self.0
    }
}

impl std::fmt::Debug for UserSearchPageToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("UserSearchPageToken([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSearchPageRequest {
    page_size: NonZeroU32,
    token: Option<UserSearchPageToken>,
}

impl UserSearchPageRequest {
    #[must_use]
    pub const fn new(page_size: NonZeroU32, token: Option<UserSearchPageToken>) -> Self {
        Self { page_size, token }
    }

    #[must_use]
    pub const fn page_size(self) -> NonZeroU32 {
        self.page_size
    }

    #[must_use]
    pub const fn token(self) -> Option<UserSearchPageToken> {
        self.token
    }
}

#[derive(Debug)]
pub struct UserSearchPage {
    users: Vec<User>,
    next_token: Option<UserSearchPageToken>,
}

impl UserSearchPage {
    #[must_use]
    pub fn new(users: Vec<User>, next_token: Option<UserSearchPageToken>) -> Self {
        Self { users, next_token }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<User>, Option<UserSearchPageToken>) {
        (self.users, self.next_token)
    }
}

pub trait UsersApi {
    type Error: ApiFailure;

    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a;
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct FriendsPageToken(u32);

impl FriendsPageToken {
    #[must_use]
    pub(crate) const fn from_offset(offset: u32) -> Self {
        Self(offset)
    }

    #[must_use]
    pub(crate) const fn offset(self) -> u32 {
        self.0
    }
}

impl std::fmt::Debug for FriendsPageToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FriendsPageToken([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FriendsPageRequest {
    page_size: NonZeroU32,
    token: Option<FriendsPageToken>,
}

impl FriendsPageRequest {
    #[must_use]
    pub const fn new(page_size: NonZeroU32, token: Option<FriendsPageToken>) -> Self {
        Self { page_size, token }
    }

    #[must_use]
    pub const fn page_size(self) -> NonZeroU32 {
        self.page_size
    }

    #[must_use]
    pub const fn token(self) -> Option<FriendsPageToken> {
        self.token
    }
}

#[derive(Debug)]
pub struct FriendsPage {
    users: Vec<User>,
    next_token: Option<FriendsPageToken>,
}

impl FriendsPage {
    #[must_use]
    pub fn new(users: Vec<User>, next_token: Option<FriendsPageToken>) -> Self {
        Self { users, next_token }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<User>, Option<FriendsPageToken>) {
        (self.users, self.next_token)
    }
}

pub trait FriendsApi {
    type Error: ApiFailure;

    fn friends<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: FriendsPageRequest,
    ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a;
}

pub trait BalanceApi {
    type Error: ApiFailure;

    fn balance<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a;
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ActivityPageToken(String);

impl ActivityPageToken {
    #[must_use]
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ActivityPageToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ActivityPageToken([REDACTED])")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityPageRequest {
    page_size: NonZeroU32,
    token: Option<ActivityPageToken>,
}

impl ActivityPageRequest {
    #[must_use]
    pub const fn new(page_size: NonZeroU32, token: Option<ActivityPageToken>) -> Self {
        Self { page_size, token }
    }

    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
    }

    #[must_use]
    pub fn token(&self) -> Option<&ActivityPageToken> {
        self.token.as_ref()
    }
}

#[derive(Debug)]
pub struct ActivityPage {
    activities: Vec<Activity>,
    next_token: Option<ActivityPageToken>,
}

impl ActivityPage {
    #[must_use]
    pub fn new(activities: Vec<Activity>, next_token: Option<ActivityPageToken>) -> Self {
        Self {
            activities,
            next_token,
        }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<Activity>, Option<ActivityPageToken>) {
        (self.activities, self.next_token)
    }
}

pub trait ActivityApi {
    type Error: ApiFailure;

    fn activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a;

    fn activity_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<Activity, Self::Error>> + Send + 'a;
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PendingRequestsPageToken(String);

impl PendingRequestsPageToken {
    #[must_use]
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for PendingRequestsPageToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PendingRequestsPageToken([REDACTED])")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRequestsPageRequest {
    page_size: NonZeroU32,
    token: Option<PendingRequestsPageToken>,
}

impl PendingRequestsPageRequest {
    #[must_use]
    pub const fn new(page_size: NonZeroU32, token: Option<PendingRequestsPageToken>) -> Self {
        Self { page_size, token }
    }

    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
    }

    #[must_use]
    pub fn token(&self) -> Option<&PendingRequestsPageToken> {
        self.token.as_ref()
    }
}

#[derive(Debug)]
pub struct PendingRequestsPage {
    requests: Vec<PendingRequest>,
    next_token: Option<PendingRequestsPageToken>,
}

impl PendingRequestsPage {
    #[must_use]
    pub fn new(
        requests: Vec<PendingRequest>,
        next_token: Option<PendingRequestsPageToken>,
    ) -> Self {
        Self {
            requests,
            next_token,
        }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<PendingRequest>, Option<PendingRequestsPageToken>) {
        (self.requests, self.next_token)
    }
}

pub trait RequestsApi {
    type Error: ApiFailure;

    fn pending_requests<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: PendingRequestsPageRequest,
    ) -> impl Future<Output = Result<PendingRequestsPage, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestLookupApi {
    type Error: ApiFailure;

    fn pending_request_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<PendingRequest, Self::Error>> + Send + 'a;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequiredShape {
    Balance,
    PaymentMethods,
    Friends,
    Activity,
    PendingRequests,
}

impl RequiredShape {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Balance => "balance",
            Self::PaymentMethods => "payment methods",
            Self::Friends => "friends",
            Self::Activity => "activity",
            Self::PendingRequests => "pending requests",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapeProbeOutcome {
    shape: RequiredShape,
    failure: Option<ApiFailureKind>,
}

impl ShapeProbeOutcome {
    #[must_use]
    pub const fn passed(shape: RequiredShape) -> Self {
        Self {
            shape,
            failure: None,
        }
    }

    #[must_use]
    pub const fn failed(shape: RequiredShape, failure: ApiFailureKind) -> Self {
        Self {
            shape,
            failure: Some(failure),
        }
    }

    #[must_use]
    pub const fn shape(self) -> RequiredShape {
        self.shape
    }

    #[must_use]
    pub const fn failure(self) -> Option<ApiFailureKind> {
        self.failure
    }
}

pub trait DoctorApi {
    type Error: ApiFailure;

    fn connectivity(&self) -> impl Future<Output = Result<(), Self::Error>> + Send + '_;

    fn diagnostic_current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a;

    fn required_shapes<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
    ) -> impl Future<Output = Vec<ShapeProbeOutcome>> + Send + 'a;
}

pub trait Clock {
    fn now_utc(&self) -> OffsetDateTime;
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("cancelled")]
    Cancelled,

    #[error("an interactive terminal is required")]
    NotInteractive,

    #[error("the bearer token is invalid: {source}")]
    InvalidAccessToken {
        #[source]
        source: AccessTokenParseError,
    },

    #[error("the Venmo device ID is invalid: {source}")]
    InvalidDeviceId {
        #[source]
        source: DeviceIdParseError,
    },

    #[error("the Venmo account identifier is invalid: {source}")]
    InvalidLoginIdentifier {
        #[source]
        source: LoginIdentifierParseError,
    },

    #[error("the Venmo password is invalid: {source}")]
    InvalidAccountPassword {
        #[source]
        source: AccountPasswordParseError,
    },

    #[error("the Venmo SMS code is invalid: {source}")]
    InvalidOtpCode {
        #[source]
        source: OtpCodeParseError,
    },

    #[error("cannot prompt with an empty choice list")]
    NoChoices,

    #[error("the prompt returned choice {index}, but only {choice_count} choices exist")]
    InvalidSelection { index: usize, choice_count: usize },

    #[error("terminal interaction failed")]
    Interaction {
        #[source]
        source: io::Error,
    },
}
