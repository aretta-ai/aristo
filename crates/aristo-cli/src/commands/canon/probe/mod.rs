//! `aristo canon probe` — the P-008 S2 presence probe (SLICE23-SPEC
//! aristo items 4-6; reference impl: the de-risk spike at
//! `aretta-books/.planning/instrument-handoff/spike/s2-presence-probe/`).
//!
//! Unions the instrumentation bundles carried by this repo's ACCEPTED
//! canon matches (`.aristo/canon-matches.toml`), generates an
//! ephemeral probe crate — one function per accessor record, binding
//! only the typed root receiver — `[patch]`es the SUT package to the
//! user's local checkout (`--sut-path`), and runs `cargo check`
//! twice: feature-on (presence) and feature-off (the accessors must
//! vanish — gating check). Outcomes are classified per accessor:
//!
//! - `PASS` — the accessor type-checked against the local SUT;
//! - `MISSING` — rustc named it absent (`E0599`/`E0609`); fails loud
//!   with the record's conformance `catch` and an escalation card;
//! - `SUT_FEATURE_UNDECLARED` — cargo feature resolution failed
//!   before rustc ran (the fork has not declared the feature);
//! - `INDETERMINATE` — the compile failed without naming the
//!   accessor; escalated, never guessed green;
//! - `SKIPPED(instr-debt: <id>)` — previously flag-broken accessors
//!   are re-surfaced visibly (and not counted as fresh failures),
//!   never silently green.
//!
//! Fully offline (no canon API call). The pure pieces — generation
//! (`gen`), classification (`classify`), the card (`card`), the
//! integrity check (`integrity`) — are unit-tested without a SUT;
//! only `exec` touches cargo.

mod card;
mod classify;
mod exec;
mod gen;
mod integrity;
mod ledger;

use std::collections::BTreeSet;
use std::path::Path;

use aristo_core::canon::{
    union_accepted_bundles, BundleCompileCheck, BundleProvenance, CanonMatchesFile,
    InstrumentationRecord,
};

use crate::commands::index::{now_rfc3339, workspace_or_error};
use crate::{CliError, CliResult};

use classify::{FeatureOffVerdict, PresenceOutcome};
use ledger::DebtEntry;

