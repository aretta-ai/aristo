#[aristo::intent("the answer is forty-two", verify = "test", id = "returns_forty_two")]
fn answer() -> i32 {
    42
}
