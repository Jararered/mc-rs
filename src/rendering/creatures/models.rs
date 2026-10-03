//! Beta 1.7.3 creature models: `ModelQuadruped` with its pig, cow, and sheep
//! variants, `ModelChicken`, `ModelSquid`, and `ModelWolf`.
//!
//! Parts are in Beta's model pixels: +Y points down, the head faces -Z, and
//! the feet rest at y = 24. Each `ModelRenderer` here holds one box, posed
//! about its rotation point. [`model_transform`] applies `RenderLiving`'s
//! body yaw, upside-down flip, and lift into the mob's frame.

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::FRAC_PI_4;
use std::f32::consts::PI;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::entity::mobs::MobKind;
use crate::item::ItemId;

/// Every Beta mob skin is 64×32 texels.
const TEXTURE_SIZE: Vec2 = Vec2::new(64.0, 32.0);
/// `RenderLiving`: `glTranslatef(0, -24 * 0.0625 - 0.0078125, 0)` under the
/// flip, which lifts y = 24 to 1/128 above the feet.
const LIFT: f32 = 24.0 / 16.0 + 1.0 / 128.0;

/// `ModelRenderer.addBox`: a box `size` texels wide from `origin`, grown by
/// `inflate` on every side, unwrapped from `texture` on the skin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuboid {
    pub texture: [u8; 2],
    pub origin: Vec3,
    pub size: [u8; 3],
    pub inflate: f32,
    /// `ModelRenderer.mirror`: the box's X extent is flipped, so the skin
    /// reads left-to-right reversed, as for a biped's left limbs.
    pub mirror: bool,
}

/// Which render pass draws a part: the main model, or `RenderPig`'s and
/// `RenderSheep`'s second model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Base,
    /// `ModelPig(0.5)` in `saddle.png`, drawn while saddled.
    Saddle,
    /// `ModelSheep1` in `sheep_fur.png`, tinted by the fleece and drawn
    /// until sheared.
    Fleece,
    /// `RenderSpider`'s second pass: `spider_eyes.png` blended over the
    /// head, more opaque the darker it is.
    Eyes,
    /// `RenderCreeper`'s `ModelCreeper(2.0)` in a scrolling `power.png`,
    /// added over a charged creeper.
    Charge,
    /// `RenderSlime`'s translucent outer cube, `ModelSlime(0)`.
    SlimeOuter,
}

/// How `setRotationAngles` and `setLivingAnimations` move a part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// Keeps the rotation it was built with.
    Fixed,
    /// Turns with the head. `ModelChicken` negates the pitch.
    Head {
        inverted_pitch: bool,
    },
    /// `cos(swing * 0.6662 + phase) * 1.4 * amount` about X.
    Leg {
        phase: f32,
    },
    /// `ModelChicken`'s wings flap about Z by the wing angle.
    Wing {
        sign: f32,
    },
    /// `ModelSquid`'s tentacles bend about X by the tentacle angle.
    Tentacle,
    Wolf(WolfPart),
    /// `ModelZombie`'s arms, held straight out and swaying a little.
    ZombieArm {
        right: bool,
    },
    /// One of `ModelSpider`'s eight legs, numbered from 1.
    SpiderLeg(u8),
    /// `ModelGhast`'s tentacles drift about X with age; each has its own phase.
    GhastTentacle(u8),
}

/// `ModelWolf` repositions most parts when the wolf sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WolfPart {
    /// The head, ears, and snout, drawn with `renderWithRotation`.
    Head,
    Body,
    Mane,
    Leg(u8),
    Tail,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub cuboid: Cuboid,
    /// `rotationPoint`.
    pub pivot: Vec3,
    /// `rotateAngleX/Y/Z` before animation, in radians.
    pub rotation: Vec3,
    pub role: Role,
    pub layer: Layer,
}

/// The values `RenderLiving` hands a model each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseInput {
    /// `legSwing`, interpolated: the walk cycle's phase.
    pub limb_swing: f32,
    /// `legYaw`, interpolated and capped at 1: how far the legs swing.
    pub limb_amount: f32,
    /// `func_170_d`: the chicken's wing angle, the squid's tentacle angle,
    /// or the wolf's tail lift.
    pub special: f32,
    /// Head yaw relative to the body, in degrees.
    pub head_yaw: f32,
    /// Head pitch, in degrees.
    pub head_pitch: f32,
    pub sitting: bool,
    pub angry: bool,
}

