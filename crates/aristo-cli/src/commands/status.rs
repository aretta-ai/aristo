//! `aristo status` — project-level summary.
//!
//! Phase 1 subset of the mockup-11 J7 output: reads the workspace's
//! `aristo.toml` for the default verify level, the index for the
//! annotation breakdown (by kind / verify / status), and reports schema
//! version. Slice 34 added the per-pipeline verification rate
//! breakdown (k/N per verify level) and tier + visible score (the same
//! computation `aristo badge` uses). Phase 2 fields (quota, B5b
//! binding counts, bundled key registry) wait for the server-side
//! commands.

use std::collections::BTreeMap;

use aristo_core::badge::{compute_tier, TierComputation};
use aristo_core::index::{
    AssumeEntry, IndexEntry, IndexFile, IntentEntry, Status, VerifyLevel, VerifyMethod,
};
use aristo_core::walk::{count_fns_per_module_with, WalkOptions};

use crate::commands::index::workspace_or_error;
use crate::commands::show::{load_index, status_label};
use crate::{CliError, CliResult};

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;
    let index = load_index(&ws)?;

    println!();
    println!("Aristo SDK v{}", env!("CARGO_PKG_VERSION"));
    println!("  Default verify:    {}", default_verify_for_display(&ws));

    let counts = Counts::from(&index);
    println!();
    println!("Annotations:");
    println!("  Total:             {}", counts.total);
    println!(
        "  By kind:           intent={}   assume={}",
        counts.intent, counts.assume
    );
    println!(
        "  By verify level:   neural={}   test={}   full={}   true={}   false={}",
        counts.verify_neural,
        counts.verify_test,
        counts.verify_full,
        counts.verify_true,
        counts.verify_false,
    );
    print!("  By status:         ");
    let status_pairs = counts.status_breakdown();
    let pieces: Vec<String> = status_pairs
        .iter()
        .map(|(label, n)| format!("{label}={n}"))
        .collect();
    println!("{}", pieces.join("   "));

    println!();
    println!("Verification rate (verified / total per pipeline):");
    println!(
        "  neural:            {} / {} ({})",
        counts.verified_neural,
        counts.verify_neural,
        format_rate(counts.verified_neural, counts.verify_neural),
    );
    println!(
        "  test:              {} / {} ({})",
        counts.verified_test,
        counts.verify_test,
        format_rate(counts.verified_test, counts.verify_test),
    );
    println!(
        "  full:              {} / {} ({})",
        counts.verified_full,
        counts.verify_full,
        format_rate(counts.verified_full, counts.verify_full),
    );

    let computation = compute_project_tier(&ws, &index)?;
    println!();
    println!("Tier:");
    println!(
        "  Score:             {:.3}  (visible)",
        computation.visible_score
    );
    println!("  Tier:              {}", computation.tier.label());
    if computation.arete_gate_met {
        println!("  Areté gate:        ✓ met (paid-formal-proof + server-bound)");
    }

    println!();
    println!("Index health:");
    println!("  schema_version: {} (current)", index.meta.schema_version);

    // ─── review-session backlog + active surfacing (slice 27.5 D9) ─
    // Surface deferred items so the user notices the backlog without
    // having to start a review session to find them. Surface an
    // active session so they don't forget it's open.
    let backlog_counts = collect_backlog_counts(&ws)?;
    if !backlog_counts.is_empty() {
        println!();
        println!("Review backlog:");
        for (kind, count) in &backlog_counts {
            let item_word = if *count == 1 { "item" } else { "items" };
            println!("  {kind}: {count} {item_word}");
        }
    }
    if let Some(active) = active_session_summary(&ws)? {
        println!();
        println!("Active review session:");
        println!("  {active}");
    }

    print_canon_health(&ws);
    print_skill_health(&ws);

    println!();
    println!(
        "For per-annotation diagnostics, run `aristo stamp` (or `aristo list --filter status=<state>`)."
    );
    Ok(())
}

