//! `#[aristo::assume]` across the same surfaces as `intent` — minus
//! `verify`, per A5.

#[aristo::assume("OS guarantees mmap pages are zero-initialized")]
fn zero_init_pages() -> u8 {
    0
}

#[aristo::assume("buffer holder; caller serializes access externally")]
struct SharedBuffer {
    bytes: Vec<u8>,
}

#[aristo::assume("upstream caller has acquired the read lock")]
impl SharedBuffer {
    fn new() -> Self {
        Self { bytes: vec![1, 2, 3] }
    }

    #[aristo::assume("first byte is well-defined under our access discipline")]
    fn first(&self) -> u8 {
        self.bytes[0]
    }
}

#[aristo::assume("trait whose impls promise interior mutability is sound")]
trait InteriorMutable {
    fn touch(&self);
}

#[aristo::assume("SharedBuffer has no interior mutability")]
impl InteriorMutable for SharedBuffer {
    fn touch(&self) {
        let _ = self.bytes.len();
    }
}

#[aristo::assume("byte alias for clarity")]
type Byte = u8;

#[aristo::assume("module groups concurrency contracts")]
mod concurrency {
    use aristo::assume;

    #[assume("returns the same value on every call")]
    pub fn pure_constant() -> i32 {
        99
    }
}

fn main() {
    assert_eq!(zero_init_pages(), 0);
    let buf = SharedBuffer::new();
    assert_eq!(buf.first(), 1);
    buf.touch();
    let _: Byte = 7;
    assert_eq!(concurrency::pure_constant(), 99);
}
