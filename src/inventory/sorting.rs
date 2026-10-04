//! Inventory stack sorting and its Beta item category order.

use crate::block::blocks::Block;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemData;
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
        Item::Apple
        | Item::MushroomStew
        | Item::Bread
        | Item::RawPorkchop
        | Item::CookedPorkchop
        | Item::GoldenApple
        | Item::RawFish
        | Item::CookedFish
        | Item::Cake
        | Item::Cookie => (1, 0, 0),
        Item::Bowl | Item::Seeds | Item::Wheat | Item::Egg | Item::Sugar => (1, 1, 0),
        // material: ingots, gems, coal, dyes, mob drops, other materials
        Item::IronIngot | Item::GoldIngot => (2, 0, 0),
        Item::Diamond => (2, 1, 0),
        Item::Coal => (2, 2, 0),
        Item::Dye => (2, 3, 0),
        Item::String
        | Item::Feather
        | Item::Gunpowder
        | Item::Leather
        | Item::Slimeball
        | Item::Bone => (2, 4, 0),
        Item::Flint
        | Item::Stick
        | Item::Brick
        | Item::ClayBall
        | Item::Redstone
        | Item::GlowstoneDust
        | Item::Paper
        | Item::Book => (2, 5, 0),
        // transport / minecart, then transport / boat
        Item::Minecart | Item::ChestMinecart | Item::FurnaceMinecart => (3, 0, 0),
        Item::Boat => (3, 1, 0),
        // equipment / weapon: sword, bow, arrow
        Item::WoodenSword
        | Item::StoneSword
        | Item::IronSword
        | Item::DiamondSword
        | Item::GoldSword => (5, 0, 0),
        Item::Bow => (5, 0, 1),
        Item::Arrow => (5, 0, 2),
        // equipment / tool: pickaxe, shovel, axe, hoe, shears, fishing rod,
        // flint and steel, bucket.
        Item::WoodenPickaxe
        | Item::StonePickaxe
        | Item::IronPickaxe
        | Item::DiamondPickaxe
        | Item::GoldPickaxe => (5, 1, 0),
        Item::WoodenShovel
        | Item::StoneShovel
        | Item::IronShovel
        | Item::DiamondShovel
        | Item::GoldShovel => (5, 1, 1),
        Item::WoodenAxe | Item::StoneAxe | Item::IronAxe | Item::DiamondAxe | Item::GoldAxe => {
            (5, 1, 2)
        }
        Item::WoodenHoe | Item::StoneHoe | Item::IronHoe | Item::DiamondHoe | Item::GoldHoe => {
            (5, 1, 3)
        }
        Item::Shears => (5, 1, 4),
        Item::FishingRod => (5, 1, 5),
        Item::FlintAndSteel => (5, 1, 6),
        Item::Bucket | Item::WaterBucket | Item::LavaBucket | Item::MilkBucket => (5, 1, 7),
        // equipment / armor: helmet, chestplate, leggings, boots
        Item::LeatherHelmet
        | Item::ChainmailHelmet
        | Item::IronHelmet
        | Item::DiamondHelmet
        | Item::GoldHelmet => (5, 2, 0),
        Item::LeatherChestplate
        | Item::ChainmailChestplate
        | Item::IronChestplate
        | Item::DiamondChestplate
        | Item::GoldChestplate => (5, 2, 1),
        Item::LeatherLeggings
        | Item::ChainmailLeggings
        | Item::IronLeggings
        | Item::DiamondLeggings
        | Item::GoldLeggings => (5, 2, 2),
        Item::LeatherBoots
        | Item::ChainmailBoots
        | Item::IronBoots
        | Item::DiamondBoots
        | Item::GoldBoots => (5, 2, 3),
        _ if matches!(definition.data, ItemData::Durability(_)) => (5, 1, 8),
        _ => (4, 0, 0), // miscellaneous
    };
    (category, subcategory, leaf, item.as_u16(), stack.data())
}

