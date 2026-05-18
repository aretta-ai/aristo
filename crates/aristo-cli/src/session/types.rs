//! Core types for the review-session substrate.
//!
//! These mirror the lifecycle file shape documented in
//! `docs/decisions/review-sessions.md`. The types are intentionally
//! pure data — read/write logic lives in sibling submodules
//! (`storage`, `pointer`, etc.) so the types stay easy to construct in
//! tests.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Unique session identifier. ULID format (Crockford base32, 26 chars,
/// time-orderable). Sorting session files by name = sorting by creation
/// time, which gives `ls` and `aristo session list` natural ordering
/// without an extra index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// Wrap a pre-generated id string. Use [`SessionId::new`] to mint a
    /// fresh ULID instead.
    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SessionId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Opaque per-kind item reference. Encoded as `<id>#<index>` (e.g.
/// `critique_queue_entries_are_self_contained#0`). The `#` separator
/// avoids ambiguity with annotation ids that contain `:`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemRef(String);

impl ItemRef {
    /// Build a ref from an id + index. The encoding is load-bearing —
    /// see the type-level rationale for `#`.
    #[aristo::intent(
        "ItemRef uses `#` as the id↔index separator rather than `:` \
         because annotation ids in this project can legitimately \
         contain `:` (e.g. `aristos:foo`). A refactor that switches to \
         `:` would silently break ref parsing the moment any session \
         touched an `aristos:`-namespaced id; `#` is safe because it's \
         a reserved character in annotation ids by design.",
        verify = "neural",
        id = "item_ref_separator_is_hash_not_colon"
    )]
    pub fn new(id: &str, index: usize) -> Self {
        Self(format!("{id}#{index}"))
    }

    /// Build a ref from a free-form opaque string (e.g. a per-kind
    /// non-indexed ref like a proof verdict).
    pub fn from_opaque(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse out `(id, index)` from a `<id>#<index>` ref. Returns `None`
    /// if the ref has no `#` or the suffix doesn't parse as usize —
    /// callers handling kinds with opaque refs use [`as_str`] instead.
    pub fn split_indexed(&self) -> Option<(&str, usize)> {
        let (id, idx) = self.0.rsplit_once('#')?;
        let idx = idx.parse().ok()?;
        Some((id, idx))
    }
}

impl fmt::Display for ItemRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What an individual item's triage state is. `Open` is the implicit
/// "user hasn't decided yet" state that forces explicit action before
/// the session can close strictly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemStatus {
    Open,
    Accepted,
    Rejected,
    Pending,
}

/// Overall session lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    Active,
    Closed,
    Aborted,
}

/// How a session was closed. Recorded on the closed-session file for
/// audit. `Exit` requires every item decided; `ExitDeferUndecided`
/// moves the still-open items to the backlog (never silently drops);
/// `Abort` is the destructive escape hatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExitKind {
    Exit,
    ExitDeferUndecided,
    Abort,
}

/// Whether other-kind sessions can nest inside this one. v0 ships
/// `Disallow` only; the design parks per-kind allow-lists as an open
/// question (Q4) until a concrete use case demands them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NestingPolicy {
    Disallow,
}

/// One reviewable item within a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    #[serde(rename = "ref")]
    pub item_ref: ItemRef,
    pub status: ItemStatus,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub closed_at: Option<String>,
}

impl Item {
    /// New open item with no decision attached.
    pub fn open(item_ref: ItemRef) -> Self {
        Self {
            item_ref,
            status: ItemStatus::Open,
            note: None,
            closed_at: None,
        }
    }
}

/// On-disk lifecycle file shape. Serializes to TOML matching the
/// schema in `docs/decisions/review-sessions.md` (lifecycle file
/// shape section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub schema_version: u32,
    pub id: SessionId,
    pub kind: String,
    pub subject: String,
    pub started_at: String,
    pub started_by: String,
    pub nesting_policy: NestingPolicy,
    pub state: SessionState,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub closed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_kind: Option<ExitKind>,
}

