#[aristo::intent("holder stores a value", id = "holder")]
struct Holder { value: i32 }
impl Holder {
    #[aristo::intent("getter returns the value", verify = r#"test"#, parent = ["rust_root", "c_external"], id = "getter")]
    fn get(&self) -> i32 { self.value }
}
trait Read {
    #[aristo::intent("default returns one", id = "read_default")]
    fn default(&self) -> i32 { 1 }
    #[aristo::intent("required returns a value", id = "read_required")]
    fn required(&self) -> i32;
}
#[aristo::assume("platform is available", id = "platform")]
fn nested() {
    let inspect = || match 1 {
        _ => {
            aristo::intent_stmt!("nested work completes");
            1
        }
    };
}
