use std::error::Error;
use std::str::FromStr;

use time::OffsetDateTime;

use super::super::{write_transfer_options, write_transfer_out_details, write_transfer_out_result};
use crate::features::transfers::model::{
    CreatedTransfer, TransferFeeMetadata, TransferId, TransferInstrument, TransferInstrumentId,
    TransferInstrumentSuffix, TransferModeOptions, TransferOptions, TransferOutAmount,
    TransferOutPlan, TransferSpeed,
};
use crate::features::transfers::options::TransferOptionsResult;
use crate::features::transfers::out::{PreparedTransferOut, TransferOutResult};
use crate::features::wallet::{Balance, SignedUsdAmount};
use crate::shared::{AccessToken, Account, CredentialEnvelope, DeviceId, Money, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn transfer_options_output_is_complete_and_sanitized() -> TestResult {
    let result = TransferOptionsResult::new(TransferOptions::new(
        Some(TransferSpeed::Standard),
        TransferModeOptions::new(
            vec![instrument("destination-1", "Outbound Bank", "2222", true)?],
            TransferFeeMetadata::default(),
            "1-3 business days".to_owned(),
        ),
        TransferModeOptions::new(
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
fn transfer_details_and_result_outputs_are_complete() -> TestResult {
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
    let mut details = Vec::new();
    let mut completed = Vec::new();
    let timestamps = super::local_timestamps();

    write_transfer_out_details(&mut details, &prepared)?;
    write_transfer_out_result(&mut completed, &result, &timestamps)?;

    insta::assert_snapshot!("transfer_out_details", String::from_utf8(details)?);
    insta::assert_snapshot!("transfer_out_result", String::from_utf8(completed)?);

    let all_plan = all_plan()?;
    let all_prepared = PreparedTransferOut::new(credential()?, all_plan.clone());
    let all_result = TransferOutResult::new(
        all_plan,
        CreatedTransfer::new(
            TransferId::from_str("transfer-all")?,
            "pending".to_owned(),
            OffsetDateTime::UNIX_EPOCH,
            Money::from_cents(10_000)?,
            0,
        ),
    );
    let mut all_details = Vec::new();
    let mut all_completed = Vec::new();
    write_transfer_out_details(&mut all_details, &all_prepared)?;
    write_transfer_out_result(&mut all_completed, &all_result, &timestamps)?;
    insta::assert_snapshot!("transfer_out_all_details", String::from_utf8(all_details)?);
    insta::assert_snapshot!("transfer_out_all_result", String::from_utf8(all_completed)?);
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
        TransferOutAmount::Exact(Money::from_cents(1_234)?),
        Money::from_cents(1_234)?,
        TransferSpeed::Standard,
        instrument("destination-1", "Synthetic Bank", "2222", true)?,
    ))
}

fn all_plan() -> Result<TransferOutPlan, Box<dyn Error>> {
    Ok(TransferOutPlan::new(
        account()?,
        Balance::new(
            SignedUsdAmount::from_cents(10_000),
            SignedUsdAmount::from_cents(500),
        ),
        TransferOutAmount::AllAvailable,
        Money::from_cents(10_000)?,
        TransferSpeed::Standard,
        instrument("destination-all", "Synthetic Bank", "2222", true)?,
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
