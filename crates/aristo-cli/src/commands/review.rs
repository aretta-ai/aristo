//! `aristo review [--mark <id>]... [--json]` — review newly-authored intents
//! (Phase 18 #7; the nudge engine's first consumer).
//!
//! The reviewed/unreviewed axis lives in the git-untracked
//! `.aristo/nudge-state.toml` reviewed map (a text/body hash drift re-opens a
//! reviewed intent — a reviewer approved a specific version, not the id
//! forever). This command is the surface the `aristo-authored-review` skill
//! drives:
//!
//! - **list** what awaits review, split into *new-this-session* vs *backlog*
//!   when a SessionStart edit-window baseline exists;
//! - **`--mark <id>`** records an intent as reviewed once the user has looked
//!   at it.
//!
//! Editing or deleting an intent is a source operation the agent does
//! directly (then `aristo stamp`); the re-stamp's hash drift naturally
//! re-opens an edited intent. Subject-only: every line describes the user's
//! own annotations.

use std::path::Path;

use serde::Serialize;

use crate::commands::index::workspace_or_error;
use crate::commands::show::{load_index, status_label, verify_label};
use crate::nudge::intents::{authored_intents, AuthoredIntent};
use crate::nudge::state::{NudgeState, STATE_FILENAME};
use crate::{CliError, CliResult};

/// Schema version of the `--json` snapshot (bump on a breaking shape change).
const REVIEW_SCHEMA_VERSION: u32 = 1;

pub(crate) fn run(json: bool, mark: Vec<String>) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let index = load_index(&ws)?;
    let authored = authored_intents(&index);
    let state_path = ws.aristo_dir().join(STATE_FILENAME);
    let mut state = NudgeState::load(&state_path);

    let mark_ids = expand_ids(&mark);
    if !mark_ids.is_empty() {
        return run_mark(&state_path, &mut state, &authored, &mark_ids, json);
    }

    let snapshot = build_snapshot(&authored, &state);
    if json {
        emit_json(&snapshot)
    } else {
        print_human(&snapshot);
        Ok(())
    }
}

/// Split comma-joined ids and trim, so `--mark a,b` and `--mark a --mark b`
/// are equivalent (mirrors `aristo critique --filter id=a,b`).
fn expand_ids(raw: &[String]) -> Vec<String> {
    raw.iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[aristo::intent(
    "Marking validates EVERY requested id against the index before writing any \
     review state: an unknown id (a typo, or an id that isn't an authored \
     intent) marks nothing and errors. Partial marking on a bad batch would \
     silently record some reviews and drop others, leaving the backlog \
     disagreeing with what the user believes they reviewed.",
    verify = "test",
    id = "review_mark_validates_all_ids_before_writing"
)]
fn run_mark(
    state_path: &Path,
    state: &mut NudgeState,
    authored: &[AuthoredIntent],
    ids: &[String],
    json: bool,
) -> CliResult<()> {
    // Validate the WHOLE batch first (atomic): every id must be an authored
    // intent before anything is written.
    let mut resolved: Vec<&AuthoredIntent> = Vec::with_capacity(ids.len());
    let mut unknown: Vec<&str> = Vec::new();
    for id in ids {
        match authored.iter().find(|i| &i.id == id) {
            Some(i) => resolved.push(i),
            None => unknown.push(id),
        }
    }
    if !unknown.is_empty() {
        return Err(CliError::Other {
            message: format!(
                "not authored intent id(s) (nothing marked): {}. \
                 Run `aristo review` to list reviewable ids.",
                unknown.join(", ")
            ),
            exit_code: 2,
        });
    }

    for i in &resolved {
        state.mark_reviewed(&i.id, &i.text_hash, &i.body_hash);
    }
    state.save(state_path).map_err(|e| CliError::Other {
        message: format!("failed to write nudge state: {e}"),
        exit_code: 1,
    })?;

    let remaining = unreviewed(authored, state).len();
    if json {
        let out = serde_json::json!({
            "marked": resolved.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            "remaining_unreviewed": remaining,
        });
        let text = serde_json::to_string_pretty(&out).map_err(|e| CliError::Other {
            message: format!("serializing review result: {e}"),
            exit_code: 1,
        })?;
        println!("{text}");
    } else {
        println!(
            "Marked {} intent(s) reviewed · {remaining} still await review.",
            resolved.len()
        );
    }
    Ok(())
}

/// The authored intents not currently vouched for by the reviewed map.
fn unreviewed<'a>(authored: &'a [AuthoredIntent], state: &NudgeState) -> Vec<&'a AuthoredIntent> {
    authored
        .iter()
        .filter(|i| !state.is_reviewed(&i.id, &i.text_hash, &i.body_hash))
        .collect()
}

