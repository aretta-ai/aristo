//! `aristo verify` — orchestrate per-entry verification dispatch.
//!
//! Slice 22 ships the dispatcher skeleton plus the `verify = false`
//! no-op arm. The other arms (`"neural"` slice 23, `"test"` slice 24,
//! `"full"` slice 26) currently return [`CliError::NotImplemented`]
//! with their target slice pointer so the user sees a precise ETA
//! rather than a silent skip.
//!
//! Flags follow J2 (mockup 11): `--filter` reuses the unified J2
//! grammar from `aristo list` / `graph` / `review`; `--rerun` is the
//! orthogonal force-flag that re-processes entries already in a clean
//! verified state; `--check` is the CI no-write mode; `--strict` is
//! reserved for warn-severity outcomes (no warnings yet — slice 22 has
//! only skip / not-implemented arms).

use aristo_core::config::ConfigFile;
use aristo_core::index::{AnnotationId, IndexEntry, Status, VerifyLevel, VerifyMethod};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::filter::Filter;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

pub(crate) fn run(
    filter_strings: &[String],
    rerun: bool,
    check: bool,
    strict: bool,
) -> CliResult<()> {
    let _ = (check, strict); // wired for forward-compat; no behavior yet (see module doc)
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;
    let filters = parse_filters(filter_strings)?;
    let cfg = ws.load_config();

    let mut stats = Stats::default();
    for (id, entry) in index.entries.iter() {
        if !matches_all(id, entry, &filters) {
            continue;
        }
        if !rerun && is_clean_verified(entry) {
            stats.skipped_clean += 1;
            continue;
        }
        match resolve_verify_level(entry, &cfg) {
            VerifyLevel::Bool(false) => {
                aristo::intent_stmt!(
                    "The verify=false arm invokes no skill, runs no \
                     test, writes no signed outcome — it is the \
                     intentional opt-out path, not a stub awaiting \
                     implementation. A contributor who 'completes' this \
                     arm by adding a skill call defeats the design: \
                     the entire purpose of verify=false is to keep an \
                     annotation as documentation without paying any \
                     verification cost.",
                    verify = "neural",
                    id = "verify_false_arm_is_intentional_skip"
                );
                stats.skipped_doc_only += 1;
            }
            VerifyLevel::Bool(true) => {
                unreachable!("resolve_verify_level returns Method(..) for Bool(true)")
            }
            VerifyLevel::Method(VerifyMethod::Neural) => {
                return Err(CliError::NotImplemented {
                    what: "aristo verify (verify=\"neural\")",
                    slice: "slice 23",
                });
            }
            VerifyLevel::Method(VerifyMethod::Test) => {
                return Err(CliError::NotImplemented {
                    what: "aristo verify (verify=\"test\")",
                    slice: "slice 24",
                });
            }
            VerifyLevel::Method(VerifyMethod::Full) => {
                return Err(CliError::NotImplemented {
                    what: "aristo verify (verify=\"full\")",
                    slice: "slice 26",
                });
            }
        }
    }

    emit_summary(&stats);
    Ok(())
}

#[derive(Default, Debug)]
struct Stats {
    /// Annotations with `verify = false`: documentation only, skipped
    /// without invoking any skill or updating status.
    skipped_doc_only: usize,
    /// Annotations already in a clean verified state, skipped because
    /// `--rerun` was not passed (default policy; --rerun overrides).
    skipped_clean: usize,
}

fn emit_summary(stats: &Stats) {
    // Single-line summary: keeps the trycmd `[..]` scenarios matchable
    // and the daily-loop output unobtrusive. When slice 25 unblocks
    // verify_false_skipped.md the multi-line per-entry format returns
    // (that spec asserts per-id bullet lines); slice 22 stays terse.
    println!(
        "ok: 0 annotations verified, {} skipped (documentation only).",
        stats.skipped_doc_only + stats.skipped_clean
    );
}

fn parse_filters(filter_strings: &[String]) -> CliResult<Vec<Filter>> {
    let mut out = Vec::with_capacity(filter_strings.len());
    for raw in filter_strings {
        let f: Filter = raw.parse().map_err(|e| CliError::Other {
            message: format!("{e}"),
            exit_code: 2,
        })?;
        out.push(f);
    }
    Ok(out)
}

fn matches_all(id: &AnnotationId, entry: &IndexEntry, filters: &[Filter]) -> bool {
    filters.iter().all(|f| matches_filter(id, entry, f))
}

fn matches_filter(id: &AnnotationId, entry: &IndexEntry, f: &Filter) -> bool {
    match f {
        Filter::Id(want) => id.as_str() == want,
        Filter::File(want) => file_of(entry) == want,
        Filter::Parent(want) => match parent_ids(entry) {
            Some(ids) => ids.any_match(want),
            None => false,
        },
        Filter::Status(want) => crate::commands::show::status_label(status_of(entry)) == want,
    }
}

