mod account;
mod client;
pub(crate) mod continuation;
mod credential;
mod credentials;
mod decimal;
mod failure;
mod first_seen;
mod money;
mod note;
pub(crate) mod opaque_id;
mod pagination;
mod user_id;
mod username;
mod visibility;

pub use account::Account;
pub use client::ClientRequestId;
pub(crate) use client::{ClientRequestIdGenerator, Clock};
pub use continuation::{ContinuationTokenParseError, MAX_CONTINUATION_TOKEN_BYTES};
pub(crate) use credential::{
    AccessToken, AccessTokenParseError, CredentialEnvelope, DeviceId, DeviceIdParseError,
};
pub(crate) use credentials::{
    CredentialAccessError, CredentialBackend, CredentialCapability, CredentialDeleteOutcome,
    CredentialDeleter, CredentialFailureKind, CredentialFormat, CredentialReader,
    CredentialStoreFailure, CredentialWriter, LoadedCredential, require_credential,
};
pub(crate) use decimal::{DecimalMagnitudeError, parse_decimal_magnitude};
pub(crate) use failure::{
    ApiFailure, ApiFailureKind, ApiOperationFailure, ApplicationFailureKind, OperationFailure,
    ReadFailureKind,
};
pub(crate) use first_seen::{FirstSeen, FirstSeenResult};
pub use money::{Money, MoneyParseError};
pub use note::{Note, NoteParseError};
pub use opaque_id::OpaqueIdParseError;
pub use pagination::{
    DEFAULT_LIST_LIMIT, Limit, LimitParseError, MAX_LIST_LIMIT, Offset, OffsetParseError,
};
pub use user_id::{UserId, UserIdParseError};
pub use username::{Username, UsernameParseError};
pub use visibility::{Visibility, VisibilityParseError};

#[cfg(test)]
pub(crate) mod test_support;
