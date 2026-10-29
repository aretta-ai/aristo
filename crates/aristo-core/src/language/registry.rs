use crate::walk::extract::{extract_from_source, ExtractError, ExtractedAnnotation};
use crate::walk::extract_c::extract_from_c_source;

pub(crate) struct SourceLanguage {
    id: &'static str,
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
    pub(crate) fn extract(&self, source: &str) -> Result<Vec<ExtractedAnnotation>, ExtractError> {
        (self.extractor)(source)
    }
}

static SOURCE_LANGUAGES: &[SourceLanguage] = &[
    SourceLanguage {
        id: "rust",
        extensions: &["rs"],
        extractor: extract_from_source,
    },
    SourceLanguage {
        id: "c",
        extensions: &["c", "h"],
        extractor: extract_from_c_source,
    },
];

// Match extensions exactly: `.RS` and `.C` remain unsupported, as before.
#[aristo::intent(
    "Both the annotation walk and the freshness preflight route their \
     extension check through this one function, so they can never disagree \
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
            (Some("rs"), Some("rust")),
            (Some("c"), Some("c")),
            (Some("h"), Some("c")),
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
                actual.map(|language| language.id),
                expected_id,
                "extension {extension:?} resolved to {actual:?}"
            );
        }
    }
}
