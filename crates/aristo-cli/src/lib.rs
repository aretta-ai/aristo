//! Library form of the Aristo CLI. The `aristo` binary (`src/main.rs`) is
//! a thin wrapper that calls [`run`] and exits with its return code.
//!
//! Splitting the CLI into a lib + tiny bin lets integration tests exercise
//! `dispatch()` directly without spawning a child process for every case
//! (the `binary_smoke` test still spawns one, on purpose, as the canary
//! for the binary's own glue).

mod error;
mod filter;
mod workspace;

pub use error::{CliError, CliResult};
pub use filter::{Filter, FilterParseError};
pub use workspace::{Workspace, WorkspaceError};
