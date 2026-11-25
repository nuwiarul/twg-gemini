use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use twg_shared::shoot_anim::{
    AparSprayConfig, BulletShootConfig, ShootAnimKind, StoneThrowConfig, TearGasConfig,
    eval_parabola_between, parse_apar_spray_config, parse_bullet_shoot_config,
    parse_stone_throw_config, parse_tear_gas_config, shoot_anim_kind,
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    scene::{parse_hex_color, playback::PlaybackOverrides},
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity},
};

const TEX_SIZE: u32 = 128;

const STONE_POOL_COUNT: usize = 28;
const STONE_DUST_POOL_COUNT: usize = 12;

const TEAR_GAS_MAX_SLOTS: usize = 5;
const TEAR_GAS_TRAIL_PER_SLOT: usize = 6;
const TEAR_GAS_SMOKE_PER_SLOT: usize = 26;

const BULLET_POOL_COUNT: usize = 28;

const APAR_PUFF_COUNT: usize = 38;

#[derive(Component)]
pub struct ShootFxEntity;

struct SpriteNode {
    entity: Entity,
    mat: Handle<StandardMaterial>,
}

impl SpriteNode {
    fn despawn(self, commands: &mut Commands, materials: &mut Assets<StandardMaterial>) {
        commands.entity(self.entity).despawn();
        materials.remove(self.mat.id());
    }
}

/// Snapshot of a marker's world launch frame when a discrete projectile is fired during Play mode,
/// so flying rocks/bullets and landed tear-gas smoke stay physically anchored even if the unit moves.
#[derive(Clone, Copy)]
struct ShotAnchor {
    origin: Vec3,
    ground_y: f32,
    aim_yaw_rad: f32,
}

struct StoneThrowRig {
    stone_mat: Handle<StandardMaterial>,
    stones: Vec<Entity>,
    dust_puffs: Vec<SpriteNode>,
    volley_anchors: HashMap<u64, ShotAnchor>,
    last_anim_time: f64,
}

struct TearGasSlotRig {
    canister: Entity,
    trail_wisps: Vec<SpriteNode>,
    impact_puff: SpriteNode,
    smoke_puffs: Vec<SpriteNode>,
}

struct TearGasRig {
    canister_mat: Handle<StandardMaterial>,
    muzzle_puff: SpriteNode,
    slots: Vec<TearGasSlotRig>,
    shot_anchors: HashMap<u64, ShotAnchor>,
    last_anim_time: f64,
}

struct BulletRig {
    core_mat: Handle<StandardMaterial>,
    glow_mat: Handle<StandardMaterial>,
    cores: Vec<Entity>,
    glows: Vec<Entity>,
    muzzle_flash_add: SpriteNode,
    muzzle_flash_blend: SpriteNode,
    muzzle_smoke: SpriteNode,
    shot_anchors: HashMap<u64, ShotAnchor>,
    last_anim_time: f64,
}

struct AparRig {
    core_cone: Entity,
    core_mat: Handle<StandardMaterial>,
    puffs: Vec<SpriteNode>,
}

enum ShootRig {
    StoneThrow(StoneThrowRig),
    TearGas(TearGasRig),
    Bullet(BulletRig),
    Apar(AparRig),
}

impl ShootRig {
    fn kind(&self) -> ShootAnimKind {
        match self {
            Self::StoneThrow(_) => ShootAnimKind::StoneThrow,
            Self::TearGas(_) => ShootAnimKind::TearGas,
            Self::Bullet(_) => ShootAnimKind::Bullet,
            Self::Apar(_) => ShootAnimKind::Apar,
        }
    }

    fn despawn(self, commands: &mut Commands, materials: &mut Assets<StandardMaterial>) {
        match self {
            Self::StoneThrow(r) => {
                materials.remove(r.stone_mat.id());
                for e in r.stones {
                    commands.entity(e).despawn();
                }
                for p in r.dust_puffs {
                    p.despawn(commands, materials);
                }
            }
            Self::TearGas(r) => {
                materials.remove(r.canister_mat.id());
                r.muzzle_puff.despawn(commands, materials);
                for slot in r.slots {
                    commands.entity(slot.canister).despawn();
                    for w in slot.trail_wisps {
                        w.despawn(commands, materials);
                    }
                    slot.impact_puff.despawn(commands, materials);
                    for s in slot.smoke_puffs {
                        s.despawn(commands, materials);
                    }
                }
            }
            Self::Bullet(r) => {
                materials.remove(r.core_mat.id());
                materials.remove(r.glow_mat.id());
                for e in r.cores {
                    commands.entity(e).despawn();
                }
                for e in r.glows {
                    commands.entity(e).despawn();
                }
                r.muzzle_flash_add.despawn(commands, materials);
                r.muzzle_flash_blend.despawn(commands, materials);
                r.muzzle_smoke.despawn(commands, materials);
            }
            Self::Apar(r) => {
                commands.entity(r.core_cone).despawn();
                materials.remove(r.core_mat.id());
                for p in r.puffs {
                    p.despawn(commands, materials);
                }
            }
        }
    }
}

struct ShootFxSharedAssets {
    quad_mesh: Handle<Mesh>,
    rock_mesh: Handle<Mesh>,
    canister_mesh: Handle<Mesh>,
    bullet_mesh: Handle<Mesh>,
    apar_cone_mesh: Handle<Mesh>,
    smoke_tex: Handle<Image>,
    flash_tex: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct ShootFxVisualPool {
    shared: Option<ShootFxSharedAssets>,
    rigs: HashMap<Uuid, ShootRig>,
    last_reset_nonce: u64,
    preview_epoch_sec: f64,
}

impl ShootFxVisualPool {
    fn ensure_shared(
        &mut self,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
    ) -> &ShootFxSharedAssets {
        self.shared.get_or_insert_with(|| ShootFxSharedAssets {
            quad_mesh: meshes.add(Rectangle::new(2.0, 2.0)),
            rock_mesh: meshes.add(build_faceted_rock_mesh()),
            canister_mesh: meshes.add(Cylinder::new(0.14, 0.44)),
            bullet_mesh: meshes.add(Sphere::new(1.0).mesh().uv(10, 6)),
            apar_cone_mesh: meshes.add(build_open_jet_cone_mesh(12, 0.04, 0.36)),
            smoke_tex: images.add(build_soft_cloud_texture(TEX_SIZE)),
            flash_tex: images.add(build_muzzle_flash_texture(TEX_SIZE)),
        })
    }
}

// ---------------------------------------------------------------------------
// Deterministic Hash, Noise & Procedural Meshes / Textures
// ---------------------------------------------------------------------------

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[inline]
fn hash_u64(mut x: u64) -> f32 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x >> 40) as f32) / ((1u32 << 24) as f32)
}

#[inline]
fn hash2d(ix: i32, iy: i32) -> f32 {
    let n = ix
        .wrapping_mul(374_761_393)
        .wrapping_add(iy.wrapping_mul(668_265_263));
    let n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    let n = n ^ (n >> 16);
    ((n as u32) & 0x00ff_ffff) as f32 / 16_777_215.0
}

fn value_noise_2d(x: f32, y: f32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2d(ix, iy);
    let b = hash2d(ix + 1, iy);
    let c = hash2d(ix, iy + 1);
    let d = hash2d(ix + 1, iy + 1);
    let top = a + (b - a) * ux;
    let bot = c + (d - c) * ux;
    top + (bot - top) * uy
}

fn fbm_2d(x: f32, y: f32) -> f32 {
    let mut sum = 0.0_f32;
    let mut amp = 0.55_f32;
    let mut freq = 1.0_f32;
    for _ in 0..3 {
        sum += amp * value_noise_2d(x * freq, y * freq);
        freq *= 2.05;
        amp *= 0.5;
    }
    (sum / 0.9625).clamp(0.0, 1.0)
}

fn make_rgba_texture(size: u32, data: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        ..default()
    });
    image
}

