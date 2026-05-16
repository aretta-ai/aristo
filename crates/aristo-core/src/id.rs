//! Annotation-id generation: the two paths `aristo stamp` uses when an
//! author leaves `id` unset.
//!
//! 1. [`snake_case_from_text`] — derive a readable id from the annotation
//!    text. Used as a hint surfaced by `aristo stamp`'s offer-rename flow
//!    (slice 17+); not auto-applied because text-derived ids may clash
//!    with existing entries or just be clumsy.
//!
//! 2. [`generate_opaque_id`] — fallback when no readable suggestion is
//!    accepted (or when text is too sparse to derive one). Produces an
//!    `aret_<8 base32-alphanumeric>` opaque id with 40 bits of entropy.
//!    Per design (B5b), opaque ids are tool-managed; they NEVER come from
//!    user input and the `aristo_check` cargo feature rejects them in
//!    source.

use crate::index::AnnotationId;

/// Suggested length for the readable-from-text path: take roughly the
/// first 4 useful words and cap at this many characters total. Short
/// enough to skim; long enough to disambiguate.
const READABLE_TARGET_LEN: usize = 32;

/// Number of random characters in an opaque id's body. 8 base32-style
/// chars = 40 bits of entropy ≈ 1.1e12 distinct ids — well clear of any
/// reasonable per-project collision risk.
const OPAQUE_BODY_LEN: usize = 8;

/// Base32-style alphabet (Crockford-ish: lowercase letters + digits, no
/// vowel/digit confusion). Stays inside the `[a-z0-9]+` charset that
/// `AnnotationId`'s opaque-namespace parser accepts.
const OPAQUE_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

#[aristo::intent(
    "snake_case_from_text returns None if no usable readable id can be \
     derived (text is empty, all non-ASCII, or contains no word \
     characters). Callers MUST handle None by falling back to \
     generate_opaque_id — that's the contract that lets stamp always \
     produce a valid id even from pathological annotations.",
    verify = "test",
    id = "snake_case_from_text_returns_none_for_unusable_input"
)]
pub fn snake_case_from_text(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_was_underscore = true; // suppress leading _

    for ch in text.chars() {
        if out.len() >= READABLE_TARGET_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_underscore = false;
        } else if !last_was_underscore {
            out.push('_');
            last_was_underscore = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        return None;
    }
    if !out.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
        // First char is a digit — prefix with `n_` (per the stamp design's
        // "leading-digit normalization" rule). Without this, the result
        // wouldn't pass AnnotationId's readable-id charset check.
        out.insert_str(0, "n_");
    }
    Some(out)
}

#[aristo::intent(
    "generate_opaque_id always returns a parseable AnnotationId with the \
     `aret_` prefix. The OS RNG (getrandom) is the source of entropy; if \
     it fails (extremely rare — usually a misconfigured kernel), this \
     function panics rather than returning a Result. The reasoning: a \
     stamped id with weak entropy is worse than a crashed `aristo stamp` \
     run that the user can retry.",
    verify = "test",
    id = "generate_opaque_id_always_parses"
)]
pub fn generate_opaque_id() -> AnnotationId {
    let mut bytes = [0u8; OPAQUE_BODY_LEN];
    getrandom::getrandom(&mut bytes)
        .expect("OS RNG failed; aborting rather than emitting a low-entropy id");

    let mut s = String::with_capacity("aret_".len() + OPAQUE_BODY_LEN);
    s.push_str("aret_");
    for byte in bytes {
        let idx = (byte as usize) % OPAQUE_ALPHABET.len();
        s.push(OPAQUE_ALPHABET[idx] as char);
    }
    AnnotationId::parse(&s).expect("opaque-id alphabet stays inside aret_ namespace charset")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── snake_case_from_text ─────────────────────────────────────────────

    #[test]
    fn extracts_readable_id_from_typical_intent_text() {
        let id = snake_case_from_text("the function returns the input plus one")
            .expect("readable id should derive from normal English");
        assert!(id.starts_with("the_function_returns_the"), "got: {id}");
        assert!(id.len() <= READABLE_TARGET_LEN);
    }

    #[test]
    fn collapses_runs_of_non_alphanumeric_into_single_underscore() {
        let id = snake_case_from_text("hello, world!! how are you?").unwrap();
        assert_eq!(id, "hello_world_how_are_you");
    }

    #[test]
    fn lowercases_uppercase_input() {
        let id = snake_case_from_text("Balance NoDuplicateCells").unwrap();
        assert_eq!(id, "balance_noduplicatecells");
    }

    #[test]
    fn caps_at_target_length() {
        let long = "a".repeat(200);
        let id = snake_case_from_text(&long).unwrap();
        assert!(id.len() <= READABLE_TARGET_LEN, "got len {}", id.len());
    }

    #[test]
    fn returns_none_for_empty_input() {
        assert_eq!(snake_case_from_text(""), None);
    }

    #[test]
    fn returns_none_for_punctuation_only() {
        assert_eq!(snake_case_from_text("!!! ??? ..."), None);
    }

    #[test]
    fn returns_none_for_non_ascii_only() {
        // "你好世界" — no ASCII alphanumerics → no derivable readable id.
        assert_eq!(snake_case_from_text("你好世界"), None);
    }

    #[test]
    fn prepends_n_underscore_when_text_starts_with_digit() {
        // AnnotationId's readable-id parser rejects leading digits, so
        // snake_case_from_text normalizes by prefixing.
        let id = snake_case_from_text("42 is the answer").unwrap();
        assert!(id.starts_with("n_"), "got: {id}");
        // Result must round-trip through AnnotationId — the whole point of
        // this helper is to feed the index without further validation.
        AnnotationId::parse(&id).expect("output should be a valid local AnnotationId");
    }

    #[test]
    fn output_is_a_valid_annotation_id() {
        // Property test (manual): every non-None output parses as AnnotationId.
        for input in [
            "the function returns x",
            "Balance no duplicate cells",
            "BTreeMap maintains sort order",
            "  whitespace trimmed  ",
            "weird,,,,,punctuation:::",
            "42 starts with digit",
        ] {
            let id = snake_case_from_text(input).unwrap();
            AnnotationId::parse(&id)
                .unwrap_or_else(|e| panic!("input {input:?} → {id:?} failed parse: {e}"));
        }
    }

    // ─── generate_opaque_id ───────────────────────────────────────────────

    #[test]
    fn opaque_id_has_aret_prefix() {
        let id = generate_opaque_id();
        assert!(id.as_str().starts_with("aret_"), "got: {id}");
    }

    #[test]
    fn opaque_id_has_expected_length() {
        let id = generate_opaque_id();
        assert_eq!(id.as_str().len(), "aret_".len() + OPAQUE_BODY_LEN);
    }

    #[test]
    fn opaque_id_uses_safe_alphabet_only() {
        let id = generate_opaque_id();
        let body = id.as_str().strip_prefix("aret_").unwrap();
        for ch in body.chars() {
            assert!(
                OPAQUE_ALPHABET.contains(&(ch as u8)),
                "opaque id contains char `{ch}` not in alphabet"
            );
        }
    }

    #[test]
    fn opaque_ids_are_distinct_with_high_probability() {
        // 40 bits of entropy → collision rate of ~1e-12 across 1000 draws.
        // Test asserts no collisions in 1000 (vacuously passes if RNG works).
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = generate_opaque_id();
            assert!(
                seen.insert(id.as_str().to_owned()),
                "duplicate opaque id within 1000 draws — RNG is broken"
            );
        }
    }
}
