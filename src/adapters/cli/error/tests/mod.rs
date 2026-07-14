use std::error::Error;
use std::fmt;
use std::io;

use crate::shared::{ApiFailureKind, ApiOperationFailure, OperationFailure};

mod auth;
mod financial;
mod general;
mod nested;

pub(super) const SENSITIVE_SOURCE_MARKER: &str = "sensitive-source-detail";

pub(super) struct FakeApiError {
    kind: ApiFailureKind,
    sensitive_detail: &'static str,
}

impl FakeApiError {
    const fn new(kind: ApiFailureKind) -> Self {
        Self {
            kind,
            sensitive_detail: SENSITIVE_SOURCE_MARKER,
        }
    }
}

impl fmt::Debug for FakeApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FakeApiError")
            .field("kind", &self.kind)
            .field("sensitive_detail", &self.sensitive_detail)
            .finish()
    }
}

impl fmt::Display for FakeApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("synthetic API failure")
    }
}

impl Error for FakeApiError {}

impl crate::shared::ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.kind
    }
}

pub(super) fn operation_failure() -> OperationFailure {
    OperationFailure::new(io::Error::other("synthetic operation failure"))
}

pub(super) fn api_operation_failure(kind: ApiFailureKind) -> ApiOperationFailure {
    ApiOperationFailure::new(FakeApiError::new(kind))
}
