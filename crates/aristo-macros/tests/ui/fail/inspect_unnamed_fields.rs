//! `#[derive(Inspect)]` requires a struct with named fields. Tuple
//! structs are rejected because the codegen needs field names for the
//! accessor methods. Bare unit structs are rejected too (no fields at
//! all to inspect).

use aristo::instrument::Inspect;

#[derive(Inspect)]
pub struct Tuple(u64, u32);

fn main() {}
