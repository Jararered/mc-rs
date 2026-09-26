//! Inventory stack sorting and its Beta item category order.

use crate::block::id::Id;
use crate::inventory::Inventory;
use crate::item::ItemData;
use crate::item::ItemId;
use crate::item::ItemStack;

/// Sort stacks by category, merge compatible stacks, and keep empty slots at
/// the end. Any amount over the stack limit remains in additional stacks.
pub fn sort_container_slots(slots: &mut [Option<ItemStack>]) {
    let mut stacks: Vec<ItemStack> = slots.iter_mut().filter_map(Option::take).collect();
    stacks.sort_by_key(|stack| sort_key(*stack));

    let mut compacted: Vec<ItemStack> = Vec::with_capacity(stacks.len());
    for stack in stacks {
        if let Some(existing) = compacted.last_mut()
            && existing.item() == stack.item()
            && existing.data() == stack.data()
            && let Some(remainder) = existing.merge(stack)
        {
            compacted.push(remainder);
        } else if compacted.last().is_none_or(|existing| {
            existing.item() != stack.item() || existing.data() != stack.data()
        }) {
            compacted.push(stack);
        }
    }

    for (slot, stack) in slots.iter_mut().zip(compacted) {
        *slot = Some(stack);
    }
}

fn sort_key(stack: ItemStack) -> (u8, u8, u8, u16, u16) {
    let item = stack.item();
    let definition = stack.definition();

    if let Some(block) = definition.block {
        let (subcategory, leaf) = block_sort_path(block);
        return (0, subcategory, leaf, u16::from(block.as_u8()), stack.data());
    }
    if let Some((subcategory, leaf)) = standalone_block_sort_path(item) {
        return (0, subcategory, leaf, item.as_u16(), stack.data());
    }

    let (category, subcategory, leaf) = match item {
        // food / edibleFood, then food / ingredients
        ItemId::Apple
        | ItemId::MushroomStew
        | ItemId::Bread
        | ItemId::RawPorkchop
        | ItemId::CookedPorkchop
        | ItemId::GoldenApple
        | ItemId::RawFish
        | ItemId::CookedFish
        | ItemId::Cake
        | ItemId::Cookie => (1, 0, 0),
        ItemId::Bowl | ItemId::Seeds | ItemId::Wheat | ItemId::Egg | ItemId::Sugar => (1, 1, 0),
        // material: ingots, gems, coal, dyes, mob drops, other materials
        ItemId::IronIngot | ItemId::GoldIngot => (2, 0, 0),
        ItemId::Diamond => (2, 1, 0),
        ItemId::Coal => (2, 2, 0),
        ItemId::Dye => (2, 3, 0),
        ItemId::String
        | ItemId::Feather
        | ItemId::Gunpowder
        | ItemId::Leather
        | ItemId::Slimeball
        | ItemId::Bone => (2, 4, 0),
        ItemId::Flint
        | ItemId::Stick
        | ItemId::Brick
        | ItemId::ClayBall
        | ItemId::Redstone
        | ItemId::GlowstoneDust
        | ItemId::Paper
        | ItemId::Book => (2, 5, 0),
        // transport / minecart, then transport / boat
        ItemId::Minecart | ItemId::ChestMinecart | ItemId::FurnaceMinecart => (3, 0, 0),
        ItemId::Boat => (3, 1, 0),
        // equipment / weapon: sword, bow, arrow
        ItemId::WoodenSword
        | ItemId::StoneSword
        | ItemId::IronSword
        | ItemId::DiamondSword
        | ItemId::GoldSword => (5, 0, 0),
        ItemId::Bow => (5, 0, 1),
        ItemId::Arrow => (5, 0, 2),
        // equipment / tool: pickaxe, shovel, axe, hoe, shears, fishing rod,
        // flint and steel, bucket.
        ItemId::WoodenPickaxe
        | ItemId::StonePickaxe
        | ItemId::IronPickaxe
        | ItemId::DiamondPickaxe
        | ItemId::GoldPickaxe => (5, 1, 0),
        ItemId::WoodenShovel
        | ItemId::StoneShovel
        | ItemId::IronShovel
        | ItemId::DiamondShovel
        | ItemId::GoldShovel => (5, 1, 1),
        ItemId::WoodenAxe
        | ItemId::StoneAxe
        | ItemId::IronAxe
        | ItemId::DiamondAxe
        | ItemId::GoldAxe => (5, 1, 2),
        ItemId::WoodenHoe
        | ItemId::StoneHoe
        | ItemId::IronHoe
        | ItemId::DiamondHoe
        | ItemId::GoldHoe => (5, 1, 3),
        ItemId::Shears => (5, 1, 4),
        ItemId::FishingRod => (5, 1, 5),
        ItemId::FlintAndSteel => (5, 1, 6),
        ItemId::Bucket | ItemId::WaterBucket | ItemId::LavaBucket | ItemId::MilkBucket => (5, 1, 7),
        // equipment / armor: helmet, chestplate, leggings, boots
        ItemId::LeatherHelmet
        | ItemId::ChainmailHelmet
        | ItemId::IronHelmet
        | ItemId::DiamondHelmet
        | ItemId::GoldHelmet => (5, 2, 0),
        ItemId::LeatherChestplate
        | ItemId::ChainmailChestplate
        | ItemId::IronChestplate
        | ItemId::DiamondChestplate
        | ItemId::GoldChestplate => (5, 2, 1),
        ItemId::LeatherLeggings
        | ItemId::ChainmailLeggings
        | ItemId::IronLeggings
        | ItemId::DiamondLeggings
        | ItemId::GoldLeggings => (5, 2, 2),
        ItemId::LeatherBoots
        | ItemId::ChainmailBoots
        | ItemId::IronBoots
        | ItemId::DiamondBoots
        | ItemId::GoldBoots => (5, 2, 3),
        _ if matches!(definition.data, ItemData::Durability(_)) => (5, 1, 8),
        _ => (4, 0, 0), // miscellaneous
    };
    (category, subcategory, leaf, item.as_u16(), stack.data())
}

