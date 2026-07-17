use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;

use crate::features::wallet::Balance;
use crate::shared::opaque_id::opaque_id;
use crate::shared::{Account, Money};

opaque_id!(TransferId, "transfer ID");
opaque_id!(TransferInstrumentId, "transfer instrument ID");

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Eq, PartialEq)]
pub struct TransferInstrumentSuffix(String);

impl TransferInstrumentSuffix {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TransferInstrumentSuffix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TransferInstrumentSuffix([REDACTED])")
    }
}

impl fmt::Display for TransferInstrumentSuffix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for TransferInstrumentSuffix {
    type Err = TransferInstrumentSuffixParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.chars().count() != 4
            || value.chars().any(|character| {
                character.is_whitespace()
                    || character.is_control()
                    || is_invisible_format_control(character)
            })
        {
            return Err(TransferInstrumentSuffixParseError);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("transfer instrument suffix must contain exactly four visible non-whitespace characters")]
pub struct TransferInstrumentSuffixParseError;

fn is_invisible_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferDirection {
    In,
    Out,
}

impl TransferDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
        }
    }
}

impl fmt::Display for TransferDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferSpeed {
    Standard,
    Instant,
}

impl TransferSpeed {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Instant => "instant",
        }
    }
}

impl fmt::Display for TransferSpeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct TransferInstrument {
    id: TransferInstrumentId,
    name: String,
    asset_name: String,
    instrument_type: String,
    last_four: TransferInstrumentSuffix,
    is_default: bool,
    transfer_to_estimate: String,
}

