#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "venmo-mcp",
    version = venmo_cli::mcp::VERSION,
    about = "Expose Venmo CLI commands through a local MCP stdio server",
    long_about = None
)]
struct Args {}

fn main() -> ExitCode {
    let Args {} = Args::parse();

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("venmo-mcp: could not initialize the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(venmo_cli::mcp::run_stdio()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("venmo-mcp: {error}");
            ExitCode::FAILURE
        }
    }
}
