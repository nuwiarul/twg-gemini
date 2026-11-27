use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use twg_shared::tactical_arrow_anim::{
    TacticalArrowConfig, is_tactical_arrow_anim, parse_tactical_arrow_config,
    smooth_polyline_points,
};
use uuid::Uuid;

use crate::{
    scene::parse_hex_color,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity},
};

#[derive(Component)]
pub struct TacticalArrowFxEntity;

#[derive(Debug, Clone)]
pub struct ActiveTacticalArrowLabel {
    pub world_pos: Vec3,
    pub text: String,
    pub color_hex: String,
    pub accent_hex: String,
    pub alpha: f32,
}

struct TacticalArrowRig {
    body_entity: Entity,
    body_mesh: Handle<Mesh>,
    body_mat: Handle<StandardMaterial>,
    accent_entity: Entity,
    accent_mesh: Handle<Mesh>,
    accent_mat: Handle<StandardMaterial>,
}

impl TacticalArrowRig {
    fn despawn(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) {
        commands.entity(self.body_entity).despawn();
        commands.entity(self.accent_entity).despawn();
        meshes.remove(self.body_mesh.id());
        meshes.remove(self.accent_mesh.id());
        materials.remove(self.body_mat.id());
        materials.remove(self.accent_mat.id());
    }
}

#[derive(Resource, Default)]
pub struct TacticalArrowVisualPool {
    rigs: HashMap<Uuid, TacticalArrowRig>,
    pub active_labels: Vec<ActiveTacticalArrowLabel>,
}

/// Samples a point and unit forward direction along a 3D polyline at metric distance `dist`.
fn sample_polyline(pts: &[Vec3], cum_len: &[f32], dist: f32) -> (Vec3, Vec3) {
    if pts.len() < 2 {
        return (pts.first().copied().unwrap_or(Vec3::ZERO), Vec3::Z);
    }
    let total = *cum_len.last().unwrap_or(&0.0);
    let d = dist.clamp(0.0, total);
    let mut seg = 0usize;
    while seg + 2 < pts.len() && cum_len[seg + 1] < d {
        seg += 1;
    }
    let a = pts[seg];
    let b = pts[seg + 1];
    let seg_len = (cum_len[seg + 1] - cum_len[seg]).max(1e-4);
    let t = ((d - cum_len[seg]) / seg_len).clamp(0.0, 1.0);
    let pos = a.lerp(b, t);
    let mut dir = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
    if dir.length_squared() < 1e-6 {
        dir = Vec3::Z;
    } else {
        dir = dir.normalize();
    }
    (pos, dir)
}

