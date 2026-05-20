//! Newtype validators for the string-shaped fields in `.aristo/index.toml`.
//!
//! Each newtype enforces structural shape only — prefix, length, character
//! set. Cryptographic well-formedness (e.g., "is this hex actually a real
//! SHA-256 of something" or "does this Ed25519 byte sequence verify") is
//! deferred to the actual signature check, which would reject any malformed
//! value anyway. This is the user-stated bar in the design discussion.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Errors from newtype `parse` functions. Distinct from [`super::ValidationError`]
/// (which covers cross-field rules); these are single-field shape failures
/// surfaced as `serde::de::Error::custom` during deserialization.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum ParseError {
    #[error("expected prefix `{prefix}`, got `{got}`")]
    MissingPrefix { prefix: &'static str, got: String },
    #[error("expected length {expected}, got {got}")]
    WrongLength { expected: usize, got: usize },
    #[error("length {got} not in allowed range {min}..={max}")]
    LengthOutOfRange { got: usize, min: usize, max: usize },
    #[error("invalid character set ({reason})")]
    InvalidCharset { reason: &'static str },
    #[error("empty value not allowed")]
    Empty,
    #[error("reserved namespace `{prefix}` is not user-writable")]
    ReservedNamespace { prefix: &'static str },
}

// ─── newtype boilerplate macro ──────────────────────────────────────────────

/// DRY: every string newtype gets the same Serialize/Deserialize/Display/
/// FromStr/AsRef/JsonSchema impls. Per-type logic stays in a free `parse`
/// function passed in by the caller.
macro_rules! string_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis $name:ident,
        parse_fn = $parse_fn:path,
        schema_pattern = $pattern:expr,
        schema_description = $desc:expr,
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        $vis struct $name(String);

        impl $name {
            /// Parse and validate. Errors describe the structural failure;
            /// they do not assert any cryptographic property.
            pub fn parse(s: &str) -> Result<Self, ParseError> {
                $parse_fn(s).map(Self)
            }

            /// Borrow as `&str`. Cheap; no allocation.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = ParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(s)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::parse(&s).map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> String {
                stringify!($name).to_owned()
            }

            fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
                schemars::schema::Schema::Object(schemars::schema::SchemaObject {
                    instance_type: Some(schemars::schema::InstanceType::String.into()),
                    string: Some(Box::new(schemars::schema::StringValidation {
                        pattern: Some($pattern.to_owned()),
                        ..Default::default()
                    })),
                    metadata: Some(Box::new(schemars::schema::Metadata {
                        description: Some($desc.to_owned()),
                        ..Default::default()
                    })),
                    ..Default::default()
                })
            }
        }
    };
}

// ─── Sha256 ─────────────────────────────────────────────────────────────────

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LEN: usize = 64;

fn parse_sha256(s: &str) -> Result<String, ParseError> {
    let hex = s
        .strip_prefix(SHA256_PREFIX)
        .ok_or_else(|| ParseError::MissingPrefix {
            prefix: SHA256_PREFIX,
            got: s.chars().take(SHA256_PREFIX.len()).collect(),
        })?;
    if hex.len() != SHA256_HEX_LEN {
        return Err(ParseError::WrongLength {
            expected: SHA256_HEX_LEN,
            got: hex.len(),
        });
    }
    if !hex.bytes().all(is_lowercase_hex) {
        return Err(ParseError::InvalidCharset {
            reason: "expected lowercase hex digits 0-9, a-f",
        });
    }
    Ok(s.to_owned())
}

string_newtype! {
    /// SHA-256 hash in canonical form `sha256:<64 lowercase hex chars>`.
    /// Validates only length and character set — no check that the digest
    /// matches any particular bytes.
    pub Sha256,
    parse_fn = parse_sha256,
    schema_pattern = r"^sha256:[0-9a-f]{64}$",
    schema_description = "SHA-256 hash, lowercase hex, prefixed `sha256:`.",
}

