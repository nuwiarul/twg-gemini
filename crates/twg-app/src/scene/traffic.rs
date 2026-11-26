use std::collections::{HashMap, HashSet};

use bevy::{camera::primitives::Aabb, prelude::*};
use twg_shared::{
    group_anim::collect_group_descendant_ids,
    traffic_anim::{
        TrafficSimEngine, TrafficVehiclePose, is_traffic_anim, parse_traffic_anim_config,
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
    let Some(anim) = state
        .anims
        .iter()
        .find(|a| a.id == preview.anim_id && (is_traffic_anim(a) || is_traffic_queue_anim(a)))
    else {
        return Vec::new();
    };
    let (lane_geom, vehicle_pool, lane_w, side_sign): (
        Vec<[f64; 3]>,
        Vec<twg_shared::traffic_anim::TrafficVehicleEntry>,
        f32,
        f32,
    ) = if is_traffic_queue_anim(anim) {
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

#[derive(Component)]
pub struct TrafficVehicleEntity;

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

/// Reusable Bevy entity & scene-handle pool for all active Traffic simulations (`traffic_flow` & `traffic_queue`).
/// Vehicles recycle idle `Entity` slots (`Visibility::Hidden` <-> `Visible`) instead of
/// spawning/despawning glTF hierarchies every cycle, keeping frame times smooth.
#[derive(Resource, Default)]
pub struct TrafficVisualPool {
    sims: HashMap<Uuid, ActiveTrafficSim>,
    queue_sims: HashMap<Uuid, ActiveTrafficQueueSim>,
    scene_handles: HashMap<String, Handle<WorldAsset>>,
    fallback_body_mesh: Option<Handle<Mesh>>,
    fallback_cabin_mesh: Option<Handle<Mesh>>,
    fallback_mat: Option<Handle<StandardMaterial>>,
    slots: Vec<PooledVehicleSlot>,
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
        state.traffic_queue_live_status.clear();
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

    if !playback.play_test_mode && !is_editor_preview && scale_preview_poses.is_empty() {
        pool.sims.clear();
        pool.queue_sims.clear();
        state.traffic_queue_live_status.clear();
        for slot in &mut pool.slots {
            slot.in_use_by = None;
            if let Ok((_, mut vis, _)) = tf_vis_q.get_mut(slot.entity) {
                *vis = Visibility::Hidden;
            }
        }
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
    let mut completed_queue_anim_ids: Vec<Uuid> = Vec::new();
    let mut live_queue_statuses: HashMap<Uuid, twg_shared::traffic_queue::TrafficQueueLiveStatus> =
        HashMap::new();
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
        let is_flow = is_traffic_anim(anim);
        let is_queue = is_traffic_queue_anim(anim);
        if (!is_flow && !is_queue) || state.hidden_entity_ids.contains(&anim.id) {
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

            sim.engine
                .advance_to(track.distance_m, track.queue_clearance_at_sec);
            let status = sim.engine.live_status();
            if status.is_completed && track.status == AnimRunStatus::Playing {
                completed_queue_anim_ids.push(anim.id);
            }
            live_queue_statuses.insert(anim.id, status);
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

    // Drop simulation state for stopped/removed animations
    pool.sims.retain(|id, _| active_anim_ids.contains(id));
    pool.queue_sims
        .retain(|id, _| active_queue_anim_ids.contains(id));

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
        let final_scale = (pose.scale * pose.fade_scale).max(0.01);
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
