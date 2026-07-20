use thiserror::Error;

use super::{RequestAction, RequestDirection, RequestId, RequestRecord};
use crate::shared::{ApplicationFailureKind, Note, Visibility};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RequestMutationValidationError {
    #[error("the request is not an incoming request addressed to the active account")]
    NotIncoming,

    #[error("the request is not an outgoing request made by the active account")]
    NotOutgoing,

    #[error(
        "the request is not in the exact pending state required for this operation; refresh `venmo requests list`"
    )]
    NotPending,

    #[error(
        "the request is not in a pending or held state that can be cancelled; refresh `venmo requests list`"
    )]
    NotOpen,

    #[error("the request does not have a verified private audience")]
    UnverifiedAudience,

    #[error("the request omitted a complete, valid note")]
    InvalidNote,

    #[error("the request omitted a supported audience")]
    UnsupportedAudience,

    #[error("the request omitted a complete counterparty identity")]
    IncompleteRequester,

    #[error("the request omitted its creation timestamp")]
    MissingCreationTime,

    #[error("the record is not a payment request")]
    NotChargeRequest,

    #[error("the authoritative request lookup returned a different request ID")]
    MismatchedId,
}

impl RequestMutationValidationError {
    #[must_use]
    pub const fn failure_kind(self) -> ApplicationFailureKind {
        match self {
            Self::NotIncoming
            | Self::NotOutgoing
            | Self::NotPending
            | Self::NotOpen
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
    if request
        .audience()
        .and_then(|audience| audience.parse::<Visibility>().ok())
        .is_none()
    {
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

pub fn validate_outgoing_open(
    request: &RequestRecord,
) -> Result<(), RequestMutationValidationError> {
    if request.direction() != RequestDirection::Outgoing {
        return Err(RequestMutationValidationError::NotOutgoing);
    }
    if !request.status().is_pending_or_held() {
        return Err(RequestMutationValidationError::NotOpen);
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
