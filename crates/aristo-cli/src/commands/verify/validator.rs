//! Mechanical validator for `.aristo/proofs/<id>.proof` files.
//!
//! The validator gates a status flip on a verdict: only if every check
//! passes does the SDK accept the verdict and update the index entry's
//! status. Failures collect into a single [`ValidatorReport`] (not
//! short-circuit) so the user — or the in-agent repair loop — sees the
//! full list of what needs fixing in one pass.
//!
//! The check list is intentionally bounded and mechanical. There is NO
//! LLM-as-judge in the validator path; the schema enforcement IS the
//! audit story. (Roundtrip semantic checks are deferred — see
//! `../../../../aretta-sdk/docs/verification-research-notes.md`.)

use std::collections::HashSet;
use std::path::Path;

use aristo_core::hash::body_hash;
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, Sha256, Status, VerifyLevel};
use aristo_core::proof::{Gap, Ground, Proof, ProofFile, ProofStep, VerdictBody, Violation};

/// Maximum repair attempts the validator accepts. A verdict with
/// `attempts > MAX_REPAIR_ATTEMPTS` is rejected even if structurally
/// well-formed — the verifier ran out of repair budget and should have
/// converted to inconclusive.
pub(crate) const MAX_REPAIR_ATTEMPTS: u32 = 3;

#[derive(Debug)]
pub(crate) struct ValidatorReport {
    pub failures: Vec<ValidationFailure>,
}

impl ValidatorReport {
    fn empty() -> Self {
        Self {
            failures: Vec::new(),
        }
    }

    fn push(&mut self, location: String, detail: String) {
        self.failures.push(ValidationFailure { location, detail });
    }

    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }

    /// Render as a multi-line human-readable message for the CLI error
    /// stream. Each failure on its own line, prefixed with the location
    /// in the proof file.
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "verdict rejected ({} check(s) failed):",
            self.failures.len()
        );
        for f in &self.failures {
            let _ = writeln!(out, "  • {}: {}", f.location, f.detail);
        }
        out
    }
}

#[derive(Debug)]
pub(crate) struct ValidationFailure {
    pub location: String,
    pub detail: String,
}

#[aristo::intent(
    "The validator collects EVERY failure into one report rather than \
     short-circuiting on the first. The user (or in-agent repair loop) \
     needs the complete list to fix in one pass; short-circuiting \
     forces N round-trips for N failures, which doesn't compose with \
     the bounded-attempts budget (the verifier would burn its budget \
     fixing failures one at a time).",
    verify = "test",
    id = "validator_collects_all_failures_not_short_circuit"
)]
pub(crate) fn validate(
    focal_id: &AnnotationId,
    pf: &ProofFile,
    index: &IndexFile,
    workspace_root: &Path,
) -> ValidatorReport {
    let mut r = ValidatorReport::empty();

    // ─── verdict meta ───
    check_attempts_budget(&mut r, pf);
    check_focal_entry_hashes(&mut r, focal_id, pf, index);
    check_method_matches(&mut r, focal_id, pf, index);

    // ─── body shape ───
    let body = match pf.body() {
        Ok(b) => b,
        Err(e) => {
            r.push("verdict".into(), format!("body shape: {e}"));
            return r; // No body → no point checking step-level invariants.
        }
    };

    // ─── per-body-type checks ───
    match body {
        VerdictBody::Verified(v) => {
            check_verified_proof_nonempty(&mut r, &v.proof);
            check_proof_tree(&mut r, "verified.proof", &v.proof.steps);
            check_all_grounds(
                &mut r,
                "verified.proof",
                &v.proof.steps,
                index,
                workspace_root,
            );
        }
        VerdictBody::Counterexample(c) => {
            check_counterexample(&mut r, &c.violation);
            check_proof_tree(
                &mut r,
                "counterexample.violation.trigger_steps",
                &c.violation.trigger_steps,
            );
            check_all_grounds(
                &mut r,
                "counterexample.violation.trigger_steps",
                &c.violation.trigger_steps,
                index,
                workspace_root,
            );
        }
        VerdictBody::Inconclusive(i) => {
            check_gap(&mut r, &i.gap);
            if let Some(p) = &i.partial_proof {
                check_proof_tree(&mut r, "inconclusive.partial_proof", &p.steps);
                check_all_grounds(
                    &mut r,
                    "inconclusive.partial_proof",
                    &p.steps,
                    index,
                    workspace_root,
                );
            }
        }
    }

    r
}

