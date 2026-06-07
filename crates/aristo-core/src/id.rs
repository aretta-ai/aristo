//! Annotation-id generation: the two paths `aristo stamp` uses when an
//! author leaves `id` unset.
//!
//! 1. [`snake_case_from_text`] — derive a readable id from the annotation
//!    text. Used as a hint surfaced by `aristo stamp`'s offer-rename flow
//!    (slice 17+); not auto-applied because text-derived ids may clash
//!    with existing entries or just be clumsy.
//!
//! 2. [`deterministic_id`] — the id `aristo stamp` actually assigns when
//!    `id` is unset. A content-addressed `aret_<8>` id derived from the
//!    annotation's *identity*: kind + normalized text + enclosing site
//!    (plus a source-order ordinal, appended only when those collide).
//!    Being a pure function of identity, it is STABLE across stamps —
//!    re-running `aristo stamp` re-mints the same id, so the index
//!    re-associates the entry's prior status and `.proof` instead of
//!    churning it as removed+new. An earlier version minted a fresh
//!    *random* id (`getrandom`) on every stamp, which silently orphaned
//!    proofs and reset verification status; this content-addressed scheme
//!    fixes that. Per design (B5b), opaque ids are tool-managed; they
//!    NEVER come from user input and the `aristo_check` cargo feature
//!    rejects them in source.

use crate::index::{AnnotationId, AnnotationKind};

/// Suggested length for the readable-from-text path: take roughly the
/// first 4 useful words and cap at this many characters total. Short
/// enough to skim; long enough to disambiguate.
const READABLE_TARGET_LEN: usize = 32;

/// Number of characters in an opaque id's body. 8 base32-style chars over a
/// 31-symbol alphabet ≈ 31^8 ≈ 8.5e11 distinct ids — well clear of any
/// reasonable per-project collision risk. With [`deterministic_id`] these
/// are derived from the annotation's identity (not random), and the ordinal
/// disambiguates the only structured collision (identical kind+text+site).
const OPAQUE_BODY_LEN: usize = 8;

/// Base32-style alphabet (Crockford-ish: lowercase letters + digits, no
/// vowel/digit confusion). Stays inside the `[a-z0-9]+` charset that
/// `AnnotationId`'s opaque-namespace parser accepts.
const OPAQUE_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

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

/// Tag bytes mixed into the hash so an intent and an assume with otherwise
/// identical (text, site) never collide on the same id.
fn kind_tag(kind: AnnotationKind) -> &'static [u8] {
    match kind {
        AnnotationKind::Intent => b"intent",
        AnnotationKind::Assume => b"assume",
    }
}

/// The collision bucket an idless annotation falls into for ordinal
/// disambiguation. Two annotations share a bucket — and therefore must get
/// distinct `ordinal`s to avoid minting the *same* [`deterministic_id`] — iff
/// this key matches. It is exactly the (kind, normalized-text, site) triple
/// the id hashes over before the ordinal, so the caller (`aristo stamp`) can
/// count occurrences without re-implementing the normalization rule.
pub fn id_bucket_key(
    kind: AnnotationKind,
    text: &str,
    site_label: &str,
) -> (AnnotationKind, String, String) {
    (
        kind,
        crate::hash::normalize_text(text),
        site_label.to_string(),
    )
}

