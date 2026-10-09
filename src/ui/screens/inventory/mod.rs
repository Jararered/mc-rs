use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::inventory::SlotId;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;

mod chest;
mod clicks;
mod refresh;
mod screen;
mod validation;

use clicks::handle_slots;
pub(crate) use refresh::durability_bar;
use refresh::highlight_slots;
use refresh::refresh;
use screen::close;
use screen::load_texture;
use screen::toggle;
use validation::close_when_requested;
use validation::sync_open_cart;
use validation::validate_chest;
use validation::validate_furnace;
use validation::validate_workbench;

const SLOT_SIZE: f32 = 16.0;
const SLOT_STEP: f32 = 18.0;
const CHEST_HALF_SLOTS: usize = 27;

pub struct InventoryGuiPlugin;

impl Plugin for InventoryGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventorySession>()
            .init_resource::<GameSettings>()
            .init_resource::<ActiveWorkbench>()
            .init_resource::<SlotDrag>()
            .init_resource::<LastInventoryClick>()
            .add_systems(PreStartup, load_texture)
            .add_systems(
                Update,
                (
                    close_when_requested,
                    validate_workbench,
                    validate_furnace,
                    validate_chest,
                    toggle,
                    handle_slots,
                    sync_open_cart,
                    highlight_slots,
                    refresh,
                )
                    .chain()
                    .after(crate::rendering::icons::build)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(OnExit(AppScreen::Playing), close);
    }
}

#[derive(Resource)]
struct InventoryTexture {
    background: Handle<Image>,
    crafting: Handle<Image>,
    furnace: Handle<Image>,
    container: Handle<Image>,
}
#[derive(Component)]
struct InventoryRoot;
/// The inventory image's node; a click outside it throws the carried stack.
#[derive(Component)]
struct InventoryPanel;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Hotbar(usize),
    Main(usize),
    Craft(usize),
    CraftResult,
    Workbench(usize),
    Chest(usize),
    Furnace(usize),
    Armor(usize),
}
#[derive(Component)]
struct SlotLabel(Slot);
/// GuiContainer draws ARGB `0x80FFFFFF` over the hovered 16×16 slot.
#[derive(Component)]
struct SlotHighlight;
#[derive(Component)]
struct SlotIcon(Slot);
#[derive(Component)]
struct SlotDurability(Slot, bool);
#[derive(Component)]
struct CarriedLabel;
#[derive(Component)]
struct CarriedIcon;
#[derive(Component)]
struct FurnaceProgress(bool);

const HOTBAR_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

#[derive(Resource, Default)]
struct SlotDrag {
    button: Option<MouseButton>,
    origin: Option<Slot>,
    slots: Vec<Slot>,
    quick_move: bool,
    quick_move_visited: Vec<SlotId>,
}

#[derive(Resource, Default)]
struct LastInventoryClick {
    at: Option<f64>,
    slot: Option<Slot>,
    container: u8,
}

const DOUBLE_CLICK_SECONDS: f64 = 0.35;

fn is_double_click(
    last: &mut LastInventoryClick,
    screen: &InventorySession,
    slot: Slot,
    now: f64,
) -> bool {
    let container = u8::from(screen.chest)
        | (u8::from(screen.furnace) << 1)
        | (u8::from(screen.workbench) << 2);
    let double = last.slot == Some(slot)
        && last.container == container
        && last.at.is_some_and(|at| now - at <= DOUBLE_CLICK_SECONDS);
    last.at = Some(now);
    last.slot = Some(slot);
    last.container = container;
    double
}