pub(crate) fn run(
    sut_path: &Path,
    flag_broken: Option<&str>,
    keep_probe: bool,
    gen_only: bool,
) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let cache = CanonMatchesFile::read(&ws.canon_matches_path()).map_err(CliError::Io)?;

    // Coverage-integrity check (spec item 6): accepted matches that
    // route test binaries but delivered no bundle.
    for w in integrity::coverage_integrity_warnings(&cache) {
        eprintln!("warning: {w}");
    }

    let union = union_accepted_bundles(&cache);
    for w in &union.warnings {
        eprintln!("warning: {w}");
    }

    if union.bundles.is_empty() {
        let accepted_total: usize = cache
            .entries
            .values()
            .map(|e| e.accepted_matches.len())
            .sum();
        println!(
            "S2 presence probe: no instrumentation bundles on this repo's \
             {accepted_total} accepted canon match(es) — nothing to probe."
        );
        println!(
            "  (bundles arrive on /canon/match suggestions from a P-008 conductor \
             and persist at accept time.)"
        );
        return Ok(());
    }

    let record_total: usize = union.bundles.iter().map(|b| b.records.len()).sum();
    println!(
        "S2 presence probe — {record_total} accessor record(s) in {} bundle group(s)",
        union.bundles.len()
    );

    let ledger_path = ws.aristo_dir().join(ledger::DEBT_LEDGER_FILE);
    let (debt, debt_warnings) = ledger::load_debt(&ledger_path);
    for w in debt_warnings {
        eprintln!("warning: {w}");
    }

    let mut reports: Vec<RecordReport> = Vec::new();

    for bundle in &union.bundles {
        let sut = exec::resolve_sut(
            sut_path,
            &bundle.compile_check.package,
            &bundle.provenance.sut_binding,
        )?;
        let patch_path = sut.patch_path.display().to_string();
        let mut krate = gen::generate_probe_crate(bundle, &patch_path, &BTreeSet::new());
        let dir_name = if union.bundles.len() == 1 {
            "aristo-s2-probe".to_string()
        } else {
            format!(
                "aristo-s2-probe-{}",
                short_ref(&bundle.provenance.payload_ref)
            )
        };
        let probe_dir = sut.target_dir.join(dir_name);
        exec::write_probe_crate(&probe_dir, &krate, sut.lock_path.as_deref())
            .map_err(CliError::Io)?;

        println!();
        println!(
            "bundle {} — {} record(s), payload_ref {}",
            bundle.bundle_id,
            bundle.records.len(),
            short_ref(&bundle.provenance.payload_ref)
        );
        println!("  probe crate: {}", probe_dir.display());

        if gen_only {
            println!("  --gen-only: compile skipped; probe crate left in place.");
            continue;
        }

        // Build-graph sharing is mandatory (PROPOSAL ~line 390): a
        // cold graph is allowed but NEVER paid silently.
        if sut.cold_reasons.is_empty() {
            println!(
                "  build graph: warm — reusing {} + {}",
                sut.sut_root.join("Cargo.lock").display(),
                sut.target_dir.display()
            );
        } else {
            eprintln!("warning: cold build graph for the probe compile:");
            for r in &sut.cold_reasons {
                eprintln!("  - {r}");
            }
            eprintln!(
                "  The first probe compile will be a full SUT build (minutes, \
                 one-time), not the warm seconds-fast check; the shared target \
                 dir is reused afterwards."
            );
        }

        let keys = classify::record_keys(bundle);

        let mut on = exec::run_cargo_check(
            &probe_dir,
            &sut.target_dir,
            Some(&bundle.compile_check.features),
        )?;
        println!(
            "  [feature-on ] cargo check --features {} ... {} in {:.1}s",
            bundle.compile_check.features,
            if on.success { "ok" } else { "FAILED" },
            on.secs
        );

        // One-shot self-healing: a derived receiver import that hit a
        // private module (E0603/E0432) is retried via the crate-root
        // re-export before classifying (turso's `Wal` shape).
        if !on.success {
            let flips = classify::failed_receiver_imports(&on.stderr, &krate.receiver_imports);
            if !flips.is_empty() {
                println!(
                    "  note: receiver import(s) {} hit a private module — retrying \
                     with the crate-root re-export",
                    flips.join(", ")
                );
                let overrides: BTreeSet<String> = flips.into_iter().collect();
                krate = gen::generate_probe_crate(bundle, &patch_path, &overrides);
                exec::write_probe_crate(&probe_dir, &krate, sut.lock_path.as_deref())
                    .map_err(CliError::Io)?;
                on = exec::run_cargo_check(
                    &probe_dir,
                    &sut.target_dir,
                    Some(&bundle.compile_check.features),
                )?;
                println!(
                    "  [feature-on ] retry ... {} in {:.1}s",
                    if on.success { "ok" } else { "FAILED" },
                    on.secs
                );
            }
        }
        let mut outcomes = classify::classify_feature_on(on.success, &on.stderr, &keys);

        let feature_undeclared = outcomes
            .iter()
            .any(|(_, o)| matches!(o, PresenceOutcome::FeatureUndeclared { .. }));
        if feature_undeclared {
            // No feature => nothing is gated ON anywhere; a feature-off
            // compile would only re-prove the accessors' absence and
            // (on a cold graph) pay a pointless full SUT build.
            println!(
                "  [feature-off] skipped — feature resolution failed \
                 (SUT_FEATURE_UNDECLARED); declare the feature first."
            );
        } else {
            run_feature_off_check(&probe_dir, &sut.target_dir, bundle, &keys)?;
        }

        if keep_probe {
            println!("  probe crate kept at {}", probe_dir.display());
        } else {
            let _ = std::fs::remove_dir_all(&probe_dir);
        }
        // Records the generator could not express: never let them
        // classify as PASS off the back of an error they can't cause.
        for (id, why) in &krate.degraded {
            if let Some((_, outcome)) = outcomes.iter_mut().find(|(oid, _)| oid == id) {
                *outcome = PresenceOutcome::Indeterminate {
                    note: format!("no probe statement could be generated: {why}"),
                };
            }
        }

        for (id, outcome) in outcomes {
            let record = bundle
                .records
                .iter()
                .find(|r| r.accessor_id == id)
                .expect("outcome ids come from bundle records")
                .clone();
            reports.push(RecordReport {
                bundle_id: bundle.bundle_id.clone(),
                record,
                provenance: bundle.provenance.clone(),
                compile_check: bundle.compile_check.clone(),
                outcome,
                debt: debt.get(&id).cloned(),
            });
        }
    }

    if gen_only {
        return Ok(());
    }

    // The flag-broken decision (spec item 5): record visible debt for
    // an accessor this very run observed failing.
    if let Some(flag_id) = flag_broken {
        let Some(report) = reports.iter_mut().find(|r| r.record.accessor_id == flag_id) else {
            let known: Vec<&str> = reports
                .iter()
                .map(|r| r.record.accessor_id.as_str())
                .collect();
            return Err(CliError::Other {
                message: format!(
                    "--flag-broken {flag_id}: no such accessor in the unioned \
                     bundles (known: {})",
                    known.join(", ")
                ),
                exit_code: 2,
            });
        };
        if report.outcome.is_pass() {
            return Err(CliError::Other {
                message: format!(
                    "--flag-broken {flag_id}: the accessor PASSED the presence \
                     probe — nothing to flag."
                ),
                exit_code: 2,
            });
        }
        let entry = DebtEntry {
            accessor_id: flag_id.to_string(),
            bundle_id: report.bundle_id.clone(),
            base_ref: report.provenance.base_ref.clone(),
            payload_ref: report.provenance.payload_ref.clone(),
            catch: report.record.catch.clone(),
            evidence: outcome_evidence(&report.outcome),
            flagged_at: now_rfc3339(),
        };
        ledger::append_debt_entry(&ledger_path, &entry).map_err(CliError::Io)?;
        println!();
        println!(
            "flag-broken: recorded `{flag_id}` in {} (visible instrumentation debt; \
             re-surfaced on every probe run).",
            ledger_path.display()
        );
        report.debt = Some(entry);
    }

    let (lines, summary) = report_lines(&reports);
    println!();
    println!("per-accessor:");
    for line in lines {
        println!("  {line}");
    }

    // Escalation cards for fresh (unflagged) failures.
    for r in &reports {
        if !r.outcome.is_pass() && r.debt.is_none() {
            println!();
            print!(
                "{}",
                card::render_escalation_card(
                    &r.record,
                    &r.provenance,
                    &r.compile_check,
                    &r.outcome
                )
            );
        }
    }

    println!();
    println!(
        "summary: {} pass, {} missing, {} feature-undeclared, {} indeterminate, \
         {} skipped (instr-debt)",
        summary.pass,
        summary.missing,
        summary.feature_undeclared,
        summary.indeterminate,
        summary.skipped_debt
    );
    if summary.failures() > 0 {
        println!(
            "FAIL: {} accessor(s) not proven present — land the proposed fixes (or \
             record visible debt via --flag-broken) and re-run.",
            summary.failures()
        );
        return Err(CliError::Silent { exit_code: 1 });
    }
    println!("ok: presence proven for every unflagged accessor.");
    Ok(())
}

