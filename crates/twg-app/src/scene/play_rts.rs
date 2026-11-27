use std::collections::{HashMap, HashSet};

use bevy::{prelude::*, window::PrimaryWindow};
use twg_shared::{
    models::{CategoryPlayBehavior, Marker},
    rts_nav::{
        ObstacleShape, compute_formation_targets, find_obstacle_avoiding_path, point_blocked,
    },
    ws::TacticalCommand,
};
use uuid::Uuid;

use crate::{
    map::camera::cursor_world_ground,
    net::ws_client::{WsClientBridge, WsOutCommand},
    scene::{
        basic_mesh::{BasicMeshShape, parse_basic_mesh_config},
        gizmo::{
            GIZMO_SCREEN_RADIUS_PX, GizmoMode, TransformGizmoState,
            begin_move_gizmo_drag_at_origin, compute_gizmo_world_radius,
            compute_move_gizmo_drag_world_pos, hit_test_gizmo, marker_quat,
        },
        markers::{MarkerWorldBounds, ScenePickParams, raycast_mesh_node},
        playback::{MarkerPlaybackPose, PlaybackOverrides},
    },
    state::{AppScreen, AppState, PlaybackState, SelectedEntity, UiPointerState},
};

#[derive(Debug, Clone)]
pub struct RtsUnitOrder {
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
    pub rot_y: f64,
    pub waypoints_xz: Vec<[f64; 2]>,
    pub waypoint_idx: usize,
    pub final_rot_y: f64,
    pub speed_mps: f64,
    pub agent_radius_m: f64,
    pub moving: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct RtsMovePing {
    pub world_pos: Vec3,
    pub age_sec: f32,
    pub duration_sec: f32,
}

#[derive(Resource, Default)]
pub struct PlayRtsState {
    pub active_in_play: bool,
    pub selected_unit_ids: Vec<Uuid>,
    pub selected_gizmo_marker_id: Option<Uuid>,
    pub hovered_marker_id: Option<Uuid>,
    /// Active left-drag marquee rectangle in logical viewport coordinates `(start_px, curr_px)`.
    pub marquee_drag: Option<(Vec2, Vec2)>,
    pub left_press_start_px: Option<Vec2>,
    pub right_press_start_px: Option<Vec2>,
    pub right_dragged: bool,
    /// Temporary runtime positions & active paths for RTS-controlled units in Play Skenario.
    pub unit_orders: HashMap<Uuid, RtsUnitOrder>,
    /// Temporary `(lng, lat, altitude)` overrides for `GizmoMove` (`tools`) objects in Play Skenario.
    pub gizmo_pose_overrides: HashMap<Uuid, (f64, f64, f64)>,
    /// Expanding tactical click-target ripples rendered on the ground after Right-Click move orders.
    pub move_pings: Vec<RtsMovePing>,
}

impl PlayRtsState {
    pub fn has_runtime_changes(&self) -> bool {
        !self.unit_orders.is_empty() || !self.gizmo_pose_overrides.is_empty()
    }

    pub fn clear_selection(&mut self) {
        self.selected_unit_ids.clear();
        self.selected_gizmo_marker_id = None;
    }

    pub fn reset_positions(&mut self) {
        self.unit_orders.clear();
        self.gizmo_pose_overrides.clear();
        self.move_pings.clear();
    }

    pub fn reset_all(&mut self) {
        self.active_in_play = false;
        self.selected_unit_ids.clear();
        self.selected_gizmo_marker_id = None;
        self.hovered_marker_id = None;
        self.marquee_drag = None;
        self.left_press_start_px = None;
        self.right_press_start_px = None;
        self.right_dragged = false;
        self.unit_orders.clear();
        self.gizmo_pose_overrides.clear();
        self.move_pings.clear();
    }

    /// Returns the effective `(lng, lat, altitude, rot_y)` of `marker` in Play Skenario,
    /// factoring in live `GizmoMove` overrides, RTS movement orders, and scripted playback poses.
    pub fn marker_runtime_pose(
        &self,
        marker: &Marker,
        overrides: &PlaybackOverrides,
    ) -> (f64, f64, f64, f64) {
        if let Some(&(lng, lat, alt)) = self.gizmo_pose_overrides.get(&marker.id) {
            return (lng, lat, alt, marker.rot_y);
        }
        if let Some(order) = self.unit_orders.get(&marker.id) {
            return (order.lng, order.lat, order.altitude, order.rot_y);
        }
        if let Some(ov) = overrides.markers.get(&marker.id) {
            return (ov.lng, ov.lat, ov.altitude, ov.rot_y);
        }
        (marker.lng, marker.lat, marker.altitude, marker.rot_y)
    }

