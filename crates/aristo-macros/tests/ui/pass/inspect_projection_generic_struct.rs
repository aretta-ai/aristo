//! Projection mode on a struct whose field type mentions the struct's own
//! generic parameters. The generated `let __project: &dyn Fn(&BTreeMap<K,
//! V>) -> usize` ascription names `K`/`V`, which an inner helper `fn`
//! could not — this guards the generic-safety of the `let`-binding choice.
//! The closure is left un-annotated, so it also exercises parameter
//! inference flowing from the binding into the closure under generics.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store<K, V> {
    #[inspect(ret = usize, with = |m| m.len())]
    map: BTreeMap<K, V>,
}

fn main() {
    let mut map = BTreeMap::new();
    map.insert(1u64, "x".to_string());
    let s = Store { map };
    assert_eq!(s.inspect_map(), 1usize);
}
