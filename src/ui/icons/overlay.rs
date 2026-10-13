//! Stack count and durability bar drawn on a slot icon.
//!
//! Beta places both in `RenderItem.renderItemOverlayIntoGUI`. The count is
//! `drawStringWithShadow(text, x + 19 - 2 - stringWidth, y + 6 + 3)`, so the
//! glyph cell's bottom-right corner is one GUI pixel past the 16×16 icon.
//! Subtracting the string width keeps a single digit on that same corner.
//! The hotbar and the inventory screen both use this geometry.
//!
//! `scale` is `GameSettings::gui_scale`, screen pixels per GUI pixel.

use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy::text::FontSmoothing;
use bevy::text::LineBreak;
use bevy::text::LineHeight;

use crate::item::ItemStack;

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
pub fn count_frame(scale: f32, icon_left: f32, icon_top: f32) -> CountFrame {
    CountFrame {
        left: icon_left,
        top: icon_top + COUNT_TOP_GUI * scale,
        width: COUNT_RIGHT_GUI * scale,
        height: FONT_GUI * scale,
    }
}

/// Top-left of a count string whose advance is `string_width` screen pixels.
///
/// This is `x + 19 - 2 - stringWidth`, `y + 6 + 3` from
/// `RenderItem.renderItemOverlayIntoGUI`, multiplied by `scale`.
pub fn count_origin(scale: f32, icon_x: f32, icon_y: f32, string_width: f32) -> (f32, f32) {
    (
        icon_x + COUNT_RIGHT_GUI * scale - string_width,
        icon_y + COUNT_TOP_GUI * scale,
    )
}

pub fn count_label(stack: Option<ItemStack>) -> String {
    stack
        .filter(|stack| stack.count() > 1)
        .map(|stack| stack.count().to_string())
        .unwrap_or_default()
}

pub fn count_text_font(scale: f32, font: &Handle<Font>) -> TextFont {
    TextFont::from_font_size(FONT_GUI * scale)
        .with_font(font.clone())
        .with_font_smoothing(FontSmoothing::None)
}

pub fn count_line_height(scale: f32) -> LineHeight {
    LineHeight::Px(FONT_GUI * scale)
}

/// `FontRenderer.drawStringWithShadow` draws the shadow one GUI pixel down-right.
/// White `0xFFFFFF` shifts to `0x3F3F3F`.
pub fn count_shadow(scale: f32) -> TextShadow {
    TextShadow {
        offset: Vec2::splat(scale),
        color: Color::srgb_u8(0x3f, 0x3f, 0x3f),
    }
}

/// Black track and colored fill of the durability bar.
///
/// Both quads start at `(itemX + 2, itemY + 13)`. The track is 13×2 and the
/// fill is the top 1px, matching the three `renderQuad` calls.
pub fn durability_track(
    scale: f32,
    icon_left: f32,
    icon_top: f32,
    foreground: bool,
) -> (f32, f32, f32, f32) {
    let height = if foreground { 1.0 } else { 2.0 };
    (
        icon_left + 2.0 * scale,
        icon_top + 13.0 * scale,
        13.0 * scale,
        height * scale,
    )
}

pub fn icon_size(scale: f32) -> f32 {
    ICON_GUI * scale
}

/// Where a stack label goes and what it says: the icon's top-left corner in
/// screen pixels, and whether the icon sprite is there to put a count on.
#[derive(Clone, Copy)]
pub struct LabelPlacement<'a> {
    pub scale: f32,
    pub font: &'a Handle<Font>,
    pub icon_left: f32,
    pub icon_top: f32,
    pub label: &'a str,
    pub has_icon: bool,
}

/// The components a stack label is made of, as plain values: what a new
/// label is spawned with.
#[derive(Bundle, Clone, Default)]
pub struct StackLabelBundle {
    pub text: Text,
    pub node: Node,
    pub layout: TextLayout,
    pub font: TextFont,
    pub line_height: LineHeight,
    pub shadow: TextShadow,
}

impl StackLabelBundle {
    /// A label laid out at `placement`.
    pub fn placed(placement: LabelPlacement) -> Self {
        let mut label = Self::default();
        label.text.0 = placement.label.to_string();
        label.place(placement);
        label
    }

    /// Puts the label on the icon, leaving its text alone. Counts are
    /// right-justified in [`count_frame`] so a one-digit stack shares its
    /// right edge with `64`. Names, used only when the icon sprite is not
    /// ready, stay in the icon box.
    ///
    /// Right alignment reads the node width. `LineBreak::NoWrap` would drop that
    /// width and pin every string to the left.
    fn place(&mut self, placement: LabelPlacement) {
        let LabelPlacement {
            scale,
            font: ui_font,
            icon_left,
            icon_top,
            has_icon,
            ..
        } = placement;
        let Self {
            node,
            layout,
            font,
            line_height,
            shadow,
            ..
        } = self;
        node.position_type = PositionType::Absolute;
        node.right = Val::Auto;
        node.bottom = Val::Auto;
        layout.linebreak = LineBreak::WordBoundary;
        if has_icon {
            let frame = count_frame(scale, icon_left, icon_top);
            node.left = px(frame.left);
            node.top = px(frame.top);
            node.width = px(frame.width);
            node.height = px(frame.height);
            layout.justify = Justify::Right;
            *font = count_text_font(scale, ui_font);
            *line_height = count_line_height(scale);
            *shadow = count_shadow(scale);
        } else {
            node.left = px(icon_left);
            node.top = px(icon_top);
            node.width = px(icon_size(scale));
            node.height = px(icon_size(scale));
            layout.justify = Justify::Left;
            *font = TextFont::from_font_size(10.0);
            *line_height = LineHeight::default();
            *shadow = TextShadow::default();
        }
    }
}

/// A spawned stack label's components, for a query.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct StackLabel {
    pub text: &'static mut Text,
    pub node: &'static mut Node,
    pub layout: &'static mut TextLayout,
    pub font: &'static mut TextFont,
    pub line_height: &'static mut LineHeight,
    pub shadow: &'static mut TextShadow,
}

impl StackLabelItem<'_, '_> {
    /// Apply label presentation without invalidating unchanged ECS components.
    pub fn sync(&mut self, placement: LabelPlacement) {
        // No string allocation is needed to compute the geometry and font.
        let mut next = StackLabelBundle {
            text: Text::default(),
            node: (*self.node).clone(),
            layout: *self.layout,
            font: (*self.font).clone(),
            line_height: *self.line_height,
            shadow: *self.shadow,
        };
        next.place(placement);
        if self.text.0 != placement.label {
            self.text.0 = placement.label.to_owned();
        }
        self.node.set_if_neq(next.node);
        if self.layout.justify != next.layout.justify
            || self.layout.linebreak != next.layout.linebreak
        {
            *self.layout = next.layout;
        }
        self.font.set_if_neq(next.font);
        self.line_height.set_if_neq(next.line_height);
        self.shadow.set_if_neq(next.shadow);
    }
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
