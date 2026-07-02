//! Stderr classification for the S2 presence probe — PURE: captured
//! `cargo check` stderr in, per-accessor outcomes out. No filesystem,
//! no subprocess, so the whole decision table is unit-testable against
//! captured stderr fixtures (adapted from the de-risk spike at
//! `aretta-books/.planning/instrument-handoff/spike/s2-presence-probe/`).
//!
//! Classification ladder (SLICE23-SPEC aristo item 4):
//!
//! 1. `SUT_FEATURE_UNDECLARED` — a cargo *resolution-time* feature
//!    error (the SUT's `turso_core` has not declared the
//!    `aristo-instr` feature). This fires BEFORE any rustc `E`-code,
//!    so it is checked first: the common partial-instrumentation
//!    state must not masquerade as a missing accessor.
//! 2. `MISSING` — rustc `E0599`/`E0609` naming the record's emitted
//!    symbol (the loud per-accessor signal the spike pinned).
//! 3. `PASS` — the record was not named by any error and no
//!    unattributed errors are present.
//! 4. `INDETERMINATE` — the compile failed with errors that name no
//!    record (bad generated import, harness-probe line referencing a
//!    type the probe cannot import, SUT build breakage). Surfaced
//!    with raw evidence instead of guessed green.

use aristo_core::canon::InstrumentationBundle;

/// Per-record outcome of the feature-on presence compile.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PresenceOutcome {
    /// The accessor type-checked.
    Pass,
    /// rustc named the accessor as absent (`E0599`/`E0609`).
    Missing { evidence: String },
    /// Cargo failed feature resolution before rustc ran — the SUT
    /// package has not declared the instrumentation feature.
    FeatureUndeclared { evidence: String },
    /// The compile failed, but rustc did not name this accessor —
    /// genuine ambiguity, escalated rather than guessed.
    Indeterminate { note: String },
}

impl PresenceOutcome {
    pub(crate) fn is_pass(&self) -> bool {
        matches!(self, PresenceOutcome::Pass)
    }
}

/// The identifiers one record can be blamed by in rustc output: its
/// lock-row id and the emitted method name (tail of
/// `presence.expected_symbol`, per the emitted-symbol rule).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecordKey {
    pub accessor_id: String,
    pub method: String,
}

/// Derive the blame keys for every record in a bundle.
pub(crate) fn record_keys(bundle: &InstrumentationBundle) -> Vec<RecordKey> {
    bundle
        .records
        .iter()
        .map(|r| RecordKey {
            accessor_id: r.accessor_id.clone(),
            method: emitted_method(&r.presence.expected_symbol, &r.accessor_id),
        })
        .collect()
}

/// The emitted accessor method name: the `::`-tail of
/// `expected_symbol` (`"LogicalLog::inspect_header_version"` →
/// `"inspect_header_version"`), falling back to the accessor id.
pub(crate) fn emitted_method(expected_symbol: &str, accessor_id: &str) -> String {
    let tail = expected_symbol.rsplit("::").next().unwrap_or("").trim();
    if tail.is_empty() {
        accessor_id.to_string()
    } else {
        tail.to_string()
    }
}

/// Classify the FEATURE-ON compile per record. Returns outcomes in
/// `keys` order.
pub(crate) fn classify_feature_on(
    compile_ok: bool,
    stderr: &str,
    keys: &[RecordKey],
) -> Vec<(String, PresenceOutcome)> {
    if compile_ok {
        return keys
            .iter()
            .map(|k| (k.accessor_id.clone(), PresenceOutcome::Pass))
            .collect();
    }
    if let Some(evidence) = feature_undeclared_evidence(stderr) {
        return keys
            .iter()
            .map(|k| {
                (
                    k.accessor_id.clone(),
                    PresenceOutcome::FeatureUndeclared {
                        evidence: evidence.clone(),
                    },
                )
            })
            .collect();
    }

    let hits = missing_item_hits(stderr);
    let mut attributed: Vec<&str> = Vec::new();
    let mut outcomes = Vec::with_capacity(keys.len());
    for key in keys {
        let mine: Vec<&str> = hits
            .iter()
            .filter(|(name, _)| *name == key.method || *name == key.accessor_id)
            .map(|(_, headline)| headline.as_str())
            .collect();
        if mine.is_empty() {
            outcomes.push((key.accessor_id.clone(), PresenceOutcome::Pass));
        } else {
            attributed.extend(mine.iter().copied());
            outcomes.push((
                key.accessor_id.clone(),
                PresenceOutcome::Missing {
                    evidence: mine.join("\n"),
                },
            ));
        }
    }

    // Errors nobody claimed poison the "unnamed record => PASS"
    // inference: rustc may have stopped before type-checking every
    // body. Demote provisional passes to INDETERMINATE.
    let unattributed: Vec<&str> = error_headlines(stderr)
        .into_iter()
        .filter(|h| !attributed.contains(h))
        .collect();
    if !unattributed.is_empty() {
        for (_, outcome) in outcomes.iter_mut() {
            if outcome.is_pass() {
                *outcome = PresenceOutcome::Indeterminate {
                    note: format!(
                        "compile failed with errors that name no accessor \
                         (first: {}); this record was not individually named \
                         by rustc — inspect the probe output",
                        unattributed[0]
                    ),
                };
            }
        }
    }
    outcomes
}

