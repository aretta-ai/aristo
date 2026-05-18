//! Cargo-fixture imperative test for the `aristo_doc` feature (slice 30).
//!
//! Spawns `cargo build` on a tempdir fixture project that:
//!   - depends on `aristo` with `features = ["aristo_doc"]`
//!   - has its own `aristo.toml` (workspace-root marker)
//!   - has its own `.aristo/doc/<id>.md` with hand-baked content
//!   - has source code that uses `#[aristo::intent(..., id = "...")]`
//!
//! Asserts:
//!   - happy path compiles with the feature on
//!   - missing `.aristo/doc/<id>.md` file fails the build (include_str! errors)
//!   - missing explicit `id` argument fails the build with the macro's
//!     compile_error
//!
//! Slow by necessity: each test compiles `aristo` and its transitive deps
//! from source into a fresh tempdir. Shared `CARGO_TARGET_DIR` across the
//! three tests cuts later runs to seconds; the first run pays full cost
//! (~30s on a warm cache, longer on cold).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn workspace_root() -> PathBuf {
    // crates/aristo-macros → ../.. = workspace root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has a parent")
        .parent()
        .expect("crates/ has a parent (workspace root)")
        .to_path_buf()
}

fn shared_target_dir() -> PathBuf {
    // CARGO_TARGET_TMPDIR is the per-test-binary temp; sharing across the
    // three tests in this file means deps compile once.
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("aristo_doc_fixture_target")
}

fn write_fixture(dir: &Path, aristo_dep_path: &Path, src_lib_rs: &str, md_files: &[(&str, &str)]) {
    let cargo_toml = format!(
        r#"[package]
name = "aristo-doc-fixture"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
aristo = {{ path = "{}", features = ["aristo_doc"] }}
"#,
        aristo_dep_path.display(),
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(
        dir.join("aristo.toml"),
        "[verify]\ndefault_method = \"test\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), src_lib_rs).unwrap();

    fs::create_dir_all(dir.join(".aristo/doc")).unwrap();
    for (name, content) in md_files {
        fs::write(dir.join(".aristo/doc").join(name), content).unwrap();
    }
}

fn cargo_build(dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO"))
        .arg("build")
        .env("CARGO_TARGET_DIR", shared_target_dir())
        .current_dir(dir)
        .output()
        .expect("cargo build should run")
}

const FIXTURE_INTENT_MD: &str = "**Aristo verified intent — `fixture_intent`**\n\
                                  \n\
                                  Fixture content for the slice-30 imperative test. \
                                  When this MD is loaded via `include_str!`, the \
                                  build succeeds.\n\
                                  \n\
                                  <sub>Verify level: **false**</sub>\n\
                                  \n\
                                  ---\n";

#[test]
fn aristo_doc_feature_compiles_with_baked_md_file() {
    let tmp = TempDir::new().unwrap();
    let aristo_dep = workspace_root().join("crates/aristo");

    write_fixture(
        tmp.path(),
        &aristo_dep,
        // The intent text + explicit id; the id is what selects which MD
        // gets `include_str!`-ed into the resulting rustdoc.
        r#"#[aristo::intent("Documentation-only fixture intent.", verify = false, id = "fixture_intent")]
pub fn fixture_function() {}
"#,
        &[("fixture_intent.md", FIXTURE_INTENT_MD)],
    );

    let output = cargo_build(tmp.path());
    assert!(
        output.status.success(),
        "build failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn aristo_doc_feature_fails_loudly_when_md_file_is_missing() {
    let tmp = TempDir::new().unwrap();
    let aristo_dep = workspace_root().join("crates/aristo");

    write_fixture(
        tmp.path(),
        &aristo_dep,
        // References an id whose `.aristo/doc/<id>.md` we deliberately
        // do NOT write — `include_str!` should error at compile time.
        r#"#[aristo::intent("Doc-only intent referencing a missing MD.", verify = false, id = "missing_doc_intent")]
pub fn fixture_function() {}
"#,
        // No MD files baked into .aristo/doc/.
        &[],
    );

    let output = cargo_build(tmp.path());
    assert!(
        !output.status.success(),
        "expected build to fail; it succeeded:\nstdout:\n{}\n",
        String::from_utf8_lossy(&output.stdout),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    // rustc's include_str! error mentions "couldn't read" or "No such file";
    // accept either since rustc's wording varies. The point is the build
    // fails loudly with a file-not-found pointer, not silently.
    assert!(
        stderr.contains("missing_doc_intent.md")
            || stderr.contains("No such file")
            || stderr.contains("couldn't read")
            || stderr.contains("cannot find file"),
        "expected MD-not-found diagnostic; got:\n{stderr}",
    );
}

#[test]
fn aristo_doc_feature_fails_when_id_argument_is_missing() {
    let tmp = TempDir::new().unwrap();
    let aristo_dep = workspace_root().join("crates/aristo");

    write_fixture(
        tmp.path(),
        &aristo_dep,
        // No explicit `id =`. With `aristo_doc` on, the macro should
        // emit a compile_error pointing at the id requirement.
        r#"#[aristo::intent("Intent without explicit id.", verify = false)]
pub fn fixture_function() {}
"#,
        &[],
    );

    let output = cargo_build(tmp.path());
    assert!(
        !output.status.success(),
        "expected build to fail; it succeeded:\nstdout:\n{}\n",
        String::from_utf8_lossy(&output.stdout),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires an explicit `id"),
        "expected id-required compile_error; got:\n{stderr}",
    );
}