// ─── verdict-meta checks ──────────────────────────────────────────────────

fn check_attempts_budget(r: &mut ValidatorReport, pf: &ProofFile) {
    if pf.verdict.attempts == 0 {
        r.push(
            "verdict.attempts".into(),
            "attempts is 0; first attempt is 1".into(),
        );
    }
    if pf.verdict.attempts > MAX_REPAIR_ATTEMPTS {
        r.push(
            "verdict.attempts".into(),
            format!(
                "attempts={} exceeds budget {}; convert to inconclusive instead of \
                 continuing to retry past the cap",
                pf.verdict.attempts, MAX_REPAIR_ATTEMPTS
            ),
        );
    }
}

fn check_focal_entry_hashes(
    r: &mut ValidatorReport,
    focal_id: &AnnotationId,
    pf: &ProofFile,
    index: &IndexFile,
) {
    let Some(entry) = index.entries.get(focal_id) else {
        r.push(
            format!("focal entry `{focal_id}`"),
            "not found in current index".into(),
        );
        return;
    };
    let (text_h, body_h) = entry_hashes(entry);
    if pf.verdict.produced_at_text_hash != text_h {
        r.push(
            "verdict.produced_at_text_hash".into(),
            "differs from current index text_hash; entry text drifted since verification".into(),
        );
    }
    if pf.verdict.produced_at_body_hash != body_h {
        r.push(
            "verdict.produced_at_body_hash".into(),
            "differs from current index body_hash; entry code drifted since verification".into(),
        );
    }
}

fn check_method_matches(
    r: &mut ValidatorReport,
    focal_id: &AnnotationId,
    pf: &ProofFile,
    index: &IndexFile,
) {
    let Some(entry) = index.entries.get(focal_id) else {
        return; // already reported by check_focal_entry_hashes
    };
    let entry_method = match entry_verify(entry) {
        VerifyLevel::Method(m) => m,
        VerifyLevel::Bool(_) => {
            r.push(
                "verdict.method".into(),
                "focal entry has verify=true/false; only verify=neural/test/full \
                 entries accept a method-bound proof"
                    .into(),
            );
            return;
        }
    };
    if pf.verdict.method != entry_method {
        r.push(
            "verdict.method".into(),
            format!(
                "verdict claims method `{:?}` but focal entry's resolved method is `{:?}`",
                pf.verdict.method, entry_method
            ),
        );
    }
}

// ─── verified-body checks ─────────────────────────────────────────────────

fn check_verified_proof_nonempty(r: &mut ValidatorReport, p: &Proof) {
    if p.conclusion.trim().is_empty() {
        r.push("verified.proof.conclusion".into(), "is empty".into());
    }
    if p.steps.is_empty() {
        r.push(
            "verified.proof.steps".into(),
            "is empty; a verified verdict needs at least one proof step".into(),
        );
    }
}

// ─── counterexample-body checks ───────────────────────────────────────────

fn check_counterexample(r: &mut ValidatorReport, v: &Violation) {
    if v.description.trim().is_empty() {
        r.push(
            "counterexample.violation.description".into(),
            "is empty".into(),
        );
    }
    if v.violated_step_path.trim().is_empty() {
        r.push(
            "counterexample.violation.violated_step_path".into(),
            "is empty; must point at the proof step the trigger refutes".into(),
        );
    }
    if v.trigger_steps.is_empty() {
        r.push(
            "counterexample.violation.trigger_steps".into(),
            "is empty; counterexamples must demonstrate the violating trace concretely".into(),
        );
    }
}

// ─── inconclusive-body checks ─────────────────────────────────────────────

fn check_gap(r: &mut ValidatorReport, g: &Gap) {
    if g.description.trim().is_empty() {
        r.push("inconclusive.gap.description".into(), "is empty".into());
    }
    if g.unfilled_path.trim().is_empty() {
        r.push(
            "inconclusive.gap.unfilled_path".into(),
            "is empty; must identify which subgoal could not be discharged".into(),
        );
    }
    if g.suggested_annotations.is_empty() {
        r.push(
            "inconclusive.gap.suggested_annotations".into(),
            "is empty; inconclusive verdicts MUST suggest at least one annotation \
             that would close the gap — bare inconclusive without actionable suggestion \
             is no better than 'I don't know'"
                .into(),
        );
    }
}

