//! `aristo verify` canon-verify dispatch (§14 phase E1) — end-to-end
//! integration tests.
//!
//! Goes through the real CLI binary (`aristo verify`) against a
//! tempdir workspace + git repo. The HTTP call to the proxy is
//! intercepted via the `ARISTO_CANON_VERIFY_FIXTURE` env var (a
//! test-only escape hatch in `commands/verify/canon_dispatch.rs`)
//! which:
//!
//! 1. Returns a canned `PostVerifySessionResponse` from a JSON file.
//! 2. Writes the actual request body the SDK sent to `<fixture>.posted.json`
//!    so this test can assert wire-shape correctness.
//!
//! What we pin:
//!
//! - The SDK reaches the dispatch path for `verify="full"` +
//!   `aristos:` / `kanon:` ids.
//! - Push-first precheck rejects local-only commits (no origin/*).
//! - Auth-missing yields the actionable hint, not a panic.
//! - The POST body shape matches WORKFLOW.md §4 wire (tags carry
//!   `arta_*` ids, bare canon_ids, version from cache, `file:line`
//!   source_path).
//! - Output prints session_id + view_url in detach default.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;
use std::path::{Path, PathBuf};

const ZERO_HASH: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    // Belt + braces: every test sets up its own env-var isolation.
    // Strip both potential auth sources by default.
    cmd.env_remove("ARETTA_TOKEN");
    cmd
}

fn run_git(repo: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Initialize `dir` as a git repo with `origin` pointing at a sibling
/// bare repo, and commit the current contents so HEAD is on
/// `origin/main`. Returns the bare-upstream tempdir (so the caller
/// keeps it alive — dropping it would invalidate the remote URL).
fn init_repo_with_pushed_head(dir: &Path) -> tempfile::TempDir {
    let bare = tempfile::tempdir().unwrap();
    run_git(bare.path(), &["init", "--bare", "-q"]);
    run_git(dir, &["init", "-q", "-b", "main"]);
    run_git(dir, &["config", "user.email", "t@x"]);
    run_git(dir, &["config", "user.name", "t"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);
    // Add a GitHub-shaped origin URL so the SDK's `derive_repo_full_name`
    // produces `owner/repo` cleanly. The actual URL doesn't need to
    // resolve — we override the actual remote target separately.
    run_git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    // Now point that origin at the local bare repo for the push.
    run_git(
        dir,
        &["remote", "set-url", "origin", bare.path().to_str().unwrap()],
    );
    // Seed file so git has something to commit. The SDK only cares
    // that HEAD is reachable from `origin/*`; subsequent uncommitted
    // workspace changes are fine.
    fs::write(dir.join("README"), b"seed").unwrap();
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-q", "-m", "init"]);
    run_git(dir, &["push", "-q", "origin", "main"]);
    // Restore the GitHub URL on origin so derive_repo_full_name picks
    // it up. (push happened against the bare upstream.)
    run_git(
        dir,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    bare
}

/// Write an `.aristo/index.toml` with one canon-bound (aristos:)
/// intent at `verify="full"`. Returns the bare upstream + the
/// fixture file path.
fn workspace_with_one_canon_bound_full_intent(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
    // aristos:foo with `linked = arta_...` is the only legal way to
    // have a bound canon-tier entry — id-namespace ↔ binding-state
    // agreement is enforced by the index validator.
    let body = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [\"aristos:foo\"]\nkind = \"intent\"\ntext = \"the property\"\n\
         verify = \"full\"\nstatus = \"unknown\"\n\
         text_hash = \"{ZERO_HASH}\"\nbody_hash = \"{ZERO_HASH}\"\n\
         file = \"src/foo.rs\"\nsite = \"fn foo (line 42)\"\n\
         covered_region = \"function\"\n\
         linked = \"arta_op4q3z9NbV\"\n",
    );
    fs::write(dir.join(".aristo/index.toml"), body).unwrap();

    // Matching canon-matches cache entry — version source of truth.
    let matches = r#"
[__meta__]
schema_version = 1

["aristos:foo"]
last_match_text_hash = "blake3:test"
canon_fetched_at = "2026-05-24T00:00:00Z"

[["aristos:foo".accepted_matches]]
canon_id = "foo"
version = "v0.1.0"
canonical_text = "the property"
canon_version = "v0.2.0"
confidence = 0.95
prefix_tier = "aristos:"
backed_by = "test backing"
accepted_at = "2026-05-24T00:00:00Z"
bound_at = "2026-05-24T00:00:00Z"
"#;
    fs::write(dir.join(".aristo/canon-matches.toml"), matches).unwrap();
}

/// Write a fixture file the SDK reads via ARISTO_CANON_VERIFY_FIXTURE.
/// Returns the path the SDK will write the captured POST body to.
fn write_post_fixture(dir: &Path, session_id: &str, plan_size: u32) -> PathBuf {
    let path = dir.join("verify-fixture.json");
    let body = format!(
        r#"{{
          "post": {{
            "session_id": "{session_id}",
            "view_url": "https://dev.aretta.ai/dashboard/jobs/{session_id}",
            "plan_size": {plan_size}
          }}
        }}"#
    );
    fs::write(&path, body).unwrap();
    dir.join("verify-fixture.json.posted.json")
}

fn write_aretta_token(home: &Path, server_url: &str) {
    // Stand up XDG-style credentials so `auth::resolve_full()` picks
    // up a token without env-var auth. The store layout matches
    // `aristo_core::auth::store::credentials_path_with`.
    let creds_dir = home.join(".config/aristo");
    fs::create_dir_all(&creds_dir).unwrap();
    let body = format!(
        r#"
[aretta]
token = "arta_test_token_xxx"
server = "{server_url}"
user_login = "tester"
user_id = 1
repo = "owner/repo"
issued_at = "2026-05-24T00:00:00Z"
"#
    );
    fs::write(creds_dir.join("credentials"), body).unwrap();
}

// ─── tests ───────────────────────────────────────────────────────────────

#[test]
fn canon_bound_full_with_no_auth_surfaces_aristo_auth_login_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    // No auth on disk + no env var. The dispatcher must surface the
    // actionable hint and exit non-zero, NOT crash and NOT silently
    // skip.
    let home = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .arg("verify")
        .assert()
        .failure()
        .stderr(contains("verify requires authentication"))
        .stderr(contains("aristo auth login"));
}

