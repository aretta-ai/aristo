//! `aristo stamp` — imperative integration tests for drift detection,
//! `--check` CI mode, and the prior-status preservation contract.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn read_index(root: &Path) -> aristo_core::index::IndexFile {
    let text = fs::read_to_string(root.join(".aristo/index.toml")).unwrap();
    toml::from_str(&text).expect("index round-trips")
}

fn write_lib(root: &Path, content: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), content).unwrap();
}

fn lookup<'a>(
    idx: &'a aristo_core::index::IndexFile,
    id: &str,
) -> &'a aristo_core::index::IndexEntry {
    let parsed = aristo_core::index::AnnotationId::parse(id).unwrap();
    idx.entries
        .get(&parsed)
        .unwrap_or_else(|| panic!("no entry `{id}`"))
}

fn force_status(root: &Path, id: &str, status: aristo_core::index::Status) {
    // Read, mutate, write — emulates what `aristo verify` will do later
    // when verification flips an entry's status.
    let mut idx = read_index(root);
    let parsed = aristo_core::index::AnnotationId::parse(id).unwrap();
    let entry = idx.entries.get_mut(&parsed).unwrap();
    match entry {
        aristo_core::index::IndexEntry::Intent(e) => e.status = status,
        aristo_core::index::IndexEntry::Assume(e) => e.status = status,
    }
    let text = toml::to_string_pretty(&idx).unwrap();
    fs::write(root.join(".aristo/index.toml"), text).unwrap();
}

#[test]
fn stamp_on_fresh_workspace_writes_initial_index() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("hello", verify = "test", id = "greeting")] fn x() {}"#,
    );

    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("ok: stamped 1 annotation"))
        .stdout(contains("new: 1"));

    let idx = read_index(tmp.path());
    assert_eq!(idx.entries.len(), 1);
}

#[test]
fn stamp_preserves_status_when_body_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Simulate a prior `aristo verify` having set status to Tested.
    force_status(tmp.path(), "a", aristo_core::index::Status::Tested);

    // Re-stamp without source changes — Tested must be preserved.
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Tested,
            "body unchanged → status preserved"
        );
    }
}

#[test]
fn stamp_flips_verified_to_stale_on_body_change() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(tmp.path(), "a", aristo_core::index::Status::Tested);

    // Edit body — same text, different body.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 99 }"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("body-drifted: 1"))
        .stdout(contains("now Stale"));

    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(e.status, aristo_core::index::Status::Stale);
    }
}

#[test]
fn stamp_flips_counterexample_to_stale_on_body_change() {
    // GAP-1 fix: the Status enum docstring promises body-drift → Stale for
    // Counterexample too, but the prior stamp.rs match arm only handled
    // Verified/Tested/Neural. Counterexample sat unchanged.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(tmp.path(), "a", aristo_core::index::Status::Counterexample);

    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 99 }"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("body-drifted: 1"))
        .stdout(contains("now Stale"));

    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(e.status, aristo_core::index::Status::Stale);
    }
}

#[test]
fn stamp_warns_loudly_on_counterexample_entries() {
    // Per the design: counterexamples are loud, never silenceable. There is
    // no `aristo accept-counterexample` command — every stamp run must
    // re-surface them. The warning goes to stderr (CI-visible, distinct
    // from the regular per-id summary).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a refuted claim", verify = "neural", id = "refuted_one")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(
        tmp.path(),
        "refuted_one",
        aristo_core::index::Status::Counterexample,
    );

    // Re-run stamp: source unchanged, status is sticky Counterexample.
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stderr(contains("refuted by counterexample"))
        .stderr(contains("refuted_one"));
}

#[test]
fn stamp_check_mode_also_surfaces_counterexamples() {
    // The warning fires even when --check makes no writes; CI gates use
    // --check and must still see refutations.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a refuted claim", verify = "neural", id = "refuted_two")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(
        tmp.path(),
        "refuted_two",
        aristo_core::index::Status::Counterexample,
    );

    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .success()
        .stderr(contains("refuted by counterexample"));
}