fn part(
    texture: [u8; 2],
    origin: [f32; 3],
    size: [u8; 3],
    inflate: f32,
    pivot: [f32; 3],
    role: Role,
    layer: Layer,
) -> Part {
    Part {
        cuboid: Cuboid {
            texture,
            origin: Vec3::from_array(origin),
            size,
            inflate,
            mirror: false,
        },
        pivot: Vec3::from_array(pivot),
        rotation: Vec3::ZERO,
        role,
        layer,
    }
}

fn mirrored(mut part: Part) -> Part {
    part.cuboid.mirror = true;
    part
}

fn rotated(mut part: Part, rotation: Vec3) -> Part {
    part.rotation = rotation;
    part
}

const HEAD: Role = Role::Head {
    inverted_pitch: false,
};
const BODY_PITCH: Vec3 = Vec3::new(FRAC_PI_2, 0.0, 0.0);

/// `ModelQuadruped(legHeight, inflate)`: head, body, and four legs.
fn quadruped(leg: u8, inflate: f32, layer: Layer) -> Vec<Part> {
    let leg_top = 24.0 - f32::from(leg);
    let leg_part = |x: f32, z: f32, phase: f32| {
        part(
            [0, 16],
            [-2.0, 0.0, -2.0],
            [4, leg, 4],
            inflate,
            [x, leg_top, z],
            Role::Leg { phase },
            layer,
        )
    };
    vec![
        part(
            [0, 0],
            [-4.0, -4.0, -8.0],
            [8, 8, 8],
            inflate,
            [0.0, 18.0 - f32::from(leg), -6.0],
            HEAD,
            layer,
        ),
        rotated(
            part(
                [28, 8],
                [-5.0, -10.0, -7.0],
                [10, 16, 8],
                inflate,
                [0.0, 17.0 - f32::from(leg), 2.0],
                Role::Fixed,
                layer,
            ),
            BODY_PITCH,
        ),
        leg_part(-3.0, 7.0, 0.0),
        leg_part(3.0, 7.0, PI),
        leg_part(-3.0, -5.0, PI),
        leg_part(3.0, -5.0, 0.0),
    ]
}

/// The parts of a creature's model, or none for mobs without one.
pub fn model(kind: MobKind) -> Vec<Part> {
    match kind {
        MobKind::Pig => {
            let mut parts = quadruped(6, 0.0, Layer::Base);
            parts.extend(quadruped(6, 0.5, Layer::Saddle));
            parts
        }
        MobKind::Cow => cow(),
        MobKind::Sheep => sheep(),
        MobKind::Chicken => chicken(),
        MobKind::Squid => squid(),
        MobKind::Wolf => wolf(),
        MobKind::Zombie | MobKind::PigZombie => biped(false),
        MobKind::Skeleton => biped(true),
        MobKind::Creeper => {
            let mut parts = creeper(0.0, Layer::Base);
            parts.extend(creeper(2.0, Layer::Charge));
            parts
        }
        MobKind::Spider => spider(),
        MobKind::Slime => slime(),
        MobKind::Ghast => ghast(),
    }
}

/// `ModelZombie`, or `ModelSkeleton`'s thinner limbs on the same frame.
fn biped(skeleton: bool) -> Vec<Part> {
    let base = Layer::Base;
    let (limb_origin, limb) = if skeleton {
        ([-1.0, 0.0, -1.0], [2, 12, 2])
    } else {
        ([-2.0, 0.0, -2.0], [4, 12, 4])
    };
    let (right_arm, left_arm) = if skeleton {
        ([-1.0, -2.0, -1.0], [-1.0, -2.0, -1.0])
    } else {
        ([-3.0, -2.0, -2.0], [-1.0, -2.0, -2.0])
    };
    vec![
        part(
            [0, 0],
            [-4.0, -8.0, -4.0],
            [8, 8, 8],
            0.0,
            [0.0; 3],
            HEAD,
            base,
        ),
        part(
            [16, 16],
            [-4.0, 0.0, -2.0],
            [8, 12, 4],
            0.0,
            [0.0; 3],
            Role::Fixed,
            base,
        ),
        part(
            [40, 16],
            right_arm,
            limb,
            0.0,
            [-5.0, 2.0, 0.0],
            Role::ZombieArm { right: true },
            base,
        ),
        mirrored(part(
            [40, 16],
            left_arm,
            limb,
            0.0,
            [5.0, 2.0, 0.0],
            Role::ZombieArm { right: false },
            base,
        )),
        part(
            [0, 16],
            limb_origin,
            limb,
            0.0,
            [-2.0, 12.0, 0.0],
            Role::Leg { phase: 0.0 },
            base,
        ),
        mirrored(part(
            [0, 16],
            limb_origin,
            limb,
            0.0,
            [2.0, 12.0, 0.0],
            Role::Leg { phase: PI },
            base,
        )),
        // `bipedHeadwear`, a half-pixel larger, drawn last.
        part(
            [32, 0],
            [-4.0, -8.0, -4.0],
            [8, 8, 8],
            0.5,
            [0.0; 3],
            HEAD,
            base,
        ),
    ]
}

