use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde_json::Value;
use tokio::sync::{Mutex, Notify};

use crate::cli::{run, write_cli_failure};

use super::{inputs::Invocation, results};

pub(crate) type ExecutionFuture<'a> = Pin<Box<dyn Future<Output = Value> + Send + 'a>>;
pub(crate) type DrainFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

pub(crate) trait Executor: Send + Sync {
    fn execute<'a>(&'a self, invocation: Invocation) -> ExecutionFuture<'a>;

    fn wait_for_writes<'a>(&'a self) -> DrainFuture<'a> {
        Box::pin(std::future::ready(()))
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProductionExecutor {
    gate: Mutex<()>,
    writes: Arc<WriteActivity>,
}

impl Executor for ProductionExecutor {
    fn execute<'a>(&'a self, invocation: Invocation) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let _write = invocation
                .may_write
                .then(|| WriteGuard::begin(self.writes.clone()));
            let _gate = self.gate.lock().await;
            execute_cli(invocation).await
        })
    }

    fn wait_for_writes<'a>(&'a self) -> DrainFuture<'a> {
        Box::pin(self.writes.wait())
    }
}

#[derive(Debug, Default)]
struct WriteActivity {
    active: AtomicUsize,
    idle: Notify,
}

impl WriteActivity {
    async fn wait(&self) {
        loop {
            if self.active.load(Ordering::Acquire) == 0 {
                return;
            }
            let notified = self.idle.notified();
            if self.active.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug)]
struct WriteGuard {
    activity: Arc<WriteActivity>,
}

impl WriteGuard {
    fn begin(activity: Arc<WriteActivity>) -> Self {
        activity.active.fetch_add(1, Ordering::AcqRel);
        Self { activity }
    }
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        if self.activity.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.activity.idle.notify_waiters();
        }
    }
}

async fn execute_cli(invocation: Invocation) -> Value {
    let Invocation {
        cli,
        command,
        human_argv,
        may_write,
        dry_run,
    } = invocation;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let execution = run(cli, &mut stdout, &mut stderr).await;

    let mut envelope = match execution {
        Ok(()) => parse_single_json(&stdout)
            .unwrap_or_else(|| results::internal_envelope(command, may_write)),
        Err(failure) => {
            if write_cli_failure(&mut stderr, &failure).is_err() {
                results::internal_envelope(command, may_write)
            } else {
                parse_single_json(&stderr)
                    .unwrap_or_else(|| results::internal_envelope(command, may_write))
            }
        }
    };
    results::attach_sms_handoff(&mut envelope, dry_run, &human_argv);
    envelope
}

fn parse_single_json(bytes: &[u8]) -> Option<Value> {
    let mut values = serde_json::Deserializer::from_slice(bytes).into_iter::<Value>();
    let value = values.next()?.ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_exactly_one_json_value_and_whitespace() {
        assert_eq!(
            parse_single_json(b" {\"ok\":true}\n"),
            Some(serde_json::json!({"ok": true}))
        );
        assert!(parse_single_json(br#"{"ok":true} {"ok":false}"#).is_none());
        assert!(parse_single_json(b"diagnostic\n{\"ok\":true}\n").is_none());
    }

    #[test]
    fn vec_writes_cannot_fail() -> std::io::Result<()> {
        let mut bytes = Vec::new();
        std::io::Write::write_all(&mut bytes, b"test")?;
        assert_eq!(bytes, b"test");
        Ok(())
    }
}
