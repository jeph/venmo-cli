use crate::shared::{
    AccessToken, Account, Clock, CredentialEnvelope, CredentialFormat, CredentialReader,
    CredentialWriter, DeviceId, OperationFailure,
};

use super::{LoginDisposition, LoginError, LoginResult};

pub(super) enum ExistingCredential {
    Missing,
    Present,
    Replaceable,
}

pub(super) fn persist_validated_login<S, C>(
    store: &S,
    clock: &C,
    existing: ExistingCredential,
    account: Account,
    access_token: AccessToken,
    device_id: DeviceId,
) -> Result<(LoginResult, CredentialEnvelope), LoginError>
where
    S: CredentialReader + CredentialWriter,
    C: Clock,
{
    let disposition = match existing {
        ExistingCredential::Missing => LoginDisposition::Created,
        ExistingCredential::Present => LoginDisposition::ReplacedExistingCredential,
        ExistingCredential::Replaceable => LoginDisposition::RecoveredUnusableEntry,
    };
    let saved_at = clock.now_utc();
    let credential = CredentialEnvelope::new(
        access_token,
        device_id,
        account.user_id().clone(),
        account.username().clone(),
        account.display_name().map(str::to_owned),
        saved_at,
    );

    store
        .save_credential(&credential)
        .map_err(|source| storage_state_unknown(Some(OperationFailure::new(source))))?;
    let verification = store
        .read_credential()
        .map_err(|source| storage_state_unknown(Some(OperationFailure::new(source))))?;
    let Some(verification) = verification else {
        return Err(storage_state_unknown(None));
    };
    if verification.format != CredentialFormat::Version1
        || !credential.storage_equivalent(&verification.envelope)
    {
        return Err(storage_state_unknown(None));
    }

    Ok((LoginResult::new(account, saved_at, disposition), credential))
}

fn storage_state_unknown(source: Option<OperationFailure>) -> LoginError {
    LoginError::IssuedCredentialStorageStateUnknown { source }
}