/// Soft feathered billowing cloud texture for white tear-gas smoke, APAR powder/mist & impact dust.
fn build_soft_cloud_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.42, 0.94, r);
            let body = (-r * r * 3.55).exp();
            let n = fbm_2d(u * 2.7 + 5.3, v * 2.7 + 2.9);
            let alpha = (body * (0.76 + 0.24 * n) * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

/// Starburst + Gaussian core texture for firearm muzzle flashes and launcher ignition pops.
fn build_muzzle_flash_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let ang = v.atan2(u);
            let spikes = 0.55 + 0.45 * (ang * 5.0).cos().abs().powf(2.2);
            let core = (-r * r * 9.0).exp();
            let rays = (-r * r * 3.2).exp() * spikes * 0.72;
            let edge = 1.0 - smoothstep(0.65, 0.98, r);
            let alpha = ((core + rays) * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

/// Builds a crisp, irregular faceted 3D rock mesh (perturbed icosahedron with flat face normals)
/// so thrown stones catch directional sunlight naturally while tumbling through the air.
fn build_faceted_rock_mesh() -> Mesh {
    let phi = (1.0_f32 + 5.0_f32.sqrt()) * 0.5;
    let raw_verts = [
        Vec3::new(-1.0, phi, 0.0),
        Vec3::new(1.0, phi, 0.0),
        Vec3::new(-1.0, -phi, 0.0),
        Vec3::new(1.0, -phi, 0.0),
        Vec3::new(0.0, -1.0, phi),
        Vec3::new(0.0, 1.0, phi),
        Vec3::new(0.0, -1.0, -phi),
        Vec3::new(0.0, 1.0, -phi),
        Vec3::new(phi, 0.0, -1.0),
        Vec3::new(phi, 0.0, 1.0),
        Vec3::new(-phi, 0.0, -1.0),
        Vec3::new(-phi, 0.0, 1.0),
    ];
    let verts: Vec<Vec3> = raw_verts
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let n = v.normalize();
            let jitter = 0.76 + 0.42 * hash_u64((i as u64 + 17) * 31);
            let squash = Vec3::new(1.08, 0.86, 0.96);
            n * jitter * squash
        })
        .collect();

    let faces: [[usize; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    let mut positions = Vec::with_capacity(faces.len() * 3);
    let mut normals = Vec::with_capacity(faces.len() * 3);
    let mut uvs = Vec::with_capacity(faces.len() * 3);
    let mut indices = Vec::with_capacity(faces.len() * 3);

    for (fi, [a, b, c]) in faces.iter().enumerate() {
        let va = verts[*a];
        let vb = verts[*b];
        let vc = verts[*c];
        let n = (vb - va).cross(vc - va).normalize_or(Vec3::Y);
        let base = (fi * 3) as u32;
        positions.push(va.to_array());
        positions.push(vb.to_array());
        positions.push(vc.to_array());
        normals.push(n.to_array());
        normals.push(n.to_array());
        normals.push(n.to_array());
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([0.5, 1.0]);
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Builds a smooth open conical frustum extending from `z = 0.0` (radius `r0`) to `z = -1.0`
/// (radius `r1`) so `Transform::looking_to(dir, Vec3::Y)` points the nozzle cone directly along `+dir`.
fn build_open_jet_cone_mesh(sides: usize, r0: f32, r1: f32) -> Mesh {
    let mut positions = Vec::with_capacity(sides * 2);
    let mut normals = Vec::with_capacity(sides * 2);
    let mut uvs = Vec::with_capacity(sides * 2);
    let mut indices = Vec::with_capacity(sides * 6);

    for k in 0..sides {
        let ang = (k as f32) * (TAU / sides as f32);
        let (ca, sa) = (ang.cos(), ang.sin());
        let n = Vec3::new(ca, sa, 0.25).normalize();
        positions.push([ca * r0, sa * r0, 0.0]);
        normals.push(n.to_array());
        uvs.push([k as f32 / sides as f32, 0.0]);

        positions.push([ca * r1, sa * r1, -1.0]);
        normals.push(n.to_array());
        uvs.push([k as f32 / sides as f32, 1.0]);
    }

    for k in 0..sides {
        let kn = (k + 1) % sides;
        let a0 = (k * 2) as u32;
        let a1 = (k * 2 + 1) as u32;
        let b0 = (kn * 2) as u32;
        let b1 = (kn * 2 + 1) as u32;
        indices.extend_from_slice(&[a0, a1, b1, a0, b1, b0]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_sprite(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    quad_mesh: &Handle<Mesh>,
    tex: &Handle<Image>,
    alpha_mode: AlphaMode,
) -> SpriteNode {
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.0),
        base_color_texture: Some(tex.clone()),
        unlit: true,
        alpha_mode,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let entity = commands
        .spawn((
            Mesh3d(quad_mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::IDENTITY,
            Visibility::Hidden,
            ShootFxEntity,
        ))
        .id();
    SpriteNode { entity, mat }
}

fn spawn_stone_throw_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ShootFxSharedAssets,
) -> ShootRig {
    let stone_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.47, 0.44, 0.42),
        perceptual_roughness: 0.92,
        metallic: 0.04,
        ..default()
    });
    let stones = (0..STONE_POOL_COUNT)
        .map(|_| {
            commands
                .spawn((
                    Mesh3d(shared.rock_mesh.clone()),
                    MeshMaterial3d(stone_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    ShootFxEntity,
                ))
                .id()
        })
        .collect();
    let dust_puffs = (0..STONE_DUST_POOL_COUNT)
        .map(|_| {
            spawn_sprite(
                commands,
                materials,
                &shared.quad_mesh,
                &shared.smoke_tex,
                AlphaMode::Blend,
            )
        })
        .collect();
    ShootRig::StoneThrow(StoneThrowRig {
        stone_mat,
        stones,
        dust_puffs,
        volley_anchors: HashMap::new(),
        last_anim_time: -1.0,
    })
}

fn spawn_tear_gas_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ShootFxSharedAssets,
) -> ShootRig {
    let canister_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.38, 0.34),
        metallic: 0.55,
        perceptual_roughness: 0.42,
        ..default()
    });
    let muzzle_puff = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.smoke_tex,
        AlphaMode::Blend,
    );
    let mut slots = Vec::with_capacity(TEAR_GAS_MAX_SLOTS);
    for _ in 0..TEAR_GAS_MAX_SLOTS {
        let canister = commands
            .spawn((
                Mesh3d(shared.canister_mesh.clone()),
                MeshMaterial3d(canister_mat.clone()),
                Transform::IDENTITY,
                Visibility::Hidden,
                ShootFxEntity,
            ))
            .id();
        let trail_wisps = (0..TEAR_GAS_TRAIL_PER_SLOT)
            .map(|_| {
                spawn_sprite(
                    commands,
                    materials,
                    &shared.quad_mesh,
                    &shared.smoke_tex,
                    AlphaMode::Blend,
                )
            })
            .collect();
        let impact_puff = spawn_sprite(
            commands,
            materials,
            &shared.quad_mesh,
            &shared.smoke_tex,
            AlphaMode::Blend,
        );
        let smoke_puffs = (0..TEAR_GAS_SMOKE_PER_SLOT)
            .map(|_| {
                spawn_sprite(
                    commands,
                    materials,
                    &shared.quad_mesh,
                    &shared.smoke_tex,
                    AlphaMode::Blend,
                )
            })
            .collect();
        slots.push(TearGasSlotRig {
            canister,
            trail_wisps,
            impact_puff,
            smoke_puffs,
        });
    }
    ShootRig::TearGas(TearGasRig {
        canister_mat,
        muzzle_puff,
        slots,
        shot_anchors: HashMap::new(),
        last_anim_time: -1.0,
    })
}

fn spawn_bullet_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ShootFxSharedAssets,
) -> ShootRig {
    let core_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.98, 0.85, 0.98),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let glow_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.86, 0.25, 0.65),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let mut cores = Vec::with_capacity(BULLET_POOL_COUNT);
    let mut glows = Vec::with_capacity(BULLET_POOL_COUNT);
    for _ in 0..BULLET_POOL_COUNT {
        cores.push(
            commands
                .spawn((
                    Mesh3d(shared.bullet_mesh.clone()),
                    MeshMaterial3d(core_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    ShootFxEntity,
                ))
                .id(),
        );
        glows.push(
            commands
                .spawn((
                    Mesh3d(shared.bullet_mesh.clone()),
                    MeshMaterial3d(glow_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    ShootFxEntity,
                ))
                .id(),
        );
    }
    let muzzle_flash_add = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.flash_tex,
        AlphaMode::Add,
    );
    let muzzle_flash_blend = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.flash_tex,
        AlphaMode::Blend,
    );
    let muzzle_smoke = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.smoke_tex,
        AlphaMode::Blend,
    );
    ShootRig::Bullet(BulletRig {
        core_mat,
        glow_mat,
        cores,
        glows,
        muzzle_flash_add,
        muzzle_flash_blend,
        muzzle_smoke,
        shot_anchors: HashMap::new(),
        last_anim_time: -1.0,
    })
}

