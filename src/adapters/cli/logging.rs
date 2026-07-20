use std::io;

use tracing::Level;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub(super) type InitializationError = Box<dyn std::error::Error + Send + Sync>;

pub(super) fn initialize(debug: bool) -> Result<(), InitializationError> {
    if !debug {
        return Ok(());
    }

    let first_party_debug = filter_fn(|metadata| {
        is_first_party_target(metadata.target()) && *metadata.level() <= Level::DEBUG
    });
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(io::stderr)
                .with_ansi(false)
                .without_time()
                .with_target(false)
                .compact()
                .with_filter(first_party_debug),
        )
        .try_init()?;
    Ok(())
}

fn is_first_party_target(target: &str) -> bool {
    target == "venmo_cli" || target.starts_with("venmo_cli::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_debug_diagnostics_do_not_install_a_global_subscriber() {
        assert!(initialize(false).is_ok());
    }

    #[test]
    fn debug_filter_accepts_only_first_party_targets() {
        for target in ["venmo_cli", "venmo_cli::adapters::venmo"] {
            assert!(is_first_party_target(target));
        }
        for target in ["venmo", "reqwest", "hyper::client", "rustls::client"] {
            assert!(!is_first_party_target(target));
        }
    }
}
