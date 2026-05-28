//! Single-annotation validators for the `aristo_check` cargo feature.
//!
//! Pure functions over already-parsed `IntentArgs` / `AssumeArgs`. Each
//! returns `Result<(), syn::Error>` where the error carries a span pointing
//! at the offending token, so the resulting `compile_error!()` lands on the
//! right squiggle in the user's editor.
//!
//! When the `aristo_check` feature is OFF (`default-features = false` on the
//! user side), `validate_intent` / `validate_assume` are still defined but
//! return `Ok(())` unconditionally — the proc-macros call them either way
//! to keep the call site uniform.
//!
//! Validation rules (per `docs/TOOLS.md` §6 `aristo_check` row + the slice 8
//! ROADMAP entry):
//!
//! - text: must be non-empty (after `trim`)
//! - verify (if present): bool literal, OR string literal in {"false",
//!   "test", "neural", "full"}
//! - id (if present): must NOT start with `aret_` or `aristos:` — those
//!   namespaces are tool-managed (stamp/sync). Must be snake_case
//!   (lowercase ASCII, digits, underscores; first char a letter).
//!
//! Project-wide rules (cycles, parent-id existence, duplicate ids,
//! signature validation) live in `aristo-cli`, not here — see TOOLS.md
//! `aristo_check` row "Reasons" column.

use crate::{AssumeArgs, IntentArgs};

/// Validate intent args. With `aristo_check` ON, runs the rule set above.
/// With it OFF, returns `Ok(())`.
pub(crate) fn validate_intent(args: &IntentArgs) -> Result<(), syn::Error> {
    #[cfg(feature = "aristo_check")]
    {
        let mut combined: Option<syn::Error> = None;
        if let Some(text) = &args.text {
            push(&mut combined, validate_text(text));
        }
        if let Some(verify) = &args.verify {
            push(&mut combined, validate_verify(verify));
        }
        if let Some(id) = &args.id {
            push(&mut combined, validate_id(id));
        }
        match combined {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    #[cfg(not(feature = "aristo_check"))]
    {
        let _ = args;
        Ok(())
    }
}

/// Validate assume args. With `aristo_check` ON, runs the rule set above
/// minus the `verify` rule (assume forbids `verify` entirely — the parser
/// catches that). With it OFF, returns `Ok(())`.
pub(crate) fn validate_assume(args: &AssumeArgs) -> Result<(), syn::Error> {
    #[cfg(feature = "aristo_check")]
    {
        let mut combined: Option<syn::Error> = None;
        if let Some(text) = &args.text {
            push(&mut combined, validate_text(text));
        }
        if let Some(id) = &args.id {
            push(&mut combined, validate_id(id));
        }
        match combined {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    #[cfg(not(feature = "aristo_check"))]
    {
        let _ = args;
        Ok(())
    }
}

#[cfg(feature = "aristo_check")]
fn push(combined: &mut Option<syn::Error>, result: Result<(), syn::Error>) {
    if let Err(e) = result {
        match combined {
            Some(existing) => existing.combine(e),
            None => *combined = Some(e),
        }
    }
}

#[cfg(feature = "aristo_check")]
fn validate_text(text: &syn::LitStr) -> Result<(), syn::Error> {
    if text.value().trim().is_empty() {
        Err(syn::Error::new(
            text.span(),
            "annotation text must not be empty",
        ))
    } else {
        Ok(())
    }
}

#[cfg(feature = "aristo_check")]
fn validate_verify(expr: &syn::Expr) -> Result<(), syn::Error> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Bool(_),
            ..
        }) => Ok(()),
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => match s.value().as_str() {
            "false" | "test" | "neural" | "full" => Ok(()),
            other => Err(syn::Error::new(
                s.span(),
                format!(
                    "invalid `verify` value `\"{other}\"`; expected one of: \
                     \"false\", \"test\", \"neural\", \"full\", or a bool literal"
                ),
            )),
        },
        _ => Err(syn::Error::new_spanned(
            expr,
            "`verify` must be a bool literal (true/false) or one of: \
             \"false\", \"test\", \"neural\", \"full\"",
        )),
    }
}

#[cfg(feature = "aristo_check")]
fn validate_id(id: &syn::LitStr) -> Result<(), syn::Error> {
    let s = id.value();
    // Namespace / provenance is intentionally NOT enforced here. `aristos:`
    // (sync-bound) and `aret_` (stamp-assigned) are provenance claims a
    // proc-macro cannot verify: it sees only this annotation's tokens, with no
    // binding/sync/stamp context, so it cannot distinguish a legitimate
    // tool-written id from a hand-typed one. Enforcing it in the macro can only
    // produce false rejects (a real bound id fails to compile) or false passes.
    // That policy belongs to `aristo lint`, which can read the repo + binding
    // state. The macro only checks that a *bare* (un-namespaced) id is a valid
    // snake_case identifier; recognized-namespace ids pass through untouched.
    if s.starts_with("aristos:") || s.starts_with("aret_") {
        return Ok(());
    }
    if !is_valid_user_id(&s) {
        return Err(syn::Error::new(
            id.span(),
            format!(
                "id `{s}` is not a valid snake_case identifier \
                 (lowercase letters, digits, underscores; must start with a letter)"
            ),
        ));
    }
    Ok(())
}

