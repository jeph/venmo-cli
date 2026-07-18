use std::collections::HashSet;

use super::{
    ApiAvailability, BuildInfo, DoctorApi, DoctorCheck, DoctorCheckStatus, DoctorReport,
    RequiredShape, ShapeProbeOutcome,
};
use crate::shared::{ApiFailure, ApiFailureKind, CredentialReader, LoadedCredential};

pub async fn diagnose<S, A>(
    store: &S,
    api: ApiAvailability<'_, A>,
    build_info: BuildInfo,
) -> DoctorReport
where
    S: CredentialReader,
    A: DoctorApi,
{
    let build = build_check(build_info);
    let credential = credential_diagnostic(store);
    let connectivity = connectivity_check(&api).await;
    let authentication = authenticate_for_doctor(&api, credential.loaded.as_ref()).await;
    let required_shapes =
        required_shapes_diagnostic(&api, credential.loaded.as_ref(), authentication).await;

    DoctorReport::new(vec![
        build,
        credential.store_check,
        credential.presence_check,
        connectivity,
        authentication_check(authentication),
        required_shapes,
    ])
}

fn build_check(build_info: BuildInfo) -> DoctorCheck {
    DoctorCheck::new(
        "Build and platform",
        DoctorCheckStatus::Pass,
        format!(
            "venmo {} on {}/{}",
            build_info.version(),
            build_info.os(),
            build_info.arch()
        ),
        None,
    )
}

struct CredentialDiagnostic {
    store_check: DoctorCheck,
    presence_check: DoctorCheck,
    loaded: Option<LoadedCredential>,
}

fn credential_diagnostic<S: CredentialReader>(store: &S) -> CredentialDiagnostic {
    match store.read_credential() {
        Ok(Some(loaded)) => CredentialDiagnostic {
            store_check: DoctorCheck::new(
                "Credential store",
                DoctorCheckStatus::Pass,
                "the native credential store is readable",
                None,
            ),
            presence_check: DoctorCheck::new(
                "Credential presence and schema",
                DoctorCheckStatus::Pass,
                "a supported credential envelope is present",
                None,
            ),
            loaded: Some(loaded),
        },
        Ok(None) => CredentialDiagnostic {
            store_check: DoctorCheck::new(
                "Credential store",
                DoctorCheckStatus::Pass,
                "the native credential store is readable",
                None,
            ),
            presence_check: DoctorCheck::new(
                "Credential presence and schema",
                DoctorCheckStatus::Fail,
                "no credential is stored",
                Some("Run `venmo auth login` from an interactive terminal."),
            ),
            loaded: None,
        },
        Err(_) => CredentialDiagnostic {
            store_check: DoctorCheck::new(
                "Credential store",
                DoctorCheckStatus::Fail,
                "the native credential store could not be read",
                Some(
                    "Unlock or configure the OS credential store; Linux requires a user-session Secret Service.",
                ),
            ),
            presence_check: DoctorCheck::new(
                "Credential presence and schema",
                DoctorCheckStatus::Skipped,
                "credential inspection depends on a readable credential store",
                None,
            ),
            loaded: None,
        },
    }
}

async fn connectivity_check<A: DoctorApi>(api: &ApiAvailability<'_, A>) -> DoctorCheck {
    match api {
        ApiAvailability::Ready(api) => match api.connectivity().await {
            Ok(()) => DoctorCheck::new(
                "Connectivity and TLS",
                DoctorCheckStatus::Pass,
                "the fixed Venmo API origin returned an HTTPS response",
                None,
            ),
            Err(error) => DoctorCheck::new(
                "Connectivity and TLS",
                DoctorCheckStatus::Fail,
                api_failure_detail(error.kind()),
                Some("Check DNS, HTTPS connectivity, system time, and TLS trust settings."),
            ),
        },
        ApiAvailability::InitializationFailed => DoctorCheck::new(
            "Connectivity and TLS",
            DoctorCheckStatus::Fail,
            "the HTTPS client could not be initialized",
            Some("Review the local TLS/runtime installation and retry."),
        ),
    }
}

