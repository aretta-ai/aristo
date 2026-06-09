//! Multiple `#[inspect(...)]` fields in one struct, mixing clone and
//! project modes. Untagged fields are silently ignored (no accessor
//! emitted, no error).

use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

#[derive(Clone)]
pub struct A;
pub struct B;
pub struct BSnap;
impl From<&B> for BSnap {
    fn from(_: &B) -> Self {
        BSnap
    }
}

#[derive(Inspect)]
pub struct Store {
    // Clone mode: V is Clone, snapshot is `Vec<(u64, A)>`.
    #[inspect]
    alphas: SkipMap<u64, A>,

    // Project mode: snapshot is `Vec<(u32, BSnap)>` via user's `From`.
    #[inspect(BSnap)]
    betas: SkipMap<u32, B>,

    // Untagged field — ignored by the derive.
    #[allow(dead_code)]
    other: u64,
}

fn main() {
    let s = Store {
        alphas: SkipMap::new(),
        betas: SkipMap::new(),
        other: 0,
    };
    let _alphas: Vec<(u64, A)> = s.inspect_alphas();
    let _betas: Vec<(u32, BSnap)> = s.inspect_betas();
}
