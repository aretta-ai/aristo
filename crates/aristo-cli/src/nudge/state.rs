//! Persistent, git-untracked engine state: `.aristo/nudge-state.toml`
//! (Phase 18 #9, S0c). Holds the runtime facts the index can't carry:
//!
//! - the **reviewed map** (`annotation id → {text_hash, body_hash, reviewed}`)
//!   — the reviewed/unreviewed axis #7 drives; a hash drift re-flips an entry
//!   to unreviewed (D6/Q3);
//! - the **proof-reviewed map** (`proof id → reviewed`);
//! - the **edit-window baseline** (score + tier captured at SessionStart) for
//!   the congrats / slump signals;
//! - the per-window **edit counter** for the authoring-debt signal;
//! - per-signal **throttle** records (last fired + last surfaced metric).
//!
//! Writes are atomic (tempfile + rename), tolerant on read (a missing or
//! corrupt file degrades to default state — the engine never fails a hook on
//! bad state). The file is git-untracked: it is per-user runtime, like
//! `.aristo/sessions/`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Basename under `.aristo/`.
pub const STATE_FILENAME: &str = "nudge-state.toml";

/// One annotation's review record. `reviewed` is re-flipped to the
/// unreviewed view whenever the live text/body hashes drift from these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub text_hash: String,
    pub body_hash: String,
    pub reviewed: bool,
}

/// The score/tier snapshot captured at the start of an edit window, against
/// which congrats (gain) and slump (drop) are measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub score: f64,
    /// The tier label at baseline (compared by string; the engine only needs
    /// "did it change", not ordering).
    pub tier: String,
}

/// Per-signal throttle bookkeeping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThrottleRecord {
    /// Unix epoch seconds the signal last surfaced (0 if never).
    pub last_fired_epoch: u64,
    /// The metric value at the last surfacing (for material-change re-arm).
    pub last_surfaced_metric: f64,
}

/// The whole engine state file. Every field defaults, so older / partial
/// files deserialize cleanly.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NudgeState {
    /// Source edits since the last annotation was added (authoring-debt).
    pub edits_since_annotation: usize,
    /// The edit-window baseline, if captured.
    pub baseline: Option<Baseline>,
    /// annotation id → review record.
    pub reviewed: BTreeMap<String, ReviewRecord>,
    /// proof id → reviewed.
    pub proof_reviewed: BTreeMap<String, bool>,
    /// signal id → throttle record.
    pub throttle: BTreeMap<String, ThrottleRecord>,
}

impl NudgeState {
    /// Read the state file, tolerating absence and corruption (either yields
    /// default state). A nudge hook must never fail on bad state.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Atomically write the state file (tempfile in the same dir + rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Count intents the user has not (currently) reviewed. An intent is
    /// unreviewed when it has no record, its record is `reviewed = false`, or
    /// its live hashes have drifted from the reviewed snapshot (a post-review
    /// edit re-opens it). `current` is `(id, text_hash, body_hash)` for every
    /// authored intent in the index.
    pub fn unreviewed_count<'a>(
        &self,
        current: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> usize {
        current
            .into_iter()
            .filter(|(id, text_hash, body_hash)| !self.is_reviewed(id, text_hash, body_hash))
            .count()
    }

    #[aristo::intent(
        "A review is current only while the annotation's text AND body hashes \
         still match the snapshot taken when it was reviewed. Editing the \
         claim or the covered code after review re-opens it (reads as \
         unreviewed) — a reviewer approved a specific version, not the id \
         forever. Without the hash check, a post-review edit would keep a \
         stale 'reviewed' badge and the review backlog would under-count work \
         that genuinely needs another look.",
        verify = "test",
        id = "nudge_review_is_current_only_until_hash_drift"
    )]
    /// True iff this annotation is currently reviewed (record present,
    /// `reviewed = true`, and both hashes still match — no drift).
    pub fn is_reviewed(&self, id: &str, text_hash: &str, body_hash: &str) -> bool {
        match self.reviewed.get(id) {
            Some(r) => r.reviewed && r.text_hash == text_hash && r.body_hash == body_hash,
            None => false,
        }
    }

    /// Mark an annotation reviewed against its current hashes (clears any
    /// prior drift).
    pub fn mark_reviewed(&mut self, id: &str, text_hash: &str, body_hash: &str) {
        self.reviewed.insert(
            id.to_string(),
            ReviewRecord {
                text_hash: text_hash.to_string(),
                body_hash: body_hash.to_string(),
                reviewed: true,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(text: &str, body: &str, reviewed: bool) -> ReviewRecord {
        ReviewRecord {
            text_hash: text.into(),
            body_hash: body.into(),
            reviewed,
        }
    }

    #[test]
    fn missing_file_loads_default_state() {
        let dir = tempfile::tempdir().unwrap();
        let s = NudgeState::load(&dir.path().join("nope.toml"));
        assert_eq!(s, NudgeState::default());
    }

    #[test]
    fn corrupt_file_loads_default_state() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(STATE_FILENAME);
        std::fs::write(&p, "this is { not valid toml ][").unwrap();
        // Must not panic / error — a hook can't fail on bad state.
        assert_eq!(NudgeState::load(&p), NudgeState::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(STATE_FILENAME);
        let mut s = NudgeState {
            edits_since_annotation: 4,
            baseline: Some(Baseline {
                score: 0.42,
                tier: "Adept".into(),
            }),
            ..Default::default()
        };
        s.mark_reviewed("aret_abc12345", "sha256:t", "sha256:b");
        s.proof_reviewed.insert("aret_abc12345".into(), true);
        s.throttle.insert(
            "review_backlog".into(),
            ThrottleRecord {
                last_fired_epoch: 1_700_000_000,
                last_surfaced_metric: 3.0,
            },
        );
        s.save(&p).unwrap();
        assert_eq!(NudgeState::load(&p), s);
    }

    #[test]
    fn unreviewed_counts_absent_unmarked_and_drifted() {
        let mut s = NudgeState::default();
        s.reviewed.insert("reviewed".into(), rec("t1", "b1", true));
        s.reviewed.insert("unmarked".into(), rec("t2", "b2", false));
        s.reviewed.insert("drifted".into(), rec("t3", "b3", true));

        let current = vec![
            ("reviewed", "t1", "b1"),  // reviewed, hashes match → reviewed
            ("unmarked", "t2", "b2"),  // record says reviewed=false → unreviewed
            ("drifted", "tX", "b3"),   // text drifted → unreviewed
            ("brand_new", "t4", "b4"), // no record → unreviewed
        ];
        assert_eq!(s.unreviewed_count(current), 3);
    }

    #[test]
    fn body_drift_after_review_reopens_the_intent() {
        let mut s = NudgeState::default();
        s.mark_reviewed("x", "sha256:t", "sha256:b");
        assert!(
            s.is_reviewed("x", "sha256:t", "sha256:b"),
            "fresh review holds"
        );
        assert!(
            !s.is_reviewed("x", "sha256:t", "sha256:b2"),
            "a body edit after review re-opens it"
        );
    }

    #[test]
    fn empty_reviewed_map_means_everything_unreviewed() {
        let s = NudgeState::default();
        let current = vec![("a", "t", "b"), ("c", "t", "b")];
        assert_eq!(s.unreviewed_count(current), 2);
    }
}
