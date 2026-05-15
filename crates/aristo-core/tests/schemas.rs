//! CI gate: every committed JSON Schema in `schemas/` must match what the
//! `dump-schemas` example would produce. Re-run
//! `cargo run --example dump-schemas` after changing any of the Rust
//! types in `aristo-core::index` and commit the regenerated schema.

use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn assert_committed_matches(name: &str, derived: String) {
    let path: PathBuf = workspace_root().join("schemas").join(name);
    let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read committed schema {}: {e}\n\
             hint: run `cargo run --example dump-schemas` to generate it",
            path.display()
        )
    });
    let derived_with_newline = format!("{derived}\n");
    if committed != derived_with_newline {
        // Show a few-line excerpt of the first diff so the failure is
        // diagnosable without diff-by-hand. Full regeneration is the fix.
        let summary = first_diff_excerpt(&committed, &derived_with_newline);
        panic!(
            "committed schema is out of date with the Rust types: {}\n\
             {summary}\n\
             To fix: run `cargo run --example dump-schemas` and commit the result.",
            path.display()
        );
    }
}

#[test]
fn aristo_index_schema_is_in_sync() {
    assert_committed_matches(
        "aristo-index.schema.json",
        aristo_core::index::index_file_schema_json(),
    );
}

#[test]
fn aristo_spec_schema_is_in_sync() {
    assert_committed_matches(
        "aristo-spec.schema.json",
        aristo_core::spec::spec_header_schema_json(),
    );
}

#[test]
fn aristo_config_schema_is_in_sync() {
    assert_committed_matches(
        "aristo-config.schema.json",
        aristo_core::config::config_file_schema_json(),
    );
}

fn first_diff_excerpt(committed: &str, derived: &str) -> String {
    for (line_no, (c, d)) in committed.lines().zip(derived.lines()).enumerate() {
        if c != d {
            return format!(
                "first divergence at line {}:\n  committed: {c}\n  derived  : {d}",
                line_no + 1
            );
        }
    }
    if committed.lines().count() != derived.lines().count() {
        format!(
            "files share a common prefix but differ in length \
             (committed: {} lines, derived: {} lines)",
            committed.lines().count(),
            derived.lines().count(),
        )
    } else {
        "files differ but no line-level diff found (trailing whitespace?)".to_string()
    }
}
