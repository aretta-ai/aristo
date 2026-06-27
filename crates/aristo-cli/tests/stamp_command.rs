//! `aristo stamp` — imperative integration tests for the proofs-sourced status
//! model, orphan-proof archival, deterministic-id stability, and `--check`.

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

/// Write a VALID `.proof` for `id` whose anchors match the entry's current
/// hashes, so the proofs-join derives the verdict's status. Status is sourced
/// from `.aristo/proofs/` now (Option B), not poked into the index directly.
fn write_valid_proof(root: &Path, id: &str, kind: aristo_core::proof::VerdictType) {
    use aristo_core::index::{AnnotationKind, IndexEntry, VerifyLevel};
    use aristo_core::proof::{
        CounterexampleBody, Gap, Ground, InconclusiveBody, Proof, ProofFile, ProofStep,
        PropertyKind, RelationKind, SuggestedAnnotation, VerdictMeta, VerdictType, VerifiedBody,
        Violation,
    };
    let idx = read_index(root);
    let (text_h, body_h, method) = match lookup(&idx, id) {
        IndexEntry::Intent(e) => {
            let m = match e.verify {
                VerifyLevel::Method(m) => m,
                _ => panic!("entry `{id}` must declare a verify method"),
            };
            (e.text_hash.clone(), e.body_hash.clone(), m)
        }
        _ => panic!("expected Intent for `{id}`"),
    };
    let step = || ProofStep {
        path: "0".into(),
        claim: "by construction".into(),
        relation_to_parent: RelationKind::Decomposes,
        grounds: vec![Ground::Composition {
            reason: "trivial".into(),
        }],
        subgoal_paths: vec![],
        proposed_promotion: false,
    };
    let (verified, counterexample, inconclusive) = match kind {
        VerdictType::Verified => (
            Some(VerifiedBody {
                proof: Proof {
                    conclusion: "holds".into(),
                    steps: vec![step()],
                },
            }),
            None,
            None,
        ),
        VerdictType::Counterexample => (
            None,
            Some(CounterexampleBody {
                violation: Violation {
                    description: "refuted".into(),
                    violated_step_path: "0".into(),
                    trigger_steps: vec![step()],
                    refuted_grounds: vec![],
                },
            }),
            None,
        ),
        VerdictType::Inconclusive => (
            None,
            None,
            Some(InconclusiveBody {
                partial_proof: None,
                gap: Gap {
                    description: "a subgoal could not be discharged".into(),
                    unfilled_path: "0".into(),
                    suggested_annotations: vec![SuggestedAnnotation {
                        kind: AnnotationKind::Assume,
                        suggested_text: "a fresh unrelated invariant to close the gap".into(),
                        at_site: "fn x (line 1)".into(),
                        rationale: "needed".into(),
                        would_close_path: None,
                    }],
                },
            }),
        ),
    };
    let pf = ProofFile {
        verdict: VerdictMeta {
            r#type: kind,
            method,
            produced_at_text_hash: text_h,
            produced_at_body_hash: body_h,
            produced_by: "test".into(),
            verifier_model: None,
            attempts: 1,
            property_kind: PropertyKind::Invariant,
        },
        verified,
        counterexample,
        inconclusive,
    };
    let p = root
        .join(".aristo/proofs")
        .join(format!("{}.proof", id.replace(':', "__")));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, pf.to_toml().unwrap()).unwrap();
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
        .stdout(contains("unverified: 1"));

    let idx = read_index(tmp.path());
    assert_eq!(idx.entries.len(), 1);
}

#[test]
fn stamp_sources_status_from_a_matching_proof() {
    // Status comes from .aristo/proofs/ now: a valid proof with matching
    // anchors yields its verdict, and a no-op re-stamp keeps it.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    write_valid_proof(
        tmp.path(),
        "a",
        aristo_core::proof::VerdictType::Inconclusive,
    );

    aristo_in(tmp.path()).arg("stamp").assert().success();
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&read_index(tmp.path()), "a") {
        assert_eq!(e.status, aristo_core::index::Status::Inconclusive);
    }
    // No-op re-stamp keeps the derived status.
    aristo_in(tmp.path()).arg("stamp").assert().success();
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&read_index(tmp.path()), "a") {
        assert_eq!(e.status, aristo_core::index::Status::Inconclusive);
    }
}

