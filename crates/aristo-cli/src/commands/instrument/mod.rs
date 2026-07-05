//! `aristo instrument` — manage the C instrumentation artifacts a SUT vendors.
//!
//! Aristo's instrument surface for Rust is proc-macros; C has no macro engine,
//! so we reproduce their OUTPUT — a small vendored runtime (`aristo.h` +
//! `aristo.c`) the SUT links, plus (later) a code generator. `vendor-c` emits
//! the runtime; everything is gated by `-DARISTO_INSTRUMENT`.

pub(crate) mod vendor_c;

/// ABI version of the vendored C runtime + generated-code contract, and the
/// single source of truth for it: it is substituted into `runtime/aristo.h.in`
/// (the `#define ARISTO_ABI` and its `_Static_assert`) at vendor time, so the
/// header cannot drift. Bump on ANY layout change to `aristo_decision`, the
/// hook typedefs, or the macro signatures — a silent drift is an
/// `aristo_decision` layout mismatch in the fault path, the hardest place to
/// notice corruption.
pub(crate) const ARISTO_ABI: u32 = 1;
