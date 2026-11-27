use bevy::prelude::*;
use bevy_egui::egui;
use twg_shared::{
    models::Anim,
    particle_anim::{
        ParticleAnimKind, is_particle_anim, parse_particle_anim_config, particle_anim_position,
    },
    route_formation::{
        PatwalUnitRole, RouteFormationKind, anim_bound_marker_ids, is_multi_route_anim,
        parse_multi_route_config, sample_multi_route_poses,
    },
    traffic_anim::{
        is_traffic_flow_or_signal, parse_traffic_anim_config, write_traffic_anim_config,
    },
    traffic_diversion::{
        is_traffic_diversion_anim, parse_traffic_diversion_config, write_traffic_diversion_config,
    },
    traffic_queue::{
        is_traffic_queue_anim, parse_traffic_queue_config, write_traffic_queue_config,
    },
};
use uuid::Uuid;

use crate::{
    scene::{
        gizmo::{
            TransformGizmoState, closest_axis_param, compute_gizmo_world_radius,
            intersect_ray_plane,
        },
        markers::MarkerWorldBounds,
        playback::route_estimated_duration_sec,
    },
    state::{AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode},
};

const WAYPOINT_GIZMO_SCREEN_PX: f32 = 58.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteWaypointGizmoHandle {
    AxisX,
    AxisY,
    AxisZ,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteHoverTarget {
    GizmoHandle(RouteWaypointGizmoHandle),
    DeleteWaypoint(usize),
    Waypoint(usize),
    Midpoint(usize),
}

#[derive(Debug, Clone)]
pub struct ActiveWaypointDrag {
    pub anim_id: Uuid,
    pub traffic_lane_idx: Option<usize>,
    pub waypoint_idx: usize,
    pub handle: RouteWaypointGizmoHandle,
    pub start_world_pos: Vec3,
    pub start_altitude: f64,
    pub axis_world: Vec3,
    pub start_axis_param: f32,
    pub free_plane_offset: Vec3,
    pub start_cursor_px: Vec2,
    pub prev_cursor_px: Vec2,
    pub moved: bool,
}

#[derive(Resource, Default)]
pub struct RouteEditState {
    pub editing_anim_id: Option<Uuid>,
    /// When editing a `traffic_flow` animation, identifies which lane inside `cfg.lanes` is active.
    pub traffic_lane_idx: Option<usize>,
    pub selected_waypoint: Option<usize>,
    pub add_point_on_map_mode: bool,
    pub hovered: Option<RouteHoverTarget>,
    pub active_drag: Option<ActiveWaypointDrag>,
}

impl RouteEditState {
    pub fn is_editing(&self, anim_id: Uuid) -> bool {
        self.editing_anim_id == Some(anim_id)
    }

    pub fn is_active(&self) -> bool {
        self.editing_anim_id.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    pub fn clear(&mut self) {
        self.editing_anim_id = None;
        self.traffic_lane_idx = None;
        self.selected_waypoint = None;
        self.add_point_on_map_mode = false;
        self.hovered = None;
        self.active_drag = None;
    }
}

/// Returns the waypoint polyline currently being edited for `anim`:
/// - `traffic_flow`: `cfg.lanes[lane_idx].geom`
/// - `traffic_queue`: even `lane_idx = c*2` -> `corridors[c].queue_geom`, odd `lane_idx = c*2+1` -> `corridors[c].clear_geom`
/// - regular routes: `anim.geom`
pub fn editable_anim_geom(anim: &Anim, traffic_lane_idx: Option<usize>) -> Vec<[f64; 3]> {
    if is_traffic_flow_or_signal(anim) {
        let cfg = parse_traffic_anim_config(anim);
        let idx = traffic_lane_idx.unwrap_or(0);
        cfg.lanes
            .get(idx)
            .map(|l| l.geom.clone())
            .unwrap_or_default()
    } else if is_traffic_queue_anim(anim) {
        let cfg = parse_traffic_queue_config(anim);
        let encoded = traffic_lane_idx.unwrap_or(0);
        let cor_idx = encoded / 2;
        let is_clear = encoded % 2 == 1;
        cfg.corridors
            .get(cor_idx)
            .map(|c| {
                if is_clear {
                    c.clear_geom.clone()
                } else {
                    c.queue_geom.clone()
                }
            })
            .unwrap_or_default()
    } else if is_traffic_diversion_anim(anim) {
        let cfg = parse_traffic_diversion_config(anim);
        let encoded = traffic_lane_idx.unwrap_or(0);
        let cor_idx = encoded / 3;
        let part = encoded % 3;
        cfg.corridors
            .get(cor_idx)
            .map(|c| match part {
                0 => c.approach_geom.clone(),
                1 => c.main_geom.clone(),
                _ => c.divert_geom.clone(),
            })
            .unwrap_or_default()
    } else {
        anim.geom.clone()
    }
}

/// Writes an edited waypoint polyline back into `anim`.
pub fn write_editable_anim_geom(
    anim: &mut Anim,
    traffic_lane_idx: Option<usize>,
    geom: Vec<[f64; 3]>,
) {
    if is_traffic_flow_or_signal(anim) {
        let mut cfg = parse_traffic_anim_config(anim);
        let idx = traffic_lane_idx.unwrap_or(0);
        if let Some(lane) = cfg.lanes.get_mut(idx) {
            lane.geom = geom;
            write_traffic_anim_config(anim, &cfg);
        }
    } else if is_traffic_queue_anim(anim) {
        let mut cfg = parse_traffic_queue_config(anim);
        let encoded = traffic_lane_idx.unwrap_or(0);
        let cor_idx = encoded / 2;
        let is_clear = encoded % 2 == 1;
        if let Some(cor) = cfg.corridors.get_mut(cor_idx) {
            if is_clear {
                cor.clear_geom = geom;
            } else {
                cor.queue_geom = geom;
            }
            write_traffic_queue_config(anim, &cfg);
        }
    } else if is_traffic_diversion_anim(anim) {
        let mut cfg = parse_traffic_diversion_config(anim);
        let encoded = traffic_lane_idx.unwrap_or(0);
        let cor_idx = encoded / 3;
        let part = encoded % 3;
        if let Some(cor) = cfg.corridors.get_mut(cor_idx) {
            match part {
                0 => cor.approach_geom = geom,
                1 => cor.main_geom = geom,
                _ => cor.divert_geom = geom,
            }
            write_traffic_diversion_config(anim, &cfg);
        }
    } else {
        anim.geom = geom;
        anim.duration_sec = route_estimated_duration_sec(&anim.geom, anim.speed);
    }
}

/// Finds which Traffic lane (or `traffic_queue` encoded corridor/clear lane) was clicked on the 3D map.
pub fn hit_test_traffic_lane_index(
    cursor_px: Vec2,
    state: &AppState,
    anim: &Anim,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<usize> {
    if is_traffic_flow_or_signal(anim) {
        let cfg = parse_traffic_anim_config(anim);
        let mut best: Option<(usize, f32)> = None;
        for (idx, lane) in cfg.lanes.iter().enumerate() {
            if lane.geom.len() < 2 {
                continue;
            }
            let projected: Vec<Option<Vec2>> = lane
                .geom
                .iter()
                .map(|&pt| {
                    let w = waypoint_world_pos(state, pt);
                    camera.world_to_viewport(cam_gtf, w).ok()
                })
                .collect();
            for pair in projected.windows(2) {
                if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                    let d = point_to_segment_dist(cursor_px, a, b);
                    if d <= 14.0 && best.is_none_or(|cur| d < cur.1) {
                        best = Some((idx, d));
                    }
                }
            }
        }
        return best.map(|(idx, _)| idx);
    }
    if is_traffic_queue_anim(anim) {
        let cfg = parse_traffic_queue_config(anim);
        let mut best: Option<(usize, f32)> = None;
        for (c_idx, cor) in cfg.corridors.iter().enumerate() {
            for (is_clear, geom) in [(false, &cor.queue_geom), (true, &cor.clear_geom)] {
                if geom.len() < 2 {
                    continue;
                }
                let encoded = c_idx * 2 + usize::from(is_clear);
                let projected: Vec<Option<Vec2>> = geom
                    .iter()
                    .map(|&pt| {
                        let w = waypoint_world_pos(state, pt);
                        camera.world_to_viewport(cam_gtf, w).ok()
                    })
                    .collect();
                for pair in projected.windows(2) {
                    if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                        let d = point_to_segment_dist(cursor_px, a, b);
                        if d <= 14.0 && best.is_none_or(|cur| d < cur.1) {
                            best = Some((encoded, d));
                        }
                    }
                }
            }
        }
        return best.map(|(idx, _)| idx);
    }
    if is_traffic_diversion_anim(anim) {
        let cfg = parse_traffic_diversion_config(anim);
        let mut best: Option<(usize, f32)> = None;
        for (c_idx, cor) in cfg.corridors.iter().enumerate() {
            for (part, geom) in [
                (0usize, &cor.approach_geom),
                (1usize, &cor.main_geom),
                (2usize, &cor.divert_geom),
            ] {
                if geom.len() < 2 {
                    continue;
                }
                let encoded = c_idx * 3 + part;
                let projected: Vec<Option<Vec2>> = geom
                    .iter()
                    .map(|&pt| {
                        let w = waypoint_world_pos(state, pt);
                        camera.world_to_viewport(cam_gtf, w).ok()
                    })
                    .collect();
                for pair in projected.windows(2) {
                    if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                        let d = point_to_segment_dist(cursor_px, a, b);
                        if d <= 14.0 && best.is_none_or(|cur| d < cur.1) {
                            best = Some((encoded, d));
                        }
                    }
                }
            }
        }
        return best.map(|(idx, _)| idx);
    }
    None
}

