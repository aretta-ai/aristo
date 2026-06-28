//! `name = "..."` overrides the method suffix in clone mode: the field
//! `pin_count` is exposed as `inspect_pins`, not `inspect_pin_count`.

use aristo::instrument::Inspect;

#[derive(Inspect)]
struct Counters {
    #[inspect(name = "pins")]
    pin_count: u32,
}

fn main() {
    let c = Counters { pin_count: 5 };
    let snap: u32 = c.inspect_pins();
    assert_eq!(snap, 5);
}
