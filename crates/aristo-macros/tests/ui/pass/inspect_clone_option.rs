//! Clone mode accepts `Option<T>`. The pre-0.3.0 `is_scalar_field`
//! allowlist wrongly rejected generic-bearing paths like `Option<u32>`
//! even though `Clone` suffices; the type-agnostic clone arm accepts it,
//! deferring the `Clone` bound to rustc.

use aristo::instrument::Inspect;

#[derive(Inspect)]
struct Log {
    #[inspect]
    pending_crc: Option<u32>,
}

fn main() {
    let l = Log {
        pending_crc: Some(7),
    };
    let snap: Option<u32> = l.inspect_pending_crc();
    assert_eq!(snap, Some(7));
}
