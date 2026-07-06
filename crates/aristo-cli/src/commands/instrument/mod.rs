//! `aristo instrument` — manage the C instrumentation artifacts a SUT vendors.
//!
//! Aristo's instrument surface for Rust is proc-macros; C has no macro engine,
//! so we reproduce their OUTPUT — a small vendored runtime (`aristo.h` +
//! `aristo.c`) the SUT links, plus (later) a code generator. `vendor-c` emits
//! the runtime; everything is gated by `-DARISTO_INSTRUMENT`.

pub(crate) mod gen_c;
pub(crate) mod vendor_c;

use std::fs;
use std::path::Path;

use crate::CliResult;

/// Outcome of writing a generated / vendored file, so callers emit the right
/// created / updated / unchanged notice.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WriteOutcome {
    Created,
    Updated,
    Unchanged,
}

#[aristo::intent(
    "Re-emitting identical content leaves the file byte-identical and returns \
     Unchanged; Created (file absent) and Updated (content differed) are the \
     other two outcomes. Idempotence is the Unchanged case specifically — a \
     re-run on up-to-date output must not rewrite the file, which would churn \
     its mtime and dirty a clean tree. Shared by vendor-c and gen-c.",
    verify = "test",
    id = "instrument_write_is_idempotent"
)]
pub(crate) fn write_file(path: &Path, content: &str) -> CliResult<WriteOutcome> {
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

/// ABI version of the vendored C runtime + generated-code contract, and the
/// single source of truth for it: it is substituted into `runtime/aristo.h.in`
/// (the `#define ARISTO_ABI` and its `_Static_assert`) at vendor time, so the
/// header cannot drift. Bump on ANY layout change to `aristo_decision`, the
/// hook typedefs, or the macro signatures — a silent drift is an
/// `aristo_decision` layout mismatch in the fault path, the hardest place to
/// notice corruption.
pub(crate) const ARISTO_ABI: u32 = 1;