/// The machine-readable review snapshot (`--json`) and the basis for the human
/// readout — one shape so the two outputs can never disagree.
#[derive(Debug, Serialize)]
struct ReviewSnapshot {
    schema_version: u32,
    unreviewed_count: usize,
    new_count: usize,
    backlog_count: usize,
    /// Whether a SessionStart edit-window baseline was captured. Without one,
    /// the new/backlog split is suppressed (`new_count = 0`, everything
    /// reported under `unreviewed_count`).
    baseline_captured: bool,
    intents: Vec<ReviewIntentJson>,
}

#[derive(Debug, Serialize)]
struct ReviewIntentJson {
    id: String,
    text: String,
    file: String,
    site: String,
    status: String,
    verify: String,
    is_new: bool,
}

#[aristo::intent(
    "New vs backlog is computed against the SessionStart edit-window baseline: \
     an unreviewed intent is 'new' only when a baseline WAS captured and its \
     id is absent from it (authored this session). With no baseline the split \
     is suppressed (new_count = 0) rather than guessed — calling everything \
     'new' on a fresh checkout would misreport a long-standing backlog as \
     this-session's work.",
    verify = "test",
    id = "review_new_vs_backlog_needs_a_baseline"
)]
fn build_snapshot(authored: &[AuthoredIntent], state: &NudgeState) -> ReviewSnapshot {
    let unrev = unreviewed(authored, state);
    let baseline = state.window_intent_ids.as_ref();
    let baseline_captured = baseline.is_some();

    let intents: Vec<ReviewIntentJson> = unrev
        .iter()
        .map(|i| ReviewIntentJson {
            id: i.id.clone(),
            text: i.text.clone(),
            file: i.file.clone(),
            site: i.site.clone(),
            status: status_label(i.status).to_string(),
            verify: verify_label(i.verify),
            // "new" requires a baseline AND absence from it.
            is_new: baseline.map(|set| !set.contains(&i.id)).unwrap_or(false),
        })
        .collect();

    let unreviewed_count = intents.len();
    let new_count = intents.iter().filter(|i| i.is_new).count();
    let backlog_count = unreviewed_count - new_count;

    ReviewSnapshot {
        schema_version: REVIEW_SCHEMA_VERSION,
        unreviewed_count,
        new_count,
        backlog_count,
        baseline_captured,
        intents,
    }
}

fn emit_json(snapshot: &ReviewSnapshot) -> CliResult<()> {
    let out = serde_json::to_string_pretty(snapshot).map_err(|e| CliError::Other {
        message: format!("serializing review snapshot: {e}"),
        exit_code: 1,
    })?;
    println!("{out}");
    Ok(())
}

