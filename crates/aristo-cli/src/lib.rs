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
mod session;
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
        /// don't write. Exits non-zero if changes are needed. Also
        /// suppresses the canon-match step (CI doesn't need to make
        /// outbound network calls).
        #[arg(long)]
        check: bool,
        /// Skip the canon-match step for this invocation. Doesn't
        /// disable canon globally — see `aristo.toml [canon] enabled
        /// = false` for the project-level opt-out. Useful for local
        /// stamps when offline or for snappy iteration.
        #[arg(long = "skip-canon")]
        skip_canon: bool,
        /// Invalidate the local canon-match cache and re-query every
        /// annotation on this run. Equivalent to
        /// `aristo canon refresh && aristo stamp`.
        #[arg(long = "refresh-canon", conflicts_with = "skip_canon")]
        refresh_canon: bool,
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
        /// Peek at queue state without claiming. Prints `pending: N`,
        /// `claimed: M` to stdout, exit 0. Used by the orchestrator
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
        /// grouped by id. Defaults to listing only findings whose
        /// `disposition` is `None` (open / not yet reviewed); pass
        /// `--include-closed` for the full view including findings
        /// already triaged via `aristo session decide`.
        #[arg(long = "apply-findings", conflicts_with_all = ["submit_findings", "pop_next", "queue_status"])]
        apply_findings: bool,
        /// Include findings whose `disposition` has been set (Accepted /
        /// Rejected / Deferred) in the `--apply-findings` summary.
        /// Default is to filter to open findings only — closed findings
        /// stop re-surfacing on every apply, which is how the review
        /// substrate closes the loop. Only meaningful with
        /// `--apply-findings`.
        #[arg(long = "include-closed", requires = "apply_findings")]
        include_closed: bool,
        /// Force re-enqueue of every matched annotation, bypassing the
        /// `last_critiqued_at_text_hash` cache. Default behavior skips
        /// annotations whose text hasn't drifted since the cached
        /// critique was produced (so re-runs of `aristo critique
        /// --filter id=X` are free when X is unchanged).
        #[arg(long = "rerun")]
        rerun: bool,
        /// Restrict scope to annotations in files git-staged for the
        /// next commit (`git diff --cached --name-only`). Useful for
        /// pre-commit hook integration. Satisfies the filter-required
        /// guard on its own; composes with explicit `--filter`
        /// clauses via intersection (annotations must match BOTH
        /// `--filter` and appear in the staged set).
        #[arg(long = "staged")]
        staged: bool,
        /// Opt-in sweep over every IntentEntry with a real verify
        /// method (excludes documentation-only `verify = false`).
        /// Loud: prints `(this will enqueue N annotations; ~$X cost
        /// — proceed with --all --yes?)` and exits 2 unless `--yes`
        /// is also passed. The cost gate is load-bearing: a sweep
        /// without confirmation lets the agent accidentally fire
        /// hundreds of LLM calls in one bash invocation.
        #[arg(long = "all", conflicts_with_all = ["filters", "staged"])]
        all: bool,
        /// Skip the confirmation prompt for `--all`. Required alongside
        /// `--all` to actually enqueue the sweep; without it `--all`
        /// just prints the cost estimate and exits 2.
        #[arg(long = "yes", requires = "all")]
        yes: bool,
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
        /// Composite: also generate the annotation graph (Mermaid)
        /// and embed it inline in `_summary.md`. Implies `--summary`.
        /// Conflicts with `--check` (read-only mode can't write the
        /// graph block).
        #[arg(long = "include-graph", conflicts_with = "check")]
        include_graph: bool,
    },

    /// Generate the annotation graph (Mermaid / DOT / SVG).
    Graph {
        /// Output format. `mermaid` (default) emits a fenced
        /// flowchart TD block; `dot` emits Graphviz DOT; `svg`
        /// requires `dot` on PATH and shells out to render.
        #[arg(long, default_value = "mermaid")]
        format: String,
        /// Write to this path instead of stdout. Atomic via
        /// temp-file + rename. Relative paths resolve against the
        /// invoking directory.
        #[arg(long)]
        out: Option<PathBuf>,
        /// J2 unified filter clause (`id=<id>`, `file=<path>[:<LO>-<HI>]`,
        /// `parent=<id>`, `status=<state>`). Repeatable; multiple
        /// `--filter` flags AND together. Default scope (no filter)
        /// is the whole index.
        #[arg(long = "filter", value_name = "key=value")]
        filters: Vec<String>,
        /// Drop `assume` nodes from the rendered graph. Default is
        /// to include them — assumes form the "ground" of the
        /// property graph and dropping them by default would hide
        /// load-bearing background facts.
        #[arg(long = "exclude-assumes")]
        exclude_assumes: bool,
        /// Walk N hops from each filter-matched node in both
        /// directions (ancestors + descendants) and include them in
        /// the rendered graph. Useful for "show me this annotation +
        /// some context". Only meaningful with `--filter`; without
        /// a filter, the default scope is already the whole index.
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        /// Include intent nodes that are graph-orphans (no parent
        /// link AND no child links). Default OMITS them — they're
        /// usually atomic load-bearing claims that don't add visual
        /// structure to the graph. Assumes are always included
        /// regardless (they're contextual "ground"; see
        /// `--exclude-assumes` for that opt-out).
        #[arg(long = "include-orphans")]
        include_orphans: bool,
        /// Swap the color axis from verify level to current B5b
        /// status (verified=green / tested=blue / neural=yellow /
        /// stale=orange / orphan=purple / forged=red+border /
        /// unknown=gray / counterexample=red+border /
        /// inconclusive=red+border / pending-deepen=gray). Verify
        /// level moves to the in-node label. Use when the dominant
        /// question is "what's still unverified".
        #[arg(long = "include-status")]
        include_status: bool,
    },

    /// Generate a shareable SVG verification badge for README / docs.
    Badge {
        /// Write SVG to this path (relative to workspace root, or absolute).
        /// Default: stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Badge style. Mirrors shields.io's three default forms.
        #[arg(long, default_value = "flat")]
        style: String,
        /// Which metric the SVG value half displays. `tier` (default,
        /// the locked D7 score → D8 tier) is the headline signal;
        /// `count` and `rate` preserve the slice-31 surfaces for
        /// projects that prefer the simpler counters.
        #[arg(long, default_value = "tier")]
        metric: String,
    },

    /// Atomic project-wide rename of an annotation id.
    ///
    /// Scope (slice 32): bare → bare and stamp-assigned opaque
    /// (`aret_*`) → bare. The canon-bound namespaces (`aristos:` and
    /// `kanon:`) are rejected in either direction per §CS13 — canon
    /// prefixes are applied by the canon accept path and removed by
    /// `aristo canon unbind`. Opaque-target ids are rejected per F1-b
    /// (stamp-assigned only). See `docs/decisions/...` (slice 32) and
    /// `../aretta-sdk/docs/launch/canon-strategy.md` §CS13.
    Rename {
        /// Annotation id to rename FROM. Must exist in the current
        /// `.aristo/index.toml`.
        old_id: String,
        /// Annotation id to rename TO. Must not already exist and must
        /// not use the reserved `aret_*` / `aristos:` / `kanon:`
        /// prefixes.
        new_id: String,
        /// Compute and print the rename plan (source edits + per-id
        /// artifact moves + index updates) without writing anything.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Stateful review-session substrate — start / inspect / decide /
    /// exit on the in-flight review of a pipeline's reviewable
    /// artifacts. See `docs/decisions/review-sessions.md`.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Authenticate against the Aretta canon API. Required for
    /// `aristo stamp` / `aristo critique` to surface canon matches on
    /// Pro / Enterprise tiers. See canon-strategy.md §CS1.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Canon-binding operations on the SDK side. The user-facing
    /// surface for the §13 canon-and-matching design — accept a
    /// pending match (PR #7); show / list / refresh / unbind /
    /// request-verify subcommands land in PR #8/#9.
    Canon {
        #[command(subcommand)]
        action: CanonAction,
    },
}