#[test]
fn canon_bound_full_with_auth_posts_session_and_prints_session_id() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    let captured_path = write_post_fixture(tmp.path(), "01HMTESTSESSION", 1);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env(
            "ARISTO_CANON_VERIFY_FIXTURE",
            tmp.path().join("verify-fixture.json"),
        )
        .arg("verify")
        .assert()
        .success()
        .stdout(contains("verify session dispatched"))
        .stdout(contains("01HMTESTSESSION"))
        .stdout(contains(
            "https://dev.aretta.ai/dashboard/jobs/01HMTESTSESSION",
        ))
        .stdout(contains("aristo verify --view 01HMTESTSESSION"));

    // Wire-shape assertions: the SDK's POST body must match WORKFLOW.md §4.
    let captured = fs::read_to_string(&captured_path)
        .unwrap_or_else(|e| panic!("expected captured POST at {captured_path:?}: {e}"));
    let body: serde_json::Value = serde_json::from_str(&captured).unwrap();
    assert_eq!(body["repo_full_name"], "owner/repo");
    assert!(body["commit_sha"].as_str().unwrap().len() == 40);
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0]["annotation_id"], "arta_op4q3z9NbV");
    assert_eq!(tags[0]["canon_id"], "foo");
    assert_eq!(tags[0]["version"], "v0.1.0");
    assert_eq!(tags[0]["source_path"], "src/foo.rs:42");
}

#[test]
fn local_only_commit_rejected_by_push_first_precheck() {
    let tmp = tempfile::tempdir().unwrap();
    // Init repo but DO NOT push — HEAD is local-only. The precheck
    // must reject before the SDK POSTs.
    run_git(tmp.path(), &["init", "-q", "-b", "main"]);
    run_git(tmp.path(), &["config", "user.email", "t@x"]);
    run_git(tmp.path(), &["config", "user.name", "t"]);
    run_git(tmp.path(), &["config", "commit.gpgsign", "false"]);
    run_git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    fs::write(tmp.path().join("README"), b"seed").unwrap();
    workspace_with_one_canon_bound_full_intent(tmp.path());
    run_git(tmp.path(), &["add", "-A"]);
    // --no-verify bypasses the pre-commit hook `aristo init` installed
    // (it'd re-stamp from an empty source tree and wipe our manual
    // index entry); the test only needs HEAD to exist + be unpushed.
    run_git(tmp.path(), &["commit", "-q", "--no-verify", "-m", "init"]);

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    // Even with a fixture set, the precheck should reject before the
    // mock client is reached.
    let _captured = write_post_fixture(tmp.path(), "01HMUNREACHED", 1);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env(
            "ARISTO_CANON_VERIFY_FIXTURE",
            tmp.path().join("verify-fixture.json"),
        )
        .arg("verify")
        .assert()
        .failure()
        .stderr(contains("not pushed to origin"))
        .stderr(contains("Push your branch first"));
}

