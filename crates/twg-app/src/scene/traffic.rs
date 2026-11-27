use std::collections::{HashMap, HashSet};

use bevy::{camera::primitives::Aabb, prelude::*};
use twg_shared::{
    group_anim::collect_group_descendant_ids,
    traffic_anim::{
        TrafficLightColor, TrafficLightVisualPose, TrafficSignalLiveStatus, TrafficSimEngine,
        TrafficVehiclePose, is_traffic_flow_or_signal, is_traffic_signal_anim,
        parse_traffic_anim_config, resolve_traffic_light_visual_poses,
    },
    traffic_diversion::{
        TrafficDiversionSimEngine, is_traffic_diversion_anim, parse_traffic_diversion_config,
    },
    traffic_queue::{
        TrafficQueueSimEngine, is_traffic_queue_anim, parse_traffic_queue_config,
    },
};
use uuid::Uuid;

use crate::state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, TrafficPreviewLayout};

/// Computes road-aligned `(width_m, height_m, length_m)` of a Traffic pool model using its
/// measured unscaled 3D mesh extents (`traffic_model_unit_dims`), accounting for `scale` and `rot_y_offset_deg`.
pub fn measured_vehicle_scaled_dims(
    state: &AppState,
    model_path: &str,
    scale: f32,
    rot_y_offset_deg: f64,
) -> Option<[f32; 3]> {
    let [uw, uh, ul] = *state.traffic_model_unit_dims.get(model_path)?;
    let rad = (rot_y_offset_deg as f32).to_radians();
    let c = rad.cos().abs();
    let s = rad.sin().abs();
    let scaled_w = (uw * c + ul * s) * scale.max(0.01);
    let scaled_h = uh * scale.max(0.01);
    let scaled_l = (ul * c + uw * s) * scale.max(0.01);
    Some([scaled_w, scaled_h, scaled_l])
}

/// World pose of one Traffic pool vehicle placed directly on the map for
/// interactive scale & orientation calibration in Plot Editor (up to 2 models).
#[derive(Debug, Clone)]
pub struct TrafficScalePreviewPose {
    pub anim_id: Uuid,
    pub pool_idx: usize,
    pub is_active_gizmo: bool,
    pub model_path: String,
    pub world_pos: Vec3,
    pub lng: f64,
    pub lat: f64,
    pub alt: f64,
    pub road_yaw_deg: f32,
    pub model_yaw_deg: f32,
    pub scale: f32,
    /// Actual measured 3D mesh width (if loaded) or lane-derived fallback width.
    pub visual_width_m: f32,
    /// Actual measured 3D mesh length (if loaded) or configured `length_m`.
    pub visual_length_m: f32,
}

/// Samples a local 3D point and road yaw (glTF `+Z` forward, degrees) at arc-length `dist_m` along a lane.
/// Extrapolates straight past the end of short lanes so preview vehicles never pile up at the endpoint.
fn sample_lane_local_at_distance(
    state: &AppState,
    geom: &[[f64; 3]],
    dist_m: f32,
) -> Option<(Vec3, f32)> {
    if geom.len() < 2 {
        return None;
    }
    let pts: Vec<Vec3> = geom
        .iter()
        .map(|p| Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], p[2])))
        .collect();
    let mut seg_lens = Vec::with_capacity(pts.len() - 1);
    let mut total_len = 0.0_f32;
    for w in pts.windows(2) {
        let l = Vec2::new(w[1].x - w[0].x, w[1].z - w[0].z).length().max(0.01);
        seg_lens.push(l);
        total_len += l;
    }
    if total_len <= 0.01 {
        return None;
    }
    if dist_m > total_len {
        let p0 = pts[pts.len() - 2];
        let p1 = pts[pts.len() - 1];
        let seg = p1 - p0;
        let len = Vec2::new(seg.x, seg.z).length().max(0.01);
        let dir = Vec3::new(seg.x / len, 0.0, seg.z / len);
        let pos = p1 + dir * (dist_m - total_len);
        let yaw_deg = dir.x.atan2(dir.z).to_degrees();
        return Some((pos, yaw_deg));
    }
    let target = dist_m.max(0.0);
    let mut acc = 0.0_f32;
    for (idx, &seg_len) in seg_lens.iter().enumerate() {
        if acc + seg_len >= target || idx == seg_lens.len() - 1 {
            let t = ((target - acc) / seg_len).clamp(0.0, 1.0);
            let p0 = pts[idx];
            let p1 = pts[idx + 1];
            let pos = p0.lerp(p1, t);
            let seg = p1 - p0;
            let yaw_deg = seg.x.atan2(seg.z).to_degrees();
            return Some((pos, yaw_deg));
        }
        acc += seg_len;
    }
    None
}