/// Subcommands under `aristo auth`. Each operates on the persistent
/// credentials store under `$XDG_CONFIG_HOME/aristo/credentials`
/// (or the platform default per `aristo_core::canon::auth`).
#[derive(clap::Subcommand, Debug)]
pub(crate) enum AuthAction {
    /// Authenticate by accepting a Bearer token.
    ///
    /// Three input modes (mutually exclusive):
    ///
    /// - **interactive (default):** print a prompt, then read the token
    ///   from stdin (one line). Suitable for human terminal use.
    /// - **`--stdin`:** read the entire stdin and use it as the token.
    ///   Useful for piping (`echo "$TOKEN" | aristo auth login --stdin`).
    /// - **`--token=<T>`:** use the literal value as the token. NOT
    ///   recommended for interactive use — the token appears in shell
    ///   history. Provided for scripting and tests.
    ///
    /// The token is persisted to `$XDG_CONFIG_HOME/aristo/credentials`
    /// with `0600` Unix permissions; the same file is read by every
    /// canon API call on subsequent runs.
    Login {
        /// Read the token from stdin (consumes entire stdin).
        /// Useful for piped credential input in CI.
        #[arg(long, conflicts_with = "token")]
        stdin: bool,
        /// Use this token directly (bypasses prompt and stdin).
        /// Provided for scripting / tests; not recommended for
        /// interactive use because the value appears in shell history.
        #[arg(long, value_name = "TOKEN")]
        token: Option<String>,
    },
    /// Show the current authentication state (does NOT print the
    /// token, only its source: env var / credentials file / none).
    /// Useful for sanity-checking before running `aristo stamp`.
    Status,
    /// Remove the stored credentials file. Idempotent — running
    /// `logout` when not logged in is not an error.
    Logout,
}

