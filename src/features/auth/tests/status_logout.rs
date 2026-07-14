use thiserror::Error;

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn status_returns_the_matching_account_from_a_read_only_reader() -> TestResult {
    // Setup scripts/DI.
    let credential = CredentialFixture::stored(AccountIdentity::Primary);
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

    // Immutable initial state.
    let calls = transcript();
    let reader = FakeReader::new(
        FakeStoreState::Present(credential.clone()),
        Rc::clone(&calls),
    );
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let expected = AuthObservation::new(
        Ok(AuthStatusSnapshot::synthetic(
            AccountIdentity::Primary,
            OffsetDateTime::UNIX_EPOCH,
            CredentialFormat::Version1,
        )),
        FakeStoreState::Present(credential).snapshot(),
        vec![
            AuthCall::ReadCredential,
            current_account_call(TokenMaterial::Stored, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = status(&reader, &api).await;
    let observed = observe_reader(status_outcome(&result), &reader, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_rejects_an_account_mismatch_without_mutation() -> TestResult {
    // Setup scripts/DI.
    let credential = CredentialFixture::stored(AccountIdentity::Primary);
    let api_script = ApiScript::successful(test_account(AccountIdentity::Secondary)?);

    // Immutable initial state.
    let calls = transcript();
    let reader = FakeReader::new(
        FakeStoreState::Present(credential.clone()),
        Rc::clone(&calls),
    );
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let expected = AuthObservation::new(
        Err(AuthStatusFailureSnapshot::synthetic(
            ApplicationFailureKind::Credential,
            AuthStatusFailure::AccountMismatch,
        )),
        FakeStoreState::Present(credential).snapshot(),
        vec![
            AuthCall::ReadCredential,
            current_account_call(TokenMaterial::Stored, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = status(&reader, &api).await;
    let observed = observe_reader(status_outcome(&result), &reader, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_missing_credential_skips_the_api() -> TestResult {
    // Setup scripts/DI.
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

    // Immutable initial state.
    let calls = transcript();
    let reader = FakeReader::new(FakeStoreState::Missing, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let expected = AuthObservation::new(
        Err(AuthStatusFailureSnapshot::synthetic(
            ApplicationFailureKind::Credential,
            AuthStatusFailure::CredentialMissing,
        )),
        CredentialStateSnapshot::Missing,
        vec![AuthCall::ReadCredential],
    );

    // Execute once.
    let result = status(&reader, &api).await;
    let observed = observe_reader(status_outcome(&result), &reader, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_load_failure_matrix_preserves_credential_category_and_skips_the_api() -> TestResult
{
    for kind in [
        CredentialFailureKind::Unavailable,
        CredentialFailureKind::Corrupt,
        CredentialFailureKind::Invalid,
        CredentialFailureKind::UnsupportedVersion,
        CredentialFailureKind::TooLarge,
        CredentialFailureKind::Ambiguous,
        CredentialFailureKind::Platform,
        CredentialFailureKind::Internal,
    ] {
        // Setup scripts/DI.
        let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

        // Immutable initial state.
        let calls = transcript();
        let reader = FakeReader::new(FakeStoreState::Failure(kind), Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let expected = AuthObservation::new(
            Err(AuthStatusFailureSnapshot::synthetic(
                ApplicationFailureKind::Credential,
                AuthStatusFailure::CredentialRead,
            )),
            CredentialStateSnapshot::Failure(kind),
            vec![AuthCall::ReadCredential],
        );

        // Execute once.
        let result = status(&reader, &api).await;
        let observed = observe_reader(status_outcome(&result), &reader, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_api_failure_matrix_preserves_every_failure_kind() -> TestResult {
    for kind in [
        ApiFailureKind::Network,
        ApiFailureKind::Timeout,
        ApiFailureKind::Rejected,
        ApiFailureKind::Contract,
        ApiFailureKind::AmbiguousWrite,
        ApiFailureKind::Internal,
    ] {
        // Setup scripts/DI.
        let credential = CredentialFixture::stored(AccountIdentity::Primary);
        let api_script = ApiScript {
            current_account: Err(kind),
            ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
        };

        // Immutable initial state.
        let calls = transcript();
        let reader = FakeReader::new(
            FakeStoreState::Present(credential.clone()),
            Rc::clone(&calls),
        );
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let expected = AuthObservation::new(
            Err(AuthStatusFailureSnapshot::synthetic(
                ApplicationFailureKind::Api(kind),
                AuthStatusFailure::TokenValidation(kind),
            )),
            FakeStoreState::Present(credential).snapshot(),
            vec![
                AuthCall::ReadCredential,
                current_account_call(TokenMaterial::Stored, DeviceMaterial::Stored),
            ],
        );

        // Execute once.
        let result = status(&reader, &api).await;
        let observed = observe_reader(status_outcome(&result), &reader, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum LocalOnlyCase {
    PresentDeleted,
    Missing,
    UnreadableDeleted,
    DeleteFailure,
}

#[test]
fn local_only_logout_needs_only_deletion_and_covers_every_local_outcome() -> TestResult {
    for case in [
        LocalOnlyCase::PresentDeleted,
        LocalOnlyCase::Missing,
        LocalOnlyCase::UnreadableDeleted,
        LocalOnlyCase::DeleteFailure,
    ] {
        // Setup scripts/DI.
        let (initial, store_script, expected_local, expected_state) = match case {
            LocalOnlyCase::PresentDeleted => (
                FakeStoreState::Present(CredentialFixture::stored(AccountIdentity::Primary)),
                StoreScript::NORMAL,
                LocalDeletionSnapshot::Deleted,
                CredentialStateSnapshot::Missing,
            ),
            LocalOnlyCase::Missing => (
                FakeStoreState::Missing,
                StoreScript::NORMAL,
                LocalDeletionSnapshot::Missing,
                CredentialStateSnapshot::Missing,
            ),
            LocalOnlyCase::UnreadableDeleted => (
                FakeStoreState::Failure(CredentialFailureKind::Corrupt),
                StoreScript::NORMAL,
                LocalDeletionSnapshot::Deleted,
                CredentialStateSnapshot::Missing,
            ),
            LocalOnlyCase::DeleteFailure => {
                let initial =
                    FakeStoreState::Present(CredentialFixture::stored(AccountIdentity::Primary));
                let expected_state = initial.snapshot();
                (
                    initial,
                    StoreScript::with_delete(DeleteScript::Fail),
                    LocalDeletionSnapshot::Failed,
                    expected_state,
                )
            }
        };

        // Immutable initial state.
        let calls = transcript();
        let store = DeleteOnlyFake::new(initial, store_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let complete = !matches!(expected_local, LocalDeletionSnapshot::Failed);
        let failure_kind = (!complete).then_some(ApplicationFailureKind::Credential);
        let expected = AuthObservation::new(
            LogoutSnapshot::synthetic(
                RemoteRevocationSnapshot::NotRequested,
                expected_local,
                complete,
                failure_kind,
            ),
            expected_state,
            vec![AuthCall::DeleteCredential],
        );

        // Execute once.
        let report = logout_local(&store);
        let observed = observe_deleter(LogoutSnapshot::from_report(&report), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn revoke_requested_with_missing_credential_covers_every_local_outcome() -> TestResult {
    for delete_script in [
        DeleteScript::Normal,
        DeleteScript::ReportDeleted,
        DeleteScript::Fail,
    ] {
        // Setup scripts/DI.
        let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
        let store_script = StoreScript::with_delete(delete_script);

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(FakeStoreState::Missing, store_script, Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let (local, complete, failure_kind) = match delete_script {
            DeleteScript::Normal | DeleteScript::ReportMissing => {
                (LocalDeletionSnapshot::Missing, true, None)
            }
            DeleteScript::ReportDeleted => (LocalDeletionSnapshot::Deleted, true, None),
            DeleteScript::Fail => (
                LocalDeletionSnapshot::Failed,
                false,
                Some(ApplicationFailureKind::Credential),
            ),
        };
        let expected = AuthObservation::new(
            LogoutSnapshot::synthetic(
                RemoteRevocationSnapshot::NotNeeded,
                local,
                complete,
                failure_kind,
            ),
            CredentialStateSnapshot::Missing,
            vec![AuthCall::LoadCredential, AuthCall::DeleteCredential],
        );

        // Execute once.
        let report = logout(&store, &api).await;
        let observed = observe_store(LogoutSnapshot::from_report(&report), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn successful_remote_revocation_covers_every_local_outcome() -> TestResult {
    for delete_script in [
        DeleteScript::Normal,
        DeleteScript::ReportMissing,
        DeleteScript::Fail,
    ] {
        // Setup scripts/DI.
        let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
        let store_script = StoreScript::with_delete(delete_script);
        let initial = CredentialFixture::stored(AccountIdentity::Primary);

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(
            FakeStoreState::Present(initial.clone()),
            store_script,
            Rc::clone(&calls),
        );
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let (local, complete, failure_kind, expected_state) = match delete_script {
            DeleteScript::Normal | DeleteScript::ReportDeleted => (
                LocalDeletionSnapshot::Deleted,
                true,
                None,
                CredentialStateSnapshot::Missing,
            ),
            DeleteScript::ReportMissing => (
                LocalDeletionSnapshot::Missing,
                true,
                None,
                CredentialStateSnapshot::Missing,
            ),
            DeleteScript::Fail => (
                LocalDeletionSnapshot::Failed,
                false,
                Some(ApplicationFailureKind::Credential),
                FakeStoreState::Present(initial).snapshot(),
            ),
        };
        let expected = AuthObservation::new(
            LogoutSnapshot::synthetic(
                RemoteRevocationSnapshot::Revoked,
                local,
                complete,
                failure_kind,
            ),
            expected_state,
            vec![
                AuthCall::LoadCredential,
                revoke_call(TokenMaterial::Stored, DeviceMaterial::Stored),
                AuthCall::DeleteCredential,
            ],
        );

        // Execute once.
        let report = logout(&store, &api).await;
        let observed = observe_store(LogoutSnapshot::from_report(&report), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn remote_revocation_failure_matrix_preserves_api_priority_over_local_failure() -> TestResult
{
    for kind in [
        ApiFailureKind::Network,
        ApiFailureKind::Timeout,
        ApiFailureKind::Rejected,
        ApiFailureKind::Contract,
        ApiFailureKind::AmbiguousWrite,
        ApiFailureKind::Internal,
    ] {
        for delete_script in [
            DeleteScript::Normal,
            DeleteScript::ReportMissing,
            DeleteScript::Fail,
        ] {
            // Setup scripts/DI.
            let api_script = ApiScript {
                revoke: Err(kind),
                ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
            };
            let store_script = StoreScript::with_delete(delete_script);
            let initial = CredentialFixture::stored(AccountIdentity::Primary);

            // Immutable initial state.
            let calls = transcript();
            let store = FakeStore::new(
                FakeStoreState::Present(initial.clone()),
                store_script,
                Rc::clone(&calls),
            );
            let api = FakeApi::new(api_script, Rc::clone(&calls));

            // Complete expected final state/outcome.
            let (local, expected_state) = match delete_script {
                DeleteScript::Normal | DeleteScript::ReportDeleted => (
                    LocalDeletionSnapshot::Deleted,
                    CredentialStateSnapshot::Missing,
                ),
                DeleteScript::ReportMissing => (
                    LocalDeletionSnapshot::Missing,
                    CredentialStateSnapshot::Missing,
                ),
                DeleteScript::Fail => (
                    LocalDeletionSnapshot::Failed,
                    FakeStoreState::Present(initial).snapshot(),
                ),
            };
            let expected = AuthObservation::new(
                LogoutSnapshot::synthetic(
                    RemoteRevocationSnapshot::Failed(kind),
                    local,
                    false,
                    Some(ApplicationFailureKind::Api(kind)),
                ),
                expected_state,
                vec![
                    AuthCall::LoadCredential,
                    revoke_call(TokenMaterial::Stored, DeviceMaterial::Stored),
                    AuthCall::DeleteCredential,
                ],
            );

            // Execute once.
            let report = logout(&store, &api).await;
            let observed = observe_store(LogoutSnapshot::from_report(&report), &store, &calls);

            assert_eq!(observed, expected);
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn unreadable_remote_credential_matrix_still_attempts_every_local_outcome() -> TestResult {
    for kind in [
        CredentialFailureKind::Unavailable,
        CredentialFailureKind::Corrupt,
        CredentialFailureKind::Invalid,
        CredentialFailureKind::UnsupportedVersion,
        CredentialFailureKind::TooLarge,
        CredentialFailureKind::Ambiguous,
        CredentialFailureKind::Platform,
        CredentialFailureKind::Internal,
    ] {
        for delete_script in [
            DeleteScript::Normal,
            DeleteScript::ReportMissing,
            DeleteScript::Fail,
        ] {
            // Setup scripts/DI.
            let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
            let store_script = StoreScript::with_delete(delete_script);

            // Immutable initial state.
            let calls = transcript();
            let store = FakeStore::new(
                FakeStoreState::Failure(kind),
                store_script,
                Rc::clone(&calls),
            );
            let api = FakeApi::new(api_script, Rc::clone(&calls));

            // Complete expected final state/outcome.
            let (local, expected_state) = match delete_script {
                DeleteScript::Normal | DeleteScript::ReportDeleted => (
                    LocalDeletionSnapshot::Deleted,
                    CredentialStateSnapshot::Missing,
                ),
                DeleteScript::ReportMissing => (
                    LocalDeletionSnapshot::Missing,
                    CredentialStateSnapshot::Missing,
                ),
                DeleteScript::Fail => (
                    LocalDeletionSnapshot::Failed,
                    CredentialStateSnapshot::Failure(kind),
                ),
            };
            let expected = AuthObservation::new(
                LogoutSnapshot::synthetic(
                    RemoteRevocationSnapshot::NotAttempted(ApplicationFailureKind::Credential),
                    local,
                    false,
                    Some(ApplicationFailureKind::Credential),
                ),
                expected_state,
                vec![AuthCall::LoadCredential, AuthCall::DeleteCredential],
            );

            // Execute once.
            let report = logout(&store, &api).await;
            let observed = observe_store(LogoutSnapshot::from_report(&report), &store, &calls);

            assert_eq!(observed, expected);
        }
    }
    Ok(())
}

const SENSITIVE_INITIALIZATION_DETAIL: &str = "sensitive initialization detail";

#[derive(Debug, Error)]
#[error("safe initialization failure")]
struct SensitiveInitializationFailure(&'static str);

#[test]
fn unavailable_remote_revocation_matrix_covers_local_outcomes_and_redacts_its_source() -> TestResult
{
    for delete_script in [
        DeleteScript::Normal,
        DeleteScript::ReportMissing,
        DeleteScript::Fail,
    ] {
        // Setup scripts/DI.
        let store_script = StoreScript::with_delete(delete_script);
        let initial = CredentialFixture::stored(AccountIdentity::Primary);

        // Immutable initial state.
        let calls = transcript();
        let store = DeleteOnlyFake::new(
            FakeStoreState::Present(initial.clone()),
            store_script,
            Rc::clone(&calls),
        );

        // Complete expected final state/outcome.
        let (local, failure_kind, expected_state) = match delete_script {
            DeleteScript::Normal | DeleteScript::ReportDeleted => (
                LocalDeletionSnapshot::Deleted,
                Some(ApplicationFailureKind::Internal),
                CredentialStateSnapshot::Missing,
            ),
            DeleteScript::ReportMissing => (
                LocalDeletionSnapshot::Missing,
                Some(ApplicationFailureKind::Internal),
                CredentialStateSnapshot::Missing,
            ),
            DeleteScript::Fail => (
                LocalDeletionSnapshot::Failed,
                Some(ApplicationFailureKind::Credential),
                FakeStoreState::Present(initial).snapshot(),
            ),
        };
        let expected = AuthObservation::new(
            LogoutSnapshot::synthetic(
                RemoteRevocationSnapshot::NotAttempted(ApplicationFailureKind::Internal),
                local,
                false,
                failure_kind,
            ),
            expected_state,
            vec![AuthCall::DeleteCredential],
        );

        // Execute once.
        let report = logout_remote_not_attempted(
            &store,
            SensitiveInitializationFailure(SENSITIVE_INITIALIZATION_DETAIL),
        );
        let rendered = format!("{report:?}");
        let observed = observe_deleter(LogoutSnapshot::from_report(&report), &store, &calls);

        assert_eq!(observed, expected);
        assert!(!rendered.contains(SENSITIVE_INITIALIZATION_DETAIL));
        assert_auth_material_not_disclosed(&rendered);
    }
    Ok(())
}
