use std::ffi::OsStr;
use std::fmt::Display;
use std::str::FromStr;

use clap::builder::TypedValueParser;
use clap::error::ErrorKind;

use crate::features::activity::ActivityBeforeId;
use crate::features::requests::{RequestId, RequestsBefore};

#[derive(Clone)]
pub(super) struct RedactedActivityBeforeIdParser;

impl TypedValueParser for RedactedActivityBeforeIdParser {
    type Value = ActivityBeforeId;

    fn parse_ref(
        &self,
        command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        parse_redacted(command, value, "token", "before-id continuation token")
    }
}

#[derive(Clone)]
pub(super) struct RedactedRequestsBeforeParser;

impl TypedValueParser for RedactedRequestsBeforeParser {
    type Value = RequestsBefore;

    fn parse_ref(
        &self,
        command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        parse_redacted(command, value, "token", "before continuation token")
    }
}

#[derive(Clone)]
pub(super) struct RedactedRequestIdParser;

impl TypedValueParser for RedactedRequestIdParser {
    type Value = RequestId;

    fn parse_ref(
        &self,
        command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        parse_redacted(command, value, "request ID", "request ID")
    }
}

fn parse_redacted<T>(
    command: &clap::Command,
    value: &OsStr,
    target: &str,
    label: &str,
) -> Result<T, clap::Error>
where
    T: FromStr,
    T::Err: Display,
{
    let parsed = value
        .to_str()
        .ok_or_else(|| parse_error(command, label, format_args!("{target} is not valid UTF-8")))?;
    T::from_str(parsed).map_err(|source| parse_error(command, label, source))
}

fn parse_error(command: &clap::Command, label: &str, source: impl Display) -> clap::Error {
    command.clone().error(
        ErrorKind::ValueValidation,
        format_args!("invalid {label}: {source}"),
    )
}