    pub fn marker_runtime_world_origin(
        &self,
        state: &AppState,
        overrides: &PlaybackOverrides,
        marker: &Marker,
    ) -> Vec3 {
        let (lng, lat, alt, _) = self.marker_runtime_pose(marker, overrides);
        Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt))
    }

    /// Applies an incoming realtime `rts_move` WS payload from another Tactical participant so
    /// peers & Layar Besar walk the exact same formation paths & play Walk -> Idle seamlessly.
    pub fn apply_remote_rts_move(
        &mut self,
        state: &AppState,
        overrides: &PlaybackOverrides,
        payload: &serde_json::Value,
    ) {
        if let Some(units) = payload.get("units").and_then(|v| v.as_array()) {
            for u in units {
                let Some(uid) = u
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                else {
                    continue;
                };
                let Some(marker) = state.markers.iter().find(|m| m.id == uid) else {
                    continue;
                };
                let waypoints_xz: Vec<[f64; 2]> = u
                    .get("waypoints_xz")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                let pair = p.as_array()?;
                                Some([pair.first()?.as_f64()?, pair.get(1)?.as_f64()?])
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if waypoints_xz.is_empty() {
                    continue;
                }
                let (cur_lng, cur_lat, alt, cur_rot_y) = self.marker_runtime_pose(marker, overrides);
                let final_rot_y = u
                    .get("final_rot_y")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(cur_rot_y);
                let speed_mps = u.get("speed_mps").and_then(|v| v.as_f64()).unwrap_or(4.5);
                let agent_radius_m = u
                    .get("agent_radius_m")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);
                let wp_idx = if waypoints_xz.len() > 1 { 1 } else { 0 };
                let last_wp = *waypoints_xz.last().unwrap();
                let start_loc = state.geo_anchor.lng_lat_alt_to_local(cur_lng, cur_lat, alt);
                let dist = (last_wp[0] - start_loc[0] as f64).hypot(last_wp[1] - start_loc[2] as f64);

                self.unit_orders.insert(
                    uid,
                    RtsUnitOrder {
                        lng: cur_lng,
                        lat: cur_lat,
                        altitude: alt,
                        rot_y: cur_rot_y,
                        waypoints_xz,
                        waypoint_idx: wp_idx,
                        final_rot_y,
                        speed_mps,
                        agent_radius_m,
                        moving: dist > 0.25,
                    },
                );
            }
        }
        if let Some(ping) = payload.get("ping_xz").and_then(|v| v.as_array())
            && let (Some(x), Some(z)) = (
                ping.first().and_then(|v| v.as_f64()),
                ping.get(1).and_then(|v| v.as_f64()),
            )
        {
            self.move_pings.push(RtsMovePing {
                world_pos: Vec3::new(x as f32, 0.14, z as f32),
                age_sec: 0.0,
                duration_sec: 0.95,
            });
        }
    }

    /// Applies an incoming realtime `gizmo_move` WS payload for a `tools` / `GizmoMove` object.
    pub fn apply_remote_gizmo_move(&mut self, marker_id: Uuid, payload: &serde_json::Value) {
        if let (Some(lng), Some(lat), Some(alt)) = (
            payload.get("lng").and_then(|v| v.as_f64()),
            payload.get("lat").and_then(|v| v.as_f64()),
            payload.get("altitude").and_then(|v| v.as_f64()),
        ) {
            self.gizmo_pose_overrides.insert(marker_id, (lng, lat, alt));
        }
    }
}

fn round_clean(val: f64, decimals: i32) -> f64 {
    let factor = 10_f64.powi(decimals);
    let r = (val * factor).round() / factor;
    if r.abs() < 1e-9 { 0.0 } else { r }
}

fn marker_agent_radius(marker: &Marker, marker_bounds: &MarkerWorldBounds) -> f64 {
    if let Some(b) = marker_bounds.bounds.get(&marker.id) {
        let hx = (b.world_max.x - b.world_min.x) * 0.5;
        let hz = (b.world_max.z - b.world_min.z) * 0.5;
        ((hx.hypot(hz) * 0.80) as f64).clamp(0.55, 4.2)
    } else {
        (marker.scale_x.max(marker.scale_z) * 1.4).clamp(0.65, 3.2)
    }
}

fn rts_unit_speed_mps(marker: &Marker) -> f64 {
    match marker.category.trim().to_ascii_lowercase().as_str() {
        "kendaraans" | "kendaraan" | "kendaraan_umums" | "kendaraan_umum" => 11.0,
        _ => 4.5,
    }
}

fn build_play_obstacles(
    state: &AppState,
    overrides: &PlaybackOverrides,
    marker_bounds: &MarkerWorldBounds,
    play_rts: &PlayRtsState,
    ignore_marker_ids: &HashSet<Uuid>,
) -> Vec<ObstacleShape> {
    let mut obstacles = Vec::new();

    // 1. Extruded 3D Buildings -> exact ground footprint polygons
    for b in &state.buildings {
        if state.hidden_entity_ids.contains(&b.id) || b.geom.len() < 3 {
            continue;
        }
        let verts: Vec<[f64; 2]> = b
            .geom
            .iter()
            .map(|p| {
                let loc = state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.0);
                [loc[0] as f64, loc[2] as f64]
            })
            .collect();
        obstacles.push(ObstacleShape::Polygon { verts });
    }

    // 2. Visible markers not currently excluded:
    //    - StaticObstacle (`bangunans`, `Basic Mesh`, custom static categories)
    //    - GizmoMove (`tools`)
    //    - Stationary other `RtsUnit`s
    for m in &state.markers {
        if ignore_marker_ids.contains(&m.id) || !overrides.is_marker_visible(m, true) {
            continue;
        }
        let behavior = state.marker_play_behavior(m);

        // Basic Mesh Polygon -> exact world-space polygon obstacle (transformed by marker pose)!
        if let Some(cfg) = parse_basic_mesh_config(m)
            && cfg.shape == BasicMeshShape::Polygon
            && cfg.polygon_points.len() >= 3
        {
            let origin = play_rts.marker_runtime_world_origin(state, overrides, m);
            let rot = marker_quat(m);
            let scale = Vec3::new(m.scale_x as f32, m.scale_y as f32, m.scale_z as f32);
            let verts: Vec<[f64; 2]> = cfg
                .polygon_points
                .iter()
                .map(|p| {
                    let wp = origin + rot * (Vec3::new(p[0], 0.0, p[1]) * scale);
                    [wp.x as f64, wp.z as f64]
                })
                .collect();
            obstacles.push(ObstacleShape::Polygon { verts });
            continue;
        }

        if let Some(b) = marker_bounds.bounds.get(&m.id) {
            let hx = ((b.world_max.x - b.world_min.x) * 0.5) as f64;
            let hz = ((b.world_max.z - b.world_min.z) * 0.5) as f64;
            let max_h = hx.max(hz);
            let min_h = hx.min(hz);
            // Elongated static structures (walls, barricades, buildings) use tight rectangular polygon
            if behavior == CategoryPlayBehavior::StaticObstacle
                && max_h > 0.8
                && max_h > min_h * 1.35
            {
                let min_x = b.world_min.x as f64;
                let max_x = b.world_max.x as f64;
                let min_z = b.world_min.z as f64;
                let max_z = b.world_max.z as f64;
                obstacles.push(ObstacleShape::Polygon {
                    verts: vec![
                        [min_x, min_z],
                        [max_x, min_z],
                        [max_x, max_z],
                        [min_x, max_z],
                    ],
                });
            } else {
                let cap = if behavior == CategoryPlayBehavior::StaticObstacle {
                    24.0
                } else {
                    4.5
                };
                let r = (hx.hypot(hz) * 0.84).clamp(0.55, cap);
                obstacles.push(ObstacleShape::Circle {
                    x: b.footprint_center.x as f64,
                    z: b.footprint_center.z as f64,
                    radius: r,
                });
            }
        } else {
            let world = play_rts.marker_runtime_world_origin(state, overrides, m);
            let r = marker_agent_radius(m, marker_bounds);
            obstacles.push(ObstacleShape::Circle {
                x: world.x as f64,
                z: world.z as f64,
                radius: r,
            });
        }
    }

    obstacles
}