/// Surface installed-skill staleness (project + user scope) so a binary
/// update that left the agent skill files behind is visible from `status`.
/// Read-only; omitted entirely when no Aristo skills are installed.
fn print_skill_health(ws: &crate::Workspace) {
    let rows =
        crate::commands::install_skills::audit_rows(Some(&ws.root), dirs::home_dir().as_deref());
    if rows.is_empty() {
        return;
    }
    println!();
    println!("Skills:");
    for r in &rows {
        let mark = if r.stale { "⚠" } else { "✓" };
        println!("  {mark} {} ({}): {}", r.agent_pretty, r.scope, r.summary);
    }
    if rows.iter().any(|r| r.stale) {
        println!("  Re-pin with: aristo install-skills --update");
    }
}

#[aristo::intent(
    "`aristo status` includes a canon-health block sourced from \
     local state only — credentials presence (no token print), the \
     [canon] config flag, last_fetched + canon_version + \
     effective_scopes from `.aristo/canon-matches.toml`'s meta, and \
     pending/accepted/rejected counts across all annotations. The \
     block must NOT make a network call: status is the offline \
     daily-loop summary; coupling it to canon API state would \
     break the offline-friendly invariant.",
    verify = "neural",
    id = "status_canon_health_is_offline_only"
)]
fn print_canon_health(ws: &crate::Workspace) {
    use aristo_core::canon::auth;
    use aristo_core::canon::cache::CanonMatchesFile;

    println!();
    println!("Canon binding:");

    // Config: enabled?
    let config = ws.load_config();
    if !config.canon.enabled {
        println!("  Status:            disabled (`[canon] enabled = false` in aristo.toml)");
        return;
    }

    // Auth: do we have a resolvable token?
    let auth_state = match auth::resolve() {
        Ok(_) => "authenticated (token present)".to_string(),
        Err(aristo_core::canon::AuthError::NoToken) => "no token (free-tier mode)".to_string(),
        Err(aristo_core::canon::AuthError::Malformed(_)) => {
            "malformed credentials file".to_string()
        }
        Err(aristo_core::canon::AuthError::Invalid) => {
            "invalid token (server rejected)".to_string()
        }
    };
    println!("  Auth:              {auth_state}");

    // Cache: last_fetched / canon_version / bucket counts.
    let cache_path = ws.canon_matches_path();
    let cache = CanonMatchesFile::read(&cache_path).unwrap_or_default();
    let last_fetched = cache.meta.last_fetched.as_deref().unwrap_or("never");
    let canon_version = cache.meta.canon_version.as_deref().unwrap_or("—");
    println!("  Last fetched:      {last_fetched}");
    println!("  Catalog version:   {canon_version}");

    // Bucket counts across all annotations.
    let mut pending = 0usize;
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for entry in cache.entries.values() {
        pending += entry.pending_matches.len();
        accepted += entry.accepted_matches.len();
        rejected += entry.rejected_matches.len();
    }
    println!("  Pending:           {pending}");
    println!("  Accepted (bound):  {accepted}");
    println!("  Rejected:          {rejected}");

    // Effective scopes is informational; we don't store it on the cache
    // meta yet (the runner stores canon_version + last_fetched only),
    // so surface the documented default :vanilla scope as a hint when
    // we don't have richer info to surface.
    let _ = config; // future: render flavor list from config if added
}

