//! Fixed Rust/C reference outputs for the source-provider refactor.

use aristo_core::walk::{walk_directory, DiscoveredAnnotation, ParentRaw};
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
    json!({
        "file": d.file, "kind": a.kind, "form": format!("{:?}", a.form),
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
