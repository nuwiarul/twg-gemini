use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use twg_shared::shoot_anim::{
    WaterCannonConfig, is_water_cannon_anim, parse_water_cannon_config,
};
use uuid::Uuid;

use crate::{
    scene::{parse_hex_color, playback::PlaybackOverrides},
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity},
};

const CORE_SEGMENTS: usize = 24;
const MIST_SEGMENTS: usize = 18;
const TUBE_SIDES: usize = 8;
const STREAM_DROPLET_COUNT: usize = 32;
const SPLASH_DROPLET_COUNT: usize = 12;

#[derive(Component)]
pub struct WaterCannonFxEntity;

struct WaterCannonRig {
    core_entity: Entity,
    core_mesh: Handle<Mesh>,
    core_mat: Handle<StandardMaterial>,
    mist_entity: Entity,
    mist_mesh: Handle<Mesh>,
    mist_mat: Handle<StandardMaterial>,
    droplet_mat: Handle<StandardMaterial>,
    splash_mat: Handle<StandardMaterial>,
    ring_a_mat: Handle<StandardMaterial>,
    ring_b_mat: Handle<StandardMaterial>,
    stream_droplets: Vec<Entity>,
    splash_disc: Entity,
    splash_ring_a: Entity,
    splash_ring_b: Entity,
    splash_droplets: Vec<Entity>,
}

#[derive(Resource, Default)]
pub struct WaterCannonVisualPool {
    droplet_mesh: Option<Handle<Mesh>>,
    splash_disc_mesh: Option<Handle<Mesh>>,
    splash_ring_mesh: Option<Handle<Mesh>>,
    rigs: HashMap<Uuid, WaterCannonRig>,
}

impl WaterCannonVisualPool {
    fn ensure_shared_meshes(&mut self, meshes: &mut Assets<Mesh>) -> (Handle<Mesh>, Handle<Mesh>, Handle<Mesh>) {
        let drop_m = self
            .droplet_mesh
            .get_or_insert_with(|| meshes.add(Sphere::new(1.0).mesh().uv(10, 6)))
            .clone();
        let disc_m = self
            .splash_disc_mesh
            .get_or_insert_with(|| meshes.add(Circle::new(1.0)))
            .clone();
        let ring_m = self
            .splash_ring_mesh
            .get_or_insert_with(|| meshes.add(Annulus::new(0.68, 1.0)))
            .clone();
        (drop_m, disc_m, ring_m)
    }

    fn despawn_rig(commands: &mut Commands, rig: WaterCannonRig) {
        commands.entity(rig.core_entity).despawn();
        commands.entity(rig.mist_entity).despawn();
        commands.entity(rig.splash_disc).despawn();
        commands.entity(rig.splash_ring_a).despawn();
        commands.entity(rig.splash_ring_b).despawn();
        for e in rig.stream_droplets {
            commands.entity(e).despawn();
        }
        for e in rig.splash_droplets {
            commands.entity(e).despawn();
        }
    }
}

