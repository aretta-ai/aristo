//! End-to-end scenario tests for `aristo canon probe` — the S2
//! presence probe (P-008 SLICE23-SPEC aristo items 4-6).
//!
//! Pattern mirrors `canon_accept_command.rs`: spawn the real `aristo`
//! binary in a sandbox workspace and assert on stdout + on-disk state.
//! The cache is seeded through the TYPED `aristo-core` structs
//! (`CanonMatchesFile::write_atomic`) so these tests also pin the
//! carry contract: a bundle persisted on an accepted match is what
//! the probe reads back.
//!
//! No network and no real SUT: the default suite stops before any
//! cargo invocation (`--gen-only`, empty-union early exit, SUT
//! validation errors). The actual compile path is covered by the
//! env-gated `probe_end_to_end_against_real_sut` (set
//! `ARISTO_PROBE_E2E_SUT=/path/to/instrumented/turso` to run it) and
//! by the classify unit tests against captured spike stderr.

use std::path::Path;
use std::process::Command;

use aristo_core::canon::{
    AcceptedMatch, CacheEntry, CanonMatchesFile, InstrumentationBundle, PrefixTier,
    VerificationMetadata,
};
use aristo_core::index::AnnotationId;
use tempfile::TempDir;

/// Byte-identical copy of the cross-repo golden fixture (also
/// round-trip-pinned in aristo-core).
const GOLDEN: &str = include_str!("../src/commands/canon/probe/fixtures/golden-bundle.json");

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

fn aristo_in(workspace: &Path) -> Command {
    let mut c = Command::new(aristo_bin());
    c.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        c.env("PATH", path);
    }
    #[cfg(target_os = "macos")]
    if let Ok(p) = std::env::var("DYLD_FALLBACK_LIBRARY_PATH") {
        c.env("DYLD_FALLBACK_LIBRARY_PATH", p);
    }
    let home = workspace.join("home");
    std::fs::create_dir_all(&home).unwrap();
    c.env("HOME", &home);
    c.env("XDG_CONFIG_HOME", home.join("xdg"));
    c.current_dir(workspace);
    c
}

fn setup_workspace() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("aristo.toml"), "").unwrap();
    std::fs::create_dir_all(tmp.path().join(".aristo")).unwrap();
    tmp
}

fn golden_bundle() -> InstrumentationBundle {
    serde_json::from_str(GOLDEN).expect("golden fixture decodes")
}

fn accepted_match(verification: Option<VerificationMetadata>) -> AcceptedMatch {
    AcceptedMatch {
        canon_id: "wal_install_coherence".into(),
        version: "v0.1.0".into(),
        canonical_text: "install_connection_state samples coherently".into(),
        canon_version: "v0.2.0".into(),
        confidence: 1.0,
        prefix_tier: PrefixTier::Aristos,
        backed_by: None,
        linked: None,
        verification,
        accepted_at: "2026-07-02T00:00:00Z".into(),
        bound_at: "2026-07-02T00:00:00Z".into(),
    }
}

fn write_cache(workspace: &Path, matches: Vec<AcceptedMatch>) {
    let mut cache = CanonMatchesFile::default();
    cache.entries.insert(
        AnnotationId::parse("aristos:wal_install_invariant").unwrap(),
        CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-07-02T00:00:00Z".into(),
            pending_matches: vec![],
            accepted_matches: matches,
            rejected_matches: vec![],
        },
    );
    cache
        .write_atomic(&workspace.join(".aristo").join("canon-matches.toml"))
        .unwrap();
}

