use bevy::{
    input::{
        gestures::{PinchGesture, RotationGesture},
        mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};
use twg_shared::{
    geo::{GeoAnchor, camera_distance_for_zoom, haversine_distance_m, zoom_for_camera_distance},
    ws::TacticalCommand,
};

use crate::{
    net::ws_client::{WsClientBridge, WsOutCommand},
    scene::{
        gizmo::TransformGizmoState, polygon_edit::PolygonEditState, route_edit::RouteEditState,
    },
    state::{AppScreen, AppState, PlaybackState, UiPointerState},
};

#[derive(Component)]
pub struct TacticalCamera {
    pub target_local: Vec2,
    pub target_alt_m: f32,
    pub distance_m: f32,
    pub pitch_deg: f32,
    pub bearing_deg: f32,

    pub smooth_target: Vec2,
    pub smooth_alt_m: f32,
    pub smooth_distance: f32,
    pub smooth_pitch: f32,
    pub smooth_bearing: f32,

    /// True on any frame where local mouse/trackpad/keyboard input moved the camera.
    /// Used by `camera_anim_system` for automatic manual interruption.
    pub user_interrupted_this_frame: bool,
}

impl Default for TacticalCamera {
    fn default() -> Self {
        Self {
            target_local: Vec2::ZERO,
            target_alt_m: 0.0,
            distance_m: 650.0,
            pitch_deg: 48.0,
            bearing_deg: 0.0,
            smooth_target: Vec2::ZERO,
            smooth_alt_m: 0.0,
            smooth_distance: 650.0,
            smooth_pitch: 48.0,
            smooth_bearing: 0.0,
            user_interrupted_this_frame: false,
        }
    }
}

impl TacticalCamera {
    pub fn apply_skenario_view(&mut self, zoom: f64, pitch: f64, bearing: f64, cos_lat: f64) {
        let dist = camera_distance_for_zoom(zoom, 820.0, 45.0_f32.to_radians(), cos_lat);
        self.target_local = Vec2::ZERO;
        self.smooth_target = Vec2::ZERO;
        self.target_alt_m = 0.0;
        self.smooth_alt_m = 0.0;
        self.distance_m = dist;
        self.smooth_distance = dist;
        self.pitch_deg = (pitch as f32).clamp(5.0, 82.0);
        self.smooth_pitch = self.pitch_deg;
        self.bearing_deg = (bearing as f32).rem_euclid(360.0);
        self.smooth_bearing = self.bearing_deg;
    }

    pub fn current_zoom(&self, viewport_h: f32, cos_lat: f64) -> f64 {
        zoom_for_camera_distance(self.distance_m, viewport_h, 45.0_f32.to_radians(), cos_lat)
    }

    /// Computes the Bevy world camera Transform from the current `smooth_*` pose.
    pub fn apply_transform(&self, tf: &mut Transform) {
        let tilt = self.smooth_pitch.to_radians();
        let bearing = self.smooth_bearing.to_radians();
        let horiz = self.smooth_distance * tilt.sin();
        let height = (self.smooth_distance * tilt.cos()).max(0.35);

        let offset = Vec3::new(-bearing.sin() * horiz, height, bearing.cos() * horiz);
        let target3 = Vec3::new(self.smooth_target.x, self.smooth_alt_m, self.smooth_target.y);
        tf.translation = target3 + offset;
        tf.look_at(target3, Vec3::Y);
    }
}

pub fn screen_pos_to_world_ground(
    screen_pos: Vec2,
    camera: &Camera,
    cam_tf: &GlobalTransform,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(cam_tf, screen_pos).ok()?;
    if ray.direction.y.abs() < 1e-4 {
        return None;
    }
    let t = -ray.origin.y / ray.direction.y;
    if t <= 0.0 {
        return None;
    }
    Some(ray.origin + ray.direction * t)
}

pub fn cursor_world_ground(
    window: &Window,
    camera: &Camera,
    cam_tf: &GlobalTransform,
) -> Option<Vec3> {
    let cursor = window.cursor_position()?;
    screen_pos_to_world_ground(cursor, camera, cam_tf)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn camera_control_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    (mut motion_ev, mut wheel_ev, mut pinch_ev, mut rot_ev): (
        MessageReader<MouseMotion>,
        MessageReader<MouseWheel>,
        MessageReader<PinchGesture>,
        MessageReader<RotationGesture>,
    ),
    window: Single<&Window, With<PrimaryWindow>>,
    ui_state: Res<UiPointerState>,
    playback: Res<PlaybackState>,
    (gizmo_state, poly_edit, route_edit, ws): (
        Res<TransformGizmoState>,
        Res<PolygonEditState>,
        Res<RouteEditState>,
        Res<WsClientBridge>,
    ),
    mut state: ResMut<AppState>,
    mut cam_q: Query<(&mut TacticalCamera, &mut Transform, &Camera, &GlobalTransform)>,
    (mut right_drag_origin, mut right_orbiting, mut last_cam_sync): (
        Local<Option<Vec2>>,
        Local<bool>,
        Local<(f64, Vec2, f32, f32, f32)>,
    ),
) {
    let raw_dt = time.delta_secs();
    if raw_dt > 1e-4 {
        let inst_fps = (1.0 / raw_dt).clamp(1.0, 999.0);
        state.smoothed_fps = state.smoothed_fps * 0.90 + inst_fps * 0.10;
    }

    let Ok((mut tcam, mut tf, camera, cam_gtf)) = cam_q.single_mut() else {
        return;
    };

    let in_preview = state.is_skenario_map_preview_active();
    let in_plot = state.screen == AppScreen::PlotSkenario && !state.is_plot_import_preview_active();
    let in_big_screen = state.is_big_screen();
    let in_play_mode = in_plot && playback.play_test_mode;

    if !in_preview && !in_plot && !in_big_screen {
        motion_ev.clear();
        wheel_ev.clear();
        pinch_ev.clear();
        rot_ev.clear();
        *right_drag_origin = None;
        *right_orbiting = false;
        return;
    }

    let mut total_motion = Vec2::ZERO;
    for ev in motion_ev.read() {
        total_motion += ev.delta;
    }

    let mut zoom_steps = 0.0_f32;
    let mut scroll_px_delta = Vec2::ZERO;
    for ev in wheel_ev.read() {
        match ev.unit {
            MouseScrollUnit::Line => {
                zoom_steps += ev.y;
                scroll_px_delta += Vec2::new(ev.x * 28.0, ev.y * 28.0);
            }
            MouseScrollUnit::Pixel => {
                zoom_steps += ev.y / 36.0;
                scroll_px_delta += Vec2::new(ev.x, ev.y);
            }
        }
    }

    let mut pinch_delta = 0.0_f32;
    for ev in pinch_ev.read() {
        pinch_delta += ev.0;
    }

    let mut rot_delta = 0.0_f32;
    for ev in rot_ev.read() {
        rot_delta += ev.0;
    }

    // Track Right-Button drag threshold in Play Skenario so short Right-Click (< 5 px)
    // issues an RTS Move Order cleanly without nudging camera orbit/pitch.
    if mouse_buttons.just_pressed(MouseButton::Right) {
        // Layar Besar is 100% locked against all local pointer & keyboard camera manipulation.
    if !in_big_screen && !ui_state.wants_pointer {
            *right_drag_origin = window.cursor_position();
            *right_orbiting = !in_play_mode;
        } else {
            *right_drag_origin = None;
            *right_orbiting = false;
        }
    } else if !mouse_buttons.pressed(MouseButton::Right) {
        *right_drag_origin = None;
        *right_orbiting = false;
    } else if in_play_mode
        && !*right_orbiting
        && let (Some(origin_px), Some(curr_px)) = (*right_drag_origin, window.cursor_position())
        && origin_px.distance(curr_px) >= 5.0
    {
        *right_orbiting = true;
    }

    let viewport_h = camera
        .logical_viewport_rect()
        .map(|r| r.height())
        .unwrap_or_else(|| window.height())
        .max(200.0);

    // Direction 1: Sync Form Inputs -> Live 3D Map Camera (in Tambah/Edit Skenario preview)
    if in_preview {
        state.active_map_mode = state.skenario_table.form_map_mode;
        state.active_base_layer = state.skenario_table.form_base_layer;
        state.active_skybox = state.skenario_table.form_skybox;

        if state.skenario_table.map_preview_dirty {
            state.skenario_table.clamp_zoom_bounds();
            let form_lng = state.skenario_table.form_center_lng;
            let form_lat = state.skenario_table.form_center_lat;
            let form_zoom = state.skenario_table.form_zoom;
            let form_pitch = state.skenario_table.form_pitch;
            let form_bearing = state.skenario_table.form_bearing;
            let snap = state.skenario_table.map_preview_snap;

            state.skenario_table.map_preview_dirty = ui_state.viewport_rect_logical.is_none();
            state.skenario_table.map_preview_snap = ui_state.viewport_rect_logical.is_none();

            let anchor_dist_m = haversine_distance_m(
                state.geo_anchor.center_lng,
                state.geo_anchor.center_lat,
                form_lng,
                form_lat,
            );
            if snap || anchor_dist_m > 30_000.0 {
                state.geo_anchor = GeoAnchor::new(form_lng, form_lat);
                tcam.target_local = Vec2::ZERO;
                if snap {
                    tcam.smooth_target = Vec2::ZERO;
                }
            } else {
                let local = state.geo_anchor.lng_lat_alt_to_local(form_lng, form_lat, 0.0);
                tcam.target_local = Vec2::new(local[0], local[2]);
            }

            let dist = camera_distance_for_zoom(
                form_zoom,
                viewport_h,
                45.0_f32.to_radians(),
                state.geo_anchor.cos_lat,
            );
            tcam.distance_m = dist;
            tcam.pitch_deg = (form_pitch as f32).clamp(5.0, 82.0);
            tcam.bearing_deg = (form_bearing as f32).rem_euclid(360.0);

            if snap {
                tcam.smooth_distance = tcam.distance_m;
                tcam.smooth_pitch = tcam.pitch_deg;
                tcam.smooth_bearing = tcam.bearing_deg;
            }
        }
    }

    // Update cursor lat/lng readout
    if !ui_state.wants_pointer
        && let Some(hit) = cursor_world_ground(&window, camera, cam_gtf)
    {
        let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
        state.cursor_lnglat = Some([lng, lat]);
    }

    // Compute camera distance bounds matching [min_zoom, max_zoom] (up to 26.0)
    let (min_zoom, max_zoom) = if in_preview {
        let min_z = state.skenario_table.form_min_zoom.clamp(1.0, 26.0);
        let max_z = state.skenario_table.form_max_zoom.clamp(min_z, 26.0);
        (min_z, max_z)
    } else if let Some(sk) = &state.active_skenario {
        let min_z = sk.min_zoom.clamp(1.0, 26.0);
        let max_z = sk.max_zoom.clamp(min_z, 26.0);
        (min_z, max_z)
    } else {
        (1.0, 26.0)
    };

    let d_at_max_zoom = camera_distance_for_zoom(
        max_zoom,
        viewport_h,
        45.0_f32.to_radians(),
        state.geo_anchor.cos_lat,
    );
    let d_at_min_zoom = camera_distance_for_zoom(
        min_zoom,
        viewport_h,
        45.0_f32.to_radians(),
        state.geo_anchor.cos_lat,
    );
    let dist_lo = d_at_max_zoom.min(d_at_min_zoom);
    let dist_hi = d_at_max_zoom.max(d_at_min_zoom);

    let dt = time.delta_secs().min(0.1);
    let mut camera_changed_by_user = false;

    // Layar Besar is 100% locked against all local pointer & keyboard camera manipulation.
    if !in_big_screen && !ui_state.wants_pointer {
        let space_held = !ui_state.wants_keyboard && keys.pressed(KeyCode::Space);
        let alt_held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
        let ctrl_or_cmd = keys.pressed(KeyCode::ControlLeft)
            || keys.pressed(KeyCode::ControlRight)
            || keys.pressed(KeyCode::SuperLeft)
            || keys.pressed(KeyCode::SuperRight);
        let no_active_edit_drag = !gizmo_state.is_dragging()
            && !poly_edit.is_dragging()
            && !route_edit.is_dragging()
            && state.dragging_catalog_entry.is_none()
            && state.dragging_basic_mesh.is_none();

        let b = tcam.bearing_deg.to_radians();
        let right = Vec2::new(b.cos(), b.sin());
        let forward = Vec2::new(b.sin(), -b.cos());
        let meters_per_px = (tcam.distance_m * 0.95) / viewport_h;

        // Native macOS trackpad pinch-to-zoom & two-finger rotation gestures (always active)
        if pinch_delta.abs() > 1e-5 {
            let factor = (1.0 - pinch_delta * 1.35).clamp(0.45, 1.85);
            tcam.distance_m = (tcam.distance_m * factor).clamp(dist_lo, dist_hi);
            camera_changed_by_user = true;
        }
        if rot_delta.abs() > 1e-5 {
            tcam.bearing_deg = (tcam.bearing_deg - rot_delta.to_degrees()).rem_euclid(360.0);
            camera_changed_by_user = true;
        }

        // Scroll wheel / Trackpad 2-finger scroll
        if state.trackpad_mode {
            if ctrl_or_cmd && zoom_steps.abs() > 1e-4 {
                let factor = (1.0 - zoom_steps * 0.14).clamp(0.4, 1.8);
                tcam.distance_m = (tcam.distance_m * factor).clamp(dist_lo, dist_hi);
                camera_changed_by_user = true;
            } else if alt_held && scroll_px_delta.length_squared() > 1e-4 {
                tcam.bearing_deg =
                    (tcam.bearing_deg - scroll_px_delta.x * 0.28).rem_euclid(360.0);
                tcam.pitch_deg = (tcam.pitch_deg - scroll_px_delta.y * 0.22).clamp(5.0, 82.0);
                camera_changed_by_user = true;
            } else if scroll_px_delta.length_squared() > 1e-4 {
                tcam.target_local -= right * (scroll_px_delta.x * meters_per_px * 1.15);
                tcam.target_local -= forward * (scroll_px_delta.y * meters_per_px * 1.15);
                camera_changed_by_user = true;
            }
        } else if zoom_steps.abs() > 1e-4 {
            let factor = (1.0 - zoom_steps * 0.12).clamp(0.4, 1.8);
            tcam.distance_m = (tcam.distance_m * factor).clamp(dist_lo, dist_hi);
            camera_changed_by_user = true;
        }

        // In Play Skenario, plain Left-Drag is reserved for RTS Box Selection (not camera pan).
        // Camera pan uses Middle-Drag, Space + Left-Drag, 2-finger Trackpad Scroll, or W/A/S/D.
        let allow_plain_left_pan = in_preview
            || (!in_play_mode
                && state.tool_mode.is_transform_tool()
                && no_active_edit_drag
                && !gizmo_state.suppress_camera_pan
                && !keys.pressed(KeyCode::ShiftLeft)
                && !keys.pressed(KeyCode::ShiftRight)
                && !alt_held);
        let space_left_pan =
            space_held && no_active_edit_drag && mouse_buttons.pressed(MouseButton::Left);

        if (mouse_buttons.pressed(MouseButton::Middle)
            || space_left_pan
            || (allow_plain_left_pan && mouse_buttons.pressed(MouseButton::Left)))
            && total_motion.length_squared() > 0.0
        {
            tcam.target_local -= right * (total_motion.x * meters_per_px);
            tcam.target_local -= forward * (total_motion.y * meters_per_px);
            camera_changed_by_user = true;
        }

        // Orbit / Pitch: Right-Drag (with >= 5 px threshold in Play Skenario) OR Alt/Option + Left-Drag
        let alt_left_orbit =
            alt_held && no_active_edit_drag && mouse_buttons.pressed(MouseButton::Left);
        let right_drag_orbit =
            mouse_buttons.pressed(MouseButton::Right) && (!in_play_mode || *right_orbiting);

        if (right_drag_orbit || alt_left_orbit) && total_motion.length_squared() > 0.0 {
            tcam.bearing_deg = (tcam.bearing_deg - total_motion.x * 0.32).rem_euclid(360.0);
            tcam.pitch_deg = (tcam.pitch_deg - total_motion.y * 0.25).clamp(5.0, 82.0);
            camera_changed_by_user = true;
        }
    }

    if !in_big_screen && !ui_state.wants_keyboard {
        let pan_speed = tcam.distance_m * 0.75 * dt;
        let b = tcam.bearing_deg.to_radians();
        let right = Vec2::new(b.cos(), b.sin());
        let forward = Vec2::new(b.sin(), -b.cos());

        if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
            tcam.target_local += forward * pan_speed;
            camera_changed_by_user = true;
        }
        if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
            tcam.target_local -= forward * pan_speed;
            camera_changed_by_user = true;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            tcam.target_local += right * pan_speed;
            camera_changed_by_user = true;
        }
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            tcam.target_local -= right * pan_speed;
            camera_changed_by_user = true;
        }
        if keys.pressed(KeyCode::KeyQ) {
            tcam.bearing_deg = (tcam.bearing_deg - 60.0 * dt).rem_euclid(360.0);
            camera_changed_by_user = true;
        }
        if keys.pressed(KeyCode::KeyE) {
            tcam.bearing_deg = (tcam.bearing_deg + 60.0 * dt).rem_euclid(360.0);
            camera_changed_by_user = true;
        }
        if keys.just_pressed(KeyCode::KeyR) {
            if in_preview {
                tcam.pitch_deg = 48.0;
                tcam.bearing_deg = 0.0;
                camera_changed_by_user = true;
            } else if let Some(sk) = &state.active_skenario {
                tcam.apply_skenario_view(sk.zoom, sk.pitch, sk.bearing, state.geo_anchor.cos_lat);
            }
        }
    }

    tcam.distance_m = tcam.distance_m.clamp(dist_lo, dist_hi);
    tcam.user_interrupted_this_frame = camera_changed_by_user;
    if camera_changed_by_user && tcam.target_alt_m.abs() > 1e-3 {
        // Gently bring elevated camera focus back to ground level when user manually pans
        tcam.target_alt_m *= (1.0 - 4.0 * dt).max(0.0);
        if tcam.target_alt_m.abs() < 0.05 {
            tcam.target_alt_m = 0.0;
        }
    }

    // Broadcast throttled camera_sync (~25 Hz) to Layar Besar ONLY when:
    // - Play Skenario was opened from Tactical (`TacticalLive`), AND
    // - User is authorized (`admin` for all scenarios, `admin_satker` for their satker's scenario).
    if in_play_mode
        && state.is_tactical_live()
        && !state.offline_preview
        && state.can_control_active_big_screen()
        && let Some(sk) = &state.active_skenario
    {
        let now_sec = time.elapsed_secs_f64();
        let (last_t, last_target, last_dist, last_pitch, last_bearing) = *last_cam_sync;
        let moved = tcam.target_local.distance(last_target) > 0.05
            || (tcam.distance_m - last_dist).abs() > 0.05
            || (tcam.pitch_deg - last_pitch).abs() > 0.05
            || (tcam.bearing_deg - last_bearing).abs() > 0.05;
        if moved && now_sec - last_t >= 0.04 {
            let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                tcam.target_local.x,
                0.0,
                tcam.target_local.y,
            ]);
            let zoom = tcam
                .current_zoom(viewport_h, state.geo_anchor.cos_lat)
                .clamp(min_zoom, max_zoom);
            ws.send(WsOutCommand::SendCommand(TacticalCommand {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: None,
                command: "camera_sync".to_string(),
                payload: serde_json::json!({
                    "lng": lng,
                    "lat": lat,
                    "alt": tcam.target_alt_m,
                    "zoom": zoom,
                    "pitch": tcam.pitch_deg,
                    "bearing": tcam.bearing_deg,
                }),
                save_history: false,
            }));
            *last_cam_sync = (
                now_sec,
                tcam.target_local,
                tcam.distance_m,
                tcam.pitch_deg,
                tcam.bearing_deg,
            );
        }
    }

    // Direction 2: Sync Live 3D Map Camera -> Form Inputs when user pans/zooms/pitches/rotates!
    if in_preview && camera_changed_by_user {
        let (mut lng, mut lat, _) = state
            .geo_anchor
            .local_to_lng_lat_alt([tcam.target_local.x, 0.0, tcam.target_local.y]);
        lat = lat.clamp(-85.0, 85.0);
        lng = ((lng + 180.0).rem_euclid(360.0)) - 180.0;

        // Re-anchor smoothly if panned very far so local metric coordinates stay crisp
        if tcam.target_local.length() > 150_000.0 {
            state.geo_anchor = GeoAnchor::new(lng, lat);
            tcam.target_local = Vec2::ZERO;
            tcam.smooth_target = Vec2::ZERO;
        }

        let zoom = tcam
            .current_zoom(viewport_h, state.geo_anchor.cos_lat)
            .clamp(min_zoom, max_zoom);

        state.skenario_table.form_center_lng = (lng * 100_000.0).round() / 100_000.0;
        state.skenario_table.form_center_lat = (lat * 100_000.0).round() / 100_000.0;
        state.skenario_table.form_zoom = (zoom * 100.0).round() / 100.0;
        state.skenario_table.form_pitch = ((tcam.pitch_deg as f64) * 10.0).round() / 10.0;
        state.skenario_table.form_bearing =
            ((tcam.bearing_deg.rem_euclid(360.0) as f64) * 10.0).round() / 10.0;
    }

    // Smooth exponential damping (with shortest-arc bearing interpolation)
    let alpha = 1.0 - (-14.0 * dt).exp();
    tcam.smooth_target = tcam.smooth_target.lerp(tcam.target_local, alpha);
    tcam.smooth_alt_m += (tcam.target_alt_m - tcam.smooth_alt_m) * alpha;
    tcam.smooth_distance += (tcam.distance_m - tcam.smooth_distance) * alpha;
    tcam.smooth_pitch += (tcam.pitch_deg - tcam.smooth_pitch) * alpha;

    let mut bearing_delta = (tcam.bearing_deg - tcam.smooth_bearing) % 360.0;
    if bearing_delta > 180.0 {
        bearing_delta -= 360.0;
    } else if bearing_delta < -180.0 {
        bearing_delta += 360.0;
    }
    tcam.smooth_bearing = (tcam.smooth_bearing + bearing_delta * alpha).rem_euclid(360.0);

    tcam.apply_transform(&mut tf);
}