/// `ModelCreeper(inflate)`: a head on a body over four short legs.
fn creeper(inflate: f32, layer: Layer) -> Vec<Part> {
    let leg = |x: f32, z: f32, phase: f32| {
        part(
            [0, 16],
            [-2.0, 0.0, -2.0],
            [4, 6, 4],
            inflate,
            [x, 16.0, z],
            Role::Leg { phase },
            layer,
        )
    };
    vec![
        part(
            [0, 0],
            [-4.0, -8.0, -4.0],
            [8, 8, 8],
            inflate,
            [0.0, 4.0, 0.0],
            HEAD,
            layer,
        ),
        part(
            [16, 16],
            [-4.0, 0.0, -2.0],
            [8, 12, 4],
            inflate,
            [0.0, 4.0, 0.0],
            Role::Fixed,
            layer,
        ),
        leg(-2.0, 4.0, 0.0),
        leg(2.0, 4.0, PI),
        leg(-2.0, -4.0, PI),
        leg(2.0, -4.0, 0.0),
    ]
}

/// `ModelSpider`: head, neck, abdomen, and four legs a side. The eyes pass
/// redraws the head.
fn spider() -> Vec<Part> {
    let base = Layer::Base;
    let head = |layer: Layer| {
        part(
            [32, 4],
            [-4.0, -4.0, -8.0],
            [8, 8, 8],
            0.0,
            [0.0, 15.0, -3.0],
            HEAD,
            layer,
        )
    };
    let mut parts = vec![
        head(base),
        part(
            [0, 0],
            [-3.0, -3.0, -3.0],
            [6, 6, 6],
            0.0,
            [0.0, 15.0, 0.0],
            Role::Fixed,
            base,
        ),
        part(
            [0, 12],
            [-5.0, -4.0, -6.0],
            [10, 8, 12],
            0.0,
            [0.0, 15.0, 9.0],
            Role::Fixed,
            base,
        ),
    ];
    for n in 1..=8u8 {
        let left = n % 2 == 1;
        let z = [2.0, 2.0, 1.0, 1.0, 0.0, 0.0, -1.0, -1.0][usize::from(n - 1)];
        parts.push(part(
            [18, 0],
            [if left { -15.0 } else { -1.0 }, -1.0, -1.0],
            [16, 2, 2],
            0.0,
            [if left { -4.0 } else { 4.0 }, 15.0, z],
            Role::SpiderLeg(n),
            base,
        ));
    }
    parts.push(head(Layer::Eyes));
    parts
}

/// `ModelSlime(16)`, the core with its eyes and mouth, and the translucent
/// `ModelSlime(0)` cube around it.
fn slime() -> Vec<Part> {
    let base = Layer::Base;
    vec![
        part(
            [0, 16],
            [-3.0, 17.0, -3.0],
            [6, 6, 6],
            0.0,
            [0.0; 3],
            Role::Fixed,
            base,
        ),
        part(
            [32, 0],
            [-3.25, 18.0, -3.5],
            [2, 2, 2],
            0.0,
            [0.0; 3],
            Role::Fixed,
            base,
        ),
        part(
            [32, 4],
            [1.25, 18.0, -3.5],
            [2, 2, 2],
            0.0,
            [0.0; 3],
            Role::Fixed,
            base,
        ),
        part(
            [32, 8],
            [0.0, 21.0, -3.5],
            [1, 1, 1],
            0.0,
            [0.0; 3],
            Role::Fixed,
            base,
        ),
        part(
            [0, 0],
            [-4.0, 16.0, -4.0],
            [8, 8, 8],
            0.0,
            [0.0; 3],
            Role::Fixed,
            Layer::SlimeOuter,
        ),
    ]
}

