#[aristo::intent("alpha claim", verify = "test", id = "alpha")]
fn a() {}

#[aristo::intent("bravo claim", verify = "neural", id = "bravo")]
fn b() {}

#[aristo::assume("external invariant", id = "charlie")]
fn c() {}
