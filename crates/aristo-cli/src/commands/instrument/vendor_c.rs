//! `aristo instrument vendor-c` — write the C instrumentation runtime
//! (`aristo.h` + `aristo.c`) into a SUT's source tree so it can link Aristo's
//! fault-injection / observation points. The files are C11, standard keywords
//! only, and entirely gated by `-DARISTO_INSTRUMENT`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::CliResult;

const ARISTO_H_TEMPLATE: &str = include_str!("runtime/aristo.h.in");
const ARISTO_C_TEMPLATE: &str = include_str!("runtime/aristo.c.in");

/// Substitute the template placeholders: `{{SDK_VERSION}}` (a header comment)
/// and `{{ARISTO_ABI}}` (the `#define` and `_Static_assert` literal). The Rust
/// `ARISTO_ABI` const is thus the single source of truth for the ABI version —
/// the header cannot drift from it.
fn resolve(template: &str) -> String {
    template
        .replace("{{SDK_VERSION}}", env!("CARGO_PKG_VERSION"))
        .replace("{{ARISTO_ABI}}", &super::ARISTO_ABI.to_string())
}

#[derive(Debug, PartialEq, Eq)]
enum WriteOutcome {
    Created,
    Updated,
    Unchanged,
}

#[aristo::intent(
    "Re-vendoring with identical content leaves the file byte-identical and \
     returns Unchanged; Created (file absent) and Updated (content differed) \
     are the other two outcomes. Idempotence is the Unchanged case \
     specifically — a second `vendor-c` on an up-to-date runtime must not \
     rewrite the file, which would churn its mtime and dirty a clean tree.",
    verify = "test",
    id = "vendor_c_write_is_idempotent"
)]
fn write_file(path: &Path, content: &str) -> CliResult<WriteOutcome> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        if existing == content {
            return Ok(WriteOutcome::Unchanged);
        }
        fs::write(path, content)?;
        return Ok(WriteOutcome::Updated);
    }
    fs::write(path, content)?;
    Ok(WriteOutcome::Created)
}

pub(crate) fn run(out: PathBuf) -> CliResult<()> {
    println!("→ Vendoring the Aristo C runtime to {}/ …", out.display());
    let files = [
        ("aristo.h", resolve(ARISTO_H_TEMPLATE)),
        ("aristo.c", resolve(ARISTO_C_TEMPLATE)),
    ];
    for (name, content) in &files {
        let outcome = write_file(&out.join(name), content)?;
        let verb = match outcome {
            WriteOutcome::Created => "created",
            WriteOutcome::Updated => "updated",
            WriteOutcome::Unchanged => "unchanged",
        };
        println!("  • {name}  {verb}");
    }
    println!();
    println!("ok: Aristo C runtime vendored (2 files).");
    println!(
        "    Add {}/ to your include path; compile aristo.c and pass",
        out.display()
    );
    println!("    -DARISTO_INSTRUMENT in instrumented builds (C11, -O1+).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::instrument::ARISTO_ABI;

    #[test]
    fn resolve_substitutes_sdk_version() {
        let h = resolve(ARISTO_H_TEMPLATE);
        assert!(
            !h.contains("{{SDK_VERSION}}"),
            "the SDK_VERSION placeholder must be substituted before writing"
        );
        assert!(h.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn header_abi_pins_match_rust_const() {
        // Gate B: the `#define ARISTO_ABI` and the `_Static_assert` literal in
        // the header must both equal the Rust ARISTO_ABI const — else the
        // vendored runtime and the generator silently disagree on the
        // aristo_decision layout in the fault path.
        let h = resolve(ARISTO_H_TEMPLATE);
        assert!(
            h.contains(&format!("#define ARISTO_ABI {ARISTO_ABI}")),
            "header #define must match the Rust ARISTO_ABI const"
        );
        assert!(
            h.contains(&format!("_Static_assert(ARISTO_ABI == {ARISTO_ABI},")),
            "header _Static_assert literal must match the Rust ARISTO_ABI const"
        );
    }

    #[test]
    fn vendor_writes_both_files_with_expected_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("aristo");
        run(out.clone()).unwrap();

        let h = fs::read_to_string(out.join("aristo.h")).unwrap();
        let c = fs::read_to_string(out.join("aristo.c")).unwrap();
        // Header carries the surface the contract promises.
        assert!(h.contains("#define ARISTO_YIELD_POINT"));
        assert!(h.contains("#define ARISTO_FAULT_POINT"));
        assert!(h.contains("#define ARISTO_TU_LOCAL"));
        assert!(h.contains("aristo_decision"));
        assert!(h.contains("aristo_set_fault_hook"));
        // Source carries the thread-local hook storage.
        assert!(c.contains("_Thread_local"));
        assert!(c.contains("aristo_fault_point"));
    }

    #[test]
    fn vendor_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("aristo");
        run(out.clone()).unwrap();
        let h1 = fs::read_to_string(out.join("aristo.h")).unwrap();
        let c1 = fs::read_to_string(out.join("aristo.c")).unwrap();
        run(out.clone()).unwrap(); // second run must not change bytes
        assert_eq!(h1, fs::read_to_string(out.join("aristo.h")).unwrap());
        assert_eq!(c1, fs::read_to_string(out.join("aristo.c")).unwrap());
    }

    #[test]
    fn write_file_reports_created_unchanged_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("nested/f.txt");
        assert_eq!(write_file(&p, "a").unwrap(), WriteOutcome::Created);
        assert_eq!(write_file(&p, "a").unwrap(), WriteOutcome::Unchanged);
        assert_eq!(write_file(&p, "b").unwrap(), WriteOutcome::Updated);
    }
}