/// Renders real-time 3D volumetric Water Cannon streams (parabolic jet core, outer spray mist,
/// animated high-velocity droplet streaks, and ground impact splash rings/droplets):
/// - In Editor: live preview automatically plays whenever a Water Cannon animation is selected.
/// - In Play Skenario: active during `Playing` (and frozen at current phase during `Paused`),
///   automatically tracking moving vehicles via `PlaybackOverrides`.
#[allow(clippy::too_many_arguments)]
pub fn water_cannon_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    mut pool: ResMut<WaterCannonVisualPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tf_q: Query<(&mut Transform, &mut Visibility), With<WaterCannonFxEntity>>,
    mut mesh3d_q: Query<&mut Mesh3d, With<WaterCannonFxEntity>>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        for (_, rig) in pool.rigs.drain() {
            WaterCannonVisualPool::despawn_rig(&mut commands, rig);
        }
        return;
    }

    let mut active_ids: HashSet<Uuid> = HashSet::new();
    let editor_time = time.elapsed_secs_f64();

    // Determine which Water Cannon animations should be rendered right now
    let mut to_render: Vec<(Uuid, Uuid, WaterCannonConfig, f64, bool)> = Vec::new();
    for anim in &state.anims {
        if !is_water_cannon_anim(anim) || state.hidden_entity_ids.contains(&anim.id) {
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

        if let Some(phase_sec) = play_phase_opt {
            let action_delay = twg_shared::shoot_anim::parse_shoot_model_action_config(anim)
                .trigger_delay_sec();
            if phase_sec >= action_delay {
                let cfg = parse_water_cannon_config(anim);
                to_render.push((
                    anim.id,
                    marker_id,
                    cfg,
                    (phase_sec - action_delay).max(0.0),
                    false,
                ));
                active_ids.insert(anim.id);
            }
        } else if is_editor_preview {
            let cfg = parse_water_cannon_config(anim);
            to_render.push((anim.id, marker_id, cfg, editor_time, true));
            active_ids.insert(anim.id);
        }
    }

    // Despawn rigs for inactive / deselected / deleted Water Cannon animations
    let stale_ids: Vec<Uuid> = pool
        .rigs
        .keys()
        .copied()
        .filter(|id| !active_ids.contains(id))
        .collect();
    for sid in stale_ids {
        if let Some(rig) = pool.rigs.remove(&sid) {
            WaterCannonVisualPool::despawn_rig(&mut commands, rig);
        }
    }

    if to_render.is_empty() {
        return;
    }

    let (drop_mesh, disc_mesh, ring_mesh) = pool.ensure_shared_meshes(&mut meshes);
    let flat_rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    for (anim_id, marker_id, cfg, anim_time, is_editor_preview) in to_render {
        let playback_active = playback.is_playback_active();
        let Some(marker) = state
            .markers
            .iter()
            .find(|m| m.id == marker_id && overrides.is_marker_visible(m, playback_active))
        else {
            if let Some(rig) = pool.rigs.remove(&anim_id) {
                WaterCannonVisualPool::despawn_rig(&mut commands, rig);
            }
            continue;
        };

        // Track live vehicle pose during Play Skenario or Plot-Editor Preview if a Route animation is moving it!
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

        let nozzle_world = base_local
            + Vec3::Y * (cfg.nozzle_height_m as f32)
            + veh_fwd * (cfg.nozzle_forward_m as f32)
            + veh_right * (cfg.nozzle_right_m as f32);

        let spray_yaw = ((rot_y + cfg.effective_yaw_offset_deg(anim_time)) as f32).to_radians();
        let spray_fwd = Vec3::new(spray_yaw.sin(), 0.0, spray_yaw.cos());
        let spray_right = Vec3::new(-spray_fwd.z, 0.0, spray_fwd.x);

        let base_col = parse_hex_color(&cfg.color_hex, 1.0).to_srgba();
        let pressure_norm = ((cfg.pressure_bar - 1.0) / 9.0).clamp(0.0, 1.0) as f32;
        let spread = cfg.spread.clamp(0.0, 1.0) as f32;
        let flow_speed = 0.65 + pressure_norm * 1.45;
        let base_r = 0.055 + pressure_norm * 0.135;

        // Spawn rig lazily if not yet allocated for this animation
        let rig = pool.rigs.entry(anim_id).or_insert_with(|| {
            let core_mesh = meshes.add(build_parabolic_tube_mesh(
                &cfg,
                nozzle_world,
                base_local.y,
                spray_fwd,
                spray_right,
                CORE_SEGMENTS,
                base_r,
                0.9 + spread * 3.8,
                anim_time as f32,
                flow_speed,
            ));
            let mist_mesh = meshes.add(build_parabolic_tube_mesh(
                &cfg,
                nozzle_world,
                base_local.y,
                spray_fwd,
                spray_right,
                MIST_SEGMENTS,
                base_r * 1.45,
                2.4 + spread * 7.8,
                anim_time as f32,
                flow_speed * 0.75,
            ));

            let core_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, 0.86),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let mist_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.85, 0.96, 1.0, 0.24),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let droplet_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.95, 0.99, 1.0, 0.92),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            let splash_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.90, 0.98, 1.0, 0.42),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let ring_a_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.90, 0.98, 1.0, 0.72),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let ring_b_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.90, 0.98, 1.0, 0.72),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });

            let core_entity = commands
                .spawn((
                    Mesh3d(core_mesh.clone()),
                    MeshMaterial3d(core_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    WaterCannonFxEntity,
                ))
                .id();
            let mist_entity = commands
                .spawn((
                    Mesh3d(mist_mesh.clone()),
                    MeshMaterial3d(mist_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    WaterCannonFxEntity,
                ))
                .id();

            let mut stream_droplets = Vec::with_capacity(STREAM_DROPLET_COUNT);
            for _ in 0..STREAM_DROPLET_COUNT {
                let e = commands
                    .spawn((
                        Mesh3d(drop_mesh.clone()),
                        MeshMaterial3d(droplet_mat.clone()),
                        Transform::IDENTITY,
                        Visibility::Visible,
                        WaterCannonFxEntity,
                    ))
                    .id();
                stream_droplets.push(e);
            }

            let splash_disc = commands
                .spawn((
                    Mesh3d(disc_mesh.clone()),
                    MeshMaterial3d(splash_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    WaterCannonFxEntity,
                ))
                .id();
            let splash_ring_a = commands
                .spawn((
                    Mesh3d(ring_mesh.clone()),
                    MeshMaterial3d(ring_a_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    WaterCannonFxEntity,
                ))
                .id();
            let splash_ring_b = commands
                .spawn((
                    Mesh3d(ring_mesh.clone()),
                    MeshMaterial3d(ring_b_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    WaterCannonFxEntity,
                ))
                .id();

            let mut splash_droplets = Vec::with_capacity(SPLASH_DROPLET_COUNT);
            for _ in 0..SPLASH_DROPLET_COUNT {
                let e = commands
                    .spawn((
                        Mesh3d(drop_mesh.clone()),
                        MeshMaterial3d(droplet_mat.clone()),
                        Transform::IDENTITY,
                        Visibility::Visible,
                        WaterCannonFxEntity,
                    ))
                    .id();
                splash_droplets.push(e);
            }

            WaterCannonRig {
                core_entity,
                core_mesh,
                core_mat,
                mist_entity,
                mist_mesh,
                mist_mat,
                droplet_mat,
                splash_mat,
                ring_a_mat,
                ring_b_mat,
                stream_droplets,
                splash_disc,
                splash_ring_a,
                splash_ring_b,
                splash_droplets,
            }
        });

        // Update materials with user-selected water/foam color and pressure/spread opacity
        let foam_r = base_col.red * 0.55 + 0.45;
        let foam_g = base_col.green * 0.55 + 0.45;
        let foam_b = base_col.blue * 0.55 + 0.45;

        if let Some(mut m) = materials.get_mut(&rig.core_mat) {
            let core_alpha = (0.76 + pressure_norm * 0.16 - spread * 0.24).clamp(0.35, 0.94);
            m.base_color = Color::srgba(foam_r, foam_g, foam_b, core_alpha);
        }
        if let Some(mut m) = materials.get_mut(&rig.mist_mat) {
            let mist_alpha = (0.14 + spread * 0.22 + pressure_norm * 0.08).clamp(0.10, 0.45);
            m.base_color = Color::srgba(base_col.red, base_col.green, base_col.blue, mist_alpha);
        }
        if let Some(mut m) = materials.get_mut(&rig.droplet_mat) {
            m.base_color = Color::srgba(
                (foam_r + 0.08).min(1.0),
                (foam_g + 0.08).min(1.0),
                (foam_b + 0.08).min(1.0),
                0.88,
            );
        }
        if let Some(mut m) = materials.get_mut(&rig.splash_mat) {
            m.base_color = Color::srgba(foam_r, foam_g, foam_b, 0.36 + pressure_norm * 0.18);
        }

        // Rebuild dynamic parabolic jet core & outer mist sheath meshes in-place
        if let Some(mut mesh) = meshes.get_mut(&rig.core_mesh) {
            *mesh = build_parabolic_tube_mesh(
                &cfg,
                nozzle_world,
                base_local.y,
                spray_fwd,
                spray_right,
                CORE_SEGMENTS,
                base_r,
                0.9 + spread * 3.8,
                anim_time as f32,
                flow_speed,
            );
        }
        if let Ok(mut m3d) = mesh3d_q.get_mut(rig.core_entity) {
            m3d.set_changed();
        }

        if let Some(mut mesh) = meshes.get_mut(&rig.mist_mesh) {
            *mesh = build_parabolic_tube_mesh(
                &cfg,
                nozzle_world,
                base_local.y,
                spray_fwd,
                spray_right,
                MIST_SEGMENTS,
                base_r * 1.45,
                2.4 + spread * 7.8,
                anim_time as f32,
                flow_speed * 0.75,
            );
        }
        if let Ok(mut m3d) = mesh3d_q.get_mut(rig.mist_entity) {
            m3d.set_changed();
        }

        // Animate 32 high-velocity water/foam streaks flying along the parabola
        for (i, &drop_e) in rig.stream_droplets.iter().enumerate() {
            let fi = i as f32;
            let phase = ((fi / STREAM_DROPLET_COUNT as f32) + (anim_time as f32) * flow_speed)
                .rem_euclid(1.0);
            let (horiz, height, th, tv) = cfg.eval_parabola(phase as f64);
            let center_pt = Vec3::new(
                nozzle_world.x + spray_fwd.x * (horiz as f32),
                base_local.y + (height as f32),
                nozzle_world.z + spray_fwd.z * (horiz as f32),
            );
            let tang = (spray_fwd * (th as f32) + Vec3::Y * (tv as f32)).normalize_or(spray_fwd);
            let perp_u = tang.cross(Vec3::Y).normalize_or(spray_right);
            let perp_v = perp_u.cross(tang).normalize_or(Vec3::Y);

            // Deterministic organic dispersion widening smoothly toward the impact end
            let ang = fi * 2.399_963 + (anim_time as f32) * 3.2;
            let radial_factor = 0.25 + 0.75 * ((fi * 0.618_034).fract());
            let cone_r = base_r * (0.45 + phase * (1.2 + spread * 5.8)) * radial_factor;
            let pos = center_pt + (perp_u * ang.cos() + perp_v * ang.sin()) * cone_r;

            let drop_r = (base_r * (0.55 + phase * (0.75 + spread * 1.3))) * (0.75 + 0.5 * radial_factor);
            let streak_len = drop_r * (2.6 + pressure_norm * 2.4) * (1.15 - spread * 0.45);

            if let Ok((mut tf, mut vis)) = tf_q.get_mut(drop_e) {
                let mut t = Transform::from_translation(pos).looking_to(tang, Vec3::Y);
                t.scale = Vec3::new(drop_r, drop_r, streak_len);
                *tf = t;
                *vis = Visibility::Visible;
            }
        }

        // Ground impact splash pool, expanding foam rings, and bouncing droplets at t = 1.0
        let impact_pos = Vec3::new(
            nozzle_world.x + spray_fwd.x * (cfg.range_m as f32),
            base_local.y + 0.06,
            nozzle_world.z + spray_fwd.z * (cfg.range_m as f32),
        );
        let splash_base_r = (0.75 + pressure_norm * 1.35) * (1.0 + spread * 1.1);
        let pulse = 1.0 + 0.08 * ((anim_time as f32) * 10.0).sin();

        if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.splash_disc) {
            tf.translation = impact_pos;
            tf.rotation = flat_rot;
            tf.scale = Vec3::splat(splash_base_r * 0.72 * pulse);
            *vis = Visibility::Visible;
        }

        let ring_freq = 1.1 + pressure_norm * 1.2;
        for (ring_e, ring_mat, phase_off) in [
            (rig.splash_ring_a, &rig.ring_a_mat, 0.0_f32),
            (rig.splash_ring_b, &rig.ring_b_mat, 0.5_f32),
        ] {
            let p = ((anim_time as f32) * ring_freq + phase_off).rem_euclid(1.0);
            let r = splash_base_r * (0.45 + p * 1.65);
            let alpha = (1.0 - p).powf(1.4) * (0.58 + pressure_norm * 0.25);
            if let Some(mut m) = materials.get_mut(ring_mat) {
                m.base_color = Color::srgba(foam_r, foam_g, foam_b, alpha);
            }
            if let Ok((mut tf, mut vis)) = tf_q.get_mut(ring_e) {
                tf.translation = impact_pos + Vec3::Y * 0.02;
                tf.rotation = flat_rot;
                tf.scale = Vec3::splat(r);
                *vis = Visibility::Visible;
            }
        }

        // 12 bouncing impact spray droplets arcing outward from impact point
        let bounce_freq = 1.6 + pressure_norm * 1.5;
        let bounce_max_h = 0.7 + pressure_norm * 1.8;
        let bounce_max_r = splash_base_r * 1.45;
        for (i, &b_e) in rig.splash_droplets.iter().enumerate() {
            let fi = i as f32;
            let u = ((anim_time as f32) * bounce_freq + fi / SPLASH_DROPLET_COUNT as f32)
                .rem_euclid(1.0);
            let ang = fi * (std::f32::consts::TAU / SPLASH_DROPLET_COUNT as f32)
                + ((anim_time as f32) * 0.6);
            let radial = bounce_max_r * (0.25 + 0.75 * u);
            let h = 4.0 * bounce_max_h * u * (1.0 - u) * (0.65 + 0.35 * (fi * 1.73).sin().abs());
            let pos = Vec3::new(
                impact_pos.x + ang.cos() * radial,
                base_local.y + h.max(0.08),
                impact_pos.z + ang.sin() * radial,
            );
            let r = (0.09 + pressure_norm * 0.12) * (1.0 - 0.55 * u);
            if let Ok((mut tf, mut vis)) = tf_q.get_mut(b_e) {
                tf.translation = pos;
                tf.rotation = Quat::IDENTITY;
                tf.scale = Vec3::splat(r);
                *vis = Visibility::Visible;
            }
        }

        // Crisp tactical nozzle & target impact guide in Editor Preview mode
        if is_editor_preview {
            let guide_col = Color::srgba(0.22, 0.88, 0.98, 0.90);
            let guide_soft = Color::srgba(0.22, 0.88, 0.98, 0.45);
            gizmos.sphere(Isometry3d::from_translation(nozzle_world), 0.16, guide_col);
            gizmos.line(
                Vec3::new(nozzle_world.x, base_local.y + 0.05, nozzle_world.z),
                nozzle_world,
                guide_soft,
            );
            gizmos.circle(
                Isometry3d::new(
                    impact_pos + Vec3::Y * 0.04,
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                ),
                splash_base_r * 1.15,
                guide_col,
            );
        }
    }
}