/// Resolves up to 2 on-map preview poses placed either Front-to-Back or Side-by-Side
/// using real measured 3D mesh extents (`traffic_model_unit_dims`) + user `traffic_preview_gap_m`
/// so vehicles never overlap regardless of scale!
pub fn resolve_traffic_scale_preview_poses(state: &AppState) -> Vec<TrafficScalePreviewPose> {
    let Some(ref preview) = state.traffic_scale_preview else {
        return Vec::new();
    };
    if state.selected != SelectedEntity::Anim(preview.anim_id) || preview.pool_indices.is_empty() {
        return Vec::new();
    }
    let Some(anim) = state.anims.iter().find(|a| {
        a.id == preview.anim_id
            && (is_traffic_flow_or_signal(a)
                || is_traffic_queue_anim(a)
                || is_traffic_diversion_anim(a))
    }) else {
        return Vec::new();
    };
    let (lane_geom, vehicle_pool, lane_w, side_sign): (
        Vec<[f64; 3]>,
        Vec<twg_shared::traffic_anim::TrafficVehicleEntry>,
        f32,
        f32,
    ) = if is_traffic_diversion_anim(anim) {
        let cfg = parse_traffic_diversion_config(anim);
        let Some(cor) = cfg
            .corridors
            .iter()
            .find(|c| c.enabled && c.approach_geom.len() >= 2)
            .or_else(|| cfg.corridors.iter().find(|c| c.approach_geom.len() >= 2))
        else {
            return Vec::new();
        };
        (cor.approach_geom.clone(), cfg.vehicle_pool, 3.6_f32, 1.0_f32)
    } else if is_traffic_queue_anim(anim) {
        let cfg = parse_traffic_queue_config(anim);
        let Some(cor) = cfg
            .corridors
            .iter()
            .find(|c| c.enabled && c.queue_geom.len() >= 2)
            .or_else(|| cfg.corridors.iter().find(|c| c.queue_geom.len() >= 2))
        else {
            return Vec::new();
        };
        (cor.queue_geom.clone(), cfg.vehicle_pool, 3.6_f32, 1.0_f32)
    } else {
        let cfg = parse_traffic_anim_config(anim);
        let Some(lane) = cfg
            .lanes
            .iter()
            .find(|l| l.enabled && l.geom.len() >= 2)
            .or_else(|| cfg.lanes.iter().find(|l| l.geom.len() >= 2))
        else {
            return Vec::new();
        };
        (
            lane.geom.clone(),
            cfg.vehicle_pool,
            cfg.lane_width_m as f32,
            cfg.overtake_side.lateral_sign() as f32,
        )
    };

    let valid_indices: Vec<usize> = preview
        .pool_indices
        .iter()
        .copied()
        .filter(|&i| i < vehicle_pool.len())
        .take(2)
        .collect();
    if valid_indices.is_empty() {
        return Vec::new();
    }

    let gap_m = (state.traffic_preview_gap_m as f32).clamp(0.0, 50.0);

    // Resolve actual 3D width & length for each preview slot (fallback to configured length if still loading)
    let dims: Vec<(f32, f32)> = valid_indices
        .iter()
        .map(|&pi| {
            let e = &vehicle_pool[pi];
            if let Some([mw, _mh, ml]) =
                measured_vehicle_scaled_dims(state, &e.model_path, e.scale as f32, e.rot_y_offset_deg)
            {
                (mw.max(0.8), ml.max(1.2))
            } else {
                ((lane_w * 0.65).clamp(1.6, 3.2), (e.length_m as f32).max(1.5))
            }
        })
        .collect();

    // Compute longitudinal distance `s` and lateral offset `lat` for each slot
    let placements: Vec<(f32, f32)> = if valid_indices.len() == 1 {
        vec![(8.0, 0.0)]
    } else {
        let (w0, l0) = dims[0];
        let (w1, l1) = dims[1];
        match state.traffic_preview_layout {
            TrafficPreviewLayout::FrontBack => {
                // Slot 0 ahead, Slot 1 directly behind with exact clear bumper-to-bumper `gap_m`!
                let spacing = 0.5 * (l0 + l1) + gap_m;
                let s_back = (0.5 * l1 + 3.0).max(5.0);
                let s_front = s_back + spacing;
                vec![(s_front, 0.0), (s_back, 0.0)]
            }
            TrafficPreviewLayout::SideBySide => {
                // Both aligned longitudinally at the same `s`, separated cleanly left/right by `gap_m`!
                let s = (0.5 * l0.max(l1) + 4.0).max(8.0);
                let lat_spacing = 0.5 * (w0 + w1) + gap_m;
                vec![(s, 0.0), (s, side_sign * lat_spacing)]
            }
        }
    };

    let mut out = Vec::with_capacity(valid_indices.len());
    for (slot_i, &pool_idx) in valid_indices.iter().enumerate() {
        let entry = &vehicle_pool[pool_idx];
        let (dist_s, lat_off) = placements[slot_i];
        let Some((base_pos, road_yaw_deg)) =
            sample_lane_local_at_distance(state, &lane_geom, dist_s)
        else {
            continue;
        };
        let yaw_rad = road_yaw_deg.to_radians();
        let fwd = Vec3::new(yaw_rad.sin(), 0.0, yaw_rad.cos());
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        let world_pos = base_pos + right * lat_off;

        let (lng, lat, alt) = state
            .geo_anchor
            .local_to_lng_lat_alt([world_pos.x, world_pos.y, world_pos.z]);
        let model_yaw_deg = road_yaw_deg + entry.rot_y_offset_deg as f32;
        let (vis_w, vis_l) = dims[slot_i];
        out.push(TrafficScalePreviewPose {
            anim_id: preview.anim_id,
            pool_idx,
            is_active_gizmo: pool_idx == preview.active_gizmo_pool_idx,
            model_path: entry.model_path.clone(),
            world_pos,
            lng,
            lat,
            alt,
            road_yaw_deg,
            model_yaw_deg,
            scale: entry.scale as f32,
            visual_width_m: vis_w,
            visual_length_m: vis_l,
        });
    }
    out
}

/// Resolves the single preview pose that currently owns the interactive 3D Scale Gizmo.
pub fn resolve_active_traffic_scale_gizmo_pose(
    state: &AppState,
) -> Option<TrafficScalePreviewPose> {
    let poses = resolve_traffic_scale_preview_poses(state);
    poses
        .iter()
        .find(|p| p.is_active_gizmo)
        .cloned()
        .or_else(|| poses.into_iter().next())
}

/// World pose of a selected 3D Traffic Light currently owning the Move/Rotate/Scale Gizmo.
#[derive(Debug, Clone)]
pub struct TrafficLightGizmoPose {
    pub anim_id: Uuid,
    pub light_id: Uuid,
    pub world_pos: Vec3,
    pub lng: f64,
    pub lat: f64,
    pub alt: f64,
    pub rot_deg: [f64; 3],
    pub scale: [f64; 3],
}