impl TransferInstrument {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: TransferInstrumentId,
        name: String,
        asset_name: String,
        instrument_type: String,
        last_four: TransferInstrumentSuffix,
        is_default: bool,
        transfer_to_estimate: String,
    ) -> Self {
        Self {
            id,
            name,
            asset_name,
            instrument_type,
            last_four,
            is_default,
            transfer_to_estimate,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &TransferInstrumentId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn asset_name(&self) -> &str {
        &self.asset_name
    }

    #[must_use]
    pub fn instrument_type(&self) -> &str {
        &self.instrument_type
    }

    #[must_use]
    pub fn last_four(&self) -> &str {
        self.last_four.as_str()
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }

    #[must_use]
    pub fn transfer_to_estimate(&self) -> &str {
        &self.transfer_to_estimate
    }
}

impl fmt::Debug for TransferInstrument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransferInstrument")
            .field("id", &REDACTED)
            .field("name", &REDACTED)
            .field("asset_name", &REDACTED)
            .field("instrument_type", &self.instrument_type)
            .field("last_four", &REDACTED)
            .field("is_default", &self.is_default)
            .field("transfer_to_estimate", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct TransferFeeMetadata {
    minimum_amount: Option<String>,
    maximum_amount: Option<String>,
    variable_percentage: Option<String>,
    has_additional_non_null_fields: bool,
}

impl TransferFeeMetadata {
    #[must_use]
    pub fn new(
        minimum_amount: Option<String>,
        maximum_amount: Option<String>,
        variable_percentage: Option<String>,
        has_additional_non_null_fields: bool,
    ) -> Self {
        Self {
            minimum_amount,
            maximum_amount,
            variable_percentage,
            has_additional_non_null_fields,
        }
    }

    #[must_use]
    pub fn minimum_amount(&self) -> Option<&str> {
        self.minimum_amount.as_deref()
    }

    #[must_use]
    pub fn maximum_amount(&self) -> Option<&str> {
        self.maximum_amount.as_deref()
    }

    #[must_use]
    pub fn variable_percentage(&self) -> Option<&str> {
        self.variable_percentage.as_deref()
    }

    #[must_use]
    pub const fn has_additional_non_null_fields(&self) -> bool {
        self.has_additional_non_null_fields
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.minimum_amount.is_none()
            && self.maximum_amount.is_none()
            && self.variable_percentage.is_none()
            && !self.has_additional_non_null_fields
    }
}

impl fmt::Debug for TransferFeeMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransferFeeMetadata")
            .field("minimum_amount", &self.minimum_amount.is_some())
            .field("maximum_amount", &self.maximum_amount.is_some())
            .field("variable_percentage", &self.variable_percentage.is_some())
            .field(
                "has_additional_non_null_fields",
                &self.has_additional_non_null_fields,
            )
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct TransferModeOptions {
    eligible_sources: Vec<TransferInstrument>,
    eligible_destinations: Vec<TransferInstrument>,
    fee: TransferFeeMetadata,
    transfer_to_estimate: String,
}

impl TransferModeOptions {
    #[must_use]
    pub fn new(
        eligible_sources: Vec<TransferInstrument>,
        eligible_destinations: Vec<TransferInstrument>,
        fee: TransferFeeMetadata,
        transfer_to_estimate: String,
    ) -> Self {
        Self {
            eligible_sources,
            eligible_destinations,
            fee,
            transfer_to_estimate,
        }
    }

    #[must_use]
    pub fn eligible_sources(&self) -> &[TransferInstrument] {
        &self.eligible_sources
    }

    #[must_use]
    pub fn eligible_destinations(&self) -> &[TransferInstrument] {
        &self.eligible_destinations
    }

    #[must_use]
    pub const fn fee(&self) -> &TransferFeeMetadata {
        &self.fee
    }

    #[must_use]
    pub fn transfer_to_estimate(&self) -> &str {
        &self.transfer_to_estimate
    }
}

impl fmt::Debug for TransferModeOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransferModeOptions")
            .field("eligible_source_count", &self.eligible_sources.len())
            .field(
                "eligible_destination_count",
                &self.eligible_destinations.len(),
            )
            .field("fee", &self.fee)
            .field("transfer_to_estimate", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct TransferOptions {
    preferred_in: Option<TransferSpeed>,
    preferred_out: Option<TransferSpeed>,
    standard: TransferModeOptions,
    instant: TransferModeOptions,
}

impl fmt::Debug for TransferOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransferOptions")
            .field("preferred_in", &self.preferred_in)
            .field("preferred_out", &self.preferred_out)
            .field("standard", &self.standard)
            .field("instant", &self.instant)
            .finish()
    }
}

impl TransferOptions {
    #[must_use]
    pub const fn new(
        preferred_in: Option<TransferSpeed>,
        preferred_out: Option<TransferSpeed>,
        standard: TransferModeOptions,
        instant: TransferModeOptions,
    ) -> Self {
        Self {
            preferred_in,
            preferred_out,
            standard,
            instant,
        }
    }

    #[must_use]
    pub const fn preferred_in(&self) -> Option<TransferSpeed> {
        self.preferred_in
    }

    #[must_use]
    pub const fn preferred_out(&self) -> Option<TransferSpeed> {
        self.preferred_out
    }

    #[must_use]
    pub const fn standard(&self) -> &TransferModeOptions {
        &self.standard
    }

    #[must_use]
    pub const fn instant(&self) -> &TransferModeOptions {
        &self.instant
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct TransferOutPlan {
    account: Account,
    balance: Balance,
    amount: Money,
    speed: TransferSpeed,
    destination: TransferInstrument,
}

impl TransferOutPlan {
    #[must_use]
    pub const fn new(
        account: Account,
        balance: Balance,
        amount: Money,
        speed: TransferSpeed,
        destination: TransferInstrument,
    ) -> Self {
        Self {
            account,
            balance,
            amount,
            speed,
            destination,
        }
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn balance(&self) -> &Balance {
        &self.balance
    }

    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    #[must_use]
    pub const fn speed(&self) -> TransferSpeed {
        self.speed
    }

    #[must_use]
    pub const fn destination(&self) -> &TransferInstrument {
        &self.destination
    }
}

impl fmt::Debug for TransferOutPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TransferOutPlan([REDACTED])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct CreatedTransfer {
    id: TransferId,
    status: String,
    requested_at: OffsetDateTime,
    net_amount: Money,
    fee_cents: u64,
}

impl CreatedTransfer {
    #[must_use]
    pub fn new(
        id: TransferId,
        status: String,
        requested_at: OffsetDateTime,
        net_amount: Money,
        fee_cents: u64,
    ) -> Self {
        Self {
            id,
            status,
            requested_at,
            net_amount,
            fee_cents,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &TransferId {
        &self.id
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn requested_at(&self) -> OffsetDateTime {
        self.requested_at
    }

    #[must_use]
    pub const fn net_amount(&self) -> Money {
        self.net_amount
    }

    #[must_use]
    pub const fn fee_cents(&self) -> u64 {
        self.fee_cents
    }
}

impl fmt::Debug for CreatedTransfer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreatedTransfer")
            .field("id", &REDACTED)
            .field("status", &self.status)
            .field("requested_at", &REDACTED)
            .field("net_amount", &self.net_amount)
            .field("fee_cents", &self.fee_cents)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use super::*;

    #[test]
    fn transfer_instrument_suffix_is_exactly_four_visible_characters_and_redacted() {
        for valid in ["1234", "ABCD", "••••", "🂡🂢🂣🂤"] {
            let parsed = TransferInstrumentSuffix::from_str(valid);
            assert!(parsed.as_ref().is_ok_and(|suffix| suffix.as_str() == valid));
            assert!(!format!("{parsed:?}").contains(valid));
        }
        for invalid in [
            "",
            "123",
            "12345",
            "1234567890123456",
            "12 4",
            "12\n4",
            "12\u{200b}4",
            "12\u{202e}4",
        ] {
            assert_eq!(
                TransferInstrumentSuffix::from_str(invalid),
                Err(TransferInstrumentSuffixParseError)
            );
        }
    }

    #[test]
    fn transfer_models_redact_instrument_plan_and_created_record_details()
    -> Result<(), Box<dyn Error>> {
        let instrument = TransferInstrument::new(
            TransferInstrumentId::from_str("sensitive-bank-id")?,
            "Sensitive bank name".to_owned(),
            "Sensitive asset".to_owned(),
            "bank".to_owned(),
            TransferInstrumentSuffix::from_str("1234")?,
            true,
            "Sensitive estimate".to_owned(),
        );
        let account = Account::new(
            crate::shared::UserId::from_str("123")?,
            crate::shared::Username::from_bare("alice")?,
            Some("Alice".to_owned()),
        );
        let plan = TransferOutPlan::new(
            account,
            Balance::new(
                crate::features::wallet::SignedUsdAmount::from_cents(500),
                crate::features::wallet::SignedUsdAmount::from_cents(0),
            ),
            Money::from_cents(100)?,
            TransferSpeed::Standard,
            instrument.clone(),
        );
        let created = CreatedTransfer::new(
            TransferId::from_str("sensitive-transfer-id")?,
            "issued".to_owned(),
            OffsetDateTime::UNIX_EPOCH,
            Money::from_cents(100)?,
            0,
        );

        let rendered = format!("{instrument:?} {plan:?} {created:?}");
        for secret in [
            "sensitive-bank-id",
            "Sensitive bank name",
            "Sensitive asset",
            "1234",
            "Sensitive estimate",
            "sensitive-transfer-id",
        ] {
            assert!(!rendered.contains(secret));
        }
        assert!(rendered.contains("bank"));
        assert!(rendered.contains("issued"));

        let options = TransferOptions::new(
            None,
            Some(TransferSpeed::Standard),
            TransferModeOptions::new(
                Vec::new(),
                vec![instrument],
                TransferFeeMetadata::new(Some("sensitive-fee".to_owned()), None, None, false),
                "sensitive-mode-estimate".to_owned(),
            ),
            TransferModeOptions::new(
                Vec::new(),
                Vec::new(),
                TransferFeeMetadata::default(),
                "sensitive-instant-estimate".to_owned(),
            ),
        );
        let options_debug = format!("{options:?}");
        for secret in [
            "sensitive-fee",
            "sensitive-mode-estimate",
            "sensitive-instant-estimate",
        ] {
            assert!(!options_debug.contains(secret));
        }
        Ok(())
    }
}