/// Picks ONLY visible 3D markers whose resolved `CategoryPlayBehavior` is `RtsUnit` or `GizmoMove`.
/// `Building`, `Line`, `TextMarker`, `Basic Mesh`, `Anim`, and `StaticObstacle` (`bangunans`)
/// are strictly non-selectable in Play Skenario.
#[allow(clippy::too_many_arguments)]
fn pick_play_selectable_marker(
    state: &AppState,
    overrides: &PlaybackOverrides,
    marker_bounds: &MarkerWorldBounds,
    play_rts: &PlayRtsState,
    cursor_px: Vec2,
    ground_hit: Option<Vec3>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
    pick: &ScenePickParams,
) -> Option<(Uuid, CategoryPlayBehavior)> {
    // Priority 1: Exact 3D mesh / AABB raycast across selectable visible markers
    if let Ok(world_ray) = camera.viewport_to_world(cam_gtf, cursor_px) {
        let mut best_hit: Option<(Uuid, CategoryPlayBehavior, f32)> = None;
        let mut stack: Vec<Entity> = Vec::with_capacity(16);

        for (root_entity, marker_comp, vis) in &pick.marker_roots {
            if *vis == Visibility::Hidden {
                continue;
            }
            let Some(marker) = state.markers.iter().find(|m| m.id == marker_comp.id) else {
                continue;
            };
            if !overrides.is_marker_visible(marker, true)
                || state.hidden_entity_ids.contains(&marker.id)
                || !state.can_edit_marker(marker)
            {
                continue;
            }
            let behavior = state.marker_play_behavior(marker);
            if behavior == CategoryPlayBehavior::StaticObstacle {
                continue;
            }

            let hidden_branch = if marker_comp.is_walking_now && marker_comp.walk_child.is_some() {
                marker_comp.idle_child
            } else {
                marker_comp.walk_child
            };
            stack.clear();
            stack.push(root_entity);
            while let Some(e) = stack.pop() {
                if Some(e) == hidden_branch {
                    continue;
                }
                if let Ok((gtf, aabb_opt, mesh3d_opt)) = pick.mesh_nodes.get(e)
                    && (aabb_opt.is_some() || mesh3d_opt.is_some())
                    && let Some(t) = raycast_mesh_node(
                        world_ray,
                        gtf,
                        aabb_opt,
                        mesh3d_opt,
                        &pick.meshes,
                        true,
                    )
                    && best_hit.is_none_or(|cur| t < cur.2)
                {
                    best_hit = Some((marker.id, behavior, t));
                }
                if let Ok(children) = pick.children.get(e) {
                    for child in children.iter() {
                        stack.push(child);
                    }
                }
            }
        }

        if let Some((id, beh, _)) = best_hit {
            return Some((id, beh));
        }
    }

    // Priority 2: Ground footprint / screen-pivot fallback for small or distant selectable markers
    let mut best_fallback: Option<(Uuid, CategoryPlayBehavior, f32)> = None;
    for m in &state.markers {
        if !overrides.is_marker_visible(m, true)
            || state.hidden_entity_ids.contains(&m.id)
            || !state.can_edit_marker(m)
        {
            continue;
        }
        let behavior = state.marker_play_behavior(m);
        if behavior == CategoryPlayBehavior::StaticObstacle {
            continue;
        }
        let (center_3d, ring_r) = if let Some(b) = marker_bounds.bounds.get(&m.id) {
            (b.footprint_center, b.footprint_radius)
        } else {
            let p = play_rts.marker_runtime_world_origin(state, overrides, m);
            (p, (m.scale_x.max(m.scale_z) as f32 * 2.2).max(0.75))
        };

        let hit_ground_ring = ground_hit.is_some_and(|hit| {
            Vec2::new(center_3d.x - hit.x, center_3d.z - hit.z).length() <= ring_r
        });
        let hit_pivot_px = camera
            .world_to_viewport(cam_gtf, center_3d)
            .ok()
            .filter(|scr| cursor_px.distance(*scr) <= 16.0);

        if hit_ground_ring || hit_pivot_px.is_some() {
            let score = hit_pivot_px
                .map(|scr| cursor_px.distance(scr))
                .unwrap_or(8.0);
            if best_fallback.is_none_or(|cur| score < cur.2) {
                best_fallback = Some((m.id, behavior, score));
            }
        }
    }

    best_fallback.map(|(id, beh, _)| (id, beh))
}

