//! Escalation-card rendering for the S2 presence probe — PURE.
//!
//! Layout per PROPOSAL §"Escalation card + flag-broken debt"
//! (`aretta-books/.planning/instrument-handoff/PROPOSAL.md` ~405-419):
//! `intent` / `placed-at` (with the authored refs) / `observed`
//! (rustc evidence) / `proposed fix` / `evidence` / `decision:
//! [ accept ] [ adjust ] [ flag-broken ]`. One additive field beyond
//! the PROPOSAL sketch: `catch` — the probe's fail-LOUD mandate names
//! the test that needs the accessor, and the card is where the human
//! decides, so the catch belongs on it.

use std::fmt::Write as _;

use aristo_core::canon::{BundleCompileCheck, BundleProvenance, InstrumentationRecord};

use super::classify::PresenceOutcome;

/// Render the escalation card for one non-passing record.
pub(crate) fn render_escalation_card(
    record: &InstrumentationRecord,
    provenance: &BundleProvenance,
    compile_check: &BundleCompileCheck,
    outcome: &PresenceOutcome,
) -> String {
    let observed = match outcome {
        PresenceOutcome::Missing { evidence } => single_line(evidence),
        PresenceOutcome::FeatureUndeclared { evidence } => format!(
            "SUT_FEATURE_UNDECLARED — cargo failed feature resolution before rustc ran: {}",
            single_line(evidence)
        ),
        PresenceOutcome::Indeterminate { note } => format!("(indeterminate) {}", single_line(note)),
        PresenceOutcome::Pass => "presence PASS (no escalation needed)".to_string(),
    };

    let mut card = String::new();
    let _ = writeln!(
        card,
        "INSTRUMENTATION ESCALATION — accessor: {}",
        record.accessor_id
    );
    let _ = writeln!(card, "  intent:       {}", single_line(&record.intent));
    let _ = writeln!(card, "  catch:        {}", single_line(&record.catch));
    let _ = writeln!(
        card,
        "  placed-at:    authored at payload_ref {} (base_ref {}) — \
         \"supposed to be at commit {}\"",
        short_ref(&provenance.payload_ref),
        short_ref(&provenance.base_ref),
        short_ref(&provenance.authored_at)
    );
    let _ = writeln!(card, "  observed:     {observed}");
    let _ = writeln!(
        card,
        "  proposed fix: {}",
        proposed_fix(record, compile_check, outcome)
    );
    let _ = writeln!(card, "  evidence:     {}", evidence_line(record));
    let _ = writeln!(
        card,
        "  decision:     [ accept ] [ adjust ] [ flag-broken ]"
    );
    let _ = writeln!(
        card,
        "                accept:      land the proposed fix in the SUT, then re-run \
         `aristo canon probe`"
    );
    let _ = writeln!(
        card,
        "                adjust:      the landing site moved — re-place the payload at \
         the new site (the harness key stays stable), then re-run"
    );
    let _ = writeln!(
        card,
        "                flag-broken: re-run with `--flag-broken {}` to record visible \
         debt in .aristo/instrumentation-debt.jsonl",
        record.accessor_id
    );
    card
}

fn proposed_fix(
    record: &InstrumentationRecord,
    compile_check: &BundleCompileCheck,
    outcome: &PresenceOutcome,
) -> String {
    let target = &record.landing.target;
    let file = target.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let container = target
        .get("container")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    if matches!(outcome, PresenceOutcome::FeatureUndeclared { .. }) {
        return format!(
            "declare the instrumentation cargo feature on the SUT package `{}` \
             (compile_check features: `{}`) — until it exists, no accessor can \
             be compiled in",
            compile_check.package, compile_check.features
        );
    }

    match &record.landing.annotation {
        Some(annotation) => {
            let field = field_display(target);
            let mut fix = format!(
                "paste the bundle's annotation on `{field}` in {file}: {}",
                single_line(annotation)
            );
            if let Some(derive) = &record.landing.ensure_derive {
                let _ = write!(fix, "; ensure {} on `{container}`", single_line(derive));
            }
            fix.push_str("; the harness key stays stable");
            fix
        }
        None => {
            let mut fix = format!(
                "add `{}` to `{container}` in {file}",
                record.presence.expected_signature
            );
            if !record.landing.companions_ref.is_empty() {
                let _ = write!(
                    fix,
                    " (companions: {})",
                    record.landing.companions_ref.join(", ")
                );
            }
            fix
        }
    }
}

/// PROPOSAL: "rustc diagnostic (Class B / required: the S1 oracle
/// RED->GREEN is the evidence)".
fn evidence_line(record: &InstrumentationRecord) -> String {
    if record.class == "A" && record.semantic_tier != "required" {
        "rustc diagnostic".to_string()
    } else {
        format!(
            "rustc diagnostic (Class {} / tier {}: the S1 oracle RED->GREEN is the \
             confirming evidence)",
            record.class, record.semantic_tier
        )
    }
}

