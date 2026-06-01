//! `aristo verify` known-failure waiver — Phase 16 (c) — end-to-end
//! integration tests.
//!
//! These tests are THE SPEC for the user-side "expected to fail"
//! mechanism: a git-tracked `.aristo/expectations.toml` sidecar that
//! records property gaps the user has explicitly accepted, keyed on the
//! stable canon id. Authored via `aristo verify --accept <canon-id>
//! --because "<reason>"`; applied as a read-time join when verify
//! renders + computes its exit code.
//!
//! What we pin:
//!
//! - `--accept` writes `.aristo/expectations.toml` with the reason
//!   (mandatory) + optional tracking ref, keyed on the prefixed canon id.
//! - `--accept` requires `--because`, rejects unknown / opaque ids, and
//!   accepts a bare canon-id suffix as shorthand. It's idempotent.
//! - Read-time join (`--wait`): a WAIVED + still-FAILING annotation
//!   renders "known gap (accepted)" and the run exits 0 (the failure is
//!   excluded from the red exit). The internal `EXPECTED TO FAIL` frame
//!   is not shown.
//! - Strict ratchet: a WAIVED annotation that now PASSES makes verify go
//!   RED ("accepted gap now passes — remove the waiver") and exit
//!   non-zero. This is what keeps waivers from rotting.
//! - An UN-waived failure is unchanged (still red, exit 1) — the waiver
//!   layer never leaks into un-waived annotations.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;
use std::path::{Path, PathBuf};

const ZERO_HASH: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
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

/// Lean workspace (no git, no `aristo init`): just `aristo.toml` + a
/// `.aristo/index.toml` carrying one canon-bound (`aristos:foo`) entry.
/// Enough to drive the write-only `--accept` flow, which only needs the
/// index to validate the canon id.
fn lean_workspace_with_canon_entry(dir: &Path) {
    fs::write(dir.join("aristo.toml"), "").unwrap();
    fs::create_dir_all(dir.join(".aristo")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
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
}

/// Full workspace for the dispatch+wait path: git repo pushed to a bare
/// origin, `.aristo/index.toml` + `.aristo/canon-matches.toml` for one
/// canon-bound entry. Mirrors `verify_canon_dispatch.rs`.
fn init_repo_with_pushed_head(dir: &Path) -> tempfile::TempDir {
    let bare = tempfile::tempdir().unwrap();
    run_git(bare.path(), &["init", "--bare", "-q"]);
    run_git(dir, &["init", "-q", "-b", "main"]);
    run_git(dir, &["config", "user.email", "t@x"]);
    run_git(dir, &["config", "user.name", "t"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);
    run_git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    run_git(
        dir,
        &["remote", "set-url", "origin", bare.path().to_str().unwrap()],
    );
    fs::write(dir.join("README"), b"seed").unwrap();
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-q", "--no-verify", "-m", "init"]);
    run_git(dir, &["push", "-q", "origin", "main"]);
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

fn workspace_with_one_canon_bound_full_intent(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
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

fn write_aretta_token(home: &Path, server_url: &str) {
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

fn write_full_fixture(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("verify-fixture.json");
    fs::write(&path, body).unwrap();
    path
}

/// One-annotation terminal GET fixture: `aristos:foo` with the given
/// annotation status + single test status. No DifferentialReport — the
/// waiver join keys on the annotation status, not the report body.
fn one_annotation_fixture(
    session: &str,
    ann_status: &str,
    test_status: &str,
    summary: &str,
) -> String {
    format!(
        r#"{{
      "post": {{ "session_id": "{session}", "view_url": "https://x", "plan_size": 1 }},
      "gets": [
        {{
          "session_id": "{session}",
          "status": "done",
          "user_commit_sha": "abc1234567890",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-24T00:00:00Z",
          "completed_at": "2026-05-24T00:01:00Z",
          "annotations": [
            {{
              "annotation_id": "arta_op4q3z9NbV",
              "canon_id": "foo",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "src/foo.rs:42",
              "status": "{ann_status}",
              "tests": [{{ "test_binary": "foo_conform", "status": "{test_status}", "duration_ms": 12 }}]
            }}
          ],
          "summary": {summary}
        }}
      ]
    }}"#
    )
}

fn expectations_toml(dir: &Path) -> String {
    fs::read_to_string(dir.join(".aristo/expectations.toml"))
        .unwrap_or_else(|e| panic!("expected .aristo/expectations.toml to exist: {e}"))
}

// ─── --accept write flow ────────────────────────────────────────────────────

#[test]
fn accept_writes_expectations_file_keyed_on_prefixed_canon_id() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:foo",
            "--because",
            "turso reports initialized from a file-existence proxy",
        ])
        .assert()
        .success()
        .stdout(contains("aristos:foo"));

    let toml = expectations_toml(tmp.path());
    assert!(
        toml.contains("[\"aristos:foo\"]"),
        "keyed on prefixed id; got:\n{toml}"
    );
    assert!(
        toml.contains("turso reports initialized from a file-existence proxy"),
        "reason recorded verbatim; got:\n{toml}"
    );
    // The schema-version meta header, mirroring the other sidecars.
    assert!(
        toml.contains("schema_version"),
        "meta header present; got:\n{toml}"
    );
}

