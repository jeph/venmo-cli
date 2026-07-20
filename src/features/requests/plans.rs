use std::fmt;

use crate::features::payments::{
    EligibilityToken, FinancialStatus, PaymentId, PeerFundingSource, PeerFundingSourceSelection,
};
use crate::features::people::User;
use crate::features::wallet::Balance;
use crate::shared::{Account, ClientRequestId, Money, Note, Visibility};

use super::{RequestId, RequestNotificationId, RequestRecord, RequestStatus};

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Eq, PartialEq)]
pub struct CreateRequestPlan {
    request_id: ClientRequestId,
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    visibility: Visibility,
}

impl CreateRequestPlan {
    #[must_use]
    pub(crate) const fn new(
        request_id: ClientRequestId,
        account: Account,
        recipient: User,
        amount: Money,
        note: Note,
        visibility: Visibility,
    ) -> Self {
        Self {
            request_id,
            account,
            recipient,
            amount,
            note,
            visibility,
        }
    }

    #[must_use]
    pub const fn request_id(&self) -> ClientRequestId {
        self.request_id
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn recipient(&self) -> &User {
        &self.recipient
    }

    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    #[must_use]
    pub const fn note(&self) -> &Note {
        &self.note
    }

    #[must_use]
    pub const fn visibility(&self) -> Visibility {
        self.visibility
    }
}

impl fmt::Debug for CreateRequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CreateRequestPlan([REDACTED])")
    }
}

#[derive(Eq, PartialEq)]
pub struct AcceptRequestPlan {
    account: Account,
    request: RequestRecord,
    balance: Balance,
    funding: Option<AcceptanceFunding>,
}

#[derive(Eq, PartialEq)]
struct AcceptanceFunding {
    notification_id: RequestNotificationId,
    source: PeerFundingSource,
    source_selection: PeerFundingSourceSelection,
    eligibility_token: EligibilityToken,
    fees: RequestApprovalFees,
    protected: bool,
}

#[derive(Eq, PartialEq)]
pub(crate) enum RequestApprovalFees {
    Omitted,
    Present {
        entries: Vec<RequestApprovalFee>,
        total_cents: u64,
    },
}

impl RequestApprovalFees {
    #[must_use]
    pub(crate) const fn omitted() -> Self {
        Self::Omitted
    }

    #[must_use]
    pub(crate) const fn present(entries: Vec<RequestApprovalFee>, total_cents: u64) -> Self {
        Self::Present {
            entries,
            total_cents,
        }
    }

    #[must_use]
    pub(crate) const fn entries(&self) -> Option<&[RequestApprovalFee]> {
        match self {
            Self::Omitted => None,
            Self::Present { entries, .. } => Some(entries.as_slice()),
        }
    }

    #[must_use]
    pub(crate) const fn total_cents(&self) -> u64 {
        match self {
            Self::Omitted => 0,
            Self::Present { total_cents, .. } => *total_cents,
        }
    }
}

impl fmt::Debug for RequestApprovalFees {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestApprovalFees([REDACTED])")
    }
}

#[derive(Eq, PartialEq)]
pub(crate) struct RequestApprovalFee {
    product_uri: String,
    applied_to: String,
    fee_token: String,
    base_fee_amount: Option<u64>,
    fee_percentage: Option<String>,
    calculated_fee_amount_in_cents: u64,
}

impl RequestApprovalFee {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) fn new(
        product_uri: String,
        applied_to: String,
        fee_token: String,
        base_fee_amount: Option<u64>,
        fee_percentage: Option<String>,
        calculated_fee_amount_in_cents: u64,
    ) -> Self {
        Self {
            product_uri,
            applied_to,
            fee_token,
            base_fee_amount,
            fee_percentage,
            calculated_fee_amount_in_cents,
        }
    }

    #[must_use]
    pub(crate) fn product_uri(&self) -> &str {
        &self.product_uri
    }

    #[must_use]
    pub(crate) fn applied_to(&self) -> &str {
        &self.applied_to
    }

    #[must_use]
    pub(crate) fn fee_token(&self) -> &str {
        &self.fee_token
    }

    #[must_use]
    pub(crate) const fn base_fee_amount(&self) -> Option<u64> {
        self.base_fee_amount
    }

    #[must_use]
    pub(crate) fn fee_percentage(&self) -> Option<&str> {
        self.fee_percentage.as_deref()
    }

    #[must_use]
    pub(crate) const fn calculated_fee_amount_in_cents(&self) -> u64 {
        self.calculated_fee_amount_in_cents
    }
}

impl fmt::Debug for RequestApprovalFee {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestApprovalFee([REDACTED])")
    }
}

impl AcceptRequestPlan {
    #[must_use]
    pub(crate) const fn new(account: Account, request: RequestRecord, balance: Balance) -> Self {
        Self {
            account,
            request,
            balance,
            funding: None,
        }
    }