#[test]
fn stamp_marks_stale_when_body_drifts_from_proof() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    write_valid_proof(
        tmp.path(),
        "a",
        aristo_core::proof::VerdictType::Inconclusive,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Edit body — the proof's body_hash anchor no longer matches → Stale.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 99 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&read_index(tmp.path()), "a") {
        assert_eq!(e.status, aristo_core::index::Status::Stale);
    }
}

#[test]
fn stamp_warns_loudly_on_counterexample_proof() {
    // Counterexamples are loud, never silenceable. A counterexample .proof
    // makes the entry Counterexample, and every stamp re-surfaces it on stderr.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a refuted claim", verify = "neural", id = "refuted_one")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    write_valid_proof(
        tmp.path(),
        "refuted_one",
        aristo_core::proof::VerdictType::Counterexample,
    );

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
    write_valid_proof(
        tmp.path(),
        "refuted_two",
        aristo_core::proof::VerdictType::Counterexample,
    );
    // Sync the committed cache to the proof first, so --check sees no drift.
    aristo_in(tmp.path()).arg("stamp").assert().success();

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
fn stamp_marks_stale_when_text_drifts_from_proof() {
    // GAP-8 strict: text drift on a verdict-bearing entry transitions to
    // Stale, same as body drift — the proof's text_hash anchor no longer
    // matches. The system can't tell "fixed a typo" from "narrowed the claim".
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("v1", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    write_valid_proof(
        tmp.path(),
        "a",
        aristo_core::proof::VerdictType::Inconclusive,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Edit ONLY the intent text (re-word the prose); body unchanged.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("v2", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&read_index(tmp.path()), "a") {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Stale,
            "text drift transitions to Stale (GAP-8 strict)"
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
    aristo_in(tmp.path()).arg("stamp").assert().success();

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

    // A valid proof for the deterministic id derives Inconclusive.
    write_valid_proof(
        tmp.path(),
        &id1,
        aristo_core::proof::VerdictType::Inconclusive,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let proof_path = tmp
        .path()
        .join(".aristo/proofs")
        .join(format!("{id1}.proof"));

    // Re-stamp with NO source change: id stable, proof-derived status + proof survive.
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let idx2 = read_index(tmp.path());
    assert_eq!(
        only_id(&idx2),
        id1,
        "deterministic id must be stable across stamps"
    );
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx2, &id1) {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Inconclusive,
            "proof-derived status must survive a no-op re-stamp"
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
    // A valid proof for the id; a body edit must KEEP the proof (id is stable,
    // so the entry is never orphaned → never archived) but flip status to Stale.
    write_valid_proof(
        tmp.path(),
        &id1,
        aristo_core::proof::VerdictType::Inconclusive,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let proof_path = tmp
        .path()
        .join(".aristo/proofs")
        .join(format!("{id1}.proof"));

    // Edit ONLY the body — same intent text, same fn name (site).
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("returns a constant", verify = "neural")] fn k() -> i32 { 2 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let idx2 = read_index(tmp.path());
    assert_eq!(only_id(&idx2), id1, "body edit must not change the id");
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx2, &id1) {
        assert_eq!(e.status, aristo_core::index::Status::Stale);
    } else {
        panic!("expected Intent");
    }
    assert!(
        proof_path.exists(),
        "body edit keeps the proof in the active set (id unchanged → not orphaned)"
    );
    assert!(
        !tmp.path()
            .join(format!(".aristo/archive/proofs/{id1}.proof"))
            .exists(),
        "body edit must NOT archive the proof"
    );
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
    let ids1: Vec<String> = read_index(tmp.path())
        .entries
        .keys()
        .map(|k| k.as_str().to_owned())
        .collect();
    assert_eq!(
        ids1.len(),
        2,
        "duplicate-text intents must both index (ordinal disambiguates)"
    );

    // Re-stamp unchanged source: the SAME two ids must reappear (the ordinal
    // path is itself deterministic). Under the old random scheme the two
    // would be fresh ids every stamp, so this is a real regression guard.
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let ids2: Vec<String> = read_index(tmp.path())
        .entries
        .keys()
        .map(|k| k.as_str().to_owned())
        .collect();
    assert_eq!(
        ids1, ids2,
        "ordinal-disambiguated ids must be stable across stamps"
    );
}

#[test]
fn stamp_explicit_id_untouched_while_idless_reword_changes() {
    // explicit-ids-untouched: a user-written id= passes through verbatim and
    // is NEVER re-hashed into an aret_ id — it stays byte-identical even
    // across a reword (the exact edit that DOES re-anchor an idless id).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("explicit one", verify = "test", id = "explicit_kept")] fn a() {}
            #[aristo::intent("idless one", verify = "test")] fn b() {}
        "#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let idx1 = read_index(tmp.path());
    let explicit = aristo_core::index::AnnotationId::parse("explicit_kept").unwrap();
    assert!(
        idx1.entries.contains_key(&explicit),
        "explicit id used verbatim"
    );
    let idless1 = idx1
        .entries
        .keys()
        .find(|k| k.as_str().starts_with("aret_"))
        .expect("idless sibling got an aret_ id")
        .as_str()
        .to_owned();

    // Reword BOTH texts.
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("explicit one reworded", verify = "test", id = "explicit_kept")] fn a() {}
            #[aristo::intent("idless one reworded", verify = "test")] fn b() {}
        "#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let idx2 = read_index(tmp.path());
    assert!(
        idx2.entries.contains_key(&explicit),
        "explicit id is untouched by a reword (still aliases the user string)"
    );
    let idless2 = idx2
        .entries
        .keys()
        .find(|k| k.as_str().starts_with("aret_"))
        .expect("idless sibling still has an aret_ id")
        .as_str()
        .to_owned();
    assert_ne!(idless1, idless2, "the idless id re-anchors on a reword");
}

#[test]
fn stamp_double_orphan_does_not_clobber_earlier_archived_proof() {
    // ID-D5 invariant: archiving never LOSES a verdict. If the same id is
    // orphaned twice (removed, re-added with a different proof, removed
    // again), the second archive must not overwrite the first.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    let src = r#"#[aristo::intent("doomed", verify = "neural", id = "doomed")] fn d() {}"#;
    let proofs_dir = tmp.path().join(".aristo/proofs");
    let archive_dir = tmp.path().join(".aristo/archive/proofs");

    // Round 1: stamp, plant proof v1, remove → archives doomed.proof (v1).
    write_lib(tmp.path(), src);
    aristo_in(tmp.path()).arg("stamp").assert().success();
    fs::create_dir_all(&proofs_dir).unwrap();
    fs::write(proofs_dir.join("doomed.proof"), "v1\n").unwrap();
    write_lib(tmp.path(), "// nothing\n");
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Round 2: re-add same id, plant proof v2, remove again → must archive
    // v2 WITHOUT clobbering the archived v1.
    write_lib(tmp.path(), src);
    aristo_in(tmp.path()).arg("stamp").assert().success();
    fs::write(proofs_dir.join("doomed.proof"), "v2\n").unwrap();
    write_lib(tmp.path(), "// nothing\n");
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let archived: Vec<String> = fs::read_dir(&archive_dir)
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(archived.len(), 2, "both archived verdicts must be retained");
    assert!(archived.contains(&"v1\n".to_string()), "v1 must survive");
    assert!(archived.contains(&"v2\n".to_string()), "v2 must survive");
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