#[test]
fn accept_records_optional_tracking_ref() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:foo",
            "--because",
            "tracked upstream",
            "--tracking",
            "https://github.com/tursodatabase/turso/issues/1234",
        ])
        .assert()
        .success();

    let toml = expectations_toml(tmp.path());
    assert!(
        toml.contains("https://github.com/tursodatabase/turso/issues/1234"),
        "tracking ref recorded; got:\n{toml}"
    );
}

#[test]
fn accept_requires_a_reason() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    // No --because: clap usage error (exit 2). A reasonless waiver is
    // how baselines rot, so the reason is mandatory at the arg layer.
    aristo_in(tmp.path())
        .args(["verify", "--accept", "aristos:foo"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("because"));

    assert!(
        !tmp.path().join(".aristo/expectations.toml").exists(),
        "a rejected --accept must not write the expectations file"
    );
}

#[test]
fn accept_bare_canon_suffix_resolves_to_prefixed_entry() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    // Bare `foo` is shorthand for the canon-bound `aristos:foo`.
    aristo_in(tmp.path())
        .args(["verify", "--accept", "foo", "--because", "shorthand works"])
        .assert()
        .success();

    let toml = expectations_toml(tmp.path());
    assert!(
        toml.contains("[\"aristos:foo\"]"),
        "bare suffix resolves to the prefixed id; got:\n{toml}"
    );
}

#[test]
fn accept_rejects_unknown_canon_id() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:nonexistent",
            "--because",
            "x",
        ])
        .assert()
        .failure()
        .stderr(contains("nonexistent"));

    assert!(
        !tmp.path().join(".aristo/expectations.toml").exists(),
        "unknown id must not write the expectations file"
    );
}

#[test]
fn accept_rejects_opaque_server_id() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    // `arta_*` is the server-side opaque ref; users never waive by it.
    aristo_in(tmp.path())
        .args(["verify", "--accept", "arta_op4q3z9NbV", "--because", "x"])
        .assert()
        .failure()
        .stderr(contains("arta_"));
}

#[test]
fn accept_is_idempotent_and_updates_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    aristo_in(tmp.path())
        .args(["verify", "--accept", "foo", "--because", "first reason"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["verify", "--accept", "foo", "--because", "second reason"])
        .assert()
        .success();

    let toml = expectations_toml(tmp.path());
    assert!(
        toml.contains("second reason"),
        "reason updated; got:\n{toml}"
    );
    assert!(
        !toml.contains("first reason"),
        "old reason replaced; got:\n{toml}"
    );
    assert_eq!(
        toml.matches("[\"aristos:foo\"]").count(),
        1,
        "exactly one entry after re-accept; got:\n{toml}"
    );
}

// ─── read-time join (--wait) ────────────────────────────────────────────────

#[test]
fn waived_failing_annotation_renders_accepted_gap_and_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    // Accept the gap first.
    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:foo",
            "--because",
            "turso uses a file-existence proxy; tracked upstream",
        ])
        .assert()
        .success();

    let fixture = one_annotation_fixture(
        "01HMGAP",
        "failed",
        "fail",
        r#"{ "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#,
    );
    let fixture_path = write_full_fixture(tmp.path(), &fixture);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        // The waiver suppresses the red exit: a known, accepted gap is
        // not a build failure.
        .assert()
        .success()
        .stdout(contains("known gap (accepted)"))
        .stdout(contains(
            "turso uses a file-existence proxy; tracked upstream",
        ))
        // The internal conformance frame is never shown for a waived gap.
        .stdout(contains("EXPECTED TO FAIL").not());
}

#[test]
fn waived_annotation_that_now_passes_trips_the_strict_ratchet() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:foo",
            "--because",
            "stale gap",
        ])
        .assert()
        .success();

    // The property now HOLDS (verified / pass) but the waiver is still
    // on disk → the ratchet fires.
    let fixture = one_annotation_fixture(
        "01HMRATCHET",
        "verified",
        "pass",
        r#"{ "total_annotations": 1, "verified": 1, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#,
    );
    let fixture_path = write_full_fixture(tmp.path(), &fixture);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stdout(contains("accepted gap now passes"))
        .stdout(contains("expectations.toml"))
        .stdout(contains("aristos:foo"));
}

