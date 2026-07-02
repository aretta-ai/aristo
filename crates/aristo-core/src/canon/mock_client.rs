//! [`MockCanonClient`] — fixture-driven canon client for tests.
//!
//! Reads canned responses from a fixture directory and returns them
//! verbatim. Used by trycmd scenarios and integration tests to
//! exercise the SDK end-to-end without a real server.
//!
//! ## Fixture layout
//!
//! Each fixture file is TOML and corresponds to one endpoint response.
//! The mock client looks up files by a deterministic naming scheme
//! against the request shape:
//!
//! ```text
//! <fixture_dir>/
//!   match.toml                        ← canned POST /canon/match response
//!   entry/<canon_id>/<version>.toml   ← canned GET /canon/entry/<id>?version=<v>
//!   entry/<canon_id>/active.toml      ← canned response when version unspecified
//!   request-verify.toml               ← canned POST /canon/request-verify response
//! ```
//!
//! Multi-fixture scenarios (one stamp + several canon shows) point
//! `ARISTO_CANON_FIXTURE` at the same directory; each endpoint's
//! response is independent.
//!
//! ## When the fixture file is missing
//!
//! The mock returns [`CanonError::Fixture`] with the expected path —
//! tests fail loudly with a "you forgot to add the fixture" message
//! rather than silently no-op-ing.
//!
//! ## Test-only scope
//!
//! This client is shipped in the library (not `#[cfg(test)]`) so
//! integration tests in `aristo-cli` can construct it. Production
//! callers must never wire this up; the `MockCanonClient::from_env`
//! constructor reads `ARISTO_CANON_FIXTURE` to make it deliberately
//! awkward for accidental production use.

use std::fs;
use std::path::{Path, PathBuf};

use super::client::{CanonClient, CanonError};
use super::types::{
    CanonCatalogue, CanonEntry, CanonMatchRequest, CanonMatchResponse, RequestVerifyBody,
    RequestVerifyResponse,
};

/// Fixture-backed canon client.
///
/// Reads canned TOML responses from `fixture_dir` per the layout
/// documented at the module level. Construction is via
/// [`MockCanonClient::new`] (explicit path) or
/// [`MockCanonClient::from_env`] (`ARISTO_CANON_FIXTURE` env var).
#[derive(Debug, Clone)]
pub struct MockCanonClient {
    fixture_dir: PathBuf,
}

impl MockCanonClient {
    /// Construct a mock client reading fixtures from `fixture_dir`.
    pub fn new(fixture_dir: PathBuf) -> Self {
        Self { fixture_dir }
    }

    /// Construct a mock client from the `ARISTO_CANON_FIXTURE` env
    /// var, or `None` if it's unset. Production callers must never
    /// reach this path — env-var-gating makes accidental use loud.
    pub fn from_env() -> Option<Self> {
        let dir = std::env::var("ARISTO_CANON_FIXTURE").ok()?;
        Some(Self::new(PathBuf::from(dir)))
    }

    /// Read and deserialize a fixture file. Returns
    /// [`CanonError::Fixture`] if the file is missing or unparseable.
    fn load<T: for<'de> serde::Deserialize<'de>>(&self, rel: &Path) -> Result<T, CanonError> {
        let path = self.fixture_dir.join(rel);
        let raw = fs::read_to_string(&path)
            .map_err(|e| CanonError::Fixture(format!("read fixture {}: {e}", path.display())))?;
        toml::from_str(&raw)
            .map_err(|e| CanonError::Fixture(format!("parse fixture {}: {e}", path.display())))
    }
}

impl CanonClient for MockCanonClient {
    fn match_annotations(
        &self,
        _req: &CanonMatchRequest,
    ) -> Result<CanonMatchResponse, CanonError> {
        // Single canned response per fixture dir. Real-world fixtures
        // typically construct one match response covering the
        // annotations the test exercises; the SDK's response handling
        // doesn't care that the same response gets returned for
        // different requests because trycmd scenarios run one stamp
        // call per fixture dir.
        self.load(Path::new("match.toml"))
    }

    fn get_entry(&self, canon_id: &str, version: Option<&str>) -> Result<CanonEntry, CanonError> {
        let file_stem = version.unwrap_or("active");
        let rel = PathBuf::from("entry")
            .join(canon_id)
            .join(format!("{file_stem}.toml"));
        self.load(&rel)
    }

    fn request_verify(
        &self,
        _body: &RequestVerifyBody,
    ) -> Result<RequestVerifyResponse, CanonError> {
        self.load(Path::new("request-verify.toml"))
    }

