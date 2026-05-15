//! Aristo SDK shared library.
//!
//! Houses protocol logic shared between the proc-macro crate and the CLI:
//! - `.aristo/index.toml` schema and IO
//! - B5b four-check verification pipeline (signature → identity → content → ancestry)
//! - Bundled server public-key registry
//! - `LanguageSyntax` registry (per K5)
//!
//! Public API is empty during initial bootstrap.
