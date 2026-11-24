use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy_egui::egui;
use twg_shared::{
    models::{Anim, Marker},
    particle_anim::{is_particle_anim, particle_anim_position, set_particle_anim_position},
};
use uuid::Uuid;

use crate::state::{AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode};

pub const GIZMO_SCREEN_RADIUS_PX: f32 = 82.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Move,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoHandle {
    AxisX,
    AxisY,
    AxisZ,
    Free,
}

impl GizmoHandle {
    pub fn label(&self, mode: GizmoMode) -> &'static str {
        match (mode, self) {
            (GizmoMode::Move, Self::AxisX) => "Move X",
            (GizmoMode::Move, Self::AxisY) => "Move Y (Alt)",
            (GizmoMode::Move, Self::AxisZ) => "Move Z",
            (GizmoMode::Move, Self::Free) => "Free Move (XZ)",
            (GizmoMode::Rotate, Self::AxisX) => "Rotate X",
            (GizmoMode::Rotate, Self::AxisY) => "Rotate Y",
            (GizmoMode::Rotate, Self::AxisZ) => "Rotate Z",
            (GizmoMode::Rotate, Self::Free) => "Free Rotate",
            (GizmoMode::Scale, Self::AxisX) => "Scale X",
            (GizmoMode::Scale, Self::AxisY) => "Scale Y",
            (GizmoMode::Scale, Self::AxisZ) => "Scale Z",
            (GizmoMode::Scale, Self::Free) => "Free Scale (XYZ)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActiveGizmoDrag {
    pub marker_id: Uuid,
    pub is_particle_anim: bool,
    pub mode: GizmoMode,
    pub handle: GizmoHandle,
    pub start_origin: Vec3,
    pub start_quat: Quat,
    pub current_quat: Quat,
    pub start_scale: Vec3,
    pub start_cursor_px: Vec2,
    pub prev_cursor_px: Vec2,
    pub start_axis_param: f32,
    pub free_plane_offset: Vec3,
    pub axis_world: Vec3,
    pub rot_start_dir: Option<Vec3>,
    pub prev_rot_vec: Option<Vec3>,
    pub accumulated_angle_rad: f32,
    pub gizmo_radius_world: f32,
    pub visual_scale_factor: f32,
    pub moved: bool,
}

#[derive(Resource, Default)]
pub struct TransformGizmoState {
    pub hovered_handle: Option<GizmoHandle>,
    pub hovered_entity: Option<SelectedEntity>,
    pub active_drag: Option<ActiveGizmoDrag>,
    pub suppress_camera_pan: bool,
    pub empty_click_start_px: Option<Vec2>,
}

impl TransformGizmoState {
    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    pub fn cancel(&mut self) {
        self.active_drag = None;
        self.hovered_handle = None;
        self.hovered_entity = None;
        self.suppress_camera_pan = false;
        self.empty_click_start_px = None;
    }
}

pub fn gizmo_mode_for_tool(tool: ToolMode) -> Option<GizmoMode> {
    match tool {
        ToolMode::MoveMarker => Some(GizmoMode::Move),
        ToolMode::RotateMarker => Some(GizmoMode::Rotate),
        ToolMode::ScaleMarker => Some(GizmoMode::Scale),
        _ => None,
    }
}

pub fn marker_quat(m: &Marker) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        (m.rot_y as f32).to_radians(),
        (m.rot_x as f32).to_radians(),
        (m.rot_z as f32).to_radians(),
    )
}

pub fn marker_world_origin(state: &AppState, m: &Marker) -> Vec3 {
    Vec3::from_array(
        state
            .geo_anchor
            .lng_lat_alt_to_local(m.lng, m.lat, m.altitude),
    )
}

pub fn particle_anim_world_origin(state: &AppState, anim: &Anim) -> Vec3 {
    let p = particle_anim_position(anim);
    Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], p[2]))
}

pub fn compute_gizmo_world_radius(
    camera: &Camera,
    cam_gtf: &GlobalTransform,
    origin: Vec3,
    target_px: f32,
) -> f32 {
    let cam_pos = cam_gtf.translation();
    let cam_forward = cam_gtf.forward().as_vec3();
    let depth = (origin - cam_pos).dot(cam_forward).max(0.25);
    let viewport_h = camera
        .logical_viewport_rect()
        .map(|r| r.height())
        .unwrap_or(820.0)
        .max(120.0);
    let fov_y = 45.0_f32.to_radians();
    let world_per_px = (2.0 * depth * (fov_y * 0.5).tan()) / viewport_h;
    (world_per_px * target_px).max(0.05)
}

pub fn gizmo_axes(mode: GizmoMode, rot_quat: Quat) -> (Vec3, Vec3, Vec3) {
    match mode {
        GizmoMode::Move => (Vec3::X, Vec3::Y, Vec3::Z),
        GizmoMode::Rotate | GizmoMode::Scale => (
            (rot_quat * Vec3::X).normalize_or(Vec3::X),
            (rot_quat * Vec3::Y).normalize_or(Vec3::Y),
            (rot_quat * Vec3::Z).normalize_or(Vec3::Z),
        ),
    }
}

/// Closest parameter `s` along the line `axis_origin + s * axis_dir` to `ray`.
pub fn closest_axis_param(ray: Ray3d, axis_origin: Vec3, axis_dir: Vec3) -> Option<f32> {
    let rd = ray.direction.as_vec3();
    let w = ray.origin - axis_origin;
    let b = axis_dir.dot(rd);
    let denom = 1.0 - b * b;
    if denom.abs() < 1e-4 {
        return None;
    }
    let d = axis_dir.dot(w);
    let e = rd.dot(w);
    Some((d - b * e) / denom)
}

pub fn intersect_ray_plane(ray: Ray3d, plane_origin: Vec3, plane_normal: Vec3) -> Option<Vec3> {
    let rd = ray.direction.as_vec3();
    let denom = rd.dot(plane_normal);
    if denom.abs() < 1e-4 {
        return None;
    }
    let t = (plane_origin - ray.origin).dot(plane_normal) / denom;
    if t <= 0.0 {
        return None;
    }
    Some(ray.origin + rd * t)
}

pub fn smooth_scale_factor(delta_px: f32, pixels_per_2x: f32) -> f32 {
    if delta_px >= 0.0 {
        (1.0 + delta_px / pixels_per_2x).clamp(0.05, 25.0)
    } else {
        (1.0 / (1.0 - delta_px / pixels_per_2x)).clamp(0.04, 1.0)
    }
}