    #[must_use]
    pub(crate) fn with_funding(
        self,
        notification_id: RequestNotificationId,
        source: PeerFundingSource,
        source_selection: PeerFundingSourceSelection,
        eligibility_token: EligibilityToken,
        fees: RequestApprovalFees,
        protected: bool,
    ) -> Self {
        Self {
            funding: Some(AcceptanceFunding {
                notification_id,
                source,
                source_selection,
                eligibility_token,
                fees,
                protected,
            }),
            ..self
        }
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn request(&self) -> &RequestRecord {
        &self.request
    }

    #[must_use]
    pub const fn balance(&self) -> &Balance {
        &self.balance
    }

    #[must_use]
    pub fn funding_source(&self) -> Option<&PeerFundingSource> {
        self.funding.as_ref().map(|funding| &funding.source)
    }

    #[must_use]
    pub fn funding_source_selection(&self) -> Option<PeerFundingSourceSelection> {
        self.funding
            .as_ref()
            .map(|funding| funding.source_selection)
    }

    #[must_use]
    pub(crate) fn approval_notification_id(&self) -> Option<&RequestNotificationId> {
        self.funding
            .as_ref()
            .map(|funding| &funding.notification_id)
    }

    #[must_use]
    pub(crate) fn eligibility_token(&self) -> Option<&EligibilityToken> {
        self.funding
            .as_ref()
            .map(|funding| &funding.eligibility_token)
    }

    #[must_use]
    pub(crate) fn approval_fees(&self) -> Option<&RequestApprovalFees> {
        self.funding.as_ref().map(|funding| &funding.fees)
    }

    #[must_use]
    pub const fn approval_fee_cents(&self) -> Option<u64> {
        match &self.funding {
            Some(funding) if funding.protected => Some(funding.fees.total_cents()),
            Some(_) | None => None,
        }
    }

    #[must_use]
    pub const fn is_purchase_protected(&self) -> bool {
        match &self.funding {
            Some(funding) => funding.protected,
            None => false,
        }
    }

    #[must_use]
    pub fn recipient_proceeds_cents(&self) -> Option<u64> {
        self.approval_fee_cents()
            .and_then(|fee| self.request.amount().cents().checked_sub(fee))
    }
}

impl fmt::Debug for AcceptRequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AcceptRequestPlan([REDACTED])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct AcceptedRequest {
    payment_id: Option<PaymentId>,
    status: Option<FinancialStatus>,
}

impl AcceptedRequest {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(payment_id: PaymentId, status: FinancialStatus) -> Self {
        Self {
            payment_id: Some(payment_id),
            status: Some(status),
        }
    }

    #[must_use]
    pub(crate) const fn source_funded() -> Self {
        Self {
            payment_id: None,
            status: None,
        }
    }

    #[must_use]
    pub const fn payment_id(&self) -> Option<&PaymentId> {
        self.payment_id.as_ref()
    }

    #[must_use]
    pub const fn status(&self) -> Option<FinancialStatus> {
        self.status
    }
}

impl fmt::Debug for AcceptedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcceptedRequest")
            .field("payment_id", &REDACTED)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub struct DeclineRequestPlan {
    account: Account,
    request: RequestRecord,
}

impl DeclineRequestPlan {
    #[must_use]
    pub(crate) const fn new(account: Account, request: RequestRecord) -> Self {
        Self { account, request }
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn request(&self) -> &RequestRecord {
        &self.request
    }
}

impl fmt::Debug for DeclineRequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeclineRequestPlan([REDACTED])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct DeclinedRequest {
    request_id: RequestId,
    status: RequestStatus,
}

impl DeclinedRequest {
    #[must_use]
    pub(crate) const fn new(request_id: RequestId, status: RequestStatus) -> Self {
        Self { request_id, status }
    }

    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    #[must_use]
    pub const fn status(&self) -> &RequestStatus {
        &self.status
    }
}

impl fmt::Debug for DeclinedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeclinedRequest")
            .field("request_id", &REDACTED)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub struct CancelRequestPlan {
    account: Account,
    request: RequestRecord,
}

impl CancelRequestPlan {
    #[must_use]
    pub(crate) const fn new(account: Account, request: RequestRecord) -> Self {
        Self { account, request }
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn request(&self) -> &RequestRecord {
        &self.request
    }
}

impl fmt::Debug for CancelRequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CancelRequestPlan([REDACTED])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct CancelledRequest {
    request_id: RequestId,
    status: RequestStatus,
}

impl CancelledRequest {
    #[must_use]
    pub(crate) const fn new(request_id: RequestId, status: RequestStatus) -> Self {
        Self { request_id, status }
    }

    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    #[must_use]
    pub const fn status(&self) -> &RequestStatus {
        &self.status
    }
}

impl fmt::Debug for CancelledRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancelledRequest")
            .field("request_id", &REDACTED)
            .field("status", &self.status)
            .finish()
    }
}