// ─── proof-tree structural checks ─────────────────────────────────────────

fn check_proof_tree(r: &mut ValidatorReport, prefix: &str, steps: &[ProofStep]) {
    if steps.is_empty() {
        return; // verified-empty / counterexample-empty already reported
    }

    // Path uniqueness.
    let mut seen = HashSet::new();
    for s in steps {
        if !seen.insert(&s.path) {
            r.push(
                format!("{prefix}.steps[{}]", s.path),
                "duplicate path".into(),
            );
        }
    }

    // Root must be "0".
    if !steps.iter().any(|s| s.path == "0") {
        r.push(
            format!("{prefix}.steps"),
            "no step with path \"0\" — tree must be rooted at \"0\"".into(),
        );
    }

    // subgoal_paths cross-references must resolve to actual steps.
    for s in steps {
        for sp in &s.subgoal_paths {
            if !steps.iter().any(|x| &x.path == sp) {
                r.push(
                    format!("{prefix}.steps[{}].subgoal_paths", s.path),
                    format!("`{sp}` does not resolve to any step in this tree"),
                );
            } else if !sp.starts_with(&format!("{}.", s.path)) {
                r.push(
                    format!("{prefix}.steps[{}].subgoal_paths", s.path),
                    format!(
                        "`{sp}` is not a descendant of `{}` (subgoal paths must extend the parent's path)",
                        s.path
                    ),
                );
            }
        }
    }

    // Every step has ≥1 ground.
    for s in steps {
        if s.grounds.is_empty() {
            r.push(
                format!("{prefix}.steps[{}].grounds", s.path),
                "step has zero grounds; every claim needs at least one ground (no 'trust me' steps)"
                    .into(),
            );
        }
    }
}

// ─── ground checks ────────────────────────────────────────────────────────

fn check_all_grounds(
    r: &mut ValidatorReport,
    prefix: &str,
    steps: &[ProofStep],
    index: &IndexFile,
    workspace_root: &Path,
) {
    let step_paths: HashSet<&str> = steps.iter().map(|s| s.path.as_str()).collect();
    for s in steps {
        for (i, g) in s.grounds.iter().enumerate() {
            check_one_ground(
                r,
                &format!("{prefix}.steps[{}].grounds[{}]", s.path, i),
                g,
                &s.path,
                &step_paths,
                index,
                workspace_root,
            );
        }
    }
}

fn check_one_ground(
    r: &mut ValidatorReport,
    location: &str,
    g: &Ground,
    owning_path: &str,
    step_paths: &HashSet<&str>,
    index: &IndexFile,
    workspace_root: &Path,
) {
    match g {
        Ground::Intent {
            id, at_text_hash, ..
        }
        | Ground::Assume {
            id, at_text_hash, ..
        } => {
            check_index_ground(r, location, id, at_text_hash, g, index);
        }
        Ground::Code {
            file,
            lines,
            code_text_hash,
            ..
        } => {
            check_code_ground(r, location, file, lines, code_text_hash, workspace_root);
        }
        Ground::PriorStep { path, .. } => {
            check_prior_step_ground(r, location, owning_path, path, step_paths);
        }
        Ground::Composition { reason } => {
            if reason.trim().is_empty() {
                r.push(
                    location.into(),
                    "composition ground has empty reason".into(),
                );
            }
        }
    }
}

