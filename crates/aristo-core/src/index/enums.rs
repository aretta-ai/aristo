//! Enum types for `.aristo/index.toml`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Annotation kind: `intent` (verifiable claim about code) or `assume`
/// (external invariant the code relies on, taken as given).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind {
    Intent,
    Assume,
}

/// Current verification state. The wire form uses kebab-case to keep
/// `pending-deepen` readable on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Verified at the strongest method available; signature is current.
    Verified,
    /// Verified via assertion-style testing (mined assertions, free or paid).
    Tested,
    /// Verified via neural reasoning (LLM-based).
    Neural,
    /// Was verified at a prior code state; current `body_hash` differs from
    /// the signed outcome. Sig still valid; only the code drifted.
    Stale,
    /// `verified_outcome.commit_hash` is not in this repository's history.
    /// Most likely cause: index entry copied from another repo (B5b).
    Orphan,
    /// `verified_outcome` does not verify against any bundled public key —
    /// either tampered or signed by a revoked/unknown key (B5b).
    Forged,
    /// Has not yet been verified, or was reverted to unknown after a rename
    /// or rebind operation invalidated the prior signature.
    Unknown,
    /// Sig is valid but commit ancestry could not be confirmed because the
    /// outcome's commit falls outside a shallow checkout's window (B5b).
    PendingDeepen,
    /// Verification produced a counterexample — the property does NOT
    /// hold under the cited code/text version. Terminal state: stays
    /// Counterexample until either `body_hash` drifts (→ Stale) or the
    /// intent text is rewritten to exclude the refuted case. There is
    /// NO accept-counterexample flow; counterexamples are loud and
    /// surface on every `aristo stamp` until the user fixes them.
    Counterexample,
    /// Verification was inconclusive — the verifier could not discharge
    /// the proof's gap and queued one or more suggested annotations the
    /// user could add to close it. Distinct from Unknown (which means
    /// "never attempted") so the skip logic can tell "we tried and got
    /// stuck" from "we haven't tried yet." Stays Inconclusive until
    /// either the source drifts (→ Stale via the body-drift rule), the
    /// user adopts a queued suggestion (→ re-verify), or the user
    /// explicitly re-runs with `--rerun`.
    Inconclusive,
}

/// `verify` field value. Mixed-type per design (G1):
///
/// - `false` (TOML bool): documentation only; no check ever runs.
/// - `true` (TOML bool): resolves to project default in `aristo.toml [verify] default_method`.
/// - one of `"neural"`, `"test"`, `"full"` (TOML string): named method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VerifyLevel {
    Bool(bool),
    Method(VerifyMethod),
}

/// Named verification methods (the string form of [`VerifyLevel`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VerifyMethod {
    Neural,
    Test,
    Full,
}

/// What syntactic region an annotation covers. Determines the body region
/// hashed for staleness detection (per B3) and the scope a `"test"`-mode
/// assertion is injected over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoveredRegion {
    /// Whole function body (most common; α-form attribute on a fn).
    Function,
    /// A single statement (β-form macro before a stmt or loop).
    Statement,
    /// Every method in an `impl` block.
    ImplMethods,
    /// The inline body of a module item.
    ModuleInlineBody,
    /// A struct or enum item.
    Type,
    /// A struct field or enum variant.
    Field,
    /// The crate root.
    Crate,
    /// A trait declaration.
    Trait,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_kind_round_trips_lowercase() {
        let v = serde_json::to_value(AnnotationKind::Intent).unwrap();
        assert_eq!(v, serde_json::json!("intent"));
        let back: AnnotationKind = serde_json::from_value(v).unwrap();
        assert_eq!(back, AnnotationKind::Intent);
    }

    #[test]
    fn status_pending_deepen_is_kebab_case() {
        let v = serde_json::to_value(Status::PendingDeepen).unwrap();
        assert_eq!(v, serde_json::json!("pending-deepen"));
        let back: Status = serde_json::from_value(serde_json::json!("pending-deepen")).unwrap();
        assert_eq!(back, Status::PendingDeepen);
    }

    #[test]
    fn verify_level_accepts_bool_form() {
        let level: VerifyLevel = serde_json::from_value(serde_json::json!(false)).unwrap();
        assert_eq!(level, VerifyLevel::Bool(false));
        let level: VerifyLevel = serde_json::from_value(serde_json::json!(true)).unwrap();
        assert_eq!(level, VerifyLevel::Bool(true));
    }

    #[test]
    fn verify_level_accepts_named_method_form() {
        let level: VerifyLevel = serde_json::from_value(serde_json::json!("full")).unwrap();
        assert_eq!(level, VerifyLevel::Method(VerifyMethod::Full));
    }

    #[test]
    fn verify_level_rejects_unknown_string() {
        let result: Result<VerifyLevel, _> = serde_json::from_value(serde_json::json!("partial"));
        assert!(result.is_err());
    }

    #[test]
    fn covered_region_uses_snake_case() {
        let v = serde_json::to_value(CoveredRegion::ImplMethods).unwrap();
        assert_eq!(v, serde_json::json!("impl_methods"));
        let v = serde_json::to_value(CoveredRegion::ModuleInlineBody).unwrap();
        assert_eq!(v, serde_json::json!("module_inline_body"));
    }

    #[test]
    fn unknown_status_value_is_rejected() {
        let result: Result<Status, _> = serde_json::from_value(serde_json::json!("partial"));
        assert!(result.is_err());
    }
}