/// Run + report the feature-off (clean) check: the gated accessors
/// must VANISH without the instrumentation feature.
fn run_feature_off_check(
    probe_dir: &Path,
    target_dir: &Path,
    bundle: &aristo_core::canon::InstrumentationBundle,
    keys: &[classify::RecordKey],
) -> CliResult<()> {
    let off = exec::run_cargo_check(probe_dir, target_dir, None)?;
    match classify::classify_feature_off(off.success, &off.stderr, keys) {
        FeatureOffVerdict::GatedOk { named, via_imports } => {
            if via_imports {
                println!(
                    "  [feature-off] gated imports vanish without the feature — \
                     gating confirmed in {:.1}s",
                    off.secs
                );
            } else {
                println!(
                    "  [feature-off] accessors vanish without the feature \
                     ({}/{} named by rustc) — gating confirmed in {:.1}s",
                    named.len(),
                    keys.len(),
                    off.secs
                );
            }
        }
        FeatureOffVerdict::UngatedLeak => {
            eprintln!(
                "warning: feature-off `cargo check` unexpectedly SUCCEEDED — the \
                 accessors are reachable without `{}`; instrumentation may be \
                 leaking into clean SUT builds (clean-SUT invariant).",
                bundle.compile_check.features
            );
        }
        FeatureOffVerdict::BuildIssue { excerpt } => {
            eprintln!(
                "warning: feature-off `cargo check` failed for reasons that do \
                 not look like gated items vanishing: {excerpt}"
            );
        }
    }
    Ok(())
}

