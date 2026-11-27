use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use twg_shared::particle_anim::{
    ParticleAnimConfig, ParticleAnimKind, is_particle_anim, parse_particle_anim_config,
    particle_anim_position,
};

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

struct BatchLayer {
    entity: Entity,
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
}

struct ParticleBatchHandles {
    fire_blend: BatchLayer,
    fire_add: BatchLayer,
    smoke_blend: BatchLayer,
    ember_add: BatchLayer,
    ring_add: BatchLayer,
    ring_blend: BatchLayer,
}

impl ParticleBatchHandles {
    fn despawn(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) {
        for layer in [
            self.fire_blend,
            self.fire_add,
            self.smoke_blend,
            self.ember_add,
            self.ring_add,
            self.ring_blend,
        ] {
            commands.entity(layer.entity).despawn();
            meshes.remove(layer.mesh.id());
            materials.remove(layer.mat.id());
        }
    }
}

#[derive(Default)]
struct QuadBatchBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl QuadBatchBuilder {
    fn clear(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.uvs.clear();
        self.colors.clear();
        self.indices.clear();
    }

    #[inline]
    fn push_quad(
        &mut self,
        center: Vec3,
        rot: Quat,
        half_x: f32,
        half_y: f32,
        color: Color,
    ) {
        let lin = color.to_linear();
        if lin.alpha <= 0.004 || half_x <= 0.001 || half_y <= 0.001 {
            return;
        }
        let c = [lin.red, lin.green, lin.blue, lin.alpha];
        let right = rot * Vec3::X * half_x;
        let up = rot * Vec3::Y * half_y;
        let nrm = (rot * Vec3::Z).to_array();

        let p0 = (center - right - up).to_array();
        let p1 = (center + right - up).to_array();
        let p2 = (center + right + up).to_array();
        let p3 = (center - right + up).to_array();

        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&[p0, p1, p2, p3]);
        self.normals.extend_from_slice(&[nrm, nrm, nrm, nrm]);
        self.uvs
            .extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        self.colors.extend_from_slice(&[c, c, c, c]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn write_into(&self, mesh: &mut Mesh) {
        let mut built = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        built.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions.clone());
        built.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals.clone());
        built.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs.clone());
        built.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors.clone());
        built.insert_indices(Indices::U32(self.indices.clone()));
        *mesh = built;
    }
}

/// GPU-Batched Particle Pool: renders ALL active Fire, Smoke, and Explosion effects in the entire
/// scenario using ONLY 6 persistent entities & 6 shared `StandardMaterial`s (6 draw calls total,
/// even with 100-1,000 simultaneous particle effects!).
#[derive(Resource, Default)]
pub struct ParticleFxPool {
    batches: Option<ParticleBatchHandles>,
    last_reset_nonce: u64,
    preview_epoch_sec: f64,
    b_fire_blend: QuadBatchBuilder,
    b_fire_add: QuadBatchBuilder,
    b_smoke_blend: QuadBatchBuilder,
    b_ember_add: QuadBatchBuilder,
    b_ring_add: QuadBatchBuilder,
    b_ring_blend: QuadBatchBuilder,
    had_geometry: bool,
}

fn spawn_batch_layer(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: Handle<Image>,
    alpha_mode: AlphaMode,
) -> BatchLayer {
    let mut empty = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    empty.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    empty.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    empty.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    empty.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    empty.insert_indices(Indices::U32(Vec::new()));

    let mesh = meshes.add(empty);
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(tex),
        alpha_mode,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let entity = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::IDENTITY,
            Visibility::Hidden,
            ParticleFxEntity,
        ))
        .id();
    BatchLayer { entity, mesh, mat }
}

