use aristo_core::{
    id::{deterministic_id, source_scoped_id},
    index::AnnotationKind,
};
use std::path::Path;

#[test]
fn source_identity_is_scoped_normalized_and_separate_from_legacy_ids() {
    let id = |path, kind, text, site| source_scoped_id(Path::new(path), kind, text, site);
    let base = id("src/a.rs", AnnotationKind::Intent, "some claim", "fn work");
    assert_eq!(
        base,
        id(
            "./src/a.rs",
            AnnotationKind::Intent,
            " some  claim ",
            "fn work"
        )
    );
    for other in [
        id("src/b.rs", AnnotationKind::Intent, "some claim", "fn work"),
        id("src/a.rs", AnnotationKind::Assume, "some claim", "fn work"),
        id("src/a.rs", AnnotationKind::Intent, "other claim", "fn work"),
        id("src/a.rs", AnnotationKind::Intent, "some claim", "fn other"),
        deterministic_id(AnnotationKind::Intent, "some claim", "fn work", 0),
    ] {
        assert_ne!(base, other);
    }
    assert_eq!(base.as_str().len(), 39);
    assert!(base.as_str().starts_with("aret_v2"));
}

#[cfg(unix)]
#[test]
fn distinct_non_utf8_names_cannot_collapse_through_replacement_characters() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let id = |bytes| {
        source_scoped_id(
            Path::new(OsStr::from_bytes(bytes)),
            AnnotationKind::Intent,
            "claim",
            "fn x",
        )
    };
    assert_ne!(id(b"a\xff.rs"), id(b"a\xfe.rs"));
    assert_ne!(id(b"a\xff.rs"), id("a�.rs".as_bytes()));
}
