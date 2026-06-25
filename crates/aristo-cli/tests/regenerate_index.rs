//! Integration: read commands regenerate the index in memory from source when
//! `.aristo/index.toml` is absent (index-as-local-cache / Option B). Covers the
//! `load_index` -> `regenerate_index` -> walk/build_entries path end-to-end over
//! real annotated source, which the unit tests (empty workspace) do not.

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
fn reader_regenerates_index_from_source_when_index_absent() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a coverage claim", verify = "neural", id = "regenerated_prop")] fn x() -> i32 { 42 }"#,
    );

    // Delete the committed index so a reader MUST regenerate it in memory.
    fs::remove_file(tmp.path().join(".aristo/index.toml")).unwrap();
    assert!(!tmp.path().join(".aristo/index.toml").is_file());

    // `aristo doc` reads via load_index; with the index absent it regenerates
    // from source and must find + render the annotation.
    aristo_in(tmp.path()).arg("doc").assert().success();
    assert!(
        tmp.path().join(".aristo/doc/regenerated_prop.md").is_file(),
        "doc must regenerate the annotation from source with no committed index"
    );

    // `aristo graph` also reads via load_index and must surface the id.
    // (--include-orphans so the lone, edge-less node is rendered.)
    aristo_in(tmp.path())
        .args(["graph", "--include-orphans"])
        .assert()
        .success()
        .stdout(contains("regenerated_prop"));

    // Reading never recreates the index file: regeneration is in-memory only.
    assert!(!tmp.path().join(".aristo/index.toml").is_file());
}