/// `ModelGhast`: a cube trailing nine tentacles whose lengths come from
/// `new Random(1660)`.
fn ghast() -> Vec<Part> {
    let mut parts = vec![part(
        [0, 0],
        [-8.0, -8.0, -8.0],
        [16, 16, 16],
        0.0,
        [0.0, 8.0, 0.0],
        Role::Fixed,
        Layer::Base,
    )];
    let mut rng = crate::random::JavaRandom::new(1660);
    for i in 0..9u8 {
        let column = f32::from(i % 3) - f32::from(i / 3 % 2) * 0.5 + 0.25;
        let x = (column / 2.0 * 2.0 - 1.0) * 5.0;
        let z = (f32::from(i / 3) / 2.0 * 2.0 - 1.0) * 5.0;
        let length = rng.next_int(7) as u8 + 8;
        parts.push(part(
            [0, 0],
            [-1.0, 0.0, -1.0],
            [2, length, 2],
            0.0,
            [x, 15.0, z],
            Role::GhastTentacle(i),
            Layer::Base,
        ));
    }
    parts
}

/// `getHeldItem`: skeletons carry a bow and zombie pigmen a gold sword.
pub fn held_item(kind: MobKind) -> Option<ItemId> {
    match kind {
        MobKind::Skeleton => Some(ItemId::Bow),
        MobKind::PigZombie => Some(ItemId::GoldSword),
        _ => None,
    }
}

/// `RenderBiped.renderEquippedItems` and `ItemRenderer.renderItem`, as a
/// transform from the right arm's frame (model pixels) to an extruded
/// sprite spanning the unit square. Swords and tools are held point up;
/// other items, like the bow, lie across the hand.
pub fn held_item_transform(full_3d: bool) -> Transform {
    let degrees = f32::to_radians;
    let into_hand = Mat4::from_translation(Vec3::new(-0.0625, 0.4375, 0.0625));
    let grip = if full_3d {
        Mat4::from_translation(Vec3::new(0.0, 0.1875, 0.0))
            * Mat4::from_scale(Vec3::new(0.625, -0.625, 0.625))
            * Mat4::from_rotation_x(degrees(-100.0))
            * Mat4::from_rotation_y(degrees(45.0))
    } else {
        Mat4::from_translation(Vec3::new(0.25, 0.1875, -0.1875))
            * Mat4::from_scale(Vec3::splat(0.375))
            * Mat4::from_rotation_z(degrees(60.0))
            * Mat4::from_rotation_x(degrees(-90.0))
            * Mat4::from_rotation_z(degrees(20.0))
    };
    let sprite = Mat4::from_translation(Vec3::new(0.0, -0.3, 0.0))
        * Mat4::from_scale(Vec3::splat(1.5))
        * Mat4::from_rotation_y(degrees(50.0))
        * Mat4::from_rotation_z(degrees(335.0))
        * Mat4::from_translation(Vec3::new(-0.9375, -0.0625, 0.0));
    // The arm's frame is in pixels; these offsets are in blocks.
    Transform::from_matrix(Mat4::from_scale(Vec3::splat(16.0)) * into_hand * grip * sprite)
}

/// `ModelCow`: a deeper head with horns, a larger body, udders, and legs
/// set a pixel wider.
fn cow() -> Vec<Part> {
    let mut parts = quadruped(12, 0.0, Layer::Base);
    parts[0] = part(
        [0, 0],
        [-4.0, -4.0, -6.0],
        [8, 8, 6],
        0.0,
        [0.0, 4.0, -8.0],
        HEAD,
        Layer::Base,
    );
    parts[1] = rotated(
        part(
            [18, 4],
            [-6.0, -10.0, -7.0],
            [12, 18, 10],
            0.0,
            [0.0, 5.0, 2.0],
            Role::Fixed,
            Layer::Base,
        ),
        BODY_PITCH,
    );
    for (leg, offset) in parts[2..6].iter_mut().zip([
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(-1.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, -1.0),
    ]) {
        leg.pivot += offset;
    }
    parts.extend([
        part(
            [22, 0],
            [-4.0, -5.0, -4.0],
            [1, 3, 1],
            0.0,
            [0.0, 3.0, -7.0],
            HEAD,
            Layer::Base,
        ),
        part(
            [22, 0],
            [3.0, -5.0, -4.0],
            [1, 3, 1],
            0.0,
            [0.0, 3.0, -7.0],
            HEAD,
            Layer::Base,
        ),
        rotated(
            part(
                [52, 0],
                [-2.0, -3.0, 0.0],
                [4, 6, 2],
                0.0,
                [0.0, 14.0, 6.0],
                Role::Fixed,
                Layer::Base,
            ),
            BODY_PITCH,
        ),
    ]);
    parts
}

