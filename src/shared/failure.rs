use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiFailureKind {
    Network,
    Timeout,
    Authentication,
    Rejected,
    Contract,
    AmbiguousWrite,
    Internal,
}

pub trait ApiFailure: Error + Send + Sync + 'static {
    fn kind(&self) -> ApiFailureKind;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationFailureKind {
    Credential,
    Usage,
    Cancelled,
    Api(ApiFailureKind),
    ApiContract,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadFailureKind {
    Credential,
    Api(ApiFailureKind),
    ResponseContract,
    Internal,
}

type BoxError = Box<dyn Error + Send + Sync>;

pub struct OperationFailure {
    source: BoxError,
}

impl OperationFailure {
    pub(crate) fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(source),
        }
    }
}

impl fmt::Debug for OperationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OperationFailure")
            .field("source", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl Error for OperationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

pub struct ApiOperationFailure {
    kind: ApiFailureKind,
    source: OperationFailure,
}

impl ApiOperationFailure {
    pub(crate) fn new(source: impl ApiFailure) -> Self {
        Self {
            kind: source.kind(),
            source: OperationFailure::new(source),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ApiFailureKind {
        self.kind
    }
}

impl fmt::Debug for ApiOperationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiOperationFailure")
            .field("kind", &self.kind)
            .field("source", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for ApiOperationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl Error for ApiOperationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
