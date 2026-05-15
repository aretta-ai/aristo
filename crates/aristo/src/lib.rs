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