#[aristo::intent(
    "Cited intent/assume grounds are rejected when (a) the id is \
     dangling, (b) the cited entry is verify=false (docs-only — not \
     load-bearing for a proof), or (c) the cited entry is Status::\
     Counterexample (refuted — building on it would launder a refuted \
     claim back into Verified). A refactor that downgrades any of \
     these to warnings would let proofs ground in claims the project \
     doesn't actually believe.",
    verify = "test",
    id = "validator_rejects_grounding_in_refuted_or_docs_only"
)]
fn check_index_ground(
    r: &mut ValidatorReport,
    location: &str,
    id: &AnnotationId,
    at_text_hash: &Sha256,
    g: &Ground,
    index: &IndexFile,
) {
    let Some(entry) = index.entries.get(id) else {
        r.push(
            location.into(),
            format!("cited id `{id}` not found in current index"),
        );
        return;
    };
    let (cur_text_hash, _) = entry_hashes(entry);
    if at_text_hash != &cur_text_hash {
        r.push(
            location.into(),
            format!(
                "at_text_hash for cited `{id}` differs from current; cited entry's \
                 text changed since verification — re-check whether the new wording \
                 still supports this step"
            ),
        );
    }
    // For Intent grounds: ensure the cited entry isn't refuted or docs-only.
    if matches!(g, Ground::Intent { .. }) {
        if let VerifyLevel::Bool(false) = entry_verify(entry) {
            r.push(
                location.into(),
                format!(
                    "cited intent `{id}` is verify=false (documentation only); cannot \
                     ground a proof in a docs-only claim"
                ),
            );
        }
        if entry_status(entry) == Status::Counterexample {
            r.push(
                location.into(),
                format!(
                    "cited intent `{id}` has status=counterexample (refuted); cannot \
                     ground a proof in a refuted claim"
                ),
            );
        }
    }
}

fn check_code_ground(
    r: &mut ValidatorReport,
    location: &str,
    file: &str,
    lines: &str,
    code_text_hash: &Sha256,
    workspace_root: &Path,
) {
    let path = workspace_root.join(file);
    let Ok(source) = std::fs::read_to_string(&path) else {
        r.push(
            location.into(),
            format!("code file `{file}` does not exist under workspace root"),
        );
        return;
    };
    let Some((lo, hi)) = parse_line_range(lines) else {
        r.push(
            location.into(),
            format!("lines `{lines}` is not a valid range (expected `N` or `N-M` with N ≤ M)"),
        );
        return;
    };
    let total_lines = source.lines().count();
    if hi > total_lines {
        r.push(
            location.into(),
            format!("line range `{lines}` exceeds file length {total_lines}"),
        );
        return;
    }
    // Hash the slice and compare. We hash the substring `lines[lo-1..=hi-1]` joined with '\n'.
    let slice: String = source
        .lines()
        .skip(lo - 1)
        .take(hi - lo + 1)
        .collect::<Vec<_>>()
        .join("\n");
    let actual = body_hash(&slice);
    if &actual != code_text_hash {
        r.push(
            location.into(),
            format!(
                "code_text_hash for `{file}:{lines}` differs from current file content; \
                 the cited code drifted since verification"
            ),
        );
    }
}

