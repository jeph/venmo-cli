use thiserror::Error;

use super::{FriendsApi, FriendsPageRequest, User};
use crate::shared::{
    ApiOperationFailure, CredentialAccessError, CredentialReader, FirstSeen, FirstSeenResult,
    Limit, Offset, ReadFailureKind, require_credential,
};

#[derive(Debug, Eq, PartialEq)]
pub struct FriendsResult {
    users: Vec<User>,
    next_offset: Option<Offset>,
}

impl FriendsResult {
    #[must_use]
    pub(crate) fn new(users: Vec<User>, next_offset: Option<Offset>) -> Self {
        Self { users, next_offset }
    }

    #[must_use]
    pub fn users(&self) -> &[User] {
        &self.users
    }

    #[must_use]
    pub const fn next_offset(&self) -> Option<Offset> {
        self.next_offset
    }
}

#[derive(Debug, Error)]
pub enum FriendsError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to list Venmo friends: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo friend-list response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },
}

impl FriendsError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::Credential(_) => ReadFailureKind::Credential,
            Self::Api { source } => ReadFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => ReadFailureKind::ResponseContract,
        }
    }
}

pub async fn list<R, A>(
    credentials: &R,
    api: &A,
    limit: Limit,
    offset: Offset,
) -> Result<FriendsResult, FriendsError>
where
    R: CredentialReader,
    A: FriendsApi,
{
    let loaded = require_credential(credentials)?;
    let page = api
        .friends(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            FriendsPageRequest::new(limit, offset),
        )
        .await
        .map_err(|source| FriendsError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    let (page_users, next_token) = page.into_parts();
    validate_page_len(page_users.len(), limit)?;
    validate_next_offset(offset, next_token)?;
    if page_users.is_empty() && next_token.is_some() {
        return Err(FriendsError::ResponseContract {
            problem: "the API returned an empty page with a continuation offset",
        });
    }
    let users = buffer_users(page_users)?;
    Ok(FriendsResult::new(users, next_token))
}

fn buffer_users(page_users: Vec<User>) -> Result<Vec<User>, FriendsError> {
    let mut users = Vec::with_capacity(page_users.len());
    let mut seen = FirstSeen::with_capacity(page_users.len());
    for user in page_users {
        match seen.push(&mut users, user.user_id().clone(), user) {
            FirstSeenResult::First | FirstSeenResult::Duplicate => {}
            FirstSeenResult::Conflicting => {
                return Err(FriendsError::ResponseContract {
                    problem: "the API returned conflicting records for one user ID",
                });
            }
        }
    }
    Ok(users)
}

fn validate_next_offset(current: Offset, next: Option<Offset>) -> Result<(), FriendsError> {
    if next.is_some_and(|next| next.get() <= current.get()) {
        return Err(FriendsError::ResponseContract {
            problem: "the API returned a repeated or non-progressing continuation offset",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), FriendsError> {
    if actual > requested.get() as usize {
        return Err(FriendsError::ResponseContract {
            problem: "the API returned more friends than requested",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::error::Error;
    use std::future::{Future, ready};
    use std::rc::Rc;
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::features::people::FriendsPage;
    use crate::shared::{
        AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
        CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId,
        LoadedCredential, UserId, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    const FIXTURE_TOKEN: &str = "synthetic-token";
    const FIXTURE_DEVICE_ID: &str = "synthetic-device";
    const FIXTURE_USER_ID: &str = "1000";
    const API_FAILURE_KINDS: [ApiFailureKind; 6] = [
        ApiFailureKind::Network,
        ApiFailureKind::Timeout,
        ApiFailureKind::Rejected,
        ApiFailureKind::Contract,
        ApiFailureKind::AmbiguousWrite,
        ApiFailureKind::Internal,
    ];
    const CREDENTIAL_FAILURE_KINDS: [CredentialFailureKind; 8] = [
        CredentialFailureKind::Unavailable,
        CredentialFailureKind::Corrupt,
        CredentialFailureKind::Invalid,
        CredentialFailureKind::UnsupportedVersion,
        CredentialFailureKind::TooLarge,
        CredentialFailureKind::Ambiguous,
        CredentialFailureKind::Platform,
        CredentialFailureKind::Internal,
    ];

    type Transcript = Rc<RefCell<Vec<Call>>>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SensitiveArgument {
        Fixture,
        Other,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct SessionCall {
        access_token: SensitiveArgument,
        device_id: SensitiveArgument,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Call {
        ReadCredential,
        Friends {
            session: SessionCall,
            current_user_id: UserId,
            page: FriendsPageRequest,
        },
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ReaderOutcome {
        Present,
        Missing,
        Failure(CredentialFailureKind),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ResponseId {
        Primary,
        UnexpectedSecond,
    }

    struct ScriptedResponse {
        id: ResponseId,
        result: Result<FriendsPage, FakeApiError>,
    }

    #[derive(Debug, Eq, PartialEq)]
    enum Outcome {
        Success(FriendsResult),
        MissingCredential,
        CredentialReadFailure,
        ApiFailure(ApiFailureKind),
        ResponseContract(&'static str),
    }

    impl From<Result<FriendsResult, FriendsError>> for Outcome {
        fn from(result: Result<FriendsResult, FriendsError>) -> Self {
            match result {
                Ok(result) => Self::Success(result),
                Err(FriendsError::Credential(CredentialAccessError::Missing)) => {
                    Self::MissingCredential
                }
                Err(FriendsError::Credential(CredentialAccessError::Read { .. })) => {
                    Self::CredentialReadFailure
                }
                Err(FriendsError::Api { source }) => Self::ApiFailure(source.kind()),
                Err(FriendsError::ResponseContract { problem }) => Self::ResponseContract(problem),
            }
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    struct FakeState {
        reader: ReaderOutcome,
        remaining_responses: Vec<ResponseId>,
        transcript: Vec<Call>,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Snapshot {
        outcome: Outcome,
        state: FakeState,
    }

    struct FakeReader {
        outcome: ReaderOutcome,
        transcript: Transcript,
    }

    impl FakeReader {
        fn new(outcome: ReaderOutcome, transcript: Transcript) -> Self {
            Self {
                outcome,
                transcript,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
    #[error("synthetic credential failure")]
    struct FakeCredentialError(CredentialFailureKind);

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            self.0
        }
    }

    impl CredentialCapability for FakeReader {
        type Error = FakeCredentialError;
    }

    impl CredentialReader for FakeReader {
        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            self.transcript.borrow_mut().push(Call::ReadCredential);
            match self.outcome {
                ReaderOutcome::Present => credential().map(Some),
                ReaderOutcome::Missing => Ok(None),
                ReaderOutcome::Failure(kind) => Err(FakeCredentialError(kind)),
            }
        }
    }

    #[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
    #[error("synthetic API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    struct FakeApi {
        responses: RefCell<VecDeque<ScriptedResponse>>,
        transcript: Transcript,
    }

    impl FakeApi {
        fn new(responses: Vec<ScriptedResponse>, transcript: Transcript) -> Self {
            Self {
                responses: RefCell::new(responses.into()),
                transcript,
            }
        }

        fn remaining_responses(&self) -> Vec<ResponseId> {
            self.responses
                .borrow()
                .iter()
                .map(|response| response.id)
                .collect()
        }
    }

    impl FriendsApi for FakeApi {
        type Error = FakeApiError;

        fn friends<'a>(
            &'a self,
            access_token: &'a AccessToken,
            device_id: &'a DeviceId,
            current_user_id: &'a UserId,
            request: FriendsPageRequest,
        ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a {
            self.transcript.borrow_mut().push(Call::Friends {
                session: session_call(access_token, device_id),
                current_user_id: current_user_id.clone(),
                page: request,
            });
            let result = self
                .responses
                .borrow_mut()
                .pop_front()
                .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                    response.result
                });
            ready(result)
        }
    }

    fn session_call(access_token: &AccessToken, device_id: &DeviceId) -> SessionCall {
        SessionCall {
            access_token: if access_token.expose_secret() == FIXTURE_TOKEN {
                SensitiveArgument::Fixture
            } else {
                SensitiveArgument::Other
            },
            device_id: if device_id.as_str() == FIXTURE_DEVICE_ID {
                SensitiveArgument::Fixture
            } else {
                SensitiveArgument::Other
            },
        }
    }

    const fn fixture_session() -> SessionCall {
        SessionCall {
            access_token: SensitiveArgument::Fixture,
            device_id: SensitiveArgument::Fixture,
        }
    }

    fn snapshot(
        result: Result<FriendsResult, FriendsError>,
        reader: &FakeReader,
        api: &FakeApi,
        transcript: &Transcript,
    ) -> Snapshot {
        Snapshot {
            outcome: Outcome::from(result),
            state: FakeState {
                reader: reader.outcome,
                remaining_responses: api.remaining_responses(),
                transcript: transcript.borrow().clone(),
            },
        }
    }

    fn scripted(id: ResponseId, result: Result<FriendsPage, FakeApiError>) -> ScriptedResponse {
        ScriptedResponse { id, result }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_invocation_fetches_exactly_one_native_page() -> TestResult {
        // Setup.
        let duplicate = user("11", "alice", "Alice")?;
        let expected_user = duplicate.clone();
        let limit = Limit::try_from(2)?;
        let offset = Offset::new(10);

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![
                scripted(
                    ResponseId::Primary,
                    Ok(FriendsPage::new(
                        vec![duplicate.clone(), duplicate],
                        Some(Offset::new(12)),
                    )),
                ),
                scripted(
                    ResponseId::UnexpectedSecond,
                    Err(FakeApiError(ApiFailureKind::Internal)),
                ),
            ],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: Outcome::Success(FriendsResult::new(
                vec![expected_user],
                Some(Offset::new(12)),
            )),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: vec![ResponseId::UnexpectedSecond],
                transcript: vec![
                    Call::ReadCredential,
                    Call::Friends {
                        session: fixture_session(),
                        current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
                        page: FriendsPageRequest::new(limit, offset),
                    },
                ],
            },
        };

        // Execute once.
        let result = list(&reader, &api, limit, offset).await;
        let observed = snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn empty_page_without_continuation_is_terminal() -> TestResult {
        // Setup.
        let limit = Limit::try_from(3)?;
        let offset = Offset::new(9);

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![scripted(
                ResponseId::Primary,
                Ok(FriendsPage::new(Vec::new(), None)),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: Outcome::Success(FriendsResult::new(Vec::new(), None)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: Vec::new(),
                transcript: vec![
                    Call::ReadCredential,
                    Call::Friends {
                        session: fixture_session(),
                        current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
                        page: FriendsPageRequest::new(limit, offset),
                    },
                ],
            },
        };

        // Execute once.
        let result = list(&reader, &api, limit, offset).await;
        let observed = snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_pages_fail_closed_after_the_one_exact_request() -> TestResult {
        // Setup.
        let cases = [
            (
                FriendsPage::new(
                    vec![user("1", "alice", "Alice")?, user("1", "alice", "Changed")?],
                    None,
                ),
                2,
                "the API returned conflicting records for one user ID",
            ),
            (
                FriendsPage::new(
                    vec![user("1", "alice", "Alice")?, user("2", "bob", "Bob")?],
                    None,
                ),
                1,
                "the API returned more friends than requested",
            ),
            (
                FriendsPage::new(Vec::new(), Some(Offset::new(11))),
                1,
                "the API returned an empty page with a continuation offset",
            ),
            (
                FriendsPage::new(vec![user("1", "alice", "Alice")?], Some(Offset::new(10))),
                1,
                "the API returned a repeated or non-progressing continuation offset",
            ),
        ];

        for (page, limit, problem) in cases {
            // Immutable initial state.
            let limit = Limit::try_from(limit)?;
            let offset = Offset::new(10);
            let transcript = Rc::new(RefCell::new(Vec::new()));
            let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
            let api = FakeApi::new(
                vec![scripted(ResponseId::Primary, Ok(page))],
                Rc::clone(&transcript),
            );

            // Complete expected outcome and final fake state.
            let expected = Snapshot {
                outcome: Outcome::ResponseContract(problem),
                state: FakeState {
                    reader: ReaderOutcome::Present,
                    remaining_responses: Vec::new(),
                    transcript: vec![
                        Call::ReadCredential,
                        Call::Friends {
                            session: fixture_session(),
                            current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
                            page: FriendsPageRequest::new(limit, offset),
                        },
                    ],
                },
            };

            // Execute once.
            let result = list(&reader, &api, limit, offset).await;
            let observed = snapshot(result, &reader, &api, &transcript);

            assert_eq!(observed, expected);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn credential_load_matrix_stops_before_the_api() -> TestResult {
        // Setup.
        let cases = std::iter::once((ReaderOutcome::Missing, Outcome::MissingCredential)).chain(
            CREDENTIAL_FAILURE_KINDS
                .into_iter()
                .map(|kind| (ReaderOutcome::Failure(kind), Outcome::CredentialReadFailure)),
        );

        for (reader_outcome, expected_outcome) in cases {
            let limit = Limit::try_from(10)?;
            let offset = Offset::default();

            // Immutable initial state.
            let transcript = Rc::new(RefCell::new(Vec::new()));
            let reader = FakeReader::new(reader_outcome, Rc::clone(&transcript));
            let api = FakeApi::new(
                vec![scripted(
                    ResponseId::Primary,
                    Ok(FriendsPage::new(Vec::new(), None)),
                )],
                Rc::clone(&transcript),
            );

            // Complete expected outcome and final fake state.
            let expected = Snapshot {
                outcome: expected_outcome,
                state: FakeState {
                    reader: reader_outcome,
                    remaining_responses: vec![ResponseId::Primary],
                    transcript: vec![Call::ReadCredential],
                },
            };

            // Execute once.
            let result = list(&reader, &api, limit, offset).await;
            let observed = snapshot(result, &reader, &api, &transcript);

            assert_eq!(observed, expected);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn api_failure_kind_matrix_is_preserved() -> TestResult {
        // Setup.
        let limit = Limit::try_from(10)?;
        let offset = Offset::new(20);

        for kind in API_FAILURE_KINDS {
            // Immutable initial state.
            let transcript = Rc::new(RefCell::new(Vec::new()));
            let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
            let api = FakeApi::new(
                vec![scripted(ResponseId::Primary, Err(FakeApiError(kind)))],
                Rc::clone(&transcript),
            );

            // Complete expected outcome and final fake state.
            let expected = Snapshot {
                outcome: Outcome::ApiFailure(kind),
                state: FakeState {
                    reader: ReaderOutcome::Present,
                    remaining_responses: Vec::new(),
                    transcript: vec![
                        Call::ReadCredential,
                        Call::Friends {
                            session: fixture_session(),
                            current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
                            page: FriendsPageRequest::new(limit, offset),
                        },
                    ],
                },
            };

            // Execute once.
            let result = list(&reader, &api, limit, offset).await;
            let observed = snapshot(result, &reader, &api, &transcript);

            assert_eq!(observed, expected);
        }
        Ok(())
    }

    fn credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str(FIXTURE_TOKEN)
                    .map_err(|_| FakeCredentialError(CredentialFailureKind::Internal))?,
                DeviceId::from_str(FIXTURE_DEVICE_ID)
                    .map_err(|_| FakeCredentialError(CredentialFailureKind::Internal))?,
                UserId::from_str(FIXTURE_USER_ID)
                    .map_err(|_| FakeCredentialError(CredentialFailureKind::Internal))?,
                Username::from_bare("tester")
                    .map_err(|_| FakeCredentialError(CredentialFailureKind::Internal))?,
                Some("Test User".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        })
    }

    fn user(id: &str, username: &str, name: &str) -> Result<User, Box<dyn Error>> {
        Ok(User::new(
            UserId::from_str(id)?,
            Some(Username::from_bare(username)?),
            Some(name.to_owned()),
        ))
    }
}