impl Session {
    /// Bucket counts across items. Used by `aristo session status`,
    /// the hook reminder, and exit-strictness checks.
    pub fn bucket_counts(&self) -> BucketCounts {
        let mut counts = BucketCounts::default();
        for item in &self.items {
            match item.status {
                ItemStatus::Open => counts.open += 1,
                ItemStatus::Accepted => counts.accepted += 1,
                ItemStatus::Rejected => counts.rejected += 1,
                ItemStatus::Pending => counts.pending += 1,
            }
        }
        counts
    }
}

/// Summary of bucket sizes across a session's items.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BucketCounts {
    pub open: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_ref_encodes_id_and_index_with_hash() {
        let r = ItemRef::new("foo_bar", 3);
        assert_eq!(r.as_str(), "foo_bar#3");
        let (id, idx) = r.split_indexed().unwrap();
        assert_eq!(id, "foo_bar");
        assert_eq!(idx, 3);
    }

    #[test]
    fn item_ref_handles_colon_in_annotation_id() {
        // The whole reason we use `#` instead of `:`. An aristos:-prefixed
        // id must round-trip through the ref encoding.
        let r = ItemRef::new("aristos:foo", 7);
        assert_eq!(r.as_str(), "aristos:foo#7");
        let (id, idx) = r.split_indexed().unwrap();
        assert_eq!(id, "aristos:foo");
        assert_eq!(idx, 7);
    }

    #[test]
    fn item_ref_split_returns_none_for_opaque() {
        let r = ItemRef::from_opaque("verdict");
        assert!(r.split_indexed().is_none());
    }

    #[test]
    fn item_status_serializes_kebab_case() {
        let s = serde_json::to_string(&ItemStatus::Accepted).unwrap();
        assert_eq!(s, "\"accepted\"");
        // The trickier one: multi-word variant.
        let exit = serde_json::to_string(&ExitKind::ExitDeferUndecided).unwrap();
        assert_eq!(exit, "\"exit-defer-undecided\"");
    }

    #[test]
    fn session_round_trips_through_toml() {
        let s = Session {
            schema_version: 1,
            id: SessionId::from_string("01J5K9N7CRITIQUEREVIEW00000".into()),
            kind: "critique-review".into(),
            subject: "src/critique/pending.rs".into(),
            started_at: "2026-05-18T13:00:00Z".into(),
            started_by: "aristo-critique skill".into(),
            nesting_policy: NestingPolicy::Disallow,
            state: SessionState::Active,
            items: vec![
                Item::open(ItemRef::new("foo", 0)),
                Item {
                    item_ref: ItemRef::new("foo", 1),
                    status: ItemStatus::Accepted,
                    note: Some("will tighten next commit".into()),
                    closed_at: Some("2026-05-18T13:05:23Z".into()),
                },
            ],
            closed_at: None,
            exit_kind: None,
        };

        let text = toml::to_string(&s).unwrap();
        let parsed: Session = toml::from_str(&text).unwrap();
        assert_eq!(parsed, s);
    }

    #[test]
    fn bucket_counts_partition_items() {
        let mk = |st| Item {
            item_ref: ItemRef::new("x", 0),
            status: st,
            note: None,
            closed_at: None,
        };
        let s = Session {
            schema_version: 1,
            id: SessionId::from_string("test".into()),
            kind: "critique-review".into(),
            subject: String::new(),
            started_at: String::new(),
            started_by: String::new(),
            nesting_policy: NestingPolicy::Disallow,
            state: SessionState::Active,
            items: vec![
                mk(ItemStatus::Open),
                mk(ItemStatus::Open),
                mk(ItemStatus::Accepted),
                mk(ItemStatus::Rejected),
                mk(ItemStatus::Pending),
                mk(ItemStatus::Pending),
            ],
            closed_at: None,
            exit_kind: None,
        };
        let counts = s.bucket_counts();
        assert_eq!(counts.open, 2);
        assert_eq!(counts.accepted, 1);
        assert_eq!(counts.rejected, 1);
        assert_eq!(counts.pending, 2);
    }
}
