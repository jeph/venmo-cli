use super::lookup;
use super::{RecipientInput, ResolvedRecipient, UserLookupApi, UserSearchApi};
use crate::shared::CredentialEnvelope;

pub(crate) use super::lookup::UserLookupError as RecipientResolutionError;

#[cfg(test)]
pub(crate) use super::lookup::UserLookupFailureKind as RecipientResolutionFailureKind;
#[cfg(test)]
use super::users::UserSearchError;
#[cfg(test)]
use super::{User, UserSearchQuery};
#[cfg(test)]
use crate::shared::{ApiFailureKind, Username};

pub(crate) async fn resolve_with_credential<A>(
    credential: &CredentialEnvelope,
    api: &A,
    recipient: &RecipientInput,
) -> Result<ResolvedRecipient, RecipientResolutionError>
where
    A: UserLookupApi + UserSearchApi,
{
    lookup::resolve_with_credential(credential, api, recipient.username())
        .await
        .map(ResolvedRecipient::new)
}

#[cfg(test)]
#[path = "recipients_tests.rs"]
mod tests;
