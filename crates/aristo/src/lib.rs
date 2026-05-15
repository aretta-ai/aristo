//! Aristo SDK — annotation macros and verification.
//!
//! This is the meta-crate. Downstream users add `aristo` to their `Cargo.toml`
//! and receive the proc-macros (`#[aristo::intent]`, `#[aristo::assume]`) plus
//! shared types via re-export. Public API is empty during initial bootstrap;
//! re-exports are added as the underlying crates land their surface.