/// Subcommands under `aristo canon`. Phase-1 lands `accept` only;
/// `show` / `list` / `refresh` / `unbind` / `request-verify` follow
/// in PR #8 + PR #9.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum CanonAction {
    /// Accept a pending canon match: rewrite source to use the
    /// canonical text + apply the `aristos:` / `kanon:` prefix to
    /// the annotation id, update the index entry's binding state
    /// to `Bound`, and move the cache entry from `pending_matches`
    /// to `accepted_matches`.
    ///
    /// Both arguments are required: the bare annotation id as it
    /// appears in `.aristo/index.toml` (NOT prefixed; the prefix
    /// is applied by accept) and the bare canon id from the
    /// pending match (e.g. `cell_written_exactly_once_per_page_edit`).
    Accept {
        /// Annotation id whose pending match you're accepting. Use
        /// the bare form (no `aristos:` / `kanon:` prefix); the
        /// prefix is applied by the accept itself based on the
        /// pending match's `prefix_tier`.
        annotation_id: String,
        /// Canon id from the pending match (also bare — no
        /// prefix). The pair `(annotation_id, canon_id)` locates
        /// the exact pending match in `.aristo/canon-matches.toml`.
        canon_id: String,
    },

    /// Reject a pending canon match: move the entry from
    /// `pending_matches` to `rejected_matches`, pinned by the
    /// current annotation `text_hash`. The rejection suppresses
    /// re-surfacing the same `(canon_id, text_hash)` tuple on
    /// future `aristo stamp` runs; once the annotation text
    /// changes, the rejection no longer applies and the match
    /// is re-evaluated. Source and index are NOT touched —
    /// rejection is a cache-only operation. Closes Flow 7 from
    /// cli-sessions.md.
    Reject {
        /// Annotation id whose pending match you're rejecting.
        annotation_id: String,
        /// Canon id from the pending match.
        canon_id: String,
        /// Optional free-text rationale recorded in the cache
        /// entry's `reason` field. Useful for documenting
        /// "this canon entry is too broad" / "wrong category"
        /// decisions for future reviewers.
        #[arg(long = "reason")]
        reason: Option<String>,
    },

    /// List the current canon match state: one line per annotation
    /// with pending / accepted / rejected counts, plus per-bucket
    /// detail lines for each match. Read-only over
    /// `.aristo/canon-matches.toml`.
    List,

    /// Fetch the canon entry detail for `<canon_id>` via the canon
    /// API and render the longer description + example + references.
    /// The user-facing trust card (which combines this data with the
    /// local index entry's site / binding) lives at `aristo show
    /// <bound_id>` (PR #10).
    Show {
        /// Bare canon id (no `aristos:` / `kanon:` prefix). The
        /// server's `GET /canon/entry/<canon_id>` endpoint returns
        /// the same entry regardless of which tier you'd bind into;
        /// the prefix is a per-user, per-scope attribute.
        canon_id: String,
        /// Optional explicit version (`v<minor>.<patch>`). Omit to
        /// get the catalog's active version per CS12.
        #[arg(long = "version")]
        version: Option<String>,
    },

    /// Force-refresh the canon match cache: re-run the
    /// `POST /canon/match` call for every annotation in the index
    /// regardless of cached `last_match_text_hash`. Equivalent to
    /// `aristo stamp --refresh-canon`, but without re-running the
    /// stamp pipeline (no source walk, no body-hash drift check,
    /// no index rewrite). Useful when the user knows a new catalog
    /// version has shipped and wants fresh matches without a stamp.
    Refresh,

    /// Reverse of `aristo canon accept`: strip the `aristos:` /
    /// `kanon:` prefix from a canon-bound annotation, revert its
    /// binding to `Local`, and drop the accepted_matches cache
    /// entry. Source is rewritten in place (only the `id =` value
    /// changes; canonical text + verify + parent are preserved).
    /// The next `aristo stamp` may re-pull a fresh pending match
    /// against the same annotation text.
    Unbind {
        /// Canon-bound annotation id including the prefix (e.g.
        /// `aristos:cell_written_exactly_once_per_page_edit`).
        prefixed_id: String,
    },

    /// Record a verification demand signal against a canon entry.
    /// Idempotent on `(canon_id, repo, user)` per canon-strategy.md
    /// §CS11. Use when an annotation is bound at the `kanon:` tier
    /// and the user wants Aretta to invest in a verifier for that
    /// canon entry.
    RequestVerify {
        /// Canon id (no prefix). The same id the trust card surfaces
        /// or that `aristo canon list` shows.
        canon_id: String,
        /// Optional free-text rationale to attach to the demand
        /// signal (e.g., "critical for our financial-tx audit"). On
        /// repeat calls with a new note, replaces the previous one
        /// server-side.
        #[arg(long = "notes")]
        notes: Option<String>,
    },
}

