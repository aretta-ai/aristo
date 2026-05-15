//! Argument-shape coverage: bool / string `verify` values; singular and
//! list `parent`; explicit `id`. All four user-writable shapes from
//! mockup 01.

use aristo::intent;

#[intent("text only — no other args")]
fn a() -> i32 {
    1
}

#[intent("verify=true (bool)", verify = true)]
fn b() -> i32 {
    2
}

#[intent("verify=false (bool — documentation-only)", verify = false)]
fn c() -> i32 {
    3
}

#[intent("verify=\"neural\"", verify = "neural")]
fn d() -> i32 {
    4
}

#[intent("verify=\"test\"", verify = "test")]
fn e() -> i32 {
    5
}

#[intent("verify=\"full\"", verify = "full")]
fn f() -> i32 {
    6
}

#[intent("singular parent", verify = "test", parent = "ancestor")]
fn g() -> i32 {
    7
}

#[intent("list parent", verify = "test", parent = ["ancestor_a", "ancestor_b"])]
fn h() -> i32 {
    8
}

#[intent("explicit id", verify = "test", id = "fully_named")]
fn i() -> i32 {
    9
}

#[intent(
    "all four args",
    verify = "test",
    parent = ["root", "summation"],
    id = "kitchen_sink"
)]
fn j() -> i32 {
    10
}

fn main() {
    assert_eq!(a() + b() + c() + d() + e() + f() + g() + h() + i() + j(), 55);
}