#[aristo::intent(
    "A hash computed by this constructor is always in canonical form — \
     the same form `parse` accepts and the same form written to the \
     index file. Hashes never need re-validation after computation.",
    verify = "test",
    id = "sha256_from_bytes_is_canonical_form"
)]
impl Sha256 {
    /// Compute the SHA-256 of `input` and return it in canonical form.
    /// Always succeeds; the output round-trips through [`Sha256::parse`].
    pub fn from_bytes(input: &[u8]) -> Self {
        use sha2::{Digest, Sha256 as Sha256Hasher};
        let digest = Sha256Hasher::digest(input);
        let mut s = String::with_capacity(SHA256_PREFIX.len() + SHA256_HEX_LEN);
        s.push_str(SHA256_PREFIX);
        for byte in digest {
            // Lowercase hex per the canonical schema. format! / write! both
            // allocate; manual encoding stays in the pre-allocated capacity.
            const HEX: &[u8; 16] = b"0123456789abcdef";
            s.push(HEX[(byte >> 4) as usize] as char);
            s.push(HEX[(byte & 0x0f) as usize] as char);
        }
        Self(s)
    }
}

// ─── CommitHash ─────────────────────────────────────────────────────────────

const GIT_SHA1_LEN: usize = 40;
const GIT_SHA256_LEN: usize = 64;

fn parse_commit_hash(s: &str) -> Result<String, ParseError> {
    if s.len() != GIT_SHA1_LEN && s.len() != GIT_SHA256_LEN {
        return Err(ParseError::LengthOutOfRange {
            got: s.len(),
            min: GIT_SHA1_LEN,
            max: GIT_SHA256_LEN,
        });
    }
    if !s.bytes().all(is_lowercase_hex) {
        return Err(ParseError::InvalidCharset {
            reason: "expected lowercase hex digits 0-9, a-f",
        });
    }
    Ok(s.to_owned())
}

string_newtype! {
    /// Git commit hash. Accepts SHA-1 (40 hex chars) or SHA-256 (64 hex
    /// chars) so the type doesn't break when git's transition lands.
    pub CommitHash,
    parse_fn = parse_commit_hash,
    schema_pattern = r"^([0-9a-f]{40}|[0-9a-f]{64})$",
    schema_description = "Git commit hash: 40 lowercase hex chars (SHA-1) or 64 (SHA-256).",
}

// ─── ArtaId ─────────────────────────────────────────────────────────────────

const ARTA_PREFIX: &str = "arta_";
/// Reasonable bounds: B5b says "128-bit-random-base32" which is ~26 chars,
/// but encoding choice is not nailed down so accept a wide range.
const ARTA_BODY_MIN: usize = 8;
const ARTA_BODY_MAX: usize = 64;

fn parse_arta_id(s: &str) -> Result<String, ParseError> {
    let body = s
        .strip_prefix(ARTA_PREFIX)
        .ok_or_else(|| ParseError::MissingPrefix {
            prefix: ARTA_PREFIX,
            got: s.chars().take(ARTA_PREFIX.len()).collect(),
        })?;
    if body.len() < ARTA_BODY_MIN || body.len() > ARTA_BODY_MAX {
        return Err(ParseError::LengthOutOfRange {
            got: body.len(),
            min: ARTA_BODY_MIN,
            max: ARTA_BODY_MAX,
        });
    }
    if !body.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(ParseError::InvalidCharset {
            reason: "expected ASCII alphanumeric (A-Z, a-z, 0-9)",
        });
    }
    Ok(s.to_owned())
}

string_newtype! {
    /// Server-side opaque identity assigned at first bind (per B5b).
    /// Format: `arta_<8-64 alphanumeric chars>`. The server's choice of
    /// encoding (base32, base62, etc.) is opaque to the SDK; we accept any
    /// alphanumeric body in the documented length window.
    pub ArtaId,
    parse_fn = parse_arta_id,
    schema_pattern = r"^arta_[A-Za-z0-9]{8,64}$",
    schema_description = "Opaque server identity, prefixed `arta_`, 8-64 alphanumeric chars.",
}

