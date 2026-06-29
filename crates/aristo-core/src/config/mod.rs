//! `aristo.toml` document schema (per TOOLS.md §4 field map).
//!
//! Every section is optional and has a sensible default — a project with
//! an empty `aristo.toml` (just `[__meta__]`-less, since this format has
//! no meta header) gets the same behavior as one with no config at all.
//!
//! ```toml
//! [verify]
//! default_method = "full"
//!
//! [verify.cache]
//! strategy     = "local"
//! commit_specs = true
//!
//! [stamp]
//! hooks            = "pre-commit"
//! hash_crate_root  = false
//!
//! [telemetry]
//! enabled = false
//!
//! [lint]
//! pre_commit = "check"     # also accepts a bool: true → "check", false → "off"
//! strict     = false
//!
//! [lint.rules.empty_text]
//! severity  = "error"
//!
//! [corpus]
//! contribute = false
//!
//! [doc]
//! commit_artifacts = true
//! position         = "before"
//! ```

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::index::VerifyMethod;

/// Top-level `aristo.toml` document. Every field defaults; an empty
/// file produces a `ConfigFile` with each section at its default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    #[serde(default)]
    pub verify: VerifyConfig,
    #[serde(default)]
    pub stamp: StampConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub lint: LintConfig,
    #[serde(default)]
    pub corpus: CorpusConfig,
    #[serde(default)]
    pub doc: DocConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub canon: CanonConfig,
    #[serde(default)]
    pub nudges: NudgesConfig,
}

// ─── [verify] ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyConfig {
    /// Resolves `verify = true` on annotations to a concrete method.
    /// `None` means "use the per-tier default" (free → `"test"`,
    /// paid → `"full"` per G1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_method: Option<VerifyMethod>,
    #[serde(default)]
    pub cache: VerifyCacheConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyCacheConfig {
    /// Where mined-assertion specs are cached. `Local` keeps them in
    /// `.aristo/specs/` only; `AristoCloud` opts in to cross-machine
    /// caching via the Aristo server (free users must explicitly
    /// enable per G7).
    #[serde(default)]
    pub strategy: CacheStrategy,
    /// Whether to commit `.aristo/specs/` to git. Default `true`
    /// (matches the .gitignore precedent — fresh clones produce
    /// reproducible verification runs).
    #[serde(default = "default_true")]
    pub commit_specs: bool,
}

impl Default for VerifyCacheConfig {
    fn default() -> Self {
        Self {
            strategy: CacheStrategy::default(),
            commit_specs: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CacheStrategy {
    /// `.aristo/specs/` only — no server roundtrip.
    #[default]
    Local,
    /// Opt-in cross-machine cache via the Aristo server.
    AristoCloud,
}

// ─── [stamp] ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StampConfig {
    /// Which git hook to install. Default `PreCommit`.
    #[serde(default)]
    pub hooks: HooksMode,
    /// Whether to hash the entire crate token-stream for crate-root
    /// annotation staleness detection. Default `false` (expensive on
    /// large crates per B3).
    #[serde(default)]
    pub hash_crate_root: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HooksMode {
    /// Install a `.git/hooks/pre-commit` script that runs `aristo stamp`
    /// and (per `[lint] pre_commit`) `aristo lint`.
    #[default]
    PreCommit,
    /// Don't install any git hooks. CI is expected to gate via
    /// `aristo stamp --check` / `aristo lint --check`.
    None,
}

// ─── [telemetry] ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Opt-in toggle for free-tier usage telemetry. Default `false`.
    /// Per H8: never gated as required; fully off by default.
    #[serde(default)]
    pub enabled: bool,
}

// ─── [lint] ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LintConfig {
    /// Pre-commit-hook lint mode. Per J6: string enum
    /// (`"off"` / `"check"` / `"fix"`) with bool back-compat
    /// (`true` → `Check`, `false` → `Off`).
    #[serde(default)]
    pub pre_commit: LintPreCommit,
    /// When `true`, `aristo lint --check` exits non-zero on `warn`
    /// findings as well as `error`. Default `false`.
    #[serde(default)]
    pub strict: bool,
    /// Per-rule configuration overrides. Map key is the rule name
    /// (e.g., `"empty_text"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rules: BTreeMap<String, LintRuleConfig>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            pre_commit: LintPreCommit::Check,
            strict: false,
            rules: BTreeMap::new(),
        }
    }
}

