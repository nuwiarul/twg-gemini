use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use twg_shared::particle_anim::{
    ParticleAnimConfig, ParticleAnimKind, is_particle_anim, parse_particle_anim_config,
    particle_anim_position,
};
use uuid::Uuid;

use crate::{
    scene::parse_hex_color,
    state::{AnimRunStatus, AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode},
};

const FIRE_CORE_COUNT: usize = 10;
const FIRE_OUTER_COUNT: usize = 14;
const FIRE_SMOKE_COUNT: usize = 14;
const FIRE_EMBER_COUNT: usize = 16;

const SMOKE_PUFF_COUNT: usize = 32;

const EXPL_FIREBALL_COUNT: usize = 18;
const EXPL_DEBRIS_COUNT: usize = 16;
const EXPL_MUSHROOM_COUNT: usize = 20;

#[derive(Component)]
pub struct ParticleFxEntity;

struct FireRig {
    base_disc: Entity,
    core_flames: Vec<Entity>,
    outer_flames: Vec<Entity>,
    smoke_puffs: Vec<Entity>,
    embers: Vec<Entity>,
    base_mat: Handle<StandardMaterial>,
    core_mat: Handle<StandardMaterial>,
    outer_mat: Handle<StandardMaterial>,
    smoke_mat: Handle<StandardMaterial>,
    ember_mat: Handle<StandardMaterial>,
}

struct SmokeRig {
    base_disc: Entity,
    puffs: Vec<Entity>,
    base_mat: Handle<StandardMaterial>,
    tier_mats: [Handle<StandardMaterial>; 4],
}

struct ExplosionRig {
    flash_core: Entity,
    fireballs: Vec<Entity>,
    shockwave_outer: Entity,
    shockwave_inner: Entity,
    debris: Vec<Entity>,
    mushroom_smoke: Vec<Entity>,
    flash_mat: Handle<StandardMaterial>,
    fireball_mat: Handle<StandardMaterial>,
    shock_outer_mat: Handle<StandardMaterial>,
    shock_inner_mat: Handle<StandardMaterial>,
    debris_mat: Handle<StandardMaterial>,
    smoke_mat: Handle<StandardMaterial>,
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

    fn despawn(self, commands: &mut Commands) {
        match self {
            Self::Fire(r) => {
                commands.entity(r.base_disc).despawn();
                for e in r.core_flames {
                    commands.entity(e).despawn();
                }
                for e in r.outer_flames {
                    commands.entity(e).despawn();
                }
                for e in r.smoke_puffs {
                    commands.entity(e).despawn();
                }
                for e in r.embers {
                    commands.entity(e).despawn();
                }
            }
            Self::Smoke(r) => {
                commands.entity(r.base_disc).despawn();
                for e in r.puffs {
                    commands.entity(e).despawn();
                }
            }
            Self::Explosion(r) => {
                commands.entity(r.flash_core).despawn();
                commands.entity(r.shockwave_outer).despawn();
                commands.entity(r.shockwave_inner).despawn();
                for e in r.fireballs {
                    commands.entity(e).despawn();
                }
                for e in r.debris {
                    commands.entity(e).despawn();
                }
                for e in r.mushroom_smoke {
                    commands.entity(e).despawn();
                }
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct ParticleFxPool {
    sphere_mesh: Option<Handle<Mesh>>,
    cone_mesh: Option<Handle<Mesh>>,
    disc_mesh: Option<Handle<Mesh>>,
    ring_mesh: Option<Handle<Mesh>>,
    rigs: HashMap<Uuid, ParticleRig>,
    last_reset_nonce: u64,
    preview_epoch_sec: f64,
}

impl ParticleFxPool {
    fn ensure_meshes(
        &mut self,
        meshes: &mut Assets<Mesh>,
    ) -> (Handle<Mesh>, Handle<Mesh>, Handle<Mesh>, Handle<Mesh>) {
        let sphere = self
            .sphere_mesh
            .get_or_insert_with(|| meshes.add(Sphere::new(1.0).mesh().uv(10, 6)))
            .clone();
        let cone = self
            .cone_mesh
            .get_or_insert_with(|| meshes.add(Cone::new(0.62, 1.8)))
            .clone();
        let disc = self
            .disc_mesh
            .get_or_insert_with(|| meshes.add(Circle::new(1.0)))
            .clone();
        let ring = self
            .ring_mesh
            .get_or_insert_with(|| meshes.add(Annulus::new(0.74, 1.0)))
            .clone();
        (sphere, cone, disc, ring)
    }
}

fn spawn_unlit_blend_mat(
    materials: &mut Assets<StandardMaterial>,
    color: Color,
    double_sided: bool,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided,
        cull_mode: if double_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
        ..default()
    })
}

fn spawn_fx_entity(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
) -> Entity {
    commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            Visibility::Visible,
            ParticleFxEntity,
        ))
        .id()
}

