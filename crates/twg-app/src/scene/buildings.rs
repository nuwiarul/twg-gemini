use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use twg_shared::{geo::GeoAnchor, models::Building};
use uuid::Uuid;

use crate::{
    scene::parse_hex_color,
    state::{AppScreen, AppState, ToolMode},
};

#[derive(Component)]
pub struct TacticalBuildingEntity {
    pub id: Uuid,
    pub sig: u64,
}

fn building_signature(b: &Building, anchor: &GeoAnchor) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    b.id.hash(&mut hasher);
    b.color.hash(&mut hasher);
    b.height.to_bits().hash(&mut hasher);
    anchor.center_lng.to_bits().hash(&mut hasher);
    anchor.center_lat.to_bits().hash(&mut hasher);
    for pt in &b.geom {
        pt[0].to_bits().hash(&mut hasher);
        pt[1].to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

pub fn sync_buildings_system(
    mut commands: Commands,
    state: Res<AppState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut existing_q: Query<(Entity, &TacticalBuildingEntity, &mut Visibility)>,
) {
    if state.screen != AppScreen::PlotSkenario {
        for (entity, _, _) in &existing_q {
            commands.entity(entity).despawn();
        }
        return;
    }

    let wanted: HashMap<Uuid, (&Building, u64)> = state
        .buildings
        .iter()
        .map(|b| (b.id, (b, building_signature(b, &state.geo_anchor))))
        .collect();

    let mut present: HashSet<Uuid> = HashSet::new();
    for (entity, comp, mut vis) in &mut existing_q {
        if let Some((_, sig)) = wanted.get(&comp.id)
            && *sig == comp.sig
        {
            present.insert(comp.id);
            *vis = if state.hidden_entity_ids.contains(&comp.id) {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        } else {
            commands.entity(entity).despawn();
        }
    }

    for (id, (b, sig)) in wanted {
        if present.contains(&id) {
            continue;
        }
        if let Some(mesh) = build_extruded_building_mesh(b, &state.geo_anchor) {
            let mesh_handle = meshes.add(mesh);
            let mat_handle = materials.add(StandardMaterial {
                base_color: parse_hex_color(&b.color, 0.92),
                perceptual_roughness: 0.65,
                metallic: 0.1,
                ..default()
            });
            let vis = if state.hidden_entity_ids.contains(&id) {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
            commands.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat_handle),
                Transform::IDENTITY,
                vis,
                TacticalBuildingEntity { id, sig },
            ));
        }
    }
}

pub fn draw_building_draft_gizmos(state: Res<AppState>, mut gizmos: Gizmos) {
    if state.screen != AppScreen::PlotSkenario
        || !matches!(
            state.tool_mode,
            ToolMode::DrawBuilding | ToolMode::DrawBasicPolygon
        )
    {
        return;
    }
    if state.draft_points_lnglat.is_empty() {
        return;
    }

    let color = Color::srgb(0.22, 0.85, 0.98);
    let pts: Vec<Vec3> = state
        .draft_points_lnglat
        .iter()
        .map(|p| {
            let l = state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.3);
            Vec3::from_array(l)
        })
        .collect();

    for p in &pts {
        gizmos.sphere(Isometry3d::from_translation(*p), 0.85, color);
    }
    for w in pts.windows(2) {
        gizmos.line(w[0], w[1], color);
    }
    // Live rubber-band preview to cursor position & closing edge back to point 1
    if let Some([clng, clat]) = state.cursor_lnglat {
        let cur = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.3));
        if let Some(last) = pts.last() {
            gizmos.line(*last, cur, Color::srgba(0.22, 0.95, 0.75, 0.85));
        }
        if pts.len() >= 2 {
            gizmos.line(cur, pts[0], Color::srgba(0.65, 0.55, 0.98, 0.55));
        }
    }
    if pts.len() >= 3 {
        gizmos.line(*pts.last().unwrap(), pts[0], Color::srgba(0.22, 0.85, 0.98, 0.55));
        gizmos.circle(
            Isometry3d::new(pts[0], Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            1.6,
            Color::srgb(0.20, 0.96, 0.62),
        );
    }
}

