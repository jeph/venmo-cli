use std::fmt;

use reqwest::StatusCode;
use zeroize::Zeroizing;

use super::error::PostSendFailure;

#[derive(Eq, PartialEq)]
pub(in crate::adapters::venmo) struct HttpResponse {
    pub(super) status: StatusCode,
    pub(super) body: Vec<u8>,
    pub(super) otp_secret: Option<Zeroizing<Vec<u8>>>,
}

impl HttpResponse {
    #[must_use]
    pub(in crate::adapters::venmo) const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn body(&self) -> &[u8] {
        &self.body
    }

    pub(in crate::adapters::venmo) fn otp_secret(&self) -> Option<&[u8]> {
        self.otp_secret.as_ref().map(|value| value.as_slice())
    }

    #[cfg(test)]
    #[must_use]
    pub(in crate::adapters::venmo) fn for_test(
        status: StatusCode,
        body: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            status,
            body: body.into(),
            otp_secret: None,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(in crate::adapters::venmo) fn with_otp_secret_for_test(mut self, otp_secret: &str) -> Self {
        self.otp_secret = Some(Zeroizing::new(otp_secret.as_bytes().to_vec()));
        self
    }
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("body_bytes", &self.body.len())
            .field("has_otp_secret", &self.otp_secret.is_some())
            .finish()
    }
}

pub(super) async fn read_bounded_body(
    mut response: reqwest::Response,
    maximum_bytes: usize,
) -> Result<Vec<u8>, PostSendFailure> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes as u64)
    {
        return Err(PostSendFailure::ResponseTooLarge);
    }

    let initial_capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or(0)
        .min(maximum_bytes);
    let mut body = Vec::new();
    body.try_reserve(initial_capacity)
        .map_err(|_| PostSendFailure::ResourceExhaustion)?;

    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|_| PostSendFailure::ResponseRead)?;
        let Some(chunk) = chunk else {
            break;
        };
        reserve_body_chunk(&mut body, chunk.len(), maximum_bytes)?;
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

pub(super) fn reserve_body_chunk(
    body: &mut Vec<u8>,
    chunk_length: usize,
    maximum_bytes: usize,
) -> Result<(), PostSendFailure> {
    if body
        .len()
        .checked_add(chunk_length)
        .is_none_or(|length| length > maximum_bytes)
    {
        return Err(PostSendFailure::ResponseTooLarge);
    }

    // Reserve relative to length so the immediately following extension cannot
    // allocate through an infallible API.
    body.try_reserve(chunk_length)
        .map_err(|_| PostSendFailure::ResourceExhaustion)
}
