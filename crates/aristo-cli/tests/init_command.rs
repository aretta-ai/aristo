//! `aristo init` — imperative integration tests covering surfaces that
//! trycmd can't easily exercise:
//!
//! - File CONTENT (not just stdout) for `aristo.toml` and `.aristo/index.toml`.
//! - The git path: `.git/hooks/pre-commit` is installed only when the
//!   working dir is inside a git repo.
//! - Idempotence at the filesystem layer: re-running doesn't mutate
//!   existing files.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

#[test]
fn creates_all_state_files_in_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root).arg("init").assert().success();

    assert!(root.join("aristo.toml").is_file());
    assert!(root.join(".aristo").is_dir());
    assert!(root.join(".aristo/index.toml").is_file());
    assert!(root.join(".aristo/specs").is_dir());
    assert!(root.join(".aristo/doc").is_dir());

    // CI workflows are opt-in (--ci / --ci-verify) — not written by default.
    assert!(!root.join(".github/workflows/aristo.yml").exists());
    assert!(!root.join(".github/workflows/aristo-verify.yml").exists());

    // No git → no hook installed.
    assert!(!root.join(".git/hooks/pre-commit").exists());
}

#[test]
fn gitignores_all_runtime_aristo_artifacts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root).arg("init").assert().success();

    let gi = fs::read_to_string(root.join(".gitignore")).unwrap();
    // Every runtime / per-user / cache path the CLI writes must be ignored,
    // or it leaks into a consumer's `git status`.
    for entry in [
        ".aristo/index.toml",
        ".aristo/sessions/",
        ".aristo/nudge-state.toml",
        ".aristo/verify-queue/",
        ".aristo/critique-queue/",
        ".aristo/critiques/",
        ".aristo/proofs/*.proof.bak",
        ".aristo/archive/",
    ] {
        assert!(
            gi.lines().any(|l| l.trim() == entry),
            "`.gitignore` is missing runtime entry `{entry}`; got:\n{gi}"
        );
    }
    // Durable state must NOT be ignored — it is committed and propagates to CI.
    for durable in [
        ".aristo/proofs/",
        ".aristo/doc/",
        ".aristo/specs/",
        ".aristo/feedback/",
        "aristo.toml",
    ] {
        assert!(
            !gi.lines().any(|l| l.trim() == durable),
            "`.gitignore` must not ignore durable path `{durable}`; got:\n{gi}"
        );
    }
}

#[test]
fn gitignore_appends_to_existing_file_and_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // A pre-existing .gitignore with the user's own content.
    fs::write(root.join(".gitignore"), "/target\n*.log\n").unwrap();

    aristo_in(root).arg("init").assert().success();

    let after = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(after.contains("/target"), "existing content lost:\n{after}");
    assert!(after.contains("*.log"), "existing content lost:\n{after}");
    assert!(
        after.lines().any(|l| l.trim() == ".aristo/sessions/"),
        "aristo entries not appended to the existing .gitignore:\n{after}"
    );

    // Re-running must not duplicate any aristo entry.
    aristo_in(root).arg("init").assert().success();
    let twice = fs::read_to_string(root.join(".gitignore")).unwrap();
    let dupes = twice
        .lines()
        .filter(|l| l.trim() == ".aristo/index.toml")
        .count();
    assert_eq!(
        dupes, 1,
        "re-running init duplicated a .gitignore entry:\n{twice}"
    );
}

#[test]
fn writes_index_with_meta_only_zero_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root).arg("init").assert().success();

    let index = fs::read_to_string(root.join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile =
        toml::from_str(&index).expect("index round-trips through aristo-core");
    assert_eq!(
        parsed.meta.schema_version, 1,
        "schema_version must be 1 (current SDK version)"
    );
    assert!(
        parsed
            .meta
            .generated_by
            .as_deref()
            .unwrap()
            .contains("aristo init"),
        "generated_by should identify init: {:?}",
        parsed.meta.generated_by
    );
    assert!(
        parsed.meta.generated_at.is_some(),
        "generated_at must be set"
    );
    assert_eq!(
        parsed.entries.len(),
        0,
        "init writes zero annotation entries"
    );
}