#[aristo::intent(
    "A stamp-assigned id is a pure function of the annotation's identity — \
     its kind, its whitespace-normalized text, and its enclosing site label \
     (plus a source-order ordinal only when those three collide). The same \
     annotation therefore mints the same id on every `aristo stamp`, which \
     is what lets the index re-associate its prior status and proof instead \
     of treating it as removed-then-new. Editing the covered CODE does not \
     change the id — that is body-hash drift, tracked separately; only \
     rewording the claim or renaming/moving the enclosing item does. The id \
     stays inside the `aret_` namespace charset so it always parses.",
    verify = "test",
    id = "deterministic_id_is_pure_function_of_identity"
)]
pub fn deterministic_id(
    kind: AnnotationKind,
    text: &str,
    site_label: &str,
    ordinal: usize,
) -> AnnotationId {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(kind_tag(kind));
    hasher.update([0u8]);
    hasher.update(crate::hash::normalize_text(text).as_bytes());
    hasher.update([0u8]);
    hasher.update(site_label.as_bytes());
    if ordinal > 0 {
        // Appended only when >0 so the common unique case hashes purely as
        // kind+text+site — an ordinal added unconditionally would make every
        // id depend on duplicate-scan order even when there are no dups.
        hasher.update([0u8]);
        hasher.update((ordinal as u64).to_le_bytes());
    }
    let digest = hasher.finalize();

    let mut s = String::with_capacity("aret_".len() + OPAQUE_BODY_LEN);
    s.push_str("aret_");
    for &byte in digest.iter().take(OPAQUE_BODY_LEN) {
        let idx = (byte as usize) % OPAQUE_ALPHABET.len();
        s.push(OPAQUE_ALPHABET[idx] as char);
    }
    AnnotationId::parse(&s).expect("deterministic-id alphabet stays inside aret_ namespace charset")
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

    // ─── deterministic_id ─────────────────────────────────────────────────

    fn det(kind: AnnotationKind, text: &str, site: &str, ord: usize) -> String {
        deterministic_id(kind, text, site, ord).as_str().to_owned()
    }

    #[test]
    fn deterministic_id_has_aret_prefix_and_length() {
        let id = deterministic_id(AnnotationKind::Intent, "some claim", "fn foo", 0);
        assert!(id.as_str().starts_with("aret_"), "got: {id}");
        assert_eq!(id.as_str().len(), "aret_".len() + OPAQUE_BODY_LEN);
    }

    #[test]
    fn deterministic_id_uses_safe_alphabet_only() {
        let id = deterministic_id(AnnotationKind::Assume, "another claim", "struct Bar", 3);
        let body = id.as_str().strip_prefix("aret_").unwrap();
        for ch in body.chars() {
            assert!(
                OPAQUE_ALPHABET.contains(&(ch as u8)),
                "deterministic id contains char `{ch}` not in alphabet"
            );
        }
    }

    #[test]
    fn deterministic_id_output_always_parses() {
        // Every output must round-trip through AnnotationId — the whole point
        // is to feed the index without further validation.
        for (kind, text, site, ord) in [
            (
                AnnotationKind::Intent,
                "the function returns x",
                "fn f",
                0usize,
            ),
            (AnnotationKind::Assume, "caller holds the lock", "fn g", 5),
            (AnnotationKind::Intent, "短い説明", "fn h", 1),
            (AnnotationKind::Intent, "", "fn empty_text", 0),
        ] {
            let id = deterministic_id(kind, text, site, ord);
            AnnotationId::parse(id.as_str())
                .unwrap_or_else(|e| panic!("({kind:?},{text:?},{site:?},{ord}) → {id}: {e}"));
        }
    }

    #[test]
    fn deterministic_id_is_stable_for_same_inputs() {
        // THE core fix: identical inputs ⇒ byte-identical id, on every call.
        let a = det(
            AnnotationKind::Intent,
            "balance is preserved",
            "fn balance",
            0,
        );
        let b = det(
            AnnotationKind::Intent,
            "balance is preserved",
            "fn balance",
            0,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn deterministic_id_changes_with_text() {
        let a = det(AnnotationKind::Intent, "claim one", "fn f", 0);
        let b = det(AnnotationKind::Intent, "claim two", "fn f", 0);
        assert_ne!(a, b, "rewording the claim must change the id");
    }

    #[test]
    fn deterministic_id_changes_with_site() {
        // Renaming/moving the enclosing item re-anchors the id (ID-D4).
        let a = det(AnnotationKind::Intent, "same claim", "fn old_name", 0);
        let b = det(AnnotationKind::Intent, "same claim", "fn new_name", 0);
        assert_ne!(a, b, "moving to a new site must change the id");
    }

    #[test]
    fn deterministic_id_changes_with_kind() {
        // An intent and an assume with identical text+site must not collide.
        let i = det(AnnotationKind::Intent, "same words", "fn f", 0);
        let a = det(AnnotationKind::Assume, "same words", "fn f", 0);
        assert_ne!(i, a, "kind is part of the identity");
    }

    #[test]
    fn deterministic_id_ignores_text_whitespace() {
        // Whitespace-only text differences normalize away (same rule as
        // text_hash), so they mint the same id — reformatting prose is not
        // an identity change.
        let a = det(AnnotationKind::Intent, "hello   world", "fn f", 0);
        let b = det(AnnotationKind::Intent, "  hello world  ", "fn f", 0);
        let c = det(AnnotationKind::Intent, "hello\n\tworld", "fn f", 0);
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn deterministic_id_distinct_ordinals_differ() {
        // Two idless annotations sharing kind+text+site get disambiguated by
        // source-order ordinal (ID-D2) so they don't collide in the index.
        let zero = det(AnnotationKind::Intent, "dup", "fn f", 0);
        let one = det(AnnotationKind::Intent, "dup", "fn f", 1);
        let two = det(AnnotationKind::Intent, "dup", "fn f", 2);
        assert_ne!(zero, one);
        assert_ne!(one, two);
        assert_ne!(zero, two);
    }

    #[test]
    fn id_bucket_key_collapses_whitespace_and_includes_kind() {
        // The bucket key must collapse whitespace exactly like the id hash so
        // the ordinal counter groups whitespace-variant texts together.
        let k1 = id_bucket_key(AnnotationKind::Intent, "a  b", "fn f");
        let k2 = id_bucket_key(AnnotationKind::Intent, "a b", "fn f");
        assert_eq!(k1, k2);
        let k3 = id_bucket_key(AnnotationKind::Assume, "a b", "fn f");
        assert_ne!(k1, k3, "kind is part of the bucket");
    }
}
