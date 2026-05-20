//! Mechanical validator for `.aristo/critiques/<id>.critique` files.
//!
//! Mirrors the shape of verify's validator but with far fewer rules:
//! critique findings are advisory, not derivations, so there is no
//! proof-tree integrity to check. Schema enums (category, severity)
//! and hash anchors are the load-bearing structural checks.

use aristo_core::critique::{CritiqueFile, Severity};
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile};

#[derive(Debug)]
pub(crate) struct CritiqueReport {
    pub failures: Vec<CritiqueFailure>,
}

impl CritiqueReport {
    fn empty() -> Self {
        Self {
            failures: Vec::new(),
        }
    }

    fn push(&mut self, location: String, detail: String) {
        self.failures.push(CritiqueFailure { location, detail });
    }

    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "critique rejected ({} check(s) failed):",
            self.failures.len()
        );
        for f in &self.failures {
            let _ = writeln!(out, "  • {}: {}", f.location, f.detail);
        }
        out
    }
}

#[derive(Debug)]
pub(crate) struct CritiqueFailure {
    pub location: String,
    pub detail: String,
}

#[aristo::intent(
    "The critique validator gates writes on schema integrity: enum \
     values for category and severity (serde rejects unknown variants \
     at parse time, so by the time we run the checks here those are \
     already known to be in the locked set), the focal id resolves in \
     the current index, and the rationale field is non-empty (a finding \
     without a rationale is noise — silently dropping the requirement \
     would let agents emit categorized-but-uninformative critiques). \
     Unlike the verify validator there is no proof-tree integrity check \
     because findings carry no derivations.",
    verify = "neural",
    id = "critique_validator_checks_schema_and_focal_id_in_index"
)]
pub(crate) fn validate(
    focal_id: &AnnotationId,
    cf: &CritiqueFile,
    index: &IndexFile,
) -> CritiqueReport {
    let mut r = CritiqueReport::empty();

    // Focal id must exist in the current index.
    let Some(entry) = index.entries.get(focal_id) else {
        r.push(
            "critique.focal_id".into(),
            format!("`{}` is not in the current index", focal_id.as_str()),
        );
        return r;
    };

    // Hash staleness anchor: the critique was produced against a
    // specific annotation text. If the entry's current text_hash differs,
    // the critique is stale — reject so the agent re-pends.
    let cur_text_hash = match entry {
        IndexEntry::Intent(e) => &e.text_hash,
        IndexEntry::Assume(e) => &e.text_hash,
    };
    if &cf.critique.critiqued_at_text_hash != cur_text_hash {
        r.push(
            "critique.critiqued_at_text_hash".into(),
            format!(
                "differs from current index text_hash; the annotation \
                 text drifted since this critique was produced — re-run \
                 `aristo critique --filter id={}` to refresh",
                focal_id.as_str()
            ),
        );
    }

    // Per-finding sanity. We don't validate enum membership here because
    // serde rejected anything out of the locked set at parse time.
    for (i, f) in cf.critique.findings.iter().enumerate() {
        if f.rationale.trim().is_empty() {
            r.push(
                format!("critique.findings[{i}].rationale"),
                "is empty; every finding must include a rationale".into(),
            );
        }
        if matches!(f.category, aristo_core::critique::Category::Rephrasing)
            && f.suggested_text
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
        {
            // Soft warning encoded as a hard reject: rephrasing without
            // a suggested replacement is the most common low-value
            // critique pattern. Force agents to commit to a proposed
            // rewrite or recategorize as 'clarity' or 'info'.
            r.push(
                format!("critique.findings[{i}]"),
                "category=`rephrasing` requires a non-empty `suggested_text`; \
                 use category=`clarity` or `info` for findings that don't \
                 propose specific replacement text"
                    .into(),
            );
        }
    }
    r
}

#[aristo::intent(
    "On accept, the SDK derives `finding_count` from `findings.len()` \
     and `highest_severity` from `findings.iter().map(|f| f.severity).max()`. \
     Agents may submit these fields explicitly (the schema accepts them) \
     but the SDK overwrites — single source of truth, no agent/SDK \
     disagreement on derived state. None when findings is empty.",
    verify = "neural",
    id = "critique_derived_fields_stamped_by_sdk_not_agent"
)]
pub(crate) fn stamp_derived(cf: &mut CritiqueFile) {
    cf.critique.finding_count = Some(cf.critique.findings.len() as u32);
    cf.critique.highest_severity = highest_severity(&cf.critique.findings);
}

fn highest_severity(findings: &[aristo_core::critique::Finding]) -> Option<Severity> {
    findings.iter().map(|f| f.severity).max()
}