fn round_clean(val: f64, decimals: i32) -> f64 {
    let factor = 10_f64.powi(decimals);
    let r = (val * factor).round() / factor;
    if r.abs() < 1e-9 { 0.0 } else { r }
}

fn point_to_segment_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 1e-5 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn ring_basis(axis: Vec3, fallback_u: Vec3) -> (Vec3, Vec3) {
    let u = (fallback_u - axis * fallback_u.dot(axis))
        .try_normalize()
        .unwrap_or_else(|| {
            let alt = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
            axis.cross(alt).normalize()
        });
    let v = axis.cross(u).normalize();
    (u, v)
}

/// Pixel-accurate screen-space hit-test against the active Move, Rotate, or Scale gizmo.
#[allow(clippy::too_many_arguments)]
pub fn hit_test_gizmo(
    cursor_px: Vec2,
    mode: GizmoMode,
    origin: Vec3,
    rot_quat: Quat,
    radius: f32,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<GizmoHandle> {
    let origin_2d = camera.world_to_viewport(cam_gtf, origin).ok()?;
    let (ax, ay, az) = gizmo_axes(mode, rot_quat);

    match mode {
        GizmoMode::Move | GizmoMode::Scale => {
            // Center Free Move / Free Uniform Scale handle has top priority at origin
            let center_hit_r = if mode == GizmoMode::Scale { 15.0 } else { 14.0 };
            if cursor_px.distance(origin_2d) <= center_hit_r {
                return Some(GizmoHandle::Free);
            }

            let mut best: Option<(GizmoHandle, f32)> = None;
            for (handle, dir) in [
                (GizmoHandle::AxisX, ax),
                (GizmoHandle::AxisY, ay),
                (GizmoHandle::AxisZ, az),
            ] {
                let Ok(tip_2d) = camera.world_to_viewport(cam_gtf, origin + dir * radius) else {
                    continue;
                };
                if tip_2d.distance(origin_2d) < 8.0 {
                    continue;
                }
                let shaft_dir = (tip_2d - origin_2d).normalize();
                let start_2d = origin_2d + shaft_dir * 12.0;
                let dist_shaft = point_to_segment_dist(cursor_px, start_2d, tip_2d);
                let dist_tip = cursor_px.distance(tip_2d);
                let eff_dist = dist_shaft.min(dist_tip * 0.85);
                if eff_dist <= 13.0 && best.is_none_or(|b| eff_dist < b.1) {
                    best = Some((handle, eff_dist));
                }
            }
            best.map(|b| b.0)
        }
        GizmoMode::Rotate => {
            // Center Free Rotate trackball orb sits inside the 3 rotation rings
            if cursor_px.distance(origin_2d) <= 17.5 {
                return Some(GizmoHandle::Free);
            }

            let to_cam = (cam_gtf.translation() - origin).normalize_or(Vec3::Y);
            const SEGMENTS: usize = 64;
            let mut best: Option<(GizmoHandle, f32)> = None;

            for (handle, axis, ref_u) in [
                (GizmoHandle::AxisX, ax, ay),
                (GizmoHandle::AxisY, ay, ax),
                (GizmoHandle::AxisZ, az, ax),
            ] {
                let (u, v) = ring_basis(axis, ref_u);
                let mut prev_pt3 = origin + u * radius;
                let mut prev_pt2 = camera.world_to_viewport(cam_gtf, prev_pt3).ok();

                for i in 1..=SEGMENTS {
                    let ang = (i as f32 / SEGMENTS as f32) * TAU;
                    let curr_pt3 = origin + (u * ang.cos() + v * ang.sin()) * radius;
                    let curr_pt2 = camera.world_to_viewport(cam_gtf, curr_pt3).ok();
                    if let (Some(a), Some(b)) = (prev_pt2, curr_pt2) {
                        let mid3 = (prev_pt3 + curr_pt3) * 0.5;
                        let front = (mid3 - origin).normalize_or_zero().dot(to_cam);
                        let raw_d = point_to_segment_dist(cursor_px, a, b);
                        // Prefer front-facing ring segments cleanly over occluded back arcs
                        let score = if front >= -0.15 { raw_d } else { raw_d + 4.5 };
                        if raw_d <= 11.0 && best.is_none_or(|cur| score < cur.1) {
                            best = Some((handle, score));
                        }
                    }
                    prev_pt3 = curr_pt3;
                    prev_pt2 = curr_pt2;
                }
            }
            best.map(|b| b.0)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn begin_gizmo_drag(
    marker: &Marker,
    mode: GizmoMode,
    handle: GizmoHandle,
    cursor_px: Vec2,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<ActiveGizmoDrag> {
    let start_origin = marker_world_origin(state, marker);
    let start_quat = marker_quat(marker);
    let start_scale = Vec3::new(
        marker.scale_x as f32,
        marker.scale_y as f32,
        marker.scale_z as f32,
    );
    let gizmo_radius_world =
        compute_gizmo_world_radius(camera, cam_gtf, start_origin, GIZMO_SCREEN_RADIUS_PX);
    let (ax, ay, az) = gizmo_axes(mode, start_quat);
    let axis_world = match handle {
        GizmoHandle::AxisX => ax,
        GizmoHandle::AxisY => ay,
        GizmoHandle::AxisZ => az,
        GizmoHandle::Free => Vec3::Y,
    };

    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let start_axis_param = closest_axis_param(ray, start_origin, axis_world).unwrap_or(0.0);

    let free_plane_offset = if mode == GizmoMode::Move && handle == GizmoHandle::Free {
        let hit = intersect_ray_plane(ray, start_origin, Vec3::Y).or_else(|| {
            let cam_fwd = cam_gtf.forward().as_vec3();
            intersect_ray_plane(ray, start_origin, -cam_fwd).map(|p| Vec3::new(p.x, start_origin.y, p.z))
        });
        hit.map(|h| start_origin - h).unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO
    };

    let rot_start_dir = if mode == GizmoMode::Rotate && handle != GizmoHandle::Free {
        intersect_ray_plane(ray, start_origin, axis_world)
            .and_then(|h| (h - start_origin).try_normalize())
    } else {
        None
    };

    Some(ActiveGizmoDrag {
        marker_id: marker.id,
        is_particle_anim: false,
        mode,
        handle,
        start_origin,
        start_quat,
        current_quat: start_quat,
        start_scale,
        start_cursor_px: cursor_px,
        prev_cursor_px: cursor_px,
        start_axis_param,
        free_plane_offset,
        axis_world,
        rot_start_dir,
        prev_rot_vec: rot_start_dir,
        accumulated_angle_rad: 0.0,
        gizmo_radius_world,
        visual_scale_factor: 1.0,
        moved: false,
    })
}

pub fn begin_particle_move_drag(
    anim: &Anim,
    handle: GizmoHandle,
    cursor_px: Vec2,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<ActiveGizmoDrag> {
    let start_origin = particle_anim_world_origin(state, anim);
    let gizmo_radius_world =
        compute_gizmo_world_radius(camera, cam_gtf, start_origin, GIZMO_SCREEN_RADIUS_PX);
    let axis_world = match handle {
        GizmoHandle::AxisX => Vec3::X,
        GizmoHandle::AxisY => Vec3::Y,
        GizmoHandle::AxisZ => Vec3::Z,
        GizmoHandle::Free => Vec3::Y,
    };
    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let start_axis_param = closest_axis_param(ray, start_origin, axis_world).unwrap_or(0.0);
    let free_plane_offset = if handle == GizmoHandle::Free {
        let hit = intersect_ray_plane(ray, start_origin, Vec3::Y).or_else(|| {
            let cam_fwd = cam_gtf.forward().as_vec3();
            intersect_ray_plane(ray, start_origin, -cam_fwd)
                .map(|p| Vec3::new(p.x, start_origin.y, p.z))
        });
        hit.map(|h| start_origin - h).unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO
    };

    Some(ActiveGizmoDrag {
        marker_id: anim.id,
        is_particle_anim: true,
        mode: GizmoMode::Move,
        handle,
        start_origin,
        start_quat: Quat::IDENTITY,
        current_quat: Quat::IDENTITY,
        start_scale: Vec3::ONE,
        start_cursor_px: cursor_px,
        prev_cursor_px: cursor_px,
        start_axis_param,
        free_plane_offset,
        axis_world,
        rot_start_dir: None,
        prev_rot_vec: None,
        accumulated_angle_rad: 0.0,
        gizmo_radius_world,
        visual_scale_factor: 1.0,
        moved: false,
    })
}

pub fn update_gizmo_drag(
    drag: &mut ActiveGizmoDrag,
    cursor_px: Vec2,
    state: &mut AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> bool {
    let Ok(ray) = camera.viewport_to_world(cam_gtf, cursor_px) else {
        return false;
    };
    let delta_px = cursor_px - drag.prev_cursor_px;
    let total_px = cursor_px - drag.start_cursor_px;
    if !drag.moved && total_px.length_squared() < 2.25 {
        drag.prev_cursor_px = cursor_px;
        return false;
    }
    let cam_fwd = cam_gtf.forward().as_vec3();

    // Dedicated Move Gizmo branch for Particle Animations (Lng / Lat / Alt)
    if drag.is_particle_anim {
        let new_pos_opt = match drag.handle {
            GizmoHandle::AxisX | GizmoHandle::AxisY | GizmoHandle::AxisZ => {
                let p = if let Some(s_curr) =
                    closest_axis_param(ray, drag.start_origin, drag.axis_world)
                {
                    drag.start_origin + drag.axis_world * (s_curr - drag.start_axis_param)
                } else if let (Ok(o2), Ok(t2)) = (
                    camera.world_to_viewport(cam_gtf, drag.start_origin),
                    camera.world_to_viewport(
                        cam_gtf,
                        drag.start_origin + drag.axis_world * drag.gizmo_radius_world,
                    ),
                ) {
                    let s_dir = (t2 - o2).normalize_or(Vec2::X);
                    let proj = total_px.dot(s_dir);
                    let world_delta = (proj / GIZMO_SCREEN_RADIUS_PX) * drag.gizmo_radius_world;
                    drag.start_origin + drag.axis_world * world_delta
                } else {
                    drag.start_origin
                };
                Some(Vec3::new(p.x, p.y.clamp(-50.0, 2000.0), p.z))
            }
            GizmoHandle::Free => {
                let hit = intersect_ray_plane(ray, drag.start_origin, Vec3::Y).or_else(|| {
                    intersect_ray_plane(ray, drag.start_origin, -cam_fwd)
                        .map(|p| Vec3::new(p.x, drag.start_origin.y, p.z))
                });
                hit.map(|h| {
                    let mut p = h + drag.free_plane_offset;
                    p.y = drag.start_origin.y;
                    p
                })
            }
        };

        let mut moved_now = false;
        if let Some(new_pos) = new_pos_opt {
            let (lng, lat, alt) = state
                .geo_anchor
                .local_to_lng_lat_alt([new_pos.x, new_pos.y, new_pos.z]);
            let clean_alt = round_clean(alt, 2);
            if let Some(anim) = state.anims.iter_mut().find(|a| a.id == drag.marker_id) {
                set_particle_anim_position(anim, lng, lat, clean_alt);
                let updated = anim.clone();
                if state
                    .anim_edit
                    .as_ref()
                    .is_some_and(|e| e.id == drag.marker_id)
                {
                    state.anim_edit = Some(updated);
                }
                drag.moved = true;
                moved_now = true;
            }
        }
        drag.prev_cursor_px = cursor_px;
        return moved_now;
    }

    let Some(m) = state.markers.iter_mut().find(|x| x.id == drag.marker_id) else {
        return false;
    };

    let mut changed = false;

    match (drag.mode, drag.handle) {
        (GizmoMode::Move, GizmoHandle::AxisX | GizmoHandle::AxisY | GizmoHandle::AxisZ) => {
            let new_pos = if let Some(s_curr) =
                closest_axis_param(ray, drag.start_origin, drag.axis_world)
            {
                drag.start_origin + drag.axis_world * (s_curr - drag.start_axis_param)
            } else if let (Ok(o2), Ok(t2)) = (
                camera.world_to_viewport(cam_gtf, drag.start_origin),
                camera.world_to_viewport(
                    cam_gtf,
                    drag.start_origin + drag.axis_world * drag.gizmo_radius_world,
                ),
            ) {
                let s_dir = (t2 - o2).normalize_or(Vec2::X);
                let proj = total_px.dot(s_dir);
                let world_delta = (proj / GIZMO_SCREEN_RADIUS_PX) * drag.gizmo_radius_world;
                drag.start_origin + drag.axis_world * world_delta
            } else {
                drag.start_origin
            };

            let clamped_y = new_pos.y.clamp(-100.0, 20_000.0);
            let (lng, lat, alt) = state
                .geo_anchor
                .local_to_lng_lat_alt([new_pos.x, clamped_y, new_pos.z]);
            m.lng = lng;
            m.lat = lat;
            m.altitude = round_clean(alt, 2);
            changed = true;
        }
        (GizmoMode::Move, GizmoHandle::Free) => {
            let hit = intersect_ray_plane(ray, drag.start_origin, Vec3::Y).or_else(|| {
                intersect_ray_plane(ray, drag.start_origin, -cam_fwd)
                    .map(|p| Vec3::new(p.x, drag.start_origin.y, p.z))
            });
            if let Some(hit) = hit {
                let mut new_pos = hit + drag.free_plane_offset;
                new_pos.y = drag.start_origin.y;
                let (lng, lat, _) =
                    state
                        .geo_anchor
                        .local_to_lng_lat_alt([new_pos.x, new_pos.y, new_pos.z]);
                m.lng = lng;
                m.lat = lat;
                changed = true;
            }
        }
        (GizmoMode::Rotate, GizmoHandle::AxisX | GizmoHandle::AxisY | GizmoHandle::AxisZ) => {
            let facing = drag.axis_world.dot(cam_fwd).abs();
            let mut stepped = false;
            if facing > 0.22
                && let Some(hit) = intersect_ray_plane(ray, drag.start_origin, drag.axis_world)
                && let Some(curr_vec) = (hit - drag.start_origin).try_normalize()
            {
                if let Some(prev_vec) = drag.prev_rot_vec {
                    let sin_a = prev_vec.cross(curr_vec).dot(drag.axis_world);
                    let cos_a = prev_vec.dot(curr_vec);
                    let step = sin_a.atan2(cos_a);
                    if step.is_finite() {
                        drag.accumulated_angle_rad += step;
                        stepped = true;
                    }
                }
                drag.prev_rot_vec = Some(curr_vec);
            }

            if !stepped && delta_px.length_squared() > 0.0 {
                // Edge-on fallback: drag perpendicular to projected axis
                let tangent = if let (Ok(o2), Ok(t2)) = (
                    camera.world_to_viewport(cam_gtf, drag.start_origin),
                    camera.world_to_viewport(
                        cam_gtf,
                        drag.start_origin + drag.axis_world * drag.gizmo_radius_world,
                    ),
                ) {
                    let s_axis = (t2 - o2).normalize_or(Vec2::Y);
                    Vec2::new(-s_axis.y, s_axis.x)
                } else {
                    Vec2::X
                };
                drag.accumulated_angle_rad += delta_px.dot(tangent) * 0.014;
                stepped = true;
            }

            if stepped {
                drag.current_quat =
                    (Quat::from_axis_angle(drag.axis_world, drag.accumulated_angle_rad)
                        * drag.start_quat)
                        .normalize();
                let (yaw, pitch, roll) = drag.current_quat.to_euler(EulerRot::YXZ);
                m.rot_y = round_clean(yaw.to_degrees() as f64, 1);
                m.rot_x = round_clean(pitch.to_degrees() as f64, 1);
                m.rot_z = round_clean(roll.to_degrees() as f64, 1);
                changed = true;
            }
        }
        (GizmoMode::Rotate, GizmoHandle::Free) => {
            if delta_px.length_squared() > 0.0 {
                let cam_right = cam_gtf.right().as_vec3();
                let cam_up = cam_gtf.up().as_vec3();
                let sens = 0.011;
                let q_yaw = Quat::from_axis_angle(cam_up, delta_px.x * sens);
                let q_pitch = Quat::from_axis_angle(cam_right, delta_px.y * sens);
                drag.current_quat = (q_yaw * q_pitch * drag.current_quat).normalize();
                let (yaw, pitch, roll) = drag.current_quat.to_euler(EulerRot::YXZ);
                m.rot_y = round_clean(yaw.to_degrees() as f64, 1);
                m.rot_x = round_clean(pitch.to_degrees() as f64, 1);
                m.rot_z = round_clean(roll.to_degrees() as f64, 1);
                changed = true;
            }
        }
        (GizmoMode::Scale, GizmoHandle::AxisX | GizmoHandle::AxisY | GizmoHandle::AxisZ) => {
            if let (Ok(o2), Ok(t2)) = (
                camera.world_to_viewport(cam_gtf, drag.start_origin),
                camera.world_to_viewport(
                    cam_gtf,
                    drag.start_origin + drag.axis_world * drag.gizmo_radius_world,
                ),
            ) {
                let screen_dir = (t2 - o2).normalize_or(Vec2::X);
                let proj_px = total_px.dot(screen_dir);
                let factor = smooth_scale_factor(proj_px, 95.0);
                drag.visual_scale_factor = factor;
                match drag.handle {
                    GizmoHandle::AxisX => {
                        m.scale_x = round_clean((drag.start_scale.x * factor).clamp(0.01, 50.0) as f64, 2);
                    }
                    GizmoHandle::AxisY => {
                        m.scale_y = round_clean((drag.start_scale.y * factor).clamp(0.01, 50.0) as f64, 2);
                    }
                    GizmoHandle::AxisZ => {
                        m.scale_z = round_clean((drag.start_scale.z * factor).clamp(0.01, 50.0) as f64, 2);
                    }
                    GizmoHandle::Free => {}
                }
                state.uniform_scale_lock = false;
                changed = true;
            }
        }
        (GizmoMode::Scale, GizmoHandle::Free) => {
            let drag_delta = total_px.x - total_px.y;
            let factor = smooth_scale_factor(drag_delta, 115.0);
            drag.visual_scale_factor = factor;
            m.scale_x = round_clean((drag.start_scale.x * factor).clamp(0.01, 50.0) as f64, 2);
            m.scale_y = round_clean((drag.start_scale.y * factor).clamp(0.01, 50.0) as f64, 2);
            m.scale_z = round_clean((drag.start_scale.z * factor).clamp(0.01, 50.0) as f64, 2);
            changed = true;
        }
    }

    if changed {
        drag.moved = true;
        if let Some(updated) = state.markers.iter().find(|x| x.id == drag.marker_id).cloned() {
            state.marker_edit = Some(updated);
        }
    }
    drag.prev_cursor_px = cursor_px;
    changed
}

// ---------------------------------------------------------------------------
// High-Craft Unity / Godot Screen-Space Projected 3D Gizmo Renderer
// ---------------------------------------------------------------------------

const COL_X: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);
const COL_Y: egui::Color32 = egui::Color32::from_rgb(34, 197, 94);
const COL_Z: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
const COL_HOT: egui::Color32 = egui::Color32::from_rgb(253, 224, 71);
const COL_OUTLINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(9, 13, 22, 185);

fn axis_color(handle: GizmoHandle, active: Option<GizmoHandle>, hovered: Option<GizmoHandle>) -> egui::Color32 {
    let base = match handle {
        GizmoHandle::AxisX => COL_X,
        GizmoHandle::AxisY => COL_Y,
        GizmoHandle::AxisZ => COL_Z,
        GizmoHandle::Free => egui::Color32::from_rgb(241, 245, 249),
    };
    if let Some(act) = active {
        if act == handle {
            return COL_HOT;
        }
        return egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 72);
    }
    if hovered == Some(handle) {
        return COL_HOT;
    }
    base
}

fn to_pos2(v: Vec2) -> egui::Pos2 {
    egui::pos2(v.x, v.y)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_transform_gizmo(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    gizmo_state: &TransformGizmoState,
    poly_edit: &crate::scene::polygon_edit::PolygonEditState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    if state.screen != AppScreen::PlotSkenario
        || !state.show_gizmos
        || playback.play_test_mode
    {
        return;
    }
    let (mode, origin, rot_quat, cur_lng, cur_lat, altitude, cur_rot, cur_scale) =
        match state.selected {
            SelectedEntity::Marker(marker_id) => {
                let Some(mode) = gizmo_mode_for_tool(state.tool_mode) else {
                    return;
                };
                if poly_edit.is_editing(marker_id) {
                    return;
                }
                let Some(marker) = state.markers.iter().find(|m| m.id == marker_id) else {
                    return;
                };
                if state.is_marker_locked(marker) {
                    return;
                }
                (
                    mode,
                    marker_world_origin(state, marker),
                    marker_quat(marker),
                    marker.lng,
                    marker.lat,
                    marker.altitude,
                    [marker.rot_x, marker.rot_y, marker.rot_z],
                    [marker.scale_x, marker.scale_y, marker.scale_z],
                )
            }
            SelectedEntity::Anim(anim_id) => {
                if !matches!(state.tool_mode, ToolMode::Select | ToolMode::MoveMarker) {
                    return;
                }
                if state.locked_entity_ids.contains(&anim_id) {
                    return;
                }
                let Some(anim) = state
                    .anims
                    .iter()
                    .find(|a| a.id == anim_id && is_particle_anim(a) && !a.geom.is_empty())
                else {
                    return;
                };
                let pos = particle_anim_position(anim);
                (
                    GizmoMode::Move,
                    particle_anim_world_origin(state, anim),
                    Quat::IDENTITY,
                    pos[0],
                    pos[1],
                    pos[2],
                    [0.0, 0.0, 0.0],
                    [1.0, 1.0, 1.0],
                )
            }
            _ => return,
        };

    let Ok(origin_2d_v) = camera.world_to_viewport(cam_gtf, origin) else {
        return;
    };
    let origin_2d = to_pos2(origin_2d_v);

    let radius = compute_gizmo_world_radius(camera, cam_gtf, origin, GIZMO_SCREEN_RADIUS_PX);
    let active_handle = gizmo_state.active_drag.as_ref().map(|d| d.handle);
    let hovered_handle = gizmo_state.hovered_handle;

    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("twg_transform_gizmo"),
        ))
        .with_clip_rect(clip_rect);

    // Subtle vertical altitude stem when elevated above ground
    if altitude.abs() > 0.15
        && let Ok(ground_2d) = camera.world_to_viewport(cam_gtf, Vec3::new(origin.x, 0.0, origin.z))
    {
        let g = to_pos2(ground_2d);
        painter.line_segment(
            [origin_2d, g],
            egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(56, 189, 248, 150)),
        );
        painter.circle_stroke(
            g,
            4.5,
            egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(56, 189, 248, 190)),
        );
    }

    match mode {
        GizmoMode::Move => draw_move_gizmo(
            &painter,
            origin,
            origin_2d,
            radius,
            active_handle,
            hovered_handle,
            gizmo_state.active_drag.as_ref(),
            camera,
            cam_gtf,
        ),
        GizmoMode::Rotate => draw_rotate_gizmo(
            &painter,
            origin,
            origin_2d,
            rot_quat,
            radius,
            active_handle,
            hovered_handle,
            gizmo_state.active_drag.as_ref(),
            camera,
            cam_gtf,
        ),
        GizmoMode::Scale => draw_scale_gizmo(
            &painter,
            origin,
            origin_2d,
            rot_quat,
            radius,
            active_handle,
            hovered_handle,
            gizmo_state.active_drag.as_ref(),
            camera,
            cam_gtf,
        ),
    }

    // Floating transform delta readout or hover handle pill
    let hud_painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("twg_gizmo_hud"),
    ));
    if let Some(drag) = &gizmo_state.active_drag {
        let text = match (drag.mode, drag.handle) {
            (GizmoMode::Move, GizmoHandle::AxisX) => {
                let dx = origin.x - drag.start_origin.x;
                format!("Move X: {:+.2} m  (Lng {:.5})", dx, cur_lng)
            }
            (GizmoMode::Move, GizmoHandle::AxisY) => {
                let dy = origin.y - drag.start_origin.y;
                format!("Move Y (Alt): {:.2} m  ({:+.2} m)", altitude, dy)
            }
            (GizmoMode::Move, GizmoHandle::AxisZ) => {
                let dz = origin.z - drag.start_origin.z;
                format!("Move Z: {:+.2} m  (Lat {:.5})", dz, cur_lat)
            }
            (GizmoMode::Move, GizmoHandle::Free) => {
                format!("Free Move: Lat {:.5}, Lng {:.5}", cur_lat, cur_lng)
            }
            (GizmoMode::Rotate, GizmoHandle::Free) => {
                format!(
                    "Free Rotate: X {:.1}°  Y {:.1}°  Z {:.1}°",
                    cur_rot[0], cur_rot[1], cur_rot[2]
                )
            }
            (GizmoMode::Rotate, h) => {
                let deg = drag.accumulated_angle_rad.to_degrees();
                format!(
                    "{}: {:+.1}°  (X {:.1}° Y {:.1}° Z {:.1}°)",
                    h.label(drag.mode),
                    deg,
                    cur_rot[0],
                    cur_rot[1],
                    cur_rot[2]
                )
            }
            (GizmoMode::Scale, GizmoHandle::Free) => {
                format!(
                    "Free Scale (XYZ): {:.2}  ({:.2}x)",
                    cur_scale[0], drag.visual_scale_factor
                )
            }
            (GizmoMode::Scale, GizmoHandle::AxisX) => {
                format!("Scale X: {:.2}  ({:.2}x)", cur_scale[0], drag.visual_scale_factor)
            }
            (GizmoMode::Scale, GizmoHandle::AxisY) => {
                format!("Scale Y: {:.2}  ({:.2}x)", cur_scale[1], drag.visual_scale_factor)
            }
            (GizmoMode::Scale, GizmoHandle::AxisZ) => {
                format!("Scale Z: {:.2}  ({:.2}x)", cur_scale[2], drag.visual_scale_factor)
            }
        };
        let badge_pos = to_pos2(drag.prev_cursor_px) + egui::vec2(16.0, -24.0);
        draw_gizmo_pill(&hud_painter, badge_pos, &text, COL_HOT);
    } else if let Some(h) = hovered_handle
        && let Some(ptr) = ctx.pointer_latest_pos()
    {
        draw_gizmo_pill(
            &hud_painter,
            ptr + egui::vec2(14.0, -22.0),
            h.label(mode),
            egui::Color32::from_rgb(186, 230, 253),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_move_gizmo(
    painter: &egui::Painter,
    origin: Vec3,
    origin_2d: egui::Pos2,
    radius: f32,
    active: Option<GizmoHandle>,
    hovered: Option<GizmoHandle>,
    drag: Option<&ActiveGizmoDrag>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    // Extended guide line along active axis during drag
    if let Some(d) = drag
        && matches!(d.handle, GizmoHandle::AxisX | GizmoHandle::AxisY | GizmoHandle::AxisZ)
        && let (Ok(p_neg), Ok(p_pos)) = (
            camera.world_to_viewport(cam_gtf, d.start_origin - d.axis_world * radius * 5.0),
            camera.world_to_viewport(cam_gtf, d.start_origin + d.axis_world * radius * 5.0),
        )
    {
        let guide_col = match d.handle {
            GizmoHandle::AxisX => egui::Color32::from_rgba_unmultiplied(239, 68, 68, 110),
            GizmoHandle::AxisY => egui::Color32::from_rgba_unmultiplied(34, 197, 94, 110),
            GizmoHandle::AxisZ => egui::Color32::from_rgba_unmultiplied(59, 130, 246, 110),
            GizmoHandle::Free => egui::Color32::TRANSPARENT,
        };
        painter.line_segment([to_pos2(p_neg), to_pos2(p_pos)], egui::Stroke::new(1.5, guide_col));
    }

    let cam_pos = cam_gtf.translation();
    let mut axes = [
        (GizmoHandle::AxisX, Vec3::X, "X"),
        (GizmoHandle::AxisY, Vec3::Y, "Y"),
        (GizmoHandle::AxisZ, Vec3::Z, "Z"),
    ];
    // Back-to-front depth sort
    axes.sort_by(|a, b| {
        let da = (origin + a.1 * radius).distance_squared(cam_pos);
        let db = (origin + b.1 * radius).distance_squared(cam_pos);
        db.total_cmp(&da)
    });

    for (handle, dir, label) in axes {
        let Ok(tip_v) = camera.world_to_viewport(cam_gtf, origin + dir * radius) else {
            continue;
        };
        let tip = to_pos2(tip_v);
        let delta = tip - origin_2d;
        let len = delta.length();
        if len < 10.0 {
            continue;
        }
        let u = delta / len;
        let perp = egui::vec2(-u.y, u.x);
        let col = axis_color(handle, active, hovered);
        let is_hot = active == Some(handle) || (active.is_none() && hovered == Some(handle));

        let shaft_start = origin_2d + u * 12.5;
        let arrow_len = if is_hot { 17.0 } else { 15.0 };
        let arrow_half_w = if is_hot { 6.5 } else { 5.5 };
        let shaft_end = tip - u * (arrow_len - 2.0);

        // Dark contrast casing + vibrant shaft
        painter.line_segment(
            [shaft_start, shaft_end],
            egui::Stroke::new(if is_hot { 6.5 } else { 5.0 }, COL_OUTLINE),
        );
        painter.line_segment(
            [shaft_start, shaft_end],
            egui::Stroke::new(if is_hot { 4.2 } else { 3.0 }, col),
        );

        // Filled arrowhead cone
        let base_center = tip - u * arrow_len;
        let tri = vec![
            tip,
            base_center + perp * arrow_half_w,
            base_center - perp * arrow_half_w,
        ];
        painter.add(egui::Shape::convex_polygon(
            tri,
            col,
            egui::Stroke::new(1.2, COL_OUTLINE),
        ));

        // Axis letter label
        let lbl_pos = tip + u * 11.0;
        painter.text(
            lbl_pos,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.0),
            col,
        );
    }

    // Center Free Move Handle (white/amber glowing circle with 4-way cross)
    let free_col = axis_color(GizmoHandle::Free, active, hovered);
    let free_hot = active == Some(GizmoHandle::Free)
        || (active.is_none() && hovered == Some(GizmoHandle::Free));
    let r = if free_hot { 11.5 } else { 9.5 };
    painter.circle_filled(
        origin_2d,
        r + 1.5,
        COL_OUTLINE,
    );
    painter.circle_filled(
        origin_2d,
        r,
        if free_hot {
            egui::Color32::from_rgba_unmultiplied(250, 204, 21, 225)
        } else {
            egui::Color32::from_rgba_unmultiplied(226, 232, 240, 175)
        },
    );
    painter.circle_stroke(origin_2d, r, egui::Stroke::new(2.0, free_col));
    // Inner cross icon for free-move affordance
    let cross_col = egui::Color32::from_rgb(15, 23, 42);
    painter.line_segment(
        [origin_2d - egui::vec2(4.0, 0.0), origin_2d + egui::vec2(4.0, 0.0)],
        egui::Stroke::new(1.5, cross_col),
    );
    painter.line_segment(
        [origin_2d - egui::vec2(0.0, 4.0), origin_2d + egui::vec2(0.0, 4.0)],
        egui::Stroke::new(1.5, cross_col),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_rotate_gizmo(
    painter: &egui::Painter,
    origin: Vec3,
    origin_2d: egui::Pos2,
    rot_quat: Quat,
    radius: f32,
    active: Option<GizmoHandle>,
    hovered: Option<GizmoHandle>,
    drag: Option<&ActiveGizmoDrag>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    // Subtle outer trackball silhouette sphere
    painter.circle_filled(
        origin_2d,
        GIZMO_SCREEN_RADIUS_PX,
        egui::Color32::from_rgba_unmultiplied(15, 23, 42, 34),
    );
    painter.circle_stroke(
        origin_2d,
        GIZMO_SCREEN_RADIUS_PX,
        egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(148, 163, 184, 90)),
    );

    let (ax, ay, az) = gizmo_axes(GizmoMode::Rotate, rot_quat);
    let to_cam = (cam_gtf.translation() - origin).normalize_or(Vec3::Y);
    const SEGMENTS: usize = 64;

    // Swept rotation angle wedge when actively rotating an axis
    if let Some(d) = drag
        && matches!(d.handle, GizmoHandle::AxisX | GizmoHandle::AxisY | GizmoHandle::AxisZ)
        && let Some(start_dir) = d.rot_start_dir
    {
        let wedge_col = match d.handle {
            GizmoHandle::AxisX => egui::Color32::from_rgba_unmultiplied(239, 68, 68, 70),
            GizmoHandle::AxisY => egui::Color32::from_rgba_unmultiplied(34, 197, 94, 70),
            GizmoHandle::AxisZ => egui::Color32::from_rgba_unmultiplied(59, 130, 246, 70),
            GizmoHandle::Free => egui::Color32::TRANSPARENT,
        };
        let steps = ((d.accumulated_angle_rad.abs() / 0.08).ceil() as usize).clamp(4, 60);
        let mut prev2 = camera
            .world_to_viewport(cam_gtf, origin + start_dir * radius)
            .ok()
            .map(to_pos2);
        if let Some(p0) = prev2 {
            painter.line_segment([origin_2d, p0], egui::Stroke::new(1.5, COL_HOT));
        }
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let ang = d.accumulated_angle_rad * t;
            let v = Quat::from_axis_angle(d.axis_world, ang) * start_dir;
            let curr2 = camera
                .world_to_viewport(cam_gtf, origin + v * radius)
                .ok()
                .map(to_pos2);
            if let (Some(a), Some(b)) = (prev2, curr2) {
                painter.add(egui::Shape::convex_polygon(
                    vec![origin_2d, a, b],
                    wedge_col,
                    egui::Stroke::NONE,
                ));
            }
            prev2 = curr2;
        }
        if let Some(p_end) = prev2 {
            painter.line_segment([origin_2d, p_end], egui::Stroke::new(2.0, COL_HOT));
        }
    }

    // Pass 1: Back-facing ring segments (dimmed)
    // Pass 2: Front-facing ring segments (crisp, bold)
    for front_pass in [false, true] {
        for (handle, axis, ref_u, label) in [
            (GizmoHandle::AxisX, ax, ay, "X"),
            (GizmoHandle::AxisY, ay, ax, "Y"),
            (GizmoHandle::AxisZ, az, ax, "Z"),
        ] {
            let (u, v) = ring_basis(axis, ref_u);
            let col = axis_color(handle, active, hovered);
            let is_hot = active == Some(handle) || (active.is_none() && hovered == Some(handle));

            let mut prev_pt3 = origin + u * radius;
            let mut prev_pt2 = camera.world_to_viewport(cam_gtf, prev_pt3).ok().map(to_pos2);
            let mut best_label_pt: Option<(egui::Pos2, f32)> = None;

            for i in 1..=SEGMENTS {
                let ang = (i as f32 / SEGMENTS as f32) * TAU;
                let curr_pt3 = origin + (u * ang.cos() + v * ang.sin()) * radius;
                let curr_pt2 = camera.world_to_viewport(cam_gtf, curr_pt3).ok().map(to_pos2);

                if let (Some(a), Some(b)) = (prev_pt2, curr_pt2) {
                    let mid_dir = ((prev_pt3 + curr_pt3) * 0.5 - origin).normalize_or_zero();
                    let facing = mid_dir.dot(to_cam);
                    let is_front = facing >= -0.08;
                    if is_front == front_pass {
                        if front_pass {
                            painter.line_segment(
                                [a, b],
                                egui::Stroke::new(if is_hot { 6.2 } else { 4.6 }, COL_OUTLINE),
                            );
                            painter.line_segment(
                                [a, b],
                                egui::Stroke::new(if is_hot { 4.2 } else { 2.8 }, col),
                            );
                            let radial_dist = a.distance(origin_2d);
                            let score = facing * 0.6 + (radial_dist / GIZMO_SCREEN_RADIUS_PX) * 0.4;
                            if best_label_pt.is_none_or(|cur| score > cur.1) {
                                best_label_pt = Some((a, score));
                            }
                        } else {
                            let dim = egui::Color32::from_rgba_unmultiplied(
                                col.r(),
                                col.g(),
                                col.b(),
                                (col.a() as f32 * 0.32) as u8,
                            );
                            painter.line_segment([a, b], egui::Stroke::new(1.5, dim));
                        }
                    }
                }
                prev_pt3 = curr_pt3;
                prev_pt2 = curr_pt2;
            }

            if front_pass
                && let Some((pt, _)) = best_label_pt
            {
                let out_dir = (pt - origin_2d).normalized();
                let lbl_pos = pt + out_dir * 12.0;
                painter.text(
                    lbl_pos,
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(11.0),
                    col,
                );
            }
        }
    }

    // Center Free Rotate Trackball Orb
    let free_hot = active == Some(GizmoHandle::Free)
        || (active.is_none() && hovered == Some(GizmoHandle::Free));
    let free_col = axis_color(GizmoHandle::Free, active, hovered);
    let r = if free_hot { 16.5 } else { 14.5 };
    painter.circle_filled(
        origin_2d,
        r,
        if free_hot {
            egui::Color32::from_rgba_unmultiplied(250, 204, 21, 95)
        } else {
            egui::Color32::from_rgba_unmultiplied(226, 232, 240, 48)
        },
    );
    painter.circle_stroke(origin_2d, r + 1.0, egui::Stroke::new(2.2, COL_OUTLINE));
    painter.circle_stroke(origin_2d, r, egui::Stroke::new(2.0, free_col));
    // Inner circular arc icon to signify Free 3D Rotate
    let arc_r = r * 0.52;
    let mut arc_pts = Vec::with_capacity(14);
    for i in 0..=12 {
        let a = -0.5 + (i as f32 / 12.0) * (TAU * 0.72);
        arc_pts.push(origin_2d + egui::vec2(a.cos(), a.sin()) * arc_r);
    }
    painter.add(egui::Shape::line(arc_pts, egui::Stroke::new(1.6, free_col)));
    painter.circle_filled(origin_2d, 2.5, free_col);
}

#[allow(clippy::too_many_arguments)]
fn draw_scale_gizmo(
    painter: &egui::Painter,
    origin: Vec3,
    origin_2d: egui::Pos2,
    rot_quat: Quat,
    radius: f32,
    active: Option<GizmoHandle>,
    hovered: Option<GizmoHandle>,
    drag: Option<&ActiveGizmoDrag>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    let (ax, ay, az) = gizmo_axes(GizmoMode::Scale, rot_quat);
    let cam_pos = cam_gtf.translation();
    let mut axes = [
        (GizmoHandle::AxisX, ax, "X"),
        (GizmoHandle::AxisY, ay, "Y"),
        (GizmoHandle::AxisZ, az, "Z"),
    ];
    axes.sort_by(|a, b| {
        let da = (origin + a.1 * radius).distance_squared(cam_pos);
        let db = (origin + b.1 * radius).distance_squared(cam_pos);
        db.total_cmp(&da)
    });

    for (handle, dir, label) in axes {
        let stretch = if let Some(d) = drag {
            if d.handle == handle || d.handle == GizmoHandle::Free {
                d.visual_scale_factor.clamp(0.42, 2.15)
            } else {
                1.0
            }
        } else {
            1.0
        };
        let Ok(tip_v) = camera.world_to_viewport(cam_gtf, origin + dir * (radius * stretch)) else {
            continue;
        };
        let tip = to_pos2(tip_v);
        let delta = tip - origin_2d;
        let len = delta.length();
        if len < 10.0 {
            continue;
        }
        let u = delta / len;
        let col = axis_color(handle, active, hovered);
        let is_hot = active == Some(handle)
            || active == Some(GizmoHandle::Free)
            || (active.is_none() && hovered == Some(handle));

        let shaft_start = origin_2d + u * 12.5;
        painter.line_segment(
            [shaft_start, tip],
            egui::Stroke::new(if is_hot { 6.2 } else { 4.8 }, COL_OUTLINE),
        );
        painter.line_segment(
            [shaft_start, tip],
            egui::Stroke::new(if is_hot { 4.0 } else { 2.8 }, col),
        );

        // Filled cube / square handle at the scale tip (Godot / Unity style)
        let box_sz = if is_hot { 13.0 } else { 10.5 };
        let tip_rect = egui::Rect::from_center_size(tip, egui::vec2(box_sz, box_sz));
        painter.rect_filled(tip_rect, 2.0, col);
        painter.rect_stroke(
            tip_rect,
            2.0,
            egui::Stroke::new(1.4, COL_OUTLINE),
            egui::StrokeKind::Outside,
        );

        let lbl_pos = tip + u * 14.0;
        painter.text(
            lbl_pos,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.0),
            col,
        );
    }

    // Center Free / Uniform Scale (XYZ) Cube Handle
    let free_hot = active == Some(GizmoHandle::Free)
        || (active.is_none() && hovered == Some(GizmoHandle::Free));
    let free_col = axis_color(GizmoHandle::Free, active, hovered);
    let sz = if free_hot { 19.0 } else { 16.0 };
    let center_rect = egui::Rect::from_center_size(origin_2d, egui::vec2(sz, sz));
    painter.rect_filled(
        center_rect,
        3.0,
        if free_hot {
            egui::Color32::from_rgba_unmultiplied(250, 204, 21, 235)
        } else {
            egui::Color32::from_rgba_unmultiplied(226, 232, 240, 200)
        },
    );
    painter.rect_stroke(
        center_rect,
        3.0,
        egui::Stroke::new(2.0, COL_OUTLINE),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        center_rect,
        3.0,
        egui::Stroke::new(1.8, free_col),
        egui::StrokeKind::Inside,
    );
    // Inner uniform-scale diagonal brackets
    let inner = egui::Rect::from_center_size(origin_2d, egui::vec2(sz * 0.45, sz * 0.45));
    painter.rect_stroke(
        inner,
        1.0,
        egui::Stroke::new(1.4, egui::Color32::from_rgb(15, 23, 42)),
        egui::StrokeKind::Inside,
    );
}

fn draw_gizmo_pill(painter: &egui::Painter, top_left: egui::Pos2, text: &str, text_color: egui::Color32) {
    let font = egui::FontId::proportional(11.5);
    let galley = painter.layout_no_wrap(text.to_string(), font, text_color);
    let pad = egui::vec2(8.0, 4.0);
    let rect = egui::Rect::from_min_size(top_left, galley.size() + pad * 2.0);
    painter.rect_filled(
        rect,
        5.0,
        egui::Color32::from_rgba_unmultiplied(10, 15, 24, 232),
    );
    painter.rect_stroke(
        rect,
        5.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(56, 189, 248, 170)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, text_color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_axis_param_hits_exact_point() {
        let ray = Ray3d::new(Vec3::new(0.0, 10.0, 0.0), Dir3::new(Vec3::new(1.0, -1.0, 0.0)).unwrap());
        let s = closest_axis_param(ray, Vec3::ZERO, Vec3::X).unwrap();
        assert!((s - 10.0).abs() < 1e-4, "expected 10.0 got {s}");
    }

    #[test]
    fn intersect_ray_plane_horizontal() {
        let ray = Ray3d::new(Vec3::new(3.0, 10.0, -4.0), Dir3::NEG_Y);
        let hit = intersect_ray_plane(ray, Vec3::new(0.0, 2.0, 0.0), Vec3::Y).unwrap();
        assert!((hit - Vec3::new(3.0, 2.0, -4.0)).length() < 1e-4);
    }

    #[test]
    fn smooth_scale_factor_monotonic_and_positive() {
        let up = smooth_scale_factor(95.0, 95.0);
        let down = smooth_scale_factor(-95.0, 95.0);
        assert!((up - 2.0).abs() < 1e-4);
        assert!((down - 0.5).abs() < 1e-4);
        assert!(smooth_scale_factor(-5000.0, 95.0) > 0.0);
    }
}