fn build_extruded_building_mesh(b: &Building, anchor: &GeoAnchor) -> Option<Mesh> {
    if b.geom.len() < 3 {
        return None;
    }

    let mut ring: Vec<[f32; 2]> = b
        .geom
        .iter()
        .map(|pt| {
            let l = anchor.lng_lat_alt_to_local(pt[0], pt[1], 0.0);
            [l[0], l[2]]
        })
        .collect();

    // Remove closing duplicate vertex if present
    if ring.len() > 3 {
        let first = ring[0];
        let last = *ring.last().unwrap();
        if (first[0] - last[0]).abs() < 1e-3 && (first[1] - last[1]).abs() < 1e-3 {
            ring.pop();
        }
    }
    if ring.len() < 3 {
        return None;
    }

    // Ensure consistent winding in (x, z) plane so roof triangles face +Y
    let signed_area_xz: f32 = ring
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let b_pt = ring[(i + 1) % ring.len()];
            a[0] * b_pt[1] - b_pt[0] * a[1]
        })
        .sum();
    // In XZ with +Y up: (x1 - x0, z1 - z0) x (x2 - x0, z2 - z0) gives -Y when CCW in (x,z),
    // so clockwise in (x, z) gives +Y upward cross product!
    if signed_area_xz > 0.0 {
        ring.reverse();
    }

    let flat_2d: Vec<f64> = ring
        .iter()
        .flat_map(|p| [p[0] as f64, p[1] as f64])
        .collect();
    let roof_tris = earcutr::earcut(&flat_2d, &[], 2).ok()?;
    if roof_tris.len() < 3 {
        return None;
    }

    let h = (b.height as f32).max(1.0);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Roof cap (flat upward normal +Y)
    let roof_base = positions.len() as u32;
    for p in &ring {
        positions.push([p[0], h, p[1]]);
        normals.push([0.0, 1.0, 0.0]);
    }
    for tri in roof_tris.as_chunks::<3>().0 {
        let p0 = Vec3::from_array(positions[roof_base as usize + tri[0]]);
        let p1 = Vec3::from_array(positions[roof_base as usize + tri[1]]);
        let p2 = Vec3::from_array(positions[roof_base as usize + tri[2]]);
        let cross = (p1 - p0).cross(p2 - p0);
        if cross.y >= 0.0 {
            indices.extend_from_slice(&[
                roof_base + tri[0] as u32,
                roof_base + tri[1] as u32,
                roof_base + tri[2] as u32,
            ]);
        } else {
            indices.extend_from_slice(&[
                roof_base + tri[0] as u32,
                roof_base + tri[2] as u32,
                roof_base + tri[1] as u32,
            ]);
        }
    }

    // Side walls (4 vertices per wall quad so each wall has sharp outward normal `edge x +Y`)
    for i in 0..ring.len() {
        let a = ring[i];
        let c = ring[(i + 1) % ring.len()];
        let edge = Vec3::new(c[0] - a[0], 0.0, c[1] - a[1]);
        let normal_vec = edge.cross(Vec3::Y).normalize_or_zero();
        let n = [normal_vec.x, normal_vec.y, normal_vec.z];

        let base = positions.len() as u32;
        positions.push([a[0], 0.0, a[1]]);
        positions.push([c[0], 0.0, c[1]]);
        positions.push([c[0], h, c[1]]);
        positions.push([a[0], h, a[1]]);
        normals.extend_from_slice(&[n, n, n, n]);

        // Test winding against outward normal
        let v0 = Vec3::from_array(positions[base as usize]);
        let v1 = Vec3::from_array(positions[base as usize + 1]);
        let v2 = Vec3::from_array(positions[base as usize + 2]);
        if (v1 - v0).cross(v2 - v0).dot(normal_vec) >= 0.0 {
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}
