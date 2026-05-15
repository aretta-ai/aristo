//! Library form of the Aristo CLI. The `aristo` binary (`src/main.rs`) is
//! a thin wrapper that calls [`run`] and exits with its return code.
//!
//! Splitting the CLI into a lib + tiny bin lets integration tests exercise
//! `dispatch` directly without spawning a child process for every case
//! (the `binary_smoke` test still spawns one, on purpose, as the canary
//! for the binary's own glue).

mod commands;
mod error;
mod filter;
mod workspace;

pub use error::{CliError, CliResult};
pub use filter::{Filter, FilterParseError};
pub use workspace::{Workspace, WorkspaceError};

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// Aristo annotation SDK CLI.
///
/// Subcommands map 1:1 to the surface defined in `docs/TOOLS.md` (offline
/// scope only — server-side commands `auth` / `sync` / `unbind` /
/// `suggestions` are deferred to Phase 2 and not declared here).
///
/// Each variant's body is a stub returning [`CliError::NotImplemented`]
/// with the roadmap slice it lands in. Future slices replace the stub
/// with the real handler in the same commit that promotes the matching
/// `_pending/` scenarios into `active/` per the testing convention.
#[derive(Parser, Debug)]
#[command(
    name = "aristo",
    version,
    about = "Aristo annotation SDK — write, verify, and document intent.",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Bootstrap a project for Aristo (creates aristo.toml, .aristo/, pre-commit hook, CI workflow).
    Init {
        /// Modify Cargo.toml to add `aristo` as a dependency. Default
        /// behavior just prints the dep line for the user to paste in.
        #[arg(short, long)]
        force: bool,
    },

    /// Print a syntax cheat sheet for the detected language.
    Lang {
        /// Per-file detection (Phase 2+ — Phase 1 errors on non-Rust extensions).
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Install Aristo skills for a coding agent (claude-code, cursor, codex, opencode, antigravity).
    InstallSkills,

    /// Reverse `install-skills`: remove SDK-bundled skills.
    UninstallSkills,

    /// Walk source, parse via syn, write .aristo/index.toml.
    Index,

    /// Index + ID assignment + drift detection + (Phase 2) B5b classification.
    Stamp,

    /// Look up an annotation by id, fn / mod / struct name, or file:line.
    Show,

    /// Flat enumeration of all annotations.
    List,

    /// Project-level summary (tier, counts, freshness).
    Status,

    /// Static text-quality pass over annotations (rule-based; no LLM).
    Lint,

    /// Run verification for every annotation that opted in.
    Verify,

    /// Agentic prose-improvement pass via the review skill.
    Review,

    /// Generate per-annotation markdown to .aristo/doc/.
    Doc,

    /// Generate the annotation graph (Mermaid / DOT / SVG).
    Graph,

    /// Generate a shareable SVG verification badge for README / docs.
    Badge,

    /// Atomic project-wide rename of an annotation id.
    Rename,
}

/// Process entry point. Parses `argv`, dispatches to the chosen subcommand,
/// and returns the exit code. Prints `error: <msg>` to stderr on any
/// `CliError`.
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

/// Maps a parsed `Commands` variant to its handler. Slice 9 wires every
/// subcommand to a stub returning the roadmap slice it lands in; future
/// slices replace each stub.
fn dispatch(cmd: Commands) -> CliResult<()> {
    match cmd {
        Commands::Init { force } => commands::init::run(force),
        Commands::Lang { file } => commands::lang::run(file),
        Commands::InstallSkills => not_yet("aristo install-skills", "slice 13"),
        Commands::UninstallSkills => not_yet("aristo uninstall-skills", "slice 13"),
        Commands::Index => not_yet("aristo index", "slice 16"),
        Commands::Stamp => not_yet("aristo stamp", "slice 17"),
        Commands::Show => not_yet("aristo show", "slice 18"),
        Commands::List => not_yet("aristo list", "slice 18"),
        Commands::Status => not_yet("aristo status", "slice 19"),
        Commands::Lint => not_yet("aristo lint", "slice 20"),
        Commands::Verify => not_yet("aristo verify", "slice 22"),
        Commands::Review => not_yet("aristo review", "slice 27"),
        Commands::Doc => not_yet("aristo doc", "slice 28"),
        Commands::Graph => not_yet("aristo graph", "slice 29"),
        Commands::Badge => not_yet("aristo badge", "slice 31"),
        Commands::Rename => not_yet("aristo rename", "slice 32"),
    }
}

fn not_yet(what: &'static str, slice: &'static str) -> CliResult<()> {
    Err(CliError::NotImplemented { what, slice })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_parser_construction_is_valid() {
        // clap performs a structural sanity check (e.g. no duplicate
        // subcommand names) when CommandFactory::command() runs. We assert
        // it succeeds rather than panicking at runtime when a user types
        // `--help`. Cheap canary against future enum-shape mistakes.
        Cli::command().debug_assert();
    }

    #[test]
    fn dispatch_returns_not_implemented_with_slice_pointer() {
        // Spot-check one not-yet-implemented variant; the implemented
        // ones (Init, Lang as of slices 10, 11) are covered by their own tests.
        let err = dispatch(Commands::Index).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristo index"), "msg: {msg}");
        assert!(msg.contains("slice 16"), "msg: {msg}");
    }

    #[test]
    fn every_unimplemented_subcommand_dispatches_to_a_distinct_slice() {
        // Catches the easy mistake of copy-pasting a stub and forgetting
        // to update the slice pointer. Implemented commands (Init, Lang) are
        // tested elsewhere.
        let variants = [
            (Commands::InstallSkills, "slice 13"),
            (Commands::UninstallSkills, "slice 13"),
            (Commands::Index, "slice 16"),
            (Commands::Stamp, "slice 17"),
            (Commands::Show, "slice 18"),
            (Commands::List, "slice 18"),
            (Commands::Status, "slice 19"),
            (Commands::Lint, "slice 20"),
            (Commands::Verify, "slice 22"),
            (Commands::Review, "slice 27"),
            (Commands::Doc, "slice 28"),
            (Commands::Graph, "slice 29"),
            (Commands::Badge, "slice 31"),
            (Commands::Rename, "slice 32"),
        ];
        for (cmd, expected_slice) in variants {
            let err = dispatch(cmd).unwrap_err();
            assert!(
                err.to_string().contains(expected_slice),
                "expected `{expected_slice}` in message; got: {err}"
            );
        }
    }
}