#[test]
fn writes_aristo_toml_that_round_trips_as_default_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root).arg("init").assert().success();

    let cfg_text = fs::read_to_string(root.join("aristo.toml")).unwrap();
    let parsed: aristo_core::config::ConfigFile =
        toml::from_str(&cfg_text).expect("aristo.toml round-trips through aristo-core");
    assert_eq!(
        parsed,
        aristo_core::config::ConfigFile::default(),
        "init writes the canonical default config"
    );
}

#[test]
fn second_invocation_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root).arg("init").assert().success();

    // Capture file mtimes after first init.
    let cfg_mtime_before = fs::metadata(root.join("aristo.toml"))
        .unwrap()
        .modified()
        .unwrap();
    let idx_mtime_before = fs::metadata(root.join(".aristo/index.toml"))
        .unwrap()
        .modified()
        .unwrap();

    // Sleep a tick so any rewrite would have a different mtime. (1ms is enough
    // on macOS HFS+; pick something safely above filesystem mtime resolution.)
    std::thread::sleep(std::time::Duration::from_millis(50));

    aristo_in(root)
        .arg("init")
        .assert()
        .success()
        .stdout(contains("nothing to do."))
        .stdout(contains("already exists"));

    let cfg_mtime_after = fs::metadata(root.join("aristo.toml"))
        .unwrap()
        .modified()
        .unwrap();
    let idx_mtime_after = fs::metadata(root.join(".aristo/index.toml"))
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(
        cfg_mtime_before, cfg_mtime_after,
        "aristo.toml must not be rewritten on second init"
    );
    assert_eq!(
        idx_mtime_before, idx_mtime_after,
        ".aristo/index.toml must not be rewritten on second init"
    );
}

#[test]
fn installs_hook_inside_git_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // Set up a minimal .git/hooks structure to simulate a git repo.
    fs::create_dir_all(root.join(".git/hooks")).unwrap();

    aristo_in(root)
        .arg("init")
        .assert()
        .success()
        .stdout(contains("installed pre-commit hook"));

    let hook = root.join(".git/hooks/pre-commit");
    assert!(hook.is_file(), "expected pre-commit hook installed");

    let content = fs::read_to_string(&hook).unwrap();
    assert!(
        content.contains("Aristo pre-commit hook"),
        "hook content should identify itself; got:\n{content}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&hook).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "hook must be executable (0755)");
    }
}

#[test]
fn second_invocation_in_git_repo_notes_existing_hook() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".git/hooks")).unwrap();

    aristo_in(root).arg("init").assert().success();

    aristo_in(root)
        .arg("init")
        .assert()
        .success()
        .stdout(contains("pre-commit hook already installed"));
}

#[test]
fn ci_flag_writes_lite_workflow_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root).arg("init").arg("--ci").assert().success();

    let lite = root.join(".github/workflows/aristo.yml");
    assert!(lite.is_file(), "--ci writes the lite PR gate");
    assert!(
        !root.join(".github/workflows/aristo-verify.yml").exists(),
        "--ci does NOT write the verify workflow"
    );

    let content = fs::read_to_string(&lite).unwrap();
    assert!(
        content.contains("aretta-ai/aristo-action"),
        "lite gate runs via the shared action; got:\n{content}"
    );
    assert!(
        content.contains("checks: audit, lint, doc"),
        "lite gate runs audit/lint/doc; got:\n{content}"
    );
}

#[test]
fn ci_verify_flag_writes_both_workflows() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    aristo_in(root)
        .arg("init")
        .arg("--ci-verify")
        .assert()
        .success()
        // On completion, --ci-verify prints token-setup guidance.
        .stdout(contains("ARETTA_TOKEN"))
        .stdout(contains("aristo auth token"));

    // --ci-verify implies the lite gate, plus the verify workflow.
    assert!(
        root.join(".github/workflows/aristo.yml").is_file(),
        "--ci-verify also writes the lite gate"
    );
    let verify = root.join(".github/workflows/aristo-verify.yml");
    assert!(verify.is_file(), "--ci-verify writes the verify workflow");

    let content = fs::read_to_string(&verify).unwrap();
    assert!(
        content.contains("checks: verify"),
        "verify workflow runs the verify check; got:\n{content}"
    );
    assert!(
        content.contains("ARETTA_TOKEN"),
        "verify workflow wires the token secret; got:\n{content}"
    );
}