/// Whole-probe verdict of the FEATURE-OFF (clean) compile: the gated
/// accessors must VANISH without the instrumentation feature. A green
/// feature-off compile means instrumentation is reachable in clean
/// SUT builds — the clean-SUT invariant is leaking.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FeatureOffVerdict {
    /// The probe failed feature-off with the accessors (or their
    /// gated imports) named absent — gating confirmed.
    GatedOk {
        /// Accessor ids rustc individually named absent.
        named: Vec<String>,
        /// True when gating was evidenced by unresolved gated imports
        /// (`E0432`/`E0433`/`E0412`/`E0603`) rather than per-accessor
        /// `E0599` lines.
        via_imports: bool,
    },
    /// The feature-off compile SUCCEEDED — accessors are reachable
    /// without the feature (gating leak).
    UngatedLeak,
    /// The feature-off compile failed for reasons that neither name
    /// an accessor nor look like vanished gated items.
    BuildIssue { excerpt: String },
}

pub(crate) fn classify_feature_off(
    compile_ok: bool,
    stderr: &str,
    keys: &[RecordKey],
) -> FeatureOffVerdict {
    if compile_ok {
        if keys.is_empty() {
            return FeatureOffVerdict::GatedOk {
                named: vec![],
                via_imports: false,
            };
        }
        return FeatureOffVerdict::UngatedLeak;
    }
    let hits = missing_item_hits(stderr);
    let named: Vec<String> = keys
        .iter()
        .filter(|k| {
            hits.iter()
                .any(|(name, _)| *name == k.method || *name == k.accessor_id)
        })
        .map(|k| k.accessor_id.clone())
        .collect();
    if !named.is_empty() {
        return FeatureOffVerdict::GatedOk {
            named,
            via_imports: false,
        };
    }
    let unresolved = error_headlines(stderr).into_iter().any(|h| {
        h.starts_with("error[E0432]")
            || h.starts_with("error[E0433]")
            || h.starts_with("error[E0412]")
            || h.starts_with("error[E0603]")
    });
    if unresolved {
        return FeatureOffVerdict::GatedOk {
            named: vec![],
            via_imports: true,
        };
    }
    let excerpt = error_headlines(stderr)
        .first()
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| {
            stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("(empty stderr)")
                .trim()
                .to_string()
        });
    FeatureOffVerdict::BuildIssue { excerpt }
}

/// Receiver-import self-healing (one-shot retry): given failing
/// stderr and the generator-derived receiver imports (type base →
/// `use` line), return the type bases whose import failed with a
/// privacy/resolution error (`E0603`/`E0432`). The runner regenerates
/// with those types imported from the crate root instead — some SUT
/// types live in private modules and are only re-exported at the root
/// (turso's `Wal`; rustc's own E0603 help suggests exactly this).
pub(crate) fn failed_receiver_imports(
    stderr: &str,
    receiver_imports: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let headlines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("error[E0603]") || l.starts_with("error[E0432]"))
        .collect();
    if headlines.is_empty() {
        return vec![];
    }
    receiver_imports
        .iter()
        .filter(|(base, import)| {
            headlines.iter().any(|h| {
                let Some(name) = first_backticked(h) else {
                    return false;
                };
                // E0603 names one private path segment; E0432 names
                // the whole unresolved path. Either must occur in the
                // derived import line to blame it.
                import.contains(&format!("::{name}::"))
                    || import.contains(&format!("::{name};"))
                    || (name.contains("::") && import.contains(name.as_str()))
                    || name == **base
            })
        })
        .map(|(base, _)| base.clone())
        .collect()
}