/// Resolves the currently selected 3D Traffic Light for interactive Move / Rotate / Scale Gizmo editing.
pub fn resolve_active_traffic_light_gizmo_pose(state: &AppState) -> Option<TrafficLightGizmoPose> {
    let (anim_id, light_id) = state.selected_traffic_light?;
    if state.selected != SelectedEntity::Anim(anim_id)
        || state.locked_entity_ids.contains(&anim_id)
        || state.hidden_entity_ids.contains(&anim_id)
    {
        return None;
    }
    let anim = state
        .anims
        .iter()
        .find(|a| a.id == anim_id && is_traffic_signal_anim(a))?;
    let cfg = parse_traffic_anim_config(anim);
    let tl = cfg.traffic_lights.iter().find(|l| l.id == light_id)?;
    let world_pos = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(
        tl.pos_lnglat[0],
        tl.pos_lnglat[1],
        tl.pos_lnglat[2],
    ));
    Some(TrafficLightGizmoPose {
        anim_id,
        light_id,
        world_pos,
        lng: tl.pos_lnglat[0],
        lat: tl.pos_lnglat[1],
        alt: tl.pos_lnglat[2],
        rot_deg: tl.rot_deg,
        scale: tl.scale,
    })
}

fn dist_pt_seg_2d(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let lsq = ab.length_squared();
    if lsq < 1e-5 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / lsq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Screen-space hit-test against all visible, unlocked 3D Traffic Light poles on the map.
pub fn hit_test_traffic_light(
    cursor_px: Vec2,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<(Uuid, Uuid)> {
    let mut best: Option<((Uuid, Uuid), f32)> = None;
    for anim in &state.anims {
        if !is_traffic_signal_anim(anim)
            || state.hidden_entity_ids.contains(&anim.id)
            || state.locked_entity_ids.contains(&anim.id)
        {
            continue;
        }
        let cfg = parse_traffic_anim_config(anim);
        for tl in &cfg.traffic_lights {
            let base = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(
                tl.pos_lnglat[0],
                tl.pos_lnglat[1],
                tl.pos_lnglat[2],
            ));
            let rot = Quat::from_euler(
                EulerRot::YXZ,
                (tl.rot_deg[1] as f32).to_radians(),
                (tl.rot_deg[0] as f32).to_radians(),
                (tl.rot_deg[2] as f32).to_radians(),
            );
            let top = base + rot * Vec3::new(0.0, 5.6 * (tl.scale[1] as f32).max(0.25), 0.0);
            let Ok(a2) = camera.world_to_viewport(cam_gtf, base) else {
                continue;
            };
            let b2 = camera.world_to_viewport(cam_gtf, top).unwrap_or(a2);
            let d = dist_pt_seg_2d(cursor_px, a2, b2);
            if d <= 16.0 && best.is_none_or(|cur| d < cur.1) {
                best = Some(((anim.id, tl.id), d));
            }
        }
    }
    best.map(|(ids, _)| ids)
}

#[derive(Component)]
pub struct TrafficVehicleEntity;

#[derive(Component)]
pub struct TrafficLightVisualEntity;

#[derive(Component)]
pub struct TrafficLightLampPart;

struct TrafficLightAssets {
    base_mesh: Handle<Mesh>,
    collar_mesh: Handle<Mesh>,
    pole_mesh: Handle<Mesh>,
    housing_mesh: Handle<Mesh>,
    cap_mesh: Handle<Mesh>,
    visor_mesh: Handle<Mesh>,
    lens_mesh: Handle<Mesh>,
    beacon_mesh: Handle<Mesh>,
    stop_line_mesh: Handle<Mesh>,
    pole_mat: Handle<StandardMaterial>,
    collar_mat: Handle<StandardMaterial>,
    housing_mat: Handle<StandardMaterial>,
    red_on: Handle<StandardMaterial>,
    red_off: Handle<StandardMaterial>,
    yellow_on: Handle<StandardMaterial>,
    yellow_off: Handle<StandardMaterial>,
    green_on: Handle<StandardMaterial>,
    green_off: Handle<StandardMaterial>,
    stop_line_mat: Handle<StandardMaterial>,
}

struct TrafficLightVisualSlot {
    root: Entity,
    red_lamp: Entity,
    yellow_lamp: Entity,
    green_lamp: Entity,
    beacon_lamp: Entity,
    stop_line: Entity,
    last_color: Option<TrafficLightColor>,
}

struct PooledVehicleSlot {
    entity: Entity,
    model_path: String,
    in_use_by: Option<(Uuid, u64)>,
}

struct ActiveTrafficSim {
    config_sig: u64,
    engine: TrafficSimEngine,
}

struct ActiveTrafficQueueSim {
    config_sig: u64,
    engine: TrafficQueueSimEngine,
}

struct ActiveTrafficDiversionSim {
    config_sig: u64,
    engine: TrafficDiversionSimEngine,
}

/// Reusable Bevy entity & scene-handle pool for all active Traffic simulations
/// (`traffic_flow`, `traffic_queue`, `traffic_gridlock`, `traffic_manual`, `traffic_diversion`).
/// Vehicles recycle idle `Entity` slots (`Visibility::Hidden` <-> `Visible`) instead of
/// spawning/despawning glTF hierarchies every cycle, keeping frame times smooth.
#[derive(Resource, Default)]
pub struct TrafficVisualPool {
    sims: HashMap<Uuid, ActiveTrafficSim>,
    queue_sims: HashMap<Uuid, ActiveTrafficQueueSim>,
    diversion_sims: HashMap<Uuid, ActiveTrafficDiversionSim>,
    scene_handles: HashMap<String, Handle<WorldAsset>>,
    fallback_body_mesh: Option<Handle<Mesh>>,
    fallback_cabin_mesh: Option<Handle<Mesh>>,
    fallback_mat: Option<Handle<StandardMaterial>>,
    slots: Vec<PooledVehicleSlot>,
    light_assets: Option<TrafficLightAssets>,
    light_slots: HashMap<(Uuid, Uuid), TrafficLightVisualSlot>,
}

impl TrafficVisualPool {
    fn get_or_load_scene(
        &mut self,
        model_path: &str,
        asset_server: &AssetServer,
    ) -> Handle<WorldAsset> {
        self.scene_handles
            .entry(model_path.to_string())
            .or_insert_with(|| {
                asset_server.load(GltfAssetLabel::Scene(0).from_asset(model_path.to_string()))
            })
            .clone()
    }

    fn ensure_fallback_assets(
        &mut self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> (Handle<Mesh>, Handle<Mesh>, Handle<StandardMaterial>) {
        let body = self
            .fallback_body_mesh
            .get_or_insert_with(|| meshes.add(Cuboid::new(1.85, 0.9, 4.2)))
            .clone();
        let cabin = self
            .fallback_cabin_mesh
            .get_or_insert_with(|| meshes.add(Cuboid::new(1.55, 0.65, 2.1)))
            .clone();
        let mat = self
            .fallback_mat
            .get_or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb(0.22, 0.62, 0.94),
                    metallic: 0.3,
                    perceptual_roughness: 0.45,
                    ..default()
                })
            })
            .clone();
        (body, cabin, mat)
    }

    fn spawn_pooled_slot(
        &mut self,
        commands: &mut Commands,
        asset_server: &AssetServer,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        state: &AppState,
        model_path: &str,
    ) -> usize {
        let has_glb = !model_path.is_empty()
            && (state.available_model_paths.contains(model_path)
                || state.assets_root.join(model_path).exists());

        let parent = commands
            .spawn((
                Transform::default(),
                Visibility::Hidden,
                TrafficVehicleEntity,
            ))
            .id();

        if has_glb {
            let scene_handle = self.get_or_load_scene(model_path, asset_server);
            let child = commands
                .spawn((
                    WorldAssetRoot(scene_handle),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                ))
                .id();
            commands.entity(parent).add_child(child);
        } else {
            let (body, cabin, mat) = self.ensure_fallback_assets(meshes, materials);
            commands.entity(parent).with_children(|child| {
                child.spawn((
                    Mesh3d(body),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_xyz(0.0, 0.55, 0.0),
                ));
                child.spawn((
                    Mesh3d(cabin),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(0.0, 1.25, -0.25),
                ));
            });
        }

        let idx = self.slots.len();
        self.slots.push(PooledVehicleSlot {
            entity: parent,
            model_path: model_path.to_string(),
            in_use_by: None,
        });
        idx
    }

    fn ensure_light_assets(
        &mut self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) {
        if self.light_assets.is_some() {
            return;
        }
        self.light_assets = Some(TrafficLightAssets {
            base_mesh: meshes.add(Cylinder::new(0.28, 0.30)),
            collar_mesh: meshes.add(Cylinder::new(0.145, 0.72)),
            pole_mesh: meshes.add(Cylinder::new(0.11, 4.45)),
            housing_mesh: meshes.add(Cuboid::new(0.62, 1.72, 0.44)),
            cap_mesh: meshes.add(Cuboid::new(0.68, 0.08, 0.50)),
            visor_mesh: meshes.add(Cuboid::new(0.46, 0.055, 0.28)),
            lens_mesh: meshes.add(Sphere::new(0.175)),
            beacon_mesh: meshes.add(Sphere::new(0.20)),
            stop_line_mesh: meshes.add(Cuboid::new(1.0, 0.05, 0.42)),
            pole_mat: materials.add(StandardMaterial {
                base_color: Color::srgb(0.22, 0.25, 0.30),
                metallic: 0.65,
                perceptual_roughness: 0.38,
                ..default()
            }),
            collar_mat: materials.add(StandardMaterial {
                base_color: Color::srgb(0.96, 0.76, 0.12),
                metallic: 0.15,
                perceptual_roughness: 0.45,
                ..default()
            }),
            housing_mat: materials.add(StandardMaterial {
                base_color: Color::srgb(0.08, 0.09, 0.12),
                metallic: 0.3,
                perceptual_roughness: 0.55,
                ..default()
            }),
            red_on: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.18, 0.14),
                emissive: LinearRgba::rgb(9.5, 0.45, 0.28),
                perceptual_roughness: 0.2,
                ..default()
            }),
            red_off: materials.add(StandardMaterial {
                base_color: Color::srgb(0.22, 0.05, 0.05),
                perceptual_roughness: 0.6,
                ..default()
            }),
            yellow_on: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.84, 0.12),
                emissive: LinearRgba::rgb(9.0, 6.2, 0.25),
                perceptual_roughness: 0.2,
                ..default()
            }),
            yellow_off: materials.add(StandardMaterial {
                base_color: Color::srgb(0.22, 0.17, 0.04),
                perceptual_roughness: 0.6,
                ..default()
            }),
            green_on: materials.add(StandardMaterial {
                base_color: Color::srgb(0.14, 1.0, 0.36),
                emissive: LinearRgba::rgb(0.32, 9.2, 1.5),
                perceptual_roughness: 0.2,
                ..default()
            }),
            green_off: materials.add(StandardMaterial {
                base_color: Color::srgb(0.04, 0.19, 0.07),
                perceptual_roughness: 0.6,
                ..default()
            }),
            stop_line_mat: materials.add(StandardMaterial {
                base_color: Color::srgb(0.96, 0.97, 0.99),
                emissive: LinearRgba::rgb(0.42, 0.44, 0.48),
                perceptual_roughness: 0.85,
                ..default()
            }),
        });
    }

    fn spawn_traffic_light_slot(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        key: (Uuid, Uuid),
    ) {
        self.ensure_light_assets(meshes, materials);
        let a = self.light_assets.as_ref().unwrap();

        let root = commands
            .spawn((
                Transform::default(),
                Visibility::Visible,
                TrafficLightVisualEntity,
            ))
            .id();

        let mut red_lamp = Entity::PLACEHOLDER;
        let mut yellow_lamp = Entity::PLACEHOLDER;
        let mut green_lamp = Entity::PLACEHOLDER;
        let mut beacon_lamp = Entity::PLACEHOLDER;

        commands.entity(root).with_children(|child| {
            // Base plinth
            child.spawn((
                Mesh3d(a.base_mesh.clone()),
                MeshMaterial3d(a.pole_mat.clone()),
                Transform::from_xyz(0.0, 0.15, 0.0),
            ));
            // Yellow reflective collar near pole base
            child.spawn((
                Mesh3d(a.collar_mesh.clone()),
                MeshMaterial3d(a.collar_mat.clone()),
                Transform::from_xyz(0.0, 0.72, 0.0),
            ));
            // Main upright pole
            child.spawn((
                Mesh3d(a.pole_mesh.clone()),
                MeshMaterial3d(a.pole_mat.clone()),
                Transform::from_xyz(0.0, 2.40, 0.0),
            ));
            // Signal head housing box
            child.spawn((
                Mesh3d(a.housing_mesh.clone()),
                MeshMaterial3d(a.housing_mat.clone()),
                Transform::from_xyz(0.0, 4.46, 0.06),
            ));
            // Top cap trim
            child.spawn((
                Mesh3d(a.cap_mesh.clone()),
                MeshMaterial3d(a.collar_mat.clone()),
                Transform::from_xyz(0.0, 5.36, 0.08),
            ));
            // 3 Visors above Red / Yellow / Green lenses (facing local +Z)
            for vy in [5.14_f32, 4.64, 4.14] {
                child.spawn((
                    Mesh3d(a.visor_mesh.clone()),
                    MeshMaterial3d(a.housing_mat.clone()),
                    Transform::from_xyz(0.0, vy, 0.34),
                ));
            }
            // Red lens (top)
            red_lamp = child
                .spawn((
                    Mesh3d(a.lens_mesh.clone()),
                    MeshMaterial3d(a.red_on.clone()),
                    Transform::from_xyz(0.0, 4.96, 0.25).with_scale(Vec3::new(1.0, 1.0, 0.45)),
                    TrafficLightLampPart,
                ))
                .id();
            // Yellow lens (middle)
            yellow_lamp = child
                .spawn((
                    Mesh3d(a.lens_mesh.clone()),
                    MeshMaterial3d(a.yellow_off.clone()),
                    Transform::from_xyz(0.0, 4.46, 0.25).with_scale(Vec3::new(1.0, 1.0, 0.45)),
                    TrafficLightLampPart,
                ))
                .id();
            // Green lens (bottom)
            green_lamp = child
                .spawn((
                    Mesh3d(a.lens_mesh.clone()),
                    MeshMaterial3d(a.green_off.clone()),
                    Transform::from_xyz(0.0, 3.96, 0.25).with_scale(Vec3::new(1.0, 1.0, 0.45)),
                    TrafficLightLampPart,
                ))
                .id();
            // 360-degree top beacon orb for high-angle / top-down tactical visibility
            beacon_lamp = child
                .spawn((
                    Mesh3d(a.beacon_mesh.clone()),
                    MeshMaterial3d(a.red_on.clone()),
                    Transform::from_xyz(0.0, 5.62, 0.06),
                    TrafficLightLampPart,
                ))
                .id();
        });

        let stop_line = commands
            .spawn((
                Mesh3d(a.stop_line_mesh.clone()),
                MeshMaterial3d(a.stop_line_mat.clone()),
                Transform::default(),
                Visibility::Visible,
                TrafficLightVisualEntity,
            ))
            .id();

        self.light_slots.insert(
            key,
            TrafficLightVisualSlot {
                root,
                red_lamp,
                yellow_lamp,
                green_lamp,
                beacon_lamp,
                stop_line,
                last_color: None,
            },
        );
    }

    fn clear_all_traffic_lights(&mut self, commands: &mut Commands) {
        for (_, slot) in self.light_slots.drain() {
            commands.entity(slot.root).despawn();
            commands.entity(slot.stop_line).despawn();
        }
    }
}