/// `ModelSheep2` for the shorn body and `ModelSheep1` for the fleece.
fn sheep() -> Vec<Part> {
    let mut parts = quadruped(12, 0.0, Layer::Base);
    parts[0] = part(
        [0, 0],
        [-3.0, -4.0, -6.0],
        [6, 6, 8],
        0.0,
        [0.0, 6.0, -8.0],
        HEAD,
        Layer::Base,
    );
    parts[1] = rotated(
        part(
            [28, 8],
            [-4.0, -10.0, -7.0],
            [8, 16, 6],
            0.0,
            [0.0, 5.0, 2.0],
            Role::Fixed,
            Layer::Base,
        ),
        BODY_PITCH,
    );
    let fleece_leg = |x: f32, z: f32, phase: f32| {
        part(
            [0, 16],
            [-2.0, 0.0, -2.0],
            [4, 6, 4],
            0.5,
            [x, 12.0, z],
            Role::Leg { phase },
            Layer::Fleece,
        )
    };
    parts.extend([
        part(
            [0, 0],
            [-3.0, -4.0, -4.0],
            [6, 6, 6],
            0.6,
            [0.0, 6.0, -8.0],
            HEAD,
            Layer::Fleece,
        ),
        rotated(
            part(
                [28, 8],
                [-4.0, -10.0, -7.0],
                [8, 16, 6],
                1.75,
                [0.0, 5.0, 2.0],
                Role::Fixed,
                Layer::Fleece,
            ),
            BODY_PITCH,
        ),
        fleece_leg(-3.0, 7.0, 0.0),
        fleece_leg(3.0, 7.0, PI),
        fleece_leg(-3.0, -5.0, PI),
        fleece_leg(3.0, -5.0, 0.0),
    ]);
    parts
}

fn chicken() -> Vec<Part> {
    let head = Role::Head {
        inverted_pitch: true,
    };
    let base = Layer::Base;
    vec![
        part(
            [0, 0],
            [-2.0, -6.0, -2.0],
            [4, 6, 3],
            0.0,
            [0.0, 15.0, -4.0],
            head,
            base,
        ),
        part(
            [14, 0],
            [-2.0, -4.0, -4.0],
            [4, 2, 2],
            0.0,
            [0.0, 15.0, -4.0],
            head,
            base,
        ),
        part(
            [14, 4],
            [-1.0, -2.0, -3.0],
            [2, 2, 2],
            0.0,
            [0.0, 15.0, -4.0],
            head,
            base,
        ),
        rotated(
            part(
                [0, 9],
                [-3.0, -4.0, -3.0],
                [6, 8, 6],
                0.0,
                [0.0, 16.0, 0.0],
                Role::Fixed,
                base,
            ),
            BODY_PITCH,
        ),
        part(
            [26, 0],
            [-1.0, 0.0, -3.0],
            [3, 5, 3],
            0.0,
            [-2.0, 19.0, 1.0],
            Role::Leg { phase: 0.0 },
            base,
        ),
        part(
            [26, 0],
            [-1.0, 0.0, -3.0],
            [3, 5, 3],
            0.0,
            [1.0, 19.0, 1.0],
            Role::Leg { phase: PI },
            base,
        ),
        part(
            [24, 13],
            [0.0, 0.0, -3.0],
            [1, 4, 6],
            0.0,
            [-4.0, 13.0, 0.0],
            Role::Wing { sign: 1.0 },
            base,
        ),
        part(
            [24, 13],
            [-1.0, 0.0, -3.0],
            [1, 4, 6],
            0.0,
            [4.0, 13.0, 0.0],
            Role::Wing { sign: -1.0 },
            base,
        ),
    ]
}

/// `ModelSquid`: the body and eight tentacles in a ring.
fn squid() -> Vec<Part> {
    let mut parts = vec![part(
        [0, 0],
        [-6.0, -8.0, -6.0],
        [12, 16, 12],
        0.0,
        [0.0, 8.0, 0.0],
        Role::Fixed,
        Layer::Base,
    )];
    for i in 0..8 {
        let around = f64::from(i) * std::f64::consts::PI * 2.0 / 8.0;
        let facing = f64::from(i) * std::f64::consts::PI * -2.0 / 8.0 + std::f64::consts::FRAC_PI_2;
        parts.push(rotated(
            part(
                [48, 0],
                [-1.0, 0.0, -1.0],
                [2, 18, 2],
                0.0,
                [around.cos() as f32 * 5.0, 15.0, around.sin() as f32 * 5.0],
                Role::Tentacle,
                Layer::Base,
            ),
            Vec3::new(0.0, facing as f32, 0.0),
        ));
    }
    parts
}

