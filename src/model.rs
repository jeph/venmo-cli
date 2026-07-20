//! Stable, frontend-neutral value and result models.
//!
//! Application orchestration, service ports, adapters, credentials, and other
//! implementation details intentionally remain crate-private. Frontends should
//! expose their own protocol-specific facades rather than widening this one.

pub use crate::features::activity::continuation::ActivityBeforeId;
pub use crate::features::activity::info::ActivityInfoResult;
pub use crate::features::activity::list::ActivityListResult;
pub use crate::features::activity::model::{
    Activity, ActivityAction, ActivityCounterparty, ActivityDetail, ActivityDetailParties,
    ActivityDirection, ActivityFeedKind, ActivityId, ActivityLabelParseError, ActivityStatus,
    ActivitySubject,
};
pub use crate::features::payments::model::{
    CreatedPayment, FinancialStatus, PayPlan, PaymentId, PeerFundingFee, PeerFundingMethod,
    PeerFundingRole, PeerFundingSource, PeerFundingSourceSelection, PeerFundingSources,
};
pub use crate::features::payments::pay::PayResult;
pub use crate::features::people::friends::FriendsResult;
pub use crate::features::people::info::UserInfoResult;
pub use crate::features::people::recipient::{RecipientInput, RecipientParseError};
pub use crate::features::people::user::{
    FriendshipStatus, User, UserProfileKind, UserSearchQuery, UserSearchQueryParseError,
};
pub use crate::features::people::users::UserSearchResult;
pub use crate::features::requests::accept::AcceptResult;
pub use crate::features::requests::cancel::CancelResult;
pub use crate::features::requests::continuation::RequestsBefore;
pub use crate::features::requests::create::RequestCreateResult;
pub use crate::features::requests::decline::DeclineResult;
pub use crate::features::requests::info::RequestInfoResult;
pub use crate::features::requests::list::RequestsResult;
pub use crate::features::requests::model::{
    CreatedRequest, RequestAction, RequestDirection, RequestDirectionFilter, RequestId,
    RequestRecord, RequestStatus, RequestStatusParseError,
};
pub use crate::features::requests::plans::{
    AcceptRequestPlan, AcceptedRequest, CancelRequestPlan, CancelledRequest, CreateRequestPlan,
    DeclineRequestPlan, DeclinedRequest,
};
pub use crate::features::transfers::model::{
    CreatedTransfer, TransferFeeMetadata, TransferId, TransferInstrument, TransferInstrumentId,
    TransferInstrumentSuffix, TransferInstrumentSuffixParseError, TransferModeOptions,
    TransferOptions, TransferOutAmount, TransferOutPlan, TransferSpeed,
};
pub use crate::features::transfers::options::TransferOptionsResult;
pub use crate::features::transfers::out::TransferOutResult;
pub use crate::features::wallet::balance::BalanceResult;
pub use crate::features::wallet::balance_model::{
    Balance, SignedUsdAmount, SignedUsdAmountParseError,
};
pub use crate::features::wallet::payment_method::{PaymentMethod, PaymentMethodId};
pub use crate::features::wallet::payment_methods::PaymentMethodsResult;
pub use crate::shared::{
    Account, ClientRequestId, ContinuationTokenParseError, DEFAULT_LIST_LIMIT, Limit,
    LimitParseError, MAX_CONTINUATION_TOKEN_BYTES, MAX_LIST_LIMIT, Money, MoneyParseError, Note,
    NoteParseError, Offset, OffsetParseError, OpaqueIdParseError, UserId, UserIdParseError,
    Username, UsernameParseError, Visibility, VisibilityParseError,
};