/// Cargo resolution-time feature error — the `SUT_FEATURE_UNDECLARED`
/// signal. This is a cargo (not rustc) failure, so it is matched on
/// cargo's message text rather than an `E`-code. Covers current
/// cargo's singular phrasing ("does not have that feature" —
/// captured verbatim in `fixtures/feature-undeclared.stderr.txt`),
/// the older plural phrasing, and the `--features` CLI rejection.
fn feature_undeclared_evidence(stderr: &str) -> Option<String> {
    const PATTERNS: [&str; 3] = [
        "does not have that feature",
        "does not have these features",
        "none of the selected packages contains these features",
    ];
    stderr
        .lines()
        .find(|l| PATTERNS.iter().any(|p| l.contains(p)))
        .map(|l| l.trim().to_string())
}

/// `(named symbol, headline)` pairs for every missing-item rustc
/// error: `E0599` (no method / no function or associated item) and
/// `E0609` (no field). The first backticked token in the headline is
/// the item name in both diagnostics.
fn missing_item_hits(stderr: &str) -> Vec<(String, String)> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("error[E0599]") || l.starts_with("error[E0609]"))
        .filter_map(|l| Some((first_backticked(l)?, l.to_string())))
        .collect()
}

/// All rustc/cargo error headlines, minus the aggregate trailers
/// (`could not compile`, `aborting due to`) that carry no signal of
/// their own.
fn error_headlines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("error"))
        .filter(|l| !l.starts_with("error: could not compile") && !l.starts_with("error: aborting"))
        .collect()
}