fn push_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    d: Vec3,
) {
    let base = positions.len() as u32;
    positions.push(a.to_array());
    positions.push(b.to_array());
    positions.push(c.to_array());
    positions.push(d.to_array());
    for _ in 0..4 {
        normals.push([0.0, 1.0, 0.0]);
    }
    uvs.push([0.0, 0.0]);
    uvs.push([1.0, 0.0]);
    uvs.push([1.0, 1.0]);
    uvs.push([0.0, 1.0]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_tri(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
) {
    let base = positions.len() as u32;
    positions.push(a.to_array());
    positions.push(b.to_array());
    positions.push(c.to_array());
    for _ in 0..3 {
        normals.push([0.0, 1.0, 0.0]);
    }
    uvs.push([0.0, 0.0]);
    uvs.push([1.0, 0.0]);
    uvs.push([0.5, 1.0]);
    indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn finalize_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Builds `(body_mesh, accent_mesh, midpoint_world)` for one animated tactical flow arrow.
fn build_tactical_arrow_meshes(
    world_pts: &[Vec3],
    cfg: &TacticalArrowConfig,
    reveal_fraction: f32,
    flow_phase: f32,
) -> (Mesh, Mesh, Vec3) {
    let raw_arr: Vec<[f32; 3]> = world_pts.iter().map(|v| v.to_array()).collect();
    let smoothed_arr = smooth_polyline_points(&raw_arr, cfg.smooth_curve);
    let pts: Vec<Vec3> = smoothed_arr.into_iter().map(Vec3::from_array).collect();

    if pts.len() < 2 {
        let empty = || finalize_mesh(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        return (empty(), empty(), Vec3::ZERO);
    }

    let mut cum_len = Vec::with_capacity(pts.len());
    cum_len.push(0.0_f32);
    for i in 1..pts.len() {
        let seg = Vec2::new(pts[i].x - pts[i - 1].x, pts[i].z - pts[i - 1].z).length();
        cum_len.push(cum_len[i - 1] + seg);
    }
    let total_len = *cum_len.last().unwrap_or(&0.0);
    if total_len < 0.2 {
        let empty = || finalize_mesh(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        return (empty(), empty(), pts[0]);
    }

    let revealed_len = (total_len * reveal_fraction.clamp(0.02, 1.0)).max(0.25);
    let half_w = (cfg.width_m * 0.5).max(0.4);
    let head_len = (cfg.width_m * cfg.head_size_mult * 0.92).min(revealed_len * 0.48);
    let head_half_w = half_w * cfg.head_size_mult;
    let body_end_len = (revealed_len - head_len * 0.65).max(0.05);

    // 1. Body ribbon + main arrowhead
    let mut b_pos = Vec::new();
    let mut b_nrm = Vec::new();
    let mut b_uv = Vec::new();
    let mut b_idx = Vec::new();

    // 2. Glowing edge rails + marching chevrons + arrowhead outline accent
    let mut a_pos = Vec::new();
    let mut a_nrm = Vec::new();
    let mut a_uv = Vec::new();
    let mut a_idx = Vec::new();

    let step_m = (cfg.width_m * 0.65).clamp(1.0, 4.0);
    let num_steps = ((body_end_len / step_m).ceil() as usize).clamp(2, 260);
    let rail_w = (half_w * 0.16).clamp(0.12, 0.55);

    let mut prev_l = Vec3::ZERO;
    let mut prev_r = Vec3::ZERO;
    let mut prev_il = Vec3::ZERO;
    let mut prev_ir = Vec3::ZERO;

    for i in 0..=num_steps {
        let d = (i as f32 / num_steps as f32) * body_end_len;
        let (p, fwd) = sample_polyline(&pts, &cum_len, d);
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        // Subtle tail taper on the first 8% for a sleek tactical silhouette
        let taper = if d < half_w * 1.4 {
            0.78 + 0.22 * (d / (half_w * 1.4)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let hw = half_w * taper;
        let l = p - right * hw;
        let r = p + right * hw;
        let il = p - right * (hw - rail_w).max(0.05) + Vec3::Y * 0.04;
        let ir = p + right * (hw - rail_w).max(0.05) + Vec3::Y * 0.04;
        let ol = l + Vec3::Y * 0.04;
        let or = r + Vec3::Y * 0.04;

        if i > 0 {
            push_quad(&mut b_pos, &mut b_nrm, &mut b_uv, &mut b_idx, prev_l, prev_r, r, l);
            // Left & right bright side rails
            push_quad(
                &mut a_pos,
                &mut a_nrm,
                &mut a_uv,
                &mut a_idx,
                prev_l + Vec3::Y * 0.04,
                prev_il,
                il,
                ol,
            );
            push_quad(
                &mut a_pos,
                &mut a_nrm,
                &mut a_uv,
                &mut a_idx,
                prev_ir,
                prev_r + Vec3::Y * 0.04,
                or,
                ir,
            );
        }
        prev_l = l;
        prev_r = r;
        prev_il = il;
        prev_ir = ir;
    }

    // Main 3D arrowhead at `revealed_len`
    let (tip_pos, tip_fwd) = sample_polyline(&pts, &cum_len, revealed_len);
    let base_dist = (revealed_len - head_len).max(0.0);
    let (base_pos, base_fwd) = sample_polyline(&pts, &cum_len, base_dist);
    let head_dir = (tip_fwd + base_fwd).normalize_or_zero();
    let head_dir = if head_dir.length_squared() < 1e-4 {
        tip_fwd
    } else {
        head_dir
    };
    let head_right = Vec3::new(-head_dir.z, 0.0, head_dir.x);
    let wing_l = base_pos - head_right * head_half_w + Vec3::Y * 0.03;
    let wing_r = base_pos + head_right * head_half_w + Vec3::Y * 0.03;
    let tip = tip_pos + Vec3::Y * 0.03;
    let notch = base_pos + head_dir * (head_len * 0.18) + Vec3::Y * 0.03;

    // Two triangles forming a sleek notched arrowhead
    push_tri(&mut b_pos, &mut b_nrm, &mut b_uv, &mut b_idx, wing_l, notch, tip);
    push_tri(&mut b_pos, &mut b_nrm, &mut b_uv, &mut b_idx, notch, wing_r, tip);

    // Bright inner chevron accent inside the arrowhead
    let inner_tip = tip - head_dir * (head_len * 0.14) + Vec3::Y * 0.05;
    let inner_l = wing_l.lerp(tip, 0.25) + head_right * (head_half_w * 0.2) + Vec3::Y * 0.05;
    let inner_r = wing_r.lerp(tip, 0.25) - head_right * (head_half_w * 0.2) + Vec3::Y * 0.05;
    let inner_mid = inner_tip - head_dir * (head_len * 0.26);
    push_tri(
        &mut a_pos,
        &mut a_nrm,
        &mut a_uv,
        &mut a_idx,
        inner_l,
        inner_mid,
        inner_tip,
    );
    push_tri(
        &mut a_pos,
        &mut a_nrm,
        &mut a_uv,
        &mut a_idx,
        inner_mid,
        inner_r,
        inner_tip,
    );

    // Marching chevrons (`>>>`) gliding along the ribbon shaft!
    let spacing = cfg.chevron_spacing_m.max(3.0);
    let chev_half_w = half_w * 0.72;
    let chev_depth = (half_w * 0.95).min(spacing * 0.42);
    let chev_thick = (half_w * 0.36).clamp(0.35, 2.2);
    let mut d = flow_phase.clamp(0.0, 1.0) * spacing;
    while d < body_end_len - chev_depth * 0.5 {
        if d > chev_depth * 0.6 {
            let (cp, cfwd) = sample_polyline(&pts, &cum_len, d);
            let cright = Vec3::new(-cfwd.z, 0.0, cfwd.x);
            let y_lift = Vec3::Y * 0.07;

            let front_tip = cp + cfwd * (chev_depth * 0.55) + y_lift;
            let back_tip = front_tip - cfwd * chev_thick;
            let front_l = cp - cfwd * (chev_depth * 0.45) - cright * chev_half_w + y_lift;
            let back_l = front_l - cfwd * chev_thick;
            let front_r = cp - cfwd * (chev_depth * 0.45) + cright * chev_half_w + y_lift;
            let back_r = front_r - cfwd * chev_thick;

            // Left arm of chevron
            push_quad(
                &mut a_pos,
                &mut a_nrm,
                &mut a_uv,
                &mut a_idx,
                back_l,
                back_tip,
                front_tip,
                front_l,
            );
            // Right arm of chevron
            push_quad(
                &mut a_pos,
                &mut a_nrm,
                &mut a_uv,
                &mut a_idx,
                back_tip,
                back_r,
                front_r,
                front_tip,
            );
        }
        d += spacing;
    }

    let (mid_pos, _) = sample_polyline(&pts, &cum_len, revealed_len * 0.5);
    (
        finalize_mesh(b_pos, b_nrm, b_uv, b_idx),
        finalize_mesh(a_pos, a_nrm, a_uv, a_idx),
        mid_pos + Vec3::Y * 1.4,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn tactical_arrow_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    mut pool: ResMut<TacticalArrowVisualPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    pool.active_labels.clear();

    if !state.is_3d_map_screen() {
        for (_, rig) in pool.rigs.drain() {
            rig.despawn(&mut commands, &mut meshes, &mut materials);
        }
        return;
    }

    let playback_active = playback.is_playback_active();
    let wall_t = time.elapsed_secs_f64();
    let mut visible_ids: HashSet<Uuid> = HashSet::new();

    for anim in &state.anims {
        if !is_tactical_arrow_anim(anim) || state.hidden_entity_ids.contains(&anim.id) {
            continue;
        }
        if anim.geom.len() < 2 {
            continue;
        }

        // In Play Skenario / Editor Preview, show only Playing/Paused arrows;
        // in Plot Editor idle mode, show live animated preview when selected (or live edit).
        let (cfg, eval) = if playback_active {
            let st = playback.track_status(anim.id);
            if st != AnimRunStatus::Playing && st != AnimRunStatus::Paused {
                continue;
            }
            let elapsed = playback
                .tracks
                .get(&anim.id)
                .map(|t| t.distance_m)
                .unwrap_or(0.0);
            let cfg = parse_tactical_arrow_config(anim);
            let ev = cfg.evaluate_at(elapsed);
            if !ev.visible {
                continue;
            }
            (cfg, ev)
        } else {
            let is_selected = state.selected == SelectedEntity::Anim(anim.id);
            if !is_selected {
                continue;
            }
            let source_anim = state
                .anim_edit
                .as_ref()
                .filter(|ea| ea.id == anim.id)
                .unwrap_or(anim);
            let cfg = parse_tactical_arrow_config(source_anim);
            let flow_phase = ((wall_t * cfg.flow_speed as f64) % 1.0) as f32;
            let ev = twg_shared::tactical_arrow_anim::TacticalArrowEval {
                visible: true,
                reveal_fraction: 1.0,
                flow_phase,
                alpha: cfg.opacity,
                finished: false,
            };
            (cfg, ev)
        };

        let world_pts: Vec<Vec3> = anim
            .geom
            .iter()
            .map(|p| {
                let alt = p[2].max(0.0) + cfg.altitude_m as f64;
                Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], alt))
            })
            .collect();

        let (body_mesh_data, accent_mesh_data, mid_world) =
            build_tactical_arrow_meshes(&world_pts, &cfg, eval.reveal_fraction, eval.flow_phase);

        let body_col = parse_hex_color(&cfg.color, eval.alpha * 0.85);
        let body_srgb = body_col.to_srgba();
        let accent_col = parse_hex_color(&cfg.accent_color, eval.alpha.clamp(0.35, 1.0));
        let accent_srgb = accent_col.to_srgba();

        if let Some(rig) = pool.rigs.get(&anim.id) {
            if let Some(mut m) = meshes.get_mut(&rig.body_mesh) {
                *m = body_mesh_data;
            }
            if let Some(mut m) = meshes.get_mut(&rig.accent_mesh) {
                *m = accent_mesh_data;
            }
            if let Some(mut mat) = materials.get_mut(&rig.body_mat) {
                mat.base_color = body_col;
                mat.emissive = LinearRgba::new(
                    body_srgb.red * 0.55,
                    body_srgb.green * 0.55,
                    body_srgb.blue * 0.55,
                    1.0,
                );
            }
            if let Some(mut mat) = materials.get_mut(&rig.accent_mat) {
                mat.base_color = accent_col;
                mat.emissive = LinearRgba::new(
                    accent_srgb.red * 1.45,
                    accent_srgb.green * 1.45,
                    accent_srgb.blue * 1.45,
                    1.0,
                );
            }
        } else {
            let body_mesh = meshes.add(body_mesh_data);
            let accent_mesh = meshes.add(accent_mesh_data);
            let body_mat = materials.add(StandardMaterial {
                base_color: body_col,
                emissive: LinearRgba::new(
                    body_srgb.red * 0.55,
                    body_srgb.green * 0.55,
                    body_srgb.blue * 0.55,
                    1.0,
                ),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let accent_mat = materials.add(StandardMaterial {
                base_color: accent_col,
                emissive: LinearRgba::new(
                    accent_srgb.red * 1.45,
                    accent_srgb.green * 1.45,
                    accent_srgb.blue * 1.45,
                    1.0,
                ),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let body_entity = commands
                .spawn((
                    Mesh3d(body_mesh.clone()),
                    MeshMaterial3d(body_mat.clone()),
                    Transform::IDENTITY,
                    TacticalArrowFxEntity,
                ))
                .id();
            let accent_entity = commands
                .spawn((
                    Mesh3d(accent_mesh.clone()),
                    MeshMaterial3d(accent_mat.clone()),
                    Transform::IDENTITY,
                    TacticalArrowFxEntity,
                ))
                .id();
            pool.rigs.insert(
                anim.id,
                TacticalArrowRig {
                    body_entity,
                    body_mesh,
                    body_mat,
                    accent_entity,
                    accent_mesh,
                    accent_mat,
                },
            );
        }

        if let Some(lbl) = cfg.effective_label() {
            pool.active_labels.push(ActiveTacticalArrowLabel {
                world_pos: mid_world,
                text: lbl.to_string(),
                color_hex: cfg.color.clone(),
                accent_hex: cfg.accent_color.clone(),
                alpha: eval.alpha,
            });
        }

        visible_ids.insert(anim.id);
    }

    let stale: Vec<Uuid> = pool
        .rigs
        .keys()
        .copied()
        .filter(|id| !visible_ids.contains(id))
        .collect();
    for id in stale {
        if let Some(rig) = pool.rigs.remove(&id) {
            rig.despawn(&mut commands, &mut meshes, &mut materials);
        }
    }
}