/// Advances deterministic Traffic simulations, measures unscaled 3D mesh extents for loaded
/// vehicle models, and synchronizes pooled Bevy vehicle entities across Play Skenario,
/// Layar Besar, and Plot Editor Live / Scale Preview.
#[allow(clippy::too_many_arguments)]
pub fn traffic_visual_system(
    mut commands: Commands,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pool: ResMut<TrafficVisualPool>,
    mut tf_vis_q: Query<(&mut Transform, &mut Visibility, &GlobalTransform), With<TrafficVehicleEntity>>,
    mut light_tf_q: Query<
        (&mut Transform, &mut Visibility),
        (With<TrafficLightVisualEntity>, Without<TrafficVehicleEntity>),
    >,
    mut lamp_mat_q: Query<&mut MeshMaterial3d<StandardMaterial>, With<TrafficLightLampPart>>,
    children_q: Query<&Children>,
    mesh_aabb_q: Query<(&GlobalTransform, &Aabb), Without<TrafficVehicleEntity>>,
) {
    // Measure unscaled local 3D mesh extents `[width_x, height_y, length_z]` for any spawned
    // vehicle model not yet recorded in `state.traffic_model_unit_dims`.
    let mut stack: Vec<Entity> = Vec::with_capacity(16);
    for slot in &pool.slots {
        if slot.in_use_by.is_none() || state.traffic_model_unit_dims.contains_key(&slot.model_path)
        {
            continue;
        }
        let Ok((_, vis, root_gtf)) = tf_vis_q.get(slot.entity) else {
            continue;
        };
        if *vis == Visibility::Hidden {
            continue;
        }
        let root_inv = root_gtf.affine().inverse();
        let mut lmin = Vec3::splat(f32::INFINITY);
        let mut lmax = Vec3::splat(f32::NEG_INFINITY);
        let mut found = false;
        stack.clear();
        stack.push(slot.entity);
        while let Some(e) = stack.pop() {
            if let Ok((gtf, aabb)) = mesh_aabb_q.get(e) {
                let c = Vec3::from(aabb.center);
                let h = Vec3::from(aabb.half_extents);
                let local_aff = root_inv * gtf.affine();
                for sx in [-1.0_f32, 1.0] {
                    for sy in [-1.0_f32, 1.0] {
                        for sz in [-1.0_f32, 1.0] {
                            let pt = local_aff
                                .transform_point3(c + Vec3::new(sx * h.x, sy * h.y, sz * h.z));
                            lmin = lmin.min(pt);
                            lmax = lmax.max(pt);
                            found = true;
                        }
                    }
                }
            }
            if let Ok(children) = children_q.get(e) {
                for child in children.iter() {
                    stack.push(child);
                }
            }
        }
        if found {
            let ext = lmax - lmin;
            if ext.x.is_finite()
                && ext.y.is_finite()
                && ext.z.is_finite()
                && ext.x.max(ext.z) > 0.05
            {
                state
                    .traffic_model_unit_dims
                    .insert(slot.model_path.clone(), [ext.x, ext.y, ext.z]);
            }
        }
    }

    if !state.is_3d_map_screen() {
        pool.sims.clear();
        pool.queue_sims.clear();
        pool.diversion_sims.clear();
        pool.clear_all_traffic_lights(&mut commands);
        state.traffic_queue_live_status.clear();
        state.traffic_diversion_live_status.clear();
        state.traffic_signal_live_status.clear();
        for slot in &mut pool.slots {
            slot.in_use_by = None;
            if let Ok((_, mut vis, _)) = tf_vis_q.get_mut(slot.entity) {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }

    let is_editor_preview =
        !playback.play_test_mode && playback.editor_preview_anim_id.is_some();
    let scale_preview_poses = if !playback.play_test_mode && !is_editor_preview {
        resolve_traffic_scale_preview_poses(&state)
    } else {
        Vec::new()
    };

    let mut live_signal_statuses: HashMap<Uuid, TrafficSignalLiveStatus> = HashMap::new();

    if !playback.play_test_mode && !is_editor_preview && scale_preview_poses.is_empty() {
        pool.sims.clear();
        pool.queue_sims.clear();
        pool.diversion_sims.clear();
        state.traffic_queue_live_status.clear();
        state.traffic_diversion_live_status.clear();
        state.traffic_signal_live_status.clear();
        for slot in &mut pool.slots {
            slot.in_use_by = None;
            if let Ok((_, mut vis, _)) = tf_vis_q.get_mut(slot.entity) {
                *vis = Visibility::Hidden;
            }
        }
        // Keep procedural 3D traffic lights visible in Plot Editor even when idle!
        sync_procedural_traffic_lights(
            &mut commands,
            &state,
            &live_signal_statuses,
            &mut meshes,
            &mut materials,
            &mut pool,
            &mut light_tf_q,
            &mut lamp_mat_q,
        );
        return;
    }

    let preview_scope: Option<HashSet<Uuid>> = if is_editor_preview {
        playback.editor_preview_anim_id.map(|pid| {
            let mut set: HashSet<Uuid> = collect_group_descendant_ids(pid, &state.anims)
                .into_iter()
                .collect();
            set.insert(pid);
            set
        })
    } else {
        None
    };

    let mut active_anim_ids: HashSet<Uuid> = HashSet::new();
    let mut active_queue_anim_ids: HashSet<Uuid> = HashSet::new();
    let mut active_diversion_anim_ids: HashSet<Uuid> = HashSet::new();
    let mut completed_queue_anim_ids: Vec<Uuid> = Vec::new();
    let mut live_queue_statuses: HashMap<Uuid, twg_shared::traffic_queue::TrafficQueueLiveStatus> =
        HashMap::new();
    let mut live_diversion_statuses: HashMap<
        Uuid,
        twg_shared::traffic_diversion::TrafficDiversionLiveStatus,
    > = HashMap::new();
    let mut active_poses: Vec<(Uuid, TrafficVehiclePose)> = Vec::new();

    for sp in scale_preview_poses {
        active_poses.push((
            sp.anim_id,
            TrafficVehiclePose {
                vehicle_id: u64::MAX - sp.pool_idx as u64,
                lane_source_idx: 0,
                model_path: sp.model_path,
                x: sp.world_pos.x,
                y: sp.world_pos.y,
                z: sp.world_pos.z,
                rot_y_deg: sp.model_yaw_deg,
                scale: sp.scale,
                fade_scale: 1.0,
                speed_mps: 0.0,
                behavior: twg_shared::traffic_anim::TrafficBehaviorState::Cruising,
            },
        ));
    }

    for anim in &state.anims {
        let is_flow = is_traffic_flow_or_signal(anim);
        let is_signal = is_traffic_signal_anim(anim);
        let is_queue = is_traffic_queue_anim(anim);
        let is_diversion = is_traffic_diversion_anim(anim);
        if (!is_flow && !is_queue && !is_diversion) || state.hidden_entity_ids.contains(&anim.id) {
            continue;
        }
        if let Some(ref scope) = preview_scope
            && !scope.contains(&anim.id)
        {
            continue;
        }

        let Some(track) = playback.tracks.get(&anim.id) else {
            continue;
        };
        if !matches!(
            track.status,
            AnimRunStatus::Playing | AnimRunStatus::Paused | AnimRunStatus::Finished
        ) {
            continue;
        }

        if is_flow {
            let cfg = parse_traffic_anim_config(anim);
            if !cfg.has_playable_setup() {
                continue;
            }

            active_anim_ids.insert(anim.id);
            let sig = cfg.config_signature(state.geo_anchor);
            let sim = pool
                .sims
                .entry(anim.id)
                .or_insert_with(|| ActiveTrafficSim {
                    config_sig: sig,
                    engine: TrafficSimEngine::new(&cfg, state.geo_anchor),
                });
            if sim.config_sig != sig {
                sim.config_sig = sig;
                sim.engine = TrafficSimEngine::new(&cfg, state.geo_anchor);
            }

            sim.engine.advance_to(track.distance_m);
            if is_signal {
                live_signal_statuses.insert(anim.id, sim.engine.live_signal_status());
            }
            for pose in sim.engine.active_poses() {
                active_poses.push((anim.id, pose));
            }
        } else if is_queue {
            let cfg = parse_traffic_queue_config(anim);
            if !cfg.has_playable_setup() {
                continue;
            }

            active_queue_anim_ids.insert(anim.id);
            let sig = cfg.config_signature(state.geo_anchor);
            let sim = pool
                .queue_sims
                .entry(anim.id)
                .or_insert_with(|| ActiveTrafficQueueSim {
                    config_sig: sig,
                    engine: TrafficQueueSimEngine::new(&cfg, state.geo_anchor),
                });
            if sim.config_sig != sig {
                sim.config_sig = sig;
                sim.engine = TrafficQueueSimEngine::new(&cfg, state.geo_anchor);
            }

            sim.engine.advance_to_with_manual(
                track.distance_m,
                track.queue_clearance_at_sec,
                &track.queue_manual_events,
            );
            let status = sim.engine.live_status();
            if status.is_completed && track.status == AnimRunStatus::Playing {
                completed_queue_anim_ids.push(anim.id);
            }
            live_queue_statuses.insert(anim.id, status);
            for pose in sim.engine.active_poses() {
                active_poses.push((anim.id, pose));
            }
        } else if is_diversion {
            let cfg = parse_traffic_diversion_config(anim);
            if !cfg.has_playable_setup() {
                continue;
            }

            active_diversion_anim_ids.insert(anim.id);
            let sig = cfg.config_signature(state.geo_anchor);
            let sim = pool
                .diversion_sims
                .entry(anim.id)
                .or_insert_with(|| ActiveTrafficDiversionSim {
                    config_sig: sig,
                    engine: TrafficDiversionSimEngine::new(&cfg, state.geo_anchor),
                });
            if sim.config_sig != sig {
                sim.config_sig = sig;
                sim.engine = TrafficDiversionSimEngine::new(&cfg, state.geo_anchor);
            }

            sim.engine
                .advance_to(track.distance_m, &track.diversion_events);
            live_diversion_statuses.insert(anim.id, sim.engine.live_status());
            for pose in sim.engine.active_poses() {
                active_poses.push((anim.id, pose));
            }
        }
    }

    // Mark any `traffic_queue` animation whose Mode Lancar drained all vehicles to 0 as Finished
    if !completed_queue_anim_ids.is_empty() {
        for aid in completed_queue_anim_ids {
            if let Some(tr) = playback.tracks.get_mut(&aid) {
                tr.status = AnimRunStatus::Finished;
                tr.completed_cycles = tr.completed_cycles.saturating_add(1);
            }
        }
        playback.playing = playback
            .tracks
            .values()
            .any(|t| t.status == AnimRunStatus::Playing);
    }

    state.traffic_queue_live_status = live_queue_statuses;
    state.traffic_diversion_live_status = live_diversion_statuses;
    state.traffic_signal_live_status = live_signal_statuses.clone();

    sync_procedural_traffic_lights(
        &mut commands,
        &state,
        &live_signal_statuses,
        &mut meshes,
        &mut materials,
        &mut pool,
        &mut light_tf_q,
        &mut lamp_mat_q,
    );

    // Drop simulation state for stopped/removed animations
    pool.sims.retain(|id, _| active_anim_ids.contains(id));
    pool.queue_sims
        .retain(|id, _| active_queue_anim_ids.contains(id));
    pool.diversion_sims
        .retain(|id, _| active_diversion_anim_ids.contains(id));

    let wanted_keys: HashMap<(Uuid, u64), &str> = active_poses
        .iter()
        .map(|(aid, p)| ((*aid, p.vehicle_id), p.model_path.as_str()))
        .collect();

    // Step 1: Release slots whose vehicles exited their lane, switched model_path, or duplicated a key
    let mut kept_keys: HashSet<(Uuid, u64)> = HashSet::with_capacity(wanted_keys.len());
    for slot in &mut pool.slots {
        if let Some(key) = slot.in_use_by {
            let valid = wanted_keys
                .get(&key)
                .is_some_and(|&expected_model| expected_model == slot.model_path.as_str())
                && kept_keys.insert(key);
            if !valid {
                slot.in_use_by = None;
                if let Ok((_, mut vis, _)) = tf_vis_q.get_mut(slot.entity) {
                    *vis = Visibility::Hidden;
                } else {
                    commands.entity(slot.entity).insert(Visibility::Hidden);
                }
            }
        }
    }

    // Step 2: Update already-bound slots and assign idle pooled slots to newly spawned vehicles
    for (anim_id, pose) in active_poses {
        let key = (anim_id, pose.vehicle_id);
        let slot_idx = if let Some(idx) = pool
            .slots
            .iter()
            .position(|s| s.in_use_by == Some(key) && s.model_path == pose.model_path)
        {
            idx
        } else if let Some(idx) = pool
            .slots
            .iter()
            .position(|s| s.in_use_by.is_none() && s.model_path == pose.model_path)
        {
            pool.slots[idx].in_use_by = Some(key);
            idx
        } else {
            let idx = pool.spawn_pooled_slot(
                &mut commands,
                &asset_server,
                &mut meshes,
                &mut materials,
                &state,
                &pose.model_path,
            );
            pool.slots[idx].in_use_by = Some(key);
            idx
        };

        let entity = pool.slots[slot_idx].entity;
        let final_scale = (pose.scale * pose.fade_scale).max(0.0005);
        let new_tf = Transform {
            translation: Vec3::new(pose.x, pose.y, pose.z),
            rotation: Quat::from_rotation_y(pose.rot_y_deg.to_radians()),
            scale: Vec3::splat(final_scale),
        };
        if let Ok((mut tf, mut vis, _)) = tf_vis_q.get_mut(entity) {
            *tf = new_tf;
            *vis = Visibility::Visible;
        } else {
            // Newly spawned entity this frame (not yet in query cache): apply via Commands immediately
            commands
                .entity(entity)
                .insert((new_tf, Visibility::Visible));
        }
    }

    // Step 3: Gently cap oversized idle pools after switching scenarios/models
    const MAX_POOLED_SLOTS: usize = 260;
    if pool.slots.len() > MAX_POOLED_SLOTS {
        let mut to_remove = pool.slots.len() - MAX_POOLED_SLOTS;
        pool.slots.retain(|slot| {
            if to_remove > 0 && slot.in_use_by.is_none() {
                commands.entity(slot.entity).despawn();
                to_remove -= 1;
                false
            } else {
                true
            }
        });
    }
}

/// Synchronizes pooled procedural 3D Traffic Light (APILL) entities & road stop lines across
/// Plot Editor, Editor Preview, Play Skenario, and Layar Besar.
#[allow(clippy::too_many_arguments)]
fn sync_procedural_traffic_lights(
    commands: &mut Commands,
    state: &AppState,
    live_signal_statuses: &HashMap<Uuid, TrafficSignalLiveStatus>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pool: &mut TrafficVisualPool,
    light_tf_q: &mut Query<
        (&mut Transform, &mut Visibility),
        (With<TrafficLightVisualEntity>, Without<TrafficVehicleEntity>),
    >,
    lamp_mat_q: &mut Query<&mut MeshMaterial3d<StandardMaterial>, With<TrafficLightLampPart>>,
) {
    let mut wanted: Vec<((Uuid, Uuid), TrafficLightVisualPose)> = Vec::new();
    for anim in &state.anims {
        if !is_traffic_signal_anim(anim) || state.hidden_entity_ids.contains(&anim.id) {
            continue;
        }
        if let Some(live) = live_signal_statuses.get(&anim.id) {
            for lp in &live.light_states {
                wanted.push(((anim.id, lp.light_id), lp.clone()));
            }
        } else {
            let cfg = parse_traffic_anim_config(anim);
            for lp in resolve_traffic_light_visual_poses(&cfg, state.geo_anchor, 0.0) {
                wanted.push(((anim.id, lp.light_id), lp));
            }
        }
    }

    let wanted_keys: HashSet<(Uuid, Uuid)> = wanted.iter().map(|(k, _)| *k).collect();
    // Despawn removed or hidden traffic light slots
    let stale_keys: Vec<(Uuid, Uuid)> = pool
        .light_slots
        .keys()
        .copied()
        .filter(|k| !wanted_keys.contains(k))
        .collect();
    for k in stale_keys {
        if let Some(slot) = pool.light_slots.remove(&k) {
            commands.entity(slot.root).despawn();
            commands.entity(slot.stop_line).despawn();
        }
    }

    if wanted.is_empty() {
        return;
    }

    pool.ensure_light_assets(meshes, materials);

    for (key, lp) in wanted {
        if !pool.light_slots.contains_key(&key) {
            pool.spawn_traffic_light_slot(commands, meshes, materials, key);
        }
        let assets = pool.light_assets.as_ref().unwrap();
        let red_mat = if lp.color == TrafficLightColor::Red {
            assets.red_on.clone()
        } else {
            assets.red_off.clone()
        };
        let yellow_mat = if lp.color == TrafficLightColor::Yellow {
            assets.yellow_on.clone()
        } else {
            assets.yellow_off.clone()
        };
        let green_mat = if lp.color == TrafficLightColor::Green {
            assets.green_on.clone()
        } else {
            assets.green_off.clone()
        };
        let beacon_mat = match lp.color {
            TrafficLightColor::Red => assets.red_on.clone(),
            TrafficLightColor::Yellow => assets.yellow_on.clone(),
            TrafficLightColor::Green => assets.green_on.clone(),
        };

        let slot = pool.light_slots.get_mut(&key).unwrap();

        let pole_rot = Quat::from_euler(
            EulerRot::YXZ,
            lp.rot_deg[1].to_radians(),
            lp.rot_deg[0].to_radians(),
            lp.rot_deg[2].to_radians(),
        );
        let pole_tf = Transform {
            translation: Vec3::from_array(lp.pos),
            rotation: pole_rot,
            scale: Vec3::new(
                lp.scale[0].max(0.15),
                lp.scale[1].max(0.15),
                lp.scale[2].max(0.15),
            ),
        };
        let vis = Visibility::Visible;
        if let Ok((mut tf, mut v)) = light_tf_q.get_mut(slot.root) {
            *tf = pole_tf;
            *v = vis;
        } else {
            commands.entity(slot.root).insert((pole_tf, vis));
        }

        // Update 3D white painted road stop-line bar across the bound lane
        let a = Vec3::from_array(lp.stop_line_a);
        let b = Vec3::from_array(lp.stop_line_b);
        let span = b - a;
        let line_len = Vec2::new(span.x, span.z).length();
        let (sl_tf, sl_vis) = if lp.enabled && line_len >= 0.4 {
            let mid = (a + b) * 0.5 + Vec3::Y * 0.05;
            // Local +X of `stop_line_mesh` stretches across `a -> b`
            let yaw = (-span.z).atan2(span.x);
            (
                Transform {
                    translation: mid,
                    rotation: Quat::from_rotation_y(yaw),
                    scale: Vec3::new(line_len, 1.0, 1.0),
                },
                Visibility::Visible,
            )
        } else {
            (Transform::default(), Visibility::Hidden)
        };
        if let Ok((mut tf, mut v)) = light_tf_q.get_mut(slot.stop_line) {
            *tf = sl_tf;
            *v = sl_vis;
        } else {
            commands.entity(slot.stop_line).insert((sl_tf, sl_vis));
        }

        // Swap emissive lamp materials whenever color changes (or on first frame)
        if slot.last_color != Some(lp.color) {
            slot.last_color = Some(lp.color);
            for (lamp_ent, mat_handle) in [
                (slot.red_lamp, red_mat),
                (slot.yellow_lamp, yellow_mat),
                (slot.green_lamp, green_mat),
                (slot.beacon_lamp, beacon_mat),
            ] {
                if let Ok(mut mat) = lamp_mat_q.get_mut(lamp_ent) {
                    mat.0 = mat_handle;
                } else {
                    commands.entity(lamp_ent).insert(MeshMaterial3d(mat_handle));
                }
            }
        }
    }
}