/// `field` may be a string or (for `field_read` rows) an array.
fn field_display(target: &serde_json::Value) -> String {
    match target.get("field") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("`/`"),
        _ => "?".to_string(),
    }
}

fn short_ref(s: &str) -> String {
    if s.chars().count() > 9 {
        let head: String = s.chars().take(9).collect();
        format!("{head}…")
    } else {
        s.to_string()
    }
}

fn single_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::canon::InstrumentationBundle;

    const GOLDEN: &str = include_str!("fixtures/golden-bundle.json");

    fn golden_bundle() -> InstrumentationBundle {
        serde_json::from_str(GOLDEN).expect("golden fixture decodes")
    }

    #[test]
    fn missing_hand_written_fn_card_carries_every_proposal_field() {
        let bundle = golden_bundle();
        let record = &bundle.records[1]; // installed_snapshot (annotation: null)
        let outcome = PresenceOutcome::Missing {
            evidence: "error[E0599]: no method named `installed_snapshot` found for \
                       reference `&dyn Wal` in the current scope"
                .into(),
        };
        let card =
            render_escalation_card(record, &bundle.provenance, &bundle.compile_check, &outcome);

        assert!(
            card.starts_with("INSTRUMENTATION ESCALATION — accessor: installed_snapshot"),
            "got:\n{card}"
        );
        for label in [
            "  intent:       ",
            "  catch:        ",
            "  placed-at:    ",
            "  observed:     ",
            "  proposed fix: ",
            "  evidence:     ",
            "  decision:     [ accept ] [ adjust ] [ flag-broken ]",
        ] {
            assert!(
                card.contains(label),
                "card must carry {label:?}, got:\n{card}"
            );
        }
        // placed-at cites both refs (the "supposed to be at commit"
        // citation is authored_at = payload_ref).
        assert!(card.contains("payload_ref 7b6cbaec0…"), "got:\n{card}");
        assert!(card.contains("base_ref ad351877c…"), "got:\n{card}");
        assert!(
            card.contains("supposed to be at commit 7b6cbaec0…"),
            "got:\n{card}"
        );
        // Hand-written kind (annotation null): the fix is the expected
        // signature + companions, never a fabricated annotation.
        assert!(
            card.contains(
                "add `fn installed_snapshot(&self) -> WalInstalledSnapshot` to \
                 `Wal (trait) / impl Wal for WalFile` in core/storage/wal.rs"
            ),
            "got:\n{card}"
        );
        assert!(
            card.contains("companions: WalInstalledSnapshot, Wal::installed_snapshot"),
            "got:\n{card}"
        );
        // required-tier: evidence defers to the S1 oracle.
        assert!(
            card.contains("the S1 oracle RED->GREEN is the confirming evidence"),
            "got:\n{card}"
        );
        assert!(
            card.contains("`--flag-broken installed_snapshot`"),
            "got:\n{card}"
        );
        assert!(
            card.contains("observed:     error[E0599]: no method named `installed_snapshot`"),
            "got:\n{card}"
        );
    }

    #[test]
    fn missing_annotation_kind_card_proposes_pasting_the_annotation() {
        let bundle = golden_bundle();
        let record = &bundle.records[0]; // inspect_header_version (inspect_projection)
        let outcome = PresenceOutcome::Missing {
            evidence: "error[E0599]: no method named `inspect_header_version` found".into(),
        };
        let card =
            render_escalation_card(record, &bundle.provenance, &bundle.compile_check, &outcome);
        assert!(
            card.contains(
                "paste the bundle's annotation on `header` in \
                           core/mvcc/persistent_storage/logical_log.rs"
            ),
            "got:\n{card}"
        );
        assert!(
            card.contains(
                "ensure #[cfg_attr(feature = \"aristo-instr\", derive(Inspect))] \
                           on `LogicalLog`"
            ),
            "got:\n{card}"
        );
        assert!(
            card.contains("the harness key stays stable"),
            "got:\n{card}"
        );
        // Class A / tier none: plain rustc evidence.
        assert!(
            card.contains("  evidence:     rustc diagnostic\n"),
            "got:\n{card}"
        );
    }

    #[test]
    fn feature_undeclared_card_gets_the_distinct_diagnostic() {
        let bundle = golden_bundle();
        let record = &bundle.records[0];
        let outcome = PresenceOutcome::FeatureUndeclared {
            evidence: "the package `aristo-s2-presence-probe` depends on `turso_core`, \
                       with features: `aristo-instr` but `turso_core` does not have \
                       these features."
                .into(),
        };
        let card =
            render_escalation_card(record, &bundle.provenance, &bundle.compile_check, &outcome);
        assert!(
            card.contains("observed:     SUT_FEATURE_UNDECLARED"),
            "got:\n{card}"
        );
        assert!(
            card.contains("declare the instrumentation cargo feature"),
            "got:\n{card}"
        );
    }
}
