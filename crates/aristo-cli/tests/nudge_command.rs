//! `aristo nudge` — engine readout integration tests (S0d.1).

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn write_lib(root: &Path, content: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), content).unwrap();
}

#[test]
fn nudge_surfaces_verify_backlog_for_an_unverified_intent() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    // Default aggressiveness (medium): a fully-unverified surface fires
    // verify_backlog (fraction 1.0 / base 0.25 = pressure 4.0).
    aristo_in(tmp.path())
        .arg("nudge")
        .assert()
        .success()
        .stdout(contains("nudge engine"))
        .stdout(contains("verify_backlog"));
}

#[test]
fn nudge_is_silent_when_aggressiveness_off() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    // Opt out via config — the hard global silence. `aristo init` already
    // writes `[nudges] aggressiveness = "medium"`, so flip that value rather
    // than appending a second (duplicate) table.
    let cfg = tmp.path().join("aristo.toml");
    let contents = fs::read_to_string(&cfg).unwrap();
    let flipped = contents.replace("aggressiveness = \"medium\"", "aggressiveness = \"off\"");
    assert_ne!(
        contents, flipped,
        "expected the default medium value to flip"
    );
    fs::write(&cfg, flipped).unwrap();

    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    aristo_in(tmp.path())
        .arg("nudge")
        .assert()
        .success()
        .stdout(contains("nothing would fire"));
}

#[test]
fn nudge_stop_event_emits_consolidated_agent_reminder() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    aristo_in(tmp.path())
        .args(["nudge", "--event", "stop"])
        .assert()
        .success()
        .stdout(contains("<system-reminder>"))
        .stdout(contains("offer the user"));
}

#[test]
fn nudge_session_start_captures_baseline_in_state() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    aristo_in(tmp.path())
        .args(["nudge", "--event", "session-start"])
        .assert()
        .success();

    let state = fs::read_to_string(tmp.path().join(".aristo/nudge-state.toml")).unwrap();
    assert!(
        state.contains("[baseline]"),
        "session-start must persist a baseline: {state}"
    );
}

#[test]
fn nudge_hook_event_is_silent_when_off() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    let cfg = tmp.path().join("aristo.toml");
    let contents = fs::read_to_string(&cfg).unwrap();
    fs::write(
        &cfg,
        contents.replace("aggressiveness = \"medium\"", "aggressiveness = \"off\""),
    )
    .unwrap();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    // off → the Stop hook emits nothing at all.
    aristo_in(tmp.path())
        .args(["nudge", "--event", "stop"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn nudge_unknown_event_is_silent() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["nudge", "--event", "bogus-event"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}