fn authentication_check(authentication: AuthenticationDiagnostic) -> DoctorCheck {
    match authentication {
        AuthenticationDiagnostic::Passed => DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Pass,
            "the stored credential matches the authenticated account",
            None,
        ),
        AuthenticationDiagnostic::Failed(kind) => DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Fail,
            api_failure_detail(kind),
            Some("Run `venmo auth login` to perform the complete authentication flow again."),
        ),
        AuthenticationDiagnostic::AccountMismatch => DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Fail,
            "the live account does not match the stored account identity",
            Some("Run `venmo auth logout`, then authenticate the intended account."),
        ),
        AuthenticationDiagnostic::Skipped => DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Skipped,
            "authentication depends on an available API client and usable credential",
            None,
        ),
    }
}

async fn required_shapes_diagnostic<A: DoctorApi>(
    api: &ApiAvailability<'_, A>,
    credential: Option<&LoadedCredential>,
    authentication: AuthenticationDiagnostic,
) -> DoctorCheck {
    match (api, credential, authentication) {
        (ApiAvailability::Ready(api), Some(credential), AuthenticationDiagnostic::Passed) => {
            let outcomes = api
                .required_shapes(
                    credential.envelope.access_token(),
                    credential.envelope.device_id(),
                    credential.envelope.user_id(),
                )
                .await;
            required_shapes_check(&outcomes)
        }
        _ => DoctorCheck::new(
            "Required private-API shapes",
            DoctorCheckStatus::Skipped,
            "shape checks depend on successful current-account authentication",
            None,
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthenticationDiagnostic {
    Passed,
    Failed(ApiFailureKind),
    AccountMismatch,
    Skipped,
}

async fn authenticate_for_doctor<A: DoctorApi>(
    api: &ApiAvailability<'_, A>,
    credential: Option<&LoadedCredential>,
) -> AuthenticationDiagnostic {
    let (ApiAvailability::Ready(api), Some(credential)) = (api, credential) else {
        return AuthenticationDiagnostic::Skipped;
    };
    match api
        .diagnostic_current_account(
            credential.envelope.access_token(),
            credential.envelope.device_id(),
        )
        .await
    {
        Ok(account) if account.user_id() == credential.envelope.user_id() => {
            AuthenticationDiagnostic::Passed
        }
        Ok(_) => AuthenticationDiagnostic::AccountMismatch,
        Err(error) => AuthenticationDiagnostic::Failed(error.kind()),
    }
}

fn required_shapes_check(outcomes: &[ShapeProbeOutcome]) -> DoctorCheck {
    let mut seen = HashSet::with_capacity(outcomes.len());
    let mut failures = Vec::new();
    for outcome in outcomes {
        if !seen.insert(outcome.shape()) {
            failures.push("duplicate diagnostic result".to_owned());
        } else if let Some(kind) = outcome.failure() {
            failures.push(format!(
                "{} ({})",
                outcome.shape().label(),
                api_failure_detail(kind)
            ));
        }
    }
    for required in RequiredShape::ALL {
        if !seen.contains(&required) {
            failures.push(format!("{} (missing diagnostic result)", required.label()));
        }
    }
    if failures.is_empty() {
        DoctorCheck::new(
            "Required private-API shapes",
            DoctorCheckStatus::Pass,
            "all required read-only response shapes were recognized",
            None,
        )
    } else {
        DoctorCheck::new(
            "Required private-API shapes",
            DoctorCheckStatus::Fail,
            failures.join(", "),
            Some("The private Venmo API may have changed; update the CLI before relying on it."),
        )
    }
}

fn api_failure_detail(kind: ApiFailureKind) -> &'static str {
    match kind {
        ApiFailureKind::Network => "the network request failed",
        ApiFailureKind::Timeout => "the network request timed out",
        ApiFailureKind::Authentication => "the stored Venmo session was rejected",
        ApiFailureKind::Rejected => "Venmo rejected the request",
        ApiFailureKind::Contract => "the private API response shape was not recognized",
        ApiFailureKind::AmbiguousWrite => "an unexpected ambiguous-write classification occurred",
        ApiFailureKind::Internal => "an internal API-client failure occurred",
    }
}