/// Walk `.aristo/sessions/backlog/` and return (kind, count) for
/// every per-kind backlog file. Returns empty on missing directory.
fn collect_backlog_counts(ws: &crate::Workspace) -> CliResult<Vec<(String, usize)>> {
    let dir = ws.sessions_backlog_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out: Vec<(String, usize)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(kind) = name.strip_suffix(".toml") else {
            continue;
        };
        if let Ok(count) = crate::session::backlog::count(ws, kind) {
            if count > 0 {
                out.push((kind.to_string(), count));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Compact summary of the active session, if any. Format:
/// `id=… kind=… subject=… (open=N accepted=M rejected=K pending=J)`.
fn active_session_summary(ws: &crate::Workspace) -> CliResult<Option<String>> {
    let Some(id) = crate::session::storage::read_active_pointer(ws)? else {
        return Ok(None);
    };
    let Some(session) = crate::session::storage::read_active_session(ws, &id)? else {
        return Ok(None);
    };
    let c = session.bucket_counts();
    Ok(Some(format!(
        "id={} kind={} subject={} (open={} accepted={} rejected={} pending={})",
        session.id, session.kind, session.subject, c.open, c.accepted, c.rejected, c.pending
    )))
}

/// Render a `verified / total` rate as a percentage string. Special-
/// cases the empty-denominator path so the output is "n/a" rather than
/// `0.0%` (which falsely implies "0 of something").
fn format_rate(verified: usize, total: usize) -> String {
    if total == 0 {
        "n/a".to_string()
    } else {
        let pct = (verified as f64 / total as f64) * 100.0;
        format!("{pct:.1}%")
    }
}

#[aristo::intent(
    "Status's tier computation routes through `aristo_core::badge::compute_tier` \
     with the same `count_fns_per_module_with(WalkOptions::none())` denominator \
     that `aristo badge` uses. Drift between the two would produce a project \
     where the badge SVG and `aristo status` report different tiers — a \
     contradiction the user can't reconcile. Sharing the call site (not the \
     formula) is the load-bearing invariant.",
    verify = "test",
    id = "status_tier_call_matches_badge_command_call"
)]
fn compute_project_tier(ws: &crate::Workspace, index: &IndexFile) -> CliResult<TierComputation> {
    let fn_counts =
        count_fns_per_module_with(&ws.root, &WalkOptions::none()).map_err(|e| CliError::Other {
            message: format!("failed to walk source for tier computation: {e}"),
            exit_code: 1,
        })?;
    let default_method = ws.load_config().verify.default_method;
    Ok(compute_tier(index, &fn_counts, default_method))
}

fn default_verify_for_display(ws: &crate::Workspace) -> String {
    let path = ws.config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return "(aristo.toml unreadable)".to_string();
    };
    let Ok(cfg): Result<aristo_core::config::ConfigFile, _> = toml::from_str(&text) else {
        return "(aristo.toml unparseable)".to_string();
    };
    match cfg.verify.default_method {
        Some(VerifyMethod::Neural) => "\"neural\"".to_string(),
        Some(VerifyMethod::Test) => "\"test\"".to_string(),
        Some(VerifyMethod::Full) => "\"full\"".to_string(),
        None => "(per-tier default)".to_string(),
    }
}

#[derive(Debug, Default)]
struct Counts {
    total: usize,
    intent: usize,
    assume: usize,
    verify_neural: usize,
    verify_test: usize,
    verify_full: usize,
    verify_true: usize,
    verify_false: usize,
    /// Intents with `verify = "neural"` that have landed in
    /// [`Status::Neural`] (the clean state for the neural pipeline).
    verified_neural: usize,
    /// Intents with `verify = "test"` that have landed in
    /// [`Status::Tested`].
    verified_test: usize,
    /// Intents with `verify = "full"` that have landed in
    /// [`Status::Verified`]. The full pipeline's terminal clean state
    /// is `Verified` (the strictest of the three).
    verified_full: usize,
    by_status: BTreeMap<&'static str, usize>,
}

impl Counts {
    fn from(index: &IndexFile) -> Self {
        let mut c = Counts::default();
        for entry in index.entries.values() {
            c.total += 1;
            match entry {
                IndexEntry::Intent(e) => {
                    c.intent += 1;
                    c.tally_verify(e);
                    c.tally_status_intent(e);
                }
                IndexEntry::Assume(e) => {
                    c.assume += 1;
                    c.tally_status_assume(e);
                }
            }
        }
        c
    }

    fn tally_verify(&mut self, e: &IntentEntry) {
        match e.verify {
            VerifyLevel::Method(VerifyMethod::Neural) => {
                self.verify_neural += 1;
                if e.status == Status::Neural {
                    self.verified_neural += 1;
                }
            }
            VerifyLevel::Method(VerifyMethod::Test) => {
                self.verify_test += 1;
                if e.status == Status::Tested {
                    self.verified_test += 1;
                }
            }
            VerifyLevel::Method(VerifyMethod::Full) => {
                self.verify_full += 1;
                if e.status == Status::Verified {
                    self.verified_full += 1;
                }
            }
            VerifyLevel::Bool(true) => self.verify_true += 1,
            VerifyLevel::Bool(false) => self.verify_false += 1,
        }
    }