// ─── VerifiedOutcome ────────────────────────────────────────────────────────

const OUTCOME_SCHEME_PREFIX: &str = "v1:";
/// Ed25519 signature is 64 bytes; base64 of 64 bytes is 88 chars (with
/// padding) or 86 (without). Accept a generous window for future schemes
/// that may include extra metadata.
const OUTCOME_BODY_MIN: usize = 64;
const OUTCOME_BODY_MAX: usize = 256;

fn parse_verified_outcome(s: &str) -> Result<String, ParseError> {
    let body = s
        .strip_prefix(OUTCOME_SCHEME_PREFIX)
        .ok_or_else(|| ParseError::MissingPrefix {
            prefix: OUTCOME_SCHEME_PREFIX,
            got: s.chars().take(OUTCOME_SCHEME_PREFIX.len()).collect(),
        })?;
    if body.len() < OUTCOME_BODY_MIN || body.len() > OUTCOME_BODY_MAX {
        return Err(ParseError::LengthOutOfRange {
            got: body.len(),
            min: OUTCOME_BODY_MIN,
            max: OUTCOME_BODY_MAX,
        });
    }
    if !body.bytes().all(is_base64_url_safe) {
        return Err(ParseError::InvalidCharset {
            reason: "expected base64-URL-safe (A-Z, a-z, 0-9, +, /, =, -, _)",
        });
    }
    Ok(s.to_owned())
}

string_newtype! {
    /// Ed25519 verification certificate (per B5b), serialized as
    /// `v<scheme>:<base64-url-safe>`. Currently only `v1:` is defined.
    /// Validates only the prefix, length window, and character set; the
    /// actual signature check happens against the bundled public key
    /// registry during verification.
    pub VerifiedOutcome,
    parse_fn = parse_verified_outcome,
    schema_pattern = r"^v1:[A-Za-z0-9+/=_-]{64,256}$",
    schema_description = "Ed25519 verification certificate, base64-URL-safe, version-prefixed `v1:`.",
}

// ─── AnnotationId ───────────────────────────────────────────────────────────

const ARISTOS_PREFIX: &str = "aristos:";
const KANON_PREFIX: &str = "kanon:";
const ARET_PREFIX: &str = "aret_";
const READABLE_MIN: usize = 1;
const READABLE_MAX: usize = 128;
const OPAQUE_BODY_MIN: usize = 4;
const OPAQUE_BODY_MAX: usize = 64;

/// Which namespace an [`AnnotationId`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdNamespace {
    /// Local readable id; user-written or stamp-promoted (e.g., `balance_no_dups`).
    Local,
    /// Stamp-assigned opaque id (e.g., `aret_a1b2c3d4`).
    Opaque,
    /// Server-bound, backed by a verification mechanism (e.g.,
    /// `aristos:balance_no_dups`). Per canon-strategy.md §CS13 this is
    /// the "backed" tier — the canon entry has a populated `backed_by`
    /// field for the user's scope.
    Aristos,
    /// Server-bound, no verification backing yet (e.g.,
    /// `kanon:checkout_total_non_negative`). Per canon-strategy.md
    /// §CS13 this is the "bound but unbacked" tier — canonical text
    /// plus `linked`, but no `backed_by` and no scheduled
    /// verification. Users signal demand for a backing via
    /// `aristo canon request-verify`.
    Kanon,
}

/// Allowable charset for the body of a readable id (after any namespace
/// prefix): lowercase ASCII letters, digits, underscores. First char must
/// be a letter or underscore.
fn validate_readable_body(body: &str) -> Result<(), ParseError> {
    if body.is_empty() {
        return Err(ParseError::Empty);
    }
    if body.len() > READABLE_MAX {
        return Err(ParseError::LengthOutOfRange {
            got: body.len(),
            min: READABLE_MIN,
            max: READABLE_MAX,
        });
    }
    let mut bytes = body.bytes();
    let first = bytes.next().expect("body length checked above");
    if !(first.is_ascii_lowercase() || first == b'_') {
        return Err(ParseError::InvalidCharset {
            reason: "readable id must start with lowercase letter or underscore",
        });
    }
    if !bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(ParseError::InvalidCharset {
            reason: "readable id must use only lowercase letters, digits, and underscores",
        });
    }
    Ok(())
}