/// One classified record, with everything the report/card/ledger
/// needs.
struct RecordReport {
    bundle_id: String,
    record: InstrumentationRecord,
    provenance: BundleProvenance,
    compile_check: BundleCompileCheck,
    outcome: PresenceOutcome,
    /// Pre-existing (or just-recorded) instr-debt entry.
    debt: Option<DebtEntry>,
}

#[derive(Debug, Default, PartialEq)]
struct Summary {
    pass: usize,
    missing: usize,
    feature_undeclared: usize,
    indeterminate: usize,
    skipped_debt: usize,
}

impl Summary {
    /// Fresh failures — flagged debt is visible but not fatal (the
    /// PROPOSAL's "visibly SKIPPED, never silently GREEN" contract).
    fn failures(&self) -> usize {
        self.missing + self.feature_undeclared + self.indeterminate
    }
}

/// Render the per-accessor report lines — PURE, so the fail-loud
/// wording is pinned by unit tests.
fn report_lines(reports: &[RecordReport]) -> (Vec<String>, Summary) {
    let mut lines = Vec::with_capacity(reports.len());
    let mut summary = Summary::default();
    for r in reports {
        let id = &r.record.accessor_id;
        let catch = single_line(&r.record.catch);
        match (&r.outcome, &r.debt) {
            (PresenceOutcome::Pass, None) => {
                summary.pass += 1;
                lines.push(format!(
                    "PASS                   {id}  ({})",
                    r.record.presence.expected_symbol
                ));
            }
            (PresenceOutcome::Pass, Some(d)) => {
                summary.pass += 1;
                lines.push(format!(
                    "PASS                   {id} — note: flagged as instr-debt {} but now \
                     passes; consider clearing its ledger entry",
                    d.flagged_at
                ));
            }
            (_, Some(d)) => {
                summary.skipped_debt += 1;
                lines.push(format!(
                    "SKIPPED(instr-debt: {id}) — flagged {}; the routed tests must stay \
                     visibly SKIPPED, never silently green; catch: {catch}",
                    d.flagged_at
                ));
            }
            (PresenceOutcome::Missing { .. }, None) => {
                summary.missing += 1;
                lines.push(format!("MISSING                {id} — needed by: {catch}"));
            }
            (PresenceOutcome::FeatureUndeclared { .. }, None) => {
                summary.feature_undeclared += 1;
                lines.push(format!(
                    "SUT_FEATURE_UNDECLARED {id} — the SUT has not declared the \
                     instrumentation feature; needed by: {catch}"
                ));
            }
            (PresenceOutcome::Indeterminate { note }, None) => {
                summary.indeterminate += 1;
                lines.push(format!(
                    "INDETERMINATE          {id} — {}; needed by: {catch}",
                    single_line(note)
                ));
            }
        }
    }
    (lines, summary)
}

fn outcome_evidence(outcome: &PresenceOutcome) -> String {
    match outcome {
        PresenceOutcome::Pass => "pass".to_string(),
        PresenceOutcome::Missing { evidence } => evidence.clone(),
        PresenceOutcome::FeatureUndeclared { evidence } => {
            format!("SUT_FEATURE_UNDECLARED: {evidence}")
        }
        PresenceOutcome::Indeterminate { note } => format!("indeterminate: {note}"),
    }
}

fn short_ref(s: &str) -> String {
    s.chars().take(7).collect()
}

