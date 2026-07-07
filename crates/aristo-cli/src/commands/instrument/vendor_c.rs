//! `aristo instrument vendor-c` — write the C instrumentation runtime
//! (`aristo.h` + `aristo.c`) so a SUT can link Aristo's fault-injection /
//! observation points. The runtime carries no SUT types, so it need not live
//! in the source tree: emit it to a scratch directory, build it once into
//! `libaristo.a`, and link that. The files are C11, standard keywords only,
//! and entirely gated by `-DARISTO_INSTRUMENT`. `--check` verifies a vendored
//! copy still matches the current CLI templates; `--seam` instead emits the
//! opt-in LD_PRELOAD seam shim (`aristo_seam.c`) — a test-only, Linux-only
//! fault seam driven by env vars, never linked into the SUT.

use std::path::{Path, PathBuf};

use super::{drifted_files, write_file, WriteOutcome};
use crate::{CliError, CliResult};

const ARISTO_H_TEMPLATE: &str = include_str!("runtime/aristo.h.in");
const ARISTO_C_TEMPLATE: &str = include_str!("runtime/aristo.c.in");
const ARISTO_SEAM_TEMPLATE: &str = include_str!("runtime/aristo_seam.c.in");

/// Substitute the template placeholders: `{{SDK_VERSION}}` (a header comment)
/// and `{{ARISTO_ABI}}` (the `#define` and `_Static_assert` literal). The Rust
/// `ARISTO_ABI` const is thus the single source of truth for the ABI version —
/// the header cannot drift from it.
fn resolve(template: &str) -> String {
    template
        .replace("{{SDK_VERSION}}", env!("CARGO_PKG_VERSION"))
        .replace("{{ARISTO_ABI}}", &super::ARISTO_ABI.to_string())
}