fn first_backticked(line: &str) -> Option<String> {
    let start = line.find('`')? + 1;
    let len = line[start..].find('`')?;
    Some(line[start..start + len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured stderr of the spike's warm POSITIVE run
    /// (`cargo check --features aristo-instr --offline`, exit 0).
    const S2A_STDERR: &str = include_str!("fixtures/s2a.stderr.txt");
    /// Captured stderr of the spike's NEGATIVE run (`cargo check`,
    /// feature off): nine clean per-accessor `E0599` errors — the
    /// classification signal this module is pinned against.
    const S2B_STDERR: &str = include_str!("fixtures/s2b.stderr.txt");
    /// Captured verbatim from this machine's cargo: the resolution
    /// failure when the probe's `[features]` table forwards to a
    /// `turso_core/aristo-instr` feature the (uninstrumented) SUT
    /// checkout has not declared. Note it fires on `cargo check`
    /// with AND without `--features` — the manifest forward alone
    /// triggers it, which is why the runner skips the feature-off
    /// pass entirely on SUT_FEATURE_UNDECLARED.
    const FEATURE_UNDECLARED_STDERR: &str = include_str!("fixtures/feature-undeclared.stderr.txt");
    /// Captured verbatim: the feature-on probe compile against the
    /// real instrumented fork where the derived deep-path receiver
    /// import (`use turso_core::storage::wal::Wal;`) hit a private
    /// module while the type is re-exported at the crate root. Drives
    /// the one-shot root-import retry.
    const PRIVATE_MODULE_STDERR: &str = include_str!("fixtures/private-module-import.stderr.txt");

    fn key(id: &str) -> RecordKey {
        RecordKey {
            accessor_id: id.into(),
            method: id.into(),
        }
    }

    fn spike_keys() -> Vec<RecordKey> {
        [
            "inspect_txs",
            "inspect_finalized",
            "inspect_tx_ids_value",
            "inspect_version_id_counter_value",
            "inspect_peek_next_ts",
            "inspect_header_version",
            "inspect_pending_running_crc",
            "inspect_mv_tx",
            "inspect_attached_mv_txs",
        ]
        .iter()
        .map(|id| key(id))
        .collect()
    }

    #[test]
    fn emitted_method_takes_expected_symbol_tail() {
        assert_eq!(
            emitted_method("LogicalLog::inspect_header_version", "x"),
            "inspect_header_version"
        );
        assert_eq!(
            emitted_method("Wal::installed_snapshot", "x"),
            "installed_snapshot"
        );
        assert_eq!(emitted_method("", "fallback_id"), "fallback_id");
    }

    #[test]
    fn successful_compile_classifies_all_pass() {
        // s2a is a success run (exit 0); stderr content is irrelevant
        // once cargo reports success.
        let outcomes = classify_feature_on(true, S2A_STDERR, &spike_keys());
        assert_eq!(outcomes.len(), 9);
        assert!(
            outcomes.iter().all(|(_, o)| o.is_pass()),
            "got: {outcomes:?}"
        );
    }

    #[test]
    fn spike_negative_run_names_every_accessor_missing() {
        let outcomes = classify_feature_on(false, S2B_STDERR, &spike_keys());
        for (id, outcome) in &outcomes {
            match outcome {
                PresenceOutcome::Missing { evidence } => {
                    assert!(
                        evidence.contains(&format!("`{id}`")),
                        "evidence for {id} must name it, got: {evidence}"
                    );
                    assert!(
                        evidence.contains("error[E0599]"),
                        "evidence keeps the rustc headline, got: {evidence}"
                    );
                }
                other => panic!("{id}: expected Missing, got {other:?}"),
            }
        }
    }

    #[test]
    fn record_not_named_by_rustc_passes_when_all_errors_attributed() {
        // A record whose accessor type-checked while its siblings
        // failed: every error in s2b maps to one of the nine spike
        // accessors, so an extra unnamed record is a PASS.
        let mut keys = spike_keys();
        keys.push(key("installed_snapshot"));
        let outcomes = classify_feature_on(false, S2B_STDERR, &keys);
        let (_, last) = outcomes.last().unwrap();
        assert!(last.is_pass(), "got: {last:?}");
    }

    #[test]
    fn unattributed_errors_demote_unnamed_records_to_indeterminate() {
        let stderr = "\
error[E0599]: no method named `inspect_txs` found for reference `&MvStore<MvccClock>` in the current scope
error[E0412]: cannot find type `HeaderView` in this scope
error: could not compile `aristo-s2-presence-probe` (lib) due to 2 previous errors
";
        let keys = vec![key("inspect_txs"), key("inspect_header_version")];
        let outcomes = classify_feature_on(false, stderr, &keys);
        assert!(
            matches!(outcomes[0].1, PresenceOutcome::Missing { .. }),
            "named record stays Missing, got: {:?}",
            outcomes[0]
        );
        match &outcomes[1].1 {
            PresenceOutcome::Indeterminate { note } => {
                assert!(
                    note.contains("E0412"),
                    "note surfaces the stray error: {note}"
                );
            }
            other => panic!("expected Indeterminate, got {other:?}"),
        }
    }

    #[test]
    fn e0609_field_error_classifies_missing() {
        let stderr = "error[E0609]: no field `offset` on type `&LogicalLog`\n";
        let keys = vec![RecordKey {
            accessor_id: "read_logicallog_offset_crc".into(),
            method: "offset".into(),
        }];
        let outcomes = classify_feature_on(false, stderr, &keys);
        assert!(
            matches!(outcomes[0].1, PresenceOutcome::Missing { .. }),
            "got: {:?}",
            outcomes[0]
        );
    }

    #[test]
    fn feature_resolution_error_classifies_sut_feature_undeclared() {
        // The resolution-time failure fires before rustc, so it must
        // win over any E-code scanning — every record gets the
        // distinct SUT_FEATURE_UNDECLARED diagnostic.
        let keys = vec![key("inspect_header_version"), key("installed_snapshot")];
        let outcomes = classify_feature_on(false, FEATURE_UNDECLARED_STDERR, &keys);
        for (id, outcome) in &outcomes {
            match outcome {
                PresenceOutcome::FeatureUndeclared { evidence } => {
                    assert!(
                        evidence.contains("does not have that feature"),
                        "{id}: evidence keeps cargo's message, got: {evidence}"
                    );
                }
                other => panic!("{id}: expected FeatureUndeclared, got {other:?}"),
            }
        }
    }

    #[test]
    fn older_cargo_plural_feature_phrasing_also_classifies() {
        // Pre-2025 cargo phrased the same failure with "these
        // features"; both spellings must classify.
        let stderr = "\
error: failed to select a version for `turso_core`.
the package `aristo-s2-presence-probe` depends on `turso_core`, with features: \
`aristo-instr` but `turso_core` does not have these features.
";
        let keys = vec![key("inspect_header_version")];
        let outcomes = classify_feature_on(false, stderr, &keys);
        assert!(
            matches!(outcomes[0].1, PresenceOutcome::FeatureUndeclared { .. }),
            "got: {:?}",
            outcomes[0]
        );
    }

    #[test]
    fn private_module_import_names_only_the_failing_receiver_type() {
        // Real-fork capture: `storage::wal` is private (Wal is a
        // crate-root re-export) while the LogicalLog deep path is
        // fine. Only Wal must be flipped to a root import.
        let mut receiver_imports = std::collections::BTreeMap::new();
        receiver_imports.insert(
            "Wal".to_string(),
            "use turso_core::storage::wal::Wal;".to_string(),
        );
        receiver_imports.insert(
            "LogicalLog".to_string(),
            "use turso_core::mvcc::persistent_storage::logical_log::LogicalLog;".to_string(),
        );
        let flips = failed_receiver_imports(PRIVATE_MODULE_STDERR, &receiver_imports);
        assert_eq!(flips, vec!["Wal".to_string()], "got: {flips:?}");
        // And before the retry, no record may classify PASS off this
        // stderr — the import error names no accessor.
        let keys = vec![key("inspect_header_version"), key("installed_snapshot")];
        let outcomes = classify_feature_on(false, PRIVATE_MODULE_STDERR, &keys);
        assert!(
            outcomes
                .iter()
                .all(|(_, o)| matches!(o, PresenceOutcome::Indeterminate { .. })),
            "got: {outcomes:?}"
        );
    }

    #[test]
    fn unresolved_import_path_blames_the_matching_receiver_type() {
        let stderr = "error[E0432]: unresolved import `turso_core::storage::wal`\n";
        let mut receiver_imports = std::collections::BTreeMap::new();
        receiver_imports.insert(
            "Wal".to_string(),
            "use turso_core::storage::wal::Wal;".to_string(),
        );
        assert_eq!(
            failed_receiver_imports(stderr, &receiver_imports),
            vec!["Wal".to_string()]
        );
        // A clean compile (or unrelated errors) flips nothing.
        assert!(failed_receiver_imports("", &receiver_imports).is_empty());
        assert!(failed_receiver_imports(
            "error[E0599]: no method named `inspect_txs` found",
            &receiver_imports
        )
        .is_empty());
    }

    #[test]
    fn feature_off_spike_negative_run_confirms_gating() {
        let verdict = classify_feature_off(false, S2B_STDERR, &spike_keys());
        match verdict {
            FeatureOffVerdict::GatedOk { named, via_imports } => {
                assert_eq!(named.len(), 9, "all nine spike accessors named");
                assert!(!via_imports);
            }
            other => panic!("expected GatedOk, got {other:?}"),
        }
    }

    #[test]
    fn feature_off_green_compile_is_a_gating_leak() {
        let verdict = classify_feature_off(true, "", &spike_keys());
        assert_eq!(verdict, FeatureOffVerdict::UngatedLeak);
    }

    #[test]
    fn feature_off_unresolved_gated_imports_count_as_gating_evidence() {
        let stderr = "\
error[E0432]: unresolved import `turso_core::mvcc::database::differential`
error: could not compile `aristo-s2-presence-probe` (lib) due to 1 previous error
";
        let verdict = classify_feature_off(false, stderr, &spike_keys());
        assert_eq!(
            verdict,
            FeatureOffVerdict::GatedOk {
                named: vec![],
                via_imports: true
            }
        );
    }

    #[test]
    fn feature_off_unrelated_failure_is_a_build_issue() {
        let stderr = "\
error[E0308]: mismatched types
error: could not compile `turso_core` (lib) due to 1 previous error
";
        let verdict = classify_feature_off(false, stderr, &spike_keys());
        match verdict {
            FeatureOffVerdict::BuildIssue { excerpt } => {
                assert!(excerpt.contains("E0308"), "got: {excerpt}");
            }
            other => panic!("expected BuildIssue, got {other:?}"),
        }
    }
}