/// Inserts a midpoint waypoint halfway between `geom[seg_idx]` and `geom[seg_idx + 1]`.
/// Returns the index of the newly inserted waypoint.
pub fn insert_route_midpoint_after(geom: &mut Vec<[f64; 3]>, seg_idx: usize) -> usize {
    if geom.len() < 2 || seg_idx + 1 >= geom.len() {
        return geom.len().saturating_sub(1);
    }
    let a = geom[seg_idx];
    let b = geom[seg_idx + 1];
    let mid = [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ];
    let insert_at = seg_idx + 1;
    geom.insert(insert_at, mid);
    insert_at
}

pub fn waypoint_world_pos(state: &AppState, pt: [f64; 3]) -> Vec3 {
    let l = state
        .geo_anchor
        .lng_lat_alt_to_local(pt[0], pt[1], pt[2].max(0.45));
    Vec3::from_array(l)
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

/// Screen-space hit-test across all visible, unlocked route animation polylines on the 3D map.
/// Enables clicking directly on any route line or waypoint to select and edit it.
pub fn hit_test_any_route_line(
    cursor_px: Vec2,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<Uuid> {
    if !state.show_gizmos {
        return None;
    }
    let mut best: Option<(Uuid, f32)> = None;
    for anim in &state.anims {
        if state.hidden_entity_ids.contains(&anim.id)
            || state.locked_entity_ids.contains(&anim.id)
        {
            continue;
        }
        if twg_shared::traffic_anim::is_traffic_flow_or_signal(anim) {
            let cfg = twg_shared::traffic_anim::parse_traffic_anim_config(anim);
            for lane in &cfg.lanes {
                if lane.geom.len() < 2 {
                    continue;
                }
                let projected: Vec<Option<Vec2>> = lane
                    .geom
                    .iter()
                    .map(|&pt| {
                        let w = waypoint_world_pos(state, pt);
                        camera.world_to_viewport(cam_gtf, w).ok()
                    })
                    .collect();
                for pair in projected.windows(2) {
                    if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                        let d = point_to_segment_dist(cursor_px, a, b);
                        if d <= 10.5 && best.is_none_or(|cur| d < cur.1) {
                            best = Some((anim.id, d));
                        }
                    }
                }
            }
            continue;
        }
        if is_traffic_queue_anim(anim) {
            let cfg = parse_traffic_queue_config(anim);
            for cor in &cfg.corridors {
                for geom in [&cor.queue_geom, &cor.clear_geom] {
                    if geom.len() < 2 {
                        continue;
                    }
                    let projected: Vec<Option<Vec2>> = geom
                        .iter()
                        .map(|&pt| {
                            let w = waypoint_world_pos(state, pt);
                            camera.world_to_viewport(cam_gtf, w).ok()
                        })
                        .collect();
                    for pair in projected.windows(2) {
                        if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                            let d = point_to_segment_dist(cursor_px, a, b);
                            if d <= 10.5 && best.is_none_or(|cur| d < cur.1) {
                                best = Some((anim.id, d));
                            }
                        }
                    }
                }
            }
            continue;
        }
        if is_traffic_diversion_anim(anim) {
            let cfg = parse_traffic_diversion_config(anim);
            for cor in &cfg.corridors {
                for geom in [&cor.approach_geom, &cor.main_geom, &cor.divert_geom] {
                    if geom.len() < 2 {
                        continue;
                    }
                    let projected: Vec<Option<Vec2>> = geom
                        .iter()
                        .map(|&pt| {
                            let w = waypoint_world_pos(state, pt);
                            camera.world_to_viewport(cam_gtf, w).ok()
                        })
                        .collect();
                    for pair in projected.windows(2) {
                        if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                            let d = point_to_segment_dist(cursor_px, a, b);
                            if d <= 10.5 && best.is_none_or(|cur| d < cur.1) {
                                best = Some((anim.id, d));
                            }
                        }
                    }
                }
            }
            continue;
        }
        if anim.geom.len() < 2 {
            continue;
        }
        let projected: Vec<Option<Vec2>> = anim
            .geom
            .iter()
            .map(|&pt| {
                let w = waypoint_world_pos(state, pt);
                camera.world_to_viewport(cam_gtf, w).ok()
            })
            .collect();

        for p_opt in &projected {
            if let Some(sp) = *p_opt {
                let d = cursor_px.distance(sp);
                if d <= 14.0 && best.is_none_or(|cur| d < cur.1) {
                    best = Some((anim.id, d));
                }
            }
        }
        for pair in projected.windows(2) {
            if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                let d = point_to_segment_dist(cursor_px, a, b);
                if d <= 10.5 && best.is_none_or(|cur| d < cur.1) {
                    best = Some((anim.id, d));
                }
            }
        }
    }
    best.map(|(id, _)| id)
}

/// Screen-space position of the floating red 'x' delete badge beside the selected waypoint.
fn delete_badge_screen_pos(origin_2d: Vec2) -> Vec2 {
    origin_2d + Vec2::new(-24.0, 22.0)
}

/// Removes a waypoint from a route animation (or active Traffic lane) if at least 2 waypoints will remain.
pub fn remove_route_waypoint(
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    anim_id: Uuid,
    idx: usize,
) -> bool {
    let lane_idx = route_edit.traffic_lane_idx;
    let Some(anim) = state.anims.iter_mut().find(|a| a.id == anim_id) else {
        return false;
    };
    let mut geom = editable_anim_geom(anim, lane_idx);
    if geom.len() <= 2 || idx >= geom.len() {
        state.status_banner = Some("Jalur animasi membutuhkan minimal 2 titik.".into());
        return false;
    }
    geom.remove(idx);
    let new_len = geom.len();
    write_editable_anim_geom(anim, lane_idx, geom);
    let updated = anim.clone();
    if state.anim_edit.as_ref().is_some_and(|e| e.id == anim_id) {
        state.anim_edit = Some(updated);
    }
    state.dirty_anim_ids.insert(anim_id);

    route_edit.active_drag = None;
    route_edit.hovered = None;
    if let Some(sel) = route_edit.selected_waypoint {
        if sel == idx {
            route_edit.selected_waypoint = Some(idx.min(new_len - 1));
        } else if sel > idx {
            route_edit.selected_waypoint = Some(sel - 1);
        }
    }
    state.status_banner = Some(format!(
        "Titik #{} dihapus (sisa {} titik) — klik Simpan (Ctrl+S) untuk menyimpan.",
        idx + 1,
        new_len
    ));
    true
}

