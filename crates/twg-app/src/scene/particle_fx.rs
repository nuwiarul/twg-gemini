use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use twg_shared::particle_anim::{
    ParticleAnimConfig, ParticleAnimKind, is_particle_anim, parse_particle_anim_config,
    particle_anim_position,
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    scene::parse_hex_color,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, ToolMode},
};

const TEX_SIZE: u32 = 128;

const FIRE_OUTER_COUNT: usize = 28;
const FIRE_CORE_COUNT: usize = 18;
const FIRE_SMOKE_COUNT: usize = 16;
const FIRE_EMBER_COUNT: usize = 22;

const SMOKE_PUFF_COUNT: usize = 38;

const EXPL_CORE_BALLS: usize = 10;
const EXPL_OUTER_BALLS: usize = 14;
const EXPL_DEBRIS_COUNT: usize = 20;
const EXPL_MUSHROOM_COUNT: usize = 26;

#[derive(Component)]
pub struct ParticleFxEntity;

struct SpriteParticle {
    entity: Entity,
    mat: Handle<StandardMaterial>,
}

impl SpriteParticle {
    fn despawn(self, commands: &mut Commands, materials: &mut Assets<StandardMaterial>) {
        commands.entity(self.entity).despawn();
        materials.remove(self.mat.id());
    }
}

struct FireRig {
    base_glow: SpriteParticle,
    outer_flames: Vec<SpriteParticle>,
    core_flames: Vec<SpriteParticle>,
    smoke_puffs: Vec<SpriteParticle>,
    embers: Vec<SpriteParticle>,
}

struct SmokeRig {
    base_haze: SpriteParticle,
    puffs: Vec<SpriteParticle>,
}

struct ExplosionRig {
    flash_core: SpriteParticle,
    core_balls: Vec<SpriteParticle>,
    outer_balls: Vec<SpriteParticle>,
    shockwave_outer: SpriteParticle,
    shockwave_inner: SpriteParticle,
    debris: Vec<SpriteParticle>,
    mushroom_smoke: Vec<SpriteParticle>,
}

enum ParticleRig {
    Fire(FireRig),
    Smoke(SmokeRig),
    Explosion(ExplosionRig),
}

impl ParticleRig {
    fn kind(&self) -> ParticleAnimKind {
        match self {
            Self::Fire(_) => ParticleAnimKind::Fire,
            Self::Smoke(_) => ParticleAnimKind::Smoke,
            Self::Explosion(_) => ParticleAnimKind::Explosion,
        }
    }

    fn despawn(self, commands: &mut Commands, materials: &mut Assets<StandardMaterial>) {
        match self {
            Self::Fire(r) => {
                r.base_glow.despawn(commands, materials);
                for p in r.outer_flames {
                    p.despawn(commands, materials);
                }
                for p in r.core_flames {
                    p.despawn(commands, materials);
                }
                for p in r.smoke_puffs {
                    p.despawn(commands, materials);
                }
                for p in r.embers {
                    p.despawn(commands, materials);
                }
            }
            Self::Smoke(r) => {
                r.base_haze.despawn(commands, materials);
                for p in r.puffs {
                    p.despawn(commands, materials);
                }
            }
            Self::Explosion(r) => {
                r.flash_core.despawn(commands, materials);
                r.shockwave_outer.despawn(commands, materials);
                r.shockwave_inner.despawn(commands, materials);
                for p in r.core_balls {
                    p.despawn(commands, materials);
                }
                for p in r.outer_balls {
                    p.despawn(commands, materials);
                }
                for p in r.debris {
                    p.despawn(commands, materials);
                }
                for p in r.mushroom_smoke {
                    p.despawn(commands, materials);
                }
            }
        }
    }
}

struct ParticleSharedAssets {
    quad_mesh: Handle<Mesh>,
    fire_tex: Handle<Image>,
    smoke_tex: Handle<Image>,
    ember_tex: Handle<Image>,
    ring_tex: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct ParticleFxPool {
    shared: Option<ParticleSharedAssets>,
    rigs: HashMap<Uuid, ParticleRig>,
    last_reset_nonce: u64,
    preview_epoch_sec: f64,
}

impl ParticleFxPool {
    fn ensure_shared(
        &mut self,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
    ) -> &ParticleSharedAssets {
        self.shared.get_or_insert_with(|| ParticleSharedAssets {
            quad_mesh: meshes.add(Rectangle::new(2.0, 2.0)),
            fire_tex: images.add(build_fire_glow_texture(TEX_SIZE)),
            smoke_tex: images.add(build_smoke_cloud_texture(TEX_SIZE)),
            ember_tex: images.add(build_ember_bokeh_texture(TEX_SIZE)),
            ring_tex: images.add(build_shockwave_ring_texture(TEX_SIZE)),
        })
    }
}

// ---------------------------------------------------------------------------
// Procedural Soft-Feathered Particle Textures (Zero Hard Polygon Edges)
// ---------------------------------------------------------------------------

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

/// Soft Gaussian radial glow subtly modulated by organic fluid noise for realistic flame plumes.
fn build_fire_glow_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.58, 0.95, r);
            let core = (-r * r * 3.35).exp();
            let n = fbm_2d(u * 2.4 + 3.7, v * 2.4 + 1.9);
            let alpha = (core * (0.84 + 0.16 * n) * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

/// Ultra-soft feathered billowy smoke cloud texture with smooth Gaussian density falloff.
fn build_smoke_cloud_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.42, 0.94, r);
            let body = (-r * r * 3.75).exp();
            let n = fbm_2d(u * 2.6 + 7.1, v * 2.6 + 4.3);
            let alpha = (body * (0.74 + 0.26 * n) * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

/// Soft glowing bokeh orb texture for floating fire sparks & embers.
fn build_ember_bokeh_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.62, 0.96, r);
            let pinpoint = (-r * r * 14.0).exp();
            let halo = 0.52 * (-r * r * 3.5).exp();
            let alpha = ((pinpoint + halo) * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

/// Feathered luminous shockwave ring texture for explosions.
fn build_shockwave_ring_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.84, 0.98, r);
            let band = (-((r - 0.72) / 0.105).powi(2)).exp();
            let inner = 0.24 * (-((r - 0.54) / 0.18).powi(2)).exp();
            let alpha = ((band + inner) * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

// ---------------------------------------------------------------------------
// Sprite Pool Spawners
// ---------------------------------------------------------------------------

fn spawn_sprite(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    quad_mesh: &Handle<Mesh>,
    tex: &Handle<Image>,
    alpha_mode: AlphaMode,
) -> SpriteParticle {
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
            Visibility::Visible,
            ParticleFxEntity,
        ))
        .id();
    SpriteParticle { entity, mat }
}

