//! `aristo instrument vendor-c` — write the C instrumentation runtime
//! (`aristo.h` + `aristo.c`) into a SUT's source tree so it can link Aristo's
//! fault-injection / observation points. The files are C11, standard keywords
//! only, and entirely gated by `-DARISTO_INSTRUMENT`.

use std::path::PathBuf;

use super::{write_file, WriteOutcome};
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
    use std::fs;

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

    /// A C harness that self-checks the runtime: no-hook Continue, fail-Nth
    /// injection, re-entrant Continue, and clear. Exit 0 = all held.
    const C_RUNTIME_SELFTEST: &str = r#"
#include "aristo.h"
#include <string.h>
typedef struct { const char *tgt; unsigned nth, seen; int reentrant_inject; } Pol;
static aristo_decision fail_nth(const char *label, void *state) {
    Pol *p = state;
    aristo_decision inner = aristo_fault_point("inner");   /* re-entrant: must Continue */
    if (aristo_is_inject(inner)) p->reentrant_inject = 1;
    if (!strcmp(label, p->tgt) && ++p->seen == p->nth) return aristo_inject(42);
    return ARISTO_CONTINUE;
}
/* a one-shot that clears ITSELF: must fire once, then stay cleared */
static aristo_decision self_clearing(const char *label, void *state) {
    (void)label; (void)state;
    aristo_set_fault_hook(0, 0);
    return aristo_inject(99);
}
int main(void) {
    if (aristo_is_inject(aristo_fault_point("x"))) return 1;      /* no hook -> Continue */
    Pol p = { "sync", 3, 0, 0 };
    aristo_set_fault_hook(fail_nth, &p);
    if (aristo_is_inject(aristo_fault_point("sync"))) return 2;   /* 1 */
    if (aristo_is_inject(aristo_fault_point("sync"))) return 3;   /* 2 */
    aristo_decision d = aristo_fault_point("sync");              /* 3 -> inject */
    if (!aristo_is_inject(d) || d.code != 42) return 4;
    if (p.reentrant_inject) return 5;                            /* re-entrant saw Continue */
    aristo_set_fault_hook(0, 0);
    if (aristo_is_inject(aristo_fault_point("sync"))) return 6;  /* cleared */
    /* self-clearing one-shot: fires once, then STAYS cleared (dirty-flag semantics) */
    aristo_set_fault_hook(self_clearing, 0);
    aristo_decision e = aristo_fault_point("any");              /* fires -> inject 99 */
    if (!aristo_is_inject(e) || e.code != 99) return 7;
    if (aristo_is_inject(aristo_fault_point("any"))) return 8;   /* stays cleared, NOT re-armed */
    return 0;
}
"#;

    /// A translation unit that uses every macro, built flag-OFF: must compile
    /// clean under -Werror with no `-DARISTO_INSTRUMENT`.
    const C_OFF_TU: &str = r#"
#include "aristo.h"
ARISTO_TU_LOCAL int helper(int x) { return x + 1; }   /* static in production */
int run(int x) {
    ARISTO_YIELD_POINT("run.enter");
    aristo_decision d = ARISTO_FAULT_POINT("run.fault");
    if (aristo_is_inject(d)) return -1;
    return helper(x);
}
"#;

    /// A TU that includes `aristo.c` twice: the include-once guard must make
    /// the second a no-op (no duplicate-symbol / redefinition error).
    const C_DOUBLE_INCLUDE: &str = r#"
#include "aristo.c"
#include "aristo.c"
int probe(void) { aristo_set_fault_hook(0, 0); return 0; }
"#;

    fn find_c_compiler() -> Option<&'static str> {
        ["cc", "gcc", "clang"].into_iter().find(|c| {
            std::process::Command::new(c)
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
    }

    #[test]
    fn emitted_runtime_compiles_and_injects_under_a_c_compiler() {
        let Some(cc) = find_c_compiler() else {
            eprintln!("skipping: no C compiler (cc/gcc/clang) on PATH");
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let inc = format!("-I{}", dir.display());
        fs::write(dir.join("aristo.h"), resolve(ARISTO_H_TEMPLATE)).unwrap();
        fs::write(dir.join("aristo.c"), resolve(ARISTO_C_TEMPLATE)).unwrap();
        fs::write(dir.join("selftest.c"), C_RUNTIME_SELFTEST).unwrap();
        fs::write(dir.join("off.c"), C_OFF_TU).unwrap();

        let strict = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-Wpedantic"];

        // Instrumented: must compile, and the self-test must exit 0.
        let bin = dir.join("selftest");
        let build = std::process::Command::new(cc)
            .args(strict)
            .args(["-O1", "-DARISTO_INSTRUMENT"])
            .arg(&inc)
            .arg(dir.join("aristo.c"))
            .arg(dir.join("selftest.c"))
            .arg("-o")
            .arg(&bin)
            .status()
            .unwrap();
        assert!(
            build.success(),
            "instrumented runtime failed to compile ({cc})"
        );
        let run = std::process::Command::new(&bin).status().unwrap();
        assert!(
            run.success(),
            "runtime self-test failed ({cc}): fail-Nth / re-entrancy / clear"
        );

        // Flag-off: a TU using every macro compiles clean with NO -D at -O2.
        let off = std::process::Command::new(cc)
            .args(strict)
            .args(["-O2", "-c"])
            .arg(&inc)
            .arg(dir.join("off.c"))
            .arg("-o")
            .arg(dir.join("off.o"))
            .status()
            .unwrap();
        assert!(off.success(), "flag-off TU failed to compile clean ({cc})");

        // Include-once guard: `#include "aristo.c"` twice in one TU must not
        // produce a duplicate-symbol / redefinition error (amalgamation safety).
        fs::write(dir.join("double.c"), C_DOUBLE_INCLUDE).unwrap();
        let dbl = std::process::Command::new(cc)
            .args(strict)
            .args(["-O1", "-DARISTO_INSTRUMENT", "-c"])
            .arg(&inc)
            .arg(dir.join("double.c"))
            .arg("-o")
            .arg(dir.join("double.o"))
            .status()
            .unwrap();
        assert!(
            dbl.success(),
            "double-#include of aristo.c failed — include-once guard missing ({cc})"
        );
    }
}
