#[aristo::intent(
    "A bare-id parent intent used as source for several error cases.",
    verify = "test",
    id = "parent_id"
)]
fn parent_fn() {}

#[aristo::intent(
    "A bare-id intent that already occupies the `taken_target` slot.",
    verify = "test",
    id = "taken_target"
)]
fn taken_fn() {}

#[aristo::intent(
    "An opaque stamp-assigned id; renaming to a readable id is the F1-c promotion path.",
    verify = "test",
    id = "post_validator"
)]
fn opaque_fn() {}
