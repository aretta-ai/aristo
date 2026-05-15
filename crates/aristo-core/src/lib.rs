//! Aristo SDK shared library.
//!
//! Houses protocol logic shared between the proc-macro crate and the CLI:
//! - `.aristo/index.toml` schema and IO (see [`index`])
//! - B5b four-check verification pipeline (signature → identity → content → ancestry)
//! - Bundled server public-key registry
//! - `LanguageSyntax` registry (per K5)
//!
//! ## Design: serde-first, schema-derived
//!
//! Types in this crate are the canonical source of truth for every Aristo
//! file format. Validation falls out of `serde::Deserialize` via newtype
//! patterns — invalid values fail at parse time with a structural error.
//! Cross-language consumers receive a JSON Schema derived from these types
//! via `schemars` (see the `dump-schemas` example in a later slice).

pub mod config;
pub mod index;
pub mod spec;