fn spawn_fire_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    sphere: &Handle<Mesh>,
    cone: &Handle<Mesh>,
    disc: &Handle<Mesh>,
) -> ParticleRig {
    let base_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.35, 0.02, 0.45), true);
    let core_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.94, 0.58, 0.90), true);
    let outer_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.42, 0.04, 0.78), true);
    let smoke_mat = spawn_unlit_blend_mat(materials, Color::srgba(0.14, 0.16, 0.20, 0.48), false);
    let ember_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.86, 0.26, 0.96), false);

    let base_disc = spawn_fx_entity(commands, disc.clone(), base_mat.clone());
    let core_flames = (0..FIRE_CORE_COUNT)
        .map(|_| spawn_fx_entity(commands, cone.clone(), core_mat.clone()))
        .collect();
    let outer_flames = (0..FIRE_OUTER_COUNT)
        .map(|_| spawn_fx_entity(commands, cone.clone(), outer_mat.clone()))
        .collect();
    let smoke_puffs = (0..FIRE_SMOKE_COUNT)
        .map(|_| spawn_fx_entity(commands, sphere.clone(), smoke_mat.clone()))
        .collect();
    let embers = (0..FIRE_EMBER_COUNT)
        .map(|_| spawn_fx_entity(commands, sphere.clone(), ember_mat.clone()))
        .collect();

    ParticleRig::Fire(FireRig {
        base_disc,
        core_flames,
        outer_flames,
        smoke_puffs,
        embers,
        base_mat,
        core_mat,
        outer_mat,
        smoke_mat,
        ember_mat,
    })
}

fn spawn_smoke_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    sphere: &Handle<Mesh>,
    disc: &Handle<Mesh>,
) -> ParticleRig {
    let base_mat = spawn_unlit_blend_mat(materials, Color::srgba(0.20, 0.24, 0.30, 0.36), true);
    let tier_mats = [
        spawn_unlit_blend_mat(materials, Color::srgba(0.24, 0.28, 0.34, 0.72), false),
        spawn_unlit_blend_mat(materials, Color::srgba(0.28, 0.33, 0.40, 0.62), false),
        spawn_unlit_blend_mat(materials, Color::srgba(0.34, 0.39, 0.46, 0.46), false),
        spawn_unlit_blend_mat(materials, Color::srgba(0.42, 0.47, 0.54, 0.26), false),
    ];
    let base_disc = spawn_fx_entity(commands, disc.clone(), base_mat.clone());
    let puffs = (0..SMOKE_PUFF_COUNT)
        .map(|i| {
            let tier = (i * 4 / SMOKE_PUFF_COUNT).min(3);
            spawn_fx_entity(commands, sphere.clone(), tier_mats[tier].clone())
        })
        .collect();

    ParticleRig::Smoke(SmokeRig {
        base_disc,
        puffs,
        base_mat,
        tier_mats,
    })
}

