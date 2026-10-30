use aristo_core::{hash::body_hash, walk::extract_from_source};

#[test]
fn markers_cover_the_following_target_in_open_visit_contexts() {
    for (prefix, suffix) in [
        ("", ""),
        ("let f = || {", "};"),
        ("match 1 { _ => {", "} }"),
        ("unsafe {", "}"),
    ] {
        let source = format!("fn work() {{ {prefix} aristo::intent_stmt!(\"first\"); aristo::assume_stmt!(\"second\"); let value = 1; {suffix} }}");
        let annotations = extract_from_source(&source).unwrap();
        assert_eq!(annotations.len(), 2, "{source}");
        for annotation in annotations {
            assert_eq!(annotation.body_hash, body_hash("let value = 1;"));
            assert_eq!(annotation.site, "fn work");
        }
    }
}

#[test]
fn statement_hash_tracks_only_the_attached_statement() {
    let source = "fn work() { aristo::intent_stmt!(\"claim\"); { let x = 1; } let y = 2; }";
    let hash = |s: &str| extract_from_source(s).unwrap()[0].body_hash.clone();
    assert_eq!(hash(source), body_hash("{ let x = 1; }"));
    assert_eq!(hash(source), hash(&source.replace("y = 2", "y = 3")));
    assert_ne!(hash(source), hash(&source.replace("x = 1", "x = 3")));
}

#[test]
fn terminal_markers_are_rejected_instead_of_hashing_themselves() {
    let error = extract_from_source("fn work() { aristo::intent_stmt!(\"claim\"); }").unwrap_err();
    assert!(
        error.to_string().contains("no following statement"),
        "{error}"
    );
}
