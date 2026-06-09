//! Aristo SDK — annotation macros and verification.
//!
//! This is the meta-crate. Downstream users add `aristo` to their `Cargo.toml`
//! and receive the proc-macros (`#[aristo::intent]`, `#[aristo::assume]`,
//! `aristo::intent_stmt!()`, `aristo::assume_stmt!()`) via re-export. Shared
//! types from `aristo-core` are re-exported as that crate's public API
//! stabilizes.
//!
//! Why a meta-crate: per K2 / K3 the canonical implementation is the Rust
//! `aristo-cli` binary; the proc-macro work happens in `aristo-macros`; the
//! types live in `aristo-core`. Downstream users should not need to learn
//! that split — they depend on `aristo` and get everything they need.

pub use aristo_macros::{assume, assume_stmt, intent, intent_stmt};

/// `aristo::instrument` — Phase 2 surface for making private state
/// observable to verification harnesses. Three proc-macros (`Inspect`,
/// `expose_pub`, `yield_point`) re-exported from `aristo-macros`, plus
/// a thread-local runtime hook (`set_hook` + `__yield_point`) defined
/// inline because a `proc-macro = true` crate can't export non-macro
/// items and `aristo-core` already depends on this crate.
///
/// Gated by the `aristo_instrument` cargo feature; off by default. See
/// `docs/ROADMAP.md` Phase 2.
#[cfg(feature = "aristo_instrument")]
pub mod instrument;
