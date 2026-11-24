use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use twg_shared::geo::{
    GeoAnchor, camera_distance_for_zoom, haversine_distance_m, zoom_for_camera_distance,
};

use crate::{
    scene::{gizmo::TransformGizmoState, polygon_edit::PolygonEditState},
    state::{AppScreen, AppState, UiPointerState},
};

#[derive(Component)]
pub struct TacticalCamera {
    pub target_local: Vec2,
    pub distance_m: f32,
    pub pitch_deg: f32,
    pub bearing_deg: f32,

    pub smooth_target: Vec2,
    pub smooth_distance: f32,
    pub smooth_pitch: f32,
    pub smooth_bearing: f32,
}

impl Default for TacticalCamera {
    fn default() -> Self {
        Self {
            target_local: Vec2::ZERO,
            distance_m: 650.0,
            pitch_deg: 48.0,
            bearing_deg: 0.0,
            smooth_target: Vec2::ZERO,
            smooth_distance: 650.0,
            smooth_pitch: 48.0,
            smooth_bearing: 0.0,
        }
    }
}

impl TacticalCamera {
    pub fn apply_skenario_view(&mut self, zoom: f64, pitch: f64, bearing: f64, cos_lat: f64) {
        let dist = camera_distance_for_zoom(zoom, 820.0, 45.0_f32.to_radians(), cos_lat);
        self.target_local = Vec2::ZERO;
        self.smooth_target = Vec2::ZERO;
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

#[allow(clippy::too_many_arguments)]
pub fn camera_control_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut motion_ev: MessageReader<MouseMotion>,
    mut wheel_ev: MessageReader<MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_state: Res<UiPointerState>,
    gizmo_state: Res<TransformGizmoState>,
    poly_edit: Res<PolygonEditState>,
    mut state: ResMut<AppState>,
    mut cam_q: Query<(&mut TacticalCamera, &mut Transform, &Camera, &GlobalTransform)>,
) {
    let Ok((mut tcam, mut tf, camera, cam_gtf)) = cam_q.single_mut() else {
        return;
    };

    let in_preview = state.is_skenario_map_preview_active();
    let in_plot = state.screen == AppScreen::PlotSkenario;

    if !in_preview && !in_plot {
        motion_ev.clear();
        wheel_ev.clear();
        return;
    }

    let mut total_motion = Vec2::ZERO;
    for ev in motion_ev.read() {
        total_motion += ev.delta;
    }

    let mut scroll_y = 0.0_f32;
    for ev in wheel_ev.read() {
        scroll_y += ev.y;
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

    if !ui_state.wants_pointer {
        if scroll_y.abs() > 0.0 {
            let factor = (1.0 - scroll_y * 0.12).clamp(0.4, 1.8);
            tcam.distance_m = (tcam.distance_m * factor).clamp(dist_lo, dist_hi);
            camera_changed_by_user = true;
        }

        let allow_left_pan = in_preview
            || (state.tool_mode.is_transform_tool()
                && !gizmo_state.is_dragging()
                && !poly_edit.is_dragging()
                && !gizmo_state.suppress_camera_pan
                && state.dragging_catalog_entry.is_none()
                && state.dragging_basic_mesh.is_none()
                && !keys.pressed(KeyCode::ShiftLeft)
                && !keys.pressed(KeyCode::ShiftRight));

        if (mouse_buttons.pressed(MouseButton::Middle)
            || (allow_left_pan && mouse_buttons.pressed(MouseButton::Left)))
            && total_motion.length_squared() > 0.0
        {
            let b = tcam.bearing_deg.to_radians();
            let right = Vec2::new(b.cos(), b.sin());
            let forward = Vec2::new(b.sin(), -b.cos());
            let meters_per_px = (tcam.distance_m * 0.95) / viewport_h;
            tcam.target_local -= right * (total_motion.x * meters_per_px);
            tcam.target_local -= forward * (total_motion.y * meters_per_px);
            camera_changed_by_user = true;
        }

        if mouse_buttons.pressed(MouseButton::Right) && total_motion.length_squared() > 0.0 {
            tcam.bearing_deg = (tcam.bearing_deg - total_motion.x * 0.32).rem_euclid(360.0);
            tcam.pitch_deg = (tcam.pitch_deg - total_motion.y * 0.25).clamp(5.0, 82.0);
            camera_changed_by_user = true;
        }
    }

    if !ui_state.wants_keyboard {
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
    tcam.smooth_distance += (tcam.distance_m - tcam.smooth_distance) * alpha;
    tcam.smooth_pitch += (tcam.pitch_deg - tcam.smooth_pitch) * alpha;

    let mut bearing_delta = (tcam.bearing_deg - tcam.smooth_bearing) % 360.0;
    if bearing_delta > 180.0 {
        bearing_delta -= 360.0;
    } else if bearing_delta < -180.0 {
        bearing_delta += 360.0;
    }
    tcam.smooth_bearing = (tcam.smooth_bearing + bearing_delta * alpha).rem_euclid(360.0);

    let tilt = tcam.smooth_pitch.to_radians();
    let bearing = tcam.smooth_bearing.to_radians();
    let horiz = tcam.smooth_distance * tilt.sin();
    let height = (tcam.smooth_distance * tilt.cos()).max(0.35);

    let offset = Vec3::new(-bearing.sin() * horiz, height, bearing.cos() * horiz);
    let target3 = Vec3::new(tcam.smooth_target.x, 0.0, tcam.smooth_target.y);
    tf.translation = target3 + offset;
    tf.look_at(target3, Vec3::Y);
}