pub fn hit_test_route_editor(
    cursor_px: Vec2,
    state: &AppState,
    anim: &Anim,
    traffic_lane_idx: Option<usize>,
    selected_waypoint: Option<usize>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<RouteHoverTarget> {
    let geom = editable_anim_geom(anim, traffic_lane_idx);
    if geom.len() < 2 {
        return None;
    }

    // Priority 1: Active Waypoint Delete 'x' Badge & 3D Move Gizmo (Red X, Green Alt/Y, Blue Z, Yellow Free)
    if let Some(wi) = selected_waypoint
        && let Some(&pt) = geom.get(wi)
    {
        let origin = waypoint_world_pos(state, pt);
        if let Ok(o2) = camera.world_to_viewport(cam_gtf, origin) {
            if geom.len() > 2 && cursor_px.distance(delete_badge_screen_pos(o2)) <= 11.5 {
                return Some(RouteHoverTarget::DeleteWaypoint(wi));
            }
            if cursor_px.distance(o2) <= 13.0 {
                return Some(RouteHoverTarget::GizmoHandle(RouteWaypointGizmoHandle::Free));
            }
            let radius =
                compute_gizmo_world_radius(camera, cam_gtf, origin, WAYPOINT_GIZMO_SCREEN_PX);
            let mut best: Option<(RouteWaypointGizmoHandle, f32)> = None;
            for (h, dir) in [
                (RouteWaypointGizmoHandle::AxisX, Vec3::X),
                (RouteWaypointGizmoHandle::AxisY, Vec3::Y),
                (RouteWaypointGizmoHandle::AxisZ, Vec3::Z),
            ] {
                if let Ok(t2) = camera.world_to_viewport(cam_gtf, origin + dir * radius) {
                    if t2.distance(o2) < 8.0 {
                        continue;
                    }
                    let u = (t2 - o2).normalize();
                    let d_shaft = point_to_segment_dist(cursor_px, o2 + u * 10.0, t2);
                    let d_tip = cursor_px.distance(t2);
                    let eff = d_shaft.min(d_tip * 0.85);
                    if eff <= 12.0 && best.is_none_or(|cur| eff < cur.1) {
                        best = Some((h, eff));
                    }
                }
            }
            if let Some((h, _)) = best {
                return Some(RouteHoverTarget::GizmoHandle(h));
            }
        }
    }

    // Priority 2: Waypoint dots
    let mut best_w: Option<(usize, f32)> = None;
    for (i, &pt) in geom.iter().enumerate() {
        let w = waypoint_world_pos(state, pt);
        if let Ok(s) = camera.world_to_viewport(cam_gtf, w) {
            let d = cursor_px.distance(s);
            if d <= 12.5 && best_w.is_none_or(|cur| d < cur.1) {
                best_w = Some((i, d));
            }
        }
    }
    if let Some((wi, _)) = best_w {
        return Some(RouteHoverTarget::Waypoint(wi));
    }

    // Priority 3: Segment midpoint "+" handles
    let mut best_m: Option<(usize, f32)> = None;
    for i in 0..(geom.len() - 1) {
        let a = geom[i];
        let b = geom[i + 1];
        let mid = [
            (a[0] + b[0]) * 0.5,
            (a[1] + b[1]) * 0.5,
            (a[2] + b[2]) * 0.5,
        ];
        let w = waypoint_world_pos(state, mid);
        if let Ok(s) = camera.world_to_viewport(cam_gtf, w) {
            let d = cursor_px.distance(s);
            if d <= 11.5 && best_m.is_none_or(|cur| d < cur.1) {
                best_m = Some((i, d));
            }
        }
    }
    best_m.map(|(seg_idx, _)| RouteHoverTarget::Midpoint(seg_idx))
}

#[allow(clippy::too_many_arguments)]
pub fn begin_waypoint_drag(
    state: &AppState,
    anim: &Anim,
    traffic_lane_idx: Option<usize>,
    waypoint_idx: usize,
    handle: RouteWaypointGizmoHandle,
    cursor_px: Vec2,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<ActiveWaypointDrag> {
    let geom = editable_anim_geom(anim, traffic_lane_idx);
    let &pt = geom.get(waypoint_idx)?;
    let start_world_pos = waypoint_world_pos(state, pt);
    let axis_world = match handle {
        RouteWaypointGizmoHandle::AxisX => Vec3::X,
        RouteWaypointGizmoHandle::AxisY => Vec3::Y,
        RouteWaypointGizmoHandle::AxisZ => Vec3::Z,
        RouteWaypointGizmoHandle::Free => Vec3::X,
    };

    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let start_axis_param = closest_axis_param(ray, start_world_pos, axis_world).unwrap_or(0.0);
    let free_plane_offset = if handle == RouteWaypointGizmoHandle::Free {
        intersect_ray_plane(ray, start_world_pos, Vec3::Y)
            .map(|h| start_world_pos - h)
            .unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO
    };

    Some(ActiveWaypointDrag {
        anim_id: anim.id,
        traffic_lane_idx,
        waypoint_idx,
        handle,
        start_world_pos,
        start_altitude: pt[2],
        axis_world,
        start_axis_param,
        free_plane_offset,
        start_cursor_px: cursor_px,
        prev_cursor_px: cursor_px,
        moved: false,
    })
}

pub fn update_waypoint_drag(
    drag: &mut ActiveWaypointDrag,
    cursor_px: Vec2,
    state: &mut AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> bool {
    let total_px = cursor_px - drag.start_cursor_px;
    if !drag.moved && total_px.length_squared() < 2.25 {
        drag.prev_cursor_px = cursor_px;
        return false;
    }
    let Ok(ray) = camera.viewport_to_world(cam_gtf, cursor_px) else {
        return false;
    };

    if drag.handle == RouteWaypointGizmoHandle::AxisY {
        let Some(s_curr) = closest_axis_param(ray, drag.start_world_pos, Vec3::Y) else {
            drag.prev_cursor_px = cursor_px;
            return false;
        };
        let delta_y = (s_curr - drag.start_axis_param) as f64;
        let raw_alt = (drag.start_altitude + delta_y).clamp(-50.0, 2000.0);
        let new_alt = if raw_alt.abs() < 0.12 {
            0.0
        } else {
            (raw_alt * 10.0).round() / 10.0
        };
        if let Some(anim) = state.anims.iter_mut().find(|a| a.id == drag.anim_id) {
            let mut geom = editable_anim_geom(anim, drag.traffic_lane_idx);
            if drag.waypoint_idx < geom.len() {
                geom[drag.waypoint_idx][2] = new_alt;
                write_editable_anim_geom(anim, drag.traffic_lane_idx, geom);
                let updated = anim.clone();
                if state.anim_edit.as_ref().is_some_and(|e| e.id == drag.anim_id) {
                    state.anim_edit = Some(updated);
                }
                state.dirty_anim_ids.insert(drag.anim_id);
                drag.moved = true;
                drag.prev_cursor_px = cursor_px;
                return true;
            }
        }
        return false;
    }

    let new_world = match drag.handle {
        RouteWaypointGizmoHandle::AxisX | RouteWaypointGizmoHandle::AxisZ => {
            let Some(s_curr) = closest_axis_param(ray, drag.start_world_pos, drag.axis_world) else {
                drag.prev_cursor_px = cursor_px;
                return false;
            };
            drag.start_world_pos + drag.axis_world * (s_curr - drag.start_axis_param)
        }
        RouteWaypointGizmoHandle::Free => {
            let Some(hit) = intersect_ray_plane(ray, drag.start_world_pos, Vec3::Y) else {
                drag.prev_cursor_px = cursor_px;
                return false;
            };
            hit + drag.free_plane_offset
        }
        RouteWaypointGizmoHandle::AxisY => unreachable!(),
    };

    let (lng, lat, _) = state
        .geo_anchor
        .local_to_lng_lat_alt([new_world.x, 0.0, new_world.z]);

    if let Some(anim) = state.anims.iter_mut().find(|a| a.id == drag.anim_id) {
        let mut geom = editable_anim_geom(anim, drag.traffic_lane_idx);
        if drag.waypoint_idx < geom.len() {
            let keep_alt = geom[drag.waypoint_idx][2];
            geom[drag.waypoint_idx] = [lng, lat, keep_alt];
            write_editable_anim_geom(anim, drag.traffic_lane_idx, geom);
            let updated = anim.clone();
            if state.anim_edit.as_ref().is_some_and(|e| e.id == drag.anim_id) {
                state.anim_edit = Some(updated);
            }
            state.dirty_anim_ids.insert(drag.anim_id);
            drag.moved = true;
            drag.prev_cursor_px = cursor_px;
            return true;
        }
    }
    false
}

/// Appends a new waypoint to the end of the route (or active Traffic lane) at the clicked ground point.
pub fn append_waypoint_from_cursor_click(
    state: &mut AppState,
    anim_id: Uuid,
    traffic_lane_idx: Option<usize>,
    cursor_px: Vec2,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<usize> {
    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let hit = intersect_ray_plane(ray, Vec3::ZERO, Vec3::Y)?;
    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);

    if let Some(anim) = state.anims.iter_mut().find(|a| a.id == anim_id) {
        let mut geom = editable_anim_geom(anim, traffic_lane_idx);
        let alt = geom.last().map(|p| p[2]).unwrap_or(0.0);
        geom.push([lng, lat, alt]);
        let new_idx = geom.len() - 1;
        write_editable_anim_geom(anim, traffic_lane_idx, geom);
        let updated = anim.clone();
        if state.anim_edit.as_ref().is_some_and(|e| e.id == anim_id) {
            state.anim_edit = Some(updated);
        }
        state.dirty_anim_ids.insert(anim_id);
        return Some(new_idx);
    }
    None
}

// ---------------------------------------------------------------------------
// Screen-Space Projected Route Draft, Pick-Marker HUD, and Waypoint Gizmo Overlay
// ---------------------------------------------------------------------------

const COL_X: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);
const COL_Y: egui::Color32 = egui::Color32::from_rgb(34, 197, 94);
const COL_Z: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
const COL_HOT: egui::Color32 = egui::Color32::from_rgb(253, 224, 71);
const COL_OUTLINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(9, 13, 22, 200);

fn to_pos2(v: Vec2) -> egui::Pos2 {
    egui::pos2(v.x, v.y)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_route_editor_overlay(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    route_edit: &RouteEditState,
    gizmo_state: &TransformGizmoState,
    marker_bounds: &MarkerWorldBounds,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    if state.screen != AppScreen::PlotSkenario || playback.play_test_mode {
        return;
    }

    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("twg_route_editor_overlay"),
        ))
        .with_clip_rect(clip_rect);

    // 1. Live numbered waypoints & rubber-band while drawing a new route or traffic lane
    if matches!(
        state.tool_mode,
        ToolMode::DrawRoute
            | ToolMode::DrawTrafficLane
            | ToolMode::DrawTrafficQueueLane
            | ToolMode::DrawTrafficDiversionLane
    ) {
        draw_route_draft_overlay(&painter, clip_rect, state, camera, cam_gtf);
        return;
    }

    // 2. Prominent guided HUD + bound-unit badges + hovered candidate pill during PickRouteMarker
    if state.tool_mode == ToolMode::PickRouteMarker {
        let target_anim = state
            .picking_marker_for_anim_id
            .and_then(|id| state.anims.iter().find(|a| a.id == id));
        let anim_name = target_anim
            .map(|a| a.name.as_str())
            .unwrap_or("Rute Baru");
        let is_multi = target_anim.is_some_and(is_multi_route_anim);
        let bound_ids: Vec<Uuid> = target_anim
            .map(anim_bound_marker_ids)
            .unwrap_or_default();
        let banner = if is_multi {
            format!(
                "Pilih Unit Formasi '{}' ({} unit): Klik Model 3D di peta untuk tambah/lepas (Amber = terikat, Ungu = kandidat) • Enter / Esc = Selesai",
                anim_name,
                bound_ids.len()
            )
        } else if !bound_ids.is_empty() {
            format!(
                "Ganti Unit Rute '{}': Klik Model 3D / Unit di peta (Amber = saat ini, Ungu = kandidat, Esc = Batal)",
                anim_name
            )
        } else {
            format!(
                "Langkah 2/2: Klik Model 3D / Unit di peta untuk mengikat '{}' (Esc = Batal)",
                anim_name
            )
        };
        draw_top_center_banner(
            &painter,
            clip_rect,
            &banner,
            egui::Color32::from_rgb(192, 132, 252),
        );

        let hovered_mid = match gizmo_state.hovered_entity {
            Some(SelectedEntity::Marker(hid)) => Some(hid),
            _ => None,
        };
        // Show Amber badges over all currently bound markers (except currently hovered one)
        for (idx, bound_mid) in bound_ids.iter().enumerate() {
            if hovered_mid == Some(*bound_mid) {
                continue;
            }
            let badge_label = if is_multi {
                format!("UNIT #{}", idx + 1)
            } else {
                "UNIT SAAT INI".to_string()
            };
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                *bound_mid,
                &badge_label,
                egui::Color32::from_rgb(245, 158, 11),
                egui::Color32::from_rgb(66, 32, 6),
                camera,
                cam_gtf,
            );
        }
        // Show Violet badge + live cursor tooltip over hovered candidate 3D marker
        if let Some(hid) = hovered_mid
            && let Some(m) = state.markers.iter().find(|m| m.id == hid)
        {
            let bound_pos = bound_ids.iter().position(|id| *id == hid);
            let already_bound = bound_pos.is_some();
            let badge_txt = if is_multi {
                if let Some(p) = bound_pos {
                    format!("- LEPAS (#{})", p + 1)
                } else {
                    format!("+ TAMBAH (#{})", bound_ids.len() + 1)
                }
            } else if already_bound {
                "UNIT TERIKAT".to_string()
            } else {
                "PILIH UNIT INI".to_string()
            };
            let (badge_border, badge_bg) = if is_multi && already_bound {
                (
                    egui::Color32::from_rgb(251, 113, 133),
                    egui::Color32::from_rgb(76, 5, 25),
                )
            } else {
                (
                    egui::Color32::from_rgb(216, 114, 255),
                    egui::Color32::from_rgb(59, 7, 100),
                )
            };
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                hid,
                &badge_txt,
                badge_border,
                badge_bg,
                camera,
                cam_gtf,
            );
            if let Some(ptr) = ctx.pointer_latest_pos() {
                let hud_painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("twg_route_pick_hud"),
                ));
                let tip = if is_multi {
                    if let Some(p) = bound_pos {
                        format!(
                            "Klik untuk melepas '{}' (#{}) dari formasi rute",
                            m.name,
                            p + 1
                        )
                    } else {
                        format!(
                            "Klik untuk menambah '{}' sebagai Unit #{} di formasi",
                            m.name,
                            bound_ids.len() + 1
                        )
                    }
                } else if already_bound {
                    format!("'{}' sudah terikat — klik untuk konfirmasi", m.name)
                } else if !bound_ids.is_empty() {
                    format!("Klik untuk ganti unit rute ke '{}'", m.name)
                } else {
                    format!("Klik untuk pilih '{}' sebagai unit rute", m.name)
                };
                draw_hud_pill(
                    &hud_painter,
                    ptr + egui::vec2(14.0, -24.0),
                    &tip,
                    egui::Color32::from_rgb(233, 213, 255),
                );
            }
        }
        return;
    }

    // 2b. Guided HUD + Amber/Violet badges during PickShootMarker (all Shoot sub-types)
    if state.tool_mode == ToolMode::PickShootMarker {
        let target_anim = state
            .picking_shoot_for_anim_id
            .and_then(|id| state.anims.iter().find(|a| a.id == id));
        let active_kind = target_anim
            .and_then(twg_shared::shoot_anim::shoot_anim_kind)
            .unwrap_or(state.new_shoot_kind);
        let kind_lbl = active_kind.label();
        let kind_tag = active_kind.short_tag();
        let banner = if let Some(a) = target_anim {
            format!(
                "Ganti Unit {kind_lbl} '{}': Klik Model 3D / Unit di peta (Amber = saat ini, Ungu = kandidat, Esc = Batal)",
                a.name
            )
        } else {
            format!(
                "Buat {kind_lbl}: Klik 1 Model 3D / Unit di peta sebagai sumber tembakan (Esc = Batal)"
            )
        };
        draw_top_center_banner(
            &painter,
            clip_rect,
            &banner,
            egui::Color32::from_rgb(56, 189, 248),
        );

        let hovered_mid = match gizmo_state.hovered_entity {
            Some(SelectedEntity::Marker(hid)) => Some(hid),
            _ => None,
        };
        if let Some(bound_mid) = target_anim.and_then(|a| a.marker_id)
            && hovered_mid != Some(bound_mid)
        {
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                bound_mid,
                kind_tag,
                egui::Color32::from_rgb(245, 158, 11),
                egui::Color32::from_rgb(66, 32, 6),
                camera,
                cam_gtf,
            );
        }
        if let Some(hid) = hovered_mid
            && let Some(m) = state.markers.iter().find(|m| m.id == hid)
        {
            let pick_badge = format!("PILIH {kind_tag}");
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                hid,
                &pick_badge,
                egui::Color32::from_rgb(216, 114, 255),
                egui::Color32::from_rgb(59, 7, 100),
                camera,
                cam_gtf,
            );
            if let Some(ptr) = ctx.pointer_latest_pos() {
                let hud_painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("twg_shoot_pick_hud"),
                ));
                let tip = if target_anim.is_some() {
                    format!("Klik untuk memindahkan {kind_lbl} ke '{}'", m.name)
                } else {
                    format!("Klik untuk memasang {kind_lbl} pada '{}'", m.name)
                };
                draw_hud_pill(
                    &hud_painter,
                    ptr + egui::vec2(14.0, -24.0),
                    &tip,
                    egui::Color32::from_rgb(233, 213, 255),
                );
            }
        }
        return;
    }

    // 2c. Guided HUD + Amber/Violet badges during PickPointerMarker (3D Pointer Arrow target selection)
    if state.tool_mode == ToolMode::PickPointerMarker {
        let target_anim = state
            .picking_pointer_for_anim_id
            .and_then(|id| state.anims.iter().find(|a| a.id == id));
        let banner = if let Some(a) = target_anim {
            format!(
                "Ganti Target Panah 3D '{}': Klik 1 Marker / Objek 3D di peta (Amber = saat ini, Ungu = kandidat, Esc = Batal)",
                a.name
            )
        } else {
            "Buat Panah Penunjuk 3D: Klik 1 Marker / Objek 3D di peta yg ingin ditunjuk panah (Esc = Batal)"
                .to_string()
        };
        draw_top_center_banner(
            &painter,
            clip_rect,
            &banner,
            egui::Color32::from_rgb(250, 204, 21),
        );

        let hovered_mid = match gizmo_state.hovered_entity {
            Some(SelectedEntity::Marker(hid)) => Some(hid),
            _ => None,
        };
        if let Some(bound_mid) = target_anim.and_then(|a| a.marker_id)
            && hovered_mid != Some(bound_mid)
        {
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                bound_mid,
                "TARGET PANAH",
                egui::Color32::from_rgb(245, 158, 11),
                egui::Color32::from_rgb(66, 32, 6),
                camera,
                cam_gtf,
            );
        }
        if let Some(hid) = hovered_mid
            && let Some(m) = state.markers.iter().find(|m| m.id == hid)
        {
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                hid,
                "PILIH TARGET PANAH",
                egui::Color32::from_rgb(216, 114, 255),
                egui::Color32::from_rgb(59, 7, 100),
                camera,
                cam_gtf,
            );
            if let Some(ptr) = ctx.pointer_latest_pos() {
                let hud_painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("twg_pointer_pick_hud"),
                ));
                let tip = if target_anim.is_some() {
                    format!("Klik untuk memindahkan Panah 3D ke '{}'", m.name)
                } else {
                    format!("Klik untuk memasang Panah 3D di atas '{}'", m.name)
                };
                draw_hud_pill(
                    &hud_painter,
                    ptr + egui::vec2(14.0, -24.0),
                    &tip,
                    egui::Color32::from_rgb(254, 240, 138),
                );
            }
        }
        return;
    }

    // 2d. Guided HUD + Emerald/Rose/Violet badges during PickSpawnHideMarker (multi-object Spawn & Hide)
    if state.tool_mode == ToolMode::PickSpawnHideMarker {
        let target_anim = state
            .picking_spawn_hide_for_anim_id
            .and_then(|id| state.anims.iter().find(|a| a.id == id));
        let action = target_anim
            .and_then(twg_shared::spawn_hide_anim::spawn_hide_action)
            .unwrap_or(state.new_spawn_hide_action);
        let anim_name = target_anim
            .map(|a| a.name.as_str())
            .unwrap_or(state.new_spawn_hide_name.as_str());
        let bound_ids: Vec<Uuid> = target_anim
            .map(anim_bound_marker_ids)
            .unwrap_or_default();
        let is_spawn = action == twg_shared::spawn_hide_anim::SpawnHideAction::Spawn;
        let (banner_col, sel_border, sel_bg, tag_word) = if is_spawn {
            (
                egui::Color32::from_rgb(52, 211, 153),
                egui::Color32::from_rgb(16, 185, 129),
                egui::Color32::from_rgb(6, 78, 59),
                "SPAWN",
            )
        } else {
            (
                egui::Color32::from_rgb(251, 113, 133),
                egui::Color32::from_rgb(244, 63, 94),
                egui::Color32::from_rgb(76, 5, 25),
                "HIDE",
            )
        };
        let banner = format!(
            "Pilih Objek {tag_word} '{}' ({} objek): Klik Marker/Objek 3D di peta untuk tambah/lepas ({} = terpilih, Ungu = kandidat) • Enter / Esc = Selesai",
            anim_name,
            bound_ids.len(),
            if is_spawn { "Hijau" } else { "Merah" }
        );
        draw_top_center_banner(&painter, clip_rect, &banner, banner_col);

        let hovered_mid = match gizmo_state.hovered_entity {
            Some(SelectedEntity::Marker(hid)) => Some(hid),
            _ => None,
        };
        for (idx, bound_mid) in bound_ids.iter().enumerate() {
            if hovered_mid == Some(*bound_mid) {
                continue;
            }
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                *bound_mid,
                &format!("#{} {tag_word}", idx + 1),
                sel_border,
                sel_bg,
                camera,
                cam_gtf,
            );
        }
        if let Some(hid) = hovered_mid
            && let Some(m) = state.markers.iter().find(|m| m.id == hid)
        {
            let bound_pos = bound_ids.iter().position(|id| *id == hid);
            let already_bound = bound_pos.is_some();
            let badge_txt = if let Some(p) = bound_pos {
                format!("- LEPAS (#{})", p + 1)
            } else {
                format!("+ {tag_word} (#{})", bound_ids.len() + 1)
            };
            let (badge_border, badge_bg) = if already_bound {
                (
                    egui::Color32::from_rgb(251, 113, 133),
                    egui::Color32::from_rgb(76, 5, 25),
                )
            } else {
                (
                    egui::Color32::from_rgb(216, 114, 255),
                    egui::Color32::from_rgb(59, 7, 100),
                )
            };
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                hid,
                &badge_txt,
                badge_border,
                badge_bg,
                camera,
                cam_gtf,
            );
            if let Some(ptr) = ctx.pointer_latest_pos() {
                let hud_painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("twg_spawn_hide_pick_hud"),
                ));
                let tip = if let Some(p) = bound_pos {
                    format!(
                        "Klik untuk melepas '{}' (#{}) dari daftar {tag_word}",
                        m.name,
                        p + 1
                    )
                } else {
                    format!(
                        "Klik untuk menambah '{}' sebagai Target #{} ({tag_word})",
                        m.name,
                        bound_ids.len() + 1
                    )
                };
                draw_hud_pill(
                    &hud_painter,
                    ptr + egui::vec2(14.0, -24.0),
                    &tip,
                    banner_col,
                );
            }
        }
        return;
    }

    // 3. Bound-unit Amber badges + Live Start-Formation Ghost Pins whenever ANY animation is selected
    if let SelectedEntity::Anim(sel_aid) = state.selected
        && let Some(anim) = state.anims.iter().find(|a| a.id == sel_aid)
    {
        let shoot_kind_opt = twg_shared::shoot_anim::shoot_anim_kind(anim);
        let is_pointer = twg_shared::pointer_anim::is_pointer_arrow_anim(anim);
        let spawn_hide_opt = twg_shared::spawn_hide_anim::spawn_hide_action(anim);
        let is_multi = is_multi_route_anim(anim);
        let multi_cfg_opt = if is_multi {
            Some(parse_multi_route_config(anim))
        } else {
            None
        };
        let is_patwal = multi_cfg_opt
            .as_ref()
            .is_some_and(|c| c.formation == RouteFormationKind::Patwal);
        let bound_ids = anim_bound_marker_ids(anim);
        for (idx, bound_mid) in bound_ids.iter().enumerate() {
            let (badge_label, badge_border, badge_bg) = if let Some(sk) = shoot_kind_opt {
                (
                    sk.short_tag().to_string(),
                    egui::Color32::from_rgb(251, 191, 36),
                    egui::Color32::from_rgb(66, 32, 6),
                )
            } else if is_pointer {
                (
                    "TARGET PANAH".to_string(),
                    egui::Color32::from_rgb(251, 191, 36),
                    egui::Color32::from_rgb(66, 32, 6),
                )
            } else if let Some(act) = spawn_hide_opt {
                match act {
                    twg_shared::spawn_hide_anim::SpawnHideAction::Spawn => (
                        format!("#{} SPAWN", idx + 1),
                        egui::Color32::from_rgb(16, 185, 129),
                        egui::Color32::from_rgb(6, 78, 59),
                    ),
                    twg_shared::spawn_hide_anim::SpawnHideAction::Hide => (
                        format!("#{} HIDE", idx + 1),
                        egui::Color32::from_rgb(244, 63, 94),
                        egui::Color32::from_rgb(76, 5, 25),
                    ),
                }
            } else if is_patwal
                && let Some(ref cfg) = multi_cfg_opt
            {
                (
                    format!("#{} {}", idx + 1, cfg.patwal_role_for(*bound_mid).short_tag()),
                    egui::Color32::from_rgb(251, 191, 36),
                    egui::Color32::from_rgb(66, 32, 6),
                )
            } else if is_multi {
                (
                    format!("UNIT #{}", idx + 1),
                    egui::Color32::from_rgb(251, 191, 36),
                    egui::Color32::from_rgb(66, 32, 6),
                )
            } else {
                (
                    "UNIT RUTE".to_string(),
                    egui::Color32::from_rgb(251, 191, 36),
                    egui::Color32::from_rgb(66, 32, 6),
                )
            };
            draw_marker_role_badge(
                &painter,
                clip_rect,
                state,
                marker_bounds,
                *bound_mid,
                &badge_label,
                badge_border,
                badge_bg,
                camera,
                cam_gtf,
            );
        }

        // Group (`Combine` / `Sequence`) member badges & hovered candidate preview badges on the 3D map
        if let Some((group_mode, entries)) = super::resolve_group_highlight_entries(state) {
            if let Some(hid) = state.group_hovered_child_anim_id
                && let Some(h_anim) = state.anims.iter().find(|a| a.id == hid)
            {
                let step_in_grp = twg_shared::group_anim::parse_group_anim_config(anim)
                    .items
                    .iter()
                    .position(|it| it.anim_id == hid)
                    .map(|p| p + 1);
                let banner = match step_in_grp {
                    Some(n) => format!(
                        "Highlight Anggota Grup #{n}: '{}' (Kuning = sedang disorot di peta)",
                        h_anim.name
                    ),
                    None => format!(
                        "Preview Kandidat Grup: '{}' — Klik untuk menambahkan ke '{}'",
                        h_anim.name, anim.name
                    ),
                };
                draw_top_center_banner(
                    &painter,
                    clip_rect,
                    &banner,
                    egui::Color32::from_rgb(250, 204, 21),
                );
            }

            let (member_border, member_bg) = match group_mode {
                twg_shared::group_anim::GroupAnimMode::Combine => (
                    egui::Color32::from_rgb(56, 189, 248),
                    egui::Color32::from_rgb(12, 42, 74),
                ),
                twg_shared::group_anim::GroupAnimMode::Sequence => (
                    egui::Color32::from_rgb(192, 132, 252),
                    egui::Color32::from_rgb(59, 7, 100),
                ),
            };
            let focus_border = egui::Color32::from_rgb(250, 204, 21);
            let focus_bg = egui::Color32::from_rgb(82, 48, 6);

            // Draw non-focused members first, then focused candidate on top so it's never occluded
            for draw_focused_pass in [false, true] {
                for entry in &entries {
                    if entry.is_focused_candidate != draw_focused_pass {
                        continue;
                    }
                    let Some(child) = state.anims.iter().find(|a| a.id == entry.anim_id) else {
                        continue;
                    };
                    let (border_c, fill_c) = if entry.is_focused_candidate {
                        (focus_border, focus_bg)
                    } else {
                        (member_border, member_bg)
                    };
                    let badge_txt = match entry.step_number {
                        Some(n) if entry.is_focused_candidate => format!(">> #{n} {}", child.name),
                        Some(n) => format!("#{n} {}", child.name),
                        None => format!("+ PILIH: {}", child.name),
                    };

                    let child_markers = anim_bound_marker_ids(child);
                    if !child_markers.is_empty() {
                        for mid in &child_markers {
                            draw_marker_role_badge(
                                &painter,
                                clip_rect,
                                state,
                                marker_bounds,
                                *mid,
                                &badge_txt,
                                border_c,
                                fill_c,
                                camera,
                                cam_gtf,
                            );
                        }
                    } else if let Some(pt) = child.geom.first() {
                        let anchor_w = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(
                            pt[0],
                            pt[1],
                            pt[2].max(1.5) + 3.0,
                        ));
                        draw_world_role_badge(
                            &painter,
                            clip_rect,
                            anchor_w,
                            &badge_txt,
                            border_c,
                            fill_c,
                            camera,
                            cam_gtf,
                        );
                    }
                }
            }
        }

        // Live numbered start-formation ghost pins on the 3D map for Multi-Unit Routes
        if let Some(ref cfg) = multi_cfg_opt
            && anim.geom.len() >= 2
        {
            let poses = sample_multi_route_poses(&anim.geom, cfg, 0.0);
            for (idx, pose) in poses.iter().enumerate() {
                let loc = state.geo_anchor.lng_lat_alt_to_local(
                    pose.lng,
                    pose.lat,
                    pose.altitude.max(0.25),
                );
                if let Ok(scr_v) = camera.world_to_viewport(cam_gtf, Vec3::from_array(loc)) {
                    let scr = to_pos2(scr_v);
                    if !clip_rect.contains(scr) {
                        continue;
                    }
                    let is_pad_sel = state.formation_pad_selected_marker == Some(pose.marker_id);
                    let r = if is_pad_sel { 9.5 } else { 7.5 };
                    let fill = if is_pad_sel {
                        egui::Color32::from_rgb(250, 204, 21)
                    } else if is_patwal {
                        match cfg.patwal_role_for(pose.marker_id) {
                            PatwalUnitRole::LeadMoto => egui::Color32::from_rgb(34, 211, 238),
                            PatwalUnitRole::FrontPolice => egui::Color32::from_rgb(59, 130, 246),
                            PatwalUnitRole::Vip => egui::Color32::from_rgb(251, 191, 36),
                            PatwalUnitRole::SideMoto => egui::Color32::from_rgb(45, 212, 191),
                            PatwalUnitRole::RearPolice => egui::Color32::from_rgb(129, 140, 248),
                        }
                    } else if idx == 0 {
                        egui::Color32::from_rgb(6, 182, 212)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(14, 116, 144, 225)
                    };
                    let stroke = if is_pad_sel {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(165, 243, 252)
                    };
                    painter.circle_filled(scr, r + 1.4, COL_OUTLINE);
                    painter.circle_filled(scr, r, fill);
                    painter.circle_stroke(scr, r, egui::Stroke::new(1.5, stroke));
                    painter.text(
                        scr,
                        egui::Align2::CENTER_CENTER,
                        format!("{}", idx + 1),
                        egui::FontId::proportional(9.5),
                        if is_pad_sel {
                            egui::Color32::from_rgb(15, 23, 42)
                        } else {
                            egui::Color32::WHITE
                        },
                    );
                }
            }
        }
    }

    // 3b. Clickable 2D tactical pins for all visible Particle Animations (Editor only; hidden in Play Skenario)
    if state.tool_mode == ToolMode::PlaceParticle {
        let banner = format!(
            "Letakkan Partikel {}: Klik lokasi di peta 3D untuk menempatkan (Esc = Batal)",
            state.new_particle_kind.label()
        );
        draw_top_center_banner(
            &painter,
            clip_rect,
            &banner,
            egui::Color32::from_rgb(251, 146, 60),
        );
    }
    draw_particle_tactical_pins(
        &painter,
        clip_rect,
        state,
        gizmo_state,
        camera,
        cam_gtf,
    );

    // 3. Interactive Route Waypoint Editing Overlay
    let Some(edit_id) = route_edit.editing_anim_id else {
        return;
    };
    if state.selected != SelectedEntity::Anim(edit_id) {
        return;
    }
    let Some(anim) = state.anims.iter().find(|a| a.id == edit_id) else {
        return;
    };
    let geom = editable_anim_geom(anim, route_edit.traffic_lane_idx);
    if geom.len() < 2 {
        return;
    }

    let n = geom.len();
    let world_pts: Vec<Vec3> = geom
        .iter()
        .map(|&p| waypoint_world_pos(state, p))
        .collect();
    let screen_pts: Vec<Option<egui::Pos2>> = world_pts
        .iter()
        .map(|&w| camera.world_to_viewport(cam_gtf, w).ok().map(to_pos2))
        .collect();

    // Draw crisp route edges with directional chevrons
    for i in 0..(n - 1) {
        if let (Some(a), Some(b)) = (screen_pts[i], screen_pts[i + 1]) {
            painter.line_segment(
                [a, b],
                egui::Stroke::new(4.2, egui::Color32::from_rgba_unmultiplied(8, 15, 28, 175)),
            );
            painter.line_segment(
                [a, b],
                egui::Stroke::new(2.4, egui::Color32::from_rgb(52, 211, 153)),
            );
            // Direction arrow at 30% of each segment
            let d = b - a;
            if d.length() > 38.0 {
                let u = d.normalized();
                let perp = egui::vec2(-u.y, u.x);
                let tip = a + d * 0.30;
                let base = tip - u * 8.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, base + perp * 4.2, base - perp * 4.2],
                    egui::Color32::from_rgb(110, 231, 183),
                    egui::Stroke::new(1.0, COL_OUTLINE),
                ));
            }
        }
    }

    // Draw green '+' midpoint insert handles on every segment (matching Polygon Edit style)
    for i in 0..(n - 1) {
        let mid_w = (world_pts[i] + world_pts[i + 1]) * 0.5;
        if let Ok(ms_v) = camera.world_to_viewport(cam_gtf, mid_w) {
            let ms = to_pos2(ms_v);
            let is_hov = route_edit.hovered == Some(RouteHoverTarget::Midpoint(i));
            let r = if is_hov { 9.0 } else { 7.0 };
            painter.circle_filled(ms, r + 1.0, COL_OUTLINE);
            painter.circle_filled(
                ms,
                r,
                if is_hov {
                    egui::Color32::from_rgb(16, 185, 129)
                } else {
                    egui::Color32::from_rgba_unmultiplied(16, 185, 129, 190)
                },
            );
            painter.circle_stroke(
                ms,
                r,
                egui::Stroke::new(
                    1.3,
                    if is_hov {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(167, 243, 208)
                    },
                ),
            );
            let arm = 3.2;
            painter.line_segment(
                [ms - egui::vec2(arm, 0.0), ms + egui::vec2(arm, 0.0)],
                egui::Stroke::new(1.6, egui::Color32::WHITE),
            );
            painter.line_segment(
                [ms - egui::vec2(0.0, arm), ms + egui::vec2(0.0, arm)],
                egui::Stroke::new(1.6, egui::Color32::WHITE),
            );
        }
    }

    // Draw crisp numbered waypoint dots (matching Polygon Edit pins: white disc + dark number + role border)
    for (i, scr_opt) in screen_pts.iter().enumerate() {
        let Some(scr) = *scr_opt else {
            continue;
        };
        // Vertical drop-stem & compact altitude tag when elevated above ground
        if geom[i][2].abs() > 0.2 {
            let g_local = state
                .geo_anchor
                .lng_lat_alt_to_local(geom[i][0], geom[i][1], 0.0);
            if let Ok(gs_v) = camera.world_to_viewport(cam_gtf, Vec3::from_array(g_local)) {
                let gs = to_pos2(gs_v);
                painter.line_segment(
                    [gs, scr],
                    egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(52, 211, 153, 185)),
                );
                painter.circle_filled(gs, 3.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, 190));
            }
            let alt_txt = format!("{:.1}m", geom[i][2]);
            painter.text(
                scr + egui::vec2(13.0, -11.0),
                egui::Align2::LEFT_CENTER,
                alt_txt,
                egui::FontId::proportional(9.5),
                egui::Color32::from_rgb(134, 239, 172),
            );
        }
        let is_sel = route_edit.selected_waypoint == Some(i);
        let is_hov = route_edit.hovered == Some(RouteHoverTarget::Waypoint(i));
        let r = if is_sel {
            10.5
        } else if is_hov {
            9.5
        } else {
            8.2
        };
        let fill = if is_sel {
            COL_HOT
        } else if is_hov {
            egui::Color32::from_rgb(224, 242, 254)
        } else {
            egui::Color32::from_rgb(248, 250, 252)
        };
        let ring_col = if is_sel {
            egui::Color32::WHITE
        } else if is_hov {
            egui::Color32::from_rgb(56, 189, 248)
        } else if i == 0 {
            egui::Color32::from_rgb(16, 185, 129)
        } else if i + 1 == n {
            egui::Color32::from_rgb(245, 158, 11)
        } else {
            egui::Color32::from_rgb(30, 64, 175)
        };
        painter.circle_filled(scr, r + 1.6, COL_OUTLINE);
        painter.circle_filled(scr, r, fill);
        painter.circle_stroke(scr, r, egui::Stroke::new(2.0, ring_col));
        painter.text(
            scr,
            egui::Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(15, 23, 42),
        );
    }

    // Draw 3D Move Gizmo (Red X, Green Alt/Y, Blue Z, Yellow Free) + Red 'x' Delete badge on the clicked waypoint
    if let Some(wi) = route_edit.selected_waypoint
        && wi < world_pts.len()
    {
        let origin = world_pts[wi];
        if let Ok(o2_v) = camera.world_to_viewport(cam_gtf, origin) {
            let o2 = to_pos2(o2_v);
            let radius =
                compute_gizmo_world_radius(camera, cam_gtf, origin, WAYPOINT_GIZMO_SCREEN_PX);
            let active_h = route_edit.active_drag.as_ref().map(|d| d.handle);
            let hovered_h = match route_edit.hovered {
                Some(RouteHoverTarget::GizmoHandle(h)) => Some(h),
                _ => None,
            };

            for (h, dir, base_col, lbl) in [
                (RouteWaypointGizmoHandle::AxisX, Vec3::X, COL_X, "X"),
                (RouteWaypointGizmoHandle::AxisY, Vec3::Y, COL_Y, "Alt"),
                (RouteWaypointGizmoHandle::AxisZ, Vec3::Z, COL_Z, "Z"),
            ] {
                if let Ok(tip_v) = camera.world_to_viewport(cam_gtf, origin + dir * radius) {
                    let tip = to_pos2(tip_v);
                    let delta = tip - o2;
                    let len = delta.length();
                    if len < 10.0 {
                        continue;
                    }
                    let u = delta / len;
                    let perp = egui::vec2(-u.y, u.x);
                    let is_hot =
                        active_h == Some(h) || (active_h.is_none() && hovered_h == Some(h));
                    let col = if is_hot {
                        COL_HOT
                    } else if active_h.is_some() {
                        egui::Color32::from_rgba_unmultiplied(
                            base_col.r(),
                            base_col.g(),
                            base_col.b(),
                            80,
                        )
                    } else {
                        base_col
                    };

                    let shaft_start = o2 + u * 11.0;
                    let arrow_len = if is_hot { 14.5 } else { 13.0 };
                    let arrow_hw = if is_hot { 5.8 } else { 4.8 };
                    let shaft_end = tip - u * (arrow_len - 2.0);
                    painter.line_segment(
                        [shaft_start, shaft_end],
                        egui::Stroke::new(if is_hot { 5.8 } else { 4.5 }, COL_OUTLINE),
                    );
                    painter.line_segment(
                        [shaft_start, shaft_end],
                        egui::Stroke::new(if is_hot { 3.6 } else { 2.6 }, col),
                    );
                    let base_c = tip - u * arrow_len;
                    painter.add(egui::Shape::convex_polygon(
                        vec![tip, base_c + perp * arrow_hw, base_c - perp * arrow_hw],
                        col,
                        egui::Stroke::new(1.1, COL_OUTLINE),
                    ));
                    painter.text(
                        tip + u * 10.0,
                        egui::Align2::CENTER_CENTER,
                        lbl,
                        egui::FontId::proportional(10.5),
                        col,
                    );
                }
            }

            // Center Free-Move handle
            let free_hot = active_h == Some(RouteWaypointGizmoHandle::Free)
                || (active_h.is_none() && hovered_h == Some(RouteWaypointGizmoHandle::Free));
            let r = if free_hot { 10.5 } else { 9.0 };
            painter.circle_filled(o2, r + 1.5, COL_OUTLINE);
            painter.circle_filled(
                o2,
                r,
                if free_hot {
                    COL_HOT
                } else {
                    egui::Color32::from_rgb(250, 204, 21)
                },
            );
            painter.circle_stroke(o2, r, egui::Stroke::new(1.8, egui::Color32::WHITE));
            let cross_col = egui::Color32::from_rgb(15, 23, 42);
            painter.line_segment(
                [o2 - egui::vec2(3.8, 0.0), o2 + egui::vec2(3.8, 0.0)],
                egui::Stroke::new(1.5, cross_col),
            );
            painter.line_segment(
                [o2 - egui::vec2(0.0, 3.8), o2 + egui::vec2(0.0, 3.8)],
                egui::Stroke::new(1.5, cross_col),
            );

            // Floating red 'x' Delete Waypoint button right on the map (when > 2 waypoints)
            if n > 2 && active_h.is_none() {
                let del_p = to_pos2(delete_badge_screen_pos(o2_v));
                let del_hot = route_edit.hovered == Some(RouteHoverTarget::DeleteWaypoint(wi));
                let dr = if del_hot { 10.0 } else { 8.2 };
                painter.circle_filled(del_p, dr + 1.4, COL_OUTLINE);
                painter.circle_filled(
                    del_p,
                    dr,
                    if del_hot {
                        egui::Color32::from_rgb(239, 68, 68)
                    } else {
                        egui::Color32::from_rgb(185, 28, 28)
                    },
                );
                painter.circle_stroke(del_p, dr, egui::Stroke::new(1.5, egui::Color32::WHITE));
                let xr = 3.3;
                painter.line_segment(
                    [del_p - egui::vec2(xr, xr), del_p + egui::vec2(xr, xr)],
                    egui::Stroke::new(1.8, egui::Color32::WHITE),
                );
                painter.line_segment(
                    [del_p + egui::vec2(-xr, xr), del_p + egui::vec2(xr, -xr)],
                    egui::Stroke::new(1.8, egui::Color32::WHITE),
                );
            }
        }
    }

    // Live HUD tooltip during hover / drag
    let hud_painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("twg_route_edit_hud"),
    ));
    if let Some(d) = &route_edit.active_drag
        && let Some(&pt) = geom.get(d.waypoint_idx)
    {
        let txt = if d.handle == RouteWaypointGizmoHandle::AxisY {
            format!("Titik #{}: Altitude {:.1} m", d.waypoint_idx + 1, pt[2])
        } else {
            format!(
                "Titik #{}: Lat {:.5}, Lng {:.5}  •  Alt {:.1} m",
                d.waypoint_idx + 1,
                pt[1],
                pt[0],
                pt[2]
            )
        };
        draw_hud_pill(
            &hud_painter,
            to_pos2(d.prev_cursor_px) + egui::vec2(15.0, -24.0),
            &txt,
            COL_HOT,
        );
    } else if let Some(hov) = route_edit.hovered
        && let Some(ptr) = ctx.pointer_latest_pos()
    {
        let txt = match hov {
            RouteHoverTarget::Waypoint(i) => {
                format!("Titik #{} — Tarik langsung atau klik untuk Move Gizmo (X / Alt / Z)", i + 1)
            }
            RouteHoverTarget::DeleteWaypoint(i) => {
                format!("Hapus Titik #{} dari rute (atau tekan Del / Backspace)", i + 1)
            }
            RouteHoverTarget::Midpoint(s) => {
                format!("+ Sisipkan titik baru antara #{} & #{}", s + 1, s + 2)
            }
            RouteHoverTarget::GizmoHandle(RouteWaypointGizmoHandle::AxisX) => {
                "Geser Titik Sumbu X".to_string()
            }
            RouteHoverTarget::GizmoHandle(RouteWaypointGizmoHandle::AxisY) => {
                "Ubah Ketinggian / Altitude Titik (Y)".to_string()
            }
            RouteHoverTarget::GizmoHandle(RouteWaypointGizmoHandle::AxisZ) => {
                "Geser Titik Sumbu Z".to_string()
            }
            RouteHoverTarget::GizmoHandle(RouteWaypointGizmoHandle::Free) => {
                "Free Move Titik Rute (XZ)".to_string()
            }
        };
        draw_hud_pill(
            &hud_painter,
            ptr + egui::vec2(14.0, -22.0),
            &txt,
            egui::Color32::from_rgb(110, 231, 183),
        );
    }
}