impl ParticleFxPool {
    fn ensure_batches(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) {
        if self.batches.is_some() {
            return;
        }
        let fire_tex = images.add(build_fire_glow_texture(TEX_SIZE));
        let smoke_tex = images.add(build_smoke_cloud_texture(TEX_SIZE));
        let ember_tex = images.add(build_ember_bokeh_texture(TEX_SIZE));
        let ring_tex = images.add(build_shockwave_ring_texture(TEX_SIZE));

        self.batches = Some(ParticleBatchHandles {
            fire_blend: spawn_batch_layer(
                commands,
                meshes,
                materials,
                fire_tex.clone(),
                AlphaMode::Blend,
            ),
            fire_add: spawn_batch_layer(commands, meshes, materials, fire_tex, AlphaMode::Add),
            smoke_blend: spawn_batch_layer(
                commands,
                meshes,
                materials,
                smoke_tex,
                AlphaMode::Blend,
            ),
            ember_add: spawn_batch_layer(commands, meshes, materials, ember_tex, AlphaMode::Add),
            ring_add: spawn_batch_layer(
                commands,
                meshes,
                materials,
                ring_tex.clone(),
                AlphaMode::Add,
            ),
            ring_blend: spawn_batch_layer(commands, meshes, materials, ring_tex, AlphaMode::Blend),
        });
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
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

fn build_smoke_cloud_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.46, 0.96, r);
            let n = fbm_2d(u * 2.8 + 11.2, v * 2.8 + 5.4);
            let billow = (0.64 + 0.36 * n) * (-r * r * 1.95).exp();
            let alpha = (billow * edge).clamp(0.0, 1.0);
            let shade = (212.0 + 43.0 * n).clamp(0.0, 255.0) as u8;
            let idx = ((py * size + px) as usize) * 4;
            data[idx] = shade;
            data[idx + 1] = shade;
            data[idx + 2] = shade;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

fn build_ember_bokeh_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let edge = 1.0 - smoothstep(0.60, 0.95, r);
            let alpha = ((-r * r * 5.8).exp() * edge).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

fn build_shockwave_ring_texture(size: u32) -> Image {
    let mut data = vec![255u8; (size * size * 4) as usize];
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let r = u.hypot(v);
            let ring = (-((r - 0.72) / 0.12).powi(2)).exp();
            let edge = 1.0 - smoothstep(0.84, 0.98, r);
            let inner = smoothstep(0.32, 0.56, r);
            let alpha = (ring * edge * inner).clamp(0.0, 1.0);
            let idx = ((py * size + px) as usize) * 4;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }
    make_rgba_texture(size, data)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

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
    mut vis_q: Query<&mut Visibility, With<ParticleFxEntity>>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        if let Some(batches) = pool.batches.take() {
            batches.despawn(&mut commands, &mut meshes, &mut materials);
        }
        pool.had_geometry = false;
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

    let mut to_render: Vec<(ParticleAnimConfig, [f64; 3], f64, f64, bool)> = Vec::new();
    let playback_active = playback.is_playback_active();

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

