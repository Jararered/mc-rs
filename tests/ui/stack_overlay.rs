use game::app::settings::DEFAULT_GUI_SCALE;
use game::ui::icons::overlay::count_frame;
use game::ui::icons::overlay::count_origin;
use game::ui::icons::overlay::durability_track;

const SCALES: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

#[test]
fn count_digits_share_the_slot_bottom_right() {
    for scale in SCALES {
        let icon = 16.0 * scale;
        let frame = count_frame(scale, 4.0, 8.0);
        // Glyph cell ends one GUI pixel past the 16×16 icon.
        assert_eq!(frame.right(), 4.0 + icon + scale);
        assert_eq!(frame.bottom(), 8.0 + icon + scale);

        // Advances match minecraft.otf digits at the 8px Beta font, scaled.
        let one = count_origin(scale, 4.0, 8.0, 6.0 * scale);
        let sixty_four = count_origin(scale, 4.0, 8.0, 12.0 * scale);
        assert_eq!(one.1, frame.top);
        assert_eq!(sixty_four.1, frame.top);
        assert_eq!(one.0 + 6.0 * scale, frame.right());
        assert_eq!(sixty_four.0 + 12.0 * scale, frame.right());
        assert!(one.0 > sixty_four.0);
    }
}

#[test]
fn durability_bar_starts_at_the_beta_overlay_origin() {
    for scale in SCALES {
        let (left, top, width, height) = durability_track(scale, 4.0, 8.0, false);
        let (fill_left, fill_top, _, fill_height) = durability_track(scale, 4.0, 8.0, true);
        assert_eq!((left, top), (4.0 + 2.0 * scale, 8.0 + 13.0 * scale));
        assert_eq!(width, 13.0 * scale);
        assert_eq!(height, 2.0 * scale);
        assert_eq!((fill_left, fill_top), (left, top));
        assert_eq!(fill_height, scale);
    }
}

#[test]
fn unchanged_stack_labels_do_not_invalidate_text_or_layout() {
    use bevy::prelude::*;
    use bevy::text::LineHeight;
    use game::ui::icons::overlay::sync_stack_label;

    #[derive(Resource)]
    struct Label(String, f32);
    #[derive(Resource, Default)]
    struct ChangedParts(usize);

    fn sync(
        label: Res<Label>,
        mut query: Query<(
            &mut Text,
            &mut Node,
            &mut TextLayout,
            &mut TextFont,
            &mut LineHeight,
            &mut TextShadow,
        )>,
    ) {
        for (mut text, mut node, mut layout, mut font, mut height, mut shadow) in &mut query {
            sync_stack_label(
                DEFAULT_GUI_SCALE,
                &mut text,
                &mut node,
                &mut layout,
                &mut font,
                &mut height,
                &mut shadow,
                &Handle::default(),
                label.1,
                0.0,
                &label.0,
                true,
            );
        }
    }
    fn observe(
        query: Query<
            (),
            Or<(
                Changed<Text>,
                Changed<Node>,
                Changed<TextLayout>,
                Changed<TextFont>,
                Changed<LineHeight>,
                Changed<TextShadow>,
            )>,
        >,
        mut changes: ResMut<ChangedParts>,
    ) {
        changes.0 = query.iter().count();
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(Label("64".into(), 0.0))
        .init_resource::<ChangedParts>()
        .add_systems(Update, sync)
        .add_systems(Last, observe);
    let entity = app
        .world_mut()
        .spawn((
            Text::default(),
            Node::default(),
            TextLayout::default(),
            TextFont::default(),
            LineHeight::default(),
            TextShadow::default(),
        ))
        .id();
    app.update();
    assert_eq!(app.world().resource::<ChangedParts>().0, 1);
    app.update();
    assert_eq!(app.world().resource::<ChangedParts>().0, 0);
    app.world_mut().resource_mut::<Label>().0 = "63".into();
    app.update();
    assert_eq!(app.world().get::<Text>(entity).unwrap().0, "63");
    assert_eq!(app.world().resource::<ChangedParts>().0, 1);
    app.update();
    assert_eq!(app.world().resource::<ChangedParts>().0, 0);
    app.world_mut().resource_mut::<Label>().1 = 20.0;
    app.update();
    assert_eq!(app.world().get::<Node>(entity).unwrap().left, px(20.0));
    assert_eq!(app.world().resource::<ChangedParts>().0, 1);
}