fn wolf() -> Vec<Part> {
    let head = Role::Wolf(WolfPart::Head);
    let base = Layer::Base;
    let leg = |n: u8| {
        part(
            [0, 18],
            [-1.0, 0.0, -1.0],
            [2, 8, 2],
            0.0,
            [0.0; 3],
            Role::Wolf(WolfPart::Leg(n)),
            base,
        )
    };
    vec![
        part(
            [0, 0],
            [-3.0, -3.0, -2.0],
            [6, 6, 4],
            0.0,
            [-1.0, 13.5, -7.0],
            head,
            base,
        ),
        part(
            [18, 14],
            [-4.0, -2.0, -3.0],
            [6, 9, 6],
            0.0,
            [0.0, 14.0, 2.0],
            Role::Wolf(WolfPart::Body),
            base,
        ),
        part(
            [21, 0],
            [-4.0, -3.0, -3.0],
            [8, 6, 7],
            0.0,
            [-1.0, 14.0, 2.0],
            Role::Wolf(WolfPart::Mane),
            base,
        ),
        leg(1),
        leg(2),
        leg(3),
        leg(4),
        part(
            [9, 18],
            [-1.0, 0.0, -1.0],
            [2, 8, 2],
            0.0,
            [-1.0, 12.0, 8.0],
            Role::Wolf(WolfPart::Tail),
            base,
        ),
        part(
            [16, 14],
            [-3.0, -5.0, 0.0],
            [2, 2, 1],
            0.0,
            [-1.0, 13.5, -7.0],
            head,
            base,
        ),
        part(
            [16, 14],
            [1.0, -5.0, 0.0],
            [2, 2, 1],
            0.0,
            [-1.0, 13.5, -7.0],
            head,
            base,
        ),
        part(
            [0, 10],
            [-2.0, 0.0, -5.0],
            [3, 3, 4],
            0.0,
            [-0.5, 13.5, -7.0],
            head,
            base,
        ),
    ]
}

/// `ModelRenderer.render`: rotate about Z, then Y, then X.
fn zyx(rotation: Vec3) -> Quat {
    Quat::from_rotation_z(rotation.z)
        * Quat::from_rotation_y(rotation.y)
        * Quat::from_rotation_x(rotation.x)
}

/// `ModelRenderer.renderWithRotation`: rotate about Y, then X, then Z.
fn yxz(rotation: Vec3) -> Quat {
    Quat::from_rotation_y(rotation.y)
        * Quat::from_rotation_x(rotation.x)
        * Quat::from_rotation_z(rotation.z)
}

fn swing(input: &PoseInput, phase: f32) -> f32 {
    (input.limb_swing * 0.6662 + phase).cos() * 1.4 * input.limb_amount
}

/// A part's pivot and rotation in model pixels for this frame.
pub fn pose(part: &Part, input: &PoseInput) -> Transform {
    let head_yaw = input.head_yaw.to_radians();
    let head_pitch = input.head_pitch.to_radians();
    let (pivot, rotation) = match part.role {
        Role::Fixed => (part.pivot, zyx(part.rotation)),
        Role::Head { inverted_pitch } => {
            let pitch = if inverted_pitch {
                -head_pitch
            } else {
                head_pitch
            };
            (part.pivot, zyx(Vec3::new(pitch, head_yaw, 0.0)))
        }
        Role::Leg { phase } => (part.pivot, zyx(Vec3::new(swing(input, phase), 0.0, 0.0))),
        Role::Wing { sign } => (part.pivot, zyx(Vec3::new(0.0, 0.0, sign * input.special))),
        Role::Tentacle => (
            part.pivot,
            zyx(Vec3::new(input.special, part.rotation.y, 0.0)),
        ),
        Role::Wolf(wolf) => wolf_pose(wolf, part.pivot, input, head_yaw, head_pitch),
        Role::ZombieArm { right } => {
            // `ModelZombie.setRotationAngles`, with no swing in progress.
            let sway = (input.special * 0.09).cos() * 0.05 + 0.05;
            let bob = (input.special * 0.067).sin() * 0.05;
            let rotation = if right {
                Vec3::new(-FRAC_PI_2 + bob, -0.1, sway)
            } else {
                Vec3::new(-FRAC_PI_2 - bob, 0.1, -sway)
            };
            (part.pivot, zyx(rotation))
        }
        Role::SpiderLeg(n) => (part.pivot, zyx(spider_leg(n, input))),
        Role::GhastTentacle(i) => {
            let x = 0.2 * (input.special * 0.3 + f32::from(i)).sin() + 0.4;
            (part.pivot, zyx(Vec3::X * x))
        }
    };
    Transform::from_translation(pivot).with_rotation(rotation)
}

