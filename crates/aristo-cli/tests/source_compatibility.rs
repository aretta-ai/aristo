//! Fixed Rust/C reference outputs for the source-provider refactor.
//!
//! Known issues pinned deliberately, not intended semantics:
//! Q1: Rust statement hashes cover only the marker, not the following code.
//!     https://github.com/aretta-ai/aristo/issues/108
//! Q2: Rust reconstructs LF bodies; C retains original CRLF bytes.
//!     https://github.com/aretta-ai/aristo/issues/109
//! Q3: Global collision ordinals let edits in a.rs rename unchanged b.c claims.
//!     https://github.com/aretta-ai/aristo/issues/110
//! Fixes must migrate identity/evidence and update these references explicitly.

use aristo_core::index::{AnnotationId, IndexFile};
use aristo_core::walk::{walk_directory, AnnotationForm, DiscoveredAnnotation, ParentRaw};
use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;

const EXPECTED: &str = include_str!("fixtures/source_compatibility/expected.json");

fn workspace(crlf: bool) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("aristo.toml"),
        "[doc]\ncommit_artifacts = false\n",
    )
    .unwrap();
    for (name, source) in [
        ("a.rs", include_str!("fixtures/source_compatibility/a.rs")),
        ("b.c", include_str!("fixtures/source_compatibility/b.c")),
    ] {
        // Construct both byte profiles independently of Git's checkout settings.
        let source = source.replace("\r\n", "\n");
        fs::write(
            tmp.path().join(name),
            if crlf {
                source.replace('\n', "\r\n")
            } else {
                source
            },
        )
        .unwrap();
    }
    tmp
}

fn expected(crlf: bool) -> Vec<Value> {
    let mut rows: Vec<Value> = serde_json::from_str(EXPECTED).unwrap();
    for row in &mut rows {
        if let Some(hash) = row.as_object_mut().unwrap().remove("crlf_body_hash") {
            if crlf {
                row["body_hash"] = hash;
            }
        }
    }
    rows
}

fn extraction(d: &DiscoveredAnnotation) -> Value {
    let a = &d.annotation;
    let parent = match &a.parent {
        None => Value::Null,
        Some(ParentRaw::Single(id)) => json!(id),
        Some(ParentRaw::Multiple(ids)) => json!(ids),
    };
    let form = match a.form {
        AnnotationForm::Attribute => "attribute",
        AnnotationForm::Statement => "statement",
    };
    json!({
        "file": d.file, "kind": a.kind, "form": form,
        "text": a.text, "raw_verify": a.verify, "id": a.id, "parent": parent,
        "site": a.site, "line": a.line, "covered_region": a.covered_region,
        "text_hash": a.text_hash, "body_hash": a.body_hash,
    })
}

#[test]
fn extraction_matches_fixed_rust_c_reference_for_both_line_endings() {
    for crlf in [false, true] {
        let tmp = workspace(crlf);
        let actual = walk_directory(tmp.path()).unwrap();
        let expected = expected(crlf);
        assert_eq!(actual.len(), expected.len(), "CRLF={crlf}");
        for (position, (annotation, mut row)) in actual.iter().zip(expected).enumerate() {
            let row = row.as_object_mut().unwrap();
            row.remove("index_id");
            row.remove("index_verify");
            assert_eq!(
                extraction(annotation),
                json!(row),
                "position={position}, CRLF={crlf}"
            );
        }
    }
}

#[test]
fn stamping_preserves_fixed_ids_hashes_and_links_in_a_mixed_workspace() {
    for crlf in [false, true] {
        let tmp = workspace(crlf);
        Command::cargo_bin("aristo")
            .unwrap()
            .current_dir(tmp.path())
            .args(["stamp", "--skip-canon"])
            .assert()
            .success();
        let index: IndexFile =
            toml::from_str(&fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap())
                .unwrap();
        let expected = expected(crlf);
        let mut actual_ids: Vec<_> = index.entries.keys().map(AnnotationId::as_str).collect();
        let mut expected_ids: Vec<_> = expected
            .iter()
            .map(|row| row["index_id"].as_str().unwrap())
            .collect();
        actual_ids.sort_unstable();
        expected_ids.sort_unstable();
        assert_eq!(
            actual_ids, expected_ids,
            "annotation IDs changed (collision or dropped entry), CRLF={crlf}"
        );
        for row in expected {
            let id = row["index_id"].as_str().unwrap();
            let entry =
                serde_json::to_value(&index.entries[&AnnotationId::parse(id).unwrap()]).unwrap();
            for field in [
                "file",
                "kind",
                "text",
                "parent",
                "covered_region",
                "text_hash",
                "body_hash",
            ] {
                assert_eq!(
                    entry[field], row[field],
                    "id={id}, field={field}, CRLF={crlf}"
                );
            }
            assert_eq!(
                entry["site"],
                json!(format!(
                    "{} (line {})",
                    row["site"].as_str().unwrap(),
                    row["line"]
                )),
                "id={id}"
            );
            assert_eq!(entry["verify"], row["index_verify"], "id={id}");
        }
    }
}