/// A fake SUT checkout: `core/Cargo.toml` (the sut_binding subpath),
/// a workspace `Cargo.lock`, and a `target/` dir (warm-graph shape).
fn setup_fake_sut(workspace: &Path) -> std::path::PathBuf {
    let sut = workspace.join("sut");
    std::fs::create_dir_all(sut.join("core")).unwrap();
    std::fs::write(
        sut.join("core").join("Cargo.toml"),
        "[package]\nname = \"turso_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(sut.join("Cargo.lock"), "# fake lock\n").unwrap();
    std::fs::create_dir_all(sut.join("target")).unwrap();
    sut
}

#[test]
fn probe_without_bundles_reports_nothing_to_probe() {
    let ws = setup_workspace();
    // Accepted match exists but carries no verification metadata at
    // all (pre-P-008 shape) — the union is empty.
    write_cache(ws.path(), vec![accepted_match(None)]);

    let out = aristo_in(ws.path())
        .args(["canon", "probe", "--sut-path", "/nonexistent-not-needed"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "no bundles is not an error; stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("no instrumentation bundles"),
        "got: {stdout}"
    );
    assert!(stdout.contains("nothing to probe"), "got: {stdout}");
}

#[test]
fn probe_warns_coverage_integrity_for_routed_tests_without_bundle() {
    let ws = setup_workspace();
    write_cache(
        ws.path(),
        vec![accepted_match(Some(VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec!["wr03_install_coherence".into()],
            instrumentation: None,
        }))],
    );

    let out = aristo_in(ws.path())
        .args(["canon", "probe", "--sut-path", "/nonexistent-not-needed"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("coverage-integrity"),
        "the keyless-gated-row condition warns; stderr: {stderr}"
    );
    assert!(
        stderr.contains("wr03_install_coherence"),
        "warning names the routed tests; stderr: {stderr}"
    );
}

#[test]
fn probe_gen_only_writes_probe_crate_into_sut_target_dir() {
    let ws = setup_workspace();
    let sut = setup_fake_sut(ws.path());
    write_cache(
        ws.path(),
        vec![accepted_match(Some(VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec!["wr03_install_coherence".into()],
            instrumentation: Some(golden_bundle()),
        }))],
    );

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "probe",
            "--sut-path",
            sut.to_str().unwrap(),
            "--gen-only",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "gen-only succeeds; stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("bundle turso:7b6cbae:ae85f8792372"),
        "got: {stdout}"
    );
    assert!(stdout.contains("compile skipped"), "got: {stdout}");

    // The probe crate landed under the SUT's target dir (build-graph
    // docking) with the [patch] pointing at the checkout.
    let probe_dir = sut.join("target").join("aristo-s2-probe");
    let manifest = std::fs::read_to_string(probe_dir.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("[patch.\"https://github.com/tursodatabase/turso.git\"]"),
        "got: {manifest}"
    );
    let canonical_core = std::fs::canonicalize(sut.join("core")).unwrap();
    assert!(
        manifest.contains(&format!(
            "turso_core = {{ path = \"{}\" }}",
            canonical_core.display()
        )),
        "patch redirects to the local SUT package; got: {manifest}"
    );
    assert!(
        manifest.contains("rev = \"ad351877c5cf38c1fafc7f08703bfe521b8f4437\""),
        "dep line pins the bundle's base_ref; got: {manifest}"
    );

    let lib = std::fs::read_to_string(probe_dir.join("src").join("lib.rs")).unwrap();
    assert!(
        lib.contains("pub fn probe_inspect_header_version(log: &LogicalLog)"),
        "got: {lib}"
    );
    assert!(
        lib.contains("let _r: Option<u8> = log.inspect_header_version();"),
        "verbatim harness probe line; got: {lib}"
    );
    assert!(
        lib.contains("pub fn probe_installed_snapshot(wal: &dyn Wal)"),
        "got: {lib}"
    );

    // Build-graph sharing: the SUT lock seeds the probe.
    assert_eq!(
        std::fs::read_to_string(probe_dir.join("Cargo.lock")).unwrap(),
        "# fake lock\n"
    );
}