/// Subcommands under `aristo session`. Each maps to one substrate
/// operation; per-kind side effects (e.g. mutating a `.critique`
/// file on accept) plug in via the `SessionKind` trait wired in
/// step 5.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum SessionAction {
    /// Begin a fresh review session of the given kind. Fails if a
    /// session is already active and `--allow-nesting` was not
    /// permitted by the kind.
    Start {
        /// Session kind (`critique-review`, `proof-review`).
        kind: String,
        /// Free-form display label for the focal artifact under review
        /// (e.g. `src/critique/pending.rs` or
        /// `proof:balance_no_duplicate_cells`).
        #[arg(long = "subject")]
        subject: String,
        /// Override the kind's default nesting policy. v0 ships
        /// `Disallow` as the only policy; the flag is reserved for
        /// future per-kind opt-ins (design Q4).
        #[arg(long = "allow-nesting", default_value_t = false)]
        allow_nesting: bool,
    },
    /// Print the active session id (or empty stdout if none).
    /// Exit 0 either way.
    Active {
        /// Emit the full `<system-reminder>` block instead of just
        /// the id — for the `UserPromptSubmit` hook installed by
        /// `aristo install-skills`. Empty stdout when no session
        /// is active (the hook then injects nothing).
        #[arg(long = "hook-format", default_value_t = false)]
        hook_format: bool,
    },
    /// Print bucket counts + open items for the active session.
    /// Exit 0; errors out if no session is active.
    Status,
    /// Record a decision on one item in the active session.
    Decide {
        /// Item reference (`<id>#<index>` for indexed items, or any
        /// opaque per-kind string).
        #[arg(long = "item")]
        item: String,
        /// Which bucket the item lands in.
        #[arg(long = "bucket", value_enum)]
        bucket: BucketArg,
        /// Optional free-text reason captured with the decision.
        #[arg(long = "note")]
        note: Option<String>,
    },
    /// Close the active session. Strict by default — errors out if
    /// any items are still in the open bucket.
    Exit {
        /// Move open items to the per-kind backlog instead of
        /// erroring. Items are NEVER silently dropped; the next
        /// session of this kind surfaces them via the backlog menu
        /// (design doc D7).
        #[arg(long = "defer-undecided", default_value_t = false)]
        defer_undecided: bool,
    },
    /// Destructive cancel: drop the session entirely with no
    /// decisions recorded. Requires `--yes` to skip the
    /// confirmation prompt.
    Abort {
        /// Skip the confirmation prompt.
        #[arg(long = "yes", default_value_t = false)]
        yes: bool,
    },
    /// List the active session and the most recent N closed sessions.
    List {
        /// Maximum number of closed-session rows to include.
        #[arg(long = "limit", default_value_t = 10)]
        limit: usize,
    },
}

