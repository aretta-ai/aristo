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
/// Each subcommand handles one stage of the annotation lifecycle:
/// authoring (`init`, `lang`, `install-skills`), indexing and stamping
/// (`index`, `stamp`, `rename`), inspection (`show`, `list`, `status`,
/// `graph`, `doc`, `badge`), quality gates (`lint`, `verify`,
/// `critique`), review-session management (`session`), and canon
/// binding against the Aretta server (`auth`, `canon`).
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
    /// Set up Aristo in this repo (creates `aristo.toml`, `.aristo/`,
    /// pre-commit hook, CI workflow).
    Init {
        /// Modify Cargo.toml to add `aristo` as a dependency. Default
        /// behavior just prints the dep line for the user to paste in.
        #[arg(short, long)]
        force: bool,
    },

    /// Print a syntax cheat sheet for the detected language.
    Lang {
        /// Detect language for a specific file. Currently only Rust
        /// (`.rs`) is supported; other extensions error out.
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

    /// Scan source for annotations and write the index (`.aristo/index.toml`).
    Index {
        /// Force a full re-walk, ignoring the per-file mtime cache.
        #[arg(long)]
        all: bool,
    },

    /// Refresh the annotation index — pick up new annotations, detect
    /// drift, and (when signed in) match against the Aretta canon.
    Stamp {
        /// CI mode: report whether `stamp` would change the index,
        /// without writing. Exits non-zero if it would. Skips the
        /// canon-match step too (no outbound network calls in this
        /// mode).
        #[arg(long)]
        check: bool,
        /// Skip the canon-match step for this run. Doesn't disable
        /// canon globally — set `[canon] enabled = false` in
        /// `aristo.toml` for that. Useful when you're offline or want
        /// a fast local stamp.
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

    /// List every annotation in the index.
    List {
        /// Unified filter clause (`id=<id>`, `file=<path>`,
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

    /// Check annotation prose for quality issues (rule-based; no LLM).
    Lint {
        /// Read-only mode: exit non-zero on `error` findings (or
        /// `warn` with `--strict`); never modifies source.
        #[arg(long)]
        check: bool,
        /// Apply auto-fixable lint rules (whitespace only) to source
        /// files in place.
        #[arg(long, conflicts_with = "check")]
        fix: bool,
        /// Treat `warn` severity as failure too (only meaningful with `--check`).
        #[arg(long)]
        strict: bool,
    },

    /// Run verification for every annotation that opted in.
    Verify {
        /// Unified filter clause (`id=<id>`, `file=<path>`,
        /// `parent=<id>`, `status=<state>`). Repeatable; multiple
        /// `--filter` flags AND together.
        #[arg(long = "filter", value_name = "key=value")]
        filters: Vec<String>,
        /// Re-verify entries that are already in a clean verified
        /// state. By default they're skipped; pass `--rerun` to force
        /// a re-check.
        #[arg(long)]
        rerun: bool,
        /// CI mode: report whether any status would change, without
        /// writing the index. Exits non-zero if any change is needed.
        #[arg(long)]
        check: bool,
        /// Treat warn-severity verification outcomes as failure too.
        #[arg(long)]
        strict: bool,
        /// Apply pending verdict files in `.aristo/proofs/` to the
        /// index. Reads every `<id>.proof`, runs the mechanical
        /// validator, and (if it passes) flips the entry's status.
        /// Skips dispatch of new verifications when set.
        #[arg(long = "apply-verdicts", conflicts_with = "submit_verdict")]
        apply_verdicts: bool,
        /// Migration only: ignore any agent-stamped ground hashes in
        /// the `.proof` files and recompute them from the cited file
        /// ranges and index entries. Use this once when migrating
        /// from older proof files that recorded hashes the SDK now
        /// fills in itself. Without this flag, a stamped hash that
        /// mismatches the current source is reported as staleness
        /// and the proof is rejected. Only meaningful with
        /// `--apply-verdicts`.
        #[arg(long = "rewrite-hashes", requires = "apply_verdicts")]
        rewrite_hashes: bool,
        /// **Internal — invoked by the verification skill.** Submit
        /// a single verdict: parse the JSON payload, validate it,
        /// and (on pass) atomically write `.aristo/proofs/<id>.proof`.
        /// Prints `accepted: sha256:<hex>` on success; structured
        /// errors on reject. Agents never write `.proof` files
        /// directly — the SDK is the sole writer.
        #[arg(long = "submit-verdict", requires = "id", requires = "json")]
        submit_verdict: bool,
        /// Annotation id this verdict is about. Required with
        /// `--submit-verdict`. The `.proof` file lands at
        /// `.aristo/proofs/<id>.proof` (with `:` rewritten to `__`).
        #[arg(long = "id", requires = "submit_verdict")]
        id: Option<String>,
        /// JSON-serialized ProofFile body. Required with
        /// `--submit-verdict`. Pass as a single-quoted shell string;
        /// the SDK parses it into a ProofFile and rejects anything
        /// the validator would reject. Same schema as the TOML body
        /// written on accept.
        #[arg(long = "json", requires = "submit_verdict")]
        json: Option<String>,
        /// **Internal — invoked by the verification skill.**
        /// Atomically claim one task from the pending queue and
        /// print its TOML body to stdout. Empty stdout means the
        /// queue is drained (exit 0 either way). Verify workers are
        /// single-shot — call once, process the task, exit — so
        /// context doesn't carry between verifications. The
        /// orchestrator runs N workers in parallel and uses
        /// `--queue-status` to decide when to spawn the next wave.
        #[arg(long = "pop-next", conflicts_with_all = ["apply_verdicts", "submit_verdict", "queue_status"])]
        pop_next: bool,
        /// Peek at queue state without claiming. Prints `pending: N`,
        /// `claimed: M` to stdout, exit 0. Used by the orchestrator
        /// to decide whether to dispatch another wave of workers.
        /// Safe to call concurrently.
        #[arg(long = "queue-status", conflicts_with_all = ["apply_verdicts", "submit_verdict"])]
        queue_status: bool,
        /// Block until the canon-verify session reaches a terminal
        /// state, rendering a snapshot at each long-poll return and
        /// emitting a `still running…` heartbeat every 60s. Exit code
        /// is derived from the final summary: `0` iff every
        /// annotation is `verified` or `no_coverage`. Without
        /// `--wait` the SDK detaches after dispatch (prints session
        /// id and exits 0). Combine with `--view <id>` to attach to
        /// a session another invocation started.
        #[arg(long = "wait")]
        wait: bool,
        /// Re-attach to a previously-dispatched canon-verify session
        /// by id. Skips the source eligibility scan, push-first
        /// precheck, and POST — just GETs the session state and
        /// renders. Combine with `--wait` to block until terminal.
        #[arg(long = "view", value_name = "SESSION_ID")]
        view: Option<String>,
        /// Subset the canon-verify dispatch to the listed annotation
        /// ids. Comma-separated; each id must be a canon-bound entry
        /// (`aristos:foo` or `kanon:bar`) in the workspace's index.
        /// Bare canon-id suffixes (e.g. `foo`) are accepted as a
        /// shorthand. `arta_*` (server-side opaque) ids are rejected
        /// — those are not user-facing.
        #[arg(long = "tags", value_name = "id1,id2,...", value_delimiter = ',')]
        tags: Vec<String>,
    },

    /// Run the critique skill against annotation prose — opinionated
    /// suggestions, severity-tagged findings.
    Critique {
        /// Unified filter clause (`id=<id>[,<id>,...]`,
        /// `file=<path>`, `parent=<id>`, `status=<state>`). Repeatable;
        /// multiple `--filter` flags AND together; values may be
        /// comma-separated. **REQUIRED** — `aristo critique` with no
        /// filter errors with usage. To sweep every annotation in the
        /// index, opt in explicitly with `--all --yes`.
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
        /// By default only open findings are listed — closed ones
        /// stop re-surfacing on every apply, which is how a review
        /// closes the loop. Only meaningful with `--apply-findings`.
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
        /// Sweep every intent annotation that has a real `verify`
        /// method (skips documentation-only `verify = false`). Loud
        /// on purpose: prints `(this will enqueue N annotations;
        /// ~$X cost — proceed with --all --yes?)` and exits 2 unless
        /// you also pass `--yes`. Without the confirmation, an agent
        /// could accidentally fire hundreds of LLM calls in one go.
        #[arg(long = "all", conflicts_with_all = ["filters", "staged"])]
        all: bool,
        /// Skip the confirmation prompt for `--all`. Required
        /// alongside `--all` to actually enqueue the sweep; without
        /// it `--all` just prints the cost estimate and exits 2.
        #[arg(long = "yes", requires = "all")]
        yes: bool,
        /// **Internal — invoked by the critique skill.** Atomically
        /// claim one task from the critique queue and print its TOML
        /// body to stdout. Empty stdout means the queue is drained
        /// (exit 0 either way). Unlike verify, critique workers loop
        /// on this call — the tasks are shallow and vocabulary stays
        /// consistent when one worker handles several.
        #[arg(long = "pop-next", conflicts_with_all = ["apply_findings", "submit_findings", "queue_status"])]
        pop_next: bool,
        /// Peek at queue state without claiming. Prints `pending: N`
        /// + `claimed: M` to stdout, exit 0.
        #[arg(long = "queue-status", conflicts_with_all = ["apply_findings", "submit_findings"])]
        queue_status: bool,
        /// **Internal — invoked by the critique skill.** Submit a
        /// single critique: parse the JSON payload, validate it, and
        /// (on accept) atomically write
        /// `.aristo/critiques/<id>.critique`. Prints
        /// `accepted: sha256:<hex>` on success.
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
        /// Include each annotation's current verification status in
        /// the rendered markdown. Status is a build-time snapshot
        /// that drifts as code evolves; the default omits it so doc
        /// artifacts stay reproducible on a clean checkout.
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
        /// Unified filter clause (`id=<id>`, `file=<path>[:<LO>-<HI>]`,
        /// `parent=<id>`, `status=<state>`). Repeatable; multiple
        /// `--filter` flags AND together. With no filter, the scope
        /// is the whole index.
        #[arg(long = "filter", value_name = "key=value")]
        filters: Vec<String>,
        /// Drop `assume` nodes from the rendered graph. They're
        /// included by default because assumes describe the
        /// background facts your intents rely on — dropping them by
        /// default would hide those.
        #[arg(long = "exclude-assumes")]
        exclude_assumes: bool,
        /// Walk N hops from each filter-matched node in both
        /// directions (ancestors + descendants) and include them in
        /// the rendered graph. Useful for "show me this annotation
        /// plus some context". Only meaningful with `--filter`;
        /// without a filter, the scope is already the whole index.
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        /// Include intent nodes that have no parent and no children.
        /// They're omitted by default — usually they're standalone
        /// claims that don't add structure to the rendered graph.
        /// Assumes are always included (see `--exclude-assumes` for
        /// that opt-out).
        #[arg(long = "include-orphans")]
        include_orphans: bool,
        /// Color nodes by their current verification status instead
        /// of by `verify` level (verified=green / tested=blue /
        /// neural=yellow / stale=orange / orphan=purple /
        /// forged=red+border / unknown=gray /
        /// counterexample=red+border / inconclusive=red+border /
        /// pending-deepen=gray). The `verify` level moves to the
        /// in-node label. Use when you want to see what's still
        /// unverified.
        #[arg(long = "include-status")]
        include_status: bool,
    },

    /// Generate a shareable SVG verification badge for README / docs.
    Badge {
        /// Write SVG to this path (relative to workspace root, or absolute).
        /// Default: stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Badge style: `flat-square` (default), `flat`, or `plastic`.
        #[arg(long, default_value = "flat-square")]
        style: String,
        /// Which metric the SVG value half displays. `tier` (default,
        /// the locked D7 score → D8 tier) is the headline signal;
        /// `count` and `rate` preserve the slice-31 surfaces for
        /// projects that prefer the simpler counters.
        #[arg(long, default_value = "tier")]
        metric: String,
    },

    /// Rename an annotation id everywhere it appears — source files,
    /// index, and doc artifacts. Either every change lands or none do.
    ///
    /// Supported renames: bare → bare, and stamp-assigned opaque
    /// (`aret_*`) → bare. Canon-bound prefixes (`aristos:` / `kanon:`)
    /// are rejected in either direction — those prefixes are applied
    /// by `aristo canon accept` and removed by `aristo canon unbind`.
    /// The new id cannot itself be an opaque `aret_*` id (those are
    /// stamp-assigned only).
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

    /// Run a review session over a pipeline's open artifacts —
    /// critique findings, proof verdicts, and so on. Start it,
    /// inspect bucket counts, record decisions, and close out.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Sign in to the Aretta canon API. Required for `aristo stamp`
    /// and `aristo critique` to see canon matches on the Pro /
    /// Enterprise tiers.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Manage canon bindings: accept or reject pending matches,
    /// inspect or refresh the local cache, unbind bound ids, and
    /// request a verifier for a canon entry.
    Canon {
        #[command(subcommand)]
        action: CanonAction,
    },
}

/// Subcommands under `aristo auth`. Each operates on the persistent
/// credentials store under `$XDG_CONFIG_HOME/aristo/credentials`
/// (or the platform default per `aristo_core::auth`).
#[derive(clap::Subcommand, Debug)]
pub(crate) enum AuthAction {
    /// Authenticate against the Aretta proxy.
    ///
    /// **Default mode (GitHub OAuth):** the CLI fetches the GitHub
    /// authorization URL from the proxy, tries to open it in your
    /// browser, and prompts you to paste the code shown on the
    /// proxy's callback page. The proxy then mints an `arta_*`
    /// token scoped to your `(user, repo)` pair.
    ///
    /// **Bypass modes (for CI / scripting):**
    ///
    /// - **`--stdin`** — read the raw token from stdin
    ///   (`echo "$TOKEN" | aristo auth login --stdin`).
    /// - **`--token=<T>`** — use the literal token value.
    ///
    /// The token is persisted to `$XDG_CONFIG_HOME/aristo/credentials`
    /// with `0600` Unix permissions.
    Login {
        /// Read the token from stdin (consumes entire stdin). Skips
        /// the OAuth flow.
        #[arg(long, conflicts_with = "token")]
        stdin: bool,
        /// Use this token directly. Skips the OAuth flow.
        #[arg(long, value_name = "TOKEN")]
        token: Option<String>,
        /// Aretta server to authenticate against. Accepts:
        /// `prod` / `production` (= https://code.aretta.ai, default),
        /// `dev` / `development` / `staging` (= https://dev.aretta.ai),
        /// or a full URL for self-hosted deployments
        /// (`https://aretta.example.com`).
        #[arg(long, default_value = "prod")]
        server: String,
        /// Repo to scope the OAuth-minted token to (`owner/repo`).
        /// Defaults to auto-deriving from `<cwd>/.git/config`'s
        /// `remote.origin.url`. Required for non-git directories or
        /// when the remote isn't a GitHub URL. Ignored in `--stdin` /
        /// `--token` bypass modes (where the token is supplied
        /// directly with its server-side scope already set).
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Show the current authentication state. Never prints the token
    /// itself — only its source (env var, credentials file, or none).
    /// Handy for sanity-checking before running `aristo stamp`.
    Status,
    /// Remove the stored credentials file. Idempotent — running
    /// `logout` when not logged in is not an error.
    Logout,
}

/// Subcommands under `aristo canon`.
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
    /// `pending_matches` to `rejected_matches`, pinned to the
    /// current annotation `text_hash`. The rejection keeps the same
    /// `(canon_id, text_hash)` pair from re-surfacing on future
    /// `aristo stamp` runs; once the annotation text changes, the
    /// rejection no longer applies and the match is re-evaluated.
    /// Source and index are not touched — rejection is a cache-only
    /// operation.
    Reject {
        /// Annotation id whose pending match you're rejecting.
        annotation_id: String,
        /// Canon id from the pending match.
        canon_id: String,
        /// Optional note recorded with the rejection. Useful for
        /// capturing the *why* (e.g. "this canon entry is too broad",
        /// "wrong category") for whoever revisits it later.
        #[arg(long = "reason")]
        reason: Option<String>,
    },

    /// List the current canon match state: one line per annotation
    /// with pending / accepted / rejected counts, plus per-bucket
    /// detail lines for each match. Reads `.aristo/canon-matches.toml`;
    /// does not call the canon API.
    List,

    /// Fetch the canon entry detail for `<canon_id>` via the canon
    /// API and render the longer description + example + references.
    /// For the full trust card (server description + local binding
    /// state combined), use `aristo show <bound_id>` instead.
    Show {
        /// Bare canon id (no `aristos:` / `kanon:` prefix). The
        /// server's `GET /canon/entry/<canon_id>` endpoint returns
        /// the same entry regardless of which tier you'd bind into;
        /// the prefix is a per-user, per-scope attribute.
        canon_id: String,
        /// Optional explicit version (`v<minor>.<patch>`). Omit to
        /// get the catalog's currently active version.
        #[arg(long = "version")]
        version: Option<String>,
    },

    /// Re-query the canon API for every annotation in the index,
    /// bypassing the local match cache. Equivalent to
    /// `aristo stamp --refresh-canon` without the rest of the stamp
    /// pipeline — no source walk, no drift check, no index rewrite.
    /// Useful when you know a new catalog version has shipped and
    /// want fresh matches without a full stamp.
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

    /// Record a verification-demand signal against a canon entry.
    /// Idempotent on `(canon_id, repo, user)` — repeated calls don't
    /// pile up. Use when an annotation is bound at the `kanon:` tier
    /// and you'd like Aretta to invest in a verifier for that canon
    /// entry.
    RequestVerify {
        /// Canon id (no prefix). The same id the trust card shows,
        /// or that `aristo canon list` reports.
        canon_id: String,
        /// Optional note to attach to the demand signal (e.g.
        /// "critical for our financial-tx audit"). A repeat call
        /// with a new note replaces the previous one server-side.
        #[arg(long = "notes")]
        notes: Option<String>,
    },

    /// Report per-binding version drift between the local cache and
    /// the canon API. Reports three classes: `current` (no change),
    /// `patch-bump` (same canon_id, newer version — recommended
    /// action: `aristo canon refresh`), and `minor-bump` (canon_id
    /// retired — recommended action: `aristo canon unbind <id>` then
    /// re-stamp). Currently diagnostic-only; automatic patch-bump
    /// application is planned.
    Migrate,
}

/// Subcommands under `aristo session`. Each maps to one substrate
/// operation; per-kind side effects (e.g. mutating a `.critique`
/// file on accept) plug in via the `SessionKind` trait wired in
/// step 5.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum SessionAction {
    /// Begin a new review session of the given kind. Fails if a
    /// session is already active — pass `--allow-nesting` to override
    /// (currently no kind allows nesting).
    Start {
        /// Session kind (`critique-review`, `proof-review`).
        kind: String,
        /// Display label for the artifact under review (e.g.
        /// `src/critique/pending.rs` or
        /// `proof:balance_no_duplicate_cells`).
        #[arg(long = "subject")]
        subject: String,
        /// Override the kind's default nesting policy. Currently only
        /// `Disallow` is implemented; the flag is reserved for future
        /// per-kind opt-ins.
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
        /// Optional note recorded with the decision.
        #[arg(long = "note")]
        note: Option<String>,
    },
    /// Close the active session. Strict by default — errors out if
    /// any items are still in the open bucket.
    Exit {
        /// Move open items to the per-kind backlog instead of
        /// erroring. Items are never silently dropped; the next
        /// session of this kind surfaces them via the backlog menu.
        #[arg(long = "defer-undecided", default_value_t = false)]
        defer_undecided: bool,
    },
    /// Cancel the session and discard every decision recorded so far.
    /// Requires `--yes` to skip the confirmation prompt.
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

/// Maps a parsed `Commands` variant to its handler.
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
            wait,
            view,
            tags,
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
            wait,
            view,
            &tags,
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
            CanonAction::Migrate => commands::canon::migrate::run(),
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
