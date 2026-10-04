//! Reusable mouse-driven, stepped UI sliders. Values are independent of settings.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;

const THUMB_WIDTH: f32 = 16.0;

#[derive(Component, Debug, Clone, Copy)]
pub struct Slider {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    value: f32,
}

impl Slider {
    pub fn new(min: f32, max: f32, step: f32, value: f32) -> Self {
        assert!(min.is_finite() && max.is_finite() && max > min);
        assert!(step.is_finite() && step > 0.0);
        Self {
            min,
            max,
            step,
            value: value.clamp(min, max),
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn fraction(&self) -> f32 {
        (self.value - self.min) / (self.max - self.min)
    }

    /// Returns whether the snapped value changed.
    pub fn set_fraction(&mut self, fraction: f32) -> bool {
        if !fraction.is_finite() {
            return false;
        }
        let raw = self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min);
        let value = (self.min + ((raw - self.min) / self.step).round() * self.step)
            .clamp(self.min, self.max);
        if (value - self.value).abs() < self.step * 0.0001 {
            return false;
        }
        self.value = value;
        true
    }
}

#[derive(Resource, Default)]
pub struct SliderDrag(pub Option<Entity>);

/// Optional textured thumb skin, including its hovered atlas region.
#[derive(Component, Clone)]
pub struct SliderSkin {
    pub image: ImageNode,
    pub hovered_rect: Rect,
}

#[derive(Component)]
pub struct SliderThumb(pub Entity);

/// Run after UI focus has supplied Interaction and RelativeCursorPosition.
pub fn update_sliders(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut drag: ResMut<SliderDrag>,
    mut sliders: Query<(
        Entity,
        &mut Slider,
        &Interaction,
        &RelativeCursorPosition,
        &ComputedNode,
        &mut BackgroundColor,
    )>,
    mut thumbs: Query<(
        &SliderThumb,
        &mut Node,
        Option<&SliderSkin>,
        Option<&mut ImageNode>,
    )>,
) {
    let focused = windows.iter().all(|window| window.focused);
    if !mouse.pressed(MouseButton::Left) || !focused {
        drag.0 = None;
    }
    if drag.0.is_some_and(|entity| sliders.get(entity).is_err()) {
        drag.0 = None;
    }
    for (entity, mut slider, interaction, cursor, computed, mut color) in &mut sliders {
        if focused
            && mouse.just_pressed(MouseButton::Left)
            && *interaction == Interaction::Pressed
            && cursor.cursor_over()
        {
            drag.0 = Some(entity);
        }
        let width = computed.size().x * computed.inverse_scale_factor();
        let travel = (width - THUMB_WIDTH).max(0.0);
        if drag.0 == Some(entity)
            && travel > 0.0
            && let Some(position) = cursor.normalized
        {
            let fraction = ((position.x + 0.5) * width - THUMB_WIDTH / 2.0) / travel;
            // Do not mark the component changed for an unchanged snapped value.
            let mut next = *slider;
            if next.set_fraction(fraction) {
                *slider = next;
            }
        }
        color.set_if_neq(BackgroundColor(
            if *interaction == Interaction::None && drag.0 != Some(entity) {
                Color::srgb(0.25, 0.25, 0.25)
            } else {
                Color::srgb(0.36, 0.36, 0.46)
            },
        ));
    }
    for (thumb, mut node, skin, image) in &mut thumbs {
        if let Ok((_, slider, interaction, _, computed, _)) = sliders.get(thumb.0) {
            if let (Some(skin), Some(mut image)) = (skin, image) {
                let rect = if *interaction != Interaction::None || drag.0 == Some(thumb.0) {
                    Some(skin.hovered_rect)
                } else {
                    skin.image.rect
                };
                if image.rect != rect {
                    image.rect = rect;
                }
            }
            let width = computed.size().x * computed.inverse_scale_factor();
            let left = px(slider.fraction() * (width - THUMB_WIDTH).max(0.0));
            if node.left != left {
                node.left = left;
            }
        }
    }
}

/// Spawn a full-width slider; callers can attach a centered text child and binding.
pub fn spawn_slider(
    parent: &mut ChildSpawnerCommands,
    slider: Slider,
    skin: Option<SliderSkin>,
) -> Entity {
    let mut control = parent.spawn((
        Button,
        slider,
        RelativeCursorPosition::default(),
        Node {
            width: percent(100),
            min_width: px(0),
            max_width: px(520),
            height: px(40),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
    ));
    let entity = control.id();
    control.with_children(|children| {
        let mut thumb = children.spawn((
            SliderThumb(entity),
            FocusPolicy::Pass,
            Node {
                position_type: PositionType::Absolute,
                width: px(THUMB_WIDTH),
                height: percent(100),
                left: px(0),
                border: UiRect::all(px(2)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.57, 0.57, 0.72)),
            BorderColor::all(Color::srgb(0.75, 0.75, 0.85)),
        ));
        if let Some(skin) = skin {
            thumb.insert((skin.image.clone(), skin));
        }
    });
    entity
}
