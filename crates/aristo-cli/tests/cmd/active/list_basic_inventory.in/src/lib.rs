#[aristo::intent("first claim", verify = "test", id = "alpha")]
fn a() {}

#[aristo::intent("second claim", verify = "full", id = "bravo")]
fn b() {}

#[aristo::assume("external invariant", id = "charlie")]
fn c() {}