/// Block subtree order: natural, building, functional, then redstone.
fn block_sort_path(block: Block) -> (u8, u8) {
    match block {
        // naturalBlock: stone, dirt, sand, gravel, ores, logs, leaves, netherBlocks
        Block::Stone | Block::Bedrock | Block::Obsidian => (0, 0),
        Block::Dirt | Block::Grass | Block::Farmland | Block::Clay => (0, 1),
        Block::Sand | Block::Sandstone => (0, 2),
        Block::Gravel => (0, 3),
        Block::GoldOre
        | Block::IronOre
        | Block::CoalOre
        | Block::LapisOre
        | Block::DiamondOre
        | Block::RedstoneOre
        | Block::LitRedstoneOre => (0, 4),
        Block::Wood => (0, 5),
        Block::Leaves
        | Block::Sapling
        | Block::TallGrass
        | Block::DeadBush
        | Block::Dandelion
        | Block::Rose
        | Block::BrownMushroom
        | Block::RedMushroom
        | Block::Crops
        | Block::SugarCane => (0, 6),
        Block::Netherrack | Block::SoulSand | Block::Glowstone | Block::NetherPortal => (0, 7),
        Block::Ice
        | Block::Snow
        | Block::SnowLayer
        | Block::Cactus
        | Block::Sponge
        | Block::Water
        | Block::FlowingWater
        | Block::Lava
        | Block::FlowingLava
        | Block::Pumpkin
        | Block::JackOLantern => (0, 8),
        // buildingBlock: planks, cobblestone, bricks, glass, wool, slabs/stairs
        Block::WoodenPlanks => (1, 0),
        Block::Cobblestone | Block::MossyCobblestone => (1, 1),
        Block::Bricks => (1, 2),
        Block::Glass => (1, 3),
        Block::Wool => (1, 4),
        Block::DoubleStoneSlab
        | Block::StoneSlab
        | Block::WoodenStairs
        | Block::CobblestoneStairs => (1, 5),
        Block::Bookshelf => (1, 6),
        // functionalBlock: chest, furnace, craftingTable, bed
        Block::Chest | Block::LockedChest => (2, 0),
        Block::Furnace | Block::LitFurnace => (2, 1),
        Block::CraftingTable => (2, 2),
        Block::Bed => (2, 3),
        Block::Dispenser
        | Block::NoteBlock
        | Block::PoweredRail
        | Block::DetectorRail
        | Block::Rail
        | Block::StandingSign
        | Block::WallSign
        | Block::WoodenDoor
        | Block::IronDoor
        | Block::Ladder
        | Block::Jukebox
        | Block::MobSpawner
        | Block::Tnt
        | Block::Cake => (2, 4),
        // redstone: wire, torch, repeater, piston, lever/button/pressurePlate
        Block::RedstoneWire => (3, 0),
        Block::Torch | Block::UnlitRedstoneTorch | Block::RedstoneTorch => (3, 1),
        Block::Repeater | Block::PoweredRepeater => (3, 2),
        Block::StickyPiston | Block::Piston | Block::PistonHead | Block::MovingPiston => (3, 3),
        Block::Lever
        | Block::StoneButton
        | Block::StonePressurePlate
        | Block::WoodenPressurePlate => (3, 4),
        _ => (0, 9), // remaining natural/functional blocks
    }
}

/// Some special standalone Beta item IDs are block-placement items and should
/// sort beside their direct block-item counterparts.
fn standalone_block_sort_path(item: Item) -> Option<(u8, u8)> {
    match item {
        Item::Bed => Some((2, 3)),
        Item::Sign | Item::WoodenDoor | Item::IronDoor => Some((2, 4)),
        Item::Repeater => Some((3, 2)),
        _ => None,
    }
}

/// Sort the 27 main inventory slots without touching the hotbar or equipment.
pub fn sort_main_inventory(inventory: &mut Inventory) {
    sort_container_slots(&mut inventory.main);
}