/// Block subtree order: natural, building, functional, then redstone.
fn block_sort_path(block: Id) -> (u8, u8) {
    match block {
        // naturalBlock: stone, dirt, sand, gravel, ores, logs, leaves, netherBlocks
        Id::Stone | Id::Bedrock | Id::Obsidian => (0, 0),
        Id::Dirt | Id::Grass | Id::Farmland | Id::Clay => (0, 1),
        Id::Sand | Id::Sandstone => (0, 2),
        Id::Gravel => (0, 3),
        Id::GoldOre
        | Id::IronOre
        | Id::CoalOre
        | Id::LapisOre
        | Id::DiamondOre
        | Id::RedstoneOre
        | Id::LitRedstoneOre => (0, 4),
        Id::Wood | Id::SpruceWood | Id::BirchWood => (0, 5),
        Id::Leaves
        | Id::SpruceLeaves
        | Id::BirchLeaves
        | Id::Sapling
        | Id::TallGrass
        | Id::Fern
        | Id::DeadBush
        | Id::Dandelion
        | Id::Rose
        | Id::BrownMushroom
        | Id::RedMushroom
        | Id::Crops
        | Id::SugarCane => (0, 6),
        Id::Netherrack | Id::SoulSand | Id::Glowstone | Id::NetherPortal => (0, 7),
        Id::Ice
        | Id::Snow
        | Id::SnowLayer
        | Id::Cactus
        | Id::Sponge
        | Id::Water
        | Id::FlowingWater
        | Id::Lava
        | Id::FlowingLava
        | Id::Pumpkin
        | Id::JackOLantern => (0, 8),
        // buildingBlock: planks, cobblestone, bricks, glass, wool, slabs/stairs
        Id::WoodenPlanks | Id::SprucePlanks | Id::BirchPlanks => (1, 0),
        Id::Cobblestone | Id::MossyCobblestone => (1, 1),
        Id::Bricks => (1, 2),
        Id::Glass => (1, 3),
        Id::Wool => (1, 4),
        Id::DoubleStoneSlab | Id::StoneSlab | Id::WoodenStairs | Id::CobblestoneStairs => (1, 5),
        Id::Bookshelf => (1, 6),
        // functionalBlock: chest, furnace, craftingTable, bed
        Id::Chest | Id::LockedChest => (2, 0),
        Id::Furnace | Id::LitFurnace => (2, 1),
        Id::CraftingTable => (2, 2),
        Id::Bed => (2, 3),
        Id::Dispenser
        | Id::NoteBlock
        | Id::PoweredRail
        | Id::DetectorRail
        | Id::Rail
        | Id::StandingSign
        | Id::WallSign
        | Id::WoodenDoor
        | Id::IronDoor
        | Id::Ladder
        | Id::Jukebox
        | Id::MobSpawner
        | Id::Tnt
        | Id::Cake => (2, 4),
        // redstone: wire, torch, repeater, piston, lever/button/pressurePlate
        Id::RedstoneWire => (3, 0),
        Id::Torch | Id::UnlitRedstoneTorch | Id::RedstoneTorch => (3, 1),
        Id::Repeater | Id::PoweredRepeater => (3, 2),
        Id::StickyPiston | Id::Piston | Id::PistonHead | Id::MovingPiston => (3, 3),
        Id::Lever | Id::StoneButton | Id::StonePressurePlate | Id::WoodenPressurePlate => (3, 4),
        _ => (0, 9), // remaining natural/functional blocks
    }
}

/// Some special standalone Beta item IDs are block-placement items and should
/// sort beside their direct block-item counterparts.
fn standalone_block_sort_path(item: ItemId) -> Option<(u8, u8)> {
    match item {
        ItemId::Bed => Some((2, 3)),
        ItemId::Sign | ItemId::WoodenDoor | ItemId::IronDoor => Some((2, 4)),
        ItemId::Repeater => Some((3, 2)),
        _ => None,
    }
}

/// Sort the 27 main inventory slots without touching the hotbar or equipment.
pub fn sort_main_inventory(inventory: &mut Inventory) {
    sort_container_slots(&mut inventory.main);
}
