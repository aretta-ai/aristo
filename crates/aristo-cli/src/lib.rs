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
mod pipeline;
mod preflight;
mod skills;
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
    InstallSkills {
        /// Target agent. Required unless `--list-agents` is used.
        #[arg(long, value_name = "name")]
        agent: Option<String>,
        /// List supported agents and exit.
        #[arg(long)]
        list_agents: bool,
        /// Install at user-level (e.g. ~/.claude/skills/) instead of project-level.
        #[arg(long)]
        user: bool,
        /// Force reinstall (re-pinning to current SDK version).
        #[arg(long)]
        update: bool,
    },

    /// Reverse `install-skills`: remove SDK-bundled skills.
    UninstallSkills {
        /// Target agent. Required.
        #[arg(long, value_name = "name")]
        agent: String,
        /// Uninstall from user-level instead of project-level.
        #[arg(long)]
        user: bool,
        /// Override the "skip locally-modified" safety check.
        #[arg(long)]
        force: bool,
    },

    /// Walk source, parse via syn, write .aristo/index.toml.
    Index {
        /// Force a full re-walk, ignoring the per-file mtime cache (slice 17+).
        #[arg(long)]
        all: bool,
    },

    /// Index + ID assignment + drift detection + (Phase 2) B5b classification.
    Stamp {
        /// CI mode: report whether stamp would change the index, but
        /// don't write. Exits non-zero if changes are needed.
        #[arg(long)]
        check: bool,
    },

    /// Look up an annotation by id, fn / mod / struct name, or file:line.
    Show {
        /// Selector: bare id, `fn <name>`, `mod <name>`, `struct <name>`,
        /// `enum <name>`, `trait <name>`, or `<file>:<line>`.
        selector: String,
        /// Emit the entry as JSON instead of human-readable text.
        #[arg(long, conflicts_with = "toml_out")]
        json: bool,
        /// Emit the entry as TOML (mirrors the on-disk index schema).
        #[arg(long = "toml", conflicts_with = "json")]
        toml_out: bool,
    },

    /// Flat enumeration of all annotations.
    List {
        /// J2 unified filter clause (`id=<id>`, `file=<path>`,
        /// `parent=<id>`, `status=<state>`). Repeatable; multiple
        /// `--filter` flags AND together.
        #[arg(long = "filter", value_name = "key=value")]
        filters: Vec<String>,
        /// Emit a JSON array of records instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Project-level summary (tier, counts, freshness).
    Status,

    /// Static text-quality pass over annotations (rule-based; no LLM).
    Lint {
        /// Read-only mode: exit non-zero on `error` findings (or
        /// `warn` with `--strict`); never modifies source.
        #[arg(long)]
        check: bool,
        /// Apply auto-fixable rules to source files in place (slice 20 C2).
        #[arg(long, conflicts_with = "check")]
        fix: bool,
        /// Treat `warn` severity as failure too (only meaningful with `--check`).
        #[arg(long)]
        strict: bool,
    },

    /// Run verification for every annotation that opted in.
    Verify {
        /// J2 unified filter clause (`id=<id>`, `file=<path>`,
        /// `parent=<id>`, `status=<state>`). Repeatable; multiple
        /// `--filter` flags AND together.
        #[arg(long = "filter", value_name = "key=value")]
        filters: Vec<String>,
        /// Re-verify entries already in a clean verified state.
        /// Default is to skip clean entries; `--rerun` overrides for
        /// post-key-rotation sweeps.
        #[arg(long)]
        rerun: bool,
        /// CI mode: report whether any status would change but do not
        /// write the index. Exits non-zero if any changes are needed.
        #[arg(long)]
        check: bool,
        /// Treat warn-severity verification outcomes as failure too.
        #[arg(long)]
        strict: bool,
        /// Apply pending verdict files in `.aristo/proofs/` to the index.
        /// Reads every `<id>.proof`, runs the mechanical validator, and
        /// (if it passes) flips the entry's status. Skips dispatch of
        /// new verifications when set.
        #[arg(long = "apply-verdicts", conflicts_with = "submit_verdict")]
        apply_verdicts: bool,
        /// Migration-only: ignore any agent-stamped ground hashes in the
        /// `.proof` files and recompute them from the cited file ranges
        /// and index entries. Use this to migrate proofs written under
        /// the pre-validator-fills-hashes schema. Without this flag, a
        /// stamped hash that mismatches the current source is reported
        /// as staleness and the proof is rejected. Only meaningful with
        /// `--apply-verdicts`.
        #[arg(long = "rewrite-hashes", requires = "apply_verdicts")]
        rewrite_hashes: bool,
        /// Subagent write-path for a single verdict: parse the JSON
        /// payload, run the mechanical validator, and (on pass) write
        /// `.aristo/proofs/<id>.proof` atomically. Prints
        /// `accepted: sha256:<hex>` to stdout on success; structured
        /// errors to stderr on reject. The SDK is the sole writer of
        /// `.proof` files — agents never write them directly.
        #[arg(long = "submit-verdict", requires = "id", requires = "json")]
        submit_verdict: bool,
        /// Annotation id that this verdict is about. Required with
        /// `--submit-verdict`. The `.proof` file lands at
        /// `.aristo/proofs/<id>.proof` (with `:` → `__`).
        #[arg(long = "id", requires = "submit_verdict")]
        id: Option<String>,
        /// JSON-serialized ProofFile body. Required with
        /// `--submit-verdict`. Pass as a single-quoted shell string;
        /// the SDK parses it into a ProofFile and rejects anything
        /// the validator would reject. Same schema as the TOML body
        /// that gets written on accept.
        #[arg(long = "json", requires = "submit_verdict")]
        json: Option<String>,
        /// Worker-facing API: atomically claim one task from the
        /// pending queue and print its TOML body to stdout. Empty
        /// stdout means the queue is drained (still exits 0). Verify
        /// workers are ONE-SHOT (call once, process the task, exit)
        /// to avoid context pollution across verifications. The
        /// orchestrator waves N workers in parallel and uses
        /// `--queue-status` to decide whether to spawn the next wave.
        #[arg(long = "pop-next", conflicts_with_all = ["apply_verdicts", "submit_verdict", "queue_status"])]
        pop_next: bool,
        /// Peek at queue state without claiming. Prints `pending: N`
        /// + `claimed: M` to stdout, exit 0. Used by the orchestrator
        /// to decide whether to dispatch another wave of workers.
        /// Non-destructive — multiple callers do not race.
        #[arg(long = "queue-status", conflicts_with_all = ["apply_verdicts", "submit_verdict"])]
        queue_status: bool,
    },

    /// Agentic prose-improvement pass via the critique skill.
    /// (Named "critique" rather than "review" because the output is
    /// opinionated suggestions on annotation prose — categorized
    /// findings with severity tags — not neutral inspection. Avoids
    /// the false analogy to PR / code review where humans sign off.)
    Critique {
        /// J2 unified filter clause (`id=<id>[,<id>,...]`,
        /// `file=<path>`, `parent=<id>`, `status=<state>`). Repeatable;
        /// multiple `--filter` flags AND together; values may be
        /// comma-separated. **REQUIRED** — `aristo critique` with no
        /// filter errors with usage. (No implicit codebase sweep per
        /// `docs/decisions/critique-and-pipeline-architecture.md` §D6.)
        #[arg(long = "filter", value_name = "key=value")]
        filters: Vec<String>,
        /// Apply pending critique files in `.aristo/critiques/` —
        /// re-validate every `<id>.critique` and print a summary
        /// grouped by id. v0 is read+summary only; v1 will stamp
        /// `last_critiqued_at_text_hash` into the index for caching.
        #[arg(long = "apply-findings", conflicts_with_all = ["submit_findings", "pop_next", "queue_status"])]
        apply_findings: bool,
        /// Worker-facing API: atomically claim one task from the
        /// critique queue and print its TOML body to stdout. Empty
        /// stdout means the queue is drained (still exits 0). Critique
        /// workers loop on this call (shallow tasks; vocabulary
        /// alignment benefits from cross-task context).
        #[arg(long = "pop-next", conflicts_with_all = ["apply_findings", "submit_findings", "queue_status"])]
        pop_next: bool,
        /// Peek at queue state without claiming. Prints `pending: N`
        /// + `claimed: M` to stdout, exit 0.
        #[arg(long = "queue-status", conflicts_with_all = ["apply_findings", "submit_findings"])]
        queue_status: bool,
        /// Subagent write-path for a single critique: parse the JSON
        /// payload, run the schema validator, write
        /// `.aristo/critiques/<id>.critique` atomically on accept.
        /// Prints `accepted: sha256:<hex>` to stdout.
        #[arg(long = "submit-findings", requires = "id", requires = "json")]
        submit_findings: bool,
        /// Annotation id this submission is about. Required with
        /// `--submit-findings`.
        #[arg(long = "id", requires = "submit_findings")]
        id: Option<String>,
        /// JSON-serialized CritiqueFile body. Required with
        /// `--submit-findings`.
        #[arg(long = "json", requires = "submit_findings")]
        json: Option<String>,
    },

    /// Generate per-annotation markdown to .aristo/doc/.
    Doc {
        /// Write only the crate-root summary (`_summary.md`); skip the
        /// per-annotation pass.
        #[arg(long)]
        summary: bool,
        /// Bake current B5b verification status into rendered MD. Status
        /// is a build-time fact and will go stale as code evolves; the
        /// default omits it so doc artifacts stay reproducible on a
        /// clean checkout.
        #[arg(long = "include-status")]
        include_status: bool,
        /// CI mode: recompute expected per-annotation MD from the index,
        /// compare against `.aristo/doc/`, exit non-zero on drift. Never
        /// writes.
        #[arg(long)]
        check: bool,
    },

    /// Generate the annotation graph (Mermaid / DOT / SVG).
    Graph,

    /// Generate a shareable SVG verification badge for README / docs.
    Badge {
        /// Write SVG to this path (relative to workspace root, or absolute).
        /// Default: stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Badge style. Mirrors shields.io's three default forms.
        #[arg(long, default_value = "flat")]
        style: String,
    },

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
            if !e.is_silent() {
                eprintln!("error: {e}");
            }
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
        Commands::InstallSkills {
            agent,
            list_agents,
            user,
            update,
        } => commands::install_skills::install(agent, list_agents, user, update),
        Commands::UninstallSkills { agent, user, force } => {
            commands::install_skills::uninstall(agent, user, force)
        }
        Commands::Index { all } => commands::index::run(all),
        Commands::Stamp { check } => commands::stamp::run(check),
        Commands::Show {
            selector,
            json,
            toml_out,
        } => commands::show::run(&selector, output_mode(json, toml_out)),
        Commands::List { filters, json } => commands::list::run(&filters, json),
        Commands::Status => commands::status::run(),
        Commands::Lint { check, fix, strict } => commands::lint::run(check, fix, strict),
        Commands::Verify {
            filters,
            rerun,
            check,
            strict,
            apply_verdicts,
            rewrite_hashes,
            submit_verdict,
            id,
            json,
            pop_next,
            queue_status,
        } => commands::verify::run(
            &filters,
            rerun,
            check,
            strict,
            apply_verdicts,
            rewrite_hashes,
            submit_verdict,
            pop_next,
            queue_status,
            id,
            json,
        ),
        Commands::Critique {
            filters,
            apply_findings,
            pop_next,
            queue_status,
            submit_findings,
            id,
            json,
        } => commands::critique::run(
            &filters,
            submit_findings,
            pop_next,
            queue_status,
            apply_findings,
            id,
            json,
        ),
        Commands::Doc {
            summary,
            include_status,
            check,
        } => commands::doc::run(summary, include_status, check),
        Commands::Graph => not_yet("aristo graph", "slice 29"),
        Commands::Badge { out, style } => {
            let style =
                commands::badge::Style::parse(&style).map_err(|message| CliError::Other {
                    message,
                    exit_code: 2,
                })?;
            commands::badge::run(out, style)
        }
        Commands::Rename => not_yet("aristo rename", "slice 32"),
    }
}

