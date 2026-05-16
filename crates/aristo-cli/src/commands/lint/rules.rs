//! Lint rule catalog per mockup 07 D3.
//!
//! Each built-in rule has a name, a default severity, and a `check`
//! function over annotation text. Per-project overrides come from
//! `aristo.toml [lint.rules.<name>] severity = "..."`.

use std::collections::HashMap;

use aristo_core::config::{ConfigFile, Severity as CoreSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Info,
    Warn,
    Error,
}

impl From<CoreSeverity> for Severity {
    fn from(s: CoreSeverity) -> Self {
        match s {
            CoreSeverity::Info => Self::Info,
            CoreSeverity::Warn => Self::Warn,
            CoreSeverity::Error => Self::Error,
        }
    }
}

pub(crate) struct CheckOutcome {
    pub rule_name: &'static str,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Default)]
pub(crate) struct Overrides {
    by_rule: HashMap<String, Severity>,
}

impl Overrides {
    pub(crate) fn from_config(cfg: &ConfigFile) -> Self {
        let mut by_rule = HashMap::new();
        for (name, rule_cfg) in cfg.lint.rules.iter() {
            if let Some(sev) = rule_cfg.severity {
                by_rule.insert(name.clone(), sev.into());
            }
        }
        Self { by_rule }
    }

    fn for_rule(&self, name: &str, default: Severity) -> Severity {
        self.by_rule.get(name).copied().unwrap_or(default)
    }
}

/// Run every built-in `check`-mode rule against `text`. Auto-fixable
/// rules (`doubled_spaces`, `trailing_whitespace`) are run in --fix
/// (slice 20 / C2), not here.
pub(crate) fn run_check_rules(text: &str, overrides: &Overrides) -> Vec<CheckOutcome> {
    let mut out = Vec::new();

    if let Some(o) = check_empty_text(text, overrides) {
        out.push(o);
    }
    if let Some(o) = check_placeholder_text(text, overrides) {
        out.push(o);
    }
    if let Some(o) = check_text_too_long(text, overrides) {
        out.push(o);
    }
    out.extend(check_weasel_words(text, overrides));
    out
}

fn check_empty_text(text: &str, overrides: &Overrides) -> Option<CheckOutcome> {
    if text.trim().is_empty() {
        Some(CheckOutcome {
            rule_name: "empty_text",
            severity: overrides.for_rule("empty_text", Severity::Error),
            message: "annotation text is empty".to_string(),
        })
    } else {
        None
    }
}

fn check_placeholder_text(text: &str, overrides: &Overrides) -> Option<CheckOutcome> {
    for marker in ["TODO", "FIXME", "TBD", "XXX"] {
        if text.contains(marker) {
            return Some(CheckOutcome {
                rule_name: "placeholder_text",
                severity: overrides.for_rule("placeholder_text", Severity::Error),
                message: format!("annotation text contains placeholder `{marker}`"),
            });
        }
    }
    None
}

const TEXT_TOO_LONG_THRESHOLD: usize = 1000;

fn check_text_too_long(text: &str, overrides: &Overrides) -> Option<CheckOutcome> {
    if text.chars().count() > TEXT_TOO_LONG_THRESHOLD {
        Some(CheckOutcome {
            rule_name: "text_too_long",
            severity: overrides.for_rule("text_too_long", Severity::Warn),
            message: format!("annotation text exceeds {TEXT_TOO_LONG_THRESHOLD} characters"),
        })
    } else {
        None
    }
}

const WEASELS: &[&str] = &[
    "should",
    "will probably",
    "we think",
    "i believe",
    "obviously",
    "just",
    "simply",
];

fn check_weasel_words(text: &str, overrides: &Overrides) -> Vec<CheckOutcome> {
    let mut out = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut seen = Vec::new();
    for phrase in WEASELS {
        if word_phrase_present(&lower, phrase) && !seen.contains(phrase) {
            seen.push(phrase);
        }
    }
    if !seen.is_empty() {
        let list = seen
            .iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push(CheckOutcome {
            rule_name: "weasel_words",
            severity: overrides.for_rule("weasel_words", Severity::Warn),
            message: format!("annotation text contains weasel phrase(s) {list}"),
        });
    }
    out
}

/// Whole-phrase containment with word-boundary checks at both ends so
/// `should` doesn't match `shoulder` and `just` doesn't match `adjust`.
fn word_phrase_present(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(idx) = haystack[start..].find(needle) {
        let abs = start + idx;
        let before_ok = abs == 0 || !is_word_char(haystack.as_bytes()[abs - 1]);
        let after = abs + needle.len();
        let after_ok = after >= haystack.len() || !is_word_char(haystack.as_bytes()[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides_none() -> Overrides {
        Overrides::default()
    }

    #[test]
    fn empty_text_detects_truly_empty_and_whitespace_only() {
        assert!(check_empty_text("", &overrides_none()).is_some());
        assert!(check_empty_text("   ", &overrides_none()).is_some());
        assert!(check_empty_text("\n\t", &overrides_none()).is_some());
        assert!(check_empty_text("ok", &overrides_none()).is_none());
    }

    #[test]
    fn placeholder_text_catches_each_marker() {
        for m in ["TODO", "FIXME", "TBD", "XXX"] {
            assert!(
                check_placeholder_text(&format!("oh no {m}"), &overrides_none()).is_some(),
                "should match marker {m}"
            );
        }
        assert!(check_placeholder_text("no markers here", &overrides_none()).is_none());
    }

    #[test]
    fn weasel_words_word_boundary_avoids_false_positives() {
        // `shoulder` should not trigger `should`.
        assert!(check_weasel_words("she shoulders the load", &overrides_none()).is_empty());
        // `adjust` should not trigger `just`.
        assert!(check_weasel_words("we adjust the value", &overrides_none()).is_empty());
        // But standalone `should` does.
        assert!(!check_weasel_words("it should not fail", &overrides_none()).is_empty());
    }

    #[test]
    fn text_too_long_threshold_is_strict_greater_than() {
        let exactly = "a".repeat(TEXT_TOO_LONG_THRESHOLD);
        assert!(check_text_too_long(&exactly, &overrides_none()).is_none());
        let one_over = "a".repeat(TEXT_TOO_LONG_THRESHOLD + 1);
        assert!(check_text_too_long(&one_over, &overrides_none()).is_some());
    }
}