/// `ModelSpider.setRotationAngles` for one leg: splayed out and back, each
/// pair stepping a quarter cycle after the last.
fn spider_leg(n: u8, input: &PoseInput) -> Vec3 {
    let index = usize::from(n - 1);
    let pair = index / 2;
    let sign = if n % 2 == 1 { 1.0 } else { -1.0 };
    let quarter = FRAC_PI_4;
    let eighth = FRAC_PI_4 / 2.0;
    let rest_z = [
        -quarter,
        quarter,
        -quarter * 0.74,
        quarter * 0.74,
        -quarter * 0.74,
        quarter * 0.74,
        -quarter,
        quarter,
    ];
    let rest_y = [
        eighth * 2.0,
        -eighth * 2.0,
        eighth,
        -eighth,
        -eighth,
        eighth,
        -eighth * 2.0,
        eighth * 2.0,
    ];
    let phase = [0.0, PI, FRAC_PI_2, 3.0 * FRAC_PI_2][pair];
    let reach = -((input.limb_swing * 0.6662 * 2.0 + phase).cos() * 0.4) * input.limb_amount;
    let lift = ((input.limb_swing * 0.6662 + phase).sin() * 0.4).abs() * input.limb_amount;
    Vec3::new(
        0.0,
        rest_y[index] + sign * reach,
        rest_z[index] + sign * lift,
    )
}

/// `ModelWolf.setLivingAnimations` followed by `setRotationAngles`. Head
/// tilts and shaking dry are not simulated, so their Z angles stay zero.
fn wolf_pose(part: WolfPart, pivot: Vec3, input: &PoseInput, yaw: f32, pitch: f32) -> (Vec3, Quat) {
    let sitting = input.sitting;
    match part {
        WolfPart::Head => (pivot, yxz(Vec3::new(pitch, yaw, 0.0))),
        WolfPart::Body => {
            if sitting {
                (Vec3::new(0.0, 18.0, 0.0), zyx(Vec3::X * FRAC_PI_4))
            } else {
                (Vec3::new(0.0, 14.0, 2.0), zyx(BODY_PITCH))
            }
        }
        WolfPart::Mane => {
            if sitting {
                (Vec3::new(-1.0, 16.0, -3.0), zyx(Vec3::X * 1.256_637_1))
            } else {
                (Vec3::new(-1.0, 14.0, -3.0), zyx(BODY_PITCH))
            }
        }
        WolfPart::Leg(n) => {
            let x = if n % 2 == 1 { -2.5 } else { 0.5 };
            if sitting {
                let (pivot, angle) = match n {
                    1 | 2 => (Vec3::new(x, 22.0, 2.0), 4.712_389),
                    _ => (Vec3::new(x + 0.01, 17.0, -4.0), 5.811_947),
                };
                (pivot, zyx(Vec3::X * angle))
            } else {
                let (z, phase) = match n {
                    1 => (7.0, 0.0),
                    2 => (7.0, PI),
                    3 => (-4.0, PI),
                    _ => (-4.0, 0.0),
                };
                (
                    Vec3::new(x, 16.0, z),
                    zyx(Vec3::new(swing(input, phase), 0.0, 0.0)),
                )
            }
        }
        WolfPart::Tail => {
            let pivot = if sitting {
                Vec3::new(-1.0, 21.0, 6.0)
            } else {
                Vec3::new(-1.0, 12.0, 8.0)
            };
            let wag = if input.angry { 0.0 } else { swing(input, 0.0) };
            (pivot, yxz(Vec3::new(input.special, wag, 0.0)))
        }
    }
}

/// `EntityWolf.setTailRotation`: raised when angry, lowered as a tamed wolf
/// loses health.
pub fn wolf_tail(angry: bool, tamed: bool, health: i16) -> f32 {
    if angry {
        1.539_380_4
    } else if tamed {
        (0.55 - f32::from(20 - health.min(20)) * 0.02) * PI
    } else {
        0.628_318_55
    }
}

/// How `RenderLiving` places a model this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// `renderYawOffset`, in degrees.
    pub body_yaw: f32,
    /// `RenderSquid`'s pitch and spin, in degrees.
    pub squid: Option<(f32, f32)>,
    /// `preRenderCallback`'s scale: a creeper's swell, a slime's size and
    /// squish, a ghast's bulk.
    pub scale: Vec3,
    /// `rotateCorpse`'s tip onto the side while dying, in degrees.
    pub death_tilt: f32,
}

impl Frame {
    pub fn facing(body_yaw: f32) -> Self {
        Self {
            body_yaw,
            squid: None,
            scale: Vec3::ONE,
            death_tilt: 0.0,
        }
    }
}

