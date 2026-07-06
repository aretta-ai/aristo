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

pub mod count;
pub mod extract;
pub mod extract_c;
pub mod fs;
pub mod scan_ids;

pub use count::{
    count_fns_in_source, count_fns_per_module, count_fns_per_module_with, FnCountByModule,
};
pub use extract::{
    extract_from_source, AnnotationForm, ExtractError, ExtractedAnnotation, ParentRaw,
};
pub use extract_c::extract_from_c_source;
pub use fs::{
    walk_directory, walk_directory_with, walk_for_freshness, walk_for_freshness_with,
    DiscoveredAnnotation, FsWalkError, WalkOptions,
};
pub use scan_ids::{scan_id_occurrences, IdOccurrence, IdOccurrenceKind};
