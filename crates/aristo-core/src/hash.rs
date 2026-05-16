//! Deterministic hashing helpers for annotation text and covered-region
//! bodies.
//!
//! Both helpers are thin wrappers over [`Sha256::from_bytes`]; they exist as
//! named functions so callers (and reviewers) see the intent — `text_hash`
//! vs `body_hash` carries domain meaning that a bare `from_bytes` call does
//! not. The two ALSO normalize their inputs differently, which is why they
//! aren't a single helper.
//!
//! Normalization rules:
//!
//! - **Text**: trimmed of leading/trailing whitespace, internal whitespace
//!   collapsed to a single space. Two annotations that differ only in
//!   indentation or line wrapping must produce the same `text_hash` so
//!   `aristo lint --fix` can reformat without invalidating verification.
//!
//! - **Body**: hashed verbatim — every byte of the source span counts. A
//!   semantically irrelevant change (extra whitespace, comment edit) DOES
//!   change `body_hash` and triggers `aristo stamp` to mark the entry as
//!   `unknown` (status drift). The author re-runs `aristo verify` to
//!   confirm the property still holds.

use crate::index::Sha256;

#[aristo::intent(
    "text_hash normalizes whitespace before hashing so that lint-induced \
     reformatting (re-wrapping a long string, fixing indentation) doesn't \
     invalidate stamped annotations. The mapping is: trim ends, then \
     collapse runs of ASCII whitespace into a single space.",
    verify = "test",
    id = "text_hash_normalizes_whitespace"
)]
pub fn text_hash(text: &str) -> Sha256 {
    let normalized = normalize_text(text);
    Sha256::from_bytes(normalized.as_bytes())
}

#[aristo::intent(
    "body_hash is verbatim — the covered region's bytes hash exactly as \
     they appear in source. This is what makes stamp's drift detection \
     work: any change inside the covered region changes the hash, flips \
     status to `unknown`, and the author re-verifies. Whitespace-only \
     edits ARE drift by design (the human reviewed THIS code, not other \
     code; even cosmetic changes deserve a fresh look).",
    verify = "test",
    id = "body_hash_is_verbatim"
)]
pub fn body_hash(body: &str) -> Sha256 {
    Sha256::from_bytes(body.as_bytes())
}

fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_ws = false;
    for ch in text.trim().chars() {
        if ch.is_ascii_whitespace() {
            if !last_was_ws {
                out.push(' ');
                last_was_ws = true;
            }
        } else {
            out.push(ch);
            last_was_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── text_hash ────────────────────────────────────────────────────────

    #[test]
    fn text_hash_is_deterministic() {
        let a = text_hash("hello world");
        let b = text_hash("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn text_hash_distinguishes_different_inputs() {
        assert_ne!(text_hash("hello"), text_hash("world"));
    }

    #[test]
    fn text_hash_returns_canonical_sha256_form() {
        let h = text_hash("anything").to_string();
        assert!(h.starts_with("sha256:"), "got: {h}");
        assert_eq!(h.len(), "sha256:".len() + 64);
        // Round-trips through parse: from_bytes always produces canonical form.
        Sha256::parse(&h).expect("from_bytes output must round-trip through parse");
    }

    #[test]
    fn text_hash_ignores_leading_and_trailing_whitespace() {
        let a = text_hash("hello world");
        let b = text_hash("   hello world   ");
        let c = text_hash("\nhello world\n");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn text_hash_collapses_internal_whitespace_runs() {
        let a = text_hash("hello world");
        let b = text_hash("hello   world"); // multiple spaces
        let c = text_hash("hello\tworld"); // tab
        let d = text_hash("hello\n  world"); // newline + indent
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a, d, "lint-induced re-wrapping must not change text_hash");
    }

    #[test]
    fn text_hash_distinguishes_word_order() {
        // Sanity check that normalization isn't TOO aggressive.
        assert_ne!(text_hash("hello world"), text_hash("world hello"));
    }

    // ─── body_hash ────────────────────────────────────────────────────────

    #[test]
    fn body_hash_is_deterministic() {
        let body = "fn x() -> i32 { 42 }";
        assert_eq!(body_hash(body), body_hash(body));
    }

    #[test]
    fn body_hash_is_verbatim_no_normalization() {
        // Whitespace IS significant — that's the design choice (B5b drift).
        assert_ne!(
            body_hash("fn x() -> i32 { 42 }"),
            body_hash("fn x() -> i32 {  42  }"),
            "extra spaces inside body must change body_hash"
        );
        assert_ne!(
            body_hash("fn x() {}"),
            body_hash(" fn x() {} "),
            "leading/trailing whitespace must change body_hash"
        );
    }

    #[test]
    fn body_hash_and_text_hash_differ_when_input_has_collapsible_whitespace() {
        // Ensures the two helpers' normalization rules actually differ:
        // text_hash collapses runs of whitespace; body_hash doesn't. An
        // input with multiple internal spaces exposes that difference.
        // (If the input is already canonical — single spaces, no edge
        // whitespace — the two hashes coincide, which is fine.)
        let s = "fn x()  ->  i32 { 42 }"; // doubled spaces
        assert_ne!(
            text_hash(s),
            body_hash(s),
            "doubled internal whitespace must hash differently under text vs body"
        );
    }
}
