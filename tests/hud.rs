use game::ui::screens::hud::hotbar_item_rect;
use game::ui::screens::hud::hotbar_selector_left;

#[test]
fn hotbar_icon_positions_follow_the_scaled_twenty_pixel_slot_stride() {
    assert_eq!(hotbar_item_rect(0), (6.0, 6.0));
    assert_eq!(hotbar_item_rect(1), (46.0, 6.0));
    assert_eq!(hotbar_item_rect(8), (326.0, 6.0));
}

#[test]
fn hotbar_selector_tracks_slots_and_clamps_out_of_range_indices() {
    assert_eq!(hotbar_selector_left(0), -2.0);
    assert_eq!(hotbar_selector_left(1), 38.0);
    assert_eq!(hotbar_selector_left(8), 318.0);
    assert_eq!(hotbar_selector_left(99), hotbar_selector_left(8));
}