fn draw_route_draft_overlay(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    let n = state.draft_points_lnglat.len();
    let msg = if n < 2 {
        format!(
            "Langkah 1/2: Klik titik rute di peta ({n} titik, min 2) — Backspace = Undo, Esc = Batal"
        )
    } else {
        format!(
            "Langkah 1/2: {n} titik rute — Tekan Enter / 'Selesai & Pilih Unit' untuk pilih Model 3D"
        )
    };
    draw_top_center_banner(
        painter,
        clip_rect,
        &msg,
        egui::Color32::from_rgb(16, 185, 129),
    );

    let screen_pts: Vec<Option<egui::Pos2>> = state
        .draft_points_lnglat
        .iter()
        .map(|p| {
            let l = state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.55);
            camera
                .world_to_viewport(cam_gtf, Vec3::from_array(l))
                .ok()
                .map(to_pos2)
        })
        .collect();

    for i in 0..screen_pts.len().saturating_sub(1) {
        if let (Some(a), Some(b)) = (screen_pts[i], screen_pts[i + 1]) {
            painter.line_segment(
                [a, b],
                egui::Stroke::new(2.8, egui::Color32::from_rgb(52, 211, 153)),
            );
        }
    }

    // Rubber-band line from last waypoint to live cursor
    if let Some(Some(last_2d)) = screen_pts.last()
        && let Some([clng, clat]) = state.cursor_lnglat
    {
        let cl = state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.55);
        if let Ok(cur_v) = camera.world_to_viewport(cam_gtf, Vec3::from_array(cl)) {
            painter.line_segment(
                [*last_2d, to_pos2(cur_v)],
                egui::Stroke::new(
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(110, 231, 183, 180),
                ),
            );
        }
    }

    for (i, s_opt) in screen_pts.iter().enumerate() {
        let Some(s) = *s_opt else {
            continue;
        };
        let fill = if i == 0 {
            egui::Color32::from_rgb(16, 185, 129)
        } else {
            egui::Color32::from_rgb(14, 116, 144)
        };
        painter.circle_filled(s, 8.5, COL_OUTLINE);
        painter.circle_filled(s, 7.2, fill);
        painter.circle_stroke(s, 7.2, egui::Stroke::new(1.5, egui::Color32::WHITE));
        painter.text(
            s,
            egui::Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::proportional(9.5),
            egui::Color32::WHITE,
        );
    }
}

