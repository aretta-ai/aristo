//! Integration: `verify --audit` — the freshness gate that replaces
//! `aristo stamp --check` (index-as-local-cache / Option B). Regenerates from
//! source + `.aristo/proofs/` and, under `--strict`, reds on stale / refuted /
//! orphan, but NOT on never-verified `unknown`.

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
fn audit_strict_passes_when_only_unverified() {
    // A declared-but-never-verified annotation is a legitimate starting state,
    // not a regression, so --audit --strict must NOT fail on it. Also exercises
    // the no-committed-index path (the audit regenerates).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#[aristo::intent("a claim", verify = "neural", id = "p1")] fn x() -> i32 { 1 }"#,
    )
    .unwrap();
    fs::remove_file(tmp.path().join(".aristo/index.toml")).unwrap();

    aristo_in(tmp.path())
        .args(["verify", "--audit", "--strict"])
        .assert()
        .success()
        .stdout(contains("unknown (unverified): 1"));
}

#[test]
fn audit_strict_fails_on_orphan_proof() {
    // A `.proof` on disk whose id has no source annotation is a leftover
    // verification artifact; --strict reds on it.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    let proofs = tmp.path().join(".aristo/proofs");
    fs::create_dir_all(&proofs).unwrap();
    fs::write(proofs.join("ghost.proof"), "verdict = {}\n").unwrap();

    aristo_in(tmp.path())
        .args(["verify", "--audit", "--strict"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("orphan proof"));
}
