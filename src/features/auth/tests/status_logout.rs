use super::*;

#[tokio::test(flavor = "current_thread")]
async fn status_returns_the_matching_account_from_a_read_only_reader() -> TestResult {
    let credential = CredentialFixture::stored(AccountIdentity::Primary);
    let calls = transcript();
    let reader = FakeReader::new(
        FakeStoreState::Present(credential.clone()),
        Rc::clone(&calls),
    );
    let api = FakeApi::new(
        ApiScript::successful(test_account(AccountIdentity::Primary)?),
        Rc::clone(&calls),
    );
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

    let result = status(&reader, &api).await;
    let observed = observe_reader(status_outcome(&result), &reader, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_missing_or_unreadable_credentials_skip_the_api() -> TestResult {
    for (state, failure) in [
        (
            FakeStoreState::Missing,
            AuthStatusFailure::CredentialMissing,
        ),
        (
            FakeStoreState::Failure(CredentialFailureKind::Corrupt),
            AuthStatusFailure::CredentialRead,
        ),
    ] {
        let calls = transcript();
        let reader = FakeReader::new(state.clone(), Rc::clone(&calls));
        let api = FakeApi::new(
            ApiScript::successful(test_account(AccountIdentity::Primary)?),
            Rc::clone(&calls),
        );
        let expected = AuthObservation::new(
            Err(AuthStatusFailureSnapshot::synthetic(
                ApplicationFailureKind::Credential,
                failure,
            )),
            state.snapshot(),
            vec![AuthCall::ReadCredential],
        );

        let result = status(&reader, &api).await;
        let observed = observe_reader(status_outcome(&result), &reader, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_rejects_account_mismatch_and_preserves_api_failure_kinds() -> TestResult {
    let credential = CredentialFixture::stored(AccountIdentity::Primary);
    let calls = transcript();
    let reader = FakeReader::new(
        FakeStoreState::Present(credential.clone()),
        Rc::clone(&calls),
    );
    let api = FakeApi::new(
        ApiScript::successful(test_account(AccountIdentity::Secondary)?),
        Rc::clone(&calls),
    );
    let result = status(&reader, &api).await;
    assert!(matches!(result, Err(AuthStatusError::AccountMismatch)));
    assert_eq!(
        reader.snapshot(),
        FakeStoreState::Present(credential).snapshot()
    );

    for kind in [
        ApiFailureKind::Network,
        ApiFailureKind::Timeout,
        ApiFailureKind::Authentication,
        ApiFailureKind::Rejected,
        ApiFailureKind::Contract,
        ApiFailureKind::AmbiguousWrite,
        ApiFailureKind::Internal,
    ] {
        let credential = CredentialFixture::stored(AccountIdentity::Primary);
        let calls = transcript();
        let reader = FakeReader::new(
            FakeStoreState::Present(credential.clone()),
            Rc::clone(&calls),
        );
        let api = FakeApi::new(
            ApiScript {
                current_account: Err(kind),
                ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
            },
            Rc::clone(&calls),
        );
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

        let result = status(&reader, &api).await;
        let observed = observe_reader(status_outcome(&result), &reader, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn logout_only_deletes_the_local_keyring_entry() -> TestResult {
    for (initial, script, local, state, failure) in [
        (
            FakeStoreState::Present(CredentialFixture::stored(AccountIdentity::Primary)),
            StoreScript::NORMAL,
            LocalDeletionSnapshot::Deleted,
            CredentialStateSnapshot::Missing,
            None,
        ),
        (
            FakeStoreState::Missing,
            StoreScript::NORMAL,
            LocalDeletionSnapshot::Missing,
            CredentialStateSnapshot::Missing,
            None,
        ),
        (
            FakeStoreState::Failure(CredentialFailureKind::Corrupt),
            StoreScript::NORMAL,
            LocalDeletionSnapshot::Deleted,
            CredentialStateSnapshot::Missing,
            None,
        ),
        (
            FakeStoreState::Present(CredentialFixture::stored(AccountIdentity::Primary)),
            StoreScript::with_delete(DeleteScript::Fail),
            LocalDeletionSnapshot::Failed,
            FakeStoreState::Present(CredentialFixture::stored(AccountIdentity::Primary)).snapshot(),
            Some(ApplicationFailureKind::Credential),
        ),
    ] {
        let calls = transcript();
        let store = DeleteOnlyFake::new(initial, script, Rc::clone(&calls));
        let expected = AuthObservation::new(
            LogoutSnapshot::synthetic(local, failure.is_none(), failure),
            state,
            vec![AuthCall::DeleteCredential],
        );

        let report = logout_local(&store);
        let observed = observe_deleter(LogoutSnapshot::from_report(&report), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}