#[test]
fn stamp_archives_orphan_proof_when_annotation_removed() {
    // ID-D5: when an annotation is removed from source, stamp MOVES its
    // orphan .aristo/proofs/<id>.proof into .aristo/archive/proofs/ instead
    // of hard-deleting it. The proof leaves the active set (so re-introducing
    // the id can't re-attach a stale verdict) but stays recoverable — which
    // is what makes a legitimate id change (reword/rename) non-destructive.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("doomed", verify = "neural", id = "doomed")] fn d() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Plant an orphan-bait proof file at the id-mapped path.
    let proofs_dir = tmp.path().join(".aristo/proofs");
    fs::create_dir_all(&proofs_dir).unwrap();
    fs::write(proofs_dir.join("doomed.proof"), "[verdict]\nfake = true\n").unwrap();
    assert!(proofs_dir.join("doomed.proof").exists());

    // Remove the annotation from source entirely.
    write_lib(tmp.path(), "// no annotations\n");
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stderr(contains("archived orphan proof"))
        .stderr(contains("doomed.proof"));

    let archived = tmp.path().join(".aristo/archive/proofs/doomed.proof");
    assert!(
        !proofs_dir.join("doomed.proof").exists(),
        "orphan proof must leave the active proofs/ dir"
    );
    assert!(
        archived.exists(),
        "orphan proof must be archived (recoverable), not deleted"
    );
    assert_eq!(
        fs::read_to_string(&archived).unwrap(),
        "[verdict]\nfake = true\n",
        "archived proof must be the original file, byte-for-byte"
    );
}

#[test]
fn stamp_check_does_not_archive_orphan_proofs() {
    // --check is CI mode; must not mutate the workspace, even to archive
    // legitimate orphans. The summary still reports them.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("doomed", verify = "neural", id = "doomed")] fn d() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let proofs_dir = tmp.path().join(".aristo/proofs");
    fs::create_dir_all(&proofs_dir).unwrap();
    fs::write(proofs_dir.join("doomed.proof"), "[verdict]\nfake = true\n").unwrap();

    write_lib(tmp.path(), "// no annotations\n");
    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .failure(); // --check exits non-zero because the index would change

    assert!(
        proofs_dir.join("doomed.proof").exists(),
        "--check must NOT touch proof files (CI safety)"
    );
    assert!(
        !tmp.path()
            .join(".aristo/archive/proofs/doomed.proof")
            .exists(),
        "--check must NOT archive either"
    );
}

#[test]
fn stamp_gc_purges_archived_orphan_proofs() {
    // `aristo stamp --gc` is the only hard-delete path: it empties the
    // archive. Without --gc the archive accumulates (recoverable).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("doomed", verify = "neural", id = "doomed")] fn d() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let proofs_dir = tmp.path().join(".aristo/proofs");
    fs::create_dir_all(&proofs_dir).unwrap();
    fs::write(proofs_dir.join("doomed.proof"), "[verdict]\nfake = true\n").unwrap();

    // Remove → archives the proof.
    write_lib(tmp.path(), "// no annotations\n");
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let archived = tmp.path().join(".aristo/archive/proofs/doomed.proof");
    assert!(archived.exists(), "precondition: proof archived");

    // gc purges the archive.
    aristo_in(tmp.path())
        .args(["stamp", "--gc"])
        .assert()
        .success()
        .stdout(contains("gc: removed 1 archived proof"));
    assert!(!archived.exists(), "--gc must purge archived proofs");
}