/// Builds a smooth, tapered parabolic 3D tube mesh from `nozzle_world` to ground impact at
/// `cfg.range_m`, with subtle animated hydraulic wave pulsation along the stream.
#[allow(clippy::too_many_arguments)]
fn build_parabolic_tube_mesh(
    cfg: &WaterCannonConfig,
    nozzle_world: Vec3,
    base_y: f32,
    spray_fwd: Vec3,
    spray_right: Vec3,
    segments: usize,
    start_radius: f32,
    end_spread_mult: f32,
    anim_time: f32,
    flow_speed: f32,
) -> Mesh {
    let ring_count = segments + 1;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(ring_count * TUBE_SIDES);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(ring_count * TUBE_SIDES);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(ring_count * TUBE_SIDES);
    let mut indices: Vec<u32> = Vec::with_capacity(segments * TUBE_SIDES * 6);

    for s in 0..ring_count {
        let t = (s as f32) / (segments as f32);
        let (horiz, height, th, tv) = cfg.eval_parabola(t as f64);
        let center = Vec3::new(
            nozzle_world.x + spray_fwd.x * (horiz as f32),
            base_y + (height as f32),
            nozzle_world.z + spray_fwd.z * (horiz as f32),
        );
        let tang = (spray_fwd * (th as f32) + Vec3::Y * (tv as f32)).normalize_or(spray_fwd);
        let right = tang.cross(Vec3::Y).normalize_or(spray_right);
        let up = right.cross(tang).normalize_or(Vec3::Y);

        let ripple = 1.0 + 0.10 * (t * 22.0 - anim_time * flow_speed * 14.0).sin() * (0.25 + t);
        let radius = start_radius * (1.0 + t * end_spread_mult) * ripple;

        for k in 0..TUBE_SIDES {
            let ang = (k as f32) * (std::f32::consts::TAU / TUBE_SIDES as f32);
            let n = (right * ang.cos() + up * ang.sin()).normalize_or(Vec3::Y);
            let p = center + n * radius;
            positions.push(p.to_array());
            normals.push(n.to_array());
            uvs.push([k as f32 / TUBE_SIDES as f32, t]);
        }
    }

    for s in 0..segments {
        let r0 = (s * TUBE_SIDES) as u32;
        let r1 = ((s + 1) * TUBE_SIDES) as u32;
        for k in 0..TUBE_SIDES {
            let kn = ((k + 1) % TUBE_SIDES) as u32;
            let k0 = k as u32;
            indices.extend_from_slice(&[r0 + k0, r1 + k0, r1 + kn, r0 + k0, r1 + kn, r0 + kn]);
        }
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
