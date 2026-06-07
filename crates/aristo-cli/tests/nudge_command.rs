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
