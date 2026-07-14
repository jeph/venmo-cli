use super::*;

#[tokio::test(flavor = "current_thread")]
async fn direct_password_login_preserves_the_already_trusted_device() -> TestResult {
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
    let saved = CredentialSnapshot::synthetic(
        TokenMaterial::Issued,
        DeviceMaterial::Prompted,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(PasswordLoginSnapshot::synthetic(
            LoginSnapshot::synthetic(AccountIdentity::Primary, clock.0, LoginDisposition::Created),
            DeviceTrustSnapshot::NotNeeded,
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            AuthCall::ReadDeviceId,
            begin_password_call(DeviceMaterial::Prompted),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
        ],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_trusted_device_stops_after_credentials_but_before_api_or_storage() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript {
        device: DeviceInputScript::Invalid,
        ..PromptScript::password_login()
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
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            AuthCall::ReadDeviceId,
        ],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn password_login_completes_the_sms_otp_flow() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
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
    let saved = CredentialSnapshot::synthetic(
        TokenMaterial::Issued,
        DeviceMaterial::Prompted,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Ok(PasswordLoginSnapshot::synthetic(
            LoginSnapshot::synthetic(AccountIdentity::Primary, clock.0, LoginDisposition::Created),
            DeviceTrustSnapshot::Trusted,
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            AuthCall::ReadDeviceId,
            begin_password_call(DeviceMaterial::Prompted),
            request_otp_call(DeviceMaterial::Prompted),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Prompted),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
            AuthCall::SaveCredential(saved),
            AuthCall::LoadCredential,
            trust_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
        ],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn password_login_blocks_an_existing_valid_credential_before_prompts() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
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
            ApplicationFailureKind::Credential,
            LoginFailure::CredentialAlreadyStored,
        )),
        FakeStoreState::Present(initial_credential).snapshot(),
        vec![AuthCall::CheckPromptAvailability, AuthCall::LoadCredential],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn trust_failure_keeps_the_verified_credential_and_preserves_failure_kinds() -> TestResult {
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
        let prompt_script = PromptScript::password_login();
        let api_script = ApiScript {
            password_start: PasswordStartScript::OtpRequired,
            trust: Err(kind),
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
        let saved = CredentialSnapshot::synthetic(
            TokenMaterial::Issued,
            DeviceMaterial::Prompted,
            AccountIdentity::Primary,
            clock.0,
        );
        let expected = AuthObservation::new(
            Ok(PasswordLoginSnapshot::synthetic(
                LoginSnapshot::synthetic(
                    AccountIdentity::Primary,
                    clock.0,
                    LoginDisposition::Created,
                ),
                DeviceTrustSnapshot::Failed(kind),
            )),
            CredentialStateSnapshot::Present(saved.clone()),
            vec![
                AuthCall::CheckPromptAvailability,
                AuthCall::LoadCredential,
                AuthCall::ReadLoginIdentifier,
                AuthCall::ReadAccountPassword,
                AuthCall::ReadDeviceId,
                begin_password_call(DeviceMaterial::Prompted),
                request_otp_call(DeviceMaterial::Prompted),
                AuthCall::ReadOtpCode,
                complete_otp_call(DeviceMaterial::Prompted),
                current_account_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
                AuthCall::SaveCredential(saved),
                AuthCall::LoadCredential,
                trust_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
            ],
        );

        // Execute once.
        let result = login_with_password(&store, &prompt, &api, &clock).await;
        let observed = observe_store(password_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn issued_token_validation_failure_is_not_stored_or_trusted() -> TestResult {
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
        let prompt_script = PromptScript::password_login();
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
                LoginFailure::IssuedTokenValidation(kind),
            )),
            CredentialStateSnapshot::Missing,
            vec![
                AuthCall::CheckPromptAvailability,
                AuthCall::LoadCredential,
                AuthCall::ReadLoginIdentifier,
                AuthCall::ReadAccountPassword,
                AuthCall::ReadDeviceId,
                begin_password_call(DeviceMaterial::Prompted),
                current_account_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
            ],
        );

        // Execute once.
        let result = login_with_password(&store, &prompt, &api, &clock).await;
        let observed = observe_store(password_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn issued_token_storage_failure_never_attempts_device_trust() -> TestResult {
    // Setup scripts/DI.
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        ..ApiScript::successful(test_account(AccountIdentity::Primary)?)
    };
    let store_script = StoreScript::with_save(SaveScript::FailAfterWrite);

    // Immutable initial state.
    let calls = transcript();
    let store = FakeStore::new(FakeStoreState::Missing, store_script, Rc::clone(&calls));
    let prompt = FakePrompt::new(prompt_script, Rc::clone(&calls));
    let api = FakeApi::new(api_script, Rc::clone(&calls));

    // Complete expected final state/outcome.
    let saved = CredentialSnapshot::synthetic(
        TokenMaterial::Issued,
        DeviceMaterial::Prompted,
        AccountIdentity::Primary,
        clock.0,
    );
    let expected = AuthObservation::new(
        Err(LoginFailureSnapshot::synthetic(
            ApplicationFailureKind::Credential,
            LoginFailure::IssuedCredentialStorageStateUnknown(StorageFailureSnapshot::Operation),
        )),
        CredentialStateSnapshot::Present(saved.clone()),
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            AuthCall::ReadDeviceId,
            begin_password_call(DeviceMaterial::Prompted),
            request_otp_call(DeviceMaterial::Prompted),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Prompted),
            current_account_call(TokenMaterial::Issued, DeviceMaterial::Prompted),
            AuthCall::SaveCredential(saved),
        ],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn password_authentication_failure_is_never_retried_or_stored() -> TestResult {
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
        let prompt_script = PromptScript::password_login();
        let api_script = ApiScript {
            password_start: PasswordStartScript::Failure(kind),
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
                LoginFailure::PasswordAuthentication(kind),
            )),
            CredentialStateSnapshot::Missing,
            vec![
                AuthCall::CheckPromptAvailability,
                AuthCall::LoadCredential,
                AuthCall::ReadLoginIdentifier,
                AuthCall::ReadAccountPassword,
                AuthCall::ReadDeviceId,
                begin_password_call(DeviceMaterial::Prompted),
            ],
        );

        // Execute once.
        let result = login_with_password(&store, &prompt, &api, &clock).await;
        let observed = observe_store(password_outcome(&result), &store, &calls);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn otp_request_failure_is_never_retried_or_stored() -> TestResult {
    // Setup scripts/DI.
    let kind = ApiFailureKind::Rejected;
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        otp_request: Err(kind),
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
            LoginFailure::OtpRequest(kind),
        )),
        CredentialStateSnapshot::Missing,
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            AuthCall::ReadDeviceId,
            begin_password_call(DeviceMaterial::Prompted),
            request_otp_call(DeviceMaterial::Prompted),
        ],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn otp_completion_failure_is_never_retried_stored_or_trusted() -> TestResult {
    // Setup scripts/DI.
    let kind = ApiFailureKind::Timeout;
    let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);
    let prompt_script = PromptScript::password_login();
    let api_script = ApiScript {
        password_start: PasswordStartScript::OtpRequired,
        otp_completion: OtpCompletionScript::Failure(kind),
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
            LoginFailure::OtpCompletion(kind),
        )),
        CredentialStateSnapshot::Missing,
        vec![
            AuthCall::CheckPromptAvailability,
            AuthCall::LoadCredential,
            AuthCall::ReadLoginIdentifier,
            AuthCall::ReadAccountPassword,
            AuthCall::ReadDeviceId,
            begin_password_call(DeviceMaterial::Prompted),
            request_otp_call(DeviceMaterial::Prompted),
            AuthCall::ReadOtpCode,
            complete_otp_call(DeviceMaterial::Prompted),
        ],
    );

    // Execute once.
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn noninteractive_password_login_touches_no_keyring_prompts_or_api() -> TestResult {
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
    let result = login_with_password(&store, &prompt, &api, &clock).await;
    let observed = observe_store(password_outcome(&result), &store, &calls);

    assert_eq!(observed, expected);
    Ok(())
}