#[test]
fn missing_cache_entry_for_canon_bound_full_skips_with_refresh_hint() {
    // Canon-bound entry but no canon-matches cache row — can't resolve
    // the `version` field; SDK must skip with a refresh hint, not
    // POST an invalid request.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    // Clobber the cache to empty so the build_tags filter drops the entry.
    fs::write(
        tmp.path().join(".aristo/canon-matches.toml"),
        "[__meta__]\nschema_version = 1\n",
    )
    .unwrap();

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .arg("verify")
        .assert()
        .success()
        .stdout(contains("aristo canon refresh"));
}

#[test]
fn zero_dispatch_prints_unmissable_warning_but_still_exits_zero() {
    // Default behavior (no --require-dispatch): a zero-dispatch run
    // keeps exit 0 (operator ruling), but the silence is gone — a
    // loud stderr warning says nothing reached the server and what
    // to check.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    fs::write(
        tmp.path().join(".aristo/canon-matches.toml"),
        "[__meta__]\nschema_version = 1\n",
    )
    .unwrap();

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .arg("verify")
        .assert()
        .success()
        .stderr(contains("warning: no canon-verify dispatch"))
        .stderr(contains("aristo canon refresh"))
        .stderr(contains("--require-dispatch"));
}

#[test]
fn require_dispatch_fails_when_dispatch_set_is_empty() {
    // Same empty-cache setup as the refresh-hint test, but with
    // --require-dispatch the vacuous run must exit NON-ZERO — this is
    // the CI gate against "green because nothing ran".
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    fs::write(
        tmp.path().join(".aristo/canon-matches.toml"),
        "[__meta__]\nschema_version = 1\n",
    )
    .unwrap();

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args(["verify", "--require-dispatch"])
        .assert()
        .failure()
        .stderr(contains("--require-dispatch"))
        .stderr(contains("aristo canon refresh"));
}

#[test]
fn require_dispatch_passes_when_something_dispatches() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    let _captured = write_post_fixture(tmp.path(), "01HMREQOK", 1);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env(
            "ARISTO_CANON_VERIFY_FIXTURE",
            tmp.path().join("verify-fixture.json"),
        )
        .args(["verify", "--require-dispatch"])
        .assert()
        .success()
        .stdout(contains("verify session dispatched"));
}

#[test]
fn require_dispatch_conflicts_with_view() {
    // --view attaches to an already-dispatched session; requiring a
    // fresh dispatch there is contradictory, so clap rejects it.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["verify", "--view", "01HM", "--require-dispatch"])
        .assert()
        .failure()
        .code(2);
}

