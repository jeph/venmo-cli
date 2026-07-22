use std::io;

use crate::features::auth::{
    AuthStatus, DeviceTrustOutcome, LocalDeletionOutcome, LoginDisposition, LogoutReport,
    PasswordLoginReport,
};
use crate::shared::CredentialFormat;

use super::Response;
use super::shared;

pub(crate) fn auth_status(status: &AuthStatus) -> io::Result<Response<'_, AuthStatus>> {
    let credential_format = match status.credential_format() {
        CredentialFormat::Version1 => "version_1",
        CredentialFormat::LegacyTypeScript => "legacy_typescript",
    };
    Ok(Response::new(
        status,
        serde_json::json!({
            "account": shared::account(status.account()),
            "saved_at": shared::timestamp(status.saved_at())?,
            "credential_format": credential_format,
            "credential_backend": status.credential_backend().as_str(),
        }),
    ))
}

pub(crate) fn password_login(
    report: &PasswordLoginReport,
) -> io::Result<Response<'_, PasswordLoginReport>> {
    let (disposition, previous_remote_token_revoked) = match report.login().disposition() {
        LoginDisposition::Created => ("created", None),
        LoginDisposition::ReplacedExistingCredential => {
            ("replaced_existing_credential", Some(false))
        }
        LoginDisposition::RecoveredUnusableEntry => ("recovered_unusable_entry", None),
    };
    let device_trust = match report.device_trust() {
        DeviceTrustOutcome::NotNeeded => serde_json::json!({
            "status": "not_needed",
            "message": null,
        }),
        DeviceTrustOutcome::Trusted => serde_json::json!({
            "status": "trusted",
            "message": null,
        }),
        DeviceTrustOutcome::Failed(source) => serde_json::json!({
            "status": "failed",
            "message": source.to_string(),
        }),
    };
    Ok(Response::new(
        report,
        serde_json::json!({
            "credential_stored": true,
            "account": shared::account(report.login().account()),
            "saved_at": shared::timestamp(report.login().saved_at())?,
            "disposition": disposition,
            "device_trust": device_trust,
            "previous_remote_token_revoked": previous_remote_token_revoked,
        }),
    ))
}

pub(crate) fn logout(report: &LogoutReport) -> Response<'_, LogoutReport> {
    let (status, message) = match report.local() {
        LocalDeletionOutcome::Deleted => ("deleted", None),
        LocalDeletionOutcome::Missing => ("missing", None),
        LocalDeletionOutcome::Failed(source) => ("failed", Some(source.to_string())),
    };
    Response::new(
        report,
        serde_json::json!({
            "local_credential": {
                "status": status,
                "message": message,
            },
            "remote_token_revoked": false,
        }),
    )
}