    fn catalogue(&self) -> Result<CanonCatalogue, CanonError> {
        self.load(Path::new("catalogue.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::types::{AnnotationMatchInput, CanonMatch, PrefixTier, VerificationMetadata};
    use tempfile::TempDir;

    fn write_match_fixture(dir: &Path, body: &str) {
        fs::write(dir.join("match.toml"), body).unwrap();
    }

    fn write_entry_fixture(dir: &Path, canon_id: &str, version: &str, body: &str) {
        let entry_dir = dir.join("entry").join(canon_id);
        fs::create_dir_all(&entry_dir).unwrap();
        fs::write(entry_dir.join(format!("{version}.toml")), body).unwrap();
    }

    #[test]
    fn match_annotations_loads_handwritten_fixture() {
        // Hand-written TOML to assert that the documented fixture
        // shape (what scenario authors will write by hand) actually
        // deserializes. The round-trip test below covers the
        // symmetrical Rust-serialize-then-load case.
        //
        // TOML's `results = Vec<Vec<CanonMatch>>` shape is expressed
        // via array-of-inline-arrays-of-inline-tables — TOML doesn't
        // have a direct nested-array-of-tables block syntax, so
        // each match candidate is one long inline table on a line.
        // Real-world fixtures (under tests/fixtures/canon/) typically
        // have one or two matches, so this stays manageable.
        let tmp = TempDir::new().unwrap();
        let fixture = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4", verification = { coverage_level = "tight", test_binaries = ["monotonicity_property"] } }
    ]
]
"#;
        write_match_fixture(tmp.path(), fixture);

        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let req = CanonMatchRequest {
            annotations: vec![AnnotationMatchInput {
                annotation_text: "test".into(),
                applies_to: vec!["fn".into()],
            }],
            confidence_threshold: 0.85,
            include_suggestions: false,
        };
        let resp = client.match_annotations(&req).unwrap();
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].len(), 1);
        let m: &CanonMatch = &resp.results[0][0];
        assert_eq!(m.canon_id, "cell_written_exactly_once_per_page_edit");
        assert_eq!(m.version, "v0.2.1");
        assert_eq!(m.prefix_tier, PrefixTier::Aristos);
        assert_eq!(m.backed_by.as_deref(), Some("specialized neural checker"));
        assert_eq!(m.scope, ":vanilla");
        assert_eq!(resp.effective_scopes, vec![":vanilla".to_string()]);
        assert_eq!(resp.canon_version, "v0.2.0");
    }

    #[test]
    fn match_annotations_missing_fixture_surfaces_fixture_error() {
        let tmp = TempDir::new().unwrap();
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let req = CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.5,
            include_suggestions: false,
        };
        let err = client.match_annotations(&req).unwrap_err();
        assert!(matches!(err, CanonError::Fixture(_)));
        // Message should include the path so test failures point at
        // the missing file.
        assert!(err.to_string().contains("match.toml"), "got: {err}");
    }

    #[test]
    fn match_annotations_unparseable_fixture_surfaces_fixture_error() {
        let tmp = TempDir::new().unwrap();
        write_match_fixture(tmp.path(), "this is not valid TOML at all = =");
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let req = CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.5,
            include_suggestions: false,
        };
        let err = client.match_annotations(&req).unwrap_err();
        assert!(matches!(err, CanonError::Fixture(_)));
    }

    #[test]
    fn get_entry_with_version_loads_versioned_fixture() {
        let tmp = TempDir::new().unwrap();
        let body = r#"
canon_id = "foo"
version = "v0.2.1"
active_version = "v0.2.1"
is_deprecated = false
canon_version = "v0.2.0"
canonical_text = "foo"
applies_to = ["fn"]
category = "invariants"
property_type = "safety"
description = ""
invariant_sketch = ""
examples = []
effective_scopes = [":vanilla"]

[backed_by]
":vanilla" = "specialized neural checker"

[prefix_tier_by_scope]
":vanilla" = "aristos:"

[references]
"#;
        write_entry_fixture(tmp.path(), "foo", "v0.2.1", body);
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let entry = client.get_entry("foo", Some("v0.2.1")).unwrap();
        assert_eq!(entry.canon_id, "foo");
        assert_eq!(entry.version, "v0.2.1");
        assert_eq!(entry.effective_scopes, vec![":vanilla".to_string()]);
        assert_eq!(
            entry.backed_by.get(":vanilla").and_then(|v| v.as_deref()),
            Some("specialized neural checker")
        );
        assert_eq!(
            entry.prefix_tier_by_scope.get(":vanilla"),
            Some(&crate::canon::PrefixTier::Aristos)
        );
    }

    #[test]
    fn get_entry_without_version_loads_active_fixture() {
        let tmp = TempDir::new().unwrap();
        let body = r#"
canon_id = "foo"
version = "v0.2.1"
active_version = "v0.2.1"
is_deprecated = false
canon_version = "v0.2.0"
canonical_text = "foo"
applies_to = ["fn"]
category = "invariants"
property_type = "safety"
description = ""
invariant_sketch = ""
examples = []
effective_scopes = [":vanilla"]

[backed_by]
":vanilla" = ""

[prefix_tier_by_scope]
":vanilla" = "kanon:"

[references]
"#;
        // Note: TOML has no null; the empty string here decodes via
        // serde as Some("") not None. To represent a true kanon: tier
        // (backed_by[scope] = null) the wire is JSON-only — mock_client
        // fixtures use the prefix_tier_by_scope as the source of truth
        // for the test below.
        write_entry_fixture(tmp.path(), "foo", "active", body);
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let entry = client.get_entry("foo", None).unwrap();
        assert_eq!(entry.canon_id, "foo");
        // kanon: tier signaled via prefix_tier_by_scope; backed_by may
        // be empty-string in the fixture (TOML limitation).
        assert_eq!(
            entry.prefix_tier_by_scope.get(":vanilla"),
            Some(&crate::canon::PrefixTier::Kanon)
        );
    }

    #[test]
    fn get_entry_missing_fixture_includes_canon_id_in_error() {
        let tmp = TempDir::new().unwrap();
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let err = client.get_entry("missing_id", Some("v0.1.0")).unwrap_err();
        assert!(matches!(err, CanonError::Fixture(_)));
        let msg = err.to_string();
        assert!(msg.contains("missing_id"), "got: {msg}");
        assert!(msg.contains("v0.1.0"), "got: {msg}");
    }

    #[test]
    fn request_verify_loads_fixture() {
        let tmp = TempDir::new().unwrap();
        let body = r#"
status = "submitted"
canon_id = "foo"
current_backing = "specialized neural checker"
"#;
        fs::write(tmp.path().join("request-verify.toml"), body).unwrap();
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let resp = client
            .request_verify(&RequestVerifyBody {
                canon_id: "foo".into(),
                notes: None,
            })
            .unwrap();
        assert_eq!(resp.status, "submitted");
        assert_eq!(resp.canon_id, "foo");
        assert_eq!(
            resp.current_backing.as_deref(),
            Some("specialized neural checker")
        );
    }

    #[test]
    fn catalogue_loads_fixture() {
        use crate::canon::{CanonCatalogue, CanonCatalogueEntry};
        let mut backed = std::collections::BTreeMap::new();
        backed.insert(
            ":vanilla".to_string(),
            "specialized neural checker".to_string(),
        );
        let cat = CanonCatalogue {
            notice: vec![],
            entries: vec![
                CanonCatalogueEntry {
                    canon_id: "foo".into(),
                    version: "v0.2.1".into(),
                    canonical_text: "foo text".into(),
                    category: "invariants".into(),
                    applies_to: vec!["fn".into()],
                    backed_by: backed,
                    coverage_level: "tight".into(),
                    spec_refs: vec!["S-001".into()],
                },
                CanonCatalogueEntry {
                    canon_id: "bar".into(),
                    version: "v0.1.0".into(),
                    canonical_text: "bar text".into(),
                    category: "invariants".into(),
                    applies_to: vec!["fn".into()],
                    backed_by: std::collections::BTreeMap::new(),
                    coverage_level: "none".into(),
                    spec_refs: vec![],
                },
            ],
        };
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("catalogue.toml"),
            toml::to_string(&cat).unwrap(),
        )
        .unwrap();
        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let loaded = client.catalogue().unwrap();
        assert_eq!(loaded, cat);
        assert_eq!(loaded.entries[0].tier_label(), "aristos");
        assert_eq!(loaded.entries[1].tier_label(), "kanon");
    }

    #[test]
    fn from_env_returns_none_when_var_unset() {
        // Don't actually unset / re-set ARISTO_CANON_FIXTURE here —
        // env-var manipulation in tests races with parallel tests
        // that might set their own. Instead assert that, in the
        // absence of the env var (which is the normal test
        // environment), from_env returns None.
        if std::env::var("ARISTO_CANON_FIXTURE").is_err() {
            assert!(MockCanonClient::from_env().is_none());
        }
    }

    #[test]
    fn mock_client_is_object_safe() {
        let tmp = TempDir::new().unwrap();
        let _boxed: Box<dyn CanonClient> = Box::new(MockCanonClient::new(tmp.path().to_path_buf()));
    }

    #[test]
    fn match_response_round_trips_through_toml() {
        // Build a known response, write it as TOML, read back via
        // mock client. Asserts the wire shape we serialize is the
        // same shape MockCanonClient deserializes.
        let resp = CanonMatchResponse {
            results: vec![vec![CanonMatch {
                canon_id: "foo".into(),
                version: "v0.1.0".into(),
                canonical_text: "foo text".into(),
                confidence: 0.9,
                scope: ":vanilla".into(),
                prefix_tier: PrefixTier::Kanon,
                backed_by: None,
                linked: Some("arta_xyz1".into()),
                verification: VerificationMetadata {
                    coverage_level: "none".into(),
                    test_binaries: vec![],
                },
            }]],
            effective_scopes: vec![":vanilla".into()],
            canon_version: "v0.2.0".into(),
            matched_at: "2026-06-15T09:14:22Z".into(),
            suggestions: None,
        };
        let tmp = TempDir::new().unwrap();
        let toml_text = toml::to_string(&resp).unwrap();
        fs::write(tmp.path().join("match.toml"), toml_text).unwrap();

        let client = MockCanonClient::new(tmp.path().to_path_buf());
        let req = CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.5,
            include_suggestions: false,
        };
        let loaded = client.match_annotations(&req).unwrap();
        assert_eq!(loaded, resp);
    }
}
