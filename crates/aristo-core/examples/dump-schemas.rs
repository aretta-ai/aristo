//! Regenerate the canonical JSON Schemas for every Aristo on-disk file
//! format and write them to `<workspace>/schemas/`.
//!
//! Run via: `cargo run --example dump-schemas`
//!
//! The `tests/schemas.rs` integration test asserts the committed schemas
//! match what this example would produce — re-run this when you change
//! the canonical Rust types in `aristo-core::index`.

use std::fs;
use std::path::PathBuf;

fn main() {
    let workspace_root: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has parent (crates/)")
        .parent()
        .expect("crates/ has parent (workspace root)")
        .to_path_buf();

    let schemas_dir = workspace_root.join("schemas");
    fs::create_dir_all(&schemas_dir).expect("create schemas/ directory");

    write_schema(
        &schemas_dir,
        "aristo-index.schema.json",
        aristo_core::index::index_file_schema_json(),
    );
    write_schema(
        &schemas_dir,
        "aristo-spec.schema.json",
        aristo_core::spec::spec_header_schema_json(),
    );
}

fn write_schema(dir: &std::path::Path, name: &str, body: String) {
    // Trailing newline so the file is POSIX-compliant + git-friendly.
    let with_newline = format!("{body}\n");
    let path = dir.join(name);
    fs::write(&path, &with_newline).unwrap_or_else(|e| panic!("write {path:?}: {e}"));
    println!("✓ wrote {} ({} bytes)", path.display(), with_newline.len());
}