    fn tally_status_intent(&mut self, e: &IntentEntry) {
        *self.by_status.entry(status_label(e.status)).or_insert(0) += 1;
    }

    fn tally_status_assume(&mut self, e: &AssumeEntry) {
        *self.by_status.entry(status_label(e.status)).or_insert(0) += 1;
    }

    #[cfg(test)]
    fn verification_rate_triple(&self) -> ((usize, usize), (usize, usize), (usize, usize)) {
        (
            (self.verified_neural, self.verify_neural),
            (self.verified_test, self.verify_test),
            (self.verified_full, self.verify_full),
        )
    }

    /// Emit status counts in a stable order: most-trusted states first,
    /// then unknown, then anomalies last. Filters out zero-count states
    /// so the line stays readable.
    fn status_breakdown(&self) -> Vec<(&'static str, usize)> {
        let order = [
            "verified",
            "tested",
            "neural",
            "stale",
            "unknown",
            "pending-deepen",
            "orphan",
            "forged",
        ];
        let mut out = Vec::new();
        for label in order {
            let n = *self.by_status.get(label).unwrap_or(&0);
            if n > 0 {
                out.push((label, n));
            }
        }
        if out.is_empty() {
            out.push(("unknown", 0));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AnnotationId, BindingState, CoveredRegion, IndexEntry, IndexFile, IntentEntry, Meta,
        Sha256, Status, VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent_with(verify: VerifyLevel, status: Status) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify,
            status,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn build(entries: &[(&str, IndexEntry)]) -> IndexFile {
        let mut map = BTreeMap::new();
        for (id, e) in entries {
            map.insert(AnnotationId::parse(id).unwrap(), e.clone());
        }
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: map,
        }
    }

    #[test]
    fn verification_rate_neural_pipeline_counts_only_status_neural() {
        let idx = build(&[
            (
                "a",
                intent_with(VerifyLevel::Method(VerifyMethod::Neural), Status::Neural),
            ),
            (
                "b",
                intent_with(VerifyLevel::Method(VerifyMethod::Neural), Status::Unknown),
            ),
            (
                "c",
                intent_with(VerifyLevel::Method(VerifyMethod::Neural), Status::Stale),
            ),
        ]);
        let c = Counts::from(&idx);
        let ((vn, n), _, _) = c.verification_rate_triple();
        assert_eq!((vn, n), (1, 3), "1 of 3 neural intents in clean state");
    }

    #[test]
    fn verification_rate_test_pipeline_counts_only_status_tested() {
        let idx = build(&[
            (
                "a",
                intent_with(VerifyLevel::Method(VerifyMethod::Test), Status::Tested),
            ),
            (
                "b",
                intent_with(VerifyLevel::Method(VerifyMethod::Test), Status::Tested),
            ),
            (
                "c",
                intent_with(VerifyLevel::Method(VerifyMethod::Test), Status::Stale),
            ),
            (
                "d",
                // test intent that drifted into neural status is NOT counted
                // as verified — pipeline mismatch.
                intent_with(VerifyLevel::Method(VerifyMethod::Test), Status::Neural),
            ),
        ]);
        let c = Counts::from(&idx);
        let (_, (vt, t), _) = c.verification_rate_triple();
        assert_eq!((vt, t), (2, 4));
    }

    #[test]
    fn verification_rate_full_pipeline_counts_only_status_verified() {
        let idx = build(&[
            (
                "a",
                intent_with(VerifyLevel::Method(VerifyMethod::Full), Status::Verified),
            ),
            (
                "b",
                intent_with(VerifyLevel::Method(VerifyMethod::Full), Status::Tested),
            ),
        ]);
        let c = Counts::from(&idx);
        let (_, _, (vf, f)) = c.verification_rate_triple();
        assert_eq!((vf, f), (1, 2));
    }

    #[test]
    fn format_rate_empty_denominator_is_not_a_number() {
        assert_eq!(format_rate(0, 0), "n/a");
    }

    #[test]
    fn format_rate_normal_case_shows_one_decimal_place() {
        assert_eq!(format_rate(1, 3), "33.3%");
        assert_eq!(format_rate(3, 4), "75.0%");
        assert_eq!(format_rate(0, 5), "0.0%");
    }
}
