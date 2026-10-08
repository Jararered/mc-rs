//! What a Nether portal puts on screen: the purple wash that builds while the
//! player stands in one (`GuiIngame.renderPortalOverlay`), and the notice
//! shown while the other dimension loads (`changeWorld`'s "Entering the
//! Nether").

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::text::FontSize;
use bevy::text::FontSource;

use crate::app::session::WorldSession;
use crate::app::state::AppScreen;
use crate::player::Player;
use crate::player::portal::PortalTravel;
use crate::player::portal::overlay_alpha;
use crate::rendering::textures::PortalTexture;
use crate::ui::icons::overlay::UiFont;
use crate::world::dimension::Dimension;
use crate::world::tick::WorldTick;

pub struct PortalUiPlugin;

impl Plugin for PortalUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Playing), spawn)
            .add_systems(OnExit(AppScreen::Playing), despawn)
            .add_systems(
                Update,
                (update_overlay, update_travel_notice).run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Component)]
struct PortalUiRoot;

/// The portal tile stretched over the screen, with the frames that animate it.
#[derive(Component)]
struct PortalOverlay {
    frames: PortalTexture,
    image: Handle<Image>,
}

#[derive(Component)]
struct TravelNotice;

#[derive(Component)]
struct TravelNoticeText;

fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>, font: Res<UiFont>) {
    let frames = PortalTexture::new();
    let image = images.add(Image::new(
        Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        frames.rgba().to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    // Beta draws the wash before the hotbar, so it sits under the HUD.
    commands.spawn((
        PortalUiRoot,
        Pickable::IGNORE,
        GlobalZIndex(-1),
        Visibility::Hidden,
        ImageNode::new(image.clone()).with_color(Color::WHITE.with_alpha(0.0)),
        PortalOverlay { frames, image },
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
    ));
    commands
        .spawn((
            PortalUiRoot,
            TravelNotice,
            Pickable::IGNORE,
            GlobalZIndex(10),
            Visibility::Hidden,
            BackgroundColor(Color::srgb(0.11, 0.08, 0.06)),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|notice| {
            notice.spawn((
                TravelNoticeText,
                Text::new(""),
                TextFont {
                    font: FontSource::Handle(font.minecraft.clone()),
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn despawn(mut commands: Commands, roots: Query<Entity, With<PortalUiRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn update_overlay(
    tick: Res<WorldTick>,
    players: Query<&PortalTravel, With<Player>>,
    mut images: ResMut<Assets<Image>>,
    mut overlays: Query<(&mut PortalOverlay, &mut ImageNode, &mut Visibility)>,
) {
    let strength = players
        .single()
        .map_or(0.0, |portal| portal.strength(tick.partial()));
    for (mut overlay, mut node, mut visibility) in &mut overlays {
        if strength <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        node.color = Color::WHITE.with_alpha(overlay_alpha(strength));
        // The frame only advances while the wash is on screen, so the image
        // is rewritten for a few seconds at a time, not all the while.
        let ticks = tick.ticks_this_frame();
        if ticks > 0 {
            for _ in 0..ticks {
                overlay.frames.tick();
            }
            if let Some(mut image) = images.get_mut(&overlay.image) {
                image.data = Some(overlay.frames.rgba().to_vec());
            }
        }
    }
}

fn update_travel_notice(
    session: Option<Res<WorldSession>>,
    mut notices: Query<&mut Visibility, With<TravelNotice>>,
    mut texts: Query<&mut Text, With<TravelNoticeText>>,
) {
    let target = session.and_then(|session| session.travelling_to());
    for mut visibility in &mut notices {
        visibility.set_if_neq(if target.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some(target) = target else {
        return;
    };
    let message = match target {
        Dimension::Nether => "Entering the Nether",
        Dimension::Overworld => "Leaving the Nether",
    };
    for mut text in &mut texts {
        if text.0 != message {
            text.0 = message.to_owned();
        }
    }
}
