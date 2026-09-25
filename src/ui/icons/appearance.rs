//! Inventory appearances transcribed from the Beta 1.7.3 Block, ItemBlock,
//! RenderItem, and specialized block/item classes in refs/mc_b1.7.3_release.
//! Tile numbers refer to terrain.png's 16x16 grid.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Flat,
    Cube,
    Slab,
    Thin,
    Stairs,
    Fence,
    Cactus,
}

#[derive(Clone, Copy, Debug)]
pub struct Appearance {
    pub shape: Shape,
    pub top: u8,
    pub left: u8,
    pub right: u8,
    pub tint: [u8; 3],
}

impl Appearance {
    const fn cube(tile: u8) -> Self {
        Self {
            shape: Shape::Cube,
            top: tile,
            left: tile,
            right: tile,
            tint: [255; 3],
        }
    }
    const fn flat(tile: u8) -> Self {
        Self {
            shape: Shape::Flat,
            top: tile,
            left: tile,
            right: tile,
            tint: [255; 3],
        }
    }
    const fn faces(shape: Shape, top: u8, left: u8, right: u8) -> Self {
        Self {
            shape,
            top,
            left,
            right,
            tint: [255; 3],
        }
    }
    fn tint(mut self, rgb: u32) -> Self {
        self.tint = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
        self
    }
}

