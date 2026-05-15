//! The canonical mockup-01 example: an attribute on the function PLUS
//! sub-item `intent_stmt!` annotations on internal loops/statements.

use aristo::{intent, intent_stmt};

#[intent(
    "balance_non_root preserves cell ordering across the rebalance",
    verify = "test",
    id = "balance_preserves_order"
)]
fn balance_non_root(cells: &mut [u32]) -> u32 {
    intent_stmt!(
        "the cumulative-counts array is monotonic non-decreasing",
        verify = "test",
        id = "cumulative_monotonic",
        parent = "balance_preserves_order"
    );
    let mut cumulative = vec![0u32; cells.len() + 1];
    for (i, &c) in cells.iter().enumerate() {
        intent_stmt!("each cell contributes its own count exactly once");
        cumulative[i + 1] = cumulative[i] + c;
    }
    *cumulative.last().unwrap()
}

fn main() {
    assert_eq!(balance_non_root(&mut [1, 2, 3, 4]), 10);
    assert_eq!(balance_non_root(&mut []), 0);
}
