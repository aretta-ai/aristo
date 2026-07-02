//! The impure edge of the S2 presence probe: SUT-checkout resolution,
//! probe-crate materialization, and the two `cargo check` runs.
//!
//! **Build-graph sharing is mandatory** (PROPOSAL ~line 390, the
//! de-risk spike's core finding): the probe compile is seconds-fast
//! ONLY when it docks into the user's existing SUT build graph — the
//! same `Cargo.lock`, the same `CARGO_TARGET_DIR`, the same toolchain.
//! Concretely:
//!
//! - the probe crate is materialized *under the SUT's target dir*
//!   (so rustup's toolchain-file discovery walking up from the probe
//!   cwd finds the SUT's `rust-toolchain`, and `cargo clean` in the
//!   SUT sweeps the ephemeral crate away);
//! - the SUT checkout's `Cargo.lock` is copied in as the probe's
//!   starting lock, pinning transitives to what the user already
//!   built (the copy is discarded with the probe dir — the user's
//!   lock is never touched);
//! - `CARGO_TARGET_DIR` is pointed at the SUT's `target/` (an
//!   existing `CARGO_TARGET_DIR` env var is respected — that IS the
//!   user's graph).
//!
//! When the graph is cold (no lock / no target dir) the caller prints
//! an explicit one-time cold-build warning — the cost is never paid
//! silently.
//!
//! Everything decision-shaped lives in the pure siblings (`gen`,
//! `classify`); this module only touches the filesystem and spawns
//! cargo, so the command stays unit-testable without a real SUT.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::{CliError, CliResult};

use super::gen::ProbeCrate;

/// Resolved build-graph coordinates of the user's SUT checkout.
#[derive(Debug, Clone)]
pub(crate) struct SutContext {
    /// The `--sut-path` checkout root (canonicalized).
    pub sut_root: PathBuf,
    /// `<sut_root>/<sut_binding[package]>` — what `[patch]` points at.
    pub patch_path: PathBuf,
    /// The shared build-graph target dir (`$CARGO_TARGET_DIR` if set,
    /// else `<sut_root>/target`).
    pub target_dir: PathBuf,
    /// The SUT checkout's `Cargo.lock`, when present.
    pub lock_path: Option<PathBuf>,
    /// Why the graph is cold; empty means warm.
    pub cold_reasons: Vec<String>,
}

pub(crate) fn resolve_sut(
    sut_path: &Path,
    package: &str,
    sut_binding: &BTreeMap<String, String>,
) -> CliResult<SutContext> {
    if !sut_path.is_dir() {
        return Err(CliError::Other {
            message: format!(
                "SUT checkout not found at `{}`\n  \
                 hint: --sut-path must point at your local instrumented SUT clone \
                 (e.g. the aretta-ai/turso fork checkout).",
                sut_path.display()
            ),
            exit_code: 2,
        });
    }
    let sut_root = fs::canonicalize(sut_path).unwrap_or_else(|_| sut_path.to_path_buf());

    let subpath = sut_binding.get(package).cloned().unwrap_or_default();
    let patch_path = if subpath.is_empty() {
        sut_root.clone()
    } else {
        sut_root.join(&subpath)
    };
    if !patch_path.join("Cargo.toml").is_file() {
        return Err(CliError::Other {
            message: format!(
                "no Cargo.toml at `{}` — expected the SUT package `{package}` there \
                 (bundle sut_binding maps `{package}` -> `{subpath}`)\n  \
                 hint: is --sut-path the checkout ROOT (not the package subdir)?",
                patch_path.display()
            ),
            exit_code: 2,
        });
    }

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| sut_root.join("target"));

    let mut cold_reasons = Vec::new();
    let lock_path = {
        let p = sut_root.join("Cargo.lock");
        if p.is_file() {
            Some(p)
        } else {
            cold_reasons.push(format!(
                "no Cargo.lock at {} (fresh dependency resolution)",
                sut_root.display()
            ));
            None
        }
    };
    if !target_dir.is_dir() {
        cold_reasons.push(format!(
            "target dir {} does not exist yet (full SUT build ahead)",
            target_dir.display()
        ));
    }

    Ok(SutContext {
        sut_root,
        patch_path,
        target_dir,
        lock_path,
        cold_reasons,
    })
}