        if playback_active {
            let status = playback.track_status(anim.id);
            if matches!(status, AnimRunStatus::Playing | AnimRunStatus::Paused) {
                let elapsed = playback
                    .tracks
                    .get(&anim.id)
                    .map(|t| t.distance_m)
                    .unwrap_or(0.0);
                to_render.push((cfg, pos, elapsed, dur_sec, false));
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
                to_render.push((cfg, pos, elapsed, dur_sec, is_sel));
            }
        }
    }

    if to_render.is_empty() {
        if pool.had_geometry
            && let Some(batches) = &pool.batches
        {
            for e in [
                batches.fire_blend.entity,
                batches.fire_add.entity,
                batches.smoke_blend.entity,
                batches.ember_add.entity,
                batches.ring_add.entity,
                batches.ring_blend.entity,
            ] {
                if let Ok(mut v) = vis_q.get_mut(e)
                    && *v != Visibility::Hidden
                {
                    *v = Visibility::Hidden;
                }
            }
            pool.had_geometry = false;
        }
        return;
    }

    let pool = pool.as_mut();
    pool.ensure_batches(&mut commands, &mut meshes, &mut images, &mut materials);
    pool.b_fire_blend.clear();
    pool.b_fire_add.clear();
    pool.b_smoke_blend.clear();
    pool.b_ember_add.clear();
    pool.b_ring_add.clear();
    pool.b_ring_blend.clear();

    let (cam_pos, cam_rot, top_down_factor) = cam_q
        .single()
        .map(|gtf| {
            let (_, rot, pos) = gtf.to_scale_rotation_translation();
            let fwd_y = gtf.forward().as_vec3().y.abs();
            (pos, rot, fwd_y)
        })
        .unwrap_or((Vec3::ZERO, Quat::IDENTITY, 0.35));

    let flat_rot = Quat::from_rotation_x(-FRAC_PI_2);
    let many_particles = to_render.len() > 24;

    for (cfg, geo_pos, elapsed, dur_sec, is_selected_in_editor) in to_render {
        let origin = Vec3::from_array(
            state
                .geo_anchor
                .lng_lat_alt_to_local(geo_pos[0], geo_pos[1], geo_pos[2]),
        );

        let cam_dist = origin.distance(cam_pos);
        if many_particles && cam_dist > 1100.0 {
            continue;
        }
        // Smooth distance-based sprite step LOD when dozens/hundreds of particles are active
        let lod_step: usize = if !many_particles {
            1
        } else if cam_dist > 450.0 {
            3
        } else if cam_dist > 220.0 {
            2
        } else {
            1
        };

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

        match cfg.kind {
            ParticleAnimKind::Fire => {
                emit_fire_quads(
                    &mut pool.b_fire_blend,
                    &mut pool.b_fire_add,
                    &mut pool.b_smoke_blend,
                    &mut pool.b_ember_add,
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
                    lod_step,
                );
            }
            ParticleAnimKind::Smoke => {
                emit_smoke_quads(
                    &mut pool.b_smoke_blend,
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
                    lod_step,
                );
            }
            ParticleAnimKind::Explosion => {
                emit_explosion_quads(
                    &mut pool.b_fire_blend,
                    &mut pool.b_fire_add,
                    &mut pool.b_smoke_blend,
                    &mut pool.b_ember_add,
                    &mut pool.b_ring_add,
                    &mut pool.b_ring_blend,
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
                    lod_step,
                );
            }
        }
    }

    if let Some(batches) = &pool.batches {
        let upload = |layer: &BatchLayer,
                      builder: &QuadBatchBuilder,
                      meshes: &mut Assets<Mesh>,
                      vis_q: &mut Query<&mut Visibility, With<ParticleFxEntity>>| {
            let visible = !builder.indices.is_empty();
            if visible
                && let Some(mut mesh) = meshes.get_mut(&layer.mesh)
            {
                builder.write_into(&mut mesh);
            }
            if let Ok(mut v) = vis_q.get_mut(layer.entity) {
                let target = if visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                if *v != target {
                    *v = target;
                }
            }
        };

        upload(&batches.fire_blend, &pool.b_fire_blend, &mut meshes, &mut vis_q);
        upload(&batches.fire_add, &pool.b_fire_add, &mut meshes, &mut vis_q);
        upload(&batches.smoke_blend, &pool.b_smoke_blend, &mut meshes, &mut vis_q);
        upload(&batches.ember_add, &pool.b_ember_add, &mut meshes, &mut vis_q);
        upload(&batches.ring_add, &pool.b_ring_add, &mut meshes, &mut vis_q);
        upload(&batches.ring_blend, &pool.b_ring_blend, &mut meshes, &mut vis_q);
        pool.had_geometry = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_fire_quads(
    fire_blend: &mut QuadBatchBuilder,
    fire_add: &mut QuadBatchBuilder,
    smoke_blend: &mut QuadBatchBuilder,
    ember_add: &mut QuadBatchBuilder,
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
    lod_step: usize,
) {
    let flicker = 1.0 + 0.08 * (t * 11.3).sin() + 0.05 * (t * 19.7).cos();
    let vert_stretch = 1.0 + 0.38 * (1.0 - top_down_factor * 0.65);
    let hot_r = lerp(base_srgb.red, 1.0, 0.85);
    let hot_g = lerp(base_srgb.green, 0.96, 0.78);
    let hot_b = lerp(base_srgb.blue, 0.72, 0.62);
    let size_comp = (lod_step as f32).sqrt();

    // 1. Soft glowing ground aura
    fire_blend.push_quad(
        origin + Vec3::Y * 0.08,
        flat_rot,
        radius * 1.15 * flicker,
        radius * 1.15 * flicker,
        Color::srgba(
            lerp(base_srgb.red, 1.0, 0.35),
            lerp(base_srgb.green, 0.68, 0.25),
            base_srgb.blue * 0.45,
            (0.32 + 0.14 * intensity).clamp(0.14, 0.68),
        ),
    );

    // 2. Outer rising flame wisps
    let flame_speed = 0.76 * rise_spd;
    for i in (0..FIRE_OUTER_COUNT).step_by(lod_step) {
        let fi = i as f32;
        let speed_var = 0.82 + 0.36 * (fi * 1.73).sin().abs();
        let u = ((fi / FIRE_OUTER_COUNT as f32) + t * flame_speed * speed_var).rem_euclid(1.0);
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

        let size_env = 0.50 + 0.58 * (u * PI).sin().powf(0.55);
        let sx = radius
            * (0.42 + 0.18 * spread)
            * size_env
            * (0.85 + 0.25 * intensity.min(2.0))
            * size_comp;
        let sy = sx * lerp(vert_stretch, 1.0, u);
        let spin = (fi * 1.9 + t * 0.95 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
        fire_blend.push_quad(
            pos,
            cam_rot * Quat::from_rotation_z(spin),
            sx,
            sy,
            Color::srgba(cr, cg, cb, alpha),
        );
    }

    // 3. Hot white-gold inner core (additive)
    for i in (0..FIRE_CORE_COUNT).step_by(lod_step) {
        let fi = i as f32;
        let u = ((fi / FIRE_CORE_COUNT as f32) + t * (1.12 * rise_spd)).rem_euclid(1.0);
        let radial = radius * (0.06 + 0.22 * (u * PI).sin()) * (0.2 + 0.8 * (fi * 0.618).fract());
        let ang = fi * 2.399_963 + t * 2.1;
        let y = height * 0.52 * u.powf(0.88) + 0.12;
        let pos = origin
            + Vec3::new(ang.cos() * radial, y, ang.sin() * radial)
            + wind_vec * (wind_spd * u * u * 0.35);
        let alpha =
            smoothstep(0.0, 0.15, u) * (1.0 - smoothstep(0.28, 1.0, u)) * (0.58 * intensity.min(1.8));
        let r_core = radius * (0.34 - 0.12 * u) * flicker * size_comp;
        fire_add.push_quad(
            pos,
            cam_rot,
            r_core,
            r_core * vert_stretch * 1.08,
            Color::srgba(hot_r, hot_g, hot_b, alpha.clamp(0.0, 0.92)),
        );
    }

    // 4. Rising fire smoke
    if with_smoke {
        for i in (0..FIRE_SMOKE_COUNT).step_by(lod_step) {
            let fi = i as f32;
            let u = ((fi / FIRE_SMOKE_COUNT as f32) + t * (0.30 * rise_spd)).rem_euclid(1.0);
            let ang = fi * 2.399_963 + u * 1.2;
            let radial = radius * (0.28 + 0.85 * u * (0.5 + 0.6 * spread));
            let y = height * (0.48 + 1.35 * u);
            let pos = origin
                + Vec3::new(ang.cos() * radial, y, ang.sin() * radial)
                + wind_vec * (wind_spd * (0.5 + u * 1.9));
            let alpha = smoothstep(0.0, 0.22, u)
                * (1.0 - smoothstep(0.42, 1.0, u))
                * (0.26 + 0.10 * intensity.min(1.6));
            let shade = lerp(0.14, 0.26, u);
            let rp = radius * (0.52 + 0.88 * u) * size_comp;
            let spin = (fi * 1.4 + t * 0.45 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
            smoke_blend.push_quad(
                pos,
                cam_rot * Quat::from_rotation_z(spin),
                rp,
                rp * 0.92,
                Color::srgba(shade, shade + 0.01, shade + 0.02, alpha.clamp(0.0, 0.48)),
            );
        }
    }

    // 5. Floating embers
    if with_embers {
        for i in (0..FIRE_EMBER_COUNT).step_by(lod_step) {
            let fi = i as f32;
            let u = ((fi / FIRE_EMBER_COUNT as f32) + t * (0.62 * rise_spd)).rem_euclid(1.0);
            let ang = fi * 2.399_963 + t * 1.4 + u * 2.5;
            let radial = radius * (0.22 + 0.95 * u) * (0.35 + 0.65 * (fi * 0.754).fract());
            let y = height * (0.18 + 1.25 * u);
            let pos = origin
                + Vec3::new(ang.cos() * radial, y, ang.sin() * radial)
                + wind_vec * (wind_spd * u * 1.4);
            let twinkle = 0.55 + 0.45 * (t * 14.0 + fi * 3.1).sin().abs();
            let alpha = smoothstep(0.0, 0.12, u) * (1.0 - smoothstep(0.55, 1.0, u)) * twinkle;
            let re = (0.12 + radius * 0.045).clamp(0.10, 0.55) * (1.0 - 0.35 * u);
            ember_add.push_quad(
                pos,
                cam_rot,
                re,
                re,
                Color::srgba(1.0, lerp(0.85, 0.42, u), 0.18, alpha.clamp(0.0, 0.95)),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_smoke_quads(
    smoke_blend: &mut QuadBatchBuilder,
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
    lod_step: usize,
) {
    let size_comp = (lod_step as f32).sqrt();
    smoke_blend.push_quad(
        origin + Vec3::Y * 0.08,
        flat_rot,
        radius * 1.15,
        radius * 1.15,
        Color::srgba(
            base_srgb.red * 0.85,
            base_srgb.green * 0.85,
            base_srgb.blue * 0.88,
            (0.22 * intensity.min(1.8)).clamp(0.08, 0.40),
        ),
    );

    let smoke_speed = 0.28 * rise_spd;
    for i in (0..SMOKE_PUFF_COUNT).step_by(lod_step) {
        let fi = i as f32;
        let spd_var = 0.82 + 0.35 * (fi * 1.37).sin().abs();
        let u = ((fi / SMOKE_PUFF_COUNT as f32) + t * smoke_speed * spd_var).rem_euclid(1.0);
        let ang = fi * 2.399_963 + u * 1.1;
        let radial = radius
            * (0.18 + u.powf(0.85) * (0.75 + spread * 1.15))
            * (0.30 + 0.70 * (fi * 0.618).fract());
        let swirl = Vec3::new(
            (t * 1.4 + fi * 1.9).sin() * radius * 0.18 * u,
            0.0,
            (t * 1.2 + fi * 2.1).cos() * radius * 0.18 * u,
        );
        let wind_drift = wind_vec * (wind_spd * u.powf(1.25) * 1.8);
        let y = (radius * 0.25) + height * u.powf(0.92);
        let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + swirl + wind_drift;

        let fade_in = smoothstep(0.0, 0.16, u);
        let fade_out = (1.0 - smoothstep(0.40, 1.0, u)).powf(1.35);
        let alpha = (fade_in * fade_out * (0.34 + 0.14 * intensity.min(2.0))).clamp(0.0, 0.56);
        let rp = radius * (0.48 + 0.95 * u) * (0.82 + 0.36 * (fi * 0.618).fract()) * size_comp;
        let spin = (fi * 1.7 + t * 0.38 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
        smoke_blend.push_quad(
            pos,
            cam_rot * Quat::from_rotation_z(spin),
            rp,
            rp * 0.92,
            Color::srgba(
                lerp(base_srgb.red * 0.72, base_srgb.red, u),
                lerp(base_srgb.green * 0.74, base_srgb.green, u),
                lerp(base_srgb.blue * 0.78, base_srgb.blue, u),
                alpha,
            ),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_explosion_quads(
    fire_blend: &mut QuadBatchBuilder,
    fire_add: &mut QuadBatchBuilder,
    smoke_blend: &mut QuadBatchBuilder,
    ember_add: &mut QuadBatchBuilder,
    ring_add: &mut QuadBatchBuilder,
    ring_blend: &mut QuadBatchBuilder,
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
    p_raw: f32,
    cam_rot: Quat,
    flat_rot: Quat,
    lod_step: usize,
) {
    let p = p_raw.clamp(0.0, 1.2);
    if p >= 1.0 {
        return;
    }
    let size_comp = (lod_step as f32).sqrt();

    // Flash core (0.0..0.24)
    if p < 0.24 {
        let fp = (p / 0.24).clamp(0.0, 1.0);
        let r_f = radius * (0.45 + 1.45 * fp.powf(0.45));
        let alpha = (1.0 - fp).powf(1.5) * 0.96;
        fire_add.push_quad(
            origin + Vec3::Y * (radius * 0.45),
            cam_rot,
            r_f,
            r_f,
            Color::srgba(1.0, 0.96, 0.82, alpha),
        );
    }

    // Core & outer fireball (0.0..0.52)
    if p < 0.52 {
        let bp = (p / 0.52).clamp(0.0, 1.0);
        for i in (0..EXPL_CORE_BALLS).step_by(lod_step) {
            let fi = i as f32;
            let az = fi * 2.399_963;
            let rad = radius * (0.22 + 0.65 * bp) * (0.35 + 0.65 * (fi * 0.618).fract());
            let y = height * 0.34 * bp + radius * 0.25;
            let pos = origin + Vec3::new(az.cos() * rad, y, az.sin() * rad);
            let alpha = (1.0 - bp).powf(1.3) * 0.78;
            let r_b = radius * (0.42 + 0.48 * bp) * size_comp;
            fire_add.push_quad(
                pos,
                cam_rot,
                r_b,
                r_b,
                Color::srgba(1.0, lerp(0.92, 0.42, bp), lerp(0.60, 0.08, bp), alpha),
            );
        }
        for i in (0..EXPL_OUTER_BALLS).step_by(lod_step) {
            let fi = i as f32;
            let az = fi * 2.399_963 + 0.7;
            let rad = radius * (0.30 + 0.95 * bp * (0.7 + 0.4 * spread));
            let y = height * 0.42 * bp + radius * 0.18;
            let pos = origin + Vec3::new(az.cos() * rad, y, az.sin() * rad);
            let alpha = (1.0 - bp).powf(1.2) * 0.72;
            let r_b = radius * (0.48 + 0.56 * bp) * size_comp;
            fire_blend.push_quad(
                pos,
                cam_rot,
                r_b,
                r_b,
                Color::srgba(
                    base_srgb.red,
                    lerp(base_srgb.green, 0.16, bp),
                    base_srgb.blue * 0.25,
                    alpha,
                ),
            );
        }
    }

    // Ground shockwave rings
    if shockwave_enabled && p < 0.46 {
        let sp_t = (p / 0.46).clamp(0.0, 1.0);
        let r_outer = radius * (0.28 + 3.1 * sp_t.powf(0.55));
        let a_outer = (1.0 - sp_t).powf(1.5) * 0.78;
        ring_add.push_quad(
            origin + Vec3::Y * 0.14,
            flat_rot,
            r_outer,
            r_outer,
            Color::srgba(
                lerp(base_srgb.red, 1.0, 0.55),
                lerp(base_srgb.green, 0.88, 0.55),
                lerp(base_srgb.blue, 0.55, 0.45),
                a_outer,
            ),
        );
        let r_inner = radius * (0.28 + 2.1 * sp_t.powf(0.55));
        let a_inner = (1.0 - sp_t).powf(1.5) * 0.46;
        ring_blend.push_quad(
            origin + Vec3::Y * 0.13,
            flat_rot,
            r_inner,
            r_inner,
            Color::srgba(0.95, 0.82, 0.60, a_inner),
        );
    }

    // Ballistic sparks & debris (0.02..0.84)
    if with_embers && (0.02..0.84).contains(&p) {
        let dp = ((p - 0.02) / 0.82).clamp(0.0, 1.0);
        for i in (0..EXPL_DEBRIS_COUNT).step_by(lod_step) {
            let fi = i as f32;
            let az = fi * (TAU / EXPL_DEBRIS_COUNT as f32) + 0.35;
            let h_dist = radius * (1.1 + 1.55 * (fi * 0.618).fract()) * dp;
            let peak_h = height * (0.38 + 0.52 * (fi * 0.754).fract());
            let y = (4.0 * peak_h * dp * (1.0 - dp)).max(0.10);
            let pos = origin + Vec3::new(az.cos() * h_dist, y, az.sin() * h_dist);
            let alpha = (1.0 - dp).powf(1.25) * 0.95;
            let r_d = (0.20 + radius * 0.065).clamp(0.16, 1.2) * (1.0 - 0.45 * dp);
            ember_add.push_quad(
                pos,
                cam_rot,
                r_d,
                r_d,
                Color::srgba(1.0, lerp(0.86, 0.32, dp), lerp(0.45, 0.10, dp), alpha),
            );
        }
    }

    // Mushroom smoke cloud (0.10..1.00)
    if with_smoke && p >= 0.10 {
        let sp_t = ((p - 0.10) / 0.90).clamp(0.0, 1.0);
        for i in (0..EXPL_MUSHROOM_COUNT).step_by(lod_step) {
            let fi = i as f32;
            let is_cap = i >= EXPL_MUSHROOM_COUNT / 3;
            let az = fi * 2.399_963;
            let wind_drift = wind_vec * (wind_spd * sp_t * sp_t * 1.4);
            let (pos, r_puff) = if is_cap {
                let cap_frac = ((i - EXPL_MUSHROOM_COUNT / 3) as f32)
                    / ((EXPL_MUSHROOM_COUNT - EXPL_MUSHROOM_COUNT / 3) as f32);
                let cap_r = radius
                    * (0.42 + 0.98 * sp_t * (0.65 + 0.55 * spread))
                    * (0.40 + 0.60 * cap_frac);
                let cap_y = height * (0.28 + 0.72 * sp_t.powf(0.75))
                    + (fi * 1.7).sin() * radius * 0.22;
                let pt =
                    origin + Vec3::new(az.cos() * cap_r, cap_y, az.sin() * cap_r) + wind_drift;
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
            let spin =
                (fi * 1.5 + sp_t * 0.8 * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(TAU);
            smoke_blend.push_quad(
                pos,
                cam_rot * Quat::from_rotation_z(spin),
                r_puff * size_comp,
                r_puff * 0.92 * size_comp,
                Color::srgba(shade, shade + 0.01, shade + 0.03, alpha),
            );
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

            for (cx, cy) in [(0, 0), (sz - 1, 0), (0, sz - 1), (sz - 1, sz - 1)] {
                let a = bytes[((cy * sz + cx) as usize) * 4 + 3];
                assert_eq!(a, 0, "{name} corner ({cx},{cy}) must have zero alpha");
            }

            let max_alpha = bytes.iter().skip(3).step_by(4).copied().max().unwrap_or(0);
            assert!(max_alpha > 180, "{name} max alpha too low: {max_alpha}");
        }
    }
}
