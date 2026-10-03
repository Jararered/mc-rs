//! Geometry for Beta's first-person block models and extruded atlas sprites.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::rendering::appearance::Appearance;
use crate::rendering::appearance::Shape;
use crate::rendering::meshing::geometry::BlockFaceGeometry;
use crate::rendering::textures::atlas_tile_uvs;

struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Builder {
    fn new() -> Self {
        Self {
            positions: vec![],
            normals: vec![],
            uvs: vec![],
            colors: vec![],
            indices: vec![],
        }
    }

    fn quad(&mut self, p: [[f32; 3]; 4], normal: [f32; 3], uv: [[f32; 2]; 4], tint: [u8; 3]) {
        let start = self.positions.len() as u32;
        self.positions.extend(p);
        self.normals.extend([normal; 4]);
        self.uvs.extend(uv);
        let color = [
            tint[0] as f32 / 255.0,
            tint[1] as f32 / 255.0,
            tint[2] as f32 / 255.0,
            1.0,
        ];
        self.colors.extend([color; 4]);
        self.indices
            .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    }

    fn finish(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn tile_uv(tile: u8, u: f32, v: f32, terrain: bool) -> [f32; 2] {
    let (u0, v0, u1, v1) = if terrain {
        atlas_tile_uvs(tile % 16, tile / 16)
    } else {
        (
            (tile % 16) as f32 / 16.0,
            (tile / 16) as f32 / 16.0,
            (tile % 16 + 1) as f32 / 16.0,
            (tile / 16 + 1) as f32 / 16.0,
        )
    };
    [u0 + (u1 - u0) * u, v0 + (v1 - v0) * v]
}

fn box_part(builder: &mut Builder, bounds: [f32; 6], look: Appearance) {
    let [x0, y0, z0, x1, y1, z1] = bounds;
    let (x0, y0, z0, x1, y1, z1) = (x0 - 0.5, y0 - 0.5, z0 - 0.5, x1 - 0.5, y1 - 0.5, z1 - 0.5);
    let faces = [
        (
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [0., 0., 1.],
            look.right,
        ),
        (
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [0., 0., -1.],
            look.left,
        ),
        (
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [1., 0., 0.],
            look.right,
        ),
        (
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [-1., 0., 0.],
            look.left,
        ),
        (
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            [0., 1., 0.],
            look.top,
        ),
        (
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [0., -1., 0.],
            look.top,
        ),
    ];
    for (p, n, tile) in faces {
        let uv = [
            tile_uv(tile, 0.001, 0.999, true),
            tile_uv(tile, 0.999, 0.999, true),
            tile_uv(tile, 0.999, 0.001, true),
            tile_uv(tile, 0.001, 0.001, true),
        ];
        builder.quad(p, n, uv, look.tint);
    }
}

fn face_geometry_part(builder: &mut Builder, geometry: BlockFaceGeometry, look: Appearance) {
    let tiles = [
        look.top, look.top, look.right, look.left, look.right, look.left,
    ];
    for (face, tile) in geometry.faces().iter().copied().zip(tiles) {
        let corners = face.corners.map(|[x, y, z]| [x - 0.5, y - 0.5, z - 0.5]);
        let uvs = face.uvs.map(|[u, v]| tile_uv(tile, u, v, true));
        builder.quad(corners, face.normal, uvs, look.tint);
    }
}

pub(super) fn block_mesh(id: u8, look: Appearance) -> Mesh {
    let mut builder = Builder::new();
    match look.shape {
        Shape::Cube => box_part(
            &mut builder,
            if id == 60 {
                [0., 0., 0., 1., 0.9375, 1.]
            } else {
                [0., 0., 0., 1., 1., 1.]
            },
            look,
        ),
        Shape::Slab => box_part(
            &mut builder,
            if id == 92 {
                [0.0625, 0., 0.0625, 0.9375, 0.5, 0.9375]
            } else {
                [0., 0., 0., 1., 0.5, 1.]
            },
            look,
        ),
        Shape::Thin => box_part(
            &mut builder,
            match id {
                70 | 72 => [0., 0.375, 0., 1., 0.625, 1.],
                77 => [0.3125, 0.375, 0.375, 0.6875, 0.625, 0.625],
                78 => [0., 0., 0., 1., 0.125, 1.],
                96 => [0., 0.40625, 0., 1., 0.59375, 1.],
                _ => [0., 0., 0., 1., 0.125, 1.],
            },
            look,
        ),
        Shape::Stairs => {
            box_part(&mut builder, [0., 0., 0., 1., 1., 0.5], look);
            box_part(&mut builder, [0., 0., 0.5, 1., 0.5, 1.], look);
        }
        Shape::Fence => {
            for bounds in [
                [0.375, 0., 0., 0.625, 1., 0.25],
                [0.375, 0., 0.75, 0.625, 1., 1.],
                [0.4375, 0.8125, -0.125, 0.5625, 0.9375, 1.125],
                [0.4375, 0.3125, -0.125, 0.5625, 0.4375, 1.125],
            ] {
                box_part(&mut builder, bounds, look);
            }
        }
        Shape::Cactus => face_geometry_part(&mut builder, BlockFaceGeometry::cactus(), look),
        Shape::Flat => unreachable!(),
    }
    builder.finish()
}

/// Two faces and four sets of sixteen strips, matching ItemRenderer's pixel-depth model.
pub(crate) fn sprite_mesh(tile: u8, tint: [u8; 3], terrain: bool) -> Mesh {
    let mut b = Builder::new();
    let u = |x: f32, y: f32| tile_uv(tile, x, y, terrain);
    let z = -1.0 / 16.0;
    let front_uv = [u(1., 1.), u(0., 1.), u(0., 0.), u(1., 0.)];
    b.quad(
        [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
        [0., 0., 1.],
        front_uv,
        tint,
    );
    b.quad(
        [[1., 0., z], [0., 0., z], [0., 1., z], [1., 1., z]],
        [0., 0., -1.],
        [u(0., 1.), u(1., 1.), u(1., 0.), u(0., 0.)],
        tint,
    );
    for i in 0..16 {
        let a = i as f32 / 16.0;
        let c = (i + 1) as f32 / 16.0;
        let xuv = u(1.0 - (i as f32 + 0.5) / 16.0, 0.0);
        let yuv = u(0.0, 1.0 - (i as f32 + 0.5) / 16.0);
        b.quad(
            [[a, 0., z], [a, 0., 0.], [a, 1., 0.], [a, 1., z]],
            [-1., 0., 0.],
            [
                [xuv[0], u(0., 1.)[1]],
                [xuv[0], u(0., 1.)[1]],
                [xuv[0], u(0., 0.)[1]],
                [xuv[0], u(0., 0.)[1]],
            ],
            tint,
        );
        b.quad(
            [[c, 1., z], [c, 1., 0.], [c, 0., 0.], [c, 0., z]],
            [1., 0., 0.],
            [
                [xuv[0], u(0., 0.)[1]],
                [xuv[0], u(0., 0.)[1]],
                [xuv[0], u(0., 1.)[1]],
                [xuv[0], u(0., 1.)[1]],
            ],
            tint,
        );
        b.quad(
            [[0., c, 0.], [1., c, 0.], [1., c, z], [0., c, z]],
            [0., 1., 0.],
            [
                [u(1., 0.)[0], yuv[1]],
                [u(0., 0.)[0], yuv[1]],
                [u(0., 0.)[0], yuv[1]],
                [u(1., 0.)[0], yuv[1]],
            ],
            tint,
        );
        b.quad(
            [[1., a, 0.], [0., a, 0.], [0., a, z], [1., a, z]],
            [0., -1., 0.],
            [
                [u(0., 0.)[0], yuv[1]],
                [u(1., 0.)[0], yuv[1]],
                [u(1., 0.)[0], yuv[1]],
                [u(0., 0.)[0], yuv[1]],
            ],
            tint,
        );
    }
    b.finish()
}
