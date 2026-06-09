//! `#[expose_pub]` on a `pub(crate)` struct raises its visibility to
//! `pub` in place. Same rules as the enum form.

mod inner {
    use aristo::instrument::expose_pub;

    #[expose_pub]
    pub(crate) struct Frame {
        pub seq: u64,
        pub payload: Vec<u8>,
    }
}

fn main() {
    let f = inner::Frame {
        seq: 1,
        payload: vec![],
    };
    assert_eq!(f.seq, 1);
    assert!(f.payload.is_empty());
}
