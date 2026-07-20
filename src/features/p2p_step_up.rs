use std::future::Future;

use thiserror::Error;

use crate::features::auth::{OtpCode, PromptAvailability, PromptError, prompt_failure_kind};
use crate::shared::{
    AccessToken, ApiFailure, ApiOperationFailure, ApplicationFailureKind, ClientRequestId,
    CredentialEnvelope, DeviceId,
};

pub(crate) trait P2pStepUpInput: PromptAvailability {
    fn read_p2p_otp(&self, prompt: &str) -> Result<OtpCode, PromptError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum P2pOtpVerification {
    Verified,
    Incorrect,
    Expired,
    Unexpected,
    TooManyIncorrectAttempts,
}

pub(crate) trait P2pStepUpApi {
    type Error: ApiFailure;

    fn issue_p2p_otp<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        session_id: &'a ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn verify_p2p_otp<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        session_id: &'a ClientRequestId,
        otp: &'a OtpCode,
    ) -> impl Future<Output = Result<P2pOtpVerification, Self::Error>> + Send + 'a;
}

#[derive(Debug, Error)]
pub enum P2pStepUpError {
    #[error("Venmo requires SMS verification; rerun in a terminal that can prompt for the code")]
    PromptRequired,
    #[error("failed to send the Venmo SMS verification code: {source}")]
    Issue {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("Venmo SMS-verification prompt failed: {source}")]
    Prompt {
        #[source]
        source: PromptError,
    },
    #[error("failed to verify the Venmo SMS code: {source}")]
    Verify {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("the Venmo SMS verification code was incorrect")]
    OtpIncorrect,
    #[error("the Venmo SMS verification code expired")]
    OtpExpired,
    #[error("Venmo did not expect an SMS verification code for this operation")]
    OtpUnexpected,
    #[error("Venmo rejected SMS verification after too many incorrect attempts")]
    OtpTooManyAttempts,
    #[error("Venmo still requires SMS verification after the code was verified")]
    StillRequired,
}

impl P2pStepUpError {
    #[must_use]
    pub(crate) const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Issue { source } | Self::Verify { source } => {
                ApplicationFailureKind::Api(source.kind())
            }
            Self::PromptRequired => ApplicationFailureKind::Usage,
            Self::Prompt { source } => prompt_failure_kind(source),
            Self::OtpIncorrect
            | Self::OtpExpired
            | Self::OtpUnexpected
            | Self::OtpTooManyAttempts
            | Self::StillRequired => {
                ApplicationFailureKind::Api(crate::shared::ApiFailureKind::Rejected)
            }
        }
    }
}

