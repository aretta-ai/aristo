//! `.aristo/specs/<id>.spec` document schema (per G5 + B3).
//!
//! On-disk format is a hybrid: a TOML header (per [`SpecHeader`])
//! followed by a `---` separator line and an opaque text body that
//! holds the mined assertion source. The body is treated as raw text
//! by this crate — its semantics (Rust assertion code that the
//! `aristo_verify` cargo feature injects) live downstream.
//!
//! ```text
//! annotation_id        = "insert_into_cell_postcondition"
//! annotation_text_hash = "sha256:..."
//! source_body_hash     = "sha256:..."
//! covered_region       = "function"
//! covered_region_path  = "src/btree.rs::insert_into_cell"
//! mined_at             = "2026-05-13T14:23:00Z"
//! mined_by             = "aristo verify (skill=...)"
//! human_reviewed       = false
//! notes                = ""
//! ---
//! { /* raw mined assertion body */ }
//! ```

mod header;

pub use header::{SpecHeader, StalenessFieldsError, StalenessState};

use std::fmt;

/// Separator line between the TOML header and the opaque body.
const SEPARATOR_LINE: &str = "---";

/// One `.aristo/specs/<id>.spec` file. Splits cleanly into a typed
/// [`SpecHeader`] and a raw body string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFile {
    pub header: SpecHeader,
    /// Opaque body text — exactly what appears after the `---` line.
    pub body: String,
}

impl SpecFile {
    /// Parse a spec file from its on-disk text. Errors classify whether
    /// the failure was structural (no separator), the header didn't parse
    /// as TOML, or the staleness pair was inconsistent.
    pub fn parse(raw: &str) -> Result<Self, SpecParseError> {
        let (header_text, body) = raw
            .split_once("\n---\n")
            .or_else(|| raw.split_once("\r\n---\r\n"))
            .ok_or(SpecParseError::MissingSeparator)?;

        let header: SpecHeader =
            toml::from_str(header_text).map_err(|e| SpecParseError::HeaderToml(e.to_string()))?;

        Ok(Self {
            header,
            body: body.to_string(),
        })
    }
}

impl fmt::Display for SpecFile {
    /// Render the file to its on-disk form. Round-trips with [`Self::parse`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut header_toml = toml::to_string(&self.header).map_err(|_| fmt::Error)?;
        if !header_toml.ends_with('\n') {
            header_toml.push('\n');
        }
        write!(f, "{header_toml}{SEPARATOR_LINE}\n{}", self.body)
    }
}

/// Errors from [`SpecFile::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpecParseError {
    #[error("missing `---` separator between header and body")]
    MissingSeparator,
    #[error("header parse failed: {0}")]
    HeaderToml(String),
}

/// Produce the canonical JSON Schema (draft-07 via schemars 0.8) for the
/// TOML header of a `.aristo/specs/<id>.spec` file. The body is opaque
/// text and is described separately in `schemas/README.md`.
pub fn spec_header_schema_json() -> String {
    let schema = schemars::schema_for!(SpecHeader);
    serde_json::to_string_pretty(&schema)
        .expect("serializing a schemars-derived schema cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{AnnotationId, CoveredRegion, Sha256};

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn sample_file() -> SpecFile {
        SpecFile {
            header: SpecHeader {
                annotation_id: AnnotationId::parse("foo_postcondition").unwrap(),
                annotation_text_hash: sha('a'),
                source_body_hash: sha('b'),
                covered_region: CoveredRegion::Function,
                covered_region_path: "src/lib.rs::foo".into(),
                mined_at: "2026-05-13T14:23:00Z".into(),
                mined_by: "aristo verify".into(),
                human_reviewed: false,
                notes: String::new(),
                staleness: StalenessState::Fresh,
            },
            body: "{ debug_assert!(true); }\n".into(),
        }
    }

    #[test]
    fn parse_separator_required() {
        assert_eq!(
            SpecFile::parse("just a body, no header"),
            Err(SpecParseError::MissingSeparator)
        );
    }

    #[test]
    fn round_trip_through_text() {
        let original = sample_file();
        let text = original.to_string();
        // Sanity check on the rendered shape
        assert!(text.contains("\n---\n"));
        assert!(text.contains("{ debug_assert!(true); }"));
        let parsed = SpecFile::parse(&text).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn body_can_contain_separator_lines_after_first() {
        // The split_once anchors on the FIRST `\n---\n`, so a body that
        // contains the literal separator further down is preserved.
        let mut original = sample_file();
        original.body = "first line\n---\nbody continues here\n".into();
        let text = original.to_string();
        let parsed = SpecFile::parse(&text).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn empty_body_parses() {
        let mut original = sample_file();
        original.body = String::new();
        let text = original.to_string();
        let parsed = SpecFile::parse(&text).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn header_parse_error_propagates_clearly() {
        let bad = "annotation_id = 42\n---\nbody";
        let err = SpecFile::parse(bad).unwrap_err();
        assert!(matches!(err, SpecParseError::HeaderToml(_)));
    }
}
