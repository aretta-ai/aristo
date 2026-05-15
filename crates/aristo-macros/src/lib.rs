//! Aristo proc-macros.
//!
//! Intentionally thin: this crate runs during downstream compile time, so
//! heavy work (project-wide cycle detection, B5b signature validation, index
//! IO) lives in `aristo-cli`. The proc-macros here only do single-annotation
//! validation (when the `aristo_check` cargo feature is on) and `include_str!`
//! injection (when `aristo_doc` is on). No macros exported during bootstrap.
