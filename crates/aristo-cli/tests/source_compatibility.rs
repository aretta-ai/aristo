//! CLI stamping against the fixed compatibility corpus; see fixture Q1–Q3.

#[path = "../../aristo-core/tests/fixtures/source_compatibility/mod.rs"]
mod fixture;

use aristo_core::index::{AnnotationId, IndexFile};
use assert_cmd::Command;
use fixture::{expected, workspace};
use serde_json::json;
use std::fs;

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