fn spawn_fire_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ParticleSharedAssets,
) -> ParticleRig {
    let base_glow = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.fire_tex,
        AlphaMode::Add,
    );
    let outer_flames = (0..FIRE_OUTER_COUNT)
        .map(|_| {
            spawn_sprite(
                commands,
                materials,
                &shared.quad_mesh,
                &shared.fire_tex,
                AlphaMode::Blend,
            )
        })
        .collect();
    let core_flames = (0..FIRE_CORE_COUNT)
        .map(|_| {
            spawn_sprite(
                commands,
                materials,
                &shared.quad_mesh,
                &shared.fire_tex,
                AlphaMode::Add,
            )
        })
        .collect();
    let smoke_puffs = (0..FIRE_SMOKE_COUNT)
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
    // Mix of additive luminous gold bokeh + blend warm amber/red bokeh orbs (pops on both light & dark maps)
    let embers = (0..FIRE_EMBER_COUNT)
        .map(|i| {
            let mode = if i % 2 == 0 {
                AlphaMode::Add
            } else {
                AlphaMode::Blend
            };
            spawn_sprite(commands, materials, &shared.quad_mesh, &shared.ember_tex, mode)
        })
        .collect();

    ParticleRig::Fire(FireRig {
        base_glow,
        outer_flames,
        core_flames,
        smoke_puffs,
        embers,
    })
}

fn spawn_smoke_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ParticleSharedAssets,
) -> ParticleRig {
    let base_haze = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.smoke_tex,
        AlphaMode::Blend,
    );
    let puffs = (0..SMOKE_PUFF_COUNT)
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
    ParticleRig::Smoke(SmokeRig { base_haze, puffs })
}