fn spawn_apar_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ShootFxSharedAssets,
) -> ShootRig {
    let core_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.97, 0.99, 1.0, 0.44),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let core_cone = commands
        .spawn((
            Mesh3d(shared.apar_cone_mesh.clone()),
            MeshMaterial3d(core_mat.clone()),
            Transform::IDENTITY,
            Visibility::Hidden,
            ShootFxEntity,
        ))
        .id();
    let puffs = (0..APAR_PUFF_COUNT)
        .map(|_| {
            spawn_sprite(
                commands,
                materials,
                &shared.quad_mesh,
                &shared.smoke_tex,
                AlphaMode::Blend,
            )
        })
        .collect();
    ShootRig::Apar(AparRig {
        core_cone,
        core_mat,
        puffs,
    })
}

// ---------------------------------------------------------------------------
// Main Bevy Runtime System for Stone Throw, Tear Gas, Bullet & APAR
// ---------------------------------------------------------------------------

enum ActiveShootPayload {
    StoneThrow(StoneThrowConfig),
    TearGas(TearGasConfig),
    Bullet(BulletShootConfig),
    Apar(AparSprayConfig),
}

impl ActiveShootPayload {
    fn kind(&self) -> ShootAnimKind {
        match self {
            Self::StoneThrow(_) => ShootAnimKind::StoneThrow,
            Self::TearGas(_) => ShootAnimKind::TearGas,
            Self::Bullet(_) => ShootAnimKind::Bullet,
            Self::Apar(_) => ShootAnimKind::Apar,
        }
    }
}