/// Beta's `RenderBlocks.renderItemIn3d` selects render types 0, 10, 11, 13,
/// and 16. Other block items use a flat terrain tile in `RenderItem`.
pub fn block_appearance(id: u8, data: u16) -> Appearance {
    let data = data as u8;
    match id {
        1 => Appearance::cube(1),
        2 => Appearance::faces(Shape::Cube, 0, 3, 3),
        3 => Appearance::cube(2),
        4 => Appearance::cube(16),
        5 => Appearance::cube(4).tint(match data {
            1 => 0xD6B183,
            2 => 0xFFF6DA,
            _ => 0xFFFFFF,
        }),
        6 => Appearance::flat(match data {
            1 => 63,
            2 => 79,
            _ => 15,
        }),
        7 => Appearance::cube(17),
        8 | 9 => Appearance::flat(14),
        10 | 11 => Appearance::flat(30),
        12 => Appearance::cube(18),
        13 => Appearance::cube(19),
        14 => Appearance::cube(32),
        15 => Appearance::cube(33),
        16 => Appearance::cube(34),
        17 => Appearance::faces(
            Shape::Cube,
            21,
            match data {
                1 => 116,
                2 => 117,
                _ => 20,
            },
            match data {
                1 => 116,
                2 => 117,
                _ => 20,
            },
        ),
        18 => Appearance::cube(if data == 1 { 132 } else { 52 }).tint(match data {
            1 => 6396257,
            2 => 8431445,
            _ => 4764952,
        }),
        19 => Appearance::cube(48),
        20 => Appearance::cube(49),
        21 => Appearance::cube(160),
        22 => Appearance::cube(144),
        23 => Appearance::faces(Shape::Cube, 62, 45, 46),
        24 => Appearance::faces(Shape::Cube, 176, 192, 192),
        25 => Appearance::cube(74),
        26 => Appearance::flat(134),
        27 => Appearance::flat(179),
        28 => Appearance::flat(195),
        29 => Appearance::faces(Shape::Cube, 106, 108, 108),
        30 => Appearance::flat(11),
        31 => Appearance::flat(39).tint(0x64a83b),
        32 => Appearance::flat(55),
        33 | 34 | 36 => Appearance::faces(Shape::Cube, 107, 108, 108),
        35 => {
            let metadata = data & 15;
            let tile = if metadata == 0 {
                64
            } else {
                let inverted = (!metadata) & 15;
                113 + ((inverted & 8) >> 3) + ((inverted & 7) * 16)
            };
            Appearance::cube(tile)
        }
        37 => Appearance::flat(13),
        38 => Appearance::flat(12),
        39 => Appearance::flat(29),
        40 => Appearance::flat(28),
        41 => Appearance::cube(23),
        42 => Appearance::cube(22),
        43 => Appearance::faces(Shape::Cube, 6, 5, 5),
        44 => {
            let (top, side) = match data {
                1 => (176, 192),
                2 => (4, 4),
                3 => (16, 16),
                _ => (6, 5),
            };
            Appearance::faces(Shape::Slab, top, side, side)
        }
        45 => Appearance::cube(7),
        46 => Appearance::faces(Shape::Cube, 9, 8, 8),
        47 => Appearance::faces(Shape::Cube, 4, 35, 35),
        48 => Appearance::cube(36),
        49 => Appearance::cube(37),
        50 => Appearance::flat(80),
        51 => Appearance::flat(31),
        52 => Appearance::cube(65),
        53 => Appearance::faces(Shape::Stairs, 4, 4, 4),
        54 => Appearance::faces(Shape::Cube, 25, 26, 27),
        55 => Appearance::flat(164),
        56 => Appearance::cube(50),
        57 => Appearance::cube(24),
        58 => Appearance::faces(Shape::Cube, 43, 59, 60),
        59 => Appearance::flat(88),
        60 => Appearance::faces(Shape::Cube, 87, 2, 2),
        61 => Appearance::faces(Shape::Cube, 62, 45, 44),
        62 => Appearance::faces(Shape::Cube, 62, 45, 44), // inventory side texture matches idle furnace
        63 => Appearance::flat(4),
        64 => Appearance::flat(97),
        65 => Appearance::flat(83),
        66 => Appearance::flat(128),
        67 => Appearance::faces(Shape::Stairs, 16, 16, 16),
        68 => Appearance::flat(4),
        69 => Appearance::flat(96),
        70 => Appearance::faces(Shape::Thin, 1, 1, 1),
        71 => Appearance::flat(98),
        72 => Appearance::faces(Shape::Thin, 4, 4, 4),
        73 | 74 => Appearance::cube(51),
        75 => Appearance::flat(115),
        76 => Appearance::flat(99),
        77 => Appearance::faces(Shape::Thin, 1, 1, 1),
        78 => Appearance::faces(Shape::Thin, 66, 66, 66),
        79 => Appearance::cube(67),
        80 => Appearance::cube(66),
        81 => Appearance::faces(Shape::Cactus, 69, 70, 70),
        82 => Appearance::cube(72),
        83 => Appearance::flat(73),
        84 => Appearance::faces(Shape::Cube, 75, 74, 74),
        85 => Appearance::faces(Shape::Fence, 4, 4, 4),
        86 => Appearance::faces(Shape::Cube, 102, 118, 119),
        87 => Appearance::cube(103),
        88 => Appearance::cube(104),
        89 => Appearance::cube(105),
        90 => Appearance::flat(14),
        91 => Appearance::faces(Shape::Cube, 102, 118, 120),
        92 => Appearance::faces(Shape::Slab, 121, 122, 122),
        93 | 94 => Appearance::flat(131),
        95 => Appearance::faces(Shape::Cube, 25, 26, 27),
        96 => Appearance::faces(Shape::Thin, 84, 84, 84),
        _ => Appearance::flat(0),
    }
}

/// The Item.java static registrations assign these tiles in gui/items.png.
/// Subtype-specific dye tiles follow ItemDye.getIconFromDamage.
pub fn item_tile(id: u16, data: u16) -> Option<u8> {
    use super::data::ITEM_TILES;
    match id {
        256..=359 => {
            let (x, y) = ITEM_TILES[(id - 256) as usize];
            if id == 351 {
                Some(14 + (data % 8) as u8 * 16 + (data / 8) as u8)
            } else {
                Some(x + y * 16)
            }
        }
        2256 => Some(240),
        2257 => Some(241),
        _ => None,
    }
}
