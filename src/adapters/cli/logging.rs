use std::io;

use tracing::Level;

pub(super) type InitializationError = Box<dyn std::error::Error + Send + Sync>;

pub(super) fn initialize(verbose: bool) -> Result<(), InitializationError> {
    if !verbose {
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_writer(io::stderr)
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .compact()
        .try_init()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_diagnostics_do_not_install_a_global_subscriber() {
        assert!(initialize(false).is_ok());
    }
}