#[test]
fn stamp_reword_archives_old_proof_keeping_it_recoverable() {
    // The headline ID-D5 property: when an idless annotation is reworded its
    // deterministic id changes, so the OLD id's proof is orphaned. It must be
    // ARCHIVED (recoverable), never hard-deleted — a reword should not
    // silently destroy verification work.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns one", verify = "neural")] fn k() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let id1 = only_id(&read_index(tmp.path()));
    let proofs_dir = tmp.path().join(".aristo/proofs");
    fs::create_dir_all(&proofs_dir).unwrap();
    fs::write(
        proofs_dir.join(format!("{id1}.proof")),
        "[verdict]\nok = true\n",
    )
    .unwrap();

    // Reword → id changes (old id removed, new id new).
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns the value one", verify = "neural")] fn k() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let id2 = only_id(&read_index(tmp.path()));
    assert_ne!(id2, id1);
    assert!(
        !proofs_dir.join(format!("{id1}.proof")).exists(),
        "old id's proof leaves the active set"
    );
    assert!(
        tmp.path()
            .join(format!(".aristo/archive/proofs/{id1}.proof"))
            .exists(),
        "old id's proof is archived, not destroyed by the reword"
    );
}

#[test]
fn stamp_flips_text_drift_to_stale() {
    // GAP-8 strict: text drift on a verdict-bearing entry transitions to
    // Stale, same as body drift. The system can't tell "fixed a typo"
    // from "narrowed the claim to exclude the failure case"; safer to
    // force re-verify and let the user explicitly opt back in via
    // --rerun on no-op text edits.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("v1", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(tmp.path(), "a", aristo_core::index::Status::Tested);

    // Edit ONLY text (re-word the intent prose); body unchanged.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("v2", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("text-changed: 1"))
        .stdout(contains("now Stale"));

    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Stale,
            "text drift on a verified entry transitions to Stale (GAP-8 strict)"
        );
        assert_eq!(e.text, "v2");
    }
}

#[test]
fn stamp_drops_removed_annotations_from_index() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("keep", verify = "test", id = "kept")] fn k() {}
            #[aristo::intent("drop", verify = "test", id = "dropped")] fn d() {}
        "#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Remove the second annotation by editing source.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("keep", verify = "test", id = "kept")] fn k() {}"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("removed: 1"));

    let idx = read_index(tmp.path());
    assert_eq!(idx.entries.len(), 1);
    assert!(idx
        .entries
        .contains_key(&aristo_core::index::AnnotationId::parse("kept").unwrap()));
}

#[test]
fn check_mode_does_not_write_when_index_matches() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("x", verify = "test", id = "a")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let mtime_before = fs::metadata(tmp.path().join(".aristo/index.toml"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .success()
        .stdout(contains("up to date"));

    let mtime_after = fs::metadata(tmp.path().join(".aristo/index.toml"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime_before, mtime_after, "--check must not write");
}

#[test]
fn check_mode_exits_nonzero_when_index_is_stale() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Now ADD a new annotation in source — index is stale relative to source.
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("a", verify = "test", id = "a")] fn x() {}
            #[aristo::intent("b", verify = "test", id = "b")] fn y() {}
        "#,
    );

    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("out of sync"));
}

#[test]
fn check_mode_does_not_corrupt_existing_index_on_diff() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let before = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();

    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("a", verify = "test", id = "a")] fn x() {}
            #[aristo::intent("b", verify = "test", id = "b")] fn y() {}
        "#,
    );
    let _ = aristo_in(tmp.path()).args(["stamp", "--check"]).output();

    let after = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    assert_eq!(
        before, after,
        "--check must leave the index file byte-identical"
    );
}

// ─── deterministic ids (Phase 18 #13) ──────────────────────────────────────

fn only_id(idx: &aristo_core::index::IndexFile) -> String {
    assert_eq!(idx.entries.len(), 1, "expected exactly one entry");
    idx.entries.keys().next().unwrap().as_str().to_owned()
}

