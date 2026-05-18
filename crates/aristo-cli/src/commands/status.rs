//! `aristo status` — project-level summary.
//!
//! Phase 1 subset of the mockup-11 J7 output: reads the workspace's
//! `aristo.toml` for the default verify level, the index for the
//! annotation breakdown (by kind / verify / status), and reports schema
//! version. Phase 2 fields (tier, quota, B5b binding counts, bundled
//! key registry) wait for the server-side commands.

use std::collections::BTreeMap;

use aristo_core::index::{
    AssumeEntry, IndexEntry, IndexFile, IntentEntry, VerifyLevel, VerifyMethod,
};

use crate::commands::index::workspace_or_error;
use crate::commands::show::{read_index, status_label};
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::CliResult;

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;
    let report = freshness_check(&ws);
    emit_advisory_if_stale(&report);
    let index = read_index(&ws.index_path())?;

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

    println!();
    println!(
        "[INFO] For per-annotation diagnostics, run `aristo stamp` (or `aristo list --filter status=<state>`)."
    );
    Ok(())
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
            VerifyLevel::Method(VerifyMethod::Neural) => self.verify_neural += 1,
            VerifyLevel::Method(VerifyMethod::Test) => self.verify_test += 1,
            VerifyLevel::Method(VerifyMethod::Full) => self.verify_full += 1,
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
