#[aristo::intent(
    "Parent invariant: every balance op preserves the cell uniqueness contract.",
    verify = "test",
    id = "balance_no_duplicate_cells"
)]
fn balance_non_root() {}

#[aristo::intent(
    "Each cell is written exactly once during edit_page.",
    verify = "test",
    parent = "balance_no_duplicate_cells",
    id = "edit_page_writes_each_cell_once"
)]
fn edit_page() {}

#[aristo::intent(
    "Cell array indices stay in bounds across rebalance operations.",
    verify = "test",
    parent = ["balance_no_duplicate_cells", "edit_page_writes_each_cell_once"],
    id = "cell_array_indices_in_bounds"
)]
fn cell_array_indices() {}