fn spawn_explosion_rig(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    sphere: &Handle<Mesh>,
    ring: &Handle<Mesh>,
) -> ParticleRig {
    let flash_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.98, 0.85, 0.95), false);
    let fireball_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.48, 0.04, 0.90), false);
    let shock_outer_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.86, 0.56, 0.80), true);
    let shock_inner_mat = spawn_unlit_blend_mat(materials, Color::srgba(0.95, 0.64, 0.32, 0.50), true);
    let debris_mat = spawn_unlit_blend_mat(materials, Color::srgba(1.0, 0.78, 0.22, 0.95), false);
    let smoke_mat = spawn_unlit_blend_mat(materials, Color::srgba(0.18, 0.20, 0.24, 0.68), false);

    let flash_core = spawn_fx_entity(commands, sphere.clone(), flash_mat.clone());
    let shockwave_outer = spawn_fx_entity(commands, ring.clone(), shock_outer_mat.clone());
    let shockwave_inner = spawn_fx_entity(commands, ring.clone(), shock_inner_mat.clone());
    let fireballs = (0..EXPL_FIREBALL_COUNT)
        .map(|_| spawn_fx_entity(commands, sphere.clone(), fireball_mat.clone()))
        .collect();
    let debris = (0..EXPL_DEBRIS_COUNT)
        .map(|_| spawn_fx_entity(commands, sphere.clone(), debris_mat.clone()))
        .collect();
    let mushroom_smoke = (0..EXPL_MUSHROOM_COUNT)
        .map(|_| spawn_fx_entity(commands, sphere.clone(), smoke_mat.clone()))
        .collect();

    ParticleRig::Explosion(ExplosionRig {
        flash_core,
        fireballs,
        shockwave_outer,
        shockwave_inner,
        debris,
        mushroom_smoke,
        flash_mat,
        fireball_mat,
        shock_outer_mat,
        shock_inner_mat,
        debris_mat,
        smoke_mat,
    })
}

