use std::io::{self, Write};

use serde::Serialize;
use serde_json::Value;

use super::super::command::CommandId;
use super::super::failure::{CliFailure, error_code};

#[derive(Serialize)]
struct SuccessEnvelope<'a, T: ?Sized> {
    command: &'static str,
    ok: bool,
    data: &'a T,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    category: &'a str,
    message: &'a str,
    exit_code: u8,
    outcome: &'a str,
}

#[derive(Serialize)]
struct FailureEnvelope<'a> {
    command: &'static str,
    ok: bool,
    error: ErrorBody<'a>,
    context: Option<FailureContext<'a>>,
    partial_result: Option<&'a Value>,
}

#[derive(Serialize)]
struct FailureContext<'a> {
    plan: &'a Value,
}

pub(super) fn write_success<W: Write, T: ?Sized + Serialize>(
    writer: &mut W,
    command: CommandId,
    data: &T,
) -> io::Result<()> {
    write_compact(
        writer,
        &SuccessEnvelope {
            command: command.as_str(),
            ok: true,
            data,
        },
    )
}

pub(super) fn write_failure(writer: &mut impl Write, failure: &CliFailure) -> io::Result<()> {
    let message = failure.error.to_string();
    write_compact(
        writer,
        &FailureEnvelope {
            command: failure.command.as_str(),
            ok: false,
            error: ErrorBody {
                code: error_code(&failure.error),
                category: failure.error.category().as_str(),
                message: &message,
                exit_code: failure.error.exit_code(),
                outcome: failure.outcome.as_str(),
            },
            context: failure.plan.as_ref().map(|plan| FailureContext { plan }),
            partial_result: failure.partial_result.as_ref(),
        },
    )
}

fn write_compact(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    writer.flush()
}
