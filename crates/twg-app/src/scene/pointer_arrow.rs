use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use twg_shared::pointer_anim::{
    PointerArrowConfig, is_pointer_arrow_anim, parse_pointer_arrow_config,
};
use uuid::Uuid;

use crate::{
    scene::{
        markers::MarkerWorldBounds,
        parse_hex_color,
        playback::PlaybackOverrides,
    },
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity},
};

/// Height of the procedural unit-scale 3D downward arrow from its bottom tip (`y = 0`) to its top cap.
pub const ARROW_UNIT_HEIGHT: f32 = 2.45;

#[derive(Component)]
pub struct PointerArrowFxEntity;

/// Screen-projected floating text label produced each frame by `pointer_arrow_visual_system`
/// and rendered cleanly in `ui::overlays::draw_projected_labels`.
#[derive(Debug, Clone)]
pub struct ActivePointerLabel {
    pub world_pos: Vec3,
    pub text: String,
    pub font_size: f32,
    pub text_color_hex: String,
    pub accent_color_hex: String,
    pub show_bg: bool,
}

struct PointerArrowRig {
    arrow_entity: Entity,
    arrow_mat: Handle<StandardMaterial>,
    ring_entity: Entity,
    ring_mat: Handle<StandardMaterial>,
}

impl PointerArrowRig {
    fn despawn(self, commands: &mut Commands, materials: &mut Assets<StandardMaterial>) {
        commands.entity(self.arrow_entity).despawn();
        commands.entity(self.ring_entity).despawn();
        materials.remove(self.arrow_mat.id());
        materials.remove(self.ring_mat.id());
    }
}

#[derive(Resource, Default)]
pub struct PointerArrowVisualPool {
    arrow_mesh: Option<Handle<Mesh>>,
    ring_mesh: Option<Handle<Mesh>>,
    rigs: HashMap<Uuid, PointerArrowRig>,
    /// Active floating labels synchronized with each currently visible 3D pointer arrow.
    pub active_labels: Vec<ActivePointerLabel>,
}

impl PointerArrowVisualPool {
    fn ensure_shared_meshes(&mut self, meshes: &mut Assets<Mesh>) -> (Handle<Mesh>, Handle<Mesh>) {
        let arrow = self
            .arrow_mesh
            .get_or_insert_with(|| meshes.add(build_3d_downward_arrow_mesh(16)))
            .clone();
        let ring = self
            .ring_mesh
            .get_or_insert_with(|| meshes.add(Annulus::new(0.76, 1.0)))
            .clone();
        (arrow, ring)
    }
}