fn file_of(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

fn status_of(entry: &IndexEntry) -> Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

trait ParentMatch {
    fn any_match(&self, want: &str) -> bool;
}

impl ParentMatch for aristo_core::index::ParentLink {
    fn any_match(&self, want: &str) -> bool {
        self.iter().any(|p| p.as_str() == want)
    }
}

fn parent_ids(entry: &IndexEntry) -> Option<&aristo_core::index::ParentLink> {
    match entry {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }
}

#[aristo::intent(
    "`assume` entries have no `verify` field by design — they describe \
     external trust (OS guarantees, library invariants, environment \
     contracts), not properties of THIS code, so there is no internal \
     method that could verify them. They resolve to Bool(false) here \
     (the same arm as opt-out intents) so the dispatcher's single \
     skip-without-skill path handles both. A refactor that tries to \
     verify assumes would either invent a verification semantic the \
     design rejects or fail trying.",
    verify = "neural",
    id = "verify_assumes_are_documentation_only_by_design"
)]
#[aristo::intent(
    "Bool(true) resolves through the project's [verify].default_method \
     and falls back to the free-tier default (\"test\") when absent. \
     A refactor that hard-codes either side would silently change \
     verification depth for every annotation that opted into the \
     project default — those are precisely the entries where the \
     author deferred to project policy, so a silent override defeats \
     the deferral.",
    verify = "neural",
    id = "verify_bool_true_resolves_through_project_default"
)]
fn resolve_verify_level(entry: &IndexEntry, cfg: &ConfigFile) -> VerifyLevel {
    let raw = match entry {
        IndexEntry::Intent(e) => e.verify,
        IndexEntry::Assume(_) => return VerifyLevel::Bool(false),
    };
    match raw {
        VerifyLevel::Bool(true) => match cfg.verify.default_method {
            Some(m) => VerifyLevel::Method(m),
            None => VerifyLevel::Method(VerifyMethod::Test), // free-tier default
        },
        other => other,
    }
}

#[aristo::intent(
    "Entries with Status in {Verified, Tested, Neural} are skipped by \
     default. --rerun overrides for post-key-rotation sweeps. A \
     refactor that re-runs verification unconditionally would make \
     every CI run pay the LLM / cargo-test cost even when nothing \
     drifted — the default-skip-clean policy is the load-bearing \
     contract that keeps the daily-loop cost bounded.",
    verify = "neural",
    id = "verify_skips_clean_entries_unless_rerun"
)]
fn is_clean_verified(entry: &IndexEntry) -> bool {
    matches!(
        status_of(entry),
        Status::Verified | Status::Tested | Status::Neural
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{IntentEntry, Sha256};

    fn intent(id: &str, verify: VerifyLevel, status: Status) -> (AnnotationId, IndexEntry) {
        let zero = Sha256::from_bytes(b"");
        (
            AnnotationId::parse(id).unwrap(),
            IndexEntry::Intent(IntentEntry {
                text: "x".to_string(),
                verify,
                status,
                text_hash: zero.clone(),
                body_hash: zero,
                file: "src/x.rs".to_string(),
                site: "fn x (line 1)".to_string(),
                covered_region: aristo_core::index::CoveredRegion::Function,
                binding: aristo_core::index::BindingState::Local,
                parent: None,
            }),
        )
    }

    #[test]
    fn bool_true_resolves_to_project_default_method_when_set() {
        let (_, entry) = intent("foo", VerifyLevel::Bool(true), Status::Unknown);
        let mut cfg = ConfigFile::default();
        cfg.verify.default_method = Some(VerifyMethod::Full);
        assert_eq!(
            resolve_verify_level(&entry, &cfg),
            VerifyLevel::Method(VerifyMethod::Full)
        );
    }

    #[test]
    fn bool_true_falls_back_to_test_when_no_project_default() {
        let (_, entry) = intent("foo", VerifyLevel::Bool(true), Status::Unknown);
        let cfg = ConfigFile::default();
        assert_eq!(
            resolve_verify_level(&entry, &cfg),
            VerifyLevel::Method(VerifyMethod::Test)
        );
    }

    #[test]
    fn bool_false_resolves_to_bool_false() {
        let (_, entry) = intent("foo", VerifyLevel::Bool(false), Status::Unknown);
        let cfg = ConfigFile::default();
        assert_eq!(resolve_verify_level(&entry, &cfg), VerifyLevel::Bool(false));
    }

    #[test]
    fn clean_verified_statuses_are_skipped_by_default() {
        for s in [Status::Verified, Status::Tested, Status::Neural] {
            let (_, entry) = intent("foo", VerifyLevel::Bool(true), s);
            assert!(
                is_clean_verified(&entry),
                "{s:?} should be considered clean-verified"
            );
        }
    }

    #[test]
    fn unknown_and_stale_are_not_clean_verified() {
        for s in [
            Status::Unknown,
            Status::Stale,
            Status::Orphan,
            Status::Forged,
            Status::PendingDeepen,
        ] {
            let (_, entry) = intent("foo", VerifyLevel::Bool(true), s);
            assert!(
                !is_clean_verified(&entry),
                "{s:?} should NOT be considered clean-verified"
            );
        }
    }
}
