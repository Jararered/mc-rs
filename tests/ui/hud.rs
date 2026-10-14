use game::ui::screens::hud::FpsCounter;

#[test]
fn fps_counter_reports_each_whole_second() {
    let mut fps = FpsCounter::default();
    // Nothing is shown until a second has passed.
    for _ in 0..59 {
        assert_eq!(fps.frame(1.0 / 60.0), 0);
    }
    assert_eq!(fps.frame(1.0 / 60.0 + 0.001), 60);
    // The figure holds through the next second.
    for _ in 0..29 {
        assert_eq!(fps.frame(1.0 / 30.0), 60);
    }
    assert_eq!(fps.frame(1.0 / 30.0 + 0.001), 30);
}
