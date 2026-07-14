use crate::shared::{
    AccessToken, Account, Clock, CredentialEnvelope, CredentialFormat, CredentialReader,
    CredentialWriter, DeviceId, LoadedCredential, OperationFailure,
};

use super::{LoginDisposition, LoginError, LoginResult};

#[derive(Clone, Copy)]
pub(super) enum CredentialOrigin {
    Imported,
    Issued,
}

pub(super) enum ExistingCredential {
    Missing,
    Loaded(LoadedCredential),
    Replaceable,
}

pub(super) fn persist_validated_login<S, C>(
    store: &S,
    clock: &C,
    existing: ExistingCredential,
    account: Account,
    access_token: AccessToken,
    device_id: DeviceId,
    origin: CredentialOrigin,
) -> Result<(LoginResult, CredentialEnvelope), LoginError>
where
    S: CredentialReader + CredentialWriter,
    C: Clock,
{
    let disposition = match existing {
        ExistingCredential::Missing => LoginDisposition::Created,
        ExistingCredential::Loaded(_) => LoginDisposition::ReplacedForSameAccount,
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
        .map_err(|source| storage_state_unknown(origin, Some(OperationFailure::new(source))))?;
    let verification = store
        .read_credential()
        .map_err(|source| storage_state_unknown(origin, Some(OperationFailure::new(source))))?;
    let Some(verification) = verification else {
        return Err(storage_state_unknown(origin, None));
    };
    if verification.format != CredentialFormat::Version1
        || !credential.storage_equivalent(&verification.envelope)
    {
        return Err(storage_state_unknown(origin, None));
    }

    Ok((LoginResult::new(account, saved_at, disposition), credential))
}

fn storage_state_unknown(origin: CredentialOrigin, source: Option<OperationFailure>) -> LoginError {
    match origin {
        CredentialOrigin::Imported => LoginError::CredentialStorageStateUnknown { source },
        CredentialOrigin::Issued => LoginError::IssuedCredentialStorageStateUnknown { source },
    }
}