/// Interactive RTS unit selection (single click & marquee box drag), Right-Click formation move
/// orders with A* obstacle avoidance, and 3D Move Gizmo manipulation for `GizmoMove` (`tools`)
/// during `Play Skenario`.
#[allow(clippy::too_many_arguments)]
pub fn play_rts_interaction_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_state: Res<UiPointerState>,
    mut state: ResMut<AppState>,
    playback: Res<PlaybackState>,
    (overrides, marker_bounds, ws): (
        Res<PlaybackOverrides>,
        Res<MarkerWorldBounds>,
        Res<WsClientBridge>,
    ),
    mut gizmo_state: ResMut<TransformGizmoState>,
    mut play_rts: ResMut<PlayRtsState>,
    cam_q: Query<(&Camera, &GlobalTransform), With<crate::map::camera::TacticalCamera>>,
    pick: ScenePickParams,
    mut last_gizmo_sync_sec: Local<f64>,
) {
    // Layar Besar is strictly read-only — never process local selection/drag/click input there,
    // and keep remote unit_orders / gizmo_pose_overrides intact!
    if state.is_big_screen() {
        return;
    }

    let in_play = state.screen == AppScreen::PlotSkenario && playback.play_test_mode;
    if !in_play {
        if play_rts.active_in_play {
            play_rts.reset_all();
            gizmo_state.cancel();
        }
        return;
    }

    // One-time clean transition into Play Skenario
    if !play_rts.active_in_play {
        play_rts.reset_all();
        play_rts.active_in_play = true;
        gizmo_state.cancel();
    }

    // Prune stale, hidden, or unauthorized selections
    play_rts.selected_unit_ids.retain(|uid| {
        state.markers.iter().any(|m| {
            m.id == *uid
                && overrides.is_marker_visible(m, true)
                && !state.hidden_entity_ids.contains(&m.id)
                && state.can_edit_marker(m)
                && state.marker_play_behavior(m) == CategoryPlayBehavior::RtsUnit
        })
    });
    if let Some(gid) = play_rts.selected_gizmo_marker_id {
        let valid = state.markers.iter().any(|m| {
            m.id == gid
                && overrides.is_marker_visible(m, true)
                && !state.hidden_entity_ids.contains(&m.id)
                && state.can_edit_marker(m)
                && state.marker_play_behavior(m) == CategoryPlayBehavior::GizmoMove
        });
        if !valid {
            play_rts.selected_gizmo_marker_id = None;
            gizmo_state.cancel();
        }
    }

    // Escape clears Play Skenario selection & active drags
    if !ui_state.wants_keyboard && keys.just_pressed(KeyCode::Escape) {
        play_rts.clear_selection();
        play_rts.marquee_drag = None;
        play_rts.left_press_start_px = None;
        gizmo_state.cancel();
        return;
    }

    let Ok((camera, cam_gtf)) = cam_q.single() else {
        return;
    };

    let cursor_px_opt = window.cursor_position();
    let shift_held = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let space_held = !ui_state.wants_keyboard && keys.pressed(KeyCode::Space);
    let alt_held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

    // -------------------------------------------------------------------------
    // 1. Active 3D Move Gizmo interaction for selected `GizmoMove` (`tools`)
    // -------------------------------------------------------------------------
    if let Some(gid) = play_rts.selected_gizmo_marker_id {
        if let Some(mut drag) = gizmo_state.active_drag.take() {
            if drag.marker_id == gid && mouse_buttons.pressed(MouseButton::Left) {
                if let Some(cursor_px) = cursor_px_opt
                    && let Some(new_pos) =
                        compute_move_gizmo_drag_world_pos(&mut drag, cursor_px, camera, cam_gtf)
                {
                    let (lng, lat, alt) = state
                        .geo_anchor
                        .local_to_lng_lat_alt([new_pos.x, new_pos.y, new_pos.z]);
                    let clean_alt = round_clean(alt, 2);
                    play_rts
                        .gizmo_pose_overrides
                        .insert(gid, (lng, lat, clean_alt));
                    drag.moved = true;

                    // Broadcast live Gizmo Move updates (~20 Hz) in Tactical Live mode
                    let now_sec = time.elapsed_secs_f64();
                    if state.is_tactical_live()
                        && !state.offline_preview
                        && now_sec - *last_gizmo_sync_sec >= 0.05
                        && let Some(sk) = &state.active_skenario
                    {
                        *last_gizmo_sync_sec = now_sec;
                        ws.send(WsOutCommand::SendCommand(TacticalCommand {
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: Some(gid),
                            command: "gizmo_move".to_string(),
                            payload: serde_json::json!({
                                "lng": lng,
                                "lat": lat,
                                "altitude": clean_alt,
                            }),
                            save_history: false,
                        }));
                    }
                }
                gizmo_state.active_drag = Some(drag);
                gizmo_state.suppress_camera_pan = true;
                play_rts.marquee_drag = None;
                play_rts.left_press_start_px = None;
                return;
            } else {
                // Drag released — send final exact gizmo pose if Tactical Live
                if drag.moved
                    && state.is_tactical_live()
                    && !state.offline_preview
                    && let Some(&(lng, lat, alt)) = play_rts.gizmo_pose_overrides.get(&gid)
                    && let Some(sk) = &state.active_skenario
                {
                    ws.send(WsOutCommand::SendCommand(TacticalCommand {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: Some(gid),
                        command: "gizmo_move".to_string(),
                        payload: serde_json::json!({
                            "lng": lng,
                            "lat": lat,
                            "altitude": alt,
                        }),
                        save_history: false,
                    }));
                }
                gizmo_state.active_drag = None;
                gizmo_state.suppress_camera_pan = false;
                play_rts.marquee_drag = None;
                play_rts.left_press_start_px = None;
                return;
            }
        }

        // Hover & press test against the Move Gizmo handles
        if !ui_state.wants_pointer
            && let Some(cursor_px) = cursor_px_opt
            && let Some(marker) = state.markers.iter().find(|m| m.id == gid)
        {
            let origin = play_rts.marker_runtime_world_origin(&state, &overrides, marker);
            let radius =
                compute_gizmo_world_radius(camera, cam_gtf, origin, GIZMO_SCREEN_RADIUS_PX);
            let hit_handle = hit_test_gizmo(
                cursor_px,
                GizmoMode::Move,
                origin,
                Quat::IDENTITY,
                radius,
                camera,
                cam_gtf,
            );
            gizmo_state.hovered_handle = hit_handle;

            if let Some(handle) = hit_handle
                && mouse_buttons.just_pressed(MouseButton::Left)
                && !space_held
                && !alt_held
            {
                gizmo_state.active_drag = begin_move_gizmo_drag_at_origin(
                    gid, origin, handle, cursor_px, camera, cam_gtf,
                );
                gizmo_state.suppress_camera_pan = true;
                play_rts.marquee_drag = None;
                play_rts.left_press_start_px = None;
                return;
            }
        } else {
            gizmo_state.hovered_handle = None;
        }
    } else {
        gizmo_state.hovered_handle = None;
        gizmo_state.active_drag = None;
    }

    // -------------------------------------------------------------------------
    // 2. Hover highlight on selectable markers (`RtsUnit` & `GizmoMove`)
    // -------------------------------------------------------------------------
    let ground_hit = if !ui_state.wants_pointer {
        cursor_world_ground(&window, camera, cam_gtf)
    } else {
        None
    };

    if !ui_state.wants_pointer
        && play_rts.marquee_drag.is_none()
        && gizmo_state.hovered_handle.is_none()
        && !space_held
        && !alt_held
        && let Some(cursor_px) = cursor_px_opt
    {
        play_rts.hovered_marker_id = pick_play_selectable_marker(
            &state,
            &overrides,
            &marker_bounds,
            &play_rts,
            cursor_px,
            ground_hit,
            camera,
            cam_gtf,
            &pick,
        )
        .map(|(id, _)| id);
        gizmo_state.hovered_entity = play_rts.hovered_marker_id.map(SelectedEntity::Marker);
    } else {
        play_rts.hovered_marker_id = None;
        gizmo_state.hovered_entity = None;
    }

    // -------------------------------------------------------------------------
    // 3. Left Click (single pick) & Left Drag (marquee box multi-select)
    // -------------------------------------------------------------------------
    if space_held || alt_held {
        // Space + Left Drag = Trackpad Pan; Alt + Left Drag = Trackpad Orbit
        play_rts.marquee_drag = None;
        play_rts.left_press_start_px = None;
    } else {
        if mouse_buttons.just_pressed(MouseButton::Left) && !ui_state.wants_pointer {
            play_rts.left_press_start_px = cursor_px_opt;
            play_rts.marquee_drag = None;
        }

        if mouse_buttons.pressed(MouseButton::Left)
            && let (Some(start_px), Some(curr_px)) = (play_rts.left_press_start_px, cursor_px_opt)
            && start_px.distance(curr_px) >= 6.0
        {
            play_rts.marquee_drag = Some((start_px, curr_px));
        }

        if mouse_buttons.just_released(MouseButton::Left) {
            if let Some((a, b)) = play_rts.marquee_drag.take() {
                play_rts.left_press_start_px = None;
                let min_x = a.x.min(b.x);
                let max_x = a.x.max(b.x);
                let min_y = a.y.min(b.y);
                let max_y = a.y.max(b.y);

                let mut boxed: Vec<Uuid> = Vec::new();
                for m in &state.markers {
                    if !overrides.is_marker_visible(m, true)
                        || state.hidden_entity_ids.contains(&m.id)
                        || !state.can_edit_marker(m)
                    {
                        continue;
                    }
                    if state.marker_play_behavior(m) != CategoryPlayBehavior::RtsUnit {
                        continue;
                    }
                    let world_pt = if let Some(bounds) = marker_bounds.bounds.get(&m.id) {
                        Vec3::new(
                            bounds.footprint_center.x,
                            (bounds.world_min.y + bounds.world_max.y) * 0.5,
                            bounds.footprint_center.z,
                        )
                    } else {
                        play_rts.marker_runtime_world_origin(&state, &overrides, m)
                    };
                    if let Ok(scr) = camera.world_to_viewport(cam_gtf, world_pt)
                        && scr.x >= min_x
                        && scr.x <= max_x
                        && scr.y >= min_y
                        && scr.y <= max_y
                    {
                        boxed.push(m.id);
                    }
                }

                play_rts.selected_gizmo_marker_id = None;
                if shift_held {
                    for uid in boxed {
                        if !play_rts.selected_unit_ids.contains(&uid) {
                            play_rts.selected_unit_ids.push(uid);
                        }
                    }
                } else {
                    play_rts.selected_unit_ids = boxed;
                }

                if !play_rts.selected_unit_ids.is_empty() {
                    state.status_banner = Some(format!(
                        "{} unit terseleksi — Klik Kanan di peta untuk menggerakkan formasi.",
                        play_rts.selected_unit_ids.len()
                    ));
                }
            } else if play_rts.left_press_start_px.take().is_some()
                && !ui_state.wants_pointer
                && let Some(cursor_px) = cursor_px_opt
            {
                match pick_play_selectable_marker(
                    &state,
                    &overrides,
                    &marker_bounds,
                    &play_rts,
                    cursor_px,
                    ground_hit,
                    camera,
                    cam_gtf,
                    &pick,
                ) {
                    Some((mid, CategoryPlayBehavior::GizmoMove)) => {
                        play_rts.selected_unit_ids.clear();
                        play_rts.selected_gizmo_marker_id = Some(mid);
                        if let Some(m) = state.markers.iter().find(|x| x.id == mid) {
                            state.status_banner = Some(format!(
                                "Peralatan '{}' terseleksi — Geser pakai Gizmo Move 3D (X / Y / Z / Free).",
                                m.name
                            ));
                        }
                    }
                    Some((mid, CategoryPlayBehavior::RtsUnit)) => {
                        play_rts.selected_gizmo_marker_id = None;
                        if shift_held {
                            if let Some(pos) =
                                play_rts.selected_unit_ids.iter().position(|x| *x == mid)
                            {
                                play_rts.selected_unit_ids.remove(pos);
                            } else {
                                play_rts.selected_unit_ids.push(mid);
                            }
                        } else {
                            play_rts.selected_unit_ids = vec![mid];
                        }
                        if play_rts.selected_unit_ids.len() == 1
                            && let Some(m) = state
                                .markers
                                .iter()
                                .find(|x| x.id == play_rts.selected_unit_ids[0])
                        {
                            state.status_banner = Some(format!(
                                "Unit '{}' terseleksi — Klik Kanan di peta untuk menggerakkan.",
                                m.name
                            ));
                        } else if !play_rts.selected_unit_ids.is_empty() {
                            state.status_banner = Some(format!(
                                "{} unit terseleksi — Klik Kanan di peta untuk menggerakkan formasi.",
                                play_rts.selected_unit_ids.len()
                            ));
                        }
                    }
                    Some((_, CategoryPlayBehavior::StaticObstacle)) | None => {
                        if !shift_held {
                            play_rts.clear_selection();
                        }
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // 4. Right Click short (< 5 px): Issue Formation Move Order with A* Avoidance
    // -------------------------------------------------------------------------
    if mouse_buttons.just_pressed(MouseButton::Right) {
        if !ui_state.wants_pointer {
            play_rts.right_press_start_px = cursor_px_opt;
            play_rts.right_dragged = false;
        } else {
            play_rts.right_press_start_px = None;
            play_rts.right_dragged = false;
        }
    } else if mouse_buttons.pressed(MouseButton::Right)
        && !play_rts.right_dragged
        && let (Some(start_px), Some(curr_px)) = (play_rts.right_press_start_px, cursor_px_opt)
        && start_px.distance(curr_px) >= 5.0
    {
        play_rts.right_dragged = true;
    }

    if mouse_buttons.just_released(MouseButton::Right) {
        let had_press = play_rts.right_press_start_px.take().is_some();
        let was_drag = play_rts.right_dragged;
        play_rts.right_dragged = false;

        if had_press
            && !was_drag
            && !ui_state.wants_pointer
            && !play_rts.selected_unit_ids.is_empty()
            && let Some(hit) = ground_hit
        {
            let selected_set: HashSet<Uuid> = play_rts.selected_unit_ids.iter().copied().collect();
            let obstacles = build_play_obstacles(
                &state,
                &overrides,
                &marker_bounds,
                &play_rts,
                &selected_set,
            );

            let mut unit_inputs: Vec<(Uuid, [f64; 2], f64)> = Vec::new();
            let mut unit_meta: HashMap<Uuid, (f64, f64, f64, f64)> = HashMap::new(); // (alt, rot_y, speed, radius)

            for &uid in &play_rts.selected_unit_ids {
                let Some(marker) = state.markers.iter().find(|m| m.id == uid) else {
                    continue;
                };
                if !overrides.is_marker_visible(marker, true)
                    || state.marker_play_behavior(marker) != CategoryPlayBehavior::RtsUnit
                {
                    continue;
                }
                let (lng, lat, alt, rot_y) = play_rts.marker_runtime_pose(marker, &overrides);
                let loc = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
                let r = marker_agent_radius(marker, &marker_bounds);
                let spd = rts_unit_speed_mps(marker);
                unit_inputs.push((uid, [loc[0] as f64, loc[2] as f64], r));
                unit_meta.insert(uid, (alt, rot_y, spd, r));
            }

            if !unit_inputs.is_empty() {
                let start_map: HashMap<Uuid, [f64; 2]> =
                    unit_inputs.iter().map(|&(id, p, _)| (id, p)).collect();
                let formation = compute_formation_targets(
                    &unit_inputs,
                    [hit.x as f64, hit.z as f64],
                    &obstacles,
                );

                let mut ws_units_payload: Vec<serde_json::Value> = Vec::new();
                for (uid, target_xz, facing_deg) in formation {
                    let Some(&start_xz) = start_map.get(&uid) else {
                        continue;
                    };
                    let Some(&(alt, cur_rot_y, speed_mps, agent_radius_m)) = unit_meta.get(&uid)
                    else {
                        continue;
                    };
                    let waypoints_xz = find_obstacle_avoiding_path(
                        start_xz,
                        target_xz,
                        agent_radius_m,
                        &obstacles,
                    );
                    if state.is_tactical_live() && !state.offline_preview {
                        ws_units_payload.push(serde_json::json!({
                            "id": uid,
                            "waypoints_xz": &waypoints_xz,
                            "final_rot_y": facing_deg,
                            "speed_mps": speed_mps,
                            "agent_radius_m": agent_radius_m,
                        }));
                    }
                    let (lng, lat, _) =
                        state
                            .geo_anchor
                            .local_to_lng_lat_alt([start_xz[0] as f32, alt as f32, start_xz[1] as f32]);
                    let wp_idx = if waypoints_xz.len() > 1 { 1 } else { 0 };
                    let dist_to_goal = (target_xz[0] - start_xz[0]).hypot(target_xz[1] - start_xz[1]);
                    play_rts.unit_orders.insert(
                        uid,
                        RtsUnitOrder {
                            lng,
                            lat,
                            altitude: alt,
                            rot_y: cur_rot_y,
                            waypoints_xz,
                            waypoint_idx: wp_idx,
                            final_rot_y: facing_deg,
                            speed_mps,
                            agent_radius_m,
                            moving: dist_to_goal > 0.25,
                        },
                    );
                }

                play_rts.move_pings.push(RtsMovePing {
                    world_pos: Vec3::new(hit.x, 0.14, hit.z),
                    age_sec: 0.0,
                    duration_sec: 0.95,
                });

                if state.is_tactical_live()
                    && !state.offline_preview
                    && !ws_units_payload.is_empty()
                    && let Some(sk) = &state.active_skenario
                {
                    ws.send(WsOutCommand::SendCommand(TacticalCommand {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: None,
                        command: "rts_move".to_string(),
                        payload: serde_json::json!({
                            "units": ws_units_payload,
                            "ping_xz": [hit.x as f64, hit.z as f64],
                        }),
                        save_history: false,
                    }));
                }
            }
        }
    }
}

/// Runs right after `playback_system` and before `sync_markers_system`.
/// Advances active Play Skenario RTS unit movement (with smooth local separation & obstacle
/// sliding) and injects both RTS poses and `GizmoMove` (`tools`) overrides into `PlaybackOverrides`
/// so `sync_markers_system` and `animate_markers_system` handle rendering + Walk/Idle seamlessly.
pub fn play_rts_movement_system(
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    marker_bounds: Res<MarkerWorldBounds>,
    mut play_rts: ResMut<PlayRtsState>,
    mut overrides: ResMut<PlaybackOverrides>,
) {
    if !state.is_3d_map_screen() || (!playback.play_test_mode && !state.is_big_screen()) {
        return;
    }

    let dt = time.delta_secs().min(0.1) as f64;
    let speed_mult = playback.speed_multiplier.max(0.1) as f64;

    // 1. Tick visual destination ripple pings
    for ping in &mut play_rts.move_pings {
        ping.age_sec += dt as f32;
    }
    play_rts.move_pings.retain(|p| p.age_sec < p.duration_sec);

    // 2. Yield to any actively moving scripted Route animation & prune deleted/hidden markers
    play_rts.gizmo_pose_overrides.retain(|mid, _| {
        let scripted_moving = overrides.markers.get(mid).is_some_and(|ov| ov.moving);
        !scripted_moving
            && state
                .markers
                .iter()
                .any(|m| m.id == *mid && overrides.is_marker_visible(m, true))
    });
    play_rts.unit_orders.retain(|mid, _| {
        let scripted_moving = overrides.markers.get(mid).is_some_and(|ov| ov.moving);
        !scripted_moving
            && state
                .markers
                .iter()
                .any(|m| m.id == *mid && overrides.is_marker_visible(m, true))
    });

    // 3. Apply temporary 3D Move Gizmo overrides for `GizmoMove` (`tools`) objects
    for (&mid, &(lng, lat, alt)) in &play_rts.gizmo_pose_overrides {
        if let Some(m) = state.markers.iter().find(|x| x.id == mid) {
            overrides.markers.insert(
                mid,
                MarkerPlaybackPose {
                    lng,
                    lat,
                    altitude: alt,
                    rot_y: m.rot_y,
                    moving: false,
                    move_anim: Default::default(),
                },
            );
        }
    }

    if play_rts.unit_orders.is_empty() {
        return;
    }

    // 4. Snapshot static obstacles & live unit positions for smooth local collision avoidance
    let any_moving = play_rts.unit_orders.values().any(|o| o.moving);
    let mut unit_positions: HashMap<Uuid, ([f64; 2], f64, bool)> = HashMap::new();
    let mut static_obstacles: Vec<ObstacleShape> = Vec::new();

    if any_moving {
        let mut rts_unit_ids: HashSet<Uuid> = HashSet::new();
        for m in &state.markers {
            if !overrides.is_marker_visible(m, true) {
                continue;
            }
            if state.marker_play_behavior(m) == CategoryPlayBehavior::RtsUnit {
                rts_unit_ids.insert(m.id);
                let (lng, lat, alt, _) = play_rts.marker_runtime_pose(m, &overrides);
                let loc = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
                let r = marker_agent_radius(m, &marker_bounds);
                let is_moving = play_rts
                    .unit_orders
                    .get(&m.id)
                    .is_some_and(|o| o.moving);
                unit_positions.insert(m.id, ([loc[0] as f64, loc[2] as f64], r, is_moving));
            }
        }
        // Static obstacles = Buildings + StaticObstacle markers + GizmoMove markers
        static_obstacles = build_play_obstacles(
            &state,
            &overrides,
            &marker_bounds,
            &play_rts,
            &rts_unit_ids,
        );
    }

    // 5. Advance each moving RTS order & inject resulting pose into `overrides.markers`
    let order_ids: Vec<Uuid> = play_rts.unit_orders.keys().copied().collect();
    for uid in order_ids {
        let Some(order) = play_rts.unit_orders.get_mut(&uid) else {
            continue;
        };

        if order.moving {
            let loc = state
                .geo_anchor
                .lng_lat_alt_to_local(order.lng, order.lat, order.altitude);
            let cur_xz = [loc[0] as f64, loc[2] as f64];

            // Advance past intermediate waypoints once close
            while order.waypoint_idx + 1 < order.waypoints_xz.len() {
                let wp = order.waypoints_xz[order.waypoint_idx];
                let d = (wp[0] - cur_xz[0]).hypot(wp[1] - cur_xz[1]);
                if d <= (order.agent_radius_m * 0.75).clamp(0.55, 1.4) {
                    order.waypoint_idx += 1;
                } else {
                    break;
                }
            }

            let target_wp = order
                .waypoints_xz
                .get(order.waypoint_idx)
                .copied()
                .unwrap_or(cur_xz);
            let is_last_wp = order.waypoint_idx + 1 >= order.waypoints_xz.len();
            let dx = target_wp[0] - cur_xz[0];
            let dz = target_wp[1] - cur_xz[1];
            let dist = dx.hypot(dz);
            let step_max = order.speed_mps * speed_mult * dt;

            if is_last_wp && dist <= step_max.max(0.22) {
                let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                    target_wp[0] as f32,
                    order.altitude as f32,
                    target_wp[1] as f32,
                ]);
                order.lng = lng;
                order.lat = lat;
                order.rot_y = order.final_rot_y;
                order.moving = false;
            } else if dist > 1e-4 {
                let mut dir_x = dx / dist;
                let mut dir_z = dz / dist;

                // Smooth local separation against other units (stronger when close, gentle near destination)
                let mut sep_x = 0.0_f64;
                let mut sep_z = 0.0_f64;
                let near_goal_fade = if is_last_wp {
                    (dist / (order.agent_radius_m * 2.5).max(1.5)).clamp(0.25, 1.0)
                } else {
                    1.0
                };
                for (&other_id, &(other_xz, other_r, other_moving)) in &unit_positions {
                    if other_id == uid {
                        continue;
                    }
                    let ox = cur_xz[0] - other_xz[0];
                    let oz = cur_xz[1] - other_xz[1];
                    let od = ox.hypot(oz);
                    let desired_sep = order.agent_radius_m + other_r + 0.55;
                    if od < desired_sep {
                        let weight = if other_moving { 1.1 } else { 1.45 } * near_goal_fade;
                        let push = ((desired_sep - od) / desired_sep.max(0.1)).powf(1.2) * weight;
                        if od > 1e-4 {
                            sep_x += (ox / od) * push;
                            sep_z += (oz / od) * push;
                        } else {
                            // Deterministic tiny tie-breaker if two units started overlapping
                            let ang = (uid.as_u128() % 360) as f64 * std::f64::consts::PI / 180.0;
                            sep_x += ang.cos() * push;
                            sep_z += ang.sin() * push;
                        }
                    }
                }

                dir_x += sep_x * 1.25;
                dir_z += sep_z * 1.25;
                let dlen = dir_x.hypot(dir_z);
                if dlen > 1e-4 {
                    dir_x /= dlen;
                    dir_z /= dlen;
                }

                let move_step = if is_last_wp { step_max.min(dist) } else { step_max };
                let mut next_xz = [cur_xz[0] + dir_x * move_step, cur_xz[1] + dir_z * move_step];

                // Ensure local separation never pushes a unit inside a static obstacle
                let obs_clr = order.agent_radius_m * 0.78;
                if point_blocked(next_xz, obs_clr, &static_obstacles) {
                    // Try steering left/right around obstacle corner
                    let mut found_clear = false;
                    for deg in [35.0_f64, -35.0, 70.0, -70.0, 110.0, -110.0] {
                        let rad = deg.to_radians();
                        let rx = dir_x * rad.cos() - dir_z * rad.sin();
                        let rz = dir_x * rad.sin() + dir_z * rad.cos();
                        let cand = [cur_xz[0] + rx * move_step, cur_xz[1] + rz * move_step];
                        if !point_blocked(cand, obs_clr, &static_obstacles) {
                            next_xz = cand;
                            found_clear = true;
                            break;
                        }
                    }
                    if !found_clear {
                        // Fall back strictly to raw waypoint direction if already grazing an edge
                        let raw_x = dx / dist;
                        let raw_z = dz / dist;
                        let cand = [
                            cur_xz[0] + raw_x * move_step,
                            cur_xz[1] + raw_z * move_step,
                        ];
                        if !point_blocked(cand, obs_clr * 0.5, &static_obstacles) {
                            next_xz = cand;
                        } else {
                            next_xz = cur_xz;
                        }
                    }
                }

                let moved_dx = next_xz[0] - cur_xz[0];
                let moved_dz = next_xz[1] - cur_xz[1];
                if moved_dx.hypot(moved_dz) > 1e-4 {
                    let desired_yaw = moved_dx.atan2(moved_dz).to_degrees().rem_euclid(360.0);
                    let mut yaw_diff = (desired_yaw - order.rot_y + 540.0).rem_euclid(360.0) - 180.0;
                    let max_turn = 380.0 * dt * speed_mult;
                    yaw_diff = yaw_diff.clamp(-max_turn, max_turn);
                    order.rot_y = (order.rot_y + yaw_diff).rem_euclid(360.0);
                }

                let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                    next_xz[0] as f32,
                    order.altitude as f32,
                    next_xz[1] as f32,
                ]);
                order.lng = lng;
                order.lat = lat;
            }
        }

        overrides.markers.insert(
            uid,
            MarkerPlaybackPose {
                lng: order.lng,
                lat: order.lat,
                altitude: order.altitude,
                rot_y: order.rot_y,
                moving: order.moving,
                move_anim: Default::default(),
            },
        );
    }
}
