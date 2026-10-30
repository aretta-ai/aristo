//! Covered-region hashes must use the original buffer, including newline bytes.
use aristo_core::{hash::body_hash, walk::extract_from_source};

#[test]
fn rust_regions_preserve_original_bytes() {
    for newline in ["\n", "\r\n"] {
        for final_newline in ["", newline] {
            let body = format!("{{{newline}    let café = \"🦀\";{newline}    café{newline}}}");
            let source = format!("// 🦀\n#[aristo::intent(\"returns café\")]\nfn café() -> &'static str {body}{final_newline}");
            let entries = extract_from_source(&source).unwrap();
            assert_eq!(
                entries[0].body_hash,
                body_hash(&body),
                "newline={newline:?}"
            );
        }
    }
}

#[test]
fn unicode_columns_and_mixed_newlines_preserve_exact_region() {
    let body = "{\r\n    \"café\"\n}";
    let source = format!("#[aristo::intent(\"café\")] fn café() -> &'static str {body}");
    assert_eq!(
        extract_from_source(&source).unwrap()[0].body_hash,
        body_hash(body)
    );
}
