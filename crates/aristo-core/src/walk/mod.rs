//! Source walker — discover Aristo annotations in Rust source.
//!
//! Two layers ship in slice 14:
//!
//! - [`extract`]: parse a single Rust source string with `syn` and pull
//!   out every annotation it contains. The unit of work is a single file's
//!   text; no filesystem touched.
//!
//! - [`fs`] (slice 14C): walk a directory tree, find `.rs` files, feed
//!   each through [`extract`], and aggregate.
//!
//! The split lets every annotation-extraction rule be unit-tested against
//! string fixtures (no tempdir gymnastics) while the filesystem layer gets
//! its own focused tests for things like `target/` exclusion.

pub mod extract;

pub use extract::{
    extract_from_source, AnnotationForm, ExtractError, ExtractedAnnotation, ParentRaw,
};