/// Materialize the generated probe crate at `probe_dir` (recreated
/// from scratch each run), seeding it with the SUT's `Cargo.lock`
/// when available.
pub(crate) fn write_probe_crate(
    probe_dir: &Path,
    krate: &ProbeCrate,
    lock_source: Option<&Path>,
) -> io::Result<()> {
    match fs::remove_dir_all(probe_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    fs::create_dir_all(probe_dir.join("src"))?;
    fs::write(probe_dir.join("Cargo.toml"), &krate.cargo_toml)?;
    fs::write(probe_dir.join("src").join("lib.rs"), &krate.lib_rs)?;
    if let Some(lock) = lock_source {
        fs::copy(lock, probe_dir.join("Cargo.lock"))?;
    }
    Ok(())
}

/// One captured `cargo check` run.
#[derive(Debug)]
pub(crate) struct CheckRun {
    pub success: bool,
    pub stderr: String,
    pub secs: f64,
}

/// Run `cargo check` in the probe dir against the shared target dir.
/// `features` is passed verbatim (the bundle's `compile_check.features`
/// spec); `None` is the feature-off clean check.
pub(crate) fn run_cargo_check(
    probe_dir: &Path,
    target_dir: &Path,
    features: Option<&str>,
) -> CliResult<CheckRun> {
    let started = Instant::now();
    let mut cmd = Command::new("cargo");
    cmd.arg("check");
    if let Some(f) = features {
        cmd.args(["--features", f]);
    }
    cmd.current_dir(probe_dir)
        .env("CARGO_TARGET_DIR", target_dir);
    let output = cmd.output().map_err(|e| CliError::Other {
        message: format!("failed to run `cargo check` for the presence probe: {e}"),
        exit_code: 1,
    })?;
    Ok(CheckRun {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        secs: started.elapsed().as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn binding() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("turso_core".to_string(), "core".to_string());
        m
    }

    fn fake_sut(with_lock: bool, with_target: bool) -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("core")).unwrap();
        fs::write(
            tmp.path().join("core").join("Cargo.toml"),
            "[package]\nname = \"turso_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        if with_lock {
            fs::write(tmp.path().join("Cargo.lock"), "# lock\n").unwrap();
        }
        if with_target {
            fs::create_dir_all(tmp.path().join("target")).unwrap();
        }
        tmp
    }

    #[test]
    fn missing_sut_path_errors_with_hint() {
        let err = resolve_sut(Path::new("/nonexistent/sut"), "turso_core", &binding())
            .expect_err("must error");
        let msg = err.to_string();
        assert!(msg.contains("SUT checkout not found"), "got: {msg}");
        assert!(msg.contains("--sut-path"), "got: {msg}");
    }

    #[test]
    fn sut_without_package_manifest_errors_with_binding_context() {
        let tmp = TempDir::new().unwrap(); // no core/Cargo.toml
        let err = resolve_sut(tmp.path(), "turso_core", &binding()).expect_err("must error");
        let msg = err.to_string();
        assert!(msg.contains("no Cargo.toml"), "got: {msg}");
        assert!(msg.contains("`turso_core` -> `core`"), "got: {msg}");
    }

    #[test]
    fn warm_graph_has_no_cold_reasons() {
        let sut = fake_sut(true, true);
        let ctx = resolve_sut(sut.path(), "turso_core", &binding()).unwrap();
        assert!(ctx.cold_reasons.is_empty(), "got: {:?}", ctx.cold_reasons);
        assert!(ctx.lock_path.is_some());
        assert!(ctx.patch_path.ends_with("core"));
        assert!(ctx.target_dir.ends_with("target"));
    }

    #[test]
    fn cold_graph_reasons_name_lock_and_target() {
        let sut = fake_sut(false, false);
        let ctx = resolve_sut(sut.path(), "turso_core", &binding()).unwrap();
        assert_eq!(ctx.cold_reasons.len(), 2, "got: {:?}", ctx.cold_reasons);
        assert!(ctx.cold_reasons[0].contains("Cargo.lock"));
        assert!(ctx.cold_reasons[1].contains("target dir"));
        assert!(ctx.lock_path.is_none());
    }

    #[test]
    fn write_probe_crate_materializes_and_seeds_lock() {
        let sut = fake_sut(true, true);
        let ctx = resolve_sut(sut.path(), "turso_core", &binding()).unwrap();
        let probe_dir = ctx.target_dir.join("aristo-s2-probe");
        let krate = ProbeCrate {
            cargo_toml: "# manifest\n".into(),
            lib_rs: "// lib\n".into(),
            degraded: vec![],
            receiver_imports: Default::default(),
        };
        write_probe_crate(&probe_dir, &krate, ctx.lock_path.as_deref()).unwrap();
        assert_eq!(
            fs::read_to_string(probe_dir.join("Cargo.toml")).unwrap(),
            "# manifest\n"
        );
        assert_eq!(
            fs::read_to_string(probe_dir.join("src/lib.rs")).unwrap(),
            "// lib\n"
        );
        assert_eq!(
            fs::read_to_string(probe_dir.join("Cargo.lock")).unwrap(),
            "# lock\n",
            "the SUT lock seeds the probe's resolution"
        );

        // Re-materializing replaces stale content wholesale.
        fs::write(probe_dir.join("stale.txt"), "old").unwrap();
        write_probe_crate(&probe_dir, &krate, None).unwrap();
        assert!(!probe_dir.join("stale.txt").exists());
        assert!(
            !probe_dir.join("Cargo.lock").exists(),
            "no lock source, no seeded lock"
        );
    }
}