/// Renders real-time 3D tactical effects for the 4 non-Water-Cannon Shoot animation types:
/// - `StoneThrow`: tumbling faceted 3D stones arcing forward + subtle ground impact dust
/// - `TearGas`: canisters launched upward in high ballistic arc -> hits altitude 0 -> dense white smoke slowly dissipating
/// - `Bullet`: high-speed glowing tracer bullets + muzzle flash that vanish cleanly at `max_range_m`
/// - `Apar`: pressurized fire extinguisher powder/CO2/foam jet cone + sweeping billowing mist cloud
#[allow(clippy::too_many_arguments)]
pub fn shoot_fx_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    mut pool: ResMut<ShootFxVisualPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam_q: Query<&GlobalTransform, With<TacticalCamera>>,
    mut tf_q: Query<(&mut Transform, &mut Visibility), With<ShootFxEntity>>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        for (_, rig) in pool.rigs.drain() {
            rig.despawn(&mut commands, &mut materials);
        }
        return;
    }

    let now_sec = time.elapsed_secs_f64();
    if state.shoot_preview_reset_nonce != pool.last_reset_nonce {
        pool.last_reset_nonce = state.shoot_preview_reset_nonce;
        pool.preview_epoch_sec = now_sec;
    }

    let mut active_ids: HashSet<Uuid> = HashSet::new();
    let mut to_render: Vec<(Uuid, Uuid, ActiveShootPayload, f64, f64, bool)> = Vec::new();

    for anim in &state.anims {
        if state.hidden_entity_ids.contains(&anim.id) {
            continue;
        }
        let Some(kind) = shoot_anim_kind(anim) else {
            continue;
        };
        if kind == ShootAnimKind::WaterCannon {
            continue;
        }
        let Some(marker_id) = anim.marker_id else {
            continue;
        };

        let playback_active = playback.is_playback_active();
        let is_editor_preview =
            !playback_active && state.selected == SelectedEntity::Anim(anim.id);
        let play_phase_opt = if playback_active {
            let status = playback.track_status(anim.id);
            match status {
                AnimRunStatus::Playing | AnimRunStatus::Paused => {
                    let phase = playback
                        .tracks
                        .get(&anim.id)
                        .map(|t| t.distance_m)
                        .unwrap_or(0.0);
                    Some(phase)
                }
                AnimRunStatus::Stopped | AnimRunStatus::Finished => None,
            }
        } else {
            None
        };

        if play_phase_opt.is_none() && !is_editor_preview {
            continue;
        }

        let payload = match kind {
            ShootAnimKind::StoneThrow => ActiveShootPayload::StoneThrow(parse_stone_throw_config(anim)),
            ShootAnimKind::TearGas => ActiveShootPayload::TearGas(parse_tear_gas_config(anim)),
            ShootAnimKind::Bullet => ActiveShootPayload::Bullet(parse_bullet_shoot_config(anim)),
            ShootAnimKind::Apar => ActiveShootPayload::Apar(parse_apar_spray_config(anim)),
            ShootAnimKind::WaterCannon => unreachable!(),
        };

        let dur_sec = twg_shared::shoot_anim::shoot_anim_effective_duration_sec(anim);
        let (anim_time, preview_flag) = if let Some(phase_sec) = play_phase_opt {
            (phase_sec, false)
        } else {
            let preview_t = match &payload {
                ActiveShootPayload::TearGas(cfg) if !cfg.continuous => {
                    let cycle = cfg.total_cycle_duration_sec() + 0.9;
                    (now_sec - pool.preview_epoch_sec).max(0.0) % cycle
                }
                ActiveShootPayload::StoneThrow(cfg) if cfg.single_shot => {
                    let cycle = cfg.single_throw_duration_sec() + 0.75;
                    (now_sec - pool.preview_epoch_sec).max(0.0) % cycle
                }
                ActiveShootPayload::Bullet(cfg) if cfg.single_shot => {
                    let cycle = cfg.single_shot_duration_sec() + 0.75;
                    (now_sec - pool.preview_epoch_sec).max(0.0) % cycle
                }
                _ => now_sec,
            };
            (preview_t, true)
        };

        to_render.push((anim.id, marker_id, payload, anim_time, dur_sec, preview_flag));
        active_ids.insert(anim.id);
    }

    // Despawn inactive / deselected / stopped / deleted rigs
    let stale_ids: Vec<Uuid> = pool
        .rigs
        .keys()
        .copied()
        .filter(|id| !active_ids.contains(id))
        .collect();
    for sid in stale_ids {
        if let Some(rig) = pool.rigs.remove(&sid) {
            rig.despawn(&mut commands, &mut materials);
        }
    }

    if to_render.is_empty() {
        return;
    }

    let cam_rot = cam_q
        .single()
        .map(|gtf| gtf.to_scale_rotation_translation().1)
        .unwrap_or(Quat::IDENTITY);

    let _ = pool.ensure_shared(&mut meshes, &mut images);

    for (anim_id, marker_id, payload, anim_time, dur_sec, is_editor_preview) in to_render {
        let playback_active = playback.is_playback_active();
        let Some(marker) = state
            .markers
            .iter()
            .find(|m| m.id == marker_id && overrides.is_marker_visible(m, playback_active))
        else {
            if let Some(rig) = pool.rigs.remove(&anim_id) {
                rig.despawn(&mut commands, &mut materials);
            }
            continue;
        };

        let want_kind = payload.kind();
        if pool
            .rigs
            .get(&anim_id)
            .is_some_and(|r| r.kind() != want_kind)
            && let Some(old) = pool.rigs.remove(&anim_id)
        {
            old.despawn(&mut commands, &mut materials);
        }

        if !pool.rigs.contains_key(&anim_id) {
            let shared_ref = pool.shared.as_ref().expect("shared shoot fx assets ready");
            let new_rig = match want_kind {
                ShootAnimKind::StoneThrow => {
                    spawn_stone_throw_rig(&mut commands, &mut materials, shared_ref)
                }
                ShootAnimKind::TearGas => {
                    spawn_tear_gas_rig(&mut commands, &mut materials, shared_ref)
                }
                ShootAnimKind::Bullet => {
                    spawn_bullet_rig(&mut commands, &mut materials, shared_ref)
                }
                ShootAnimKind::Apar => spawn_apar_rig(&mut commands, &mut materials, shared_ref),
                ShootAnimKind::WaterCannon => continue,
            };
            pool.rigs.insert(anim_id, new_rig);
        }

        // Track live moving marker pose in Play Skenario or Plot-Editor Route/Group preview
        let (lng, lat, alt, rot_y) = if playback_active
            && let Some(ov) = overrides.markers.get(&marker_id)
        {
            (ov.lng, ov.lat, ov.altitude, ov.rot_y)
        } else {
            (marker.lng, marker.lat, marker.altitude, marker.rot_y)
        };

        let base_local = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt));
        let veh_yaw = (rot_y as f32).to_radians();
        let veh_fwd = Vec3::new(veh_yaw.sin(), 0.0, veh_yaw.cos());
        let veh_right = Vec3::new(-veh_fwd.z, 0.0, veh_fwd.x);

        let Some(rig) = pool.rigs.get_mut(&anim_id) else {
            continue;
        };

        match (rig, payload) {
            (ShootRig::StoneThrow(sr), ActiveShootPayload::StoneThrow(cfg)) => {
                update_stone_throw_rig(
                    sr,
                    &cfg,
                    base_local,
                    veh_fwd,
                    veh_right,
                    rot_y,
                    anim_time,
                    dur_sec,
                    is_editor_preview,
                    cam_rot,
                    &mut materials,
                    &mut tf_q,
                    &mut gizmos,
                );
            }
            (ShootRig::TearGas(tr), ActiveShootPayload::TearGas(cfg)) => {
                update_tear_gas_rig(
                    tr,
                    &cfg,
                    base_local,
                    veh_fwd,
                    veh_right,
                    rot_y,
                    anim_time,
                    is_editor_preview,
                    cam_rot,
                    &mut materials,
                    &mut tf_q,
                    &mut gizmos,
                );
            }
            (ShootRig::Bullet(br), ActiveShootPayload::Bullet(cfg)) => {
                update_bullet_rig(
                    br,
                    &cfg,
                    base_local,
                    veh_fwd,
                    veh_right,
                    rot_y,
                    anim_time,
                    dur_sec,
                    is_editor_preview,
                    cam_rot,
                    &mut materials,
                    &mut tf_q,
                    &mut gizmos,
                );
            }
            (ShootRig::Apar(ar), ActiveShootPayload::Apar(cfg)) => {
                update_apar_rig(
                    ar,
                    &cfg,
                    base_local,
                    veh_fwd,
                    veh_right,
                    rot_y,
                    anim_time,
                    is_editor_preview,
                    cam_rot,
                    &mut materials,
                    &mut tf_q,
                    &mut gizmos,
                );
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Stone Throw Update
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update_stone_throw_rig(
    rig: &mut StoneThrowRig,
    cfg: &StoneThrowConfig,
    base_local: Vec3,
    veh_fwd: Vec3,
    veh_right: Vec3,
    rot_y: f64,
    anim_time: f64,
    dur_sec: f64,
    is_editor_preview: bool,
    cam_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ShootFxEntity>>,
    gizmos: &mut Gizmos,
) {
    if anim_time + 0.01 < rig.last_anim_time || is_editor_preview {
        rig.volley_anchors.clear();
    }
    rig.last_anim_time = anim_time;

    let origin_world = base_local
        + Vec3::Y * (cfg.origin_height_m as f32)
        + veh_fwd * (cfg.origin_forward_m as f32)
        + veh_right * (cfg.origin_right_m as f32);
    let ground_y = if base_local.y >= 0.0 { 0.0 } else { base_local.y };
    let base_aim_yaw = ((rot_y + cfg.yaw_offset_deg) as f32).to_radians();

    if let Some(mut m) = materials.get_mut(&rig.stone_mat) {
        m.base_color = parse_hex_color(&cfg.color_hex, 1.0);
    }

    let period = (1.0 / cfg.throw_rate_hz.clamp(0.2, 6.0)).max(0.12);
    let flight_t = cfg.flight_time_sec();
    let bounce_t = 0.36_f64;
    let total_stone_life = flight_t + bounce_t;
    let per_volley = cfg.stones_per_throw.clamp(1, 6) as u64;

    let (earliest_volley, latest_volley) = if cfg.single_shot {
        (0u64, 0u64)
    } else {
        let max_fire_t = if !is_editor_preview && !cfg.continuous {
            (dur_sec - 0.15).max(0.0)
        } else {
            anim_time
        };
        let latest = (max_fire_t.min(anim_time) / period).floor().max(0.0) as u64;
        let earliest = (((anim_time - total_stone_life - 0.3) / period).floor()).max(0.0) as u64;
        (earliest, latest)
    };

    if !is_editor_preview {
        rig.volley_anchors.retain(|&v, _| v >= earliest_volley);
    }

    let mut stone_slot = 0usize;
    let mut dust_slot = 0usize;

    for v in (earliest_volley..=latest_volley).rev() {
        let anchor = if is_editor_preview {
            ShotAnchor {
                origin: origin_world,
                ground_y,
                aim_yaw_rad: base_aim_yaw,
            }
        } else {
            *rig.volley_anchors.entry(v).or_insert(ShotAnchor {
                origin: origin_world,
                ground_y,
                aim_yaw_rad: base_aim_yaw,
            })
        };

        for k in 0..per_volley {
            if stone_slot >= rig.stones.len() {
                break;
            }
            let fire_t = (v as f64) * period + (k as f64) * 0.055;
            let age = anim_time - fire_t;
            if age < 0.0 || age > total_stone_life {
                continue;
            }

            let seed = v.wrapping_mul(131).wrapping_add(k.wrapping_mul(17)).wrapping_add(7);
            let h_yaw = hash_u64(seed) - 0.5;
            let h_range = 0.86 + 0.28 * hash_u64(seed.wrapping_add(1));
            let h_arc = 0.85 + 0.32 * hash_u64(seed.wrapping_add(2));
            let h_size = 0.78 + 0.44 * hash_u64(seed.wrapping_add(3));

            let yaw = anchor.aim_yaw_rad + (h_yaw * (cfg.spread_deg as f32)).to_radians();
            let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            let s_range = cfg.range_m * (h_range as f64);
            let s_arc = cfg.arc_height_m * (h_arc as f64);
            let stone_r = (cfg.stone_size_m as f32) * h_size;

            let (pos, scale_mult) = if age <= flight_t {
                let u = (age / flight_t).clamp(0.0, 1.0);
                let (horiz, wy, _, _) = eval_parabola_between(
                    s_range,
                    anchor.origin.y as f64,
                    anchor.ground_y as f64,
                    s_arc,
                    u,
                );
                let p = Vec3::new(
                    anchor.origin.x + dir.x * (horiz as f32),
                    (wy as f32).max(anchor.ground_y + stone_r * 0.4),
                    anchor.origin.z + dir.z * (horiz as f32),
                );
                (p, 1.0_f32)
            } else {
                // Short ground bounce + settle then shrink away
                let bu = ((age - flight_t) / bounce_t).clamp(0.0, 1.0) as f32;
                let impact_x = anchor.origin.x + dir.x * (s_range as f32);
                let impact_z = anchor.origin.z + dir.z * (s_range as f32);
                let roll = 1.15 * bu;
                let hop = 0.55 * 4.0 * bu * (1.0 - bu);
                let p = Vec3::new(
                    impact_x + dir.x * roll,
                    anchor.ground_y + stone_r * 0.45 + hop,
                    impact_z + dir.z * roll,
                );
                let shrink = 1.0 - smoothstep(0.68, 1.0, bu);

                if cfg.dust_impact && dust_slot < rig.dust_puffs.len() {
                    let dp = &rig.dust_puffs[dust_slot];
                    dust_slot += 1;
                    let dust_alpha = (1.0 - bu).powf(1.4) * 0.48;
                    let dust_size = (stone_r * 2.2 + 0.35) * (0.55 + bu * 1.65);
                    if let Some(mut dm) = materials.get_mut(&dp.mat) {
                        dm.base_color = Color::srgba(0.82, 0.78, 0.72, dust_alpha);
                    }
                    if let Ok((mut dtf, mut dvis)) = tf_q.get_mut(dp.entity) {
                        dtf.translation = Vec3::new(
                            impact_x,
                            anchor.ground_y + 0.18 + bu * 0.65,
                            impact_z,
                        );
                        dtf.rotation = cam_rot * Quat::from_rotation_z(bu * 1.4 + h_yaw * 4.0);
                        dtf.scale = Vec3::splat(dust_size);
                        *dvis = if dust_alpha > 0.015 {
                            Visibility::Visible
                        } else {
                            Visibility::Hidden
                        };
                    }
                }
                (p, shrink)
            };

            let tumble_x = (age as f32) * (6.5 + 5.0 * hash_u64(seed.wrapping_add(4)));
            let tumble_y = (age as f32) * (5.2 + 4.5 * hash_u64(seed.wrapping_add(5)));
            let tumble_z = (age as f32) * (4.1 + 4.0 * hash_u64(seed.wrapping_add(6)));

            let entity = rig.stones[stone_slot];
            stone_slot += 1;
            if let Ok((mut tf, mut vis)) = tf_q.get_mut(entity) {
                tf.translation = pos;
                tf.rotation = Quat::from_euler(EulerRot::XYZ, tumble_x, tumble_y, tumble_z);
                tf.scale = Vec3::splat((stone_r * scale_mult).max(0.01));
                *vis = if scale_mult > 0.02 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }

    for &e in &rig.stones[stone_slot..] {
        if let Ok((_, mut vis)) = tf_q.get_mut(e) {
            *vis = Visibility::Hidden;
        }
    }
    for dp in &rig.dust_puffs[dust_slot..] {
        if let Ok((_, mut vis)) = tf_q.get_mut(dp.entity) {
            *vis = Visibility::Hidden;
        }
    }

    // Crisp Editor Preview trajectory arc + landing zone circle
    if is_editor_preview {
        let dir = Vec3::new(base_aim_yaw.sin(), 0.0, base_aim_yaw.cos());
        let guide_col = Color::srgba(0.96, 0.72, 0.32, 0.85);
        let soft_col = Color::srgba(0.96, 0.72, 0.32, 0.42);
        gizmos.sphere(Isometry3d::from_translation(origin_world), 0.14, guide_col);
        let steps = 18usize;
        let mut prev = origin_world;
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            let (h, wy, _, _) = eval_parabola_between(
                cfg.range_m,
                origin_world.y as f64,
                ground_y as f64,
                cfg.arc_height_m,
                t,
            );
            let cur = Vec3::new(
                origin_world.x + dir.x * (h as f32),
                (wy as f32).max(ground_y + 0.05),
                origin_world.z + dir.z * (h as f32),
            );
            gizmos.line(prev, cur, soft_col);
            prev = cur;
        }
        let spread_r = (cfg.range_m as f32 * (cfg.spread_deg as f32 * 0.5).to_radians().tan())
            .clamp(0.8, 18.0);
        let land_center = Vec3::new(
            origin_world.x + dir.x * (cfg.range_m as f32),
            ground_y + 0.08,
            origin_world.z + dir.z * (cfg.range_m as f32),
        );
        gizmos.circle(
            Isometry3d::new(land_center, Quat::from_rotation_x(FRAC_PI_2)),
            spread_r,
            guide_col,
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Tear Gas Update (High Arc -> Hits Ground / Altitude 0 -> White Smoke Slowly Fades)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update_tear_gas_rig(
    rig: &mut TearGasRig,
    cfg: &TearGasConfig,
    base_local: Vec3,
    veh_fwd: Vec3,
    veh_right: Vec3,
    rot_y: f64,
    anim_time: f64,
    is_editor_preview: bool,
    cam_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ShootFxEntity>>,
    gizmos: &mut Gizmos,
) {
    if anim_time + 0.01 < rig.last_anim_time || is_editor_preview {
        rig.shot_anchors.clear();
    }
    rig.last_anim_time = anim_time;

    let origin_world = base_local
        + Vec3::Y * (cfg.origin_height_m as f32)
        + veh_fwd * (cfg.origin_forward_m as f32)
        + veh_right * (cfg.origin_right_m as f32);
    let base_aim_yaw = ((rot_y + cfg.yaw_offset_deg) as f32).to_radians();

    let flight_t = cfg.flight_time_sec.max(0.4);
    let smoke_dur = cfg.smoke_duration_sec.max(1.5);
    let canister_life = flight_t + smoke_dur;
    let smoke_col = parse_hex_color(&cfg.smoke_color_hex, 1.0).to_srgba();
    let (wx, wz) = cfg.wind_dir_xz();
    let wind_vec = Vec3::new(wx as f32, 0.0, wz as f32);
    let wind_spd = cfg.wind_speed_mps as f32;
    let smoke_r = cfg.smoke_radius_m as f32;
    let smoke_h = cfg.smoke_height_m as f32;
    let density = cfg.smoke_density as f32;

    // Build list of active shots `(shot_id, fire_time, fan_yaw_offset_deg)`
    let mut active_shots: Vec<(u64, f64, f32)> = Vec::new();
    if cfg.continuous {
        let interval = cfg.refire_interval_sec.max(1.5);
        let latest = (anim_time / interval).floor().max(0.0) as u64;
        let earliest = (((anim_time - canister_life - 0.1) / interval).floor()).max(0.0) as u64;
        if !is_editor_preview {
            rig.shot_anchors.retain(|&id, _| id >= earliest);
        }
        for s_id in (earliest..=latest).rev() {
            if active_shots.len() >= TEAR_GAS_MAX_SLOTS {
                break;
            }
            let fire_t = (s_id as f64) * interval;
            let age = anim_time - fire_t;
            if age >= 0.0 && age <= canister_life {
                let jitter = (hash_u64(s_id.wrapping_mul(97).wrapping_add(13)) - 0.5)
                    * (cfg.salvo_spread_deg as f32);
                active_shots.push((s_id, fire_t, jitter));
            }
        }
    } else {
        let count = cfg.salvo_count.clamp(1, TEAR_GAS_MAX_SLOTS as u32) as usize;
        for i in 0..count {
            let fire_t = (i as f64) * cfg.salvo_interval_sec;
            let age = anim_time - fire_t;
            if age >= 0.0 && age <= canister_life {
                let fan = if count <= 1 {
                    0.0_f32
                } else {
                    let frac = (i as f32) / ((count - 1) as f32) - 0.5;
                    frac * (cfg.salvo_spread_deg as f32)
                };
                active_shots.push((i as u64, fire_t, fan));
            }
        }
    }

    // Brief white launcher pop smoke puff right at muzzle when a canister fires
    let mut best_muzzle_age: Option<f32> = None;
    for &(_, fire_t, _) in &active_shots {
        let a = (anim_time - fire_t) as f32;
        if (0.0..=0.45).contains(&a) && best_muzzle_age.is_none_or(|cur| a < cur) {
            best_muzzle_age = Some(a);
        }
    }
    if let Some(ma) = best_muzzle_age {
        let u = (ma / 0.45).clamp(0.0, 1.0);
        let alpha = (1.0 - u).powf(1.4) * 0.65;
        let aim_dir = Vec3::new(base_aim_yaw.sin(), 0.0, base_aim_yaw.cos());
        if let Some(mut m) = materials.get_mut(&rig.muzzle_puff.mat) {
            m.base_color = Color::srgba(smoke_col.red, smoke_col.green, smoke_col.blue, alpha);
        }
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.muzzle_puff.entity) {
            tf.translation = origin_world + aim_dir * (0.35 + u * 1.1) + Vec3::Y * (u * 0.45);
            tf.rotation = cam_rot * Quat::from_rotation_z(u * 2.2);
            tf.scale = Vec3::splat(0.45 + u * 1.25);
            *vis = Visibility::Visible;
        }
    } else if let Ok((_, mut vis)) = tf_q.get_mut(rig.muzzle_puff.entity) {
        *vis = Visibility::Hidden;
    }

    for (slot_idx, slot) in rig.slots.iter().enumerate() {
        let Some(&(shot_id, fire_t, fan_deg)) = active_shots.get(slot_idx) else {
            // Hide entire unused slot cleanly
            if let Ok((_, mut vis)) = tf_q.get_mut(slot.canister) {
                *vis = Visibility::Hidden;
            }
            if let Ok((_, mut vis)) = tf_q.get_mut(slot.impact_puff.entity) {
                *vis = Visibility::Hidden;
            }
            for w in &slot.trail_wisps {
                if let Ok((_, mut vis)) = tf_q.get_mut(w.entity) {
                    *vis = Visibility::Hidden;
                }
            }
            for sp in &slot.smoke_puffs {
                if let Ok((_, mut vis)) = tf_q.get_mut(sp.entity) {
                    *vis = Visibility::Hidden;
                }
            }
            continue;
        };

        let anchor = if is_editor_preview {
            ShotAnchor {
                origin: origin_world,
                ground_y: 0.0, // Tear gas always detonates at ground / altitude 0.0!
                aim_yaw_rad: base_aim_yaw,
            }
        } else {
            *rig.shot_anchors.entry(shot_id).or_insert(ShotAnchor {
                origin: origin_world,
                ground_y: 0.0,
                aim_yaw_rad: base_aim_yaw,
            })
        };

        let age = (anim_time - fire_t).max(0.0);
        let shot_yaw = anchor.aim_yaw_rad + fan_deg.to_radians();
        let dir = Vec3::new(shot_yaw.sin(), 0.0, shot_yaw.cos());
        let start_y = (anchor.origin.y as f64).max(0.2);

        if age < flight_t {
            // PHASE 1: Airborne high ballistic arc toward ground (altitude 0.0)
            let u = (age / flight_t).clamp(0.0, 1.0);
            let (horiz, wy, _, _) = cfg.eval_canister_arc(start_y, u);
            let can_pos = Vec3::new(
                anchor.origin.x + dir.x * (horiz as f32),
                (wy as f32).max(0.08),
                anchor.origin.z + dir.z * (horiz as f32),
            );
            if let Ok((mut tf, mut vis)) = tf_q.get_mut(slot.canister) {
                tf.translation = can_pos;
                tf.rotation = Quat::from_euler(
                    EulerRot::XYZ,
                    (age as f32) * 8.5,
                    shot_yaw,
                    (age as f32) * 5.2,
                );
                tf.scale = Vec3::ONE;
                *vis = Visibility::Visible;
            }

            // Soft white smoke trail wisps streaming behind the airborne canister
            for (wi, wisp) in slot.trail_wisps.iter().enumerate() {
                let lag_t = u - ((wi + 1) as f64) * 0.048;
                if lag_t <= 0.01 {
                    if let Ok((_, mut vis)) = tf_q.get_mut(wisp.entity) {
                        *vis = Visibility::Hidden;
                    }
                    continue;
                }
                let (th, ty, _, _) = cfg.eval_canister_arc(start_y, lag_t);
                let frac = (wi as f32 + 1.0) / (slot.trail_wisps.len() as f32);
                let alpha = (1.0 - frac * 0.78) * 0.42;
                if let Some(mut m) = materials.get_mut(&wisp.mat) {
                    m.base_color = Color::srgba(smoke_col.red, smoke_col.green, smoke_col.blue, alpha);
                }
                if let Ok((mut tf, mut vis)) = tf_q.get_mut(wisp.entity) {
                    tf.translation = Vec3::new(
                        anchor.origin.x + dir.x * (th as f32),
                        (ty as f32).max(0.12) + frac * 0.25,
                        anchor.origin.z + dir.z * (th as f32),
                    );
                    tf.rotation = cam_rot * Quat::from_rotation_z((wi as f32) * 1.1 + age as f32);
                    tf.scale = Vec3::splat(0.28 + frac * 0.65);
                    *vis = Visibility::Visible;
                }
            }

            // Hide ground smoke until canister touches altitude 0
            if let Ok((_, mut vis)) = tf_q.get_mut(slot.impact_puff.entity) {
                *vis = Visibility::Hidden;
            }
            for sp in &slot.smoke_puffs {
                if let Ok((_, mut vis)) = tf_q.get_mut(sp.entity) {
                    *vis = Visibility::Hidden;
                }
            }
        } else {
            // PHASE 2: Reached Ground (Altitude 0.0) -> White Smoke Blooms & Slowly Dissipates!
            let smoke_age = (age - flight_t).max(0.0);
            let smoke_u = (smoke_age / smoke_dur).clamp(0.0, 1.0) as f32;
            let impact_base = Vec3::new(
                anchor.origin.x + dir.x * (cfg.range_m as f32),
                0.0, // Altitude 0.0
                anchor.origin.z + dir.z * (cfg.range_m as f32),
            );

            // Hide airborne trail wisps
            for w in &slot.trail_wisps {
                if let Ok((_, mut vis)) = tf_q.get_mut(w.entity) {
                    *vis = Visibility::Hidden;
                }
            }

            // Canister settles on the ground at altitude 0 with a brief 0.35s bounce
            let settle_u = (smoke_age as f32 / 0.38).clamp(0.0, 1.0);
            let hop = 0.42 * 4.0 * settle_u * (1.0 - settle_u);
            let can_rest_pos = impact_base + dir * (0.75 * settle_u) + Vec3::Y * (0.12 + hop);
            let can_vis_fade = smoke_u < 0.94;
            if let Ok((mut tf, mut vis)) = tf_q.get_mut(slot.canister) {
                tf.translation = can_rest_pos;
                tf.rotation = Quat::from_rotation_y(shot_yaw + settle_u * 1.2)
                    * Quat::from_rotation_z(FRAC_PI_2);
                tf.scale = Vec3::ONE;
                *vis = if can_vis_fade {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }

            // Initial ground-impact white burst puff (first 0.9s after touching altitude 0)
            let pop_u = (smoke_age as f32 / 0.95).clamp(0.0, 1.0);
            if pop_u < 1.0 {
                let pop_alpha = smoothstep(0.0, 0.12, pop_u) * (1.0 - pop_u).powf(1.5) * 0.72;
                if let Some(mut m) = materials.get_mut(&slot.impact_puff.mat) {
                    m.base_color =
                        Color::srgba(smoke_col.red, smoke_col.green, smoke_col.blue, pop_alpha);
                }
                if let Ok((mut tf, mut vis)) = tf_q.get_mut(slot.impact_puff.entity) {
                    tf.translation = can_rest_pos + Vec3::Y * (0.25 + pop_u * 0.9);
                    tf.rotation = cam_rot * Quat::from_rotation_z(pop_u * 1.4);
                    tf.scale = Vec3::splat(smoke_r * (0.18 + pop_u * 0.55));
                    *vis = Visibility::Visible;
                }
            } else if let Ok((_, mut vis)) = tf_q.get_mut(slot.impact_puff.entity) {
                *vis = Visibility::Hidden;
            }

            // Global cloud lifecycle: brisk white bloom -> dense linger -> slow smooth fade to 0!
            let cloud_bloom = smoothstep(0.0, 0.16, smoke_u);
            let cloud_fade_out = (1.0 - smoothstep(0.38, 1.0, smoke_u)).powf(1.38);
            let global_env = cloud_bloom * cloud_fade_out;

            let puff_count = slot.smoke_puffs.len() as f32;
            let st = smoke_age as f32;

            for (pi, sp) in slot.smoke_puffs.iter().enumerate() {
                let fi = pi as f32;
                let seed = shot_id
                    .wrapping_mul(311)
                    .wrapping_add((pi as u64).wrapping_mul(29))
                    .wrapping_add(11);
                let h1 = hash_u64(seed);
                let h2 = hash_u64(seed.wrapping_add(1));
                let h3 = hash_u64(seed.wrapping_add(2));

                // Staggered organic puff life inside the cloud envelope so smoke actively billows
                let stagger = (fi / puff_count) * 0.22;
                let local_u = ((smoke_u - stagger * 0.45) / (1.0 - stagger * 0.45)).clamp(0.0, 1.0);
                let expand = 1.0 - (1.0 - local_u).powf(2.2);

                let ang = fi * 2.399_963 + st * (0.22 + 0.16 * (h1 - 0.5)) + shot_yaw;
                let radial_frac = (0.12 + 0.88 * h2.sqrt()) * expand;
                let radial = smoke_r * radial_frac * (0.85 + 0.35 * local_u);

                // Heavy low-lying tear gas blanket that rises gently as it thins out
                let vertical_frac = (0.08 + 0.92 * h3) * (0.25 + 0.75 * expand);
                let y = (0.35 + smoke_h * vertical_frac * (0.55 + 0.45 * local_u)).max(0.25);

                let swirl = Vec3::new(
                    (st * 0.75 + fi * 1.7).sin() * smoke_r * 0.08 * expand,
                    0.0,
                    (st * 0.68 + fi * 2.1).cos() * smoke_r * 0.08 * expand,
                );
                let drift = wind_vec * (wind_spd * st * (0.25 + 0.55 * vertical_frac));
                let pos = impact_base
                    + dir * 0.75
                    + Vec3::new(ang.cos() * radial, y, ang.sin() * radial)
                    + swirl
                    + drift;

                // Subtle volumetric lighting gradient (bright pure white crown, softly shaded base)
                let shade = lerp(0.90, 1.0, (y / smoke_h.max(1.0)).clamp(0.0, 1.0));
                let puff_alpha = (global_env
                    * (0.34 + 0.20 * density.min(2.0))
                    * (0.78 + 0.22 * (1.0 - h2)))
                .clamp(0.0, 0.78);

                if let Some(mut m) = materials.get_mut(&sp.mat) {
                    m.base_color = Color::srgba(
                        (smoke_col.red * shade).min(1.0),
                        (smoke_col.green * shade).min(1.0),
                        (smoke_col.blue * shade).min(1.0),
                        puff_alpha,
                    );
                }

                let size = smoke_r
                    * (0.26 + 0.32 * h1)
                    * (0.42 + 0.88 * expand)
                    * (0.85 + 0.15 * density.min(2.0));
                let spin = (fi * 1.85 + st * 0.28 * if pi % 2 == 0 { 1.0 } else { -1.0 })
                    .rem_euclid(TAU);

                if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
                    tf.translation = pos;
                    tf.rotation = cam_rot * Quat::from_rotation_z(spin);
                    tf.scale = Vec3::new(size * 1.15, size * 0.92, 1.0);
                    *vis = if puff_alpha > 0.012 {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                }
            }
        }
    }

    // Crisp Editor Preview ballistic arc + ground smoke coverage ring at Altitude 0
    if is_editor_preview {
        let dir = Vec3::new(base_aim_yaw.sin(), 0.0, base_aim_yaw.cos());
        let guide_col = Color::srgba(0.92, 0.96, 1.0, 0.88);
        let soft_col = Color::srgba(0.75, 0.88, 0.98, 0.50);
        gizmos.sphere(Isometry3d::from_translation(origin_world), 0.16, guide_col);

        let steps = 24usize;
        let mut prev = origin_world;
        let start_y = (origin_world.y as f64).max(0.2);
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            let (h, wy, _, _) = cfg.eval_canister_arc(start_y, t);
            let cur = Vec3::new(
                origin_world.x + dir.x * (h as f32),
                (wy as f32).max(0.05),
                origin_world.z + dir.z * (h as f32),
            );
            gizmos.line(prev, cur, soft_col);
            prev = cur;
        }
        let land_pos = Vec3::new(
            origin_world.x + dir.x * (cfg.range_m as f32),
            0.08,
            origin_world.z + dir.z * (cfg.range_m as f32),
        );
        gizmos.circle(
            Isometry3d::new(land_pos, Quat::from_rotation_x(FRAC_PI_2)),
            smoke_r,
            guide_col,
        );
        gizmos.circle(
            Isometry3d::new(land_pos, Quat::from_rotation_x(FRAC_PI_2)),
            0.6,
            Color::srgba(0.98, 0.75, 0.25, 0.92),
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Bullet Shoot Update (Forward Projectiles Disappearing at `max_range_m`)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update_bullet_rig(
    rig: &mut BulletRig,
    cfg: &BulletShootConfig,
    base_local: Vec3,
    veh_fwd: Vec3,
    veh_right: Vec3,
    rot_y: f64,
    anim_time: f64,
    dur_sec: f64,
    is_editor_preview: bool,
    cam_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ShootFxEntity>>,
    gizmos: &mut Gizmos,
) {
    if anim_time + 0.01 < rig.last_anim_time || is_editor_preview {
        rig.shot_anchors.clear();
    }
    rig.last_anim_time = anim_time;

    let origin_world = base_local
        + Vec3::Y * (cfg.origin_height_m as f32)
        + veh_fwd * (cfg.origin_forward_m as f32)
        + veh_right * (cfg.origin_right_m as f32);
    let base_aim_yaw = ((rot_y + cfg.yaw_offset_deg) as f32).to_radians();
    let pitch_rad = (cfg.pitch_deg as f32).to_radians();

    let base_col = parse_hex_color(&cfg.color_hex, 1.0).to_srgba();
    if let Some(mut m) = materials.get_mut(&rig.core_mat) {
        m.base_color = Color::srgba(
            lerp(base_col.red, 1.0, 0.65),
            lerp(base_col.green, 1.0, 0.65),
            lerp(base_col.blue, 1.0, 0.55),
            0.98,
        );
    }
    if let Some(mut m) = materials.get_mut(&rig.glow_mat) {
        m.base_color = Color::srgba(base_col.red, base_col.green, base_col.blue, 0.68);
    }

    // Find the highest shot index whose firing timestamp <= effective firing window
    let latest_shot = if cfg.single_shot {
        if anim_time >= 0.0 { Some(0) } else { None }
    } else {
        let fire_cutoff = if !is_editor_preview && !cfg.continuous {
            (dur_sec - 0.02).max(0.0).min(anim_time)
        } else {
            anim_time
        };
        find_latest_bullet_shot_index(cfg, fire_cutoff)
    };
    let bullet_life = cfg.bullet_lifetime_sec();

    let mut used_slots = 0usize;
    let mut min_kept_shot = latest_shot.unwrap_or(0);

    if let Some(last_idx) = latest_shot {
        let mut idx = last_idx;
        loop {
            if used_slots >= BULLET_POOL_COUNT {
                break;
            }
            let fire_t = cfg.shot_fire_time_sec(idx);
            let age = anim_time - fire_t;
            if age > bullet_life + 0.05 {
                break;
            }
            min_kept_shot = idx;

            if let Some(dist_m) = cfg.bullet_distance_at_age(age) {
                let anchor = if is_editor_preview {
                    ShotAnchor {
                        origin: origin_world,
                        ground_y: 0.0,
                        aim_yaw_rad: base_aim_yaw,
                    }
                } else {
                    *rig.shot_anchors.entry(idx).or_insert(ShotAnchor {
                        origin: origin_world,
                        ground_y: 0.0,
                        aim_yaw_rad: base_aim_yaw,
                    })
                };

                let seed = idx.wrapping_mul(257).wrapping_add(19);
                let dyaw = ((hash_u64(seed) - 0.5) * (cfg.spread_deg as f32)).to_radians();
                let dpitch =
                    ((hash_u64(seed.wrapping_add(1)) - 0.5) * (cfg.spread_deg as f32) * 0.7)
                        .to_radians();

                let yaw = anchor.aim_yaw_rad + dyaw;
                let p = (pitch_rad + dpitch).clamp(-1.35, 1.35);
                let cp = p.cos();
                let dir = Vec3::new(yaw.sin() * cp, p.sin(), yaw.cos() * cp).normalize_or(Vec3::Z);

                let dist_f = dist_m as f32;
                let max_r = cfg.max_range_m as f32;
                let tracer_len = (cfg.tracer_length_m as f32)
                    .min((dist_f + 0.1).max(0.15))
                    .min(((max_r - dist_f).max(0.05) * 2.5).max(0.15));
                let thick = cfg.tracer_thickness_m as f32;
                let center = anchor.origin + dir * (dist_f - tracer_len * 0.45).max(0.0);

                let rot = Transform::from_translation(center)
                    .looking_to(dir, Vec3::Y)
                    .rotation;

                if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.cores[used_slots]) {
                    tf.translation = center;
                    tf.rotation = rot;
                    tf.scale = Vec3::new(thick * 0.55, thick * 0.55, tracer_len * 0.50);
                    *vis = Visibility::Visible;
                }
                if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.glows[used_slots]) {
                    tf.translation = center;
                    tf.rotation = rot;
                    tf.scale = Vec3::new(thick * 1.45, thick * 1.45, tracer_len * 0.62);
                    *vis = Visibility::Visible;
                }
                used_slots += 1;
            }

            if idx == 0 {
                break;
            }
            idx -= 1;
        }
    }

    if !is_editor_preview {
        rig.shot_anchors.retain(|&id, _| id >= min_kept_shot);
    }

    for i in used_slots..BULLET_POOL_COUNT {
        if let Ok((_, mut vis)) = tf_q.get_mut(rig.cores[i]) {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = tf_q.get_mut(rig.glows[i]) {
            *vis = Visibility::Hidden;
        }
    }

    // Dynamic Muzzle Flash + subtle barrel smoke wisp on each shot
    let muzzle_age_opt = latest_shot.and_then(|idx| {
        let a = anim_time - cfg.shot_fire_time_sec(idx);
        if a >= 0.0 { Some((idx, a as f32)) } else { None }
    });
    let aim_dir = Vec3::new(
        base_aim_yaw.sin() * pitch_rad.cos(),
        pitch_rad.sin(),
        base_aim_yaw.cos() * pitch_rad.cos(),
    )
    .normalize_or(Vec3::Z);

    if cfg.muzzle_flash
        && let Some((shot_idx, ma)) = muzzle_age_opt
        && ma < 0.085
    {
        let fu = (ma / 0.085).clamp(0.0, 1.0);
        let flash_alpha = (1.0 - fu).powf(1.2);
        let flash_r = (0.38 + cfg.tracer_thickness_m as f32 * 2.4) * (1.15 - 0.45 * fu);
        let spin = hash_u64(shot_idx.wrapping_mul(53)) * TAU;
        let flash_pos = origin_world + aim_dir * 0.22;

        if let Some(mut m) = materials.get_mut(&rig.muzzle_flash_add.mat) {
            m.base_color = Color::srgba(1.0, 0.94, 0.70, flash_alpha * 0.95);
        }
        if let Some(mut m) = materials.get_mut(&rig.muzzle_flash_blend.mat) {
            m.base_color = Color::srgba(1.0, 0.62, 0.16, flash_alpha * 0.75);
        }
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.muzzle_flash_add.entity) {
            tf.translation = flash_pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::splat(flash_r);
            *vis = Visibility::Visible;
        }
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.muzzle_flash_blend.entity) {
            tf.translation = flash_pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(-spin * 0.7);
            tf.scale = Vec3::splat(flash_r * 1.28);
            *vis = Visibility::Visible;
        }
    } else {
        if let Ok((_, mut vis)) = tf_q.get_mut(rig.muzzle_flash_add.entity) {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = tf_q.get_mut(rig.muzzle_flash_blend.entity) {
            *vis = Visibility::Hidden;
        }
    }

    if cfg.muzzle_flash
        && let Some((shot_idx, ma)) = muzzle_age_opt
        && ma < 0.24
    {
        let su = (ma / 0.24).clamp(0.0, 1.0);
        let sa = (1.0 - su).powf(1.5) * 0.34;
        if let Some(mut m) = materials.get_mut(&rig.muzzle_smoke.mat) {
            m.base_color = Color::srgba(0.86, 0.88, 0.92, sa);
        }
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.muzzle_smoke.entity) {
            tf.translation = origin_world + aim_dir * (0.35 + su * 0.85) + Vec3::Y * (su * 0.32);
            tf.rotation = cam_rot * Quat::from_rotation_z(hash_u64(shot_idx) * TAU + su);
            tf.scale = Vec3::splat(0.28 + su * 0.65);
            *vis = Visibility::Visible;
        }
    } else if let Ok((_, mut vis)) = tf_q.get_mut(rig.muzzle_smoke.entity) {
        *vis = Visibility::Hidden;
    }

    // Crisp Editor Preview firing line + exact max-range vanish marker
    if is_editor_preview {
        let guide_col = Color::srgba(1.0, 0.86, 0.25, 0.90);
        let soft_col = Color::srgba(1.0, 0.86, 0.25, 0.42);
        let end_pos = origin_world + aim_dir * (cfg.max_range_m as f32);
        gizmos.sphere(Isometry3d::from_translation(origin_world), 0.12, guide_col);
        gizmos.line(origin_world, end_pos, soft_col);
        // Perpendicular range-limit cross at exact max_range_m
        let perp = aim_dir.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = perp.cross(aim_dir).normalize_or(Vec3::Y);
        gizmos.line(end_pos - perp * 0.9, end_pos + perp * 0.9, guide_col);
        gizmos.line(end_pos - up * 0.9, end_pos + up * 0.9, guide_col);
    }
}

/// Finds the largest bullet shot index `i` such that `cfg.shot_fire_time_sec(i) <= t`.
fn find_latest_bullet_shot_index(cfg: &BulletShootConfig, t: f64) -> Option<u64> {
    if t < 0.0 {
        return None;
    }
    let rate = cfg.fire_rate_rps.clamp(0.5, 18.0);
    let mut hi: u64 = ((t * rate * 2.0).ceil() as u64).max(8) + 8;
    while cfg.shot_fire_time_sec(hi) <= t && hi < 1_000_000 {
        hi = hi.saturating_mul(2);
    }
    let mut lo: u64 = 0;
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if cfg.shot_fire_time_sec(mid) <= t {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Some(lo)
}

// ---------------------------------------------------------------------------
// 4. APAR Fire Extinguisher Spray Update
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update_apar_rig(
    rig: &mut AparRig,
    cfg: &AparSprayConfig,
    base_local: Vec3,
    veh_fwd: Vec3,
    veh_right: Vec3,
    rot_y: f64,
    anim_time: f64,
    is_editor_preview: bool,
    cam_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ShootFxEntity>>,
    gizmos: &mut Gizmos,
) {
    let origin_world = base_local
        + Vec3::Y * (cfg.origin_height_m as f32)
        + veh_fwd * (cfg.origin_forward_m as f32)
        + veh_right * (cfg.origin_right_m as f32);
    let ground_y = if base_local.y >= 0.0 { 0.0 } else { base_local.y };

    let base_col = parse_hex_color(&cfg.color_hex, 1.0).to_srgba();
    let range = cfg.range_m as f32;
    let pitch_rad = (cfg.pitch_deg as f32).to_radians();
    let pressure = cfg.pressure as f32;
    let cone_spread = cfg.cone_spread as f32;
    let density = cfg.density as f32;

    let nozzle_yaw = ((rot_y + cfg.effective_yaw_offset_deg(anim_time)) as f32).to_radians();
    let nozzle_dir = Vec3::new(
        nozzle_yaw.sin() * pitch_rad.cos(),
        pitch_rad.sin(),
        nozzle_yaw.cos() * pitch_rad.cos(),
    )
    .normalize_or(Vec3::Z);

    // 1. Short pressurized inner discharge cone right out of the APAR nozzle
    let jet_len = (range * 0.36).clamp(0.8, 7.5);
    let jet_width = (0.75 + cone_spread * 0.85) * (0.85 + 0.15 * range.min(12.0) / 12.0);
    let core_alpha = (0.28 + density * 0.14).clamp(0.18, 0.56);
    if let Some(mut m) = materials.get_mut(&rig.core_mat) {
        m.base_color = Color::srgba(
            lerp(base_col.red, 1.0, 0.35),
            lerp(base_col.green, 1.0, 0.35),
            lerp(base_col.blue, 1.0, 0.35),
            core_alpha,
        );
    }
    if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.core_cone) {
        let mut t = Transform::from_translation(origin_world).looking_to(nozzle_dir, Vec3::Y);
        t.scale = Vec3::new(jet_width, jet_width, jet_len);
        *tf = t;
        *vis = Visibility::Visible;
    }

    // 2. 38 dense pressurized powder / CO2 / foam billowing cloud puffs with PASS sweep lag!
    let flow_freq = 0.85 + pressure * 0.72;
    let sweep_lag_sec = (0.34 / pressure.max(0.4)) as f64;
    let puff_count = rig.puffs.len() as f32;
    let t = anim_time as f32;

    for (i, sp) in rig.puffs.iter().enumerate() {
        let fi = i as f32;
        let speed_var = 0.86 + 0.28 * (fi * 1.91).sin().abs();
        let u = ((fi / puff_count) + t * flow_freq * speed_var).rem_euclid(1.0);

        // Decelerating aerodynamic curve: bursts out rapidly near nozzle, billows & hovers near end
        let dist_frac = 1.0 - (1.0 - u).powf(1.85);
        let dist = range * dist_frac;

        // Evaluate lagged sweep angle so older powder puffs curve naturally during PASS sweeping
        let lagged_time = anim_time - (u as f64) * sweep_lag_sec;
        let puff_yaw = ((rot_y + cfg.effective_yaw_offset_deg(lagged_time)) as f32).to_radians();
        let puff_dir = Vec3::new(
            puff_yaw.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
            puff_yaw.cos() * pitch_rad.cos(),
        )
        .normalize_or(Vec3::Z);
        let right = puff_dir.cross(Vec3::Y).normalize_or(veh_right);
        let up = right.cross(puff_dir).normalize_or(Vec3::Y);

        let ang = fi * 2.399_963 + t * 2.6 + u * 2.1;
        let rad_seed = 0.20 + 0.80 * (fi * 0.618_034).fract();
        let cone_r = (0.06 + range * 0.24 * cone_spread * u.powf(1.12)) * rad_seed;

        let swirl = (right * ang.cos() + up * ang.sin()) * cone_r;
        // Slight upward curl/buoyancy on dispersing powder mist at far end
        let lift = Vec3::Y * (u.powf(2.0) * (0.45 + 0.25 * cone_spread));
        let mut pos = origin_world + puff_dir * dist + swirl + lift;

        // Deflect cleanly off the ground plane so downward APAR spray spreads across the fire base!
        let puff_size = (0.20 + (0.65 + range * 0.13) * cone_spread * u.powf(0.82))
            * (0.78 + 0.42 * (fi * 0.754_877).fract());
        let min_y = ground_y + puff_size * 0.36;
        if pos.y < min_y {
            let push = min_y - pos.y;
            pos.y = min_y;
            // Spread laterally across ground when deflected
            pos += right * (ang.cos() * push * 0.85);
        }

        let fade_in = smoothstep(0.0, 0.08, u);
        let fade_out = (1.0 - smoothstep(0.42, 1.0, u)).powf(1.32);
        let alpha = (fade_in * fade_out * (0.42 + 0.22 * density.min(2.0))).clamp(0.0, 0.84);

        // Bright pure white near pressurized nozzle -> soft tinted mist toward edge
        let shade = lerp(1.0, 0.90, u);
        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(
                (base_col.red * shade).min(1.0),
                (base_col.green * shade).min(1.0),
                (base_col.blue * shade).min(1.0),
                alpha,
            );
        }

        let spin = (fi * 1.7 + t * 1.4 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::splat(puff_size);
            *vis = if alpha > 0.014 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    // Crisp Editor Preview guide
    if is_editor_preview {
        let guide_col = Color::srgba(0.35, 0.92, 0.78, 0.88);
        let soft_col = Color::srgba(0.35, 0.92, 0.78, 0.42);
        let mut end_pos = origin_world + nozzle_dir * range;
        end_pos.y = end_pos.y.max(ground_y + 0.08);
        gizmos.sphere(Isometry3d::from_translation(origin_world), 0.12, guide_col);
        gizmos.line(origin_world, end_pos, soft_col);
        let end_r = (range * 0.25 * cone_spread).clamp(0.5, 10.0);
        gizmos.circle(
            Isometry3d::new(
                Vec3::new(end_pos.x, ground_y + 0.08, end_pos.z),
                Quat::from_rotation_x(FRAC_PI_2),
            ),
            end_r,
            guide_col,
        );
    }
}