fn draw_top_center_banner(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    text: &str,
    accent: egui::Color32,
) {
    let font = egui::FontId::proportional(11.5);
    let galley = painter.layout_no_wrap(text.to_string(), font, egui::Color32::WHITE);
    let pad = egui::vec2(12.0, 6.0);
    let size = galley.size() + pad * 2.0;
    let top_left = egui::pos2(
        clip_rect.center().x - size.x * 0.5,
        clip_rect.top() + 10.0,
    );
    let rect = egui::Rect::from_min_size(top_left, size);
    painter.rect_filled(
        rect,
        6.0,
        egui::Color32::from_rgba_unmultiplied(10, 17, 30, 235),
    );
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.5, accent),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, egui::Color32::WHITE);
}

#[allow(clippy::too_many_arguments)]
fn draw_marker_role_badge(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    state: &AppState,
    marker_bounds: &MarkerWorldBounds,
    marker_id: Uuid,
    tag: &str,
    border_col: egui::Color32,
    fill_col: egui::Color32,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    let Some(m) = state.markers.iter().find(|x| x.id == marker_id && x.is_spawn) else {
        return;
    };
    let anchor_w = if let Some(b) = marker_bounds.bounds.get(&marker_id) {
        let h_span = (b.world_max.y - b.world_min.y).max(1.0);
        Vec3::new(
            b.footprint_center.x,
            b.world_max.y + (h_span * 0.28).clamp(0.9, 6.0),
            b.footprint_center.z,
        )
    } else {
        let l = state
            .geo_anchor
            .lng_lat_alt_to_local(m.lng, m.lat, m.altitude + 13.0);
        Vec3::from_array(l)
    };
    let Ok(scr) = camera.world_to_viewport(cam_gtf, anchor_w) else {
        return;
    };
    let pos = egui::pos2(scr.x, scr.y - 16.0);
    if !clip_rect.contains(pos) {
        return;
    }
    let font = egui::FontId::proportional(10.0);
    let galley = painter.layout_no_wrap(tag.to_string(), font, egui::Color32::WHITE);
    let pad = egui::vec2(7.0, 2.5);
    let rect = egui::Rect::from_center_size(pos, galley.size() + pad * 2.0);
    painter.rect_filled(rect, 9.0, fill_col);
    painter.rect_stroke(
        rect,
        9.0,
        egui::Stroke::new(1.5, border_col),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, egui::Color32::WHITE);
}

