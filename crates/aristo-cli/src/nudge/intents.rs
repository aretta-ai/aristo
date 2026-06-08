//! Enumerate authored intents for the reviewed/unreviewed axis (Phase 18 #7).
//!
//! One function, [`authored_intents`], is the single source of truth for
//! "what is a reviewable authored intent" — both the engine's `review_backlog`
//! signal (via the union function) and the `aristo review` surface read it, so
//! they can never disagree on the backlog.

use aristo_core::index::{IndexEntry, IndexFile, Status, VerifyLevel};

/// One authored intent, carrying the fields the review surface and the
/// reviewed map need. The hashes are stringified so they line up with the
/// `NudgeState` reviewed map (`id → {text_hash, body_hash}`).
#[derive(Debug, Clone)]
pub struct AuthoredIntent {
    pub id: String,
    pub text: String,
    pub file: String,
    pub site: String,
    pub status: Status,
    pub verify: VerifyLevel,
    pub text_hash: String,
    pub body_hash: String,
}

#[aristo::intent(
    "Authored intents are exactly the `IndexEntry::Intent` entries — every \
     one, including documentation-only `verify = false` intents (still claims \
     the user authored and may want to review). Assumes are excluded: they \
     state external invariants, not reviewable claims. This is the same set \
     the engine's review_backlog metric counts, so `aristo review` and the \
     nudge can never report a different backlog size for the same index.",
    verify = "test",
    id = "authored_intents_are_exactly_the_index_intents"
)]
/// Every authored intent in the index, in id order (the `BTreeMap` iteration
/// order — deterministic).
pub fn authored_intents(index: &IndexFile) -> Vec<AuthoredIntent> {
    index
        .entries
        .iter()
        .filter_map(|(id, entry)| match entry {
            IndexEntry::Intent(e) => Some(AuthoredIntent {
                id: id.as_str().to_string(),
                text: e.text.clone(),
                file: e.file.clone(),
                site: e.site.clone(),
                status: e.status,
                verify: e.verify,
                text_hash: e.text_hash.as_str().to_string(),
                body_hash: e.body_hash.as_str().to_string(),
            }),
            IndexEntry::Assume(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AnnotationId, AnnotationKind, AssumeEntry, BindingState, CoveredRegion, IntentEntry, Meta,
        Sha256, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(text: &str) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: text.into(),
            verify: VerifyLevel::Method(VerifyMethod::Test),
            status: Status::Tested,
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

    fn assume(text: &str) -> IndexEntry {
        IndexEntry::Assume(AssumeEntry {
            text: text.into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn foo".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn index_with(entries: Vec<(&str, IndexEntry)>) -> IndexFile {
        let mut idx = IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: BTreeMap::new(),
        };
        for (id, e) in entries {
            idx.entries.insert(AnnotationId::parse(id).unwrap(), e);
        }
        idx
    }

    #[test]
    fn enumerates_intents_excludes_assumes() {
        let idx = index_with(vec![
            ("i_one", intent("first claim")),
            ("a_one", assume("an external invariant")),
            ("i_two", intent("second claim")),
        ]);
        let got = authored_intents(&idx);
        let ids: Vec<&str> = got.iter().map(|i| i.id.as_str()).collect();
        // Sorted by id (BTreeMap order); the assume is dropped.
        assert_eq!(ids, vec!["i_one", "i_two"]);
    }

    #[test]
    fn carries_text_site_and_hashes() {
        let idx = index_with(vec![("i_one", intent("first claim"))]);
        let got = authored_intents(&idx);
        let one = &got[0];
        assert_eq!(one.text, "first claim");
        assert_eq!(one.site, "fn foo");
        assert_eq!(one.text_hash, sha('a').as_str());
        assert_eq!(one.body_hash, sha('b').as_str());
        assert_eq!(one.status, Status::Tested);
    }

    #[test]
    fn includes_documentation_only_intents() {
        // verify = false (doc-only) intents are still authored claims; they
        // must appear so the review backlog matches the engine's count.
        let mut e = intent("doc only");
        if let IndexEntry::Intent(ref mut ie) = e {
            ie.verify = VerifyLevel::Bool(false);
        }
        let idx = index_with(vec![("i_doc", e)]);
        assert_eq!(authored_intents(&idx).len(), 1);
    }

    // Touch AnnotationKind so the import documents intent-vs-assume scope even
    // though enumeration matches on IndexEntry directly.
    #[test]
    fn annotation_kind_is_intent_or_assume() {
        assert_ne!(AnnotationKind::Intent, AnnotationKind::Assume);
    }
}
