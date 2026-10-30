//! Core extraction against the fixed compatibility corpus; see fixture Q1–Q3.

#[path = "fixtures/source_compatibility/mod.rs"]
mod fixture;

use aristo_core::walk::{walk_directory, AnnotationForm, DiscoveredAnnotation, ParentRaw};
use fixture::{expected, workspace};
use serde_json::{json, Value};

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
