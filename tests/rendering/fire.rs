use game::block::blocks::Block;
use game::rendering::meshing::mesh_chunk_with_settings;
use game::rendering::textures::FIRE_TILE;
use game::rendering::textures::FIRE_TILE_ALT;
use game::rendering::textures::FlamesTexture;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;

const X: usize = 8;
const Y: usize = 8;
const Z: usize = 8;

fn mesh(chunk: &Chunk) -> game::rendering::meshing::ChunkMeshes {
    mesh_chunk_with_settings(chunk, &Skylight::from_chunk(chunk), false)
}

fn tiles(meshes: &game::rendering::meshing::ChunkMeshes) -> Vec<[u8; 2]> {
    meshes
        .masked
        .vertices()
        .chunks_exact(4)
        .map(|quad| quad[0].texel.tile)
        .collect()
}

#[test]
fn fire_on_a_solid_block_is_eight_leaning_sheets_over_both_flame_tiles() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y - 1, Z, Block::Stone);
    chunk.set(X, Y, Z, Block::Fire);
    let meshes = mesh(&chunk);
    let tiles = tiles(&meshes);
    assert_eq!(tiles.len(), 8);
    let main = [FIRE_TILE.0, FIRE_TILE.1];
    let alt = [FIRE_TILE_ALT.0, FIRE_TILE_ALT.1];
    assert_eq!(tiles.iter().filter(|tile| **tile == main).count(), 4);
    assert_eq!(tiles.iter().filter(|tile| **tile == alt).count(), 4);
    // Each sheet stands 1.4 blocks tall from the floor of the cell.
    for quad in meshes.masked.vertices().chunks_exact(4) {
        let ys: Vec<f32> = quad.iter().map(|vertex| vertex.position[1]).collect();
        let low = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let high = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(low, Y as f32);
        assert!((high - (Y as f32 + 1.4)).abs() < 1e-5);
    }
}

#[test]
fn fire_over_a_flammable_block_stands_on_it() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y - 1, Z, Block::Leaves);
    chunk.set(X, Y, Z, Block::Fire);
    assert_eq!(tiles(&mesh(&chunk)).len(), 8);
}

#[test]
fn unsupported_fire_clings_to_each_flammable_neighbor() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y, Z, Block::Fire);
    assert!(tiles(&mesh(&chunk)).is_empty());

    chunk.set(X + 1, Y, Z, Block::WoodenPlanks);
    let one = mesh(&chunk);
    // One sheet, drawn from both sides, leaning away from the planks.
    assert_eq!(tiles(&one).len(), 2);
    for vertex in one.masked.vertices() {
        let x = vertex.position[0] - X as f32;
        assert!((0.8 - 1e-5..=1.0).contains(&x), "x {x}");
    }

    chunk.set(X, Y, Z - 1, Block::Wool);
    assert_eq!(tiles(&mesh(&chunk)).len(), 4);

    // The block above adds two hanging sheets, one per tile.
    chunk.set(X, Y + 1, Z, Block::Wood);
    let hanging = tiles(&mesh(&chunk));
    assert_eq!(hanging.len(), 6);
    assert!(hanging.contains(&[FIRE_TILE.0, FIRE_TILE.1]));
    assert!(hanging.contains(&[FIRE_TILE_ALT.0, FIRE_TILE_ALT.1]));
}

#[test]
fn fire_age_does_not_change_its_mesh() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y - 1, Z, Block::Stone);
    chunk.set(X, Y, Z, Block::Fire);
    let young = mesh(&chunk);
    chunk.set_metadata(X, Y, Z, 15);
    let old = mesh(&chunk);
    assert_eq!(young.masked.vertices(), old.masked.vertices());
}

#[test]
fn flames_are_cut_out_and_hottest_at_the_bottom() {
    let mut flames = FlamesTexture::new(7);
    for _ in 0..80 {
        flames.tick();
    }
    let rgba = flames.rgba();
    assert!(
        rgba.chunks_exact(4)
            .all(|pixel| pixel[3] == 0 || pixel[3] == 255)
    );
    let opaque = |row: usize| {
        (0..16)
            .filter(|x| rgba[(row * 16 + x) * 4 + 3] == 255)
            .count()
    };
    assert!(opaque(15) > opaque(0), "the base burns wider than the tip");
    assert!(opaque(15) >= 12);
    // `TextureFlamesFX`: red never drops below 100.
    assert!(rgba.chunks_exact(4).all(|pixel| pixel[0] >= 100));
}

#[test]
fn flames_change_every_tick() {
    let mut flames = FlamesTexture::new(7);
    for _ in 0..40 {
        flames.tick();
    }
    let before = flames.rgba().to_vec();
    flames.tick();
    assert_ne!(before, flames.rgba());
}
