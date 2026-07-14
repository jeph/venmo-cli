use std::fmt;
use std::num::NonZeroU64;

use thiserror::Error;
use zeroize::Zeroizing;

use crate::features::people::User;
use crate::features::wallet::{Balance, PaymentMethod};
use crate::shared::opaque_id::opaque_id;
use crate::shared::{Account, ClientRequestId, Money, Note};

const MAX_ELIGIBILITY_TOKEN_BYTES: usize = 4096;
const REDACTED: &str = "[REDACTED]";

opaque_id!(PaymentId, "payment ID");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerFundingRole {
    Default,
    Backup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerFundingFee {
    ProvenZero,
    NonZero { cents: NonZeroU64 },
    Unknown,
}

impl PeerFundingFee {
    #[must_use]
    pub const fn from_cents(cents: u64) -> Self {
        match NonZeroU64::new(cents) {
            Some(cents) => Self::NonZero { cents },
            None => Self::ProvenZero,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PeerFundingMethod {
    method: PaymentMethod,
    role: PeerFundingRole,
    fee: PeerFundingFee,
}

impl PeerFundingMethod {
    #[must_use]
    pub(crate) const fn new(
        method: PaymentMethod,
        role: PeerFundingRole,
        fee: PeerFundingFee,
    ) -> Self {
        Self { method, role, fee }
    }

    #[must_use]
    pub const fn method(&self) -> &PaymentMethod {
        &self.method
    }

    #[must_use]
    pub const fn role(&self) -> PeerFundingRole {
        self.role
    }

    #[must_use]
    pub const fn fee(&self) -> PeerFundingFee {
        self.fee
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        matches!(self.role, PeerFundingRole::Default)
    }
}

impl fmt::Debug for PeerFundingMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PeerFundingMethod")
            .field("method", &REDACTED)
            .field("role", &self.role)
            .field("fee", &self.fee)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub struct EligibilityToken(Zeroizing<String>);

impl EligibilityToken {
    pub(crate) fn parse_owned(value: String) -> Result<Self, EligibilityTokenParseError> {
        if value.is_empty() {
            return Err(EligibilityTokenParseError::Empty);
        }
        if value.len() > MAX_ELIGIBILITY_TOKEN_BYTES {
            return Err(EligibilityTokenParseError::TooLong {
                maximum_bytes: MAX_ELIGIBILITY_TOKEN_BYTES,
            });
        }
        if value.chars().any(char::is_whitespace) {
            return Err(EligibilityTokenParseError::Whitespace);
        }
        if value.chars().any(char::is_control) {
            return Err(EligibilityTokenParseError::ControlCharacter);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    #[must_use]
    pub(crate) fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for EligibilityToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EligibilityToken([REDACTED])")
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EligibilityTokenParseError {
    #[error("eligibility token must not be empty")]
    Empty,
    #[error("eligibility token must not exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },
    #[error("eligibility token must not contain whitespace")]
    Whitespace,
    #[error("eligibility token must not contain control characters")]
    ControlCharacter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinancialStatus {
    Settled,
    Pending,
    Held,
}

#[derive(Clone, Eq, PartialEq)]
pub struct CreatedPayment {
    id: PaymentId,
    status: FinancialStatus,
}

impl CreatedPayment {
    #[must_use]
    pub(crate) const fn new(id: PaymentId, status: FinancialStatus) -> Self {
        Self { id, status }
    }

    #[must_use]
    pub const fn id(&self) -> &PaymentId {
        &self.id
    }

    #[must_use]
    pub const fn status(&self) -> FinancialStatus {
        self.status
    }
}

impl fmt::Debug for CreatedPayment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreatedPayment")
            .field("id", &REDACTED)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub struct PayPlan {
    request_id: ClientRequestId,
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    balance: Balance,
    backup_method: PeerFundingMethod,
    eligibility_token: EligibilityToken,
}

impl PayPlan {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) const fn new(
        request_id: ClientRequestId,
        account: Account,
        recipient: User,
        amount: Money,
        note: Note,
        balance: Balance,
        backup_method: PeerFundingMethod,
        eligibility_token: EligibilityToken,
    ) -> Self {
        Self {
            request_id,
            account,
            recipient,
            amount,
            note,
            balance,
            backup_method,
            eligibility_token,
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
    pub const fn balance(&self) -> &Balance {
        &self.balance
    }

    #[must_use]
    pub const fn backup_method(&self) -> &PeerFundingMethod {
        &self.backup_method
    }

    #[must_use]
    pub(crate) const fn eligibility_token(&self) -> &EligibilityToken {
        &self.eligibility_token
    }
}

impl fmt::Debug for PayPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PayPlan([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn eligibility_tokens_are_bounded_and_redacted() -> Result<(), Box<dyn Error>> {
        let token = EligibilityToken::parse_owned("synthetic-token".to_owned())?;
        assert_eq!(token.expose(), "synthetic-token");
        assert_eq!(format!("{token:?}"), "EligibilityToken([REDACTED])");
        assert!(EligibilityToken::parse_owned("has space".to_owned()).is_err());
        assert!(
            EligibilityToken::parse_owned("x".repeat(MAX_ELIGIBILITY_TOKEN_BYTES + 1)).is_err()
        );
        for value in [
            "x".repeat(MAX_ELIGIBILITY_TOKEN_BYTES),
            "é".repeat(MAX_ELIGIBILITY_TOKEN_BYTES / 2),
        ] {
            assert_eq!(value.len(), MAX_ELIGIBILITY_TOKEN_BYTES);
            assert_eq!(
                EligibilityToken::parse_owned(value)?.expose().len(),
                MAX_ELIGIBILITY_TOKEN_BYTES
            );
        }
        assert!(matches!(
            EligibilityToken::parse_owned("é".repeat(MAX_ELIGIBILITY_TOKEN_BYTES / 2 + 1)),
            Err(EligibilityTokenParseError::TooLong {
                maximum_bytes: MAX_ELIGIBILITY_TOKEN_BYTES
            })
        ));
        assert_eq!(
            EligibilityToken::parse_owned("line break".to_owned()),
            Err(EligibilityTokenParseError::Whitespace)
        );
        assert_eq!(
            EligibilityToken::parse_owned("zero\u{0}byte".to_owned()),
            Err(EligibilityTokenParseError::ControlCharacter)
        );
        Ok(())
    }

    #[test]
    fn peer_funding_fee_encodes_zero_and_nonzero_as_whole_values() {
        assert_eq!(PeerFundingFee::from_cents(0), PeerFundingFee::ProvenZero);
        assert!(matches!(
            PeerFundingFee::from_cents(3),
            PeerFundingFee::NonZero {
                cents
            } if cents.get() == 3
        ));
    }
}
