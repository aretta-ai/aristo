#[aristo::intent("café stays valid", verify = "test", id = "rust_root")]
fn work() {
    if true {
        aristo::intent_stmt!("duplicate claim", parent = "rust_root");
        let first = 1;
        aristo::intent_stmt!("duplicate claim", parent = "rust_root");
        let second = 2;
    }
}