#[test]
fn stamp_idless_annotation_id_and_status_survive_restamp() {
    // REGRESSION (Task #13): idless annotations used to get a fresh RANDOM id
    // on every stamp, so the id-keyed index could never re-associate them —
    // each re-stamp read the entry as removed+new, silently resetting its
    // verification status to Unknown and cascade-deleting its `.proof`.
    // Deterministic ids fix it: the SAME annotation mints the SAME id, so
    // status AND proof survive a no-op re-stamp.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("the function returns a constant", verify = "neural")] fn k() -> i32 { 7 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let id1 = only_id(&read_index(tmp.path()));
    assert!(id1.starts_with("aret_"), "idless → opaque id, got {id1}");

    // Simulate a prior `aristo verify`: set Neural status + plant the proof.
    force_status(tmp.path(), &id1, aristo_core::index::Status::Neural);
    let proofs_dir = tmp.path().join(".aristo/proofs");
    fs::create_dir_all(&proofs_dir).unwrap();
    let proof_path = proofs_dir.join(format!("{id1}.proof"));
    fs::write(&proof_path, "[verdict]\nfake = true\n").unwrap();

    // Re-stamp with NO source change.
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("unchanged: 1"))
        .stdout(contains("removed: 0"));

    let idx2 = read_index(tmp.path());
    assert_eq!(
        only_id(&idx2),
        id1,
        "deterministic id must be stable across stamps"
    );
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx2, &id1) {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Neural,
            "status must survive a no-op re-stamp (the bug reset it to Unknown)"
        );
    } else {
        panic!("expected Intent");
    }
    assert!(
        proof_path.exists(),
        "proof must survive a no-op re-stamp (the bug cascade-deleted it)"
    );
}

#[test]
fn stamp_idless_body_edit_keeps_id_and_marks_stale() {
    // Editing the covered CODE (not the text) must NOT change a deterministic
    // id — body drift is tracked separately via body_hash. Id stable; status
    // flips to Stale so the user re-verifies against the new code.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns a constant", verify = "neural")] fn k() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let id1 = only_id(&read_index(tmp.path()));
    force_status(tmp.path(), &id1, aristo_core::index::Status::Neural);

    // Edit ONLY the body — same intent text, same fn name (site).
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns a constant", verify = "neural")] fn k() -> i32 { 2 }"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("body-drifted: 1"));

    let idx2 = read_index(tmp.path());
    assert_eq!(only_id(&idx2), id1, "body edit must not change the id");
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx2, &id1) {
        assert_eq!(e.status, aristo_core::index::Status::Stale);
    } else {
        panic!("expected Intent");
    }
}

#[test]
fn stamp_idless_reword_changes_id() {
    // Rewording the claim IS an identity change → the deterministic id
    // changes (old id removed, new id new).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns one", verify = "neural")] fn k() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let id1 = only_id(&read_index(tmp.path()));

    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns the value one", verify = "neural")] fn k() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let id2 = only_id(&read_index(tmp.path()));
    assert_ne!(id2, id1, "rewording the claim must change the id");
}

#[test]
fn stamp_idless_duplicates_get_distinct_ordinal_ids() {
    // Two idless annotations with identical kind+text+site must NOT collide —
    // the source-order ordinal disambiguates them so BOTH land in the index
    // (with random ids this happened by luck; deterministic ids would alias
    // without the ordinal).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            fn f() {
                aristo::intent_stmt!("loop body is independent");
                let _a = 1;
                aristo::intent_stmt!("loop body is independent");
                let _b = 2;
            }
        "#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let idx = read_index(tmp.path());
    assert_eq!(
        idx.entries.len(),
        2,
        "duplicate-text intents must both index (ordinal disambiguates)"
    );
}

#[test]
fn cycle_in_source_aborts_stamp_with_diagnostic() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("a", verify = "test", id = "a", parent = "b")] fn a() {}
            #[aristo::intent("b", verify = "test", id = "b", parent = "a")] fn b() {}
        "#,
    );

    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("cycle"))
        .stderr(contains("No files modified"));
}