#[cfg(feature = "aristo_check")]
fn is_valid_user_id(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// Unit tests live in the proc-macro crate's `cargo test` build, where
// `cfg(feature = "aristo_check")` follows the workspace default (on).
// Tests parse their inputs through the public `IntentArgs` / `AssumeArgs`
// types so the test exercises the same path the macros use.
#[cfg(all(test, feature = "aristo_check"))]
mod tests {
    use super::*;

    fn intent(s: &str) -> IntentArgs {
        syn::parse_str(s).expect("test input parses as IntentArgs")
    }

    fn assume(s: &str) -> AssumeArgs {
        syn::parse_str(s).expect("test input parses as AssumeArgs")
    }

    // ---- text ----

    #[test]
    fn empty_text_rejected() {
        let err = validate_intent(&intent("\"\"")).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn whitespace_only_text_rejected() {
        let err = validate_intent(&intent("\"   \\t\\n  \"")).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn nonempty_text_accepted() {
        validate_intent(&intent("\"a real intent\"")).unwrap();
    }

    // ---- verify ----

    #[test]
    fn verify_bool_literals_accepted() {
        validate_intent(&intent("\"text\", verify = true")).unwrap();
        validate_intent(&intent("\"text\", verify = false")).unwrap();
    }

    #[test]
    fn verify_canonical_string_values_accepted() {
        for v in ["false", "test", "neural", "full"] {
            validate_intent(&intent(&format!("\"text\", verify = \"{v}\"")))
                .unwrap_or_else(|e| panic!("verify=\"{v}\" should be accepted: {e}"));
        }
    }

    #[test]
    fn unknown_verify_string_rejected() {
        let err = validate_intent(&intent("\"text\", verify = \"yolo\"")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid `verify` value"), "msg: {msg}");
        assert!(msg.contains("yolo"), "msg should quote bad value: {msg}");
    }

    #[test]
    fn nonliteral_verify_rejected() {
        let err = validate_intent(&intent("\"text\", verify = SOME_CONST")).unwrap_err();
        assert!(err.to_string().contains("must be a bool literal"));
    }

    // ---- id ----

    #[test]
    fn aret_prefix_id_accepted() {
        // Provenance policy moved to `aristo lint`; the macro has no stamp
        // context and must not reject tool-namespaced ids.
        validate_intent(&intent("\"text\", id = \"aret_foo\"")).unwrap();
    }

    #[test]
    fn aristos_namespace_id_accepted() {
        // Canon-bound ids must compile; boundness is checked by `aristo lint`,
        // not the macro (which has no binding context).
        validate_intent(&intent("\"text\", id = \"aristos:wal_initialized\"")).unwrap();
    }

    #[test]
    fn uppercase_id_rejected() {
        let err = validate_intent(&intent("\"text\", id = \"FooBar\"")).unwrap_err();
        assert!(err.to_string().contains("snake_case"));
    }

    #[test]
    fn id_starting_with_digit_rejected() {
        let err = validate_intent(&intent("\"text\", id = \"1_foo\"")).unwrap_err();
        assert!(err.to_string().contains("snake_case"));
    }

    #[test]
    fn id_with_dash_rejected() {
        let err = validate_intent(&intent("\"text\", id = \"foo-bar\"")).unwrap_err();
        assert!(err.to_string().contains("snake_case"));
    }

    #[test]
    fn snake_case_id_accepted() {
        validate_intent(&intent("\"text\", id = \"foo_bar_42\"")).unwrap();
    }

    // ---- combine ----

    #[test]
    fn multiple_errors_combine() {
        // Empty text + bad verify + bad id → all three surface (syn::Error
        // chains via .combine; iterating yields each individually).
        let err = validate_intent(&intent("\"\", verify = \"yolo\", id = \"BadId\"")).unwrap_err();
        let msgs: Vec<String> = err.into_iter().map(|e| e.to_string()).collect();
        let joined = msgs.join("\n");
        assert!(joined.contains("must not be empty"), "msgs:\n{joined}");
        assert!(joined.contains("invalid `verify`"), "msgs:\n{joined}");
        assert!(joined.contains("snake_case"), "msgs:\n{joined}");
        assert_eq!(msgs.len(), 3, "expected 3 errors, got {}", msgs.len());
    }

    // ---- assume parallels ----

    #[test]
    fn assume_validates_text_and_id() {
        validate_assume(&assume("\"text\", id = \"foo_bar\"")).unwrap();
        let err = validate_assume(&assume("\"\"")).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
        let err = validate_assume(&assume("\"text\", id = \"BadId\"")).unwrap_err();
        assert!(err.to_string().contains("snake_case"));
    }
}
