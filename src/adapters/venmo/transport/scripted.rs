use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use reqwest::{Method, Url};
use zeroize::Zeroizing;

use super::client::parse_trusted_next_link_at_origin;
use super::error::{TransportBuildError, TransportError};
use super::request::{HttpRequest, OperationClass, RequestCredentials, ResponseCapture};
use super::response::HttpResponse;
use super::{ApiTransport, PRODUCTION_API_BASE};

#[derive(Clone, Eq, PartialEq)]
pub(in crate::adapters::venmo) struct ScriptedSecret(Zeroizing<String>);

impl ScriptedSecret {
    fn capture(value: &str) -> Self {
        Self(Zeroizing::new(value.to_owned()))
    }
}

impl fmt::Debug for ScriptedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::adapters::venmo) enum ScriptedCredentials {
    Authenticated {
        access_token: ScriptedSecret,
        device_id: ScriptedSecret,
    },
    Device {
        device_id: ScriptedSecret,
    },
    OtpSecret {
        device_id: ScriptedSecret,
        otp_secret: ScriptedSecret,
    },
    OtpCode {
        device_id: ScriptedSecret,
        otp_secret: ScriptedSecret,
        otp_code: ScriptedSecret,
    },
}

impl ScriptedCredentials {
    fn capture(credentials: RequestCredentials<'_>) -> Self {
        match credentials {
            RequestCredentials::Authenticated(session) => Self::Authenticated {
                access_token: ScriptedSecret::capture(session.access_token.expose_secret()),
                device_id: ScriptedSecret::capture(session.device_id.as_str()),
            },
            RequestCredentials::Device(device_id) => Self::Device {
                device_id: ScriptedSecret::capture(device_id.as_str()),
            },
            RequestCredentials::OtpSecret {
                device_id,
                otp_secret,
            } => Self::OtpSecret {
                device_id: ScriptedSecret::capture(device_id.as_str()),
                otp_secret: ScriptedSecret::capture(otp_secret.expose()),
            },
            RequestCredentials::OtpCode {
                device_id,
                otp_secret,
                otp_code,
            } => Self::OtpCode {
                device_id: ScriptedSecret::capture(device_id.as_str()),
                otp_secret: ScriptedSecret::capture(otp_secret.expose()),
                otp_code: ScriptedSecret::capture(otp_code.expose()),
            },
        }
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn authenticated_for_test(
        access_token: &str,
        device_id: &str,
    ) -> Self {
        Self::Authenticated {
            access_token: ScriptedSecret::capture(access_token),
            device_id: ScriptedSecret::capture(device_id),
        }
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn device_for_test(device_id: &str) -> Self {
        Self::Device {
            device_id: ScriptedSecret::capture(device_id),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
struct ScriptedJsonBody {
    bytes: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for ScriptedJsonBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptedJsonBody")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(in crate::adapters::venmo) struct ScriptedRequest {
    credentials: ScriptedCredentials,
    method: Method,
    route_template: &'static str,
    path_segments: Vec<String>,
    query: Vec<(String, String)>,
    json_body: Option<ScriptedJsonBody>,
    operation: OperationClass,
    response_capture: ResponseCapture,
}

impl ScriptedRequest {
    fn capture(credentials: RequestCredentials<'_>, request: HttpRequest<'_>) -> Self {
        Self {
            credentials: ScriptedCredentials::capture(credentials),
            method: request.method,
            route_template: request.route_template,
            path_segments: request
                .path_segments
                .iter()
                .map(|segment| (*segment).to_owned())
                .collect(),
            query: request
                .query
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            json_body: request
                .json_body
                .map(|body| ScriptedJsonBody { bytes: body.bytes }),
            operation: request.operation,
            response_capture: request.response_capture,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(in crate::adapters::venmo) fn for_test(
        credentials: ScriptedCredentials,
        method: Method,
        route_template: &'static str,
        path_segments: &[&str],
        query: &[(&str, &str)],
        json_body: Option<&[u8]>,
        operation: OperationClass,
        response_capture: ResponseCapture,
    ) -> Self {
        Self {
            credentials,
            method,
            route_template,
            path_segments: path_segments
                .iter()
                .map(|segment| (*segment).to_owned())
                .collect(),
            query: query
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            json_body: json_body.map(|bytes| ScriptedJsonBody {
                bytes: Zeroizing::new(bytes.to_vec()),
            }),
            operation,
            response_capture,
        }
    }
}

impl fmt::Debug for ScriptedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let query_fields = self
            .query
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        formatter
            .debug_struct("ScriptedRequest")
            .field("credentials", &self.credentials)
            .field("method", &self.method)
            .field("route_template", &self.route_template)
            .field("path_segment_count", &self.path_segments.len())
            .field("query_fields", &query_fields)
            .field("json_body", &self.json_body)
            .field("operation", &self.operation)
            .field("response_capture", &self.response_capture)
            .finish()
    }
}

struct ScriptedState {
    results: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
    transcript: Mutex<Vec<ScriptedRequest>>,
    unexpected_request: Mutex<bool>,
}

#[derive(Eq, PartialEq)]
pub(in crate::adapters::venmo) struct ScriptedResponseBody(Zeroizing<Vec<u8>>);

impl fmt::Debug for ScriptedResponseBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptedResponseBody")
            .field("bytes", &self.0.len())
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::adapters::venmo) enum ScriptedResultSnapshot {
    Response {
        status: u16,
        body: ScriptedResponseBody,
        otp_secret: Option<ScriptedResponseBody>,
    },
    Error(TransportError),
}

impl ScriptedResultSnapshot {
    fn capture(result: &Result<HttpResponse, TransportError>) -> Self {
        match result {
            Ok(response) => Self::Response {
                status: response.status().as_u16(),
                body: ScriptedResponseBody(Zeroizing::new(response.body().to_vec())),
                otp_secret: response
                    .otp_secret()
                    .map(|secret| ScriptedResponseBody(Zeroizing::new(secret.to_vec()))),
            },
            Err(error) => Self::Error(error.clone()),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::adapters::venmo) struct ScriptedTransportSnapshot {
    transcript: Vec<ScriptedRequest>,
    remaining_results: Vec<ScriptedResultSnapshot>,
    unexpected_request: bool,
}

impl ScriptedTransportSnapshot {
    #[must_use]
    pub(in crate::adapters::venmo) const fn for_test(
        transcript: Vec<ScriptedRequest>,
        remaining_results: Vec<ScriptedResultSnapshot>,
        unexpected_request: bool,
    ) -> Self {
        Self {
            transcript,
            remaining_results,
            unexpected_request,
        }
    }
}

#[derive(Clone)]
pub(in crate::adapters::venmo) struct ScriptedTransport {
    base_url: Url,
    state: Arc<ScriptedState>,
}

impl ScriptedTransport {
    pub(in crate::adapters::venmo) fn new(
        results: impl IntoIterator<Item = Result<HttpResponse, TransportError>>,
    ) -> Result<Self, TransportBuildError> {
        let base_url = Url::parse(PRODUCTION_API_BASE)
            .map_err(|_| TransportBuildError::InvalidProductionOrigin)?;
        Ok(Self {
            base_url,
            state: Arc::new(ScriptedState {
                results: Mutex::new(results.into_iter().collect()),
                transcript: Mutex::new(Vec::new()),
                unexpected_request: Mutex::new(false),
            }),
        })
    }

    fn respond(
        &self,
        credentials: RequestCredentials<'_>,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        lock_unpoisoned(&self.state.transcript)
            .push(ScriptedRequest::capture(credentials, request));
        match lock_unpoisoned(&self.state.results).pop_front() {
            Some(result) => result,
            None => {
                *lock_unpoisoned(&self.state.unexpected_request) = true;
                Err(TransportError::Network)
            }
        }
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn snapshot(&self) -> ScriptedTransportSnapshot {
        ScriptedTransportSnapshot {
            transcript: lock_unpoisoned(&self.state.transcript).clone(),
            remaining_results: lock_unpoisoned(&self.state.results)
                .iter()
                .map(ScriptedResultSnapshot::capture)
                .collect(),
            unexpected_request: *lock_unpoisoned(&self.state.unexpected_request),
        }
    }
}

impl ApiTransport for ScriptedTransport {
    async fn execute<'a>(
        &'a self,
        credentials: RequestCredentials<'a>,
        request: HttpRequest<'a>,
    ) -> Result<HttpResponse, TransportError> {
        self.respond(credentials, request)
    }

    fn parse_trusted_next_link(
        &self,
        raw: &str,
        expected_path_segments: &[&str],
    ) -> Result<Vec<(String, String)>, TransportError> {
        parse_trusted_next_link_at_origin(&self.base_url, raw, expected_path_segments)
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