#[test]
fn probe_with_missing_sut_path_errors_with_hint() {
    let ws = setup_workspace();
    write_cache(
        ws.path(),
        vec![accepted_match(Some(VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec![],
            instrumentation: Some(golden_bundle()),
        }))],
    );

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "probe",
            "--sut-path",
            "/nonexistent/sut",
            "--gen-only",
        ])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "missing SUT is an error once bundles exist"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("SUT checkout not found"), "got: {stderr}");
    assert!(stderr.contains("--sut-path"), "hint present; got: {stderr}");
}

#[test]
fn probe_flag_broken_conflicts_with_gen_only() {
    let ws = setup_workspace();
    let out = aristo_in(ws.path())
        .args([
            "canon",
            "probe",
            "--sut-path",
            "/x",
            "--gen-only",
            "--flag-broken",
            "installed_snapshot",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "clap conflict surfaces; got: {stderr}"
    );
}

/// Full compile path against a REAL instrumented SUT checkout —
/// env-gated so the default suite needs no SUT and no network:
///
/// ```sh
/// ARISTO_PROBE_E2E_SUT=/path/to/aretta-turso cargo test -p aristo-cli \
///     --test canon_probe_command -- --ignored
/// ```
#[test]
#[ignore = "needs a local instrumented SUT checkout (set ARISTO_PROBE_E2E_SUT)"]
fn probe_end_to_end_against_real_sut() {
    let Some(sut) = std::env::var_os("ARISTO_PROBE_E2E_SUT") else {
        eprintln!("ARISTO_PROBE_E2E_SUT not set; skipping");
        return;
    };
    let ws = setup_workspace();
    write_cache(
        ws.path(),
        vec![accepted_match(Some(VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec!["wr03_install_coherence".into()],
            instrumentation: Some(golden_bundle()),
        }))],
    );
    let out = aristo_in(ws.path())
        .args(["canon", "probe", "--sut-path"])
        .arg(&sut)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Whatever the SUT's instrumentation state, the probe must reach a
    // classified per-accessor report — never a crash.
    assert!(
        stdout.contains("per-accessor:"),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("summary:"), "stdout: {stdout}");

    if out.status.success() {
        // Instrumented SUT: presence proven end-to-end.
        assert!(stdout.contains("PASS"), "stdout: {stdout}");
        return;
    }

    // Uninstrumented SUT: exercise the escalation card + the
    // flag-broken ledger + the visibly-skipped re-surface.
    assert!(
        stdout.contains("INSTRUMENTATION ESCALATION — accessor:"),
        "failures raise the card; stdout: {stdout}"
    );
    let flag = aristo_in(ws.path())
        .args([
            "canon",
            "probe",
            "--sut-path",
            sut.to_str().unwrap(),
            "--flag-broken",
            "installed_snapshot",
        ])
        .output()
        .unwrap();
    let flag_stdout = String::from_utf8_lossy(&flag.stdout);
    assert!(
        flag_stdout.contains("flag-broken: recorded `installed_snapshot`"),
        "stdout: {flag_stdout}\nstderr: {}",
        String::from_utf8_lossy(&flag.stderr)
    );
    let ledger =
        std::fs::read_to_string(ws.path().join(".aristo").join("instrumentation-debt.jsonl"))
            .unwrap();
    let entry: serde_json::Value = serde_json::from_str(ledger.lines().next().unwrap()).unwrap();
    assert_eq!(entry["accessor_id"], "installed_snapshot");
    assert!(
        entry["evidence"].as_str().unwrap().len() > 10,
        "got: {entry}"
    );

    let rerun = aristo_in(ws.path())
        .args(["canon", "probe", "--sut-path"])
        .arg(&sut)
        .output()
        .unwrap();
    let rerun_stdout = String::from_utf8_lossy(&rerun.stdout);
    assert!(
        rerun_stdout.contains("SKIPPED(instr-debt: installed_snapshot)"),
        "flagged debt is re-surfaced visibly; stdout: {rerun_stdout}"
    );
}
