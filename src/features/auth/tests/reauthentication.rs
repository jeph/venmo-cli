use super::*;

#[tokio::test(flavor = "current_thread")]
async fn direct_reauthentication_reuses_the_exact_stored_device_without_trust() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(9));
    let prompt_script = PromptScript::password_login();
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
        TokenMaterial::Issued,
        DeviceMaterial::Stored,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(PasswordLoginSnapshot::synthetic(
            LoginSnapshot::synthetic(
                AccountIdentity::Primary,
                clock.0,
                LoginDisposition::ReplacedForSameAccount,
            ),
            DeviceTrustSnapshot::NotNeeded,
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Stored),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_completes_otp_with_the_stored_device_then_trusts_it() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
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
        TokenMaterial::Issued,
        DeviceMaterial::Stored,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(PasswordLoginSnapshot::synthetic(
            LoginSnapshot::synthetic(
                AccountIdentity::Primary,
                clock.0,
                LoginDisposition::ReplacedForSameAccount,
            ),
            DeviceTrustSnapshot::Trusted,
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            request_otp_call(DeviceMaterial::Stored),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Stored),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Stored),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
            trust_call(TokenMaterial::Issued, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_otp_trust_failure_keeps_the_verified_replacement() -> TestResult {
    // Setup scripts/DI.
    let kind = ApiFailureKind::Rejected;
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        trust: Err(kind),
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
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
        TokenMaterial::Issued,
        DeviceMaterial::Stored,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(PasswordLoginSnapshot::synthetic(
            LoginSnapshot::synthetic(
                AccountIdentity::Primary,
                clock.0,
                LoginDisposition::ReplacedForSameAccount,
            ),
            DeviceTrustSnapshot::Failed(kind),
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            request_otp_call(DeviceMaterial::Stored),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Stored),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Stored),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
            trust_call(TokenMaterial::Issued, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_requires_a_stored_credential() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
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
            ApplicationFailureKind::Credential,
            LoginFailure::ReauthenticationCredentialMissing,
        )),
        CredentialStateSnapshot::Missing,
        vec![AuthCall::CheckPromptAvailability, AuthCall::LoadCredential],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_rejects_every_unreadable_credential_before_prompts() -> TestResult {
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
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
        let prompt_script = PromptScript::password_login();
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
        let result = reauthenticate(&store, &prompt, &api, &clock).await;
        let observed = observe_store(password_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_reauthentication_identifier_preserves_the_old_credential() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript {
        identifier: IdentifierInputScript::Invalid,
        ..PromptScript::password_login()
    };
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
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
            ApplicationFailureKind::Usage,
            LoginFailure::Prompt(PromptFailureSnapshot::InvalidLoginIdentifier),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_reauthentication_password_preserves_the_old_credential() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript {
        password: PasswordInputScript::Invalid,
        ..PromptScript::password_login()
    };
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
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
            ApplicationFailureKind::Usage,
            LoginFailure::Prompt(PromptFailureSnapshot::InvalidAccountPassword),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_reauthentication_otp_preserves_the_old_credential() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript {
        otp: OtpInputScript::Invalid,
        ..PromptScript::password_login()
    };
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
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
            ApplicationFailureKind::Usage,
            LoginFailure::Prompt(PromptFailureSnapshot::InvalidOtpCode),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            request_otp_call(DeviceMaterial::Stored),
            AuthCall::ReadOtpCode,
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_account_mismatch_never_replaces_or_discloses_the_old_credential()
-> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
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
            LoginFailure::IssuedTokenDifferentAccount,
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);
    let rendered = match &result {
        Ok(_) => String::new(),
        Err(error) => error.to_string(),
    };

    assert_eq!(observed, expected);
    assert_auth_material_not_disclosed(&rendered);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_validation_failure_never_replaces_the_old_credential() -> TestResult {
    // Setup scripts/DI.
    let kind = ApiFailureKind::Rejected;
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        current_account: Err(kind),
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
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
            ApplicationFailureKind::Api(kind),
            LoginFailure::IssuedTokenValidation(kind),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_password_failure_is_never_retried_or_stored() -> TestResult {
    // Setup scripts/DI.
    let kind = ApiFailureKind::Rejected;
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::Failure(kind),
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
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
            ApplicationFailureKind::Api(kind),
            LoginFailure::PasswordAuthentication(kind),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_otp_completion_failure_preserves_the_old_credential() -> TestResult {
    // Setup scripts/DI.
    let kind = ApiFailureKind::Timeout;
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        otp_completion: OtpCompletionScript::Failure(kind),
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
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
            ApplicationFailureKind::Api(kind),
            LoginFailure::OtpCompletion(kind),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            request_otp_call(DeviceMaterial::Stored),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Stored),
        ],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reauthentication_save_and_readback_failures_report_the_complete_unknown_state()
-> TestResult {
    for behavior in [
        SaveScript::FailAfterWrite,
        SaveScript::ReadBackFailure,
        SaveScript::ReadBackMissing,
        SaveScript::StoreMismatch,
    ] {
        // Setup scripts/DI.
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
        let prompt_script = PromptScript::password_login();
        let api_script = ApiScript {
            password_start: PasswordStartScript::OtpRequired,
            ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
        };
        let store_script = StoreScript::with_save(behavior);
        let initial_credential = CredentialFixture::stored(AccountIdentity::Primary);

        // Immutable initial state.
        let calls = transcript();
        let store = FakeStore::new(
            FakeStoreState::Present(initial_credential),
            store_script,
            Rc::clone(&calls),
        );
        let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
        let api = FakeApi::new(api_script, Rc::clone(&calls));

        // Complete expected final state/outcome.
        let attempted = CredentialSnapshot::synthetic(
            TokenMaterial::Issued,
            DeviceMaterial::Stored,
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
                    DeviceMaterial::Stored,
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
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            begin_password_call(DeviceMaterial::Stored),
            request_otp_call(DeviceMaterial::Stored),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Stored),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Stored),
            AuthCall::SaveCredential(attempted),
        ]
        .into_iter()
        .chain(readback_call)
        .collect();
        let expected = AuthObservation::new(
            Err(LoginFailureSnapshot::synthetic(
                ApplicationFailureKind::Credential,
                LoginFailure::IssuedCredentialStorageStateUnknown(failure),
            )),
            credential_state,
            expected_calls,
        );

        // Execute once.
        let result = reauthenticate(&store, &prompt, &api, &clock).await;
        let observed = observe_store(password_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn noninteractive_reauthentication_touches_no_store_prompts_or_api() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::noninteractive();
    let api_script = ApiScript::successful(test_account(AccountIdentity::Primary)?);
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
            ApplicationFailureKind::Usage,
            LoginFailure::Prompt(PromptFailureSnapshot::NotInteractive),
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![AuthCall::CheckPromptAvailability],
    );

    // Execute once.
    let result = reauthenticate(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}