fn single_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = include_str!("fixtures/golden-bundle.json");

    fn golden_reports() -> Vec<RecordReport> {
        let bundle: aristo_core::canon::InstrumentationBundle =
            serde_json::from_str(GOLDEN).unwrap();
        bundle
            .records
            .iter()
            .map(|r| RecordReport {
                bundle_id: bundle.bundle_id.clone(),
                record: r.clone(),
                provenance: bundle.provenance.clone(),
                compile_check: bundle.compile_check.clone(),
                outcome: PresenceOutcome::Pass,
                debt: None,
            })
            .collect()
    }

    fn debt(flagged_at: &str) -> DebtEntry {
        DebtEntry {
            accessor_id: "installed_snapshot".into(),
            bundle_id: "turso:7b6cbae:ae85f8792372".into(),
            base_ref: "ad351877".into(),
            payload_ref: "7b6cbaec".into(),
            catch: "WAL install coherence (WR-03).".into(),
            evidence: "error[E0599]".into(),
            flagged_at: flagged_at.into(),
        }
    }

    #[test]
    fn all_pass_report_has_no_failures() {
        let (lines, summary) = report_lines(&golden_reports());
        assert_eq!(summary.pass, 2);
        assert_eq!(summary.failures(), 0);
        assert!(lines[0].starts_with("PASS"), "got: {}", lines[0]);
        assert!(
            lines[0].contains("LogicalLog::inspect_header_version"),
            "got: {}",
            lines[0]
        );
    }

    #[test]
    fn missing_line_fails_loud_naming_accessor_and_catch() {
        let mut reports = golden_reports();
        reports[1].outcome = PresenceOutcome::Missing {
            evidence: "error[E0599]: no method named `installed_snapshot` found".into(),
        };
        let (lines, summary) = report_lines(&reports);
        assert_eq!(summary.missing, 1);
        assert_eq!(summary.failures(), 1);
        assert!(
            lines[1].starts_with("MISSING                installed_snapshot"),
            "got: {}",
            lines[1]
        );
        assert!(
            lines[1].contains("needed by: WAL install coherence"),
            "the failure names the test that needs it (catch); got: {}",
            lines[1]
        );
    }

    #[test]
    fn feature_undeclared_gets_its_distinct_tag() {
        let mut reports = golden_reports();
        for r in &mut reports {
            r.outcome = PresenceOutcome::FeatureUndeclared {
                evidence: "does not have these features".into(),
            };
        }
        let (lines, summary) = report_lines(&reports);
        assert_eq!(summary.feature_undeclared, 2);
        assert!(
            lines
                .iter()
                .all(|l| l.starts_with("SUT_FEATURE_UNDECLARED")),
            "got: {lines:?}"
        );
    }

    #[test]
    fn flagged_debt_is_visibly_skipped_not_failed() {
        let mut reports = golden_reports();
        reports[1].outcome = PresenceOutcome::Missing {
            evidence: "error[E0599]".into(),
        };
        reports[1].debt = Some(debt("2026-07-01T09:00:00Z"));
        let (lines, summary) = report_lines(&reports);
        assert_eq!(summary.skipped_debt, 1);
        assert_eq!(summary.failures(), 0, "flagged debt is not a fresh failure");
        assert!(
            lines[1].starts_with("SKIPPED(instr-debt: installed_snapshot)"),
            "the PROPOSAL's exact visibly-skipped shape; got: {}",
            lines[1]
        );
        assert!(
            lines[1].contains("flagged 2026-07-01T09:00:00Z"),
            "got: {}",
            lines[1]
        );
    }

    #[test]
    fn passing_flagged_accessor_suggests_clearing_the_ledger() {
        let mut reports = golden_reports();
        reports[1].debt = Some(debt("2026-07-01T09:00:00Z"));
        let (lines, summary) = report_lines(&reports);
        assert_eq!(summary.pass, 2);
        assert_eq!(summary.skipped_debt, 0);
        assert!(
            lines[1].contains("consider clearing its ledger entry"),
            "got: {}",
            lines[1]
        );
    }

    #[test]
    fn indeterminate_counts_as_failure() {
        let mut reports = golden_reports();
        reports[0].outcome = PresenceOutcome::Indeterminate {
            note: "compile failed with errors that name no accessor".into(),
        };
        let (_, summary) = report_lines(&reports);
        assert_eq!(summary.indeterminate, 1);
        assert_eq!(
            summary.failures(),
            1,
            "ambiguity is escalated, never guessed green"
        );
    }
}