fn spawn_explosion_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    shared: &ParticleSharedAssets,
) -> ParticleRig {
    let flash_core = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.fire_tex,
        AlphaMode::Add,
    );
    let core_balls = (0..EXPL_CORE_BALLS)
        .map(|_| {
            spawn_sprite(
                commands,
                materials,
                &shared.quad_mesh,
                &shared.fire_tex,
                AlphaMode::Add,
            )
        })
        .collect();
    let outer_balls = (0..EXPL_OUTER_BALLS)
        .map(|_| {
            spawn_sprite(
                commands,
                materials,
                &shared.quad_mesh,
                &shared.fire_tex,
                AlphaMode::Blend,
            )
        })
        .collect();
    let shockwave_outer = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.ring_tex,
        AlphaMode::Add,
    );
    let shockwave_inner = spawn_sprite(
        commands,
        materials,
        &shared.quad_mesh,
        &shared.ring_tex,
        AlphaMode::Blend,
    );
    let debris = (0..EXPL_DEBRIS_COUNT)
        .map(|i| {
            let mode = if i % 2 == 0 {
                AlphaMode::Add
            } else {
                AlphaMode::Blend
            };
            spawn_sprite(commands, materials, &shared.quad_mesh, &shared.ember_tex, mode)
        })
        .collect();
    let mushroom_smoke = (0..EXPL_MUSHROOM_COUNT)
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

    ParticleRig::Explosion(ExplosionRig {
        flash_core,
        core_balls,
        outer_balls,
        shockwave_outer,
        shockwave_inner,
        debris,
        mushroom_smoke,
    })
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Renders real-time soft camera-facing billboard Fire, Smoke, and Explosion particle animations:
/// - Editor Mode: previews automatically when selected OR when `always_preview_in_editor` is enabled,
///   plus shows a live placement target ring when `ToolMode::PlaceParticle` is active.
/// - Play Skenario: plays/pauses/stops strictly according to each animation's `PlaybackState` track.
#[allow(clippy::too_many_arguments)]
pub fn particle_fx_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    mut pool: ResMut<ParticleFxPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam_q: Query<&GlobalTransform, With<TacticalCamera>>,
    mut tf_q: Query<(&mut Transform, &mut Visibility), With<ParticleFxEntity>>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        for (_, rig) in pool.rigs.drain() {
            rig.despawn(&mut commands, &mut materials);
        }
        return;
    }

    let now_sec = time.elapsed_secs_f64();
    if state.particle_preview_reset_nonce != pool.last_reset_nonce {
        pool.last_reset_nonce = state.particle_preview_reset_nonce;
        pool.preview_epoch_sec = now_sec;
    }

    // Live cursor placement target ring when placing a new particle on the map
    if !playback.play_test_mode
        && state.tool_mode == ToolMode::PlaceParticle
        && let Some([clng, clat]) = state.cursor_lnglat
    {
        let loc = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.14));
        let col = match state.new_particle_kind {
            ParticleAnimKind::Fire => Color::srgba(1.0, 0.48, 0.10, 0.95),
            ParticleAnimKind::Smoke => Color::srgba(0.68, 0.82, 0.98, 0.95),
            ParticleAnimKind::Explosion => Color::srgba(1.0, 0.25, 0.25, 0.95),
        };
        let def = ParticleAnimConfig::default_for(state.new_particle_kind);
        let r = (def.radius_m as f32).clamp(1.2, 18.0);
        let pulse = 1.0 + 0.08 * (now_sec as f32 * 5.0).sin();
        let flat = Quat::from_rotation_x(FRAC_PI_2);
        gizmos.circle(Isometry3d::new(loc, flat), r * pulse, col);
        gizmos.circle(Isometry3d::new(loc, flat), r * 0.35, col);
        gizmos.line(loc - Vec3::X * r * 0.6, loc + Vec3::X * r * 0.6, col);
        gizmos.line(loc - Vec3::Z * r * 0.6, loc + Vec3::Z * r * 0.6, col);
    }

    let mut active_ids: HashSet<Uuid> = HashSet::new();
    let mut to_render: Vec<(Uuid, ParticleAnimConfig, [f64; 3], f64, f64, bool)> = Vec::new();

    for anim in &state.anims {
        if !is_particle_anim(anim)
            || anim.geom.is_empty()
            || state.hidden_entity_ids.contains(&anim.id)
        {
            continue;
        }
        let cfg = parse_particle_anim_config(anim);
        let pos = particle_anim_position(anim);
        let dur_sec = anim.duration_sec.max(0.5);

        if playback.is_playback_active() {
            let status = playback.track_status(anim.id);
            if matches!(status, AnimRunStatus::Playing | AnimRunStatus::Paused) {
                let elapsed = playback
                    .tracks
                    .get(&anim.id)
                    .map(|t| t.distance_m)
                    .unwrap_or(0.0);
                to_render.push((anim.id, cfg, pos, elapsed, dur_sec, false));
                active_ids.insert(anim.id);
            }
        } else {
            let is_sel = state.selected == SelectedEntity::Anim(anim.id);
            if is_sel || cfg.always_preview_in_editor {
                let elapsed = if cfg.kind == ParticleAnimKind::Explosion {
                    let cycle = dur_sec + 0.85;
                    (now_sec - pool.preview_epoch_sec).max(0.0) % cycle
                } else {
                    now_sec
                };
                to_render.push((anim.id, cfg, pos, elapsed, dur_sec, is_sel));
                active_ids.insert(anim.id);
            }
        }
    }

    // Despawn inactive or deleted particle rigs (and reclaim pooled StandardMaterials)
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

    // Camera rotation for true view-facing billboards
    let (cam_rot, top_down_factor) = cam_q
        .single()
        .map(|gtf| {
            let (_, rot, _) = gtf.to_scale_rotation_translation();
            let fwd_y = gtf.forward().as_vec3().y.abs();
            (rot, fwd_y)
        })
        .unwrap_or((Quat::IDENTITY, 0.35));

    // Ensure shared quad mesh + 4 soft procedural textures are ready
    let _ = pool.ensure_shared(&mut meshes, &mut images);
    let flat_rot = Quat::from_rotation_x(-FRAC_PI_2);

    for (anim_id, cfg, geo_pos, elapsed, dur_sec, is_selected_in_editor) in to_render {
        // Recreate rig if user switched particle kind in Inspector (e.g. Fire -> Smoke)
        if pool
            .rigs
            .get(&anim_id)
            .is_some_and(|r| r.kind() != cfg.kind)
            && let Some(old) = pool.rigs.remove(&anim_id)
        {
            old.despawn(&mut commands, &mut materials);
        }

        let shared_ref = pool.shared.as_ref().expect("shared particle assets ready");
        let needs_spawn = !pool.rigs.contains_key(&anim_id);
        if needs_spawn {
            let new_rig = match cfg.kind {
                ParticleAnimKind::Fire => spawn_fire_rig(&mut commands, &mut materials, shared_ref),
                ParticleAnimKind::Smoke => {
                    spawn_smoke_rig(&mut commands, &mut materials, shared_ref)
                }
                ParticleAnimKind::Explosion => {
                    spawn_explosion_rig(&mut commands, &mut materials, shared_ref)
                }
            };
            pool.rigs.insert(anim_id, new_rig);
        }

        let Some(rig) = pool.rigs.get(&anim_id) else {
            continue;
        };

        let origin = Vec3::from_array(
            state
                .geo_anchor
                .lng_lat_alt_to_local(geo_pos[0], geo_pos[1], geo_pos[2]),
        );
        let (wx, wz) = cfg.wind_dir_xz();
        let wind_vec = Vec3::new(wx as f32, 0.0, wz as f32);
        let wind_spd = cfg.wind_speed_mps as f32;
        let radius = cfg.radius_m as f32;
        let height = cfg.height_m as f32;
        let intensity = cfg.intensity as f32;
        let rise_spd = cfg.rise_speed as f32;
        let spread = cfg.spread as f32;
        let base_srgb = parse_hex_color(&cfg.color_hex, 1.0).to_srgba();
        let t = elapsed as f32;

        // Subtle footprint guide circle when selected in Editor
        if is_selected_in_editor {
            let guide_col = match cfg.kind {
                ParticleAnimKind::Fire => Color::srgba(1.0, 0.56, 0.14, 0.65),
                ParticleAnimKind::Smoke => Color::srgba(0.60, 0.78, 0.96, 0.65),
                ParticleAnimKind::Explosion => Color::srgba(1.0, 0.30, 0.25, 0.70),
            };
            gizmos.circle(
                Isometry3d::new(origin + Vec3::Y * 0.08, Quat::from_rotation_x(FRAC_PI_2)),
                radius,
                guide_col,
            );
        }

        match rig {
            ParticleRig::Fire(fr) => {
                update_fire_rig(
                    fr,
                    origin,
                    wind_vec,
                    wind_spd,
                    radius,
                    height,
                    intensity,
                    rise_spd,
                    spread,
                    base_srgb,
                    cfg.with_smoke,
                    cfg.with_embers,
                    t,
                    cam_rot,
                    top_down_factor,
                    flat_rot,
                    &mut materials,
                    &mut tf_q,
                );
            }
            ParticleRig::Smoke(sr) => {
                update_smoke_rig(
                    sr,
                    origin,
                    wind_vec,
                    wind_spd,
                    radius,
                    height,
                    intensity,
                    rise_spd,
                    spread,
                    base_srgb,
                    t,
                    cam_rot,
                    flat_rot,
                    &mut materials,
                    &mut tf_q,
                );
            }
            ParticleRig::Explosion(er) => {
                update_explosion_rig(
                    er,
                    origin,
                    wind_vec,
                    wind_spd,
                    radius,
                    height,
                    intensity,
                    spread,
                    base_srgb,
                    cfg.shockwave_enabled,
                    cfg.with_embers,
                    cfg.with_smoke,
                    (elapsed / dur_sec.max(0.5)) as f32,
                    cam_rot,
                    flat_rot,
                    &mut materials,
                    &mut tf_q,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_fire_rig(
    fr: &FireRig,
    origin: Vec3,
    wind_vec: Vec3,
    wind_spd: f32,
    radius: f32,
    height: f32,
    intensity: f32,
    rise_spd: f32,
    spread: f32,
    base_srgb: Srgba,
    with_smoke: bool,
    with_embers: bool,
    t: f32,
    cam_rot: Quat,
    top_down_factor: f32,
    flat_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ParticleFxEntity>>,
) {
    let flicker = 1.0 + 0.08 * (t * 11.3).sin() + 0.05 * (t * 19.7).cos();
    let vert_stretch = 1.0 + 0.38 * (1.0 - top_down_factor * 0.65);
    let hot_r = lerp(base_srgb.red, 1.0, 0.85);
    let hot_g = lerp(base_srgb.green, 0.96, 0.78);
    let hot_b = lerp(base_srgb.blue, 0.72, 0.62);

    // 1. Soft glowing ground aura beneath the flame
    if let Some(mut m) = materials.get_mut(&fr.base_glow.mat) {
        m.base_color = Color::srgba(
            lerp(base_srgb.red, 1.0, 0.35),
            lerp(base_srgb.green, 0.68, 0.25),
            base_srgb.blue * 0.45,
            (0.32 + 0.14 * intensity).clamp(0.14, 0.68),
        );
    }
    if let Ok((mut tf, mut vis)) = tf_q.get_mut(fr.base_glow.entity) {
        tf.translation = origin + Vec3::Y * 0.08;
        tf.rotation = flat_rot;
        tf.scale = Vec3::new(radius * 1.15 * flicker, radius * 1.15 * flicker, 1.0);
        *vis = Visibility::Visible;
    }

    // 2. Outer & rising turbulent fiery flame wisps (AlphaMode::Blend for rich orange/crimson on light & dark maps)
    let flame_speed = 0.76 * rise_spd;
    for (i, sp) in fr.outer_flames.iter().enumerate() {
        let fi = i as f32;
        let speed_var = 0.82 + 0.36 * (fi * 1.73).sin().abs();
        let u = ((fi / FIRE_OUTER_COUNT as f32) + t * flame_speed * speed_var).rem_euclid(1.0);

        // Tear-drop lateral profile: narrow base -> bulging mid flame -> drifting curling wisps
        let base_tight = 0.14 + 0.48 * (u * PI).sin().powf(0.75) + u * (0.28 + spread * 0.55);
        let radial = radius * base_tight * (0.25 + 0.75 * (fi * 0.618_034).fract());
        let ang = fi * 2.399_963 + t * (1.25 + 0.4 * (fi * 0.91).sin()) + u * 1.6;
        let sway = Vec3::new(
            (t * 4.4 + fi * 2.3 + u * 5.0).sin() * radius * 0.14 * u,
            0.0,
            (t * 3.9 + fi * 1.9 + u * 4.6).cos() * radius * 0.14 * u,
        );
        let wind_drift = wind_vec * (wind_spd * u.powf(1.45) * (height * 0.14).max(0.8));
        let y = (radius * 0.18).min(height * 0.12) + height * 0.95 * u.powf(0.92);
        let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + sway + wind_drift;

        // Smooth color evolution: creamy hot core -> rich orange -> soft coral-red wisps
        let (cr, cg, cb) = if u < 0.30 {
            let k = u / 0.30;
            (
                lerp(hot_r, base_srgb.red, k),
                lerp(hot_g, (base_srgb.green * 1.15).min(1.0), k),
                lerp(hot_b, base_srgb.blue, k),
            )
        } else {
            let k = ((u - 0.30) / 0.70).clamp(0.0, 1.0);
            (
                lerp(base_srgb.red, (base_srgb.red * 0.98).min(1.0), k),
                lerp((base_srgb.green * 1.15).min(1.0), base_srgb.green * 0.38, k),
                lerp(base_srgb.blue, base_srgb.blue * 0.45, k),
            )
        };
        let fade_in = smoothstep(0.0, 0.12, u);
        let fade_out = (1.0 - smoothstep(0.32, 1.0, u)).powf(1.45);
        let alpha = (fade_in * fade_out * (0.44 + 0.18 * intensity.min(2.0))).clamp(0.0, 0.82);

        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(cr, cg, cb, alpha);
        }

        // Puff size swells in mid flame then shrinks gently as wisps break apart
        let size_env = 0.50 + 0.58 * (u * PI).sin().powf(0.55);
        let sx = radius * (0.42 + 0.18 * spread) * size_env * (0.85 + 0.25 * intensity.min(2.0));
        let sy = sx * lerp(vert_stretch, 1.0, u);
        let spin = (fi * 1.9 + t * 0.95 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);

        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::new(sx, sy, 1.0);
            *vis = if alpha > 0.01 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    // 3. Incandescent luminous heart of the flame (AlphaMode::Add -> bright creamy yellow-white glow!)
    let core_speed = 0.96 * rise_spd;
    for (i, sp) in fr.core_flames.iter().enumerate() {
        let fi = i as f32;
        let speed_var = 0.86 + 0.28 * (fi * 2.17).cos().abs();
        let u = ((fi / FIRE_CORE_COUNT as f32) + t * core_speed * speed_var).rem_euclid(1.0);

        // Tighter tear-drop body concentrated at 0..60% of flame height
        let width_profile = 0.08 + 0.30 * (u * PI).sin().powf(0.65);
        let radial = radius * width_profile * (0.2 + 0.8 * (fi * 0.754_877).fract());
        let ang = fi * 2.399_963 + 1.1 + t * 1.8;
        let sway = Vec3::new(
            (t * 6.2 + fi * 2.5).sin() * radius * 0.08 * u,
            0.0,
            (t * 5.6 + fi * 2.1).cos() * radius * 0.08 * u,
        );
        let wind_drift = wind_vec * (wind_spd * u.powf(1.5) * (height * 0.08).max(0.45));
        let y = (radius * 0.16).min(height * 0.10) + height * 0.56 * u;
        let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + sway + wind_drift;

        let cr = lerp(1.0, base_srgb.red, u * 0.45);
        let cg = lerp(0.96, (base_srgb.green * 1.15).min(1.0), u * 0.70);
        let cb = lerp(0.76, (base_srgb.blue + 0.12).min(1.0), u * 0.85);
        let fade_in = smoothstep(0.0, 0.10, u);
        let fade_out = (1.0 - smoothstep(0.26, 1.0, u)).powf(1.5);
        let alpha = (fade_in * fade_out * (0.50 + 0.22 * intensity.min(2.0))).clamp(0.0, 0.90);

        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(cr, cg, cb, alpha);
        }

        let size_env = 0.45 + 0.65 * (u * PI).sin().powf(0.5);
        let sx = radius * 0.38 * size_env * (0.85 + 0.22 * intensity.min(2.0));
        let sy = sx * lerp(vert_stretch * 1.18, 1.0, u * 0.7);
        let spin = (fi * 2.4 - t * 0.8).rem_euclid(TAU);

        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::new(sx, sy, 1.0);
            *vis = if alpha > 0.01 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    // 4. Soft feathered dark smoke rising gently above the flames (optional)
    let smoke_speed = 0.26 * rise_spd;
    for (i, sp) in fr.smoke_puffs.iter().enumerate() {
        if !with_smoke {
            if let Ok((_, mut vis)) = tf_q.get_mut(sp.entity) {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        let fi = i as f32;
        let u = ((fi / FIRE_SMOKE_COUNT as f32) + t * smoke_speed * (0.88 + 0.24 * (fi * 1.31).sin().abs()))
            .rem_euclid(1.0);
        let ang = fi * 2.399_963 + t * 0.35;
        let radial = radius * (0.22 + u * (0.72 + spread * 1.1)) * (0.3 + 0.7 * (fi * 0.618).fract());
        let smoke_base_y = height * 0.85;
        let smoke_rise = height * 1.55 * u;
        let wind_drift = wind_vec * (wind_spd * (0.28 + 1.25 * u.powf(1.4)) * (height * 0.14).max(1.2));
        let pos = origin
            + Vec3::new(ang.cos() * radial, smoke_base_y + smoke_rise, ang.sin() * radial)
            + wind_drift;

        let fade_in = smoothstep(0.0, 0.22, u);
        let fade_out = (1.0 - smoothstep(0.45, 1.0, u)).powf(1.45);
        let alpha = (fade_in * fade_out * (0.18 + 0.08 * intensity.min(2.0))).clamp(0.0, 0.36);
        let shade = lerp(0.14, 0.26, u);
        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(shade, shade + 0.01, shade + 0.03, alpha);
        }

        let r_puff = radius * (0.48 + u * (0.95 + spread * 0.85));
        let spin = (fi * 1.3 + t * 0.32 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::new(r_puff, r_puff, 1.0);
            *vis = if alpha > 0.008 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    // 5. Luminous floating spark / ember bokeh orbs rising high above the flame (optional)
    let ember_speed = 0.48 * rise_spd;
    for (i, sp) in fr.embers.iter().enumerate() {
        if !with_embers {
            if let Ok((_, mut vis)) = tf_q.get_mut(sp.entity) {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        let fi = i as f32;
        let u = ((fi / FIRE_EMBER_COUNT as f32)
            + t * ember_speed * (0.72 + 0.56 * (fi * 0.618_034).fract()))
        .rem_euclid(1.0);
        let ang = fi * 2.399_963 + t * 1.35 + u * 3.2;
        let radial = radius * (0.18 + u * (0.95 + spread * 0.65)) * (0.35 + 0.65 * (fi * 0.754).fract());
        let flutter = Vec3::new(
            (t * 4.8 + fi * 3.1).sin() * radius * 0.14 * u,
            0.0,
            (t * 4.1 + fi * 2.7).cos() * radius * 0.14 * u,
        );
        let wind_drift = wind_vec * (wind_spd * 0.85 * u.powf(1.4) * (height * 0.15).max(1.0));
        let y = height * (0.24 + 1.48 * u);
        let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + flutter + wind_drift;

        let twinkle = 0.72 + 0.28 * (t * 14.0 + fi * 5.7).sin();
        let fade_in = smoothstep(0.0, 0.12, u);
        let fade_out = (1.0 - smoothstep(0.55, 1.0, u)).powf(1.3);
        let alpha = (fade_in * fade_out * twinkle * (0.72 + 0.18 * intensity.min(1.8))).clamp(0.0, 0.96);

        // Even sparks: creamy golden bokeh (Add); odd sparks: glowing warm coral/orange bokeh (Blend)
        let (er, eg, eb) = if i % 2 == 0 {
            (
                lerp(1.0, base_srgb.red, 0.25),
                lerp(0.92, base_srgb.green, 0.35),
                lerp(0.58, base_srgb.blue, 0.40),
            )
        } else {
            (
                lerp(base_srgb.red, 1.0, 0.35),
                lerp(base_srgb.green, 0.55, 0.25) * (1.0 - 0.35 * u),
                (base_srgb.blue * 0.65 + 0.12).min(1.0),
            )
        };
        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(er, eg, eb, alpha);
        }

        let r_emb = (0.14 + radius * 0.095).clamp(0.10, 0.85) * (0.75 + 0.45 * (fi * 0.431).fract()) * (1.0 - 0.35 * u);
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            tf.translation = pos;
            tf.rotation = cam_rot;
            tf.scale = Vec3::new(r_emb, r_emb, 1.0);
            *vis = if alpha > 0.01 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_smoke_rig(
    sr: &SmokeRig,
    origin: Vec3,
    wind_vec: Vec3,
    wind_spd: f32,
    radius: f32,
    height: f32,
    intensity: f32,
    rise_spd: f32,
    spread: f32,
    base_srgb: Srgba,
    t: f32,
    cam_rot: Quat,
    flat_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ParticleFxEntity>>,
) {
    // Subtle soft ground haze at smoke source (feathered cloud texture, never a hard circle)
    if let Some(mut m) = materials.get_mut(&sr.base_haze.mat) {
        m.base_color = Color::srgba(
            base_srgb.red * 0.82,
            base_srgb.green * 0.82,
            base_srgb.blue * 0.82,
            (0.18 * intensity).clamp(0.06, 0.36),
        );
    }
    if let Ok((mut tf, mut vis)) = tf_q.get_mut(sr.base_haze.entity) {
        tf.translation = origin + Vec3::Y * 0.08;
        tf.rotation = flat_rot;
        tf.scale = Vec3::new(radius * 1.15, radius * 1.15, 1.0);
        *vis = Visibility::Visible;
    }

    // Continuous softly fading feathered cloud puffs matching natural wispy smoke
    let cycle_speed = 0.16 * rise_spd;
    for (i, sp) in sr.puffs.iter().enumerate() {
        let fi = i as f32;
        let speed_var = 0.86 + 0.28 * (fi * 1.13).sin().abs();
        let u = ((fi / SMOKE_PUFF_COUNT as f32) + t * cycle_speed * speed_var).rem_euclid(1.0);

        let ang = fi * 2.399_963 + t * 0.24;
        let radial = radius * (0.16 + u * (0.58 + spread * 1.15)) * (0.28 + 0.72 * (fi * 0.618_034).fract());
        let wind_drift = wind_vec * (wind_spd * u.powf(1.42) * (height * 0.16).max(1.4));
        let meander = Vec3::new(
            (t * 0.72 + fi * 1.7 + u * 2.6).sin() * radius * (0.16 + 0.24 * u),
            0.0,
            (t * 0.64 + fi * 2.3 + u * 2.2).cos() * radius * (0.16 + 0.24 * u),
        );
        let y = radius * 0.28 + height * u;
        let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + wind_drift + meander;

        // Silky continuous fade-in & fade-out per puff so zero popping ever occurs
        let fade_in = smoothstep(0.0, 0.16, u);
        let fade_out = (1.0 - smoothstep(0.42, 1.0, u)).powf(1.4);
        let alpha = (fade_in * fade_out * (0.28 * intensity)).clamp(0.0, 0.56);

        // Gently brighten as smoke mixes with ambient air higher up
        let bright = lerp(0.84, 1.14, u);
        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(
                (base_srgb.red * bright).min(1.0),
                (base_srgb.green * bright).min(1.0),
                (base_srgb.blue * bright).min(1.0),
                alpha,
            );
        }

        let r_puff = radius * (0.58 + u * (1.15 + spread * 1.25)) * (0.82 + 0.34 * (fi * 0.754).fract());
        let spin = (fi * 2.1 + t * 0.26 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);

        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::new(r_puff, r_puff * 0.94, 1.0);
            *vis = if alpha > 0.006 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_explosion_rig(
    er: &ExplosionRig,
    origin: Vec3,
    wind_vec: Vec3,
    wind_spd: f32,
    radius: f32,
    height: f32,
    intensity: f32,
    spread: f32,
    base_srgb: Srgba,
    shockwave_enabled: bool,
    with_embers: bool,
    with_smoke: bool,
    p: f32,
    cam_rot: Quat,
    flat_rot: Quat,
    materials: &mut Assets<StandardMaterial>,
    tf_q: &mut Query<(&mut Transform, &mut Visibility), With<ParticleFxEntity>>,
) {
    let active_blast = (0.0..=1.0).contains(&p);

    // Phase 1: Blinding additive core flash (p in 0.0..0.28)
    if let Ok((mut tf, mut vis)) = tf_q.get_mut(er.flash_core.entity) {
        if active_blast && p < 0.28 {
            let fp = p / 0.28;
            let flash_r = radius * (0.45 + 1.55 * fp.powf(0.35)) * intensity.sqrt();
            let alpha = (1.0 - fp).powf(1.5) * 0.95;
            if let Some(mut m) = materials.get_mut(&er.flash_core.mat) {
                m.base_color = Color::srgba(1.0, 0.96, 0.80, alpha);
            }
            tf.translation = origin + Vec3::Y * (radius * 0.28 + fp * radius * 0.40);
            tf.rotation = cam_rot;
            tf.scale = Vec3::new(flash_r, flash_r, 1.0);
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Phase 1b: Expanding glowing core fireballs (Add) + outer fiery clouds (Blend)
    let fb_visible = active_blast && p < 0.60;
    let fp = (p / 0.60).clamp(0.0, 1.0);
    let expand = fp.powf(0.42);

    for (i, sp) in er.core_balls.iter().enumerate() {
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            if !fb_visible {
                *vis = Visibility::Hidden;
                continue;
            }
            let fi = i as f32;
            let az = fi * 2.399_963;
            let elev = (0.12 + 0.78 * (fi * 0.618_034).fract()) * FRAC_PI_2;
            let dist = radius * (0.18 + 0.52 * (fi * 0.754).fract()) * expand;
            let lift = height * 0.26 * fp;
            let pos = origin
                + Vec3::new(
                    az.cos() * elev.cos() * dist,
                    (elev.sin() * dist * 0.85 + lift).max(radius * 0.16),
                    az.sin() * elev.cos() * dist,
                );
            let alpha = (1.0 - fp.powf(1.6)) * (0.68 * intensity.min(1.8)).clamp(0.25, 0.90);
            if let Some(mut m) = materials.get_mut(&sp.mat) {
                m.base_color = Color::srgba(
                    1.0,
                    lerp(0.94, base_srgb.green, fp),
                    lerp(0.68, base_srgb.blue * 0.5, fp),
                    alpha,
                );
            }
            let r_ball = radius * (0.48 + 0.35 * (fi * 0.431).fract()) * (0.38 + 0.92 * expand);
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(fi * 1.7 + fp * 1.5);
            tf.scale = Vec3::new(r_ball, r_ball, 1.0);
            *vis = if alpha > 0.01 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    for (i, sp) in er.outer_balls.iter().enumerate() {
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            if !fb_visible {
                *vis = Visibility::Hidden;
                continue;
            }
            let fi = i as f32;
            let az = fi * 2.399_963 + 0.55;
            let elev = (0.10 + 0.84 * (fi * 0.754).fract()) * FRAC_PI_2;
            let dist = radius * (0.34 + 0.76 * (fi * 0.618).fract()) * expand;
            let lift = height * 0.36 * fp;
            let pos = origin
                + Vec3::new(
                    az.cos() * elev.cos() * dist,
                    (elev.sin() * dist * 0.92 + lift).max(radius * 0.20),
                    az.sin() * elev.cos() * dist,
                );
            let cool = fp.powf(0.85);
            let alpha = (1.0 - smoothstep(0.25, 1.0, fp)).powf(1.35)
                * (0.64 + 0.16 * intensity.min(1.6)).min(0.88);
            if let Some(mut m) = materials.get_mut(&sp.mat) {
                m.base_color = Color::srgba(
                    lerp((base_srgb.red + 0.25).min(1.0), base_srgb.red * 0.72, cool),
                    lerp((base_srgb.green + 0.25).min(1.0), base_srgb.green * 0.22, cool),
                    lerp((base_srgb.blue + 0.10).min(1.0), base_srgb.blue * 0.18, cool),
                    alpha,
                );
            }
            let r_ball = radius * (0.55 + 0.36 * (fi * 0.517).fract()) * (0.35 + 0.95 * expand);
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(fi * 2.1 - fp * 1.4);
            tf.scale = Vec3::new(r_ball, r_ball, 1.0);
            *vis = if alpha > 0.01 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    // Phase 2: Feathered ground shockwave rings (p in 0.0..0.48)
    let shock_vis = active_blast && shockwave_enabled && p < 0.48;
    for (sp, delay, max_mult, peak_alpha) in [
        (&er.shockwave_outer, 0.0_f32, 2.55_f32, 0.85_f32),
        (&er.shockwave_inner, 0.05_f32, 1.80_f32, 0.58_f32),
    ] {
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            if shock_vis && p >= delay {
                let sp_t = ((p - delay) / (0.48 - delay)).clamp(0.0, 1.0);
                let r_ring = radius * (0.28 + max_mult * sp_t.powf(0.55));
                let alpha = (1.0 - sp_t).powf(1.5) * peak_alpha;
                if let Some(mut m) = materials.get_mut(&sp.mat) {
                    m.base_color = Color::srgba(
                        lerp(base_srgb.red, 1.0, 0.55),
                        lerp(base_srgb.green, 0.88, 0.55),
                        lerp(base_srgb.blue, 0.55, 0.45),
                        alpha,
                    );
                }
                tf.translation = origin + Vec3::Y * 0.14;
                tf.rotation = flat_rot;
                tf.scale = Vec3::new(r_ring, r_ring, 1.0);
                *vis = Visibility::Visible;
            } else {
                *vis = Visibility::Hidden;
            }
        }
    }

    // Phase 3: Ballistic glowing bokeh sparks & flying debris (p in 0.02..0.84)
    let debris_vis = active_blast && with_embers && (0.02..0.84).contains(&p);
    let dp = ((p - 0.02) / 0.82).clamp(0.0, 1.0);
    for (i, sp) in er.debris.iter().enumerate() {
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            if !debris_vis {
                *vis = Visibility::Hidden;
                continue;
            }
            let fi = i as f32;
            let az = fi * (TAU / EXPL_DEBRIS_COUNT as f32) + 0.35;
            let h_dist = radius * (1.1 + 1.55 * (fi * 0.618).fract()) * dp;
            let peak_h = height * (0.38 + 0.52 * (fi * 0.754).fract());
            let y = (4.0 * peak_h * dp * (1.0 - dp)).max(0.10);
            let pos = origin + Vec3::new(az.cos() * h_dist, y, az.sin() * h_dist);
            let alpha = (1.0 - dp).powf(1.25) * 0.95;
            if let Some(mut m) = materials.get_mut(&sp.mat) {
                m.base_color = Color::srgba(
                    1.0,
                    lerp(0.86, 0.32, dp),
                    lerp(0.45, 0.10, dp),
                    alpha,
                );
            }
            let r_d = (0.20 + radius * 0.065).clamp(0.16, 1.2) * (1.0 - 0.45 * dp);
            tf.translation = pos;
            tf.rotation = cam_rot;
            tf.scale = Vec3::new(r_d, r_d, 1.0);
            *vis = Visibility::Visible;
        }
    }

    // Phase 4: Soft billowy mushroom smoke cloud (p in 0.10..1.00)
    let smoke_vis = active_blast && with_smoke && p >= 0.10;
    let sp_t = ((p - 0.10) / 0.90).clamp(0.0, 1.0);
    for (i, sp) in er.mushroom_smoke.iter().enumerate() {
        if let Ok((mut tf, mut vis)) = tf_q.get_mut(sp.entity) {
            if !smoke_vis {
                *vis = Visibility::Hidden;
                continue;
            }
            let fi = i as f32;
            let is_cap = i >= EXPL_MUSHROOM_COUNT / 3;
            let az = fi * 2.399_963;
            let wind_drift = wind_vec * (wind_spd * sp_t * sp_t * 1.4);
            let (pos, r_puff) = if is_cap {
                let cap_frac = ((i - EXPL_MUSHROOM_COUNT / 3) as f32)
                    / ((EXPL_MUSHROOM_COUNT - EXPL_MUSHROOM_COUNT / 3) as f32);
                let cap_r = radius * (0.42 + 0.98 * sp_t * (0.65 + 0.55 * spread))
                    * (0.40 + 0.60 * cap_frac);
                let cap_y = height * (0.28 + 0.72 * sp_t.powf(0.75))
                    + (fi * 1.7).sin() * radius * 0.22;
                let pt = origin + Vec3::new(az.cos() * cap_r, cap_y, az.sin() * cap_r) + wind_drift;
                let rp = radius * (0.58 + 0.78 * sp_t) * (0.80 + 0.36 * (fi * 0.618).fract());
                (pt, rp)
            } else {
                let stem_frac = (i as f32 + 0.5) / (EXPL_MUSHROOM_COUNT / 3) as f32;
                let stem_r = radius * 0.24 * (1.0 + 0.45 * sp_t);
                let stem_y = height * 0.72 * sp_t.powf(0.8) * stem_frac + radius * 0.2;
                let pt = origin
                    + Vec3::new(az.cos() * stem_r, stem_y, az.sin() * stem_r)
                    + wind_drift * stem_frac;
                let rp = radius * (0.48 + 0.42 * sp_t);
                (pt, rp)
            };

            let fade_in = smoothstep(0.0, 0.18, sp_t);
            let fade_out = (1.0 - smoothstep(0.45, 1.0, sp_t)).powf(1.35);
            let alpha = (fade_in * fade_out * (0.34 + 0.10 * intensity.min(1.6))).clamp(0.0, 0.54);
            let shade = lerp(0.16, 0.28, sp_t);
            if let Some(mut m) = materials.get_mut(&sp.mat) {
                m.base_color = Color::srgba(shade, shade + 0.01, shade + 0.03, alpha);
            }

            let spin = (fi * 1.5 + sp_t * 0.8 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
            tf.translation = pos;
            tf.rotation = cam_rot * Quat::from_rotation_z(spin);
            tf.scale = Vec3::new(r_puff, r_puff * 0.92, 1.0);
            *vis = if alpha > 0.008 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_particle_textures_have_soft_center_and_zero_border_alpha() {
        let sz = 64u32;
        for (name, img) in [
            ("fire", build_fire_glow_texture(sz)),
            ("smoke", build_smoke_cloud_texture(sz)),
            ("ember", build_ember_bokeh_texture(sz)),
            ("ring", build_shockwave_ring_texture(sz)),
        ] {
            let bytes = img.data.as_ref().expect("image bytes present");
            assert_eq!(bytes.len(), (sz * sz * 4) as usize, "{name} size mismatch");

            // Corner pixels MUST be strictly alpha == 0 (no harsh square quad border)
            for (cx, cy) in [(0, 0), (sz - 1, 0), (0, sz - 1), (sz - 1, sz - 1)] {
                let a = bytes[((cy * sz + cx) as usize) * 4 + 3];
                assert_eq!(a, 0, "{name} corner ({cx},{cy}) must have zero alpha");
            }

            // Texture must have significant visible alpha somewhere inside
            let max_alpha = bytes.iter().skip(3).step_by(4).copied().max().unwrap_or(0);
            assert!(max_alpha > 180, "{name} max alpha too low: {max_alpha}");
        }
    }
}
