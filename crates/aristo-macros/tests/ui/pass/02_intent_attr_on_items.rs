//! `#[aristo::intent]` on non-fn items: struct, impl, trait, trait impl,
//! type alias, inline module, plus a method inside an impl block.

#[aristo::intent("a tiny number holder")]
struct Holder {
    value: i32,
}

#[aristo::intent("inherent impl on Holder")]
impl Holder {
    #[aristo::intent("constructor preserves the input value verbatim")]
    fn new(value: i32) -> Self {
        Self { value }
    }

    #[aristo::intent("getter returns what new() stored")]
    fn get(&self) -> i32 {
        self.value
    }
}

#[aristo::intent("things that can be doubled")]
trait Doublable {
    fn doubled(&self) -> i32;
}

#[aristo::intent("Holder doubles by integer multiplication")]
impl Doublable for Holder {
    fn doubled(&self) -> i32 {
        self.value * 2
    }
}

#[aristo::intent("alias clarifies that this is a small int")]
type SmallInt = i32;

#[aristo::intent("module groups math utilities")]
mod math {
    use aristo::intent;

    #[intent("squares its input")]
    pub fn sq(x: i32) -> i32 {
        x * x
    }
}

fn main() {
    let h = Holder::new(5);
    assert_eq!(h.get(), 5);
    assert_eq!(h.doubled(), 10);
    let _: SmallInt = 9;
    assert_eq!(math::sq(4), 16);
}