fn check_prior_step_ground(
    r: &mut ValidatorReport,
    location: &str,
    owning_path: &str,
    cited_path: &str,
    step_paths: &HashSet<&str>,
) {
    if !step_paths.contains(cited_path) {
        r.push(
            location.into(),
            format!("prior-step `{cited_path}` does not resolve to any step in this tree"),
        );
        return;
    }
    // Prior-step must be lexicographically earlier than owning path so the
    // proof DAG is acyclic. We approximate "earlier" with string compare on
    // the dotted path — works because step indices are integers ≤ 9 in
    // practice (verifier discipline). For ≥10-child nodes a richer
    // comparison would be needed; revisit if it bites.
    if cited_path >= owning_path {
        r.push(
            location.into(),
            format!(
                "prior-step `{cited_path}` is not earlier than owning step `{owning_path}`; \
                 prior-step grounds must reference EARLIER steps only (no cycles)"
            ),
        );
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────

fn entry_hashes(entry: &IndexEntry) -> (Sha256, Sha256) {
    match entry {
        IndexEntry::Intent(e) => (e.text_hash.clone(), e.body_hash.clone()),
        IndexEntry::Assume(e) => (e.text_hash.clone(), e.body_hash.clone()),
    }
}

fn entry_verify(entry: &IndexEntry) -> VerifyLevel {
    match entry {
        IndexEntry::Intent(e) => e.verify,
        IndexEntry::Assume(_) => VerifyLevel::Bool(false),
    }
}

fn entry_status(entry: &IndexEntry) -> Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

fn parse_line_range(s: &str) -> Option<(usize, usize)> {
    match s.split_once('-') {
        Some((a, b)) => {
            let lo: usize = a.parse().ok()?;
            let hi: usize = b.parse().ok()?;
            if lo == 0 || hi < lo {
                None
            } else {
                Some((lo, hi))
            }
        }
        None => {
            let n: usize = s.parse().ok()?;
            if n == 0 {
                None
            } else {
                Some((n, n))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::hash::text_hash;
    use aristo_core::index::{IntentEntry, Meta, VerifyMethod};
    use aristo_core::proof::{
        Ground, GroundRelation, PropertyKind, RelationKind, VerdictMeta, VerdictType, VerifiedBody,
    };
    use std::collections::BTreeMap;

    fn sample_id(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    fn sample_hash(s: &str) -> Sha256 {
        text_hash(s)
    }

    fn index_with_one_intent(id: &str, text: &str, status: Status) -> IndexFile {
        let mut entries = BTreeMap::new();
        entries.insert(
            sample_id(id),
            IndexEntry::Intent(IntentEntry {
                text: text.into(),
                verify: VerifyLevel::Method(VerifyMethod::Neural),
                status,
                text_hash: text_hash(text),
                body_hash: sample_hash("body"),
                file: "src/x.rs".into(),
                site: "fn x (line 1)".into(),
                covered_region: aristo_core::index::CoveredRegion::Function,
                binding: aristo_core::index::BindingState::Local,
                parent: None,
            }),
        );
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries,
        }
    }

    fn minimal_verified(focal_text: &str, focal_body: &str) -> ProofFile {
        ProofFile {
            verdict: VerdictMeta {
                r#type: VerdictType::Verified,
                method: VerifyMethod::Neural,
                produced_at_text_hash: text_hash(focal_text),
                produced_at_body_hash: sample_hash(focal_body),
                produced_by: "test@0".into(),
                verifier_model: None,
                attempts: 1,
                property_kind: PropertyKind::Invariant,
            },
            verified: Some(VerifiedBody {
                proof: Proof {
                    conclusion: "the property holds".into(),
                    steps: vec![ProofStep {
                        path: "0".into(),
                        claim: "by construction".into(),
                        relation_to_parent: RelationKind::Decomposes,
                        grounds: vec![Ground::Composition {
                            reason: "trivial".into(),
                        }],
                        subgoal_paths: vec![],
                        proposed_promotion: false,
                    }],
                },
            }),
            counterexample: None,
            inconclusive: None,
        }
    }

    #[test]
    fn empty_failure_list_renders_as_zero_failed() {
        let r = ValidatorReport::empty();
        assert!(r.is_empty());
    }

    #[test]
    fn minimal_valid_verdict_passes() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let pf = minimal_verified("the property", "body");
        let tmp = tempfile::tempdir().unwrap();
        let report = validate(&sample_id("foo"), &pf, &idx, tmp.path());
        assert!(report.is_empty(), "expected pass; got: {}", report.render());
    }

    #[test]
    fn rejects_zero_attempts() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("the property", "body");
        pf.verdict.attempts = 0;
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.location == "verdict.attempts"));
    }

    #[test]
    fn rejects_attempts_over_budget() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("the property", "body");
        pf.verdict.attempts = MAX_REPAIR_ATTEMPTS + 1;
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| { f.location == "verdict.attempts" && f.detail.contains("exceeds budget") }));
    }

    #[test]
    fn rejects_stale_focal_text_hash() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("DIFFERENT TEXT", "body");
        pf.verdict.attempts = 1;
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.location == "verdict.produced_at_text_hash"));
    }

    #[test]
    fn rejects_dangling_focal_id() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let pf = minimal_verified("anything", "body");
        let report = validate(&sample_id("bar"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.location == "focal entry `bar`"));
    }

    #[test]
    fn rejects_intent_ground_citing_dangling_id() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("the property", "body");
        pf.verified.as_mut().unwrap().proof.steps[0].grounds = vec![Ground::Intent {
            id: sample_id("nonexistent"),
            at_text_hash: sample_hash("x"),
            relation: GroundRelation::Instantiates,
            reason: None,
        }];
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.detail.contains("not found in current index")));
    }

    #[test]
    fn rejects_intent_ground_with_stale_text_hash() {
        let idx = index_with_one_intent("ground", "current text", Status::Unknown);
        let mut idx = idx;
        // Add focal entry too
        idx.entries.insert(
            sample_id("focal"),
            IndexEntry::Intent(IntentEntry {
                text: "focal text".into(),
                verify: VerifyLevel::Method(VerifyMethod::Neural),
                status: Status::Unknown,
                text_hash: text_hash("focal text"),
                body_hash: sample_hash("focal body"),
                file: "src/x.rs".into(),
                site: "fn focal (line 1)".into(),
                covered_region: aristo_core::index::CoveredRegion::Function,
                binding: aristo_core::index::BindingState::Local,
                parent: None,
            }),
        );
        let mut pf = minimal_verified("focal text", "focal body");
        pf.verified.as_mut().unwrap().proof.steps[0].grounds = vec![Ground::Intent {
            id: sample_id("ground"),
            at_text_hash: text_hash("STALE TEXT"),
            relation: GroundRelation::Instantiates,
            reason: None,
        }];
        let report = validate(&sample_id("focal"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.detail.contains("differs from current")));
    }

    #[test]
    fn rejects_grounding_in_counterexample_status() {
        let idx = index_with_one_intent("ground", "current text", Status::Counterexample);
        let mut idx = idx;
        idx.entries.insert(
            sample_id("focal"),
            IndexEntry::Intent(IntentEntry {
                text: "focal text".into(),
                verify: VerifyLevel::Method(VerifyMethod::Neural),
                status: Status::Unknown,
                text_hash: text_hash("focal text"),
                body_hash: sample_hash("focal body"),
                file: "src/x.rs".into(),
                site: "fn focal (line 1)".into(),
                covered_region: aristo_core::index::CoveredRegion::Function,
                binding: aristo_core::index::BindingState::Local,
                parent: None,
            }),
        );
        let mut pf = minimal_verified("focal text", "focal body");
        pf.verified.as_mut().unwrap().proof.steps[0].grounds = vec![Ground::Intent {
            id: sample_id("ground"),
            at_text_hash: text_hash("current text"),
            relation: GroundRelation::Instantiates,
            reason: None,
        }];
        let report = validate(&sample_id("focal"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.detail.contains("status=counterexample")));
    }

    #[test]
    fn rejects_empty_grounds_step() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("the property", "body");
        pf.verified.as_mut().unwrap().proof.steps[0].grounds = vec![];
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.detail.contains("zero grounds")));
    }

    #[test]
    fn rejects_prior_step_pointing_at_later_step() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("the property", "body");
        pf.verified.as_mut().unwrap().proof.steps = vec![
            ProofStep {
                path: "0".into(),
                claim: "root".into(),
                relation_to_parent: RelationKind::Decomposes,
                grounds: vec![Ground::PriorStep {
                    path: "1".into(),
                    reason: None,
                }],
                subgoal_paths: vec![],
                proposed_promotion: false,
            },
            ProofStep {
                path: "1".into(),
                claim: "later".into(),
                relation_to_parent: RelationKind::Decomposes,
                grounds: vec![Ground::Composition {
                    reason: "trivial".into(),
                }],
                subgoal_paths: vec![],
                proposed_promotion: false,
            },
        ];
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.detail.contains("not earlier than")));
    }

    #[test]
    fn rejects_inconclusive_without_suggestions() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("the property", "body");
        pf.verified = None;
        pf.inconclusive = Some(aristo_core::proof::InconclusiveBody {
            partial_proof: None,
            gap: Gap {
                description: "stuck".into(),
                unfilled_path: "0.1".into(),
                suggested_annotations: vec![],
            },
        });
        pf.verdict.r#type = VerdictType::Inconclusive;
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        assert!(report
            .failures
            .iter()
            .any(|f| f.detail.contains("MUST suggest at least one annotation")));
    }

    #[test]
    fn collects_multiple_failures_not_short_circuit() {
        let idx = index_with_one_intent("foo", "the property", Status::Unknown);
        let mut pf = minimal_verified("WRONG TEXT", "body");
        pf.verdict.attempts = 99; // also over budget
        pf.verified.as_mut().unwrap().proof.steps[0].grounds = vec![]; // empty grounds
        let report = validate(&sample_id("foo"), &pf, &idx, Path::new("/"));
        // Expect at least: stale text hash + attempts over budget + empty grounds
        assert!(
            report.failures.len() >= 3,
            "expected ≥3 failures collected; got {} — render: {}",
            report.failures.len(),
            report.render()
        );
    }
}
