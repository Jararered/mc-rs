use game::crafting::CraftingGrid;
use game::crafting::WorkbenchSession;
use game::item::Item;

use super::stack;

#[test]
fn workbench_session_matches_beta_reach_limit() {
    let session = WorkbenchSession::new((4, 8, -2));
    assert!(session.clone().within_reach((4.5, 8.5, -2.5)));
    assert!(!session.within_reach((13.0, 8.5, -2.5)));
}

#[test]
fn draining_a_crafting_grid_removes_all_inputs() {
    let mut grid = CraftingGrid::workbench();
    grid.set(0, 0, Some(stack(Item::WoodenPickaxe, 1)));
    grid.set(2, 2, Some(stack(Item::Stick, 3)));

    let drained = grid.drain();

    assert_eq!(drained.len(), 2);
    assert_eq!(grid.occupied(), 0);
    assert!(grid.slots().all(|slot| slot.is_none()));
}