/// Fixture builder for `--wait` / `--view` paths: takes a JSON object
/// describing the post-and-gets sequence and writes it to a file the
/// SDK reads via ARISTO_CANON_VERIFY_FIXTURE.
fn write_full_fixture(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("verify-fixture.json");
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn wait_recovers_from_transient_poll_error_and_still_renders_verdict() {
    // A 503 mid-poll (proxy restart / server redeploy) must NOT abort
    // the customer's CI wait: the loop retries and the terminal
    // verdict still lands.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    let fixture_body = r#"{
      "post": {"session_id": "01HMBLIP", "view_url": "https://x", "plan_size": 1},
      "gets": [
        { "error": {"kind": "server", "status": 503, "message": "upstream restart"} },
        {
          "session_id": "01HMBLIP",
          "status": "running",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "annotations": [],
          "summary": {"total_annotations": 1, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        },
        { "error": {"kind": "network", "message": "connection reset by peer"} },
        {
          "session_id": "01HMBLIP",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [],
          "summary": {"total_annotations": 1, "verified": 1, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        }
      ]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .success()
        .stdout(contains("status: done (1/1 verified)"))
        .stderr(contains("transient error polling verify session"))
        .stderr(contains("retrying"));
}

#[test]
fn wait_4xx_poll_error_stays_fatal() {
    // A 4xx is deterministic — retrying re-sends the same rejected
    // request — so the poll loop must abort on first hit.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    let fixture_body = r#"{
      "post": {"session_id": "01HMGONE", "view_url": "https://x", "plan_size": 1},
      "gets": [
        { "error": {"kind": "bad_request", "status": 404, "message": "not_found"} }
      ]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stderr(contains("404"))
        .stderr(contains("retrying").not());
}

#[test]
fn wait_deadline_expiry_exits_with_distinct_message() {
    // ARISTO_VERIFY_WAIT_TIMEOUT_SECS=0 forces immediate expiry: the
    // CLI must exit non-zero with the deadline message + re-attach
    // hint instead of polling forever.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    let fixture_body = r#"{
      "post": {"session_id": "01HMSTUCK", "view_url": "https://x", "plan_size": 1}
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .env("ARISTO_VERIFY_WAIT_TIMEOUT_SECS", "0")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stderr(contains("deadline exceeded"))
        .stderr(contains("aristo verify --view 01HMSTUCK --wait"));
}

#[test]
fn wait_blocks_until_session_terminal_and_renders_final_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    // Two GET responses: one running, one done. The poll loop must
    // skip the first, render the second.
    let fixture_body = r#"{
      "post": {
        "session_id": "01HMWAIT",
        "view_url": "https://dev.aretta.ai/dashboard/jobs/01HMWAIT",
        "plan_size": 1
      },
      "gets": [
        {
          "session_id": "01HMWAIT",
          "status": "running",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "annotations": [],
          "summary": {"total_annotations": 1, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        },
        {
          "session_id": "01HMWAIT",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [
            {
              "annotation_id": "arta_op4q3z9NbV",
              "canon_id": "foo",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "src/foo.rs:42",
              "status": "verified",
              "tests": [{"test_binary": "foo_conform", "status": "pass", "duration_ms": 1234}]
            }
          ],
          "summary": {"total_annotations": 1, "verified": 1, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        }
      ]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .success()
        .stdout(contains("01HMWAIT"))
        .stdout(contains("session 01HMWAIT"))
        .stdout(contains("status: done (1/1 verified)"))
        .stdout(contains("aristos:foo@v0.1.0"))
        .stdout(contains("verified"));
}

#[test]
fn wait_exits_nonzero_when_summary_has_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    let fixture_body = r#"{
      "post": {"session_id": "01HMFAIL", "view_url": "https://x", "plan_size": 1},
      "gets": [
        {
          "session_id": "01HMFAIL",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [
            {
              "annotation_id": "arta_op4q3z9NbV",
              "canon_id": "foo",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "src/foo.rs:42",
              "status": "failed",
              "tests": [{"test_binary": "foo_conform", "status": "fail", "duration_ms": 1234}]
            }
          ],
          "summary": {"total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        }
      ]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stdout(contains("status: done"))
        .stdout(contains("failed"))
        .stderr(contains("1 failed"));
}

#[test]
fn wait_renders_structured_card_when_test_carries_phase16_report() {
    // Phase 16 Track A: a failing TestOutcome carrying the golden
    // cr03.minimal report must render a structured violation card
    // INSTEAD of the `• <bin> failed — see <url>` bullet. The report
    // body below is a byte-copy of the cross-repo contract fixture.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    // Lean-redacted (Slice 2.1): no model labels, no `spec_source`, the
    // provenance + reason are turso-framed.
    let report = r#"{
      "property": {
        "canon_id": "wal_initialized_reflects_sync_outcome",
        "statement": "The WAL initialized flag is set true only after a successful sync of the wal-header.",
        "impl_source": { "path": "core/storage/wal.rs", "line": 3989, "snippet": "prepare_wal_finish" }
      },
      "scenario": {
        "op_trace": [],
        "seed": "cr_03_initialized_under_sync_eio/fail_nth(Sync,1,EIO)/pragma+create"
      },
      "verdict": {
        "cr_id": "CR-03",
        "expected_to_fail": {
          "tag": "CR-03",
          "reason": "turso (a) rolls back the partial header write on sync failure, or (b) exposes a public wal_initialized_atomic() accessor that distinguishes \"flag was set\" from \"file exists\"."
        }
      },
      "relation": {
        "kind": "state_eq",
        "description": "StateEq under ProjectionMask { initialized }",
        "compared": ["initialized"],
        "ignored": ["max_frame", "max_frame_inflight", "nbackfills", "nbackfills_inflight", "checkpoint_seq", "on_disk_frame_count", "durable_frame_count", "on_disk_header_present", "on_disk_header_durable"]
      },
      "finding": {
        "kind": "state_eq",
        "expected": { "label": "reference", "fields": [ { "field": "initialized", "value": "false" } ] },
        "actual": { "label": "turso (observed)", "fields": [ { "field": "initialized", "value": "true" } ] },
        "divergence": [ { "field": "initialized", "expected": "false", "actual": "true", "provenance": "turso reports `initialized` from the *.db-wal file-existence proxy; a non-durable header reads as present." } ]
      }
    }"#;

    let fixture_body = format!(
        r#"{{
      "post": {{ "session_id": "01HMCARD", "view_url": "https://x", "plan_size": 1 }},
      "gets": [
        {{
          "session_id": "01HMCARD",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [
            {{
              "annotation_id": "arta_op4q3z9NbV",
              "canon_id": "wal_initialized_reflects_sync_outcome",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "core/storage/wal.rs:3989",
              "status": "failed",
              "tests": [
                {{ "test_binary": "cr_03_conform", "test_kind": "differential", "relation": "tight", "status": "fail", "duration_ms": 4210, "report": {report} }}
              ]
            }}
          ],
          "summary": {{ "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }}
        }}
      ]
    }}"#
    );
    let fixture_path = write_full_fixture(tmp.path(), &fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        // Headline: canon id + the plain-language statement. The statement
        // is word-wrapped into the card, so assert a fragment that stays
        // within one wrapped line rather than the whole sentence.
        .stdout(contains("wal_initialized_reflects_sync_outcome"))
        .stdout(contains("The WAL initialized flag is set true"))
        // Lean-redacted, user-framed snapshot labels.
        .stdout(contains("reference"))
        .stdout(contains("turso (observed)"))
        // The reworked "divergence observed" block: a header naming the
        // mask, then aligned expected/actual rows (the `-`/`+` gutter was
        // replaced by `expected`/`actual` word tags; ANSI auto-stripped
        // off-TTY).
        .stdout(contains("divergence observed"))
        .stdout(contains("compared [initialized]"))
        .stdout(contains("expected").and(contains("initialized = false")))
        .stdout(contains("actual").and(contains("initialized = true")))
        // The internal conformance verdict frame is NOT rendered — `CR-03`
        // and `EXPECTED TO FAIL` are Aretta-side bookkeeping, confusing on a
        // user surface.
        .stdout(contains("CR-03").not())
        .stdout(contains("EXPECTED TO FAIL").not())
        // Reproduce section is a real command scoped to this annotation.
        .stdout(contains(
            "aristo verify --filter id=aristos:wal_initialized_reflects_sync_outcome --rerun",
        ))
        // An un-waived property failure offers the one-liner to accept it.
        .stdout(contains(
            "aristo verify --accept aristos:wal_initialized_reflects_sync_outcome --because",
        ));
}

#[test]
fn wait_renders_fault_banner_but_not_op_trace_for_enriched_report() {
    // Phase 16 Slice 3a: a failing TestOutcome carrying an enriched report
    // (populated scenario.fault + scenario.op_trace) must render the fault
    // banner, but the op-trace timeline is NO LONGER rendered — it stays in
    // the contract data, off the card. Report body mirrors the salient
    // fields of the cross-repo cr03.full contract fixture.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    let report = r#"{
      "property": {
        "canon_id": "wal_initialized_reflects_sync_outcome",
        "statement": "The WAL initialized flag is set true only after a successful sync of the wal-header.",
        "impl_source": { "path": "core/storage/wal.rs", "line": 3989, "snippet": "prepare_wal_finish" }
      },
      "scenario": {
        "fault": { "kind": "fail_nth", "op": "Sync", "nth": 1, "error": "EIO", "fired_count": 1 },
        "op_trace": [
          { "seq": 1, "op": "pwrite", "detail": "db @0 len=4096", "injected": false, "result": "ok" },
          { "seq": 2, "op": "pwrite", "detail": "db-wal @0 len=32", "injected": false, "result": "ok" },
          { "seq": 3, "op": "sync", "detail": "db-wal", "injected": true, "result": "EIO" }
        ],
        "seed": "cr_03_initialized_under_sync_eio/fail_nth(Sync,1,EIO)/pragma+create"
      },
      "verdict": {
        "cr_id": "CR-03",
        "expected_to_fail": { "tag": "CR-03", "reason": "turso rolls back the partial header write on sync failure." }
      },
      "relation": {
        "kind": "state_eq",
        "description": "StateEq under ProjectionMask { initialized }",
        "compared": ["initialized"],
        "ignored": ["max_frame", "nbackfills"]
      },
      "finding": {
        "kind": "state_eq",
        "expected": { "label": "reference", "fields": [ { "field": "initialized", "value": "false" } ] },
        "actual": { "label": "turso (observed)", "fields": [ { "field": "initialized", "value": "true" } ] },
        "divergence": [ { "field": "initialized", "expected": "false", "actual": "true", "provenance": "turso reports initialized from the *.db-wal file-existence proxy; a non-durable header reads as present." } ]
      }
    }"#;

    let fixture_body = format!(
        r#"{{
      "post": {{ "session_id": "01HMFULL", "view_url": "https://x", "plan_size": 1 }},
      "gets": [
        {{
          "session_id": "01HMFULL",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [
            {{
              "annotation_id": "arta_op4q3z9NbV",
              "canon_id": "wal_initialized_reflects_sync_outcome",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "core/storage/wal.rs:3989",
              "status": "failed",
              "tests": [
                {{ "test_binary": "cr_03_conform", "test_kind": "differential", "relation": "tight", "status": "fail", "duration_ms": 4210, "report": {report} }}
              ]
            }}
          ],
          "summary": {{ "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }}
        }}
      ]
    }}"#
    );
    let fixture_path = write_full_fixture(tmp.path(), &fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        // The fault banner: the named, parameterized injected fault. The
        // "fired" count is no longer parenthesized (Slice 3a redesign).
        .stdout(contains("fault"))
        .stdout(contains("fail_nth · sync #1 → EIO"))
        .stdout(contains("fired 1×"))
        .stdout(contains("(fired 1×)").not())
        // The op-trace timeline is NO LONGER rendered (Slice 3a): the
        // op stream stays in the contract data but is off the card. Its
        // header, the per-op rows, and the injected marker are all gone.
        .stdout(contains("what we tested").not())
        .stdout(contains("I/O ops").not())
        .stdout(contains("← injected").not())
        // This enriched report carries no call_path / test_shape, so the
        // "what turso ran" / "where your code broke it" sections don't
        // render — only the divergence block follows the banner.
        .stdout(contains("divergence observed"))
        // Still no model/Lean identity on the user surface (IP).
        .stdout(contains("lean").not())
        .stdout(contains("Lean").not());
}

/// A fixture body with `n` identical "running" GET snapshots (and
/// optionally a POST response) — enough runway that a signal test can
/// interrupt the poll loop long before the mock queue drains.
fn fixture_with_running_gets(session_id: &str, n: usize, with_post: bool) -> String {
    let running = serde_json::json!({
        "session_id": session_id,
        "status": "running",
        "user_commit_sha": "abc1234567890",
        "canon_version": "v0.1.0",
        "started_at": "2026-05-24T00:00:00Z",
        "annotations": [],
        "summary": {"total_annotations": 1, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
    });
    let gets: Vec<serde_json::Value> = std::iter::repeat_with(|| running.clone()).take(n).collect();
    let mut fx = serde_json::json!({ "gets": gets });
    if with_post {
        fx["post"] = serde_json::json!({
            "session_id": session_id,
            "view_url": "https://x",
            "plan_size": 1
        });
    }
    fx.to_string()
}

/// Spawn the real binary with the given verify args, let it reach the
/// poll loop, deliver SIGINT, and collect the output.
#[cfg(unix)]
fn spawn_and_interrupt(
    dir: &Path,
    home: &Path,
    fixture_path: &Path,
    args: &[&str],
) -> std::process::Output {
    let bin = assert_cmd::cargo::cargo_bin("aristo");
    let child = std::process::Command::new(bin)
        .current_dir(dir)
        .env_remove("ARETTA_TOKEN")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "50")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn aristo");
    // Give the process time to install the interrupt handler and
    // enter the poll loop, then deliver SIGINT.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let status = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(status.success(), "kill -INT failed");
    child.wait_with_output().expect("collect output")
}

#[cfg(unix)]
#[test]
fn sigint_during_dispatch_wait_cancels_the_owned_session() {
    // The dispatching invocation OWNS the session: interrupting its
    // --wait must fire the server-side cancel (proven via the mock's
    // .cancelled sidecar) and exit 130.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    let fixture_path = write_full_fixture(
        tmp.path(),
        &fixture_with_running_gets("01HMOWNED", 400, true),
    );

    let out = spawn_and_interrupt(
        tmp.path(),
        home.path(),
        &fixture_path,
        &["verify", "--wait"],
    );

    assert_eq!(
        out.status.code(),
        Some(130),
        "interrupt must exit 130; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cancel requested"),
        "owner interrupt must cancel: {stderr}"
    );
    let cancelled = fs::read_to_string(tmp.path().join("verify-fixture.json.cancelled"))
        .expect("cancel sidecar must exist — the dispatcher owns the session");
    assert_eq!(cancelled, "01HMOWNED");
}

#[cfg(unix)]
#[test]
fn sigint_during_view_wait_detaches_without_cancelling() {
    // --view attaches to a session ANOTHER invocation started — a
    // viewer is an observer, not the owner. Interrupting it must NOT
    // cancel the session (that would kill the primary CI waiter's
    // run); it detaches with a re-attach hint and exits 130.
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    let fixture_path = write_full_fixture(
        tmp.path(),
        &fixture_with_running_gets("01HMOTHERS", 400, false),
    );

    let out = spawn_and_interrupt(
        tmp.path(),
        home.path(),
        &fixture_path,
        &["verify", "--view", "01HMOTHERS", "--wait"],
    );

    assert_eq!(
        out.status.code(),
        Some(130),
        "viewer interrupt must exit 130; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("detached"),
        "viewer interrupt must detach, not cancel: {stderr}"
    );
    assert!(
        !stderr.contains("cancel requested"),
        "viewer must never cancel a session it did not start: {stderr}"
    );
    assert!(
        !tmp.path().join("verify-fixture.json.cancelled").exists(),
        "no cancel request may leave a viewer process"
    );
}

#[test]
fn view_attaches_to_existing_session_without_post() {
    // --view <id> skips POST entirely — no workspace, no git, no auth-
    // sourced repo. Only a single GET. We deliberately don't init a
    // git repo in this test to prove push-first/precheck is skipped.
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    // No `post` block — the SDK should never POST under --view.
    let fixture_body = r#"{
      "gets": [
        {
          "session_id": "01HMVIEW",
          "status": "running",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "annotations": [],
          "summary": {"total_annotations": 0, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        }
      ]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .args(["verify", "--view", "01HMVIEW"])
        .assert()
        .success()
        .stdout(contains("session 01HMVIEW"))
        .stdout(contains("status: running"));

    // The .posted.json sidecar must NOT have been written.
    let posted = tmp.path().join("verify-fixture.json.posted.json");
    assert!(
        !posted.exists(),
        "--view must not POST; sidecar exists at {posted:?}"
    );
}

#[test]
fn view_with_wait_blocks_until_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    let fixture_body = r#"{
      "gets": [
        {
          "session_id": "01HMVW",
          "status": "running",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "annotations": [],
          "summary": {"total_annotations": 1, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        },
        {
          "session_id": "01HMVW",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [],
          "summary": {"total_annotations": 1, "verified": 1, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0}
        }
      ]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture_body);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--view", "01HMVW", "--wait"])
        .assert()
        .success()
        .stdout(contains("status: done (1/1 verified)"));
}

#[test]
fn tags_filter_narrows_to_requested_canon_bound_ids() {
    // Two canon-bound entries; --tags picks one. Wire-shape proves
    // only the requested tag's annotation_id reached the POST body.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    aristo_in(tmp.path()).arg("init").assert().success();
    let body = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [\"aristos:alpha\"]\nkind = \"intent\"\ntext = \"a\"\n\
         verify = \"full\"\nstatus = \"unknown\"\n\
         text_hash = \"{ZERO_HASH}\"\nbody_hash = \"{ZERO_HASH}\"\n\
         file = \"src/a.rs\"\nsite = \"fn a (line 1)\"\n\
         covered_region = \"function\"\nlinked = \"arta_alpha000000\"\n\n\
         [\"kanon:beta\"]\nkind = \"intent\"\ntext = \"b\"\n\
         verify = \"full\"\nstatus = \"unknown\"\n\
         text_hash = \"{ZERO_HASH}\"\nbody_hash = \"{ZERO_HASH}\"\n\
         file = \"src/b.rs\"\nsite = \"fn b (line 1)\"\n\
         covered_region = \"function\"\nlinked = \"arta_beta00000000\"\n",
    );
    fs::write(tmp.path().join(".aristo/index.toml"), body).unwrap();
    let matches = r#"
[__meta__]
schema_version = 1

["aristos:alpha"]
last_match_text_hash = "blake3:a"
canon_fetched_at = "2026-05-24T00:00:00Z"
[["aristos:alpha".accepted_matches]]
canon_id = "alpha"
version = "v0.1.0"
canonical_text = "a"
canon_version = "v0.2.0"
confidence = 0.9
prefix_tier = "aristos:"
backed_by = "x"
accepted_at = "2026-05-24T00:00:00Z"
bound_at = "2026-05-24T00:00:00Z"

["kanon:beta"]
last_match_text_hash = "blake3:b"
canon_fetched_at = "2026-05-24T00:00:00Z"
[["kanon:beta".accepted_matches]]
canon_id = "beta"
version = "v0.2.0"
canonical_text = "b"
canon_version = "v0.2.0"
confidence = 0.9
prefix_tier = "kanon:"
accepted_at = "2026-05-24T00:00:00Z"
bound_at = "2026-05-24T00:00:00Z"
"#;
    fs::write(tmp.path().join(".aristo/canon-matches.toml"), matches).unwrap();

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    let captured_path = write_post_fixture(tmp.path(), "01HMTAGS", 1);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env(
            "ARISTO_CANON_VERIFY_FIXTURE",
            tmp.path().join("verify-fixture.json"),
        )
        .args(["verify", "--tags", "kanon:beta"])
        .assert()
        .success();

    let captured = fs::read_to_string(&captured_path).unwrap();
    let body: serde_json::Value = serde_json::from_str(&captured).unwrap();
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(
        tags.len(),
        1,
        "--tags kanon:beta must dispatch one tag, got {tags:?}"
    );
    assert_eq!(tags[0]["annotation_id"], "arta_beta00000000");
    assert_eq!(tags[0]["canon_id"], "beta");
}

#[test]
fn tags_rejects_arta_prefixed_ids() {
    // arta_* is server-side opaque; users should never paste it.
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());

    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args(["verify", "--tags", "arta_op4q3z9NbV"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("--tags rejects opaque server ids"));
}

#[test]
fn view_with_tags_is_rejected_as_incompatible() {
    // The session was already dispatched with its tag set; --tags is
    // not meaningful on attach.
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");
    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args(["verify", "--view", "01HM", "--tags", "aristos:foo"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("--tags is not compatible with --view"));
}

#[test]
fn non_canon_bound_full_intent_still_yields_deferred_design_message() {
    // Regression for the pre-§14 NotImplemented arm — local-id
    // intents at verify=full must still hit the deferred-design
    // NotImplemented (their verification mechanism is unrelated to
    // canon-verify).
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    aristo_in(tmp.path()).arg("init").assert().success();
    let body = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [my_local_full]\nkind = \"intent\"\ntext = \"x\"\nverify = \"full\"\nstatus = \"unknown\"\n\
         text_hash = \"{ZERO_HASH}\"\nbody_hash = \"{ZERO_HASH}\"\n\
         file = \"src/x.rs\"\nsite = \"fn x (line 1)\"\n\
         covered_region = \"function\"\n",
    );
    fs::write(tmp.path().join(".aristo/index.toml"), body).unwrap();

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .stderr(contains("non-canon-bound"))
        .stderr(contains("post-MVP"));
}
