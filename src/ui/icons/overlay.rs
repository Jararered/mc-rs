//! Stack count and durability bar drawn on a slot icon.
//!
//! Beta places both in `RenderItem.renderItemOverlayIntoGUI`. The count is
//! `drawStringWithShadow(text, x + 19 - 2 - stringWidth, y + 6 + 3)`, so the
//! glyph cell's bottom-right corner is one GUI pixel past the 16×16 icon.
//! Subtracting the string width keeps a single digit on that same corner.
//! The hotbar and the inventory screen both use this geometry.

use bevy::prelude::*;
use bevy::text::FontSmoothing;
use bevy::text::LineBreak;
use bevy::text::LineHeight;

use crate::item::ItemStack;

/// GUI pixels per screen pixel. Beta auto-scale is applied by drawing the
/// 176×166 inventory and the 182×22 hotbar at this size.
pub const GUI_SCALE: f32 = 2.0;

const ICON_GUI: f32 = 16.0;
const FONT_GUI: f32 = 8.0;
/// `x + 19 - 2` in `renderItemOverlayIntoGUI`.
const COUNT_RIGHT_GUI: f32 = 17.0;
/// `y + 6 + 3` in `renderItemOverlayIntoGUI`.
const COUNT_TOP_GUI: f32 = 9.0;

/// Screen-pixel frame that right-justified count text is laid out in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CountFrame {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl CountFrame {
    /// Right edge of the glyph cell. A count of any width ends here.
    pub fn right(self) -> f32 {
        self.left + self.width
    }

    /// Bottom edge of the 8px glyph cell.
    pub fn bottom(self) -> f32 {
        self.top + self.height
    }
}

/// Frame covering `itemX .. itemX + 17` and `itemY + 9 .. itemY + 17`, scaled.
pub fn count_frame(icon_left: f32, icon_top: f32) -> CountFrame {
    CountFrame {
        left: icon_left,
        top: icon_top + COUNT_TOP_GUI * GUI_SCALE,
        width: COUNT_RIGHT_GUI * GUI_SCALE,
        height: FONT_GUI * GUI_SCALE,
    }
}

/// Top-left of a count string whose advance is `string_width` screen pixels.
///
/// This is `x + 19 - 2 - stringWidth`, `y + 6 + 3` from
/// `RenderItem.renderItemOverlayIntoGUI`, multiplied by [`GUI_SCALE`].
pub fn count_origin(icon_x: f32, icon_y: f32, string_width: f32) -> (f32, f32) {
    (
        icon_x + COUNT_RIGHT_GUI * GUI_SCALE - string_width,
        icon_y + COUNT_TOP_GUI * GUI_SCALE,
    )
}

pub fn count_label(stack: Option<ItemStack>) -> String {
    stack
        .filter(|stack| stack.count() > 1)
        .map(|stack| stack.count().to_string())
        .unwrap_or_default()
}

pub fn count_text_font(font: &Handle<Font>) -> TextFont {
    TextFont::from_font_size(FONT_GUI * GUI_SCALE)
        .with_font(font.clone())
        .with_font_smoothing(FontSmoothing::None)
}

pub fn count_line_height() -> LineHeight {
    LineHeight::Px(FONT_GUI * GUI_SCALE)
}

/// `FontRenderer.drawStringWithShadow` draws the shadow one GUI pixel down-right.
/// White `0xFFFFFF` shifts to `0x3F3F3F`.
pub fn count_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::splat(GUI_SCALE),
        color: Color::srgb_u8(0x3f, 0x3f, 0x3f),
    }
}

/// Black track and colored fill of the durability bar.
///
/// Both quads start at `(itemX + 2, itemY + 13)`. The track is 13×2 and the
/// fill is the top 1px, matching the three `renderQuad` calls.
pub fn durability_track(icon_left: f32, icon_top: f32, foreground: bool) -> (f32, f32, f32, f32) {
    let height = if foreground { 1.0 } else { 2.0 };
    (
        icon_left + 2.0 * GUI_SCALE,
        icon_top + 13.0 * GUI_SCALE,
        13.0 * GUI_SCALE,
        height * GUI_SCALE,
    )
}

pub fn icon_size() -> f32 {
    ICON_GUI * GUI_SCALE
}

/// Puts a stack label on the icon. Counts are right-justified in [`count_frame`]
/// so a one-digit stack shares its right edge with `64`. Names, used only when
/// the icon sprite is not ready, stay in the icon box.
///
/// Right alignment reads the node width. `LineBreak::NoWrap` would drop that
/// width and pin every string to the left.
pub fn place_stack_label(
    text: &mut Text,
    node: &mut Node,
    layout: &mut TextLayout,
    font: &mut TextFont,
    line_height: &mut LineHeight,
    shadow: &mut TextShadow,
    ui_font: &Handle<Font>,
    icon_left: f32,
    icon_top: f32,
    label: &str,
    has_icon: bool,
) {
    **text = label.to_string();
    node.position_type = PositionType::Absolute;
    node.right = Val::Auto;
    node.bottom = Val::Auto;
    layout.linebreak = LineBreak::WordBoundary;
    if has_icon {
        let frame = count_frame(icon_left, icon_top);
        node.left = px(frame.left);
        node.top = px(frame.top);
        node.width = px(frame.width);
        node.height = px(frame.height);
        layout.justify = Justify::Right;
        *font = count_text_font(ui_font);
        *line_height = count_line_height();
        *shadow = count_shadow();
    } else {
        node.left = px(icon_left);
        node.top = px(icon_top);
        node.width = px(icon_size());
        node.height = px(icon_size());
        layout.justify = Justify::Left;
        *font = TextFont::from_font_size(10.0);
        *line_height = LineHeight::default();
        *shadow = TextShadow::default();
    }
}

/// Apply label presentation without invalidating unchanged ECS components.
/// The plain-value helper above is also used when initially spawning labels.
pub fn sync_stack_label(
    text: &mut Mut<Text>,
    node: &mut Mut<Node>,
    layout: &mut Mut<TextLayout>,
    font: &mut Mut<TextFont>,
    line_height: &mut Mut<LineHeight>,
    shadow: &mut Mut<TextShadow>,
    ui_font: &Handle<Font>,
    icon_left: f32,
    icon_top: f32,
    label: &str,
    has_icon: bool,
) {
    let mut next_node = (**node).clone();
    let mut next_layout = **layout;
    let mut next_font = (**font).clone();
    let mut next_height = **line_height;
    let mut next_shadow = **shadow;
    // No string allocation is needed to compute the geometry and font.
    place_stack_label(
        &mut Text::default(),
        &mut next_node,
        &mut next_layout,
        &mut next_font,
        &mut next_height,
        &mut next_shadow,
        ui_font,
        icon_left,
        icon_top,
        "",
        has_icon,
    );
    if text.0 != label {
        text.0 = label.to_owned();
    }
    node.set_if_neq(next_node);
    if layout.justify != next_layout.justify || layout.linebreak != next_layout.linebreak {
        **layout = next_layout;
    }
    font.set_if_neq(next_font);
    line_height.set_if_neq(next_height);
    shadow.set_if_neq(next_shadow);
}

#[derive(Resource)]
pub struct UiFont {
    pub minecraft: Handle<Font>,
}

pub fn load_ui_font(commands: &mut Commands, assets: &AssetServer) {
    commands.insert_resource(UiFont {
        minecraft: assets.load("font/minecraft.otf"),
    });
}