/// Builds a crisp, solid, closed 3D downward-pointing tactical arrow mesh with tip at `(0, 0, 0)`
/// and flat top cap at `y = ARROW_UNIT_HEIGHT`:
/// - Sharp conical arrowhead (`y: 0.0 -> 1.15`, radius `0.0 -> 0.72`)
/// - Crisp horizontal overhang collar (`y = 1.15`, radius `0.72 -> 0.30`)
/// - Cylindrical shaft (`y: 1.15 -> 2.45`, radius `0.30`)
/// - Solid top cap (`y = 2.45`, radius `0.30 -> 0.0`)
pub fn build_3d_downward_arrow_mesh(sides: usize) -> Mesh {
    let sides = sides.max(8);
    let head_h = 1.15_f32;
    let head_r = 0.72_f32;
    let shaft_r = 0.30_f32;
    let total_h = ARROW_UNIT_HEIGHT;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let push_tri = |positions: &mut Vec<[f32; 3]>,
                    normals: &mut Vec<[f32; 3]>,
                    uvs: &mut Vec<[f32; 2]>,
                    indices: &mut Vec<u32>,
                    a: Vec3,
                    b: Vec3,
                    c: Vec3,
                    na: Vec3,
                    nb: Vec3,
                    nc: Vec3| {
        let base = positions.len() as u32;
        positions.push(a.to_array());
        positions.push(b.to_array());
        positions.push(c.to_array());
        normals.push(na.to_array());
        normals.push(nb.to_array());
        normals.push(nc.to_array());
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([0.5, 1.0]);
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    };

    let cone_slope_len = head_h.hypot(head_r);
    let cone_ny = -head_r / cone_slope_len;
    let cone_nr = head_h / cone_slope_len;

    for k in 0..sides {
        let a0 = (k as f32) * (TAU / sides as f32);
        let a1 = (((k + 1) % sides) as f32) * (TAU / sides as f32);
        let (c0, s0) = (a0.cos(), a0.sin());
        let (c1, s1) = (a1.cos(), a1.sin());
        let am = 0.5 * (a0 + a1);
        let (cm, sm) = (am.cos(), am.sin());

        // 1. Arrowhead cone side (tip at y=0, flared rim at y=head_h)
        let tip = Vec3::ZERO;
        let rim0 = Vec3::new(c0 * head_r, head_h, s0 * head_r);
        let rim1 = Vec3::new(c1 * head_r, head_h, s1 * head_r);
        let n_tip = Vec3::new(cm * cone_nr, cone_ny, sm * cone_nr).normalize();
        let n0 = Vec3::new(c0 * cone_nr, cone_ny, s0 * cone_nr).normalize();
        let n1 = Vec3::new(c1 * cone_nr, cone_ny, s1 * cone_nr).normalize();
        // Outward CCW winding when viewed from outside: (tip, rim0, rim1) -> cross = (rim0 - tip) x (rim1 - tip)
        // At a0=0 (+X) and a1=+d (+Z): rim0=(r,h,0), rim1=(r,h,dz) -> rim0 x rim1 = (h*dz, 0, -r*dz) -> +X outward!
        push_tri(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            tip,
            rim0,
            rim1,
            n_tip,
            n0,
            n1,
        );

        // 2. Flat upward shoulder collar connecting arrowhead rim (head_r) inward to shaft (shaft_r)
        let sh_bot0 = Vec3::new(c0 * shaft_r, head_h, s0 * shaft_r);
        let sh_bot1 = Vec3::new(c1 * shaft_r, head_h, s1 * shaft_r);
        push_tri(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            rim0,
            sh_bot0,
            sh_bot1,
            Vec3::Y,
            Vec3::Y,
            Vec3::Y,
        );
        push_tri(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            rim0,
            sh_bot1,
            rim1,
            Vec3::Y,
            Vec3::Y,
            Vec3::Y,
        );

        // 3. Cylindrical shaft (head_h -> total_h)
        let sh_top0 = Vec3::new(c0 * shaft_r, total_h, s0 * shaft_r);
        let sh_top1 = Vec3::new(c1 * shaft_r, total_h, s1 * shaft_r);
        let sn0 = Vec3::new(c0, 0.0, s0);
        let sn1 = Vec3::new(c1, 0.0, s1);
        push_tri(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            sh_bot0,
            sh_top0,
            sh_top1,
            sn0,
            sn0,
            sn1,
        );
        push_tri(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            sh_bot0,
            sh_top1,
            sh_bot1,
            sn0,
            sn1,
            sn1,
        );

        // 4. Flat top cap at y = total_h
        let top_center = Vec3::new(0.0, total_h, 0.0);
        push_tri(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            top_center,
            sh_top1,
            sh_top0,
            Vec3::Y,
            Vec3::Y,
            Vec3::Y,
        );
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

/// Renders real-time bouncing 3D downward Pointer Arrows above their bound 3D markers/objects
/// and publishes synchronized floating label coordinates into `PointerArrowVisualPool::active_labels`:
/// - In **Play Skenario**: visible and animated ONLY when `Playing` (frozen when `Paused`),
///   completely hidden when `Stopped` or `Finished`. Automatically tracks moving route markers!
/// - In **Editor**: live preview plays whenever the Pointer Arrow animation is selected.
#[allow(clippy::too_many_arguments)]
pub fn pointer_arrow_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    marker_bounds: Res<MarkerWorldBounds>,
    mut pool: ResMut<PointerArrowVisualPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tf_q: Query<(&mut Transform, &mut Visibility), With<PointerArrowFxEntity>>,
    mut gizmos: Gizmos,
) {
    pool.active_labels.clear();

    if !state.is_3d_map_screen() {
        for (_, rig) in pool.rigs.drain() {
            rig.despawn(&mut commands, &mut materials);
        }
        return;
    }

    let editor_time = time.elapsed_secs_f64();
    let mut active_ids: HashSet<Uuid> = HashSet::new();
    let mut to_render: Vec<(Uuid, Uuid, PointerArrowConfig, f64, bool)> = Vec::new();

    for anim in &state.anims {
        if !is_pointer_arrow_anim(anim) || state.hidden_entity_ids.contains(&anim.id) {
            continue;
        }
        let Some(marker_id) = anim.marker_id else {
            continue;
        };
        let playback_active = playback.is_playback_active();
        let is_editor_preview =
            !playback_active && state.selected == SelectedEntity::Anim(anim.id);
        let play_phase_opt = if playback_active {
            match playback.track_status(anim.id) {
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
            to_render.push((
                anim.id,
                marker_id,
                parse_pointer_arrow_config(anim),
                phase_sec,
                false,
            ));
            active_ids.insert(anim.id);
        } else if is_editor_preview {
            to_render.push((
                anim.id,
                marker_id,
                parse_pointer_arrow_config(anim),
                editor_time,
                true,
            ));
            active_ids.insert(anim.id);
        }
    }

    // Despawn rigs for stopped, finished, deselected, or deleted pointer animations
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

    let (arrow_mesh, ring_mesh) = pool.ensure_shared_meshes(&mut meshes);
    let flat_rot = Quat::from_rotation_x(-FRAC_PI_2);

    for (anim_id, marker_id, cfg, anim_time, is_editor_preview) in to_render {
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

        // Resolve exact 3D top anchor & footprint radius of the target marker/object
        let bounds_opt = marker_bounds.bounds.get(&marker_id);
        let measured_height = bounds_opt
            .map(|b| (b.world_max.y - b.world_min.y).clamp(0.5, 160.0))
            .unwrap_or(3.0);
        let footprint_r = bounds_opt
            .map(|b| {
                ((b.world_max.x - b.world_min.x)
                    .hypot(b.world_max.z - b.world_min.z)
                    * 0.55)
                    .clamp(1.2, 45.0)
            })
            .unwrap_or(2.2);

        let (anchor_x, base_y, top_y, anchor_z) = if playback_active
            && let Some(ov) = overrides.markers.get(&marker_id)
        {
            let loc = state
                .geo_anchor
                .lng_lat_alt_to_local(ov.lng, ov.lat, ov.altitude);
            (loc[0], loc[1], loc[1] + measured_height, loc[2])
        } else if let Some(b) = bounds_opt {
            (
                b.footprint_center.x,
                b.world_min.y,
                b.world_max.y,
                b.footprint_center.z,
            )
        } else {
            let loc = state
                .geo_anchor
                .lng_lat_alt_to_local(marker.lng, marker.lat, marker.altitude);
            (loc[0], loc[1], loc[1] + measured_height, loc[2])
        };

        let rig = pool.rigs.entry(anim_id).or_insert_with(|| {
            let arrow_mat = materials.add(StandardMaterial {
                base_color: Color::srgb(0.98, 0.80, 0.08),
                emissive: LinearRgba::rgb(0.45, 0.36, 0.04),
                metallic: 0.14,
                perceptual_roughness: 0.34,
                ..default()
            });
            let ring_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.98, 0.80, 0.08, 0.72),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let arrow_entity = commands
                .spawn((
                    Mesh3d(arrow_mesh.clone()),
                    MeshMaterial3d(arrow_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    PointerArrowFxEntity,
                ))
                .id();
            let ring_entity = commands
                .spawn((
                    Mesh3d(ring_mesh.clone()),
                    MeshMaterial3d(ring_mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    PointerArrowFxEntity,
                ))
                .id();
            PointerArrowRig {
                arrow_entity,
                arrow_mat,
                ring_entity,
                ring_mat,
            }
        });

        let arrow_col = parse_hex_color(&cfg.arrow_color_hex, 1.0);
        let arrow_srgb = arrow_col.to_srgba();
        let arrow_lin = arrow_col.to_linear();
        if let Some(mut mat) = materials.get_mut(&rig.arrow_mat) {
            mat.base_color = arrow_col;
            mat.emissive = LinearRgba::rgb(
                arrow_lin.red * 0.55,
                arrow_lin.green * 0.55,
                arrow_lin.blue * 0.55,
            );
        }

        let scale = cfg.arrow_scale as f32;
        let bounce_y = cfg.bounce_offset_m(anim_time) as f32;
        let tip_y = top_y + (cfg.height_above_m as f32) + bounce_y;
        let spin_rad = ((anim_time * cfg.spin_speed_deg) as f32).to_radians();

        if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.arrow_entity) {
            tf.translation = Vec3::new(anchor_x, tip_y, anchor_z);
            tf.rotation = Quat::from_rotation_y(spin_rad);
            tf.scale = Vec3::splat(scale);
            *vis = Visibility::Visible;
        }

        // Optional pulsing highlight ring around the marker base
        if cfg.show_ground_ring {
            let pulse = 1.0 + 0.14 * ((anim_time as f32) * 3.8).sin();
            let alpha = 0.55 + 0.25 * ((anim_time as f32) * 3.8).cos();
            if let Some(mut rmat) = materials.get_mut(&rig.ring_mat) {
                rmat.base_color = Color::srgba(
                    arrow_srgb.red,
                    arrow_srgb.green,
                    arrow_srgb.blue,
                    alpha,
                );
            }
            if let Ok((mut tf, mut vis)) = tf_q.get_mut(rig.ring_entity) {
                tf.translation = Vec3::new(anchor_x, base_y.max(0.0) + 0.08, anchor_z);
                tf.rotation = flat_rot;
                tf.scale = Vec3::splat(footprint_r * pulse);
                *vis = Visibility::Visible;
            }
        } else if let Ok((_, mut vis)) = tf_q.get_mut(rig.ring_entity) {
            *vis = Visibility::Hidden;
        }

        // Publish optional floating text label synchronized with the bouncing 3D arrow top
        if let Some(text) = cfg.effective_label() {
            let label_y = tip_y + ARROW_UNIT_HEIGHT * scale + (cfg.text_offset_m as f32);
            pool.active_labels.push(ActivePointerLabel {
                world_pos: Vec3::new(anchor_x, label_y, anchor_z),
                text: text.to_string(),
                font_size: cfg.font_size as f32,
                text_color_hex: cfg.text_color_hex.clone(),
                accent_color_hex: cfg.arrow_color_hex.clone(),
                show_bg: cfg.show_text_bg,
            });
        }

        // Subtle vertical alignment guide line in Editor preview mode
        if is_editor_preview {
            let guide_col = Color::srgba(arrow_srgb.red, arrow_srgb.green, arrow_srgb.blue, 0.55);
            gizmos.line(
                Vec3::new(anchor_x, top_y, anchor_z),
                Vec3::new(anchor_x, tip_y, anchor_z),
                guide_col,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_downward_arrow_mesh_is_valid_with_tip_at_origin() {
        let mesh = build_3d_downward_arrow_mesh(16);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("arrow mesh must contain Float32x3 positions");
        };
        assert!(!pos.is_empty());
        let min_y = pos.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let max_y = pos.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        assert!(min_y.abs() < 1e-5, "tip must sit at y=0, got {min_y}");
        assert!(
            (max_y - ARROW_UNIT_HEIGHT).abs() < 1e-4,
            "top cap must sit at ARROW_UNIT_HEIGHT, got {max_y}"
        );
    }
}
