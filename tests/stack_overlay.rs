use game::ui::stack_overlay::GUI_SCALE;
use game::ui::stack_overlay::count_frame;
use game::ui::stack_overlay::count_origin;
use game::ui::stack_overlay::durability_track;

#[test]
fn count_digits_share_the_slot_bottom_right() {
    let icon = 16.0 * GUI_SCALE;
    let frame = count_frame(4.0, 8.0);
    // Glyph cell ends one GUI pixel past the 16×16 icon.
    assert_eq!(frame.right(), 4.0 + icon + GUI_SCALE);
    assert_eq!(frame.bottom(), 8.0 + icon + GUI_SCALE);

    // Advances match minecraft.otf digits at the 8px Beta font scaled by GUI_SCALE.
    let one = count_origin(4.0, 8.0, 12.0);
    let sixty_four = count_origin(4.0, 8.0, 24.0);
    assert_eq!(one.1, frame.top);
    assert_eq!(sixty_four.1, frame.top);
    assert_eq!(one.0 + 12.0, frame.right());
    assert_eq!(sixty_four.0 + 24.0, frame.right());
    assert!(one.0 > sixty_four.0);
}

#[test]
fn durability_bar_starts_at_the_beta_overlay_origin() {
    let (left, top, width, height) = durability_track(4.0, 8.0, false);
    let (fill_left, fill_top, _, fill_height) = durability_track(4.0, 8.0, true);
    assert_eq!((left, top), (4.0 + 2.0 * GUI_SCALE, 8.0 + 13.0 * GUI_SCALE));
    assert_eq!(width, 13.0 * GUI_SCALE);
    assert_eq!(height, 2.0 * GUI_SCALE);
    assert_eq!((fill_left, fill_top), (left, top));
    assert_eq!(fill_height, GUI_SCALE);
}
