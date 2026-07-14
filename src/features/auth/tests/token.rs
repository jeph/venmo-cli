use super::*;

#[tokio::test(flavor = "current_thread")]
async fn token_validation_preserves_every_api_failure_kind_and_never_saves() -> TestResult {
    for kind in [
        ApiFailureKind::Network,
        ApiFailureKind::Timeout,
        ApiFailureKind::Rejected,
        ApiFailureKind::Contract,
        ApiFailureKind::AmbiguousWrite,
        ApiFailureKind::Internal,
    ] {
        // Setup scripts/DI.
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
        let prompt_script = PromptScript::token_login();
        let api_script = ApiScript {
            current_account: Err(kind),
            ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
        };

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(
            FakeStoreState::Missing,
            StoreScript::NORMAL,
            Rc::clone(&calls),
        );
        let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let expected = AuthObservation::new(
            Err(LoginFailureSnapshot::synthetic(
                ApplicationFailureKind::Api(kind),
                LoginFailure::TokenValidation(kind),
            )),
            CredentialStateSnapshot::Missing,
            vec![
                AuthCall::CheckPromptAvailability,
                AuthCall::LoadCredential,
                AuthCall::ReadAccessToken,
                AuthCall::ReadDeviceId,
                current_account_call(TokenMaterial::Imported, DeviceMaterial::Prompted),
            ],
        );

        // Execute once.
        let result = login_with_token(&store, &prompt, &api, &clock).await;
        let observed = observe_store(login_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn token_import_requires_the_matching_device_and_verifies_storage() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(5));
    let prompt_script = PromptScript::token_login();
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

    // Immutable initial state.
    let calls = transcript();
    let store = FakeStore::new(
        FakeStoreState::Missing,
        StoreScript::NORMAL,
        Rc::clone(&calls),
    );
    let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let saved = CredentialSnapshot::synthetic(
        TokenMaterial::Imported,
        DeviceMaterial::Prompted,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(LoginSnapshot::synthetic(
            AccountIdentity::Primary,
            clock.0,
            LoginDisposition::Created,
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadAccessToken,
            AuthCall::ReadDeviceId,
            current_account_call(TokenMaterial::Imported, DeviceMaterial::Prompted),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
        ],
    );

    // Execute once.
    let result = login_with_token(&store, &prompt, &api, &clock).await;
    let observed = observe_store(login_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_imported_device_stops_before_api_or_storage() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript {
        device: DeviceInputScript::Invalid,
        ..PromptScript::token_login()
    };
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

    // Immutable initial state.
    let calls = transcript();
    let store = FakeStore::new(
        FakeStoreState::Missing,
        StoreScript::NORMAL,
        Rc::clone(&calls),
    );
    let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let expected = AuthObservation::new(
        Err(LoginFailureSnapshot::synthetic(
            ApplicationFailureKind::Usage,
            LoginFailure::Prompt(PromptFailureSnapshot::InvalidDeviceId),
        )),
        CredentialStateSnapshot::Missing,
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadAccessToken,
            AuthCall::ReadDeviceId,
        ],
    );

    // Execute once.
    let result = login_with_token(&store, &prompt, &api, &clock).await;
    let observed = observe_store(login_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn noninteractive_token_login_touches_no_keyring_prompts_or_api() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::noninteractive();
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

    // Immutable initial state.
    let calls = transcript();
    let store = FakeStore::new(
        FakeStoreState::Missing,
        StoreScript::NORMAL,
        Rc::clone(&calls),
    );
    let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let expected = AuthObservation::new(
        Err(LoginFailureSnapshot::synthetic(
            ApplicationFailureKind::Usage,
            LoginFailure::Prompt(PromptFailureSnapshot::NotInteractive),
        )),
        CredentialStateSnapshot::Missing,
        vec![AuthCall::CheckPromptAvailability],
    );

    // Execute once.
    let result = login_with_token(&store, &prompt, &api, &clock).await;
    let observed = observe_store(login_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn token_login_reuses_the_stored_device_but_blocks_a_different_account() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::token_login();
    let api_script = ApiScript::successful(test_account(AccountIdentity::Secondary)?);
    let initial_credential = CredentialFixture::stored(AccountIdentity::Primary);

    // Immutable initial state.
    let calls = transcript();
    let store = FakeStore::new(
        FakeStoreState::Present(initial_credential.clone()),
        StoreScript::NORMAL,
        Rc::clone(&calls),
    );
    let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let expected = AuthObservation::new(
        Err(LoginFailureSnapshot::synthetic(
            ApplicationFailureKind::Credential,
            LoginFailure::DifferentAccount,
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadAccessToken,
            current_account_call(TokenMaterial::Imported, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = login_with_token(&store, &prompt, &api, &clock).await;
    let observed = observe_store(login_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn token_login_replaces_a_credential_for_the_same_account() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(7));
    let prompt_script = PromptScript::token_login();
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
    let initial_credential = CredentialFixture::stored(AccountIdentity::Primary);

    // Immutable initial state.
    let calls = transcript();
    let store = FakeStore::new(
        FakeStoreState::Present(initial_credential),
        StoreScript::NORMAL,
        Rc::clone(&calls),
    );
    let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let saved = CredentialSnapshot::synthetic(
        TokenMaterial::Imported,
        DeviceMaterial::Stored,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(LoginSnapshot::synthetic(
            AccountIdentity::Primary,
            clock.0,
            LoginDisposition::ReplacedForSameAccount,
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadAccessToken,
            current_account_call(TokenMaterial::Imported, DeviceMaterial::Stored),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
        ],
    );

    // Execute once.
    let result = login_with_token(&store, &prompt, &api, &clock).await;
    let observed = observe_store(login_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn validated_login_replaces_every_targetable_unusable_entry() -> TestResult {
    for kind in [
        CredentialFailureKind::Corrupt,
        CredentialFailureKind::Invalid,
        CredentialFailureKind::UnsupportedVersion,
        CredentialFailureKind::TooLarge,
    ] {
        // Setup scripts/DI.
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
        let prompt_script = PromptScript::token_login();
        let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(
            FakeStoreState::Failure(kind),
            StoreScript::NORMAL,
            Rc::clone(&calls),
        );
        let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let saved = CredentialSnapshot::synthetic(
            TokenMaterial::Imported,
            DeviceMaterial::Prompted,
            AccountIdentity::Primary,
            clock.0,
        );
        let expected = AuthObservation::new(
            Ok(LoginSnapshot::synthetic(
                AccountIdentity::Primary,
                clock.0,
                LoginDisposition::RecoveredUnusableEntry,
            )),
            CredentialStateSnapshot::Present(saved.clone()),
            vec![
                AuthCall::CheckPromptAvailability,
                AuthCall::LoadCredential,
                AuthCall::ReadAccessToken,
                AuthCall::ReadDeviceId,
                current_account_call(TokenMaterial::Imported, DeviceMaterial::Prompted),
                AuthCall::SaveCredential(saved),
                AuthCall::LoadCredential,
            ],
        );

        // Execute once.
        let result = login_with_token(&store, &prompt, &api, &clock).await;
        let observed = observe_store(login_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn login_does_not_prompt_when_the_store_cannot_target_one_entry() -> TestResult {
    for kind in [
        CredentialFailureKind::Unavailable,
        CredentialFailureKind::Ambiguous,
        CredentialFailureKind::Platform,
        CredentialFailureKind::Internal,
    ] {
        // Setup scripts/DI.
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
        let prompt_script = PromptScript::token_login();
        let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(
            FakeStoreState::Failure(kind),
            StoreScript::NORMAL,
            Rc::clone(&calls),
        );
        let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let expected = AuthObservation::new(
            Err(LoginFailureSnapshot::synthetic(
                ApplicationFailureKind::Credential,
                LoginFailure::CredentialLoad,
            )),
            CredentialStateSnapshot::Failure(kind),
            vec![AuthCall::CheckPromptAvailability, AuthCall::LoadCredential],
        );

        // Execute once.
        let result = login_with_token(&store, &prompt, &api, &clock).await;
        let observed = observe_store(login_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn imported_token_save_and_readback_failures_report_the_complete_unknown_state() -> TestResult
{
    for behavior in [
        SaveScript::FailAfterWrite,
        SaveScript::ReadBackFailure,
        SaveScript::ReadBackMissing,
        SaveScript::StoreMismatch,
    ] {
        // Setup scripts/DI.
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
        let prompt_script = PromptScript::token_login();
        let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
        let store_script = StoreScript::with_save(behavior);

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(FakeStoreState::Missing, store_script, Rc::clone(&calls));
        let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let attempted = CredentialSnapshot::synthetic(
            TokenMaterial::Imported,
            DeviceMaterial::Prompted,
            AccountIdentity::Primary,
            clock.0,
        );
        let (credential_state, failure, readback_call) = match behavior {
            SaveScript::FailAfterWrite => (
                CredentialStateSnapshot::Present(attempted.clone()),
                StorageFailureSnapshot::Operation,
                None,
            ),
            SaveScript::ReadBackFailure => (
                CredentialStateSnapshot::Failure(CredentialFailureKind::Platform),
                StorageFailureSnapshot::Operation,
                Some(AuthCall::LoadCredential),
            ),
            SaveScript::ReadBackMissing => (
                CredentialStateSnapshot::Missing,
                StorageFailureSnapshot::MissingOrMismatch,
                Some(AuthCall::LoadCredential),
            ),
            SaveScript::StoreMismatch => (
                CredentialStateSnapshot::Present(CredentialSnapshot::synthetic(
                    TokenMaterial::Mismatched,
                    DeviceMaterial::Prompted,
                    AccountIdentity::Primary,
                    clock.0,
                )),
                StorageFailureSnapshot::MissingOrMismatch,
                Some(AuthCall::LoadCredential),
            ),
            SaveScript::Normal => (
                CredentialStateSnapshot::Present(attempted.clone()),
                StorageFailureSnapshot::MissingOrMismatch,
                Some(AuthCall::LoadCredential),
            ),
        };
        let expected_calls = vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadAccessToken,
            AuthCall::ReadDeviceId,
            current_account_call(TokenMaterial::Imported, DeviceMaterial::Prompted),
            AuthCall::SaveCredential(attempted),
        ]
        .into_iter()
        .chain(readback_call)
        .collect();
        let expected = AuthObservation::new(
            Err(LoginFailureSnapshot::synthetic(
                ApplicationFailureKind::Credential,
                LoginFailure::CredentialStorageStateUnknown(failure),
            )),
            credential_state,
            expected_calls,
        );

        // Execute once.
        let result = login_with_token(&store, &prompt, &api, &clock).await;
        let observed = observe_store(login_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}
