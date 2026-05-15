//! CLI error type and exit-code mapping.
//!
//! Slice 9A defines the shell; new variants are added by their first
//! concrete user. Exit codes follow loose `sysexits.h` precedent where
//! reasonable: 0 success, 1 general failure, 2 misuse / clap rejection, 64
//! not-yet-implemented.

use std::fmt;
use std::path::PathBuf;

pub type CliResult<T> = Result<T, CliError>;

/// Top-level CLI failure mode.
#[derive(Debug)]
pub enum CliError {
    /// A defined subcommand whose body has not yet landed. Carries the
    /// roadmap pointer so the message tells the user when to expect it.
    NotImplemented {
        what: &'static str,
        slice: &'static str,
    },
    /// The current working directory is not inside an Aristo workspace
    /// (no `aristo.toml` found by walking upward).
    NotInWorkspace { searched_from: PathBuf },
    /// I/O failure during file read/write.
    Io(std::io::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::NotImplemented { what, slice } => {
                write!(f, "{what} is not yet implemented (planned for {slice})")
            }
            CliError::NotInWorkspace { searched_from } => {
                write!(
                    f,
                    "not inside an Aristo workspace (no aristo.toml found at or above {})\n\
                     hint: run `aristo init` to bootstrap a new workspace here",
                    searched_from.display()
                )
            }
            CliError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Io(e)
    }
}

impl CliError {
    /// Process exit code for this error class.
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::NotImplemented { .. } => 64,
            CliError::NotInWorkspace { .. } => 2,
            CliError::Io(_) => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_implemented_message_includes_slice_pointer() {
        let e = CliError::NotImplemented {
            what: "aristo init",
            slice: "slice 10",
        };
        let msg = e.to_string();
        assert!(msg.contains("aristo init"), "msg: {msg}");
        assert!(msg.contains("slice 10"), "msg: {msg}");
    }

    #[test]
    fn not_in_workspace_includes_search_origin_and_hint() {
        let e = CliError::NotInWorkspace {
            searched_from: PathBuf::from("/tmp/elsewhere"),
        };
        let msg = e.to_string();
        assert!(msg.contains("/tmp/elsewhere"));
        assert!(msg.contains("aristo init"), "should hint at init: {msg}");
    }

    #[test]
    fn exit_codes_are_distinct_per_class() {
        let ni = CliError::NotImplemented {
            what: "x",
            slice: "y",
        }
        .exit_code();
        let nw = CliError::NotInWorkspace {
            searched_from: PathBuf::new(),
        }
        .exit_code();
        let io = CliError::Io(std::io::Error::other("boom")).exit_code();
        assert_ne!(ni, nw);
        assert_ne!(nw, io);
        assert_ne!(ni, io);
    }
}
