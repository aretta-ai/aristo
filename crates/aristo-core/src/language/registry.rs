use crate::walk::extract::{extract_from_source, ExtractError, ExtractedAnnotation};
use crate::walk::extract_c::extract_from_c_source;
use std::path::Path;

/// Opaque source language identity. Additional built-in IDs may be added.
/// Compare against the associated constants rather than string spellings.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LanguageId(&'static str);

impl LanguageId {
    /// Rust source language.
    pub const RUST: Self = Self("rust");
    /// C source language, including C headers.
    pub const C: Self = Self("c");

    /// Canonical spelling for display and diagnostics.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Read-only built-in language descriptor from [`language_for_path`] or [`built_in_languages`].
/// Extraction remains internal to the core crate.
pub struct SourceLanguage {
    id: LanguageId,
    extensions: &'static [&'static str],
    extractor: fn(&str) -> Result<Vec<ExtractedAnnotation>, ExtractError>,
}

impl std::fmt::Debug for SourceLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Report the language identity, not an unstable extractor address.
        f.debug_struct("SourceLanguage")
            .field("id", &self.id)
            .field("extensions", &self.extensions)
            .finish_non_exhaustive()
    }
}

impl SourceLanguage {
    /// Typed language identifier; compare with [`LanguageId`] constants.
    pub fn id(&self) -> LanguageId {
        self.id
    }

    pub(crate) fn extract(&self, source: &str) -> Result<Vec<ExtractedAnnotation>, ExtractError> {
        (self.extractor)(source)
    }
}

static SOURCE_LANGUAGES: &[SourceLanguage] = &[
    SourceLanguage {
        id: LanguageId::RUST,
        extensions: &["rs"],
        extractor: extract_from_source,
    },
    SourceLanguage {
        id: LanguageId::C,
        extensions: &["c", "h"],
        extractor: extract_from_c_source,
    },
];

/// Read-only inventory of built-in source languages, without a promised order.
pub fn built_in_languages() -> &'static [SourceLanguage] {
    SOURCE_LANGUAGES
}

/// Select the source language from a path without opening or parsing the file.
/// Missing, non-UTF-8 and unsupported extensions return `None`.
pub fn language_for_path(path: &Path) -> Option<&'static SourceLanguage> {
    language_for_extension(path.extension().and_then(|extension| extension.to_str()))
}

/// Select a built-in source language by its exact, case-sensitive extension.
///
/// Extensions do not include a leading dot. `rs` selects Rust; `c` and `h`
/// select C. Missing or unsupported extensions, including `RS` and `C`,
/// return `None`. This lookup performs no filesystem access or parsing.
#[aristo::intent(
    "The annotation walk, freshness preflight and aristo lang route their \
     source-extension selection through this function (via the path lookup), \
     so they agree \
     on which files count as source. Filtering `.c`/`.h` in here but not in \
     the freshness walk would silently stop drift detection for C files — \
     they would be indexed once but never re-checked for staleness. `.h` is \
     treated as C.",
    verify = "test",
    id = "source_language_extension_set_is_single_authority"
)]
pub(crate) fn language_for_extension(ext: Option<&str>) -> Option<&'static SourceLanguage> {
    let ext = ext?;
    SOURCE_LANGUAGES
        .iter()
        .find(|language| language.extensions.contains(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn path_lookup_preserves_exact_extension_selection() {
        for (path, expected) in [
            ("src/café.rs", Some(LanguageId::RUST)),
            ("src/api.c", Some(LanguageId::C)),
            ("include/api.h", Some(LanguageId::C)),
            ("src/lib.RS", None),
            ("src/api.C", None),
            ("src/api.cpp", None),
            ("src/app.py", None),
            ("src.rs/README", None),
            ("src/.rs", None),
        ] {
            assert_eq!(
                language_for_path(Path::new(path)).map(SourceLanguage::id),
                expected,
                "incorrect language for {path}"
            );
        }
    }

    #[test]
    fn every_extension_maps_to_exactly_one_language() {
        let mut seen = HashSet::new();
        for language in SOURCE_LANGUAGES {
            for extension in language.extensions {
                assert!(seen.insert(*extension), "duplicate extension `{extension}`");
            }
        }
    }

    #[test]
    fn extension_lookup_preserves_language_identity() {
        for (extension, expected_id) in [
            (Some("rs"), Some(LanguageId::RUST)),
            (Some("c"), Some(LanguageId::C)),
            (Some("h"), Some(LanguageId::C)),
            (None, None),
            (Some(""), None),
            (Some("RS"), None),
            (Some("C"), None),
            (Some("cpp"), None),
            (Some("hpp"), None),
            (Some("py"), None),
            (Some("ts"), None),
            (Some("go"), None),
        ] {
            let actual = language_for_extension(extension);
            assert_eq!(
                actual.map(SourceLanguage::id),
                expected_id,
                "extension {extension:?} resolved to {actual:?}"
            );
        }
    }
}