#[test]
fn unwaived_failure_is_unchanged_and_still_red() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    // No --accept: the expectations file does not exist.
    let fixture = one_annotation_fixture(
        "01HMRED",
        "failed",
        "fail",
        r#"{ "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#,
    );
    let fixture_path = write_full_fixture(tmp.path(), &fixture);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stdout(contains("known gap (accepted)").not());
}

// ─── hardening (from the adversarial review) ─────────────────────────────────

#[test]
fn accept_rejects_an_empty_reason() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    // clap enforces flag PRESENCE; the command enforces a non-empty value
    // (a reasonless waiver is how baselines rot).
    aristo_in(tmp.path())
        .args(["verify", "--accept", "foo", "--because", "   "])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("non-empty reason"));

    assert!(
        !tmp.path().join(".aristo/expectations.toml").exists(),
        "a blank reason must not write the expectations file"
    );
}

#[test]
fn accept_conflicts_with_dispatch_and_ci_flags() {
    let tmp = tempfile::tempdir().unwrap();
    lean_workspace_with_canon_entry(tmp.path());

    // --accept is write-only; combining it with the no-write CI mode (or any
    // dispatch flag) is a clap usage error, never a silent write.
    aristo_in(tmp.path())
        .args(["verify", "--accept", "foo", "--because", "x", "--check"])
        .assert()
        .failure()
        .code(2);

    assert!(
        !tmp.path().join(".aristo/expectations.toml").exists(),
        "a rejected flag combo must not write the expectations file"
    );
}

#[test]
fn malformed_expectations_hard_errors_verify_wait() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    // A malformed committed sidecar surfaces LOUDLY rather than silently
    // dropping the user's waivers (and flipping accepted gaps to red).
    fs::write(
        tmp.path().join(".aristo/expectations.toml"),
        "this is = = not valid toml [[[",
    )
    .unwrap();

    let fixture = one_annotation_fixture(
        "01HMBAD",
        "failed",
        "fail",
        r#"{ "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#,
    );
    let fixture_path = write_full_fixture(tmp.path(), &fixture);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stderr(contains("expectations.toml"));
}

#[test]
fn waived_annotation_with_an_operational_test_is_red_not_an_accepted_gap() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:foo",
            "--because",
            "the property gap",
        ])
        .assert()
        .success();

    // Annotation aggregates to `failed` (precedence), but one of its tests
    // is a build failure — an operational break the waiver must NOT mask.
    let fixture = r#"{
      "post": { "session_id": "01HMOP", "view_url": "https://x", "plan_size": 1 },
      "gets": [{
        "session_id": "01HMOP", "status": "done", "user_commit_sha": "abc1234567890",
        "canon_version": "v0.1.0", "started_at": "2026-05-24T00:00:00Z", "completed_at": "2026-05-24T00:01:00Z",
        "annotations": [{
          "annotation_id": "arta_op4q3z9NbV", "canon_id": "foo", "version": "v0.1.0",
          "scope": "turso", "tier": "aristos:", "source_path": "src/foo.rs:42", "status": "failed",
          "tests": [
            { "test_binary": "foo_conform", "status": "fail" },
            { "test_binary": "foo_build", "status": "build_failed" }
          ]
        }],
        "summary": { "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }
      }]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .failure()
        .stdout(contains("known gap (accepted)").not());
}

#[test]
fn orphan_waiver_warns_when_it_matches_no_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    let _bare = init_repo_with_pushed_head(tmp.path());
    workspace_with_one_canon_bound_full_intent(tmp.path());
    let home = tempfile::tempdir().unwrap();
    write_aretta_token(home.path(), "https://example.test");

    aristo_in(tmp.path())
        .args([
            "verify",
            "--accept",
            "aristos:foo",
            "--because",
            "drifted gap",
        ])
        .assert()
        .success();

    // The session returns no annotation for the waived id (renamed / removed
    // upstream) → the stale waiver is surfaced, not silently rotting.
    let fixture = r#"{
      "post": { "session_id": "01HMORPH", "view_url": "https://x", "plan_size": 1 },
      "gets": [{
        "session_id": "01HMORPH", "status": "done", "user_commit_sha": "abc1234567890",
        "canon_version": "v0.1.0", "started_at": "2026-05-24T00:00:00Z", "completed_at": "2026-05-24T00:01:00Z",
        "annotations": [],
        "summary": { "total_annotations": 0, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }
      }]
    }"#;
    let fixture_path = write_full_fixture(tmp.path(), fixture);

    aristo_in(tmp.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("ARISTO_CANON_VERIFY_FIXTURE", &fixture_path)
        .env("ARISTO_VERIFY_POLL_MS", "1")
        .args(["verify", "--wait"])
        .assert()
        .success()
        .stderr(contains("aristos:foo"))
        .stderr(contains("matched no annotation"));
}
