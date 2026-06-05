//! End-to-end coverage for `aristo install-skills --check` / `--update`
//! staleness handling: the clap wiring, the non-zero exit on stale, and the
//! heal. Drives the real binary against an isolated project + `$HOME` so it
//! never touches the developer's actual `~/.claude`.

use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `aristo` invocation pinned to an isolated project dir + `$HOME`. The
/// post-command notice is suppressed automatically (stderr isn't a TTY under
/// the test harness), so stdout is the only surface we assert on.
fn aristo(proj: &Path, home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(proj).env("HOME", home);
    cmd
}

#[test]
fn check_then_update_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    // Install project-level Claude Code skills.
    aristo(&proj, &home)
        .args(["install-skills", "--agent", "claude-code"])
        .assert()
        .success();

    // --check on a fresh install: current, exit 0.
    aristo(&proj, &home)
        .args(["install-skills", "--check"])
        .assert()
        .success()
        .stdout(contains("all installed skills are current"));

    // Tamper one installed skill to simulate an older install.
    let tampered = proj.join(".claude/skills/aristo-authoring/SKILL.md");
    std::fs::write(&tampered, "stale\n").unwrap();

    // --check now reports staleness AND exits non-zero (gate behavior).
    aristo(&proj, &home)
        .args(["install-skills", "--check"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("out of date"))
        .stdout(contains("--update"));

    // --update heals in place.
    aristo(&proj, &home)
        .args(["install-skills", "--update"])
        .assert()
        .success()
        .stdout(contains("re-pinned"));

    // The tampered file was rewritten to the binary's content.
    assert_ne!(std::fs::read_to_string(&tampered).unwrap(), "stale\n");

    // --check is clean again.
    aristo(&proj, &home)
        .args(["install-skills", "--check"])
        .assert()
        .success()
        .stdout(contains("all installed skills are current"));
}

#[test]
fn check_reports_nothing_when_no_skills_installed() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    // No install — `--check` is a clean no-op (exit 0), not an error.
    aristo(&proj, &home)
        .args(["install-skills", "--check"])
        .assert()
        .success()
        .stdout(contains("none installed"));
}

#[test]
fn update_with_no_install_is_a_clean_noop() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    aristo(&proj, &home)
        .args(["install-skills", "--update"])
        .assert()
        .success()
        .stdout(contains("nothing to update"));
}

#[test]
fn hook_format_emits_sessionstart_context_only_when_stale() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    aristo(&proj, &home)
        .args(["install-skills", "--agent", "claude-code"])
        .assert()
        .success();

    // Fresh install → hook stays silent (no context, exit 0).
    aristo(&proj, &home)
        .args(["install-skills", "--hook-format"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());

    // Tamper → hook emits a SessionStart additionalContext block that tells
    // the agent to offer a refresh.
    std::fs::write(
        proj.join(".claude/skills/aristo-authoring/SKILL.md"),
        "stale\n",
    )
    .unwrap();
    aristo(&proj, &home)
        .args(["install-skills", "--hook-format"])
        .assert()
        .success()
        .stdout(contains("\"hookEventName\":\"SessionStart\""))
        .stdout(contains("additionalContext"))
        .stdout(contains("aristo install-skills --update"));
}