/// Renders real-time 3D volumetric Fire, Smoke, and Explosion particle animations:
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
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tf_q: Query<(&mut Transform, &mut Visibility), With<ParticleFxEntity>>,
    mut gizmos: Gizmos,
) {
    if state.screen != AppScreen::PlotSkenario {
        for (_, rig) in pool.rigs.drain() {
            rig.despawn(&mut commands);
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

        if playback.play_test_mode {
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

    // Despawn inactive or deleted particle rigs
    let stale_ids: Vec<Uuid> = pool
        .rigs
        .keys()
        .copied()
        .filter(|id| !active_ids.contains(id))
        .collect();
    for sid in stale_ids {
        if let Some(rig) = pool.rigs.remove(&sid) {
            rig.despawn(&mut commands);
        }
    }

    if to_render.is_empty() {
        return;
    }

    let (sphere, cone, disc, ring) = pool.ensure_meshes(&mut meshes);
    let flat_rot = Quat::from_rotation_x(-FRAC_PI_2);

    for (anim_id, cfg, geo_pos, elapsed, dur_sec, is_selected_in_editor) in to_render {
        // Recreate rig if user switched particle kind in Inspector (e.g. Fire -> Smoke)
        if pool
            .rigs
            .get(&anim_id)
            .is_some_and(|r| r.kind() != cfg.kind)
            && let Some(old) = pool.rigs.remove(&anim_id)
        {
            old.despawn(&mut commands);
        }

        let rig = pool.rigs.entry(anim_id).or_insert_with(|| match cfg.kind {
            ParticleAnimKind::Fire => {
                spawn_fire_rig(&mut commands, &mut materials, &sphere, &cone, &disc)
            }
            ParticleAnimKind::Smoke => {
                spawn_smoke_rig(&mut commands, &mut materials, &sphere, &disc)
            }
            ParticleAnimKind::Explosion => {
                spawn_explosion_rig(&mut commands, &mut materials, &sphere, &ring)
            }
        });

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
                ParticleAnimKind::Fire => Color::srgba(1.0, 0.56, 0.14, 0.72),
                ParticleAnimKind::Smoke => Color::srgba(0.60, 0.78, 0.96, 0.72),
                ParticleAnimKind::Explosion => Color::srgba(1.0, 0.30, 0.25, 0.75),
            };
            gizmos.circle(
                Isometry3d::new(origin + Vec3::Y * 0.08, Quat::from_rotation_x(FRAC_PI_2)),
                radius,
                guide_col,
            );
        }

        match rig {
            ParticleRig::Fire(fr) => {
                let pulse = 1.0 + 0.08 * (t * 9.5).sin();
                if let Some(mut m) = materials.get_mut(&fr.base_mat) {
                    m.base_color = Color::srgba(
                        base_srgb.red,
                        (base_srgb.green * 0.65).min(1.0),
                        (base_srgb.blue * 0.35).min(1.0),
                        (0.35 + 0.14 * intensity).clamp(0.15, 0.72),
                    );
                }
                if let Some(mut m) = materials.get_mut(&fr.core_mat) {
                    m.base_color = Color::srgba(
                        (base_srgb.red * 0.35 + 0.65).min(1.0),
                        (base_srgb.green * 0.45 + 0.55).min(1.0),
                        (base_srgb.blue * 0.35 + 0.40).min(1.0),
                        (0.75 + 0.12 * intensity).clamp(0.45, 0.96),
                    );
                }
                if let Some(mut m) = materials.get_mut(&fr.outer_mat) {
                    m.base_color = Color::srgba(
                        base_srgb.red,
                        base_srgb.green,
                        base_srgb.blue,
                        (0.62 + 0.15 * intensity).clamp(0.35, 0.92),
                    );
                }
                if let Some(mut m) = materials.get_mut(&fr.smoke_mat) {
                    m.base_color = Color::srgba(
                        0.15,
                        0.17,
                        0.21,
                        (0.32 + 0.16 * intensity).clamp(0.18, 0.66),
                    );
                }
                if let Some(mut m) = materials.get_mut(&fr.ember_mat) {
                    m.base_color = Color::srgba(
                        1.0,
                        (0.72 + base_srgb.green * 0.25).min(1.0),
                        (0.20 + base_srgb.blue * 0.40).min(1.0),
                        0.95,
                    );
                }

                // Hot base glow disc
                if let Ok((mut tf, mut vis)) = tf_q.get_mut(fr.base_disc) {
                    tf.translation = origin + Vec3::Y * 0.06;
                    tf.rotation = flat_rot;
                    tf.scale = Vec3::splat(radius * 1.12 * pulse);
                    *vis = Visibility::Visible;
                }

                // Inner bright flickering flame tongues
                let speed = 1.15 * rise_spd;
                for (i, &e) in fr.core_flames.iter().enumerate() {
                    let fi = i as f32;
                    let u = ((fi / FIRE_CORE_COUNT as f32) + t * speed * (0.9 + 0.25 * (fi * 1.37).sin().abs()))
                        .rem_euclid(1.0);
                    let ang = fi * 2.399_963 + (t * 1.8).sin() * 0.35;
                    let radial = radius * 0.42 * (1.0 - 0.55 * u) * (0.3 + 0.7 * (fi * 0.618).fract());
                    let wind_drift = wind_vec * (wind_spd * 0.22 * height * 0.12 * u * u);
                    let sway = Vec3::new((t * 7.0 + fi * 2.1).sin(), 0.0, (t * 6.2 + fi * 1.7).cos())
                        * (radius * 0.09 * u);
                    let y = height * 0.55 * u;
                    let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + wind_drift + sway;

                    let env = (u * PI).sin().powf(0.65).max(0.05);
                    let sx = radius * (0.28 + 0.16 * intensity.min(1.8)) * env;
                    let sy = height * (0.26 + 0.12 * intensity.min(1.8)) * env;
                    let lean = (wind_vec * (wind_spd * 0.04 * u) + Vec3::Y).normalize_or(Vec3::Y);
                    if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                        tf.translation = pos;
                        tf.rotation = Quat::from_rotation_arc(Vec3::Y, lean);
                        tf.scale = Vec3::new(sx, sy, sx);
                        *vis = Visibility::Visible;
                    }
                }

                // Outer vivid flame tongues
                for (i, &e) in fr.outer_flames.iter().enumerate() {
                    let fi = i as f32;
                    let u = ((fi / FIRE_OUTER_COUNT as f32) + t * speed * 0.85 * (0.85 + 0.3 * (fi * 2.13).cos().abs()))
                        .rem_euclid(1.0);
                    let ang = fi * 2.399_963 + 0.7 + (t * 1.3 + fi).sin() * 0.28;
                    let radial = radius * (0.78 * (1.0 - 0.45 * u) + spread * 0.25 * u)
                        * (0.35 + 0.65 * (fi * 0.754).fract());
                    let wind_drift = wind_vec * (wind_spd * 0.35 * height * 0.14 * u * u);
                    let y = height * 0.78 * u;
                    let pos = origin + Vec3::new(ang.cos() * radial, y, ang.sin() * radial) + wind_drift;

                    let env = (u * PI).sin().powf(0.55).max(0.05);
                    let sx = radius * (0.34 + 0.18 * intensity.min(2.0)) * env * (1.0 + 0.25 * spread);
                    let sy = height * (0.32 + 0.14 * intensity.min(2.0)) * env;
                    let lean = (wind_vec * (wind_spd * 0.06 * u) + Vec3::Y).normalize_or(Vec3::Y);
                    if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                        tf.translation = pos;
                        tf.rotation = Quat::from_rotation_arc(Vec3::Y, lean);
                        tf.scale = Vec3::new(sx, sy, sx);
                        *vis = Visibility::Visible;
                    }
                }

                // Dark smoke puffs rising above flames (optional)
                for (i, &e) in fr.smoke_puffs.iter().enumerate() {
                    if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                        if !cfg.with_smoke {
                            *vis = Visibility::Hidden;
                            continue;
                        }
                        let fi = i as f32;
                        let u = ((fi / FIRE_SMOKE_COUNT as f32) + t * 0.32 * rise_spd).rem_euclid(1.0);
                        let ang = fi * 2.399_963 + t * 0.35;
                        let radial = radius * (0.35 + u * (0.9 + spread * 1.3)) * (0.35 + 0.65 * (fi * 0.618).fract());
                        let smoke_base_y = height * 0.55;
                        let smoke_rise = height * 1.45 * u;
                        let wind_drift = wind_vec * (wind_spd * (0.35 + 1.35 * u * u) * (height * 0.14).max(1.2));
                        let pos = origin
                            + Vec3::new(ang.cos() * radial, smoke_base_y + smoke_rise, ang.sin() * radial)
                            + wind_drift;
                        let env = if u < 0.15 {
                            u / 0.15
                        } else if u > 0.78 {
                            ((1.0 - u) / 0.22).max(0.05)
                        } else {
                            1.0
                        };
                        let r_puff = radius * (0.32 + u * (0.62 + spread * 0.55)) * env;
                        tf.translation = pos;
                        tf.rotation = Quat::IDENTITY;
                        tf.scale = Vec3::new(r_puff, r_puff * 0.84, r_puff);
                        *vis = Visibility::Visible;
                    }
                }

                // Glowing rising embers (optional)
                for (i, &e) in fr.embers.iter().enumerate() {
                    if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                        if !cfg.with_embers {
                            *vis = Visibility::Hidden;
                            continue;
                        }
                        let fi = i as f32;
                        let u = ((fi / FIRE_EMBER_COUNT as f32) + t * 0.58 * rise_spd * (0.75 + 0.5 * (fi * 0.618).fract()))
                            .rem_euclid(1.0);
                        let ang = fi * 2.399_963 + t * 2.8 + u * 4.5;
                        let radial = radius * (0.35 + u * 1.25) * (0.4 + 0.6 * (fi * 0.754).fract());
                        let wind_drift = wind_vec * (wind_spd * 0.9 * u * u * (height * 0.15).max(1.0));
                        let pos = origin
                            + Vec3::new(ang.cos() * radial, height * (0.25 + 1.45 * u), ang.sin() * radial)
                            + wind_drift;
                        let r_emb = (0.06 + radius * 0.025).clamp(0.05, 0.35) * (1.0 - 0.65 * u);
                        tf.translation = pos;
                        tf.rotation = Quat::IDENTITY;
                        tf.scale = Vec3::new(r_emb, r_emb * 1.9, r_emb);
                        *vis = Visibility::Visible;
                    }
                }
            }
            ParticleRig::Smoke(sr) => {
                let tier_alphas = [
                    (0.58 * intensity).clamp(0.22, 0.88),
                    (0.48 * intensity).clamp(0.18, 0.78),
                    (0.34 * intensity).clamp(0.12, 0.62),
                    (0.18 * intensity).clamp(0.06, 0.38),
                ];
                let tier_bright = [0.82_f32, 0.94, 1.06, 1.16];
                if let Some(mut m) = materials.get_mut(&sr.base_mat) {
                    m.base_color = Color::srgba(
                        base_srgb.red * 0.75,
                        base_srgb.green * 0.75,
                        base_srgb.blue * 0.75,
                        (0.32 * intensity).clamp(0.12, 0.55),
                    );
                }
                for (ti, mat_h) in sr.tier_mats.iter().enumerate() {
                    if let Some(mut m) = materials.get_mut(mat_h) {
                        let b = tier_bright[ti];
                        m.base_color = Color::srgba(
                            (base_srgb.red * b).min(1.0),
                            (base_srgb.green * b).min(1.0),
                            (base_srgb.blue * b).min(1.0),
                            tier_alphas[ti],
                        );
                    }
                }

                if let Ok((mut tf, mut vis)) = tf_q.get_mut(sr.base_disc) {
                    tf.translation = origin + Vec3::Y * 0.06;
                    tf.rotation = flat_rot;
                    tf.scale = Vec3::splat(radius * 0.92);
                    *vis = Visibility::Visible;
                }

                let cycle_speed = 0.22 * rise_spd;
                for (i, &e) in sr.puffs.iter().enumerate() {
                    let fi = i as f32;
                    let tier = (i * 4 / SMOKE_PUFF_COUNT).min(3);
                    let tier_base = tier as f32 * 0.25;
                    let local_u = (((i % (SMOKE_PUFF_COUNT / 4)) as f32 / (SMOKE_PUFF_COUNT / 4) as f32)
                        + t * cycle_speed * (0.85 + 0.3 * (fi * 1.13).sin().abs()))
                        .rem_euclid(1.0);
                    let u = (tier_base + local_u * 0.25).clamp(0.0, 1.0);

                    let ang = fi * 2.399_963 + t * 0.28;
                    let radial = radius * (0.28 + u * (0.75 + spread * 1.35))
                        * (0.30 + 0.70 * (fi * 0.618).fract());
                    let wind_drift = wind_vec * (wind_spd * u.powf(1.35) * (height * 0.16).max(1.5));
                    let billow = Vec3::new(
                        (t * 0.9 + fi * 1.9).sin() * radius * 0.18 * u,
                        0.0,
                        (t * 0.8 + fi * 2.7).cos() * radius * 0.18 * u,
                    );
                    let pos = origin
                        + Vec3::new(ang.cos() * radial, height * u + radius * 0.22, ang.sin() * radial)
                        + wind_drift
                        + billow;

                    let fade_scale = if u < 0.08 {
                        0.35 + 0.65 * (u / 0.08)
                    } else if u > 0.86 {
                        (1.0 - (u - 0.86) / 0.14 * 0.35).max(0.45)
                    } else {
                        1.0
                    };
                    let r_puff = radius * (0.42 + u * (0.68 + spread * 0.95)) * fade_scale;
                    if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                        tf.translation = pos;
                        tf.rotation = Quat::from_rotation_y(fi + t * 0.2);
                        tf.scale = Vec3::new(r_puff, r_puff * 0.82, r_puff);
                        *vis = Visibility::Visible;
                    }
                }
            }
            ParticleRig::Explosion(er) => {
                let p = (elapsed / dur_sec.max(0.5)) as f32;
                let active_blast = (0.0..=1.0).contains(&p);

                // Phase 1: Blinding core flash (p in 0.0..0.28)
                if let Ok((mut tf, mut vis)) = tf_q.get_mut(er.flash_core) {
                    if active_blast && p < 0.28 {
                        let fp = p / 0.28;
                        let flash_r = radius * (0.25 + 1.15 * fp.powf(0.35)) * intensity.sqrt();
                        let alpha = (1.0 - fp).powf(1.6) * 0.96;
                        if let Some(mut m) = materials.get_mut(&er.flash_mat) {
                            m.base_color = Color::srgba(1.0, 0.98, 0.86, alpha);
                        }
                        tf.translation = origin + Vec3::Y * (radius * 0.22 + fp * radius * 0.35);
                        tf.rotation = Quat::IDENTITY;
                        tf.scale = Vec3::splat(flash_r);
                        *vis = Visibility::Visible;
                    } else {
                        *vis = Visibility::Hidden;
                    }
                }

                // Phase 1b: Turbulent expanding fireball cluster (p in 0.0..0.56)
                let fb_visible = active_blast && p < 0.56;
                if fb_visible {
                    let fp = (p / 0.56).clamp(0.0, 1.0);
                    let cool = fp.powf(0.8);
                    let r_c = base_srgb.red * (1.0 - 0.35 * cool) + 0.35 * (1.0 - cool);
                    let g_c = (base_srgb.green * (1.0 - 0.72 * cool) + 0.45 * (1.0 - cool).powi(2)).clamp(0.0, 1.0);
                    let b_c = (base_srgb.blue * (1.0 - 0.80 * cool) + 0.25 * (1.0 - cool).powi(3)).clamp(0.0, 1.0);
                    let alpha = (1.0 - fp.powf(2.2)) * (0.78 + 0.14 * intensity.min(1.5)).min(0.95);
                    if let Some(mut m) = materials.get_mut(&er.fireball_mat) {
                        m.base_color = Color::srgba(r_c.min(1.0), g_c, b_c, alpha);
                    }
                    let expand = fp.powf(0.42);
                    for (i, &e) in er.fireballs.iter().enumerate() {
                        let fi = i as f32;
                        let az = fi * 2.399_963;
                        let elev = (0.15 + 0.80 * ((fi * 0.618).fract())) * FRAC_PI_2;
                        let dist = radius * (0.35 + 0.72 * (fi * 0.754).fract()) * expand;
                        let lift = height * 0.32 * fp;
                        let pos = origin
                            + Vec3::new(
                                az.cos() * elev.cos() * dist,
                                (elev.sin() * dist * 0.92 + lift).max(0.15),
                                az.sin() * elev.cos() * dist,
                            );
                        let r_ball = radius * (0.28 + 0.26 * (fi * 0.431).fract()) * (0.3 + 0.85 * expand) * (1.0 - 0.35 * fp);
                        if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                            tf.translation = pos;
                            tf.rotation = Quat::IDENTITY;
                            tf.scale = Vec3::splat(r_ball.max(0.1));
                            *vis = Visibility::Visible;
                        }
                    }
                } else {
                    for &e in &er.fireballs {
                        if let Ok((_, mut vis)) = tf_q.get_mut(e) {
                            *vis = Visibility::Hidden;
                        }
                    }
                }

                // Phase 2: Expanding ground shockwave rings (p in 0.0..0.46)
                let shock_vis = active_blast && cfg.shockwave_enabled && p < 0.46;
                for (ring_e, mat_h, delay, max_mult) in [
                    (er.shockwave_outer, &er.shock_outer_mat, 0.0_f32, 2.35_f32),
                    (er.shockwave_inner, &er.shock_inner_mat, 0.06_f32, 1.65_f32),
                ] {
                    if let Ok((mut tf, mut vis)) = tf_q.get_mut(ring_e) {
                        if shock_vis && p >= delay {
                            let sp = ((p - delay) / (0.46 - delay)).clamp(0.0, 1.0);
                            let r_ring = radius * (0.22 + max_mult * sp.powf(0.55));
                            let alpha = (1.0 - sp).powf(1.5) * 0.82;
                            if let Some(mut m) = materials.get_mut(mat_h) {
                                m.base_color = Color::srgba(
                                    (base_srgb.red * 0.4 + 0.6).min(1.0),
                                    (base_srgb.green * 0.4 + 0.55).min(1.0),
                                    (base_srgb.blue * 0.3 + 0.35).min(1.0),
                                    alpha,
                                );
                            }
                            tf.translation = origin + Vec3::Y * 0.14;
                            tf.rotation = flat_rot;
                            tf.scale = Vec3::splat(r_ring);
                            *vis = Visibility::Visible;
                        } else {
                            *vis = Visibility::Hidden;
                        }
                    }
                }

                // Phase 3: Ballistic flying fire/debris streaks (p in 0.02..0.82)
                let debris_vis = active_blast && cfg.with_embers && (0.02..0.82).contains(&p);
                if debris_vis {
                    let dp = ((p - 0.02) / 0.80).clamp(0.0, 1.0);
                    if let Some(mut m) = materials.get_mut(&er.debris_mat) {
                        m.base_color = Color::srgba(1.0, 0.78 - 0.35 * dp, 0.22, (1.0 - dp).powf(1.2));
                    }
                    for (i, &e) in er.debris.iter().enumerate() {
                        let fi = i as f32;
                        let az = fi * (TAU / EXPL_DEBRIS_COUNT as f32) + 0.35;
                        let h_dist = radius * (1.2 + 1.4 * (fi * 0.618).fract()) * dp;
                        let peak_h = height * (0.42 + 0.48 * (fi * 0.754).fract());
                        let y = (4.0 * peak_h * dp * (1.0 - dp)).max(0.08);
                        let dx = az.cos() * h_dist;
                        let dz = az.sin() * h_dist;
                        let vel = Vec3::new(
                            az.cos() * radius * 1.8,
                            4.0 * peak_h * (1.0 - 2.0 * dp),
                            az.sin() * radius * 1.8,
                        )
                        .normalize_or(Vec3::Y);
                        let r_d = (0.12 + radius * 0.03).clamp(0.10, 0.65) * (1.0 - 0.55 * dp);
                        if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                            let mut t_d = Transform::from_translation(origin + Vec3::new(dx, y, dz))
                                .looking_to(vel, Vec3::Y);
                            t_d.scale = Vec3::new(r_d, r_d, r_d * 3.2);
                            *tf = t_d;
                            *vis = Visibility::Visible;
                        }
                    }
                } else {
                    for &e in &er.debris {
                        if let Ok((_, mut vis)) = tf_q.get_mut(e) {
                            *vis = Visibility::Hidden;
                        }
                    }
                }

                // Phase 4: Rising mushroom smoke cloud (p in 0.12..1.00)
                let smoke_vis = active_blast && cfg.with_smoke && p >= 0.12;
                if smoke_vis {
                    let sp = ((p - 0.12) / 0.88).clamp(0.0, 1.0);
                    let fade_in = (sp / 0.16).min(1.0);
                    let fade_out = ((1.0 - sp) / 0.28).clamp(0.0, 1.0);
                    let alpha = fade_in * fade_out * (0.58 + 0.14 * intensity.min(1.5)).min(0.82);
                    if let Some(mut m) = materials.get_mut(&er.smoke_mat) {
                        m.base_color = Color::srgba(0.18, 0.20, 0.24, alpha);
                    }
                    for (i, &e) in er.mushroom_smoke.iter().enumerate() {
                        let fi = i as f32;
                        let is_cap = i >= EXPL_MUSHROOM_COUNT / 3;
                        let az = fi * 2.399_963;
                        let wind_drift = wind_vec * (wind_spd * sp * sp * 1.4);
                        let (pos, r_puff) = if is_cap {
                            let cap_frac = ((i - EXPL_MUSHROOM_COUNT / 3) as f32)
                                / ((EXPL_MUSHROOM_COUNT - EXPL_MUSHROOM_COUNT / 3) as f32);
                            let cap_r = radius * (0.45 + 0.95 * sp * (0.65 + 0.55 * spread))
                                * (0.45 + 0.55 * cap_frac);
                            let cap_y = height * (0.28 + 0.72 * sp.powf(0.75))
                                + (fi * 1.7).sin() * radius * 0.18;
                            let pt = origin
                                + Vec3::new(az.cos() * cap_r, cap_y, az.sin() * cap_r)
                                + wind_drift;
                            let rp = radius * (0.32 + 0.42 * sp) * (0.8 + 0.35 * (fi * 0.618).fract());
                            (pt, rp)
                        } else {
                            let stem_frac = (i as f32 + 0.5) / (EXPL_MUSHROOM_COUNT / 3) as f32;
                            let stem_r = radius * 0.26 * (1.0 + 0.4 * sp);
                            let stem_y = height * 0.72 * sp.powf(0.8) * stem_frac;
                            let pt = origin
                                + Vec3::new(az.cos() * stem_r, stem_y, az.sin() * stem_r)
                                + wind_drift * stem_frac;
                            let rp = radius * (0.28 + 0.22 * sp);
                            (pt, rp)
                        };
                        if let Ok((mut tf, mut vis)) = tf_q.get_mut(e) {
                            tf.translation = pos;
                            tf.rotation = Quat::IDENTITY;
                            tf.scale = Vec3::new(r_puff, r_puff * 0.82, r_puff);
                            *vis = Visibility::Visible;
                        }
                    }
                } else {
                    for &e in &er.mushroom_smoke {
                        if let Ok((_, mut vis)) = tf_q.get_mut(e) {
                            *vis = Visibility::Hidden;
                        }
                    }
                }
            }
        }
    }
}
