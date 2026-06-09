//! `yield_point!("...")` is a function-like macro that expands to a
//! call into `aristo::instrument::__yield_point` (the runtime hook
//! defined in the meta-crate). With no hook installed, the call is a
//! no-op. With a hook installed via `aristo::instrument::set_hook`, the
//! hook receives the literal label.
//!
//! This trybuild fixture only verifies compilation; the round-trip
//! (set_hook → yield_point! → hook receives label) is exercised by the
//! aristo crate's integration test at
//! `crates/aristo/tests/yield_point_dispatch.rs`.

use aristo::instrument::yield_point;

fn write_header() {
    let _version: u32 = 7;
    yield_point!("write_header.before_fsync");
}

fn write_record(_idx: usize) {
    // Multiple labels at the same source position each emit
    // independently; the runtime hook is called once per label.
    yield_point!("write_record.before_validate");
    yield_point!("write_record.after_validate");
}

fn main() {
    write_header();
    write_record(0);
}