fn parse_annotation_id(s: &str) -> Result<String, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    // Server-bound, backed: `aristos:` + readable body
    if let Some(body) = s.strip_prefix(ARISTOS_PREFIX) {
        validate_readable_body(body)?;
        return Ok(s.to_owned());
    }
    // Server-bound, unbacked: `kanon:` + readable body
    if let Some(body) = s.strip_prefix(KANON_PREFIX) {
        validate_readable_body(body)?;
        return Ok(s.to_owned());
    }
    // Stamp-assigned opaque id
    if let Some(body) = s.strip_prefix(ARET_PREFIX) {
        if body.len() < OPAQUE_BODY_MIN || body.len() > OPAQUE_BODY_MAX {
            return Err(ParseError::LengthOutOfRange {
                got: body.len(),
                min: OPAQUE_BODY_MIN,
                max: OPAQUE_BODY_MAX,
            });
        }
        if !body.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(ParseError::InvalidCharset {
                reason: "opaque id must be ASCII alphanumeric",
            });
        }
        return Ok(s.to_owned());
    }
    // Local readable id (no namespace prefix). Reject ids that look like a
    // mistyped reserved prefix to give a clearer error than "invalid charset".
    if let Some(stripped) = s.strip_prefix(':') {
        let _ = stripped;
        return Err(ParseError::InvalidCharset {
            reason: "ids must not start with `:` (forgot a namespace prefix?)",
        });
    }
    validate_readable_body(s)?;
    Ok(s.to_owned())
}

string_newtype! {
    /// Annotation identity. Four accepted forms:
    ///
    /// - Local readable: `snake_case` (e.g., `balance_no_dups`).
    /// - Stamp-assigned opaque: `aret_<alphanumeric>` (e.g., `aret_a1b2c3d4`).
    /// - Server-bound, backed: `aristos:<readable>` (e.g.,
    ///   `aristos:balance_no_dups`) — canon entry has a populated
    ///   `backed_by` for the user's scope per canon-strategy.md §CS13.
    /// - Server-bound, unbacked: `kanon:<readable>` (e.g.,
    ///   `kanon:checkout_total_non_negative`) — canon entry has no
    ///   `backed_by` yet; user signals demand via `aristo canon
    ///   request-verify`.
    ///
    /// All three reserved namespaces (`aristos:`, `kanon:`, `aret_*`)
    /// are not user-writable — `aristo stamp` rejects user-written
    /// ids in those namespaces. Canon prefixes are applied
    /// exclusively by the canon accept path
    /// (`aristo critique --apply-findings`); opaque ids are
    /// stamp-assigned only.
    pub AnnotationId,
    parse_fn = parse_annotation_id,
    schema_pattern =
        r"^(aristos:|kanon:)?[a-z_][a-z0-9_]{0,127}$|^aret_[A-Za-z0-9]{4,64}$",
    schema_description =
        "Annotation id: local snake_case, stamp-assigned `aret_<alphanumeric>`, \
         server-bound backed `aristos:<snake_case>`, or server-bound unbacked \
         `kanon:<snake_case>`.",
}

impl AnnotationId {
    /// Which namespace this id belongs to.
    pub fn namespace(&self) -> IdNamespace {
        if self.0.starts_with(ARISTOS_PREFIX) {
            IdNamespace::Aristos
        } else if self.0.starts_with(KANON_PREFIX) {
            IdNamespace::Kanon
        } else if self.0.starts_with(ARET_PREFIX) {
            IdNamespace::Opaque
        } else {
            IdNamespace::Local
        }
    }