/// `[lint] pre_commit` value. Wire form is a string (`"off"` / `"check"` /
/// `"fix"`); deserialization additionally accepts a bool for back-compat
/// per J6 — `true` → `Check`, `false` → `Off`.
///
/// Custom `Serialize` always emits the canonical string form so a
/// round-trip normalizes the bool form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LintPreCommit {
    /// Skip lint in the pre-commit hook entirely. CI still runs
    /// `aristo lint --check` per the starter workflow.
    Off,
    /// Run `aristo lint --check` in the hook — fail-fast, never
    /// silently modifies staged content. Standard devtool default.
    #[default]
    Check,
    /// Run `aristo lint --fix` and re-stage modified files.
    /// Opt-in for teams that want auto-fix-and-restage.
    Fix,
}

impl Serialize for LintPreCommit {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LintPreCommit {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Accepts string ("off" | "check" | "fix") OR bool (J6 back-compat).
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Str(String),
            Bool(bool),
        }
        match Wire::deserialize(d)? {
            Wire::Str(s) => match s.as_str() {
                "off" => Ok(Self::Off),
                "check" => Ok(Self::Check),
                "fix" => Ok(Self::Fix),
                other => Err(serde::de::Error::unknown_variant(
                    other,
                    &["off", "check", "fix"],
                )),
            },
            Wire::Bool(true) => Ok(Self::Check),
            Wire::Bool(false) => Ok(Self::Off),
        }
    }
}

impl JsonSchema for LintPreCommit {
    fn schema_name() -> String {
        "LintPreCommit".to_owned()
    }
    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::*;
        // oneOf: string enum OR bool (back-compat).
        Schema::Object(SchemaObject {
            subschemas: Some(Box::new(SubschemaValidation {
                one_of: Some(vec![
                    Schema::Object(SchemaObject {
                        instance_type: Some(InstanceType::String.into()),
                        enum_values: Some(vec![
                            serde_json::json!("off"),
                            serde_json::json!("check"),
                            serde_json::json!("fix"),
                        ]),
                        ..Default::default()
                    }),
                    Schema::Object(SchemaObject {
                        instance_type: Some(InstanceType::Boolean.into()),
                        ..Default::default()
                    }),
                ]),
                ..Default::default()
            })),
            metadata: Some(Box::new(Metadata {
                description: Some(
                    "`[lint] pre_commit` — string enum (\"off\" | \"check\" | \"fix\") \
                     or bool (true → \"check\", false → \"off\") for J6 back-compat."
                        .to_owned(),
                ),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

impl LintPreCommit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Check => "check",
            Self::Fix => "fix",
        }
    }
}

/// Per-rule lint configuration. All fields are optional; each individual
/// rule consumes the subset that applies to it (e.g., `pattern` +
/// `message` are only meaningful for the custom-regex rule).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LintRuleConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_fix: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

// ─── [corpus] ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorpusConfig {
    /// Opt-in for paid users to contribute abstracted annotation
    /// patterns to the server-side property-template library.
    /// Default `false`. Default-on for design partners (set in
    /// their contract, applied as user-visible `true`).
    #[serde(default)]
    pub contribute: bool,
}

// ─── [doc] ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocConfig {
    /// Whether `.aristo/doc/*` markdown + graph artifacts are
    /// committed. Default `true` so a fresh clone renders correct
    /// docs without re-running anything.
    #[serde(default = "default_true")]
    pub commit_artifacts: bool,
    /// Where the Aristo-injected `#[doc = ...]` block sits relative
    /// to the user's hand-written `///` comments. Default `Before`
    /// (verified-intent claims at the top of each item's rendered docs).
    #[serde(default)]
    pub position: DocPosition,
}

impl Default for DocConfig {
    fn default() -> Self {
        Self {
            commit_artifacts: true,
            position: DocPosition::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DocPosition {
    #[default]
    Before,
    After,
}

// ─── [index] ──────────────────────────────────────────────────────────────

/// Filters applied during the source walk. Always-skipped directory
/// names (`target/`, `.git/`, `.aristo/`, `node_modules/`) are
/// hardcoded in the walker; `exclude` adds project-specific globs on
/// top of that floor (e.g., `"**/tests/ui/**"` to skip trybuild
/// fixtures that contain intentional empty-text annotations).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexConfig {
    /// Glob patterns (matched against paths relative to the workspace
    /// root) that the walker skips. Standard `**` / `*` / `?` syntax
    /// per `globset`. Paths use forward slashes regardless of host OS.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

// ─── [canon] ──────────────────────────────────────────────────────────────

/// §13 canon-and-matching tunables (Pro/Enterprise tiers only — the
/// free tier ignores this section and surfaces an upgrade nudge).
///
/// `enabled` is the project-level opt-out: regulated buyers and
/// air-gapped CI set `enabled = false` to skip canon API calls
/// unconditionally. Default is `true`; tier-gating is server-side
/// (the API returns the upgrade nudge for free-tier tokens).
///
/// The two threshold knobs control which match candidates surface.
/// Server enforces a floor of `0.5` (HTTP 400 below that). Defaults
/// match `docs/mockups/13-canon-and-matching/README.md` §L3:
///   - `threshold_stamp = 0.85` — stamp surfaces only high-confidence
///     matches (the daily-loop default; minimizes noise).
///   - `threshold_critique = 0.65` — critique surfaces broader
///     candidates (the deliberate review pass; user is reviewing).
///
/// No `flavor` field: scope membership is server-resolved from
/// repo identity per canon-strategy.md §CS8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonConfig {
    /// Project-level opt-out. Default `true`. When `false`, canon
    /// API calls are skipped unconditionally; cached matches remain
    /// readable but no new matches are surfaced and no accept-path
    /// runs.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Confidence threshold for matches surfaced by `aristo stamp`.
    /// Honored above the server-enforced `0.5` floor. Default `0.85`.
    #[serde(default = "default_threshold_stamp")]
    pub threshold_stamp: f64,
    /// Confidence threshold for matches surfaced by `aristo critique`.
    /// Honored above the server-enforced `0.5` floor. Default `0.65`.
    #[serde(default = "default_threshold_critique")]
    pub threshold_critique: f64,
}

impl Default for CanonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_stamp: default_threshold_stamp(),
            threshold_critique: default_threshold_critique(),
        }
    }
}