pub(crate) async fn complete<A, P>(
    api: &A,
    prompt: &P,
    credential: &CredentialEnvelope,
    session_id: &ClientRequestId,
) -> Result<(), P2pStepUpError>
where
    A: P2pStepUpApi,
    P: P2pStepUpInput,
{
    if !prompt.can_prompt() {
        return Err(P2pStepUpError::PromptRequired);
    }
    api.issue_p2p_otp(
        credential.access_token(),
        credential.device_id(),
        session_id,
    )
    .await
    .map_err(|source| P2pStepUpError::Issue {
        source: ApiOperationFailure::new(source),
    })?;
    let otp = prompt
        .read_p2p_otp("Venmo SMS verification code")
        .map_err(|source| P2pStepUpError::Prompt { source })?;
    let verification = api
        .verify_p2p_otp(
            credential.access_token(),
            credential.device_id(),
            session_id,
            &otp,
        )
        .await
        .map_err(|source| P2pStepUpError::Verify {
            source: ApiOperationFailure::new(source),
        })?;
    match verification {
        P2pOtpVerification::Verified => Ok(()),
        P2pOtpVerification::Incorrect => Err(P2pStepUpError::OtpIncorrect),
        P2pOtpVerification::Expired => Err(P2pStepUpError::OtpExpired),
        P2pOtpVerification::Unexpected => Err(P2pStepUpError::OtpUnexpected),
        P2pOtpVerification::TooManyIncorrectAttempts => Err(P2pStepUpError::OtpTooManyAttempts),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::future::{Future, ready};
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::shared::{Account, ApiFailureKind, UserId, Username};

    const SESSION_ID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Call {
        PromptAvailability,
        Issue(ClientRequestId),
        ReadOtp(String),
        Verify(ClientRequestId, String),
    }

    #[derive(Clone, Copy, Debug, thiserror::Error)]
    #[error("synthetic P2P step-up failure")]
    struct FakeApiError;

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            ApiFailureKind::Rejected
        }
    }

    struct FakeApi {
        verification: P2pOtpVerification,
        calls: RefCell<Vec<Call>>,
    }

    impl P2pStepUpApi for FakeApi {
        type Error = FakeApiError;

        fn issue_p2p_otp<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            session_id: &'a ClientRequestId,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
            self.calls.borrow_mut().push(Call::Issue(*session_id));
            ready(Ok(()))
        }

        fn verify_p2p_otp<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            session_id: &'a ClientRequestId,
            otp: &'a OtpCode,
        ) -> impl Future<Output = Result<P2pOtpVerification, Self::Error>> + Send + 'a {
            self.calls
                .borrow_mut()
                .push(Call::Verify(*session_id, otp.expose().to_owned()));
            ready(Ok(self.verification))
        }
    }

    struct FakePrompt<'a> {
        interactive: bool,
        calls: &'a RefCell<Vec<Call>>,
    }

    impl PromptAvailability for FakePrompt<'_> {
        fn can_prompt(&self) -> bool {
            self.calls.borrow_mut().push(Call::PromptAvailability);
            self.interactive
        }
    }

    impl P2pStepUpInput for FakePrompt<'_> {
        fn read_p2p_otp(&self, prompt: &str) -> Result<OtpCode, PromptError> {
            self.calls
                .borrow_mut()
                .push(Call::ReadOtp(prompt.to_owned()));
            OtpCode::parse_owned("123456".to_owned()).map_err(|_| PromptError::Interaction {
                source: std::io::Error::other("synthetic OTP was invalid"),
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn completion_uses_one_shared_exact_interactive_sequence()
    -> Result<(), Box<dyn std::error::Error>> {
        let api = FakeApi {
            verification: P2pOtpVerification::Verified,
            calls: RefCell::new(Vec::new()),
        };
        let prompt = FakePrompt {
            interactive: true,
            calls: &api.calls,
        };
        let session_id = ClientRequestId::from_str(SESSION_ID)?;

        complete(&api, &prompt, &credential()?, &session_id).await?;

        assert_eq!(
            api.calls.into_inner(),
            vec![
                Call::PromptAvailability,
                Call::Issue(session_id),
                Call::ReadOtp("Venmo SMS verification code".to_owned()),
                Call::Verify(session_id, "123456".to_owned()),
            ]
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn completion_requires_an_interactive_prompt_before_sending_sms()
    -> Result<(), Box<dyn std::error::Error>> {
        let api = FakeApi {
            verification: P2pOtpVerification::Verified,
            calls: RefCell::new(Vec::new()),
        };
        let prompt = FakePrompt {
            interactive: false,
            calls: &api.calls,
        };

        let result = complete(
            &api,
            &prompt,
            &credential()?,
            &ClientRequestId::from_str(SESSION_ID)?,
        )
        .await;

        assert!(matches!(result, Err(P2pStepUpError::PromptRequired)));
        assert_eq!(api.calls.into_inner(), vec![Call::PromptAvailability]);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn completion_preserves_each_verification_rejection()
    -> Result<(), Box<dyn std::error::Error>> {
        for (verification, expected) in [
            (P2pOtpVerification::Incorrect, "incorrect"),
            (P2pOtpVerification::Expired, "expired"),
            (P2pOtpVerification::Unexpected, "did not expect"),
            (P2pOtpVerification::TooManyIncorrectAttempts, "too many"),
        ] {
            let api = FakeApi {
                verification,
                calls: RefCell::new(Vec::new()),
            };
            let prompt = FakePrompt {
                interactive: true,
                calls: &api.calls,
            };
            let result = complete(
                &api,
                &prompt,
                &credential()?,
                &ClientRequestId::from_str(SESSION_ID)?,
            )
            .await;
            let Err(error) = result else {
                return Err(std::io::Error::other(
                    "the synthetic verification was accepted unexpectedly",
                )
                .into());
            };
            assert!(error.to_string().contains(expected));
        }
        Ok(())
    }

    fn credential() -> Result<CredentialEnvelope, Box<dyn std::error::Error>> {
        let account = Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        );
        Ok(CredentialEnvelope::new(
            AccessToken::from_str("synthetic-token")?,
            DeviceId::from_str("synthetic-device")?,
            account.user_id().clone(),
            account.username().clone(),
            account.display_name().map(str::to_owned),
            OffsetDateTime::UNIX_EPOCH,
        ))
    }
}