    /// True iff this id is bound to a canon entry (either tier).
    /// Convenience for callers that need to gate on "canon-bound"
    /// regardless of whether a backing exists.
    pub fn is_canon_bound(&self) -> bool {
        matches!(self.namespace(), IdNamespace::Aristos | IdNamespace::Kanon)
    }
}

// ─── shared char predicates ─────────────────────────────────────────────────

fn is_lowercase_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

fn is_base64_url_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Sha256 ─────────────────────────────────────────────────────────

    #[test]
    fn sha256_accepts_canonical_form() {
        let s = format!("sha256:{}", "0".repeat(64));
        assert_eq!(Sha256::parse(&s).unwrap().as_str(), s);
    }

    #[test]
    fn sha256_rejects_missing_prefix() {
        let s = "0".repeat(64);
        assert!(matches!(
            Sha256::parse(&s),
            Err(ParseError::MissingPrefix {
                prefix: "sha256:",
                ..
            })
        ));
    }

    #[test]
    fn sha256_rejects_wrong_length() {
        assert!(matches!(
            Sha256::parse("sha256:abc"),
            Err(ParseError::WrongLength {
                expected: 64,
                got: 3
            })
        ));
    }

    #[test]
    fn sha256_rejects_uppercase_hex() {
        let s = format!("sha256:{}", "A".repeat(64));
        assert!(matches!(
            Sha256::parse(&s),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    #[test]
    fn sha256_rejects_non_hex_chars() {
        let s = format!("sha256:{}", "g".repeat(64));
        assert!(matches!(
            Sha256::parse(&s),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    // ─── CommitHash ─────────────────────────────────────────────────────

    #[test]
    fn commit_hash_accepts_sha1() {
        let s = "a".repeat(40);
        assert_eq!(CommitHash::parse(&s).unwrap().as_str(), s);
    }

    #[test]
    fn commit_hash_accepts_sha256() {
        let s = "a".repeat(64);
        assert_eq!(CommitHash::parse(&s).unwrap().as_str(), s);
    }

    #[test]
    fn commit_hash_rejects_other_lengths() {
        assert!(matches!(
            CommitHash::parse(&"a".repeat(50)),
            Err(ParseError::LengthOutOfRange { .. })
        ));
    }

    #[test]
    fn commit_hash_rejects_uppercase() {
        assert!(matches!(
            CommitHash::parse(&"A".repeat(40)),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    // ─── ArtaId ─────────────────────────────────────────────────────────

    #[test]
    fn arta_id_accepts_mixed_case_alphanumeric_body() {
        ArtaId::parse("arta_op4q3z9NbV").unwrap();
    }

    #[test]
    fn arta_id_rejects_missing_prefix() {
        assert!(matches!(
            ArtaId::parse("op4q3z9NbV"),
            Err(ParseError::MissingPrefix {
                prefix: "arta_",
                ..
            })
        ));
    }

    #[test]
    fn arta_id_rejects_too_short_body() {
        assert!(matches!(
            ArtaId::parse("arta_abc"),
            Err(ParseError::LengthOutOfRange { .. })
        ));
    }

    #[test]
    fn arta_id_rejects_punctuation_in_body() {
        assert!(matches!(
            ArtaId::parse("arta_op4q3-9NbV"),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    // ─── VerifiedOutcome ────────────────────────────────────────────────

    #[test]
    fn verified_outcome_accepts_base64_body_in_range() {
        let s = format!("v1:{}", "A".repeat(86));
        VerifiedOutcome::parse(&s).unwrap();
    }

    #[test]
    fn verified_outcome_rejects_missing_prefix() {
        let s = "A".repeat(86);
        assert!(matches!(
            VerifiedOutcome::parse(&s),
            Err(ParseError::MissingPrefix { prefix: "v1:", .. })
        ));
    }

    #[test]
    fn verified_outcome_rejects_too_short_body() {
        assert!(matches!(
            VerifiedOutcome::parse("v1:short"),
            Err(ParseError::LengthOutOfRange { .. })
        ));
    }

    #[test]
    fn verified_outcome_rejects_non_base64_chars() {
        // Single illegal char (`!`) embedded in an otherwise length-valid
        // body, so the length check passes and the charset check fires.
        let mut body = "A".repeat(85);
        body.push('!');
        let s = format!("v1:{body}");
        assert!(matches!(
            VerifiedOutcome::parse(&s),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    // ─── AnnotationId ───────────────────────────────────────────────────

    #[test]
    fn annotation_id_accepts_local_snake_case() {
        let id = AnnotationId::parse("balance_no_duplicate_cells").unwrap();
        assert_eq!(id.namespace(), IdNamespace::Local);
    }

    #[test]
    fn annotation_id_accepts_aristos_namespace() {
        let id = AnnotationId::parse("aristos:balance_no_duplicate_cells").unwrap();
        assert_eq!(id.namespace(), IdNamespace::Aristos);
        assert!(id.is_canon_bound());
    }

    #[test]
    fn annotation_id_accepts_kanon_namespace() {
        let id = AnnotationId::parse("kanon:checkout_total_non_negative").unwrap();
        assert_eq!(id.namespace(), IdNamespace::Kanon);
        assert!(id.is_canon_bound());
    }

    #[test]
    fn annotation_id_rejects_kanon_uppercase_body() {
        // kanon: bodies follow the same readable-body grammar as
        // local + aristos: ids — lowercase only.
        assert!(matches!(
            AnnotationId::parse("kanon:Foo"),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    #[test]
    fn annotation_id_rejects_kanon_empty_body() {
        // Bare `kanon:` with no body must error like bare `aristos:`.
        assert!(matches!(
            AnnotationId::parse("kanon:"),
            Err(ParseError::Empty)
        ));
    }

    #[test]
    fn annotation_id_local_is_not_canon_bound() {
        let id = AnnotationId::parse("balance_no_duplicate_cells").unwrap();
        assert!(!id.is_canon_bound());
        let id = AnnotationId::parse("aret_a1b2c3d4").unwrap();
        assert!(!id.is_canon_bound());
    }

    #[test]
    fn annotation_id_accepts_aret_opaque() {
        let id = AnnotationId::parse("aret_a1b2c3d4").unwrap();
        assert_eq!(id.namespace(), IdNamespace::Opaque);
    }

    #[test]
    fn annotation_id_rejects_empty() {
        assert_eq!(AnnotationId::parse(""), Err(ParseError::Empty));
    }

    #[test]
    fn annotation_id_rejects_starting_digit() {
        assert!(matches!(
            AnnotationId::parse("1foo"),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    #[test]
    fn annotation_id_rejects_uppercase_in_local() {
        assert!(matches!(
            AnnotationId::parse("FooBar"),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    #[test]
    fn annotation_id_rejects_aret_with_too_short_body() {
        assert!(matches!(
            AnnotationId::parse("aret_xy"),
            Err(ParseError::LengthOutOfRange { .. })
        ));
    }

    #[test]
    fn annotation_id_rejects_leading_colon() {
        assert!(matches!(
            AnnotationId::parse(":foo"),
            Err(ParseError::InvalidCharset { .. })
        ));
    }

    // ─── round-trip / display / fromstr ─────────────────────────────────

    #[test]
    fn display_matches_parsed_input() {
        let s = format!("sha256:{}", "1".repeat(64));
        let h = Sha256::parse(&s).unwrap();
        assert_eq!(h.to_string(), s);
    }

    #[test]
    fn from_str_works() {
        let s = format!("sha256:{}", "1".repeat(64));
        let _: Sha256 = s.parse().unwrap();
    }

    #[test]
    fn serde_round_trips_via_json_string() {
        let s = format!("sha256:{}", "1".repeat(64));
        let h = Sha256::parse(&s).unwrap();
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json, format!("\"{}\"", s));
        let back: Sha256 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn serde_deserialize_propagates_parse_error() {
        let bad = "\"sha256:short\"";
        let result: Result<Sha256, _> = serde_json::from_str(bad);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("expected length 64"));
    }
}
