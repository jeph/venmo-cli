//! Public Model Context Protocol frontend.
//!
//! The MCP server mirrors the stable CLI leaf commands as typed tools while
//! keeping credential, API, and feature internals private. It uses stdio only;
//! stdout is reserved for MCP protocol messages.

mod catalog;
mod execution;
mod inputs;
mod resources;
mod results;
mod server;

#[cfg(test)]
mod tests;

pub use server::{McpBuildError, VenmoMcpServer, run_stdio};

/// Version printed by the `venmo-mcp` binary and advertised during MCP initialization.
pub const VERSION: &str = match option_env!("VENMO_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
