use std::error::Error;
use std::str::FromStr;

use time::OffsetDateTime;

use super::super::{
    write_transfer_in_preflight, write_transfer_in_result, write_transfer_options,
    write_transfer_out_preflight, write_transfer_out_result,
};
use crate::features::transfers::in_transfer::{PreparedTransferIn, TransferInResult};
use crate::features::transfers::model::{
    CreatedTransfer, CreatedTransferIn, TransferFeeMetadata, TransferId, TransferInPlan,
    TransferInstrument, TransferInstrumentId, TransferInstrumentSuffix, TransferModeOptions,
    TransferOptions, TransferOutPlan, TransferPayoutId, TransferSpeed,
};
use crate::features::transfers::options::TransferOptionsResult;
use crate::features::transfers::out::{PreparedTransferOut, TransferOutResult};
use crate::features::wallet::{Balance, SignedUsdAmount};
use crate::shared::{AccessToken, Account, CredentialEnvelope, DeviceId, Money, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn transfer_options_output_is_complete_and_sanitized() -> TestResult {
    let result = TransferOptionsResult::new(TransferOptions::new(
        None,
        Some(TransferSpeed::Standard),
        TransferModeOptions::new(
            vec![instrument("source-1", "Inbound\nBank", "1111", true)?],
            vec![instrument("destination-1", "Outbound Bank", "2222", true)?],
            TransferFeeMetadata::default(),
            "1-3 business days".to_owned(),
        ),
        TransferModeOptions::new(
            Vec::new(),
            Vec::new(),
            TransferFeeMetadata::new(
                Some("25".to_owned()),
                Some("2500".to_owned()),
                Some("1.75".to_owned()),
                true,
            ),
            "Usually within minutes".to_owned(),
        ),
    ));
    let mut output = Vec::new();

    write_transfer_options(&mut output, &result)?;

    let output = String::from_utf8(output)?;
    insta::assert_snapshot!("transfer_options", output);
    Ok(())
}

#[test]
fn transfer_preflight_and_result_outputs_are_complete() -> TestResult {
    let plan = plan()?;
    let prepared = PreparedTransferOut::new(credential()?, plan.clone());
    let result = TransferOutResult::new(
        plan,
        CreatedTransfer::new(
            TransferId::from_str("transfer-1")?,
            "pending".to_owned(),
            OffsetDateTime::UNIX_EPOCH,
            Money::from_cents(1_234)?,
            0,
        ),
    );
    let mut preflight = Vec::new();
    let mut completed = Vec::new();
    let timestamps = super::local_timestamps();

    write_transfer_out_preflight(&mut preflight, &prepared)?;
    write_transfer_out_result(&mut completed, &result, &timestamps)?;

    insta::assert_snapshot!("transfer_out_preflight", String::from_utf8(preflight)?);
    insta::assert_snapshot!("transfer_out_result", String::from_utf8(completed)?);
    Ok(())
}

#[test]
fn transfer_in_preflight_and_result_outputs_are_complete() -> TestResult {
    let plan = TransferInPlan::new(
        account()?,
        Money::from_cents(1_234)?,
        instrument("source-1", "Synthetic Bank", "1111", true)?,
    );
    let prepared = PreparedTransferIn::new(credential()?, plan.clone());
    let result = TransferInResult::new(
        plan,
        CreatedTransferIn::new(
            TransferPayoutId::from_str("payout-1")?,
            "pending".to_owned(),
            Money::from_cents(1_234)?,
            OffsetDateTime::UNIX_EPOCH,
        ),
    );
    let mut preflight = Vec::new();
    let mut completed = Vec::new();
    let timestamps = super::local_timestamps();

    write_transfer_in_preflight(&mut preflight, &prepared)?;
    write_transfer_in_result(&mut completed, &result, &timestamps)?;

    insta::assert_snapshot!("transfer_in_preflight", String::from_utf8(preflight)?);
    insta::assert_snapshot!("transfer_in_result", String::from_utf8(completed)?);
    Ok(())
}

fn instrument(
    id: &str,
    name: &str,
    last_four: &str,
    is_default: bool,
) -> Result<TransferInstrument, Box<dyn Error>> {
    Ok(TransferInstrument::new(
        TransferInstrumentId::from_str(id)?,
        name.to_owned(),
        "Checking".to_owned(),
        "bank".to_owned(),
        TransferInstrumentSuffix::from_str(last_four)?,
        is_default,
        "1-3 business days".to_owned(),
    ))
}

fn plan() -> Result<TransferOutPlan, Box<dyn Error>> {
    Ok(TransferOutPlan::new(
        account()?,
        Balance::new(
            SignedUsdAmount::from_cents(10_000),
            SignedUsdAmount::from_cents(0),
        ),
        Money::from_cents(1_234)?,
        TransferSpeed::Standard,
        instrument("destination-1", "Synthetic Bank", "2222", true)?,
    ))
}

fn account() -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str("123")?,
        Username::from_bare("alice")?,
        Some("Alice".to_owned()),
    ))
}

fn credential() -> Result<CredentialEnvelope, Box<dyn Error>> {
    Ok(CredentialEnvelope::new(
        AccessToken::from_str("synthetic-token")?,
        DeviceId::from_str("synthetic-device")?,
        UserId::from_str("123")?,
        Username::from_bare("alice")?,
        Some("Alice".to_owned()),
        OffsetDateTime::UNIX_EPOCH,
    ))
}
