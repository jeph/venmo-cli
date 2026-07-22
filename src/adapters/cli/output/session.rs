use std::io::{self, Write};

use serde_json::Value;

use super::super::command::{CommandId, OutputFormat};
use super::super::error::AppError;
use super::super::failure::{CliFailure, FailureOutcome, intrinsic_outcome};
use super::super::response::Response;
use super::json;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreflightMode {
    DryRun,
    AssumeYes,
    Prompt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputClass {
    Ordinary,
    DryRun,
    FinancialMutation,
    StateMutation,
    AuthState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MutationPhase {
    None,
    Planned,
    WriteStarted,
    Completed,
}

pub(crate) struct OutputSession<'a, W, E> {
    format: OutputFormat,
    command: CommandId,
    can_prompt: bool,
    stdout: &'a mut W,
    stderr: &'a mut E,
    plan: Option<Value>,
    partial_result: Option<Value>,
    phase: MutationPhase,
}

impl<'a, W, E> OutputSession<'a, W, E>
where
    W: Write,
    E: Write,
{
    pub(crate) fn new(
        format: OutputFormat,
        command: CommandId,
        can_prompt: bool,
        stdout: &'a mut W,
        stderr: &'a mut E,
    ) -> Self {
        Self {
            format,
            command,
            can_prompt,
            stdout,
            stderr,
            plan: None,
            partial_result: None,
            phase: MutationPhase::None,
        }
    }

    pub(crate) fn write_preflight<T: ?Sized>(
        &mut self,
        response: &Response<'_, T>,
        mode: PreflightMode,
        human: impl FnOnce(&mut E, &Response<'_, T>) -> io::Result<()>,
    ) -> Result<(), AppError> {
        self.plan = Some(response.data().clone());
        self.phase = MutationPhase::Planned;
        let should_render = self.format == OutputFormat::Human
            || (mode == PreflightMode::Prompt && self.can_prompt);
        if should_render {
            human(self.stderr, response)?;
            self.stderr.flush()?;
        }
        Ok(())
    }

    pub(crate) fn write_success<T: ?Sized>(
        &mut self,
        response: &Response<'_, T>,
        class: OutputClass,
        human: impl FnOnce(&mut W, &mut E, &Response<'_, T>) -> io::Result<()>,
    ) -> Result<(), AppError> {
        let format = self.format;
        let result = match format {
            OutputFormat::Human => human(self.stdout, self.stderr, response),
            OutputFormat::Json => json::write_success(self.stdout, self.command, response),
        }
        .and_then(|()| match (format, class) {
            // The JSON renderer serializes, writes, and flushes as one operation.
            (OutputFormat::Json, _) => Ok(()),
            (OutputFormat::Human, OutputClass::Ordinary) => Ok(()),
            (
                OutputFormat::Human,
                OutputClass::DryRun | OutputClass::FinancialMutation | OutputClass::StateMutation,
            ) => self.stdout.flush(),
            (OutputFormat::Human, OutputClass::AuthState) => {
                let stdout = self.stdout.flush();
                let stderr = self.stderr.flush();
                stdout.and(stderr)
            }
        });
        result.map_err(|source| output_error(class, source))
    }

    pub(crate) fn write_partial<T: ?Sized>(
        &mut self,
        response: &Response<'_, T>,
        human: impl FnOnce(&mut W, &mut E, &Response<'_, T>) -> io::Result<()>,
    ) -> Result<(), AppError> {
        self.record_partial(response);
        if self.format == OutputFormat::Json {
            return Ok(());
        }
        human(self.stdout, self.stderr, response)
            .and_then(|()| {
                let stdout = self.stdout.flush();
                let stderr = self.stderr.flush();
                stdout.and(stderr)
            })
            .map_err(|source| AppError::AuthStateOutput { source })
    }

    pub(crate) fn record_partial<T: ?Sized>(&mut self, response: &Response<'_, T>) {
        self.partial_result = Some(response.data().clone());
    }

    pub(crate) fn clear_partial(&mut self) {
        self.partial_result = None;
    }

    pub(crate) fn mark_write_started(&mut self) {
        self.phase = MutationPhase::WriteStarted;
    }

    pub(crate) fn mark_completed(&mut self) {
        self.phase = MutationPhase::Completed;
    }

    pub(crate) fn stderr(&mut self) -> &mut E {
        self.stderr
    }

    pub(crate) fn into_failure(self, error: AppError) -> CliFailure {
        let intrinsic = intrinsic_outcome(&error);
        let outcome = match (intrinsic, self.phase, self.partial_result.is_some()) {
            (FailureOutcome::NotPerformed, _, true) => FailureOutcome::Partial,
            (FailureOutcome::NotPerformed, MutationPhase::Completed, false) => {
                FailureOutcome::Completed
            }
            (outcome, _, _) => outcome,
        };
        CliFailure {
            error,
            command: self.command,
            format: self.format,
            outcome,
            plan: self.plan,
            partial_result: self.partial_result,
        }
    }
}

fn output_error(class: OutputClass, source: io::Error) -> AppError {
    match class {
        OutputClass::Ordinary | OutputClass::DryRun => AppError::CommandOutput { source },
        OutputClass::FinancialMutation => AppError::FinancialResultOutput { source },
        OutputClass::StateMutation => AppError::StateMutationResultOutput { source },
        OutputClass::AuthState => AppError::AuthStateOutput { source },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    struct FlushFailWriter(Vec<u8>);

    impl Write for FlushFailWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("synthetic flush failure"))
        }
    }

    #[test]
    fn json_success_is_one_compact_newline_terminated_object() -> TestResult {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let response = Response::new(&(), serde_json::json!({ "items": [], "next": null }));
        let mut output = OutputSession::new(
            OutputFormat::Json,
            CommandId::RequestsList,
            false,
            &mut stdout,
            &mut stderr,
        );

        output.write_success(
            &response,
            OutputClass::Ordinary,
            |_stdout, _stderr, _response| Err(io::Error::other("human renderer must not run")),
        )?;

        assert!(stderr.is_empty());
        assert_eq!(stdout.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(stdout.last(), Some(&b'\n'));
        assert_eq!(
            serde_json::from_slice::<Value>(&stdout)?,
            serde_json::json!({
                "command": "requests.list",
                "ok": true,
                "data": { "items": [], "next": null },
            })
        );
        Ok(())
    }

    #[test]
    fn json_preflights_are_quiet_unless_an_interactive_prompt_will_follow() -> TestResult {
        for (mode, can_prompt, expected) in [
            (PreflightMode::DryRun, true, ""),
            (PreflightMode::AssumeYes, true, ""),
            (PreflightMode::Prompt, false, ""),
            (PreflightMode::Prompt, true, "human preflight\n"),
        ] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let response = Response::new(&(), serde_json::json!({ "safe": "plan" }));
            let mut output = OutputSession::new(
                OutputFormat::Json,
                CommandId::PayUser,
                can_prompt,
                &mut stdout,
                &mut stderr,
            );

            output.write_preflight(&response, mode, |writer, _response| {
                writer.write_all(b"human preflight\n")
            })?;
            assert_eq!(String::from_utf8(stderr)?, expected);
            assert!(stdout.is_empty());
        }
        Ok(())
    }

    #[test]
    fn failures_keep_safe_plan_context_and_unknown_outcomes() -> TestResult {
        let secret = "synthetic-secret-credential-envelope";
        let mut stdout = Vec::new();
        let mut diagnostics = Vec::new();
        let source = secret.to_owned();
        let plan = Response::new(&source, serde_json::json!({ "amount": "1.00" }));
        let mut output = OutputSession::new(
            OutputFormat::Json,
            CommandId::PayUser,
            false,
            &mut stdout,
            &mut diagnostics,
        );
        output.write_preflight(&plan, PreflightMode::AssumeYes, |_writer, _response| Ok(()))?;
        output.mark_write_started();
        let failure = output.into_failure(AppError::FinancialWriteInterruptedUnknown);
        let mut stderr = Vec::new();
        super::super::json::write_failure(&mut stderr, &failure)?;

        let text = String::from_utf8(stderr.clone())?;
        assert!(!text.contains(secret));
        let value: Value = serde_json::from_slice(&stderr)?;
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["command"], "pay.user");
        assert_eq!(value["error"]["code"], "write_outcome_unknown");
        assert_eq!(value["error"]["category"], "ambiguous_write");
        assert_eq!(value["error"]["exit_code"], 3);
        assert_eq!(value["error"]["outcome"], "unknown");
        assert_eq!(
            value["context"]["plan"],
            serde_json::json!({ "amount": "1.00" })
        );
        assert_eq!(value["partial_result"], Value::Null);
        assert!(stdout.is_empty());
        assert!(diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn json_mutation_flush_failures_remain_completed_exit_three_errors() -> TestResult {
        let mut stdout = FlushFailWriter(Vec::new());
        let mut stderr = Vec::new();
        let response = Response::new(&(), serde_json::json!({ "outcome": "completed" }));
        let mut output = OutputSession::new(
            OutputFormat::Json,
            CommandId::PayUser,
            false,
            &mut stdout,
            &mut stderr,
        );
        output.mark_completed();

        let error = match output.write_success(
            &response,
            OutputClass::FinancialMutation,
            |_stdout, _stderr, _response| Ok(()),
        ) {
            Err(error) => error,
            Ok(()) => return Err("the synthetic JSON flush should fail".into()),
        };
        assert!(matches!(error, AppError::FinancialResultOutput { .. }));
        let failure = output.into_failure(error);

        assert_eq!(failure.exit_code(), 3);
        assert_eq!(failure.outcome, FailureOutcome::Completed);
        assert!(stderr.is_empty());
        let value: Value = serde_json::from_slice(&stdout.0)?;
        assert_eq!(value["data"]["outcome"], "completed");
        Ok(())
    }

    #[test]
    fn partial_and_completed_failure_outcomes_are_explicit() {
        for (partial, completed, expected) in [
            (
                Some(serde_json::json!({ "stored": true })),
                false,
                "partial",
            ),
            (None, true, "completed"),
            (None, false, "not_performed"),
        ] {
            let mut stdout = Vec::new();
            let mut diagnostics = Vec::new();
            let source = ();
            let mut output = OutputSession::new(
                OutputFormat::Json,
                CommandId::AuthLogin,
                false,
                &mut stdout,
                &mut diagnostics,
            );
            if let Some(partial) = partial {
                output.record_partial(&Response::new(&source, partial));
            }
            if completed {
                output.mark_completed();
            }
            let failure = output.into_failure(AppError::CommandOutput {
                source: io::Error::other("synthetic output failure"),
            });
            assert_eq!(failure.outcome.as_str(), expected);
        }
    }
}
