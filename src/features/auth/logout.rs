use std::error::Error;

use super::TokenRevocationApi;
use crate::shared::{
    ApiOperationFailure, ApplicationFailureKind, CredentialDeleteOutcome, CredentialDeleter,
    CredentialReader, OperationFailure,
};

#[derive(Debug)]
pub enum RemoteRevocationOutcome {
    NotRequested,
    NotNeeded,
    Revoked,
    Failed(ApiOperationFailure),
    NotAttempted {
        kind: ApplicationFailureKind,
        source: OperationFailure,
    },
}

impl RemoteRevocationOutcome {
    #[cfg(test)]
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::NotRequested | Self::NotNeeded | Self::Revoked)
    }

    #[must_use]
    pub const fn failure_kind(&self) -> Option<ApplicationFailureKind> {
        match self {
            Self::NotRequested | Self::NotNeeded | Self::Revoked => None,
            Self::Failed(source) => Some(ApplicationFailureKind::Api(source.kind())),
            Self::NotAttempted { kind, .. } => Some(*kind),
        }
    }
}

#[derive(Debug)]
pub enum LocalDeletionOutcome {
    Deleted,
    Missing,
    Failed(OperationFailure),
}

impl LocalDeletionOutcome {
    #[cfg(test)]
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Deleted | Self::Missing)
    }
}

#[derive(Debug)]
pub struct LogoutReport {
    remote: RemoteRevocationOutcome,
    local: LocalDeletionOutcome,
}

impl LogoutReport {
    #[must_use]
    pub const fn remote(&self) -> &RemoteRevocationOutcome {
        &self.remote
    }

    #[must_use]
    pub const fn local(&self) -> &LocalDeletionOutcome {
        &self.local
    }

    #[cfg(test)]
    #[must_use]
    pub const fn is_complete_success(&self) -> bool {
        self.remote.is_complete() && self.local.is_complete()
    }

    #[must_use]
    pub const fn failure_kind(&self) -> Option<ApplicationFailureKind> {
        if let RemoteRevocationOutcome::Failed(source) = &self.remote {
            return Some(ApplicationFailureKind::Api(source.kind()));
        }
        if matches!(self.local, LocalDeletionOutcome::Failed(_)) {
            return Some(ApplicationFailureKind::Credential);
        }
        self.remote.failure_kind()
    }
}

pub async fn logout<S, A>(store: &S, api: &A) -> LogoutReport
where
    S: CredentialReader + CredentialDeleter,
    A: TokenRevocationApi,
{
    let remote = {
        match store.read_credential() {
            Ok(Some(loaded)) => match api
                .revoke_access_token(loaded.envelope.access_token(), loaded.envelope.device_id())
                .await
            {
                Ok(()) => RemoteRevocationOutcome::Revoked,
                Err(source) => RemoteRevocationOutcome::Failed(ApiOperationFailure::new(source)),
            },
            Ok(None) => RemoteRevocationOutcome::NotNeeded,
            Err(source) => RemoteRevocationOutcome::NotAttempted {
                kind: ApplicationFailureKind::Credential,
                source: OperationFailure::new(source),
            },
        }
    };

    let local = delete_local(store);

    LogoutReport { remote, local }
}

pub fn logout_local<S>(store: &S) -> LogoutReport
where
    S: CredentialDeleter,
{
    LogoutReport {
        remote: RemoteRevocationOutcome::NotRequested,
        local: delete_local(store),
    }
}

pub(crate) fn logout_remote_not_attempted<S, F>(store: &S, source: F) -> LogoutReport
where
    S: CredentialDeleter,
    F: Error + Send + Sync + 'static,
{
    LogoutReport {
        remote: RemoteRevocationOutcome::NotAttempted {
            kind: ApplicationFailureKind::Internal,
            source: OperationFailure::new(source),
        },
        local: delete_local(store),
    }
}

fn delete_local<S>(store: &S) -> LocalDeletionOutcome
where
    S: CredentialDeleter,
{
    match store.delete_credential() {
        Ok(CredentialDeleteOutcome::Deleted) => LocalDeletionOutcome::Deleted,
        Ok(CredentialDeleteOutcome::Missing) => LocalDeletionOutcome::Missing,
        Err(source) => LocalDeletionOutcome::Failed(OperationFailure::new(source)),
    }
}