pub(crate) fn run(out: PathBuf, check: bool, seam: bool) -> CliResult<()> {
    // Two vendorable artifacts: the in-process runtime (aristo.{c,h}, the
    // default) and the opt-in LD_PRELOAD seam shim (aristo_seam.c, --seam).
    let (files, what): (Vec<(&str, String)>, &str) = if seam {
        (
            vec![("aristo_seam.c", resolve(ARISTO_SEAM_TEMPLATE))],
            "seam shim",
        )
    } else {
        (
            vec![
                ("aristo.h", resolve(ARISTO_H_TEMPLATE)),
                ("aristo.c", resolve(ARISTO_C_TEMPLATE)),
            ],
            "runtime",
        )
    };
    if check {
        let drifted = drifted_files(&files, &out);
        if drifted.is_empty() {
            println!(
                "ok: vendored Aristo C {what} is up to date (aristo {}).",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        let reinvoke = if seam { "vendor-c --seam" } else { "vendor-c" };
        return Err(CliError::Other {
            message: format!(
                "vendored Aristo C {what} is stale: {} differ from the aristo {} \
                 templates.\n       Re-run `aristo instrument {reinvoke}` (or `make \
                 revendor`) and commit the result.",
                drifted.join(", "),
                env!("CARGO_PKG_VERSION")
            ),
            exit_code: 2,
        });
    }
    println!("→ Vendoring the Aristo C {what} to {}/ …", out.display());
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
    println!(
        "{}",
        if seam {
            seam_guidance(&out)
        } else {
            guidance(&out)
        }
    );
    Ok(())
}

/// The post-vendor guidance: how to build the emitted runtime out of the source
/// tree and link it. Pure in `out` (the emit directory) so the exact wording
/// can be asserted; `run` prints it after writing the files.
fn guidance(out: &Path) -> String {
    let dir = out.display();
    [
        "ok: Aristo C runtime vendored (2 files).".to_string(),
        "    Out-of-tree link model — the runtime need not live in your source".to_string(),
        "    tree. Build it once into a static library, then link that:".to_string(),
        format!(
            "      cc -std=c11 -DARISTO_INSTRUMENT -I{dir} -c {dir}/aristo.c -o {dir}/aristo.o"
        ),
        format!("      ar rcs {dir}/libaristo.a {dir}/aristo.o"),
        format!("    Instrumented build: add -I{dir} (both flavors — ARISTO_TU_LOCAL pulls"),
        "    aristo.h in unconditionally), pass -DARISTO_INSTRUMENT, and link".to_string(),
        format!("    {dir}/libaristo.a."),
    ]
    .join("\n")
}

/// Guidance for the `--seam` LD_PRELOAD shim: build the `.so`, then preload it
/// before a dynamically-linked target. Pure in `out` so the wording can be
/// asserted; `run` prints it after writing the shim.
fn seam_guidance(out: &Path) -> String {
    let dir = out.display();
    [
        "ok: Aristo C seam shim vendored (1 file).".to_string(),
        "    LD_PRELOAD fault seam — Linux, dynamically-linked targets only. Build".to_string(),
        "    the shim once, then preload it before the program (no SUT rebuild):".to_string(),
        format!(
            "      cc -std=c11 -shared -fPIC -D_GNU_SOURCE {dir}/aristo_seam.c -o {dir}/aristo_seam.so -ldl"
        ),
        format!(
            "      ARISTO_SEAM_FN=calloc ARISTO_SEAM_NTH=1 LD_PRELOAD={dir}/aristo_seam.so ./your_prog"
        ),
        "    Fails the Nth calloc/malloc with errno; all other calls forward to".to_string(),
        "    libc. The shim is test-only — never linked into the SUT.".to_string(),
    ]
    .join("\n")
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
    fn guidance_describes_out_of_tree_link_model() {
        // The exact wording is the spec: the out-of-tree recipe (build the
        // runtime once into libaristo.a, then link it) plus the both-flavors
        // -I note. Pinned byte-for-byte so a reword is a deliberate change.
        let expected = "\
ok: Aristo C runtime vendored (2 files).
    Out-of-tree link model — the runtime need not live in your source
    tree. Build it once into a static library, then link that:
      cc -std=c11 -DARISTO_INSTRUMENT -Iscratch -c scratch/aristo.c -o scratch/aristo.o
      ar rcs scratch/libaristo.a scratch/aristo.o
    Instrumented build: add -Iscratch (both flavors — ARISTO_TU_LOCAL pulls
    aristo.h in unconditionally), pass -DARISTO_INSTRUMENT, and link
    scratch/libaristo.a.";
        assert_eq!(guidance(std::path::Path::new("scratch")), expected);
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
        run(out.clone(), false, false).unwrap();

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
        run(out.clone(), false, false).unwrap();
        let h1 = fs::read_to_string(out.join("aristo.h")).unwrap();
        let c1 = fs::read_to_string(out.join("aristo.c")).unwrap();
        run(out.clone(), false, false).unwrap(); // second run must not change bytes
        assert_eq!(h1, fs::read_to_string(out.join("aristo.h")).unwrap());
        assert_eq!(c1, fs::read_to_string(out.join("aristo.c")).unwrap());
    }

    #[test]
    fn check_passes_on_freshly_vendored_runtime() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("aristo");
        run(out.clone(), false, false).unwrap();
        // --check on the just-written runtime is up to date (writes nothing).
        run(out, true, false).unwrap();
    }

    #[test]
    fn check_fails_on_missing_and_on_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("aristo");
        // Absent runtime -> stale (a missing file is never "up to date").
        assert!(
            run(out.clone(), true, false).is_err(),
            "--check must fail when the runtime is absent"
        );
        // Vendor, then hand-edit one file -> stale.
        run(out.clone(), false, false).unwrap();
        fs::write(out.join("aristo.c"), "/* hand-edited */\n").unwrap();
        assert!(
            run(out, true, false).is_err(),
            "--check must fail when a vendored file drifts from the CLI template"
        );
    }

    #[test]
    fn seam_vendors_shim_and_check_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("seam");
        // --seam writes aristo_seam.c only (not the in-process runtime pair).
        run(out.clone(), false, true).unwrap();
        assert!(out.join("aristo_seam.c").exists());
        assert!(
            !out.join("aristo.c").exists(),
            "--seam must not emit the in-process runtime"
        );
        // --seam --check is up to date on the fresh shim...
        run(out.clone(), true, true).unwrap();
        // ...and detects drift after a hand-edit.
        fs::write(out.join("aristo_seam.c"), "/* hand-edited */\n").unwrap();
        assert!(
            run(out, true, true).is_err(),
            "--seam --check must fail when the shim drifts from the template"
        );
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

    /// A tiny dynamically-linked program that calloc()s in a loop, touches and
    /// frees each block, and returns how many came back NULL. Under the preload
    /// shim that count is exactly 1 (the targeted Nth) or 0 (disabled); the
    /// touch+free of every forwarded block also proves forwarding returns real
    /// memory.
    const C_SEAM_CALLER: &str = r#"
#include <stdlib.h>
#include <string.h>
int main(void) {
    int nulls = 0;
    for (int i = 0; i < 2000; i++) {
        void *p = calloc(4, 8);
        if (!p) { nulls++; continue; }
        memset(p, 0xAB, 32);   /* touch: a bogus forwarded pointer would crash */
        free(p);
    }
    return nulls;
}
"#;

    #[test]
    #[cfg(target_os = "linux")]
    fn seam_shim_fails_nth_calloc_under_ld_preload() {
        let Some(cc) = find_c_compiler() else {
            eprintln!("skipping: no C compiler (cc/gcc/clang) on PATH");
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // Emit + build the shim into a shared object (the vendored recipe).
        fs::write(dir.join("aristo_seam.c"), resolve(ARISTO_SEAM_TEMPLATE)).unwrap();
        let so = dir.join("aristo_seam.so");
        let build_so = std::process::Command::new(cc)
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-shared",
                "-fPIC",
                "-D_GNU_SOURCE",
            ])
            .arg(dir.join("aristo_seam.c"))
            .arg("-o")
            .arg(&so)
            .arg("-ldl")
            .status()
            .unwrap();
        assert!(
            build_so.success(),
            "seam shim failed to compile as a shared object ({cc})"
        );

        // Build the dynamically-linked caller (default linkage).
        fs::write(dir.join("caller.c"), C_SEAM_CALLER).unwrap();
        let caller = dir.join("caller");
        let build_caller = std::process::Command::new(cc)
            .arg("-std=c11")
            .arg(dir.join("caller.c"))
            .arg("-o")
            .arg(&caller)
            .status()
            .unwrap();
        assert!(
            build_caller.success(),
            "seam caller failed to compile ({cc})"
        );

        // Fault: fail the 200th calloc — well past any startup callocs, well
        // within the 2000-iteration loop → exactly one NULL.
        let faulted = std::process::Command::new(&caller)
            .env("LD_PRELOAD", &so)
            .env("ARISTO_SEAM_FN", "calloc")
            .env("ARISTO_SEAM_NTH", "200")
            .status()
            .unwrap();
        assert_eq!(
            faulted.code(),
            Some(1),
            "preload should fail exactly one calloc (the 200th); got {:?}",
            faulted.code()
        );

        // Disabled: preloaded but no ARISTO_SEAM_FN → every calloc forwards,
        // zero NULLs, and touch+free of each forwarded block must not crash.
        let disabled = std::process::Command::new(&caller)
            .env("LD_PRELOAD", &so)
            .status()
            .unwrap();
        assert_eq!(
            disabled.code(),
            Some(0),
            "preloaded-but-disabled shim must forward every calloc cleanly; got {:?}",
            disabled.code()
        );
    }
}