fn not_yet(what: &'static str, slice: &'static str) -> CliResult<()> {
    Err(CliError::NotImplemented { what, slice })
}

fn output_mode(json: bool, toml_out: bool) -> commands::show::OutputMode {
    if json {
        commands::show::OutputMode::Json
    } else if toml_out {
        commands::show::OutputMode::Toml
    } else {
        commands::show::OutputMode::Text
    }
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
        // ones are covered by their own tests.
        let err = dispatch(Commands::Graph).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristo graph"), "msg: {msg}");
        assert!(msg.contains("slice 29"), "msg: {msg}");
    }

    #[test]
    fn every_unimplemented_subcommand_dispatches_to_a_distinct_slice() {
        // Catches the easy mistake of copy-pasting a stub and forgetting
        // to update the slice pointer. Implemented commands (Init, Lang,
        // Index, Stamp, Show, List, Status, Lint, Verify, Critique, Doc)
        // are tested elsewhere. `Doc` with no flags still returns
        // NotImplemented during slice 28's incremental build-out (only
        // `--summary` ships in the first slice-28 commit); that variant
        // is exercised by `binary_smoke::defined_but_unimplemented_subcommand_exits_64`
        // for now and migrates here once `aristo doc` (no flags) is
        // fully implemented.
        let variants = [
            (Commands::Graph, "slice 29"),
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
