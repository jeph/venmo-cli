use crate::shared::{
    ApplicationFailureKind, CredentialDeleteOutcome, CredentialDeleter, OperationFailure,
};

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
    local: LocalDeletionOutcome,
}

impl LogoutReport {
    #[must_use]
    pub const fn local(&self) -> &LocalDeletionOutcome {
        &self.local
    }

    #[cfg(test)]
    #[must_use]
    pub const fn is_complete_success(&self) -> bool {
        self.local.is_complete()
    }

    #[must_use]
    pub const fn failure_kind(&self) -> Option<ApplicationFailureKind> {
        if matches!(self.local, LocalDeletionOutcome::Failed(_)) {
            return Some(ApplicationFailureKind::Credential);
        }
        None
    }
}

pub fn logout_local<S>(store: &S) -> LogoutReport
where
    S: CredentialDeleter,
{
    LogoutReport {
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