impl Eq for CanonConfig {}

fn default_threshold_stamp() -> f64 {
    0.85
}

fn default_threshold_critique() -> f64 {
    0.65
}

// ─── [nudges] ─────────────────────────────────────────────────────────────

/// `[nudges]` — the proactive nudge/progress engine (Phase 18). A single
/// `aggressiveness` knob scales every nudge's fire threshold (and the
/// human-prompt cooldown); `off` silences the engine entirely — the global
/// opt-out, mirroring `[canon] enabled = false`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NudgesConfig {
    /// How eagerly the engine surfaces nudges. Higher lowers every
    /// signal's fire threshold and shortens the human cooldown; `off`
    /// disables all nudges. Default `medium`.
    #[serde(default)]
    pub aggressiveness: Aggressiveness,
}

/// Nudge aggressiveness ladder. Maps to a numeric factor `f` the scorer
/// multiplies into each signal's normalized pressure: a signal fires when
/// `pressure * f >= 1`, so higher `f` fires sooner. `Off` yields `f = 0`,
/// the structural global opt-out (nothing can fire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Aggressiveness {
    /// No nudges at all (global opt-out).
    Off,
    /// Quietest: only large backlogs / strong signals surface.
    Low,
    /// Balanced default.
    #[default]
    Medium,
    /// Eager: surfaces sooner and re-arms faster.
    High,
}