#[allow(clippy::too_many_arguments)]
fn draw_world_role_badge(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    anchor_w: Vec3,
    tag: &str,
    border_col: egui::Color32,
    fill_col: egui::Color32,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    let Ok(scr) = camera.world_to_viewport(cam_gtf, anchor_w) else {
        return;
    };
    let pos = egui::pos2(scr.x, scr.y - 16.0);
    if !clip_rect.contains(pos) {
        return;
    }
    let font = egui::FontId::proportional(10.0);
    let galley = painter.layout_no_wrap(tag.to_string(), font, egui::Color32::WHITE);
    let pad = egui::vec2(7.0, 2.5);
    let rect = egui::Rect::from_center_size(pos, galley.size() + pad * 2.0);
    painter.rect_filled(rect, 9.0, fill_col);
    painter.rect_stroke(
        rect,
        9.0,
        egui::Stroke::new(1.5, border_col),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, egui::Color32::WHITE);
}

fn draw_hud_pill(
    painter: &egui::Painter,
    top_left: egui::Pos2,
    text: &str,
    text_color: egui::Color32,
) {
    let font = egui::FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(text.to_string(), font, text_color);
    let pad = egui::vec2(8.0, 4.0);
    let rect = egui::Rect::from_min_size(top_left, galley.size() + pad * 2.0);
    painter.rect_filled(
        rect,
        5.0,
        egui::Color32::from_rgba_unmultiplied(12, 17, 30, 235),
    );
    painter.rect_stroke(
        rect,
        5.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, 185)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, text_color);
}