/// User-facing bucket choices for `aristo session decide`. Maps to
/// the substrate's [`session::types::ItemStatus`] (minus `Open`,
/// which is the implicit pre-decision state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum BucketArg {
    Accepted,
    Rejected,
    Pending,
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
        Commands::Stamp {
            check,
            skip_canon,
            refresh_canon,
        } => commands::stamp::run(check, skip_canon, refresh_canon),
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
            include_closed,
            rerun,
            staged,
            all,
            yes,
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
            include_closed,
            rerun,
            staged,
            all,
            yes,
            id,
            json,
        ),
        Commands::Doc {
            summary,
            include_status,
            check,
            include_graph,
        } => commands::doc::run(summary, include_status, check, include_graph),
        Commands::Graph {
            format,
            out,
            filters,
            exclude_assumes,
            depth,
            include_orphans,
            include_status,
        } => commands::graph::run(
            &format,
            out,
            &filters,
            exclude_assumes,
            depth,
            include_orphans,
            include_status,
        ),
        Commands::Badge { out, style, metric } => {
            let style =
                commands::badge::Style::parse(&style).map_err(|message| CliError::Other {
                    message,
                    exit_code: 2,
                })?;
            let metric =
                commands::badge::Metric::parse(&metric).map_err(|message| CliError::Other {
                    message,
                    exit_code: 2,
                })?;
            commands::badge::run(out, style, metric)
        }
        Commands::Rename {
            old_id,
            new_id,
            dry_run,
        } => commands::rename::run(&old_id, &new_id, dry_run),
        Commands::Session { action } => commands::session::run(action),
        Commands::Auth { action } => commands::auth::run(action),
        Commands::Canon { action } => match action {
            CanonAction::Accept {
                annotation_id,
                canon_id,
            } => commands::canon::accept::run(&annotation_id, &canon_id),
            CanonAction::Reject {
                annotation_id,
                canon_id,
                reason,
            } => commands::canon::reject::run(&annotation_id, &canon_id, reason),
            CanonAction::List => commands::canon::list::run(),
            CanonAction::Show { canon_id, version } => {
                commands::canon::show::run(&canon_id, version)
            }
            CanonAction::Refresh => commands::canon::refresh::run(),
            CanonAction::Unbind { prefixed_id } => commands::canon::unbind::run(&prefixed_id),
            CanonAction::RequestVerify { canon_id, notes } => {
                commands::canon::request_verify::run(&canon_id, notes)
            }
        },
    }
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

    // Note: slice 32 removed the last `not_yet(...)` stub (Rename now
    // has a real implementation). The `CliError::NotImplemented` variant
    // is kept for future stubs but is no longer reachable from dispatch.
}