fn print_human(s: &ReviewSnapshot) {
    println!("Aristo review");
    if s.unreviewed_count == 0 {
        println!("  All authored intents are reviewed. ✅");
        return;
    }
    if s.baseline_captured {
        println!(
            "  {} await review — {} new this session · {} backlog.",
            s.unreviewed_count, s.new_count, s.backlog_count
        );
    } else {
        println!("  {} await review.", s.unreviewed_count);
    }
    println!();
    for i in &s.intents {
        if s.baseline_captured {
            let tag = if i.is_new { "new" } else { "backlog" };
            println!(
                "  {} [{tag}] ({}, {}, {})",
                i.id, i.site, i.verify, i.status
            );
        } else {
            println!("  {} ({}, {}, {})", i.id, i.site, i.verify, i.status);
        }
        println!("    {}", i.text);
    }
    println!();
    println!("Mark reviewed with: aristo review --mark <id>[,<id>...]");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nudge::state::Baseline;
    use aristo_core::index::{
        AnnotationId, BindingState, CoveredRegion, IndexEntry, IndexFile, IntentEntry, Meta,
        Sha256, Status, VerifyLevel, VerifyMethod,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent_entry(text: &str) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: text.into(),
            verify: VerifyLevel::Method(VerifyMethod::Test),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn foo".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn index_with(ids: &[&str]) -> IndexFile {
        let mut idx = IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: BTreeMap::new(),
        };
        for id in ids {
            idx.entries
                .insert(AnnotationId::parse(id).unwrap(), intent_entry(id));
        }
        idx
    }

    #[test]
    fn no_baseline_suppresses_the_split() {
        let idx = index_with(&["i_one", "i_two"]);
        let authored = authored_intents(&idx);
        let state = NudgeState::default();
        let snap = build_snapshot(&authored, &state);
        assert_eq!(snap.unreviewed_count, 2);
        assert!(!snap.baseline_captured);
        assert_eq!(snap.new_count, 0, "no baseline ⇒ nothing claimed as new");
        assert_eq!(snap.backlog_count, 2);
    }

    #[test]
    fn baseline_splits_new_from_backlog() {
        let idx = index_with(&["i_old", "i_new"]);
        let authored = authored_intents(&idx);
        let state = NudgeState {
            baseline: Some(Baseline {
                score: 0.0,
                tier: "Aspirant".into(),
            }),
            // Only i_old existed when the window started; i_new is this session.
            window_intent_ids: Some(BTreeSet::from(["i_old".to_string()])),
            ..Default::default()
        };
        let snap = build_snapshot(&authored, &state);
        assert!(snap.baseline_captured);
        assert_eq!(snap.unreviewed_count, 2);
        assert_eq!(snap.new_count, 1);
        assert_eq!(snap.backlog_count, 1);
        let new_ids: Vec<&str> = snap
            .intents
            .iter()
            .filter(|i| i.is_new)
            .map(|i| i.id.as_str())
            .collect();
        assert_eq!(new_ids, vec!["i_new"]);
    }

    #[test]
    fn marking_drops_an_intent_from_the_backlog() {
        let idx = index_with(&["i_one", "i_two"]);
        let authored = authored_intents(&idx);
        let mut state = NudgeState::default();
        // Mark i_one against its current hashes.
        state.mark_reviewed("i_one", sha('a').as_str(), sha('b').as_str());
        let snap = build_snapshot(&authored, &state);
        assert_eq!(snap.unreviewed_count, 1);
        assert_eq!(snap.intents[0].id, "i_two");
    }

    #[test]
    fn mark_rejects_unknown_id_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILENAME);
        let idx = index_with(&["i_one"]);
        let authored = authored_intents(&idx);
        let mut state = NudgeState::default();
        let err = run_mark(
            &path,
            &mut state,
            &authored,
            &["i_one".to_string(), "i_bogus".to_string()],
            false,
        )
        .unwrap_err();
        match err {
            CliError::Other { exit_code, .. } => assert_eq!(exit_code, 2),
            other => panic!("expected Other(exit 2), got {other:?}"),
        }
        // Atomic: the valid id was NOT marked because the batch had a bad id.
        assert!(
            !state.is_reviewed("i_one", sha('a').as_str(), sha('b').as_str()),
            "a bad batch must mark nothing"
        );
        assert!(!path.exists(), "no state file written on a rejected batch");
    }

    #[test]
    fn expand_ids_splits_commas_and_trims() {
        let got = expand_ids(&["a, b".to_string(), "c".to_string(), "  ,".to_string()]);
        assert_eq!(got, vec!["a", "b", "c"]);
    }
}
