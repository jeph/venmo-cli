use super::login::{
    ACCOUNT_IDENTIFIER_PROMPT, ACCOUNT_PASSWORD_PROMPT, LoginError, PasswordLoginReport,
    issue_password_login, persist_issued_password_login,
};
use super::persistence::ExistingCredential;
use super::{AuthenticationInput, CurrentAccountApi, PasswordLoginApi, PromptError};
use crate::shared::{
    ApiOperationFailure, Clock, CredentialReader, CredentialWriter, OperationFailure,
};

/// Issues a replacement token while preserving the readable stored credential's device identity.
///
/// The stored access token is deliberately not validated: this operation exists so an expired
/// token can be replaced without losing the device ID that Venmo already recognizes.
pub async fn reauthenticate<S, P, A, C>(
    store: &S,
    prompt: &P,
    api: &A,
    clock: &C,
) -> Result<PasswordLoginReport, LoginError>
where
    S: CredentialReader + CredentialWriter,
    P: AuthenticationInput,
    A: CurrentAccountApi + PasswordLoginApi,
    C: Clock,
{
    if !prompt.can_prompt() {
        return Err(LoginError::Prompt(PromptError::NotInteractive));
    }

    let loaded = store
        .read_credential()
        .map_err(|source| LoginError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(LoginError::ReauthenticationCredentialMissing)?;
    let expected_user_id = loaded.envelope.user_id().clone();
    let device_id = loaded.envelope.device_id().clone();

    let identifier = prompt.read_login_identifier(ACCOUNT_IDENTIFIER_PROMPT)?;
    let password = prompt.read_account_password(ACCOUNT_PASSWORD_PROMPT)?;
    let issued_login = issue_password_login(prompt, api, &device_id, identifier, password).await?;
    let account = api
        .current_account(issued_login.access_token(), &device_id)
        .await
        .map_err(|source| LoginError::IssuedTokenValidation {
            source: ApiOperationFailure::new(source),
        })?;
    if account.user_id() != &expected_user_id {
        return Err(LoginError::IssuedTokenDifferentAccount);
    }

    persist_issued_password_login(
        store,
        api,
        clock,
        ExistingCredential::Loaded(loaded),
        account,
        issued_login,
        device_id,
    )
    .await
}
