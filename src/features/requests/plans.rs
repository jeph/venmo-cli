use std::fmt;

use crate::features::payments::{FinancialStatus, PaymentId};
use crate::features::people::User;
use crate::features::wallet::Balance;
use crate::shared::{Account, ClientRequestId, Money, Note};

use super::{RequestId, RequestRecord, RequestStatus};

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Eq, PartialEq)]
pub struct CreateRequestPlan {
    request_id: ClientRequestId,
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
}

impl CreateRequestPlan {
    #[must_use]
    pub(crate) const fn new(
        request_id: ClientRequestId,
        account: Account,
        recipient: User,
        amount: Money,
        note: Note,
    ) -> Self {
        Self {
            request_id,
            account,
            recipient,
            amount,
            note,
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
}

impl AcceptRequestPlan {
    #[must_use]
    pub(crate) const fn new(account: Account, request: RequestRecord, balance: Balance) -> Self {
        Self {
            account,
            request,
            balance,
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
}

impl fmt::Debug for AcceptRequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AcceptRequestPlan([REDACTED])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct AcceptedRequest {
    payment_id: PaymentId,
    status: FinancialStatus,
}

impl AcceptedRequest {
    #[must_use]
    pub(crate) const fn new(payment_id: PaymentId, status: FinancialStatus) -> Self {
        Self { payment_id, status }
    }

    #[must_use]
    pub const fn payment_id(&self) -> &PaymentId {
        &self.payment_id
    }

    #[must_use]
    pub const fn status(&self) -> FinancialStatus {
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
