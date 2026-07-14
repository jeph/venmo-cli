use thiserror::Error;

use super::{RequestAction, RequestDirection, RequestId, RequestRecord};
use crate::shared::{ApplicationFailureKind, Note};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RequestMutationValidationError {
    #[error("the request is not an incoming request addressed to the active account")]
    NotIncoming,

    #[error(
        "the request is not in the exact pending state required for this operation; refresh `venmo requests list`"
    )]
    NotPending,

    #[error("the request does not have a verified private audience")]
    UnverifiedAudience,

    #[error("the request omitted a complete, valid note")]
    InvalidNote,

    #[error("the request omitted a supported audience")]
    UnsupportedAudience,

    #[error("the request omitted a complete requester identity")]
    IncompleteRequester,

    #[error("the request omitted its creation timestamp")]
    MissingCreationTime,

    #[error("the record is not an incoming payment request")]
    NotChargeRequest,

    #[error("the authoritative request lookup returned a different request ID")]
    MismatchedId,
}

impl RequestMutationValidationError {
    #[must_use]
    pub const fn failure_kind(self) -> ApplicationFailureKind {
        match self {
            Self::NotIncoming
            | Self::NotPending
            | Self::UnverifiedAudience
            | Self::NotChargeRequest => ApplicationFailureKind::Usage,
            Self::InvalidNote
            | Self::UnsupportedAudience
            | Self::IncompleteRequester
            | Self::MissingCreationTime
            | Self::MismatchedId => ApplicationFailureKind::ApiContract,
        }
    }
}

pub fn validate_complete_request(
    request: &RequestRecord,
) -> Result<(), RequestMutationValidationError> {
    if request.action() != RequestAction::Charge {
        return Err(RequestMutationValidationError::NotChargeRequest);
    }
    if request
        .note()
        .and_then(|note| note.parse::<Note>().ok())
        .is_none()
    {
        return Err(RequestMutationValidationError::InvalidNote);
    }
    if !matches!(request.audience(), Some("private" | "friends" | "public")) {
        return Err(RequestMutationValidationError::UnsupportedAudience);
    }
    if request.counterparty().username().is_none() {
        return Err(RequestMutationValidationError::IncompleteRequester);
    }
    if request.created_at().is_none() {
        return Err(RequestMutationValidationError::MissingCreationTime);
    }
    Ok(())
}

pub fn validate_request_id(
    request: &RequestRecord,
    expected: &RequestId,
) -> Result<(), RequestMutationValidationError> {
    if request.id() != expected {
        return Err(RequestMutationValidationError::MismatchedId);
    }
    Ok(())
}

pub fn validate_incoming_pending(
    request: &RequestRecord,
) -> Result<(), RequestMutationValidationError> {
    if request.direction() != RequestDirection::Incoming {
        return Err(RequestMutationValidationError::NotIncoming);
    }
    if request.status().as_str() != "pending" {
        return Err(RequestMutationValidationError::NotPending);
    }
    Ok(())
}

pub fn validate_private_audience(
    request: &RequestRecord,
) -> Result<(), RequestMutationValidationError> {
    if request.audience() != Some("private") {
        return Err(RequestMutationValidationError::UnverifiedAudience);
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