fn draw_particle_tactical_pins(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    state: &AppState,
    gizmo_state: &TransformGizmoState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    for anim in &state.anims {
        if !is_particle_anim(anim)
            || anim.geom.is_empty()
            || state.hidden_entity_ids.contains(&anim.id)
        {
            continue;
        }
        let [lng, lat, alt] = particle_anim_position(anim);
        let cfg = parse_particle_anim_config(anim);
        let world_pos = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt));
        let Ok(scr_v) = camera.world_to_viewport(cam_gtf, world_pos) else {
            continue;
        };
        let scr = to_pos2(scr_v);
        if !clip_rect.contains(scr) {
            continue;
        }

        let is_selected = state.selected == SelectedEntity::Anim(anim.id);
        let is_hovered = gizmo_state.hovered_entity == Some(SelectedEntity::Anim(anim.id));
        let is_locked = state.locked_entity_ids.contains(&anim.id);

        // Altitude drop-stem when elevated above/below ground (and not already drawn by active Move Gizmo)
        if alt.abs() > 0.2 && !is_selected {
            let ground_w = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0));
            if let Ok(g_scr_v) = camera.world_to_viewport(cam_gtf, ground_w) {
                let g_scr = to_pos2(g_scr_v);
                painter.line_segment(
                    [g_scr, scr],
                    egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(251, 146, 60, 180)),
                );
                painter.circle_filled(
                    g_scr,
                    3.0,
                    egui::Color32::from_rgba_unmultiplied(251, 146, 60, 210),
                );
            }
        }

        let (tag, accent, fill_bg) = match cfg.kind {
            ParticleAnimKind::Fire => (
                "API",
                egui::Color32::from_rgb(251, 146, 60),
                egui::Color32::from_rgba_unmultiplied(67, 20, 7, 225),
            ),
            ParticleAnimKind::Smoke => (
                "ASAP",
                egui::Color32::from_rgb(203, 213, 225),
                egui::Color32::from_rgba_unmultiplied(30, 41, 59, 225),
            ),
            ParticleAnimKind::Explosion => (
                "LED",
                egui::Color32::from_rgb(248, 113, 113),
                egui::Color32::from_rgba_unmultiplied(69, 10, 10, 225),
            ),
        };

        // Ground anchor dot (kept unobtrusive when Move Gizmo is drawn on selected particle)
        if !is_selected {
            let dot_r = if is_hovered { 6.5 } else { 5.0 };
            painter.circle_filled(scr, dot_r + 1.5, COL_OUTLINE);
            painter.circle_filled(scr, dot_r, accent);
            painter.circle_stroke(
                scr,
                dot_r,
                egui::Stroke::new(
                    1.4,
                    if is_hovered {
                        COL_HOT
                    } else {
                        egui::Color32::WHITE
                    },
                ),
            );
        }

        // Offset pill above the gizmo/dot so it never obscures the Move Gizmo center handle
        let pill_offset_y = if is_selected { -34.0 } else { -20.0 };
        let label = if alt.abs() > 0.2 {
            format!(
                "[{tag}] {} (+{:.1}m){}",
                anim.name,
                alt,
                if is_locked { " [L]" } else { "" }
            )
        } else {
            format!(
                "[{tag}] {}{}",
                anim.name,
                if is_locked { " [L]" } else { "" }
            )
        };
        let font = egui::FontId::proportional(if is_selected { 10.5 } else { 10.0 });
        let galley = painter.layout_no_wrap(label, font, egui::Color32::WHITE);
        let pad = egui::vec2(6.0, 2.5);
        let center = egui::pos2(scr.x, scr.y + pill_offset_y);
        let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
        let border_col = if is_selected {
            COL_HOT
        } else if is_hovered {
            egui::Color32::WHITE
        } else {
            accent
        };
        painter.rect_filled(rect, 4.0, fill_bg);
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(if is_selected || is_hovered { 1.6 } else { 1.0 }, border_col),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, egui::Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_insert_splits_route_segment() {
        let mut geom = vec![[115.20, -8.67, 0.0], [115.22, -8.69, 10.0]];
        let idx = insert_route_midpoint_after(&mut geom, 0);
        assert_eq!(idx, 1);
        assert_eq!(geom.len(), 3);
        assert!((geom[1][0] - 115.21).abs() < 1e-6);
        assert!((geom[1][1] - (-8.68)).abs() < 1e-6);
        assert!((geom[1][2] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn remove_route_waypoint_enforces_min_two_points() {
        let mut st = AppState::new(std::path::PathBuf::from("."));
        let now = chrono::Utc::now();
        let aid = Uuid::new_v4();
        st.anims.push(Anim {
            id: aid,
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Rute Uji".into(),
            anim_type: "route".into(),
            sort_order: 1,
            speed: 5.0,
            duration_sec: 10.0,
            replay: false,
            autoplay: false,
            geom: vec![
                [115.20, -8.67, 0.0],
                [115.21, -8.67, 15.0],
                [115.22, -8.67, 0.0],
            ],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        });
        let mut re = RouteEditState {
            editing_anim_id: Some(aid),
            selected_waypoint: Some(1),
            ..default()
        };

        assert!(remove_route_waypoint(&mut st, &mut re, aid, 1));
        assert_eq!(st.anims[0].geom.len(), 2);
        assert_eq!(re.selected_waypoint, Some(1));
        assert!(st.dirty_anim_ids.contains(&aid));

        // Removing again when only 2 points remain must be refused
        assert!(!remove_route_waypoint(&mut st, &mut re, aid, 0));
        assert_eq!(st.anims[0].geom.len(), 2);
    }
}