/// `RenderLiving.rotateCorpse`'s tilt: up to `max` degrees, easing in over
/// the death animation.
pub fn death_tilt(death_time: f32, max: f32) -> f32 {
    if death_time <= 0.0 {
        return 0.0;
    }
    ((death_time - 1.0) / 20.0 * 1.6).max(0.0).sqrt().min(1.0) * max
}

/// `RenderLiving` from the feet into model pixels: turn to the body's yaw,
/// tip over if dying, flip upside down (`glScalef(-1, -1, 1)`), scale, lift,
/// and scale by 1/16. `RenderSquid` instead tips the body by its pitch and
/// spins it by its own yaw about a point half a block up.
pub fn model_transform(frame: Frame) -> Transform {
    let mut corpse = Quat::from_rotation_y((180.0 - frame.body_yaw).to_radians());
    let lift = Vec3::Y * LIFT * frame.scale.y;
    let translation = if let Some((pitch, yaw)) = frame.squid {
        corpse = corpse
            * Quat::from_rotation_x(pitch.to_radians())
            * Quat::from_rotation_y(yaw.to_radians());
        Vec3::Y * 0.5 + corpse * (lift - Vec3::Y * 1.2)
    } else {
        corpse *= Quat::from_rotation_z(frame.death_tilt.to_radians());
        corpse * lift
    };
    Transform {
        translation,
        rotation: corpse * Quat::from_rotation_z(PI),
        scale: frame.scale / 16.0,
    }
}

/// `ModelRenderer.addBox` and `TexturedQuad`: six quads in model pixels, each
/// sampling its rectangle of the unwrapped skin inset by a tenth of a texel.
pub fn cuboid_mesh(cuboid: &Cuboid) -> Mesh {
    let [w, h, d] = cuboid.size.map(f32::from);
    let [u, v] = cuboid.texture.map(f32::from);
    let mut low = cuboid.origin - Vec3::splat(cuboid.inflate);
    let mut high = cuboid.origin + Vec3::new(w, h, d) + Vec3::splat(cuboid.inflate);
    if cuboid.mirror {
        std::mem::swap(&mut low.x, &mut high.x);
    }
    let corner = |x: bool, y: bool, z: bool| {
        Vec3::new(
            if x { high.x } else { low.x },
            if y { high.y } else { low.y },
            if z { high.z } else { low.z },
        )
    };
    let corners = [
        corner(false, false, false),
        corner(true, false, false),
        corner(true, true, false),
        corner(false, true, false),
        corner(false, false, true),
        corner(true, false, true),
        corner(true, true, true),
        corner(false, true, true),
    ];
    // Vertex order and texture rectangles (u1, v1, u2, v2) as `addBox` lists
    // them: +X, -X, top (-Y), bottom (+Y), front (-Z), back (+Z).
    let faces: [([usize; 4], [f32; 4]); 6] = [
        ([5, 1, 2, 6], [u + d + w, v + d, u + d + w + d, v + d + h]),
        ([0, 4, 7, 3], [u, v + d, u + d, v + d + h]),
        ([5, 4, 0, 1], [u + d, v, u + d + w, v + d]),
        ([2, 3, 7, 6], [u + d + w, v, u + d + w + w, v + d]),
        ([1, 0, 3, 2], [u + d, v + d, u + d + w, v + d + h]),
        (
            [4, 5, 6, 7],
            [u + d + w + d, v + d, u + d + w + d + w, v + d + h],
        ),
    ];
    let inset = Vec2::new(0.1, 0.1) / TEXTURE_SIZE;
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (vertices, [u1, v1, u2, v2]) in faces {
        let base = positions.len() as u16;
        let low_uv = Vec2::new(u1, v1) / TEXTURE_SIZE + inset;
        let high_uv = Vec2::new(u2, v2) / TEXTURE_SIZE - inset;
        let mut quad = [
            (corners[vertices[0]], Vec2::new(high_uv.x, low_uv.y)),
            (corners[vertices[1]], low_uv),
            (corners[vertices[2]], Vec2::new(low_uv.x, high_uv.y)),
            (corners[vertices[3]], high_uv),
        ];
        // `TexturedQuad.flipFace` keeps a mirrored box's faces outward.
        if cuboid.mirror {
            quad.reverse();
        }
        // `TexturedQuad.draw`, with Beta's reversed `subtract`.
        let [(a, _), (b, _), (c, _), _] = quad;
        let normal = (c - b).cross(a - b).normalize();
        for (corner, uv) in quad {
            positions.push(corner.to_array());
            normals.push(normal.to_array());
            uvs.push(uv.to_array());
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U16(indices))
}