impl Aggressiveness {
    #[aristo::intent(
        "Off MUST map to factor zero — it is the global opt-out. The scorer \
         fires only when a signal's pressure scaled by its factor reaches the \
         firing threshold, so an exact zero is the only value that guarantees \
         nothing ever fires no matter how overdue a signal is. Assigning Off \
         any small but non-zero factor would let extreme pressure leak through \
         to a user who deliberately silenced nudges. The non-zero levels are \
         tunable defaults (D8); this table is the single place to retune \
         global nudge sensitivity.",
        verify = "neural",
        id = "aggressiveness_off_is_hard_silence"
    )]
    pub fn factor(self) -> f64 {
        match self {
            Aggressiveness::Off => 0.0,
            Aggressiveness::Low => 0.6,
            Aggressiveness::Medium => 1.0,
            Aggressiveness::High => 1.6,
        }
    }

    /// True when nudges are entirely disabled (`aggressiveness = "off"`).
    pub fn is_off(self) -> bool {
        matches!(self, Aggressiveness::Off)
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

/// Produce the canonical JSON Schema (draft-07 via schemars 0.8) for the
/// project-level `aristo.toml` config file.
pub fn config_file_schema_json() -> String {
    let schema = schemars::schema_for!(ConfigFile);
    serde_json::to_string_pretty(&schema)
        .expect("serializing a schemars-derived schema cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_toml_yields_all_defaults() {
        let config: ConfigFile = toml::from_str("").unwrap();
        assert_eq!(config, ConfigFile::default());
        assert_eq!(config.verify.cache.strategy, CacheStrategy::Local);
        assert!(config.verify.cache.commit_specs);
        assert_eq!(config.stamp.hooks, HooksMode::PreCommit);
        assert!(!config.stamp.hash_crate_root);
        assert!(!config.telemetry.enabled);
        assert_eq!(config.lint.pre_commit, LintPreCommit::Check);
        assert!(!config.lint.strict);
        assert!(config.lint.rules.is_empty());
        assert!(!config.corpus.contribute);
        assert!(config.doc.commit_artifacts);
        assert_eq!(config.doc.position, DocPosition::Before);
        assert!(config.canon.enabled);
        assert!((config.canon.threshold_stamp - 0.85).abs() < f64::EPSILON);
        assert!((config.canon.threshold_critique - 0.65).abs() < f64::EPSILON);
        assert_eq!(config.nudges.aggressiveness, Aggressiveness::Medium);
    }

    #[test]
    fn canon_section_round_trips() {
        let toml_text = "\
            [canon]\n\
            enabled = true\n\
            threshold_stamp = 0.9\n\
            threshold_critique = 0.7\n\
        ";
        let config: ConfigFile = toml::from_str(toml_text).unwrap();
        assert!(config.canon.enabled);
        assert!((config.canon.threshold_stamp - 0.9).abs() < f64::EPSILON);
        assert!((config.canon.threshold_critique - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn canon_enabled_false_is_the_opt_out_for_regulated_buyers() {
        // canon-strategy.md §CS5 + README L3: project-level opt-out
        // via `[canon] enabled = false`.
        let toml_text = "[canon]\nenabled = false\n";
        let config: ConfigFile = toml::from_str(toml_text).unwrap();
        assert!(!config.canon.enabled);
        // Thresholds still default when unspecified.
        assert!((config.canon.threshold_stamp - 0.85).abs() < f64::EPSILON);
        assert!((config.canon.threshold_critique - 0.65).abs() < f64::EPSILON);
    }

    #[test]
    fn canon_section_rejects_flavor_field() {
        // Per canon-strategy.md §CS8: NO user-side flavor declaration
        // anywhere. Scope membership is server-resolved from repo
        // identity. A `flavor` field in [canon] must be rejected by
        // serde's `deny_unknown_fields`.
        let toml_text = "[canon]\nflavor = \"turso\"\n";
        let result: Result<ConfigFile, _> = toml::from_str(toml_text);
        assert!(result.is_err(), "expected deny_unknown_fields rejection");
    }

    #[test]
    fn canon_partial_section_keeps_other_defaults() {
        // Only enabled set; thresholds keep their defaults.
        let toml_text = "[canon]\nenabled = false\n";
        let config: ConfigFile = toml::from_str(toml_text).unwrap();
        assert!(!config.canon.enabled);
        assert_eq!(
            config.canon.threshold_stamp,
            CanonConfig::default().threshold_stamp
        );
        assert_eq!(
            config.canon.threshold_critique,
            CanonConfig::default().threshold_critique
        );
    }

    #[test]
    fn lint_pre_commit_accepts_string_form() {
        for (s, expected) in [
            ("off", LintPreCommit::Off),
            ("check", LintPreCommit::Check),
            ("fix", LintPreCommit::Fix),
        ] {
            let toml_text = format!("[lint]\npre_commit = \"{s}\"\n");
            let config: ConfigFile = toml::from_str(&toml_text).unwrap();
            assert_eq!(config.lint.pre_commit, expected);
        }
    }

    #[test]
    fn lint_pre_commit_bool_back_compat() {
        // J6: true → Check, false → Off
        for (b, expected) in [(true, LintPreCommit::Check), (false, LintPreCommit::Off)] {
            let toml_text = format!("[lint]\npre_commit = {b}\n");
            let config: ConfigFile = toml::from_str(&toml_text).unwrap();
            assert_eq!(config.lint.pre_commit, expected);
        }
    }

    #[test]
    fn lint_pre_commit_unknown_string_rejected() {
        let toml_text = "[lint]\npre_commit = \"sometimes\"\n";
        let result: Result<ConfigFile, _> = toml::from_str(toml_text);
        assert!(result.is_err());
    }

    #[test]
    fn lint_pre_commit_serializes_as_string() {
        let mut config = ConfigFile::default();
        config.lint.pre_commit = LintPreCommit::Fix;
        let toml_text = toml::to_string(&config).unwrap();
        assert!(toml_text.contains("pre_commit = \"fix\""));
    }

    #[test]
    fn lint_pre_commit_bool_form_normalizes_on_round_trip() {
        // bool input → string output (canonical form)
        let config: ConfigFile = toml::from_str("[lint]\npre_commit = true\n").unwrap();
        let serialized = toml::to_string(&config).unwrap();
        let reparsed: ConfigFile = toml::from_str(&serialized).unwrap();
        assert_eq!(reparsed.lint.pre_commit, LintPreCommit::Check);
    }

    #[test]
    fn cache_strategy_uses_kebab_case() {
        let v = serde_json::to_value(CacheStrategy::AristoCloud).unwrap();
        assert_eq!(v, serde_json::json!("aristo-cloud"));
    }

    #[test]
    fn hooks_mode_uses_kebab_case() {
        let v = serde_json::to_value(HooksMode::PreCommit).unwrap();
        assert_eq!(v, serde_json::json!("pre-commit"));
    }

    #[test]
    fn doc_position_uses_lowercase() {
        for variant in [DocPosition::Before, DocPosition::After] {
            let v = serde_json::to_value(variant).unwrap();
            // "before" or "after"
            assert!(v.is_string());
            assert_eq!(
                v.as_str().unwrap(),
                match variant {
                    DocPosition::Before => "before",
                    DocPosition::After => "after",
                }
            );
        }
    }

    #[test]
    fn lint_rules_map_round_trips() {
        let toml_text = r#"
[lint.rules.empty_text]
severity = "error"

[lint.rules.long_text]
severity = "warn"
threshold = 200
"#;
        let config: ConfigFile = toml::from_str(toml_text).unwrap();
        assert_eq!(config.lint.rules.len(), 2);
        let empty_text = config.lint.rules.get("empty_text").unwrap();
        assert_eq!(empty_text.severity, Some(Severity::Error));
        let long_text = config.lint.rules.get("long_text").unwrap();
        assert_eq!(long_text.threshold, Some(200));
    }

    #[test]
    fn unknown_top_level_field_rejected() {
        let toml_text = "totally_unknown = 42\n";
        let result: Result<ConfigFile, _> = toml::from_str(toml_text);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_section_field_rejected() {
        let toml_text = "[verify]\nunknown_field = \"x\"\n";
        let result: Result<ConfigFile, _> = toml::from_str(toml_text);
        assert!(result.is_err());
    }

    #[test]
    fn full_config_round_trips() {
        let mut config = ConfigFile::default();
        config.verify.default_method = Some(VerifyMethod::Full);
        config.verify.cache.strategy = CacheStrategy::AristoCloud;
        config.verify.cache.commit_specs = false;
        config.stamp.hooks = HooksMode::None;
        config.stamp.hash_crate_root = true;
        config.telemetry.enabled = true;
        config.lint.pre_commit = LintPreCommit::Fix;
        config.lint.strict = true;
        config.lint.rules.insert(
            "empty_text".into(),
            LintRuleConfig {
                severity: Some(Severity::Error),
                ..Default::default()
            },
        );
        config.corpus.contribute = true;
        config.doc.commit_artifacts = false;
        config.doc.position = DocPosition::After;

        let toml_text = toml::to_string(&config).unwrap();
        let back: ConfigFile = toml::from_str(&toml_text).unwrap();
        assert_eq!(back, config);
    }
}
