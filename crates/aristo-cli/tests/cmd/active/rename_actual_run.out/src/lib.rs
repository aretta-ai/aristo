#[aristo::intent(
    "Parent invariant: every balance op preserves the cell uniqueness contract.",
    verify = "test",
    id = "balance_op_unique_cells"
)]
fn balance_non_root() {}

#[aristo::intent(
    "Each cell is written exactly once during edit_page.",
    verify = "test",
    parent = "balance_op_unique_cells",
    id = "edit_page_writes_each_cell_once"
)]
fn edit_page() {}
