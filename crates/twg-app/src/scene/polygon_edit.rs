use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{math::Affine3A, prelude::*};
use bevy_egui::egui;
use twg_shared::models::Marker;
use uuid::Uuid;

use crate::{
    scene::{
        basic_mesh::{BasicMeshConfig, BasicMeshShape, parse_basic_mesh_config, write_basic_mesh_config},
        gizmo::{
            closest_axis_param, compute_gizmo_world_radius, intersect_ray_plane, marker_quat,
            marker_world_origin,
        },
        markers::MIN_OBJECT_3D_SCALE,
    },
    state::{AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode},
};

const VERTEX_GIZMO_SCREEN_PX: f32 = 58.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonVertexGizmoHandle {
    AxisX,
    AxisZ,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonHoverTarget {
    GizmoHandle(PolygonVertexGizmoHandle),
    Vertex(usize),
    Midpoint(usize),
}

#[derive(Debug, Clone)]
pub struct ActiveVertexDrag {
    pub marker_id: Uuid,
    pub vertex_idx: usize,
    pub handle: PolygonVertexGizmoHandle,
    pub start_world_pos: Vec3,
    pub axis_world: Vec3,
    pub plane_normal: Vec3,
    pub start_axis_param: f32,
    pub free_plane_offset: Vec3,
    pub start_cursor_px: Vec2,
    pub prev_cursor_px: Vec2,
    pub moved: bool,
}

#[derive(Resource, Default)]
pub struct PolygonEditState {
    pub editing_marker_id: Option<Uuid>,
    pub selected_vertex: Option<usize>,
    pub add_point_on_map_mode: bool,
    pub hovered: Option<PolygonHoverTarget>,
    pub active_drag: Option<ActiveVertexDrag>,
}

impl PolygonEditState {
    pub fn is_editing(&self, marker_id: Uuid) -> bool {
        self.editing_marker_id == Some(marker_id)
    }

    pub fn is_active(&self) -> bool {
        self.editing_marker_id.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    pub fn clear(&mut self) {
        self.editing_marker_id = None;
        self.selected_vertex = None;
        self.add_point_on_map_mode = false;
        self.hovered = None;
        self.active_drag = None;
    }
}

/// Ensures a Basic Mesh Polygon has explicit editable `[x, z]` vertices (converting a regular
/// N-gon into its corner points if `polygon_points` is empty).
pub fn ensure_editable_polygon_points(cfg: &mut BasicMeshConfig) {
    if cfg.polygon_points.len() >= 3 {
        return;
    }
    let sides = cfg.segments.clamp(3, 32);
    let r = cfg.radius.max(0.5);
    cfg.polygon_points = (0..sides)
        .map(|i| {
            let ang = (i as f32 / sides as f32) * TAU - FRAC_PI_2;
            [
                ((ang.cos() * r) * 100.0).round() / 100.0,
                ((ang.sin() * r) * 100.0).round() / 100.0,
            ]
        })
        .collect();
}

/// Inserts a midpoint vertex halfway between `pts[edge_idx]` and `pts[(edge_idx + 1) % len]`.
/// Returns the index of the newly inserted vertex.
pub fn insert_midpoint_after(pts: &mut Vec<[f32; 2]>, edge_idx: usize) -> usize {
    if pts.len() < 2 {
        return 0;
    }
    let i0 = edge_idx % pts.len();
    let i1 = (i0 + 1) % pts.len();
    let mid = [
        (((pts[i0][0] + pts[i1][0]) * 0.5) * 100.0).round() / 100.0,
        (((pts[i0][1] + pts[i1][1]) * 0.5) * 100.0).round() / 100.0,
    ];
    let insert_at = i0 + 1;
    pts.insert(insert_at, mid);
    insert_at
}

/// Inserts `new_pt` into the polygon edge closest to `new_pt` so the boundary stays clean
/// and avoids self-intersecting edges. Returns the index of the inserted vertex.
pub fn insert_point_at_best_edge(pts: &mut Vec<[f32; 2]>, new_pt: [f32; 2]) -> usize {
    if pts.len() < 2 {
        pts.push(new_pt);
        return pts.len() - 1;
    }
    let p = Vec2::from_array(new_pt);
    let mut best_edge = 0usize;
    let mut best_dist = f32::INFINITY;
    for i in 0..pts.len() {
        let a = Vec2::from_array(pts[i]);
        let b = Vec2::from_array(pts[(i + 1) % pts.len()]);
        let d = point_to_segment_dist(p, a, b);
        if d < best_dist {
            best_dist = d;
            best_edge = i;
        }
    }
    let rounded = [
        (new_pt[0] * 100.0).round() / 100.0,
        (new_pt[1] * 100.0).round() / 100.0,
    ];
    let insert_at = best_edge + 1;
    pts.insert(insert_at, rounded);
    insert_at
}

pub fn marker_local_to_world_affine(state: &AppState, m: &Marker) -> Affine3A {
    let origin = marker_world_origin(state, m);
    let rot = marker_quat(m);
    let scale = Vec3::new(
        (m.scale_x as f32).max(MIN_OBJECT_3D_SCALE as f32),
        (m.scale_y as f32).max(MIN_OBJECT_3D_SCALE as f32),
        (m.scale_z as f32).max(MIN_OBJECT_3D_SCALE as f32),
    );
    Affine3A::from_scale_rotation_translation(scale, rot, origin)
}

pub fn vertex_world_pos(aff: Affine3A, pt: [f32; 2], y_local: f32) -> Vec3 {
    aff.transform_point3(Vec3::new(pt[0], y_local, pt[1]))
}

pub fn world_to_local_xz(aff: Affine3A, world_pos: Vec3) -> [f32; 2] {
    let local = aff.inverse().transform_point3(world_pos);
    [
        (local.x * 100.0).round() / 100.0,
        (local.z * 100.0).round() / 100.0,
    ]
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

pub fn hit_test_polygon_editor(
    cursor_px: Vec2,
    state: &AppState,
    marker: &Marker,
    cfg: &BasicMeshConfig,
    selected_vertex: Option<usize>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<PolygonHoverTarget> {
    if cfg.shape != BasicMeshShape::Polygon || cfg.polygon_points.len() < 3 {
        return None;
    }

    let aff = marker_local_to_world_affine(state, marker);
    let rot = marker_quat(marker);
    let y_local = cfg.height.max(0.15);

    // Priority 1: Active Vertex Move Gizmo (Red X, Blue Z, Center Free Move)
    if let Some(vi) = selected_vertex
        && let Some(&pt) = cfg.polygon_points.get(vi)
    {
        let origin = vertex_world_pos(aff, pt, y_local);
        if let Ok(o2) = camera.world_to_viewport(cam_gtf, origin) {
            if cursor_px.distance(o2) <= 13.0 {
                return Some(PolygonHoverTarget::GizmoHandle(PolygonVertexGizmoHandle::Free));
            }
            let radius = compute_gizmo_world_radius(camera, cam_gtf, origin, VERTEX_GIZMO_SCREEN_PX);
            let ax = (rot * Vec3::X).normalize_or(Vec3::X);
            let az = (rot * Vec3::Z).normalize_or(Vec3::Z);
            let mut best: Option<(PolygonVertexGizmoHandle, f32)> = None;
            for (h, dir) in [
                (PolygonVertexGizmoHandle::AxisX, ax),
                (PolygonVertexGizmoHandle::AxisZ, az),
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
                return Some(PolygonHoverTarget::GizmoHandle(h));
            }
        }
    }

    // Priority 2: Vertex Dots (click any vertex to reveal its Move Gizmo)
    let mut best_v: Option<(usize, f32)> = None;
    for (i, &pt) in cfg.polygon_points.iter().enumerate() {
        let w = vertex_world_pos(aff, pt, y_local);
        if let Ok(s) = camera.world_to_viewport(cam_gtf, w) {
            let d = cursor_px.distance(s);
            if d <= 12.0 && best_v.is_none_or(|cur| d < cur.1) {
                best_v = Some((i, d));
            }
        }
    }
    if let Some((vi, _)) = best_v {
        return Some(PolygonHoverTarget::Vertex(vi));
    }

    // Priority 3: Edge Midpoint "+" Ghost Handles (click to insert a new vertex on that edge!)
    let n = cfg.polygon_points.len();
    let mut best_m: Option<(usize, f32)> = None;
    for i in 0..n {
        let a = cfg.polygon_points[i];
        let b = cfg.polygon_points[(i + 1) % n];
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let w = vertex_world_pos(aff, mid, y_local);
        if let Ok(s) = camera.world_to_viewport(cam_gtf, w) {
            let d = cursor_px.distance(s);
            if d <= 11.0 && best_m.is_none_or(|cur| d < cur.1) {
                best_m = Some((i, d));
            }
        }
    }
    best_m.map(|(edge_idx, _)| PolygonHoverTarget::Midpoint(edge_idx))
}

#[allow(clippy::too_many_arguments)]
pub fn begin_vertex_drag(
    state: &AppState,
    marker: &Marker,
    cfg: &BasicMeshConfig,
    vertex_idx: usize,
    handle: PolygonVertexGizmoHandle,
    cursor_px: Vec2,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<ActiveVertexDrag> {
    let &pt = cfg.polygon_points.get(vertex_idx)?;
    let aff = marker_local_to_world_affine(state, marker);
    let rot = marker_quat(marker);
    let y_local = cfg.height.max(0.15);
    let start_world_pos = vertex_world_pos(aff, pt, y_local);

    let axis_world = match handle {
        PolygonVertexGizmoHandle::AxisX => (rot * Vec3::X).normalize_or(Vec3::X),
        PolygonVertexGizmoHandle::AxisZ => (rot * Vec3::Z).normalize_or(Vec3::Z),
        PolygonVertexGizmoHandle::Free => Vec3::X,
    };
    let plane_normal = (rot * Vec3::Y).normalize_or(Vec3::Y);

    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let start_axis_param = closest_axis_param(ray, start_world_pos, axis_world).unwrap_or(0.0);
    let free_plane_offset = if handle == PolygonVertexGizmoHandle::Free {
        intersect_ray_plane(ray, start_world_pos, plane_normal)
            .map(|h| start_world_pos - h)
            .unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO
    };

    Some(ActiveVertexDrag {
        marker_id: marker.id,
        vertex_idx,
        handle,
        start_world_pos,
        axis_world,
        plane_normal,
        start_axis_param,
        free_plane_offset,
        start_cursor_px: cursor_px,
        prev_cursor_px: cursor_px,
        moved: false,
    })
}

pub fn update_vertex_drag(
    drag: &mut ActiveVertexDrag,
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

    let new_world = match drag.handle {
        PolygonVertexGizmoHandle::AxisX | PolygonVertexGizmoHandle::AxisZ => {
            let Some(s_curr) = closest_axis_param(ray, drag.start_world_pos, drag.axis_world) else {
                drag.prev_cursor_px = cursor_px;
                return false;
            };
            drag.start_world_pos + drag.axis_world * (s_curr - drag.start_axis_param)
        }
        PolygonVertexGizmoHandle::Free => {
            let Some(hit) = intersect_ray_plane(ray, drag.start_world_pos, drag.plane_normal) else {
                drag.prev_cursor_px = cursor_px;
                return false;
            };
            hit + drag.free_plane_offset
        }
    };

    let Some(marker_snapshot) = state.markers.iter().find(|m| m.id == drag.marker_id).cloned() else {
        return false;
    };
    let Some(mut cfg) = parse_basic_mesh_config(&marker_snapshot) else {
        return false;
    };
    if drag.vertex_idx >= cfg.polygon_points.len() {
        return false;
    }

    let aff = marker_local_to_world_affine(state, &marker_snapshot);
    let new_xz = world_to_local_xz(aff, new_world);
    cfg.polygon_points[drag.vertex_idx] = new_xz;

    if let Some(m) = state.markers.iter_mut().find(|x| x.id == drag.marker_id) {
        write_basic_mesh_config(m, &cfg);
        let updated = m.clone();
        if state.marker_edit.as_ref().is_some_and(|e| e.id == drag.marker_id) {
            state.marker_edit = Some(updated);
        }
        state.dirty_marker_ids.insert(drag.marker_id);
        drag.moved = true;
        drag.prev_cursor_px = cursor_px;
        return true;
    }

    false
}

/// Inserts a new vertex at the clicked world ground/polygon-plane point on the closest edge.
pub fn insert_vertex_from_cursor_click(
    state: &mut AppState,
    marker_id: Uuid,
    cursor_px: Vec2,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<usize> {
    let marker_snapshot = state.markers.iter().find(|m| m.id == marker_id)?.clone();
    let mut cfg = parse_basic_mesh_config(&marker_snapshot)?;
    if cfg.shape != BasicMeshShape::Polygon {
        return None;
    }
    ensure_editable_polygon_points(&mut cfg);

    let aff = marker_local_to_world_affine(state, &marker_snapshot);
    let rot = marker_quat(&marker_snapshot);
    let plane_origin = aff.transform_point3(Vec3::new(0.0, cfg.height.max(0.15), 0.0));
    let plane_normal = (rot * Vec3::Y).normalize_or(Vec3::Y);
    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let hit = intersect_ray_plane(ray, plane_origin, plane_normal)?;
    let new_xz = world_to_local_xz(aff, hit);

    let new_idx = insert_point_at_best_edge(&mut cfg.polygon_points, new_xz);
    if let Some(m) = state.markers.iter_mut().find(|x| x.id == marker_id) {
        write_basic_mesh_config(m, &cfg);
        let updated = m.clone();
        if state.marker_edit.as_ref().is_some_and(|e| e.id == marker_id) {
            state.marker_edit = Some(updated);
        }
        state.dirty_marker_ids.insert(marker_id);
        return Some(new_idx);
    }
    None
}

// ---------------------------------------------------------------------------
// Crisp Screen-Space Projected Polygon Vertex + Move Gizmo Overlay Renderer
// ---------------------------------------------------------------------------

const COL_X: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);
const COL_Z: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
const COL_HOT: egui::Color32 = egui::Color32::from_rgb(253, 224, 71);
const COL_OUTLINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(9, 13, 22, 200);

fn to_pos2(v: Vec2) -> egui::Pos2 {
    egui::pos2(v.x, v.y)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_polygon_vertex_editor(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    poly_edit: &PolygonEditState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    if state.screen != AppScreen::PlotSkenario || playback.play_test_mode {
        return;
    }

    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("twg_polygon_vertex_editor"),
        ))
        .with_clip_rect(clip_rect);

    // 1. Live numbered vertex markers while actively drawing a brand-new Polygon on the map
    if state.tool_mode == ToolMode::DrawBasicPolygon && !state.draft_points_lnglat.is_empty() {
        for (idx, pt) in state.draft_points_lnglat.iter().enumerate() {
            let local = state.geo_anchor.lng_lat_alt_to_local(pt[0], pt[1], 0.35);
            if let Ok(s) = camera.world_to_viewport(cam_gtf, Vec3::from_array(local)) {
                let p2 = to_pos2(s);
                let is_first = idx == 0 && state.draft_points_lnglat.len() >= 3;
                let r = if is_first { 9.5 } else { 7.5 };
                let fill = if is_first {
                    egui::Color32::from_rgb(16, 185, 129)
                } else {
                    egui::Color32::from_rgb(124, 58, 237)
                };
                painter.circle_filled(p2, r + 1.5, COL_OUTLINE);
                painter.circle_filled(p2, r, fill);
                painter.circle_stroke(p2, r, egui::Stroke::new(1.5, egui::Color32::WHITE));
                painter.text(
                    p2,
                    egui::Align2::CENTER_CENTER,
                    format!("{}", idx + 1),
                    egui::FontId::proportional(9.5),
                    egui::Color32::WHITE,
                );
            }
        }
        return;
    }

    // 2. Interactive Vertex Editing on an existing selected Basic Mesh Polygon
    let Some(marker_id) = poly_edit.editing_marker_id else {
        return;
    };
    if state.selected != SelectedEntity::Marker(marker_id) {
        return;
    }
    let Some(marker) = state.markers.iter().find(|m| m.id == marker_id) else {
        return;
    };
    if state.is_marker_locked(marker) {
        return;
    }
    let Some(cfg) = parse_basic_mesh_config(marker) else {
        return;
    };
    if cfg.shape != BasicMeshShape::Polygon || cfg.polygon_points.len() < 3 {
        return;
    }

    let aff = marker_local_to_world_affine(state, marker);
    let rot = marker_quat(marker);
    let y_local = cfg.height.max(0.15);
    let n = cfg.polygon_points.len();

    // Draw top perimeter outline & faint vertical stems from ground to top rim
    let mut top_2d: Vec<Option<egui::Pos2>> = Vec::with_capacity(n);
    for &pt in &cfg.polygon_points {
        let top_w = vertex_world_pos(aff, pt, y_local);
        let bot_w = vertex_world_pos(aff, pt, 0.0);
        let t2 = camera.world_to_viewport(cam_gtf, top_w).ok().map(to_pos2);
        let b2 = camera.world_to_viewport(cam_gtf, bot_w).ok().map(to_pos2);
        if let (Some(ta), Some(ba)) = (t2, b2) {
            painter.line_segment(
                [ba, ta],
                egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(167, 139, 250, 120)),
            );
        }
        top_2d.push(t2);
    }

    for i in 0..n {
        if let (Some(a), Some(b)) = (top_2d[i], top_2d[(i + 1) % n]) {
            painter.line_segment(
                [a, b],
                egui::Stroke::new(2.2, egui::Color32::from_rgba_unmultiplied(167, 139, 250, 210)),
            );
        }
    }

    // Draw Edge Midpoint "+" Handles (click to insert new vertex along that edge!)
    for i in 0..n {
        let a = cfg.polygon_points[i];
        let b = cfg.polygon_points[(i + 1) % n];
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let mid_w = vertex_world_pos(aff, mid, y_local);
        if let Ok(s) = camera.world_to_viewport(cam_gtf, mid_w) {
            let p2 = to_pos2(s);
            let is_hov = poly_edit.hovered == Some(PolygonHoverTarget::Midpoint(i));
            let r = if is_hov { 9.0 } else { 7.0 };
            painter.circle_filled(p2, r + 1.0, COL_OUTLINE);
            painter.circle_filled(
                p2,
                r,
                if is_hov {
                    egui::Color32::from_rgb(16, 185, 129)
                } else {
                    egui::Color32::from_rgba_unmultiplied(16, 185, 129, 155)
                },
            );
            painter.circle_stroke(
                p2,
                r,
                egui::Stroke::new(
                    1.2,
                    if is_hov {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(167, 243, 208)
                    },
                ),
            );
            let plus_r = 3.2;
            painter.line_segment(
                [p2 - egui::vec2(plus_r, 0.0), p2 + egui::vec2(plus_r, 0.0)],
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
            painter.line_segment(
                [p2 - egui::vec2(0.0, plus_r), p2 + egui::vec2(0.0, plus_r)],
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
        }
    }

    // Draw Numbered Polygon Vertex Dots
    for (i, t2_opt) in top_2d.iter().enumerate() {
        let Some(p2) = *t2_opt else {
            continue;
        };
        let is_sel = poly_edit.selected_vertex == Some(i);
        let is_hov = poly_edit.hovered == Some(PolygonHoverTarget::Vertex(i));
        let r = if is_sel {
            9.5
        } else if is_hov {
            8.5
        } else {
            7.0
        };
        let fill = if is_sel {
            COL_HOT
        } else if is_hov {
            egui::Color32::from_rgb(56, 189, 248)
        } else {
            egui::Color32::from_rgb(241, 245, 249)
        };
        let text_col = egui::Color32::from_rgb(15, 23, 42);
        painter.circle_filled(p2, r + 1.5, COL_OUTLINE);
        painter.circle_filled(p2, r, fill);
        painter.circle_stroke(
            p2,
            r,
            egui::Stroke::new(
                1.5,
                if is_sel {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(14, 116, 144)
                },
            ),
        );
        if !is_sel {
            painter.text(
                p2,
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                egui::FontId::proportional(9.0),
                text_col,
            );
        }
    }

    // Draw Dedicated Vertex Move Gizmo ONLY on the clicked/selected vertex!
    if let Some(vi) = poly_edit.selected_vertex
        && let Some(&pt) = cfg.polygon_points.get(vi)
    {
        let origin = vertex_world_pos(aff, pt, y_local);
        if let Ok(o2_v) = camera.world_to_viewport(cam_gtf, origin) {
            let o2 = to_pos2(o2_v);
            let radius = compute_gizmo_world_radius(camera, cam_gtf, origin, VERTEX_GIZMO_SCREEN_PX);
            let ax = (rot * Vec3::X).normalize_or(Vec3::X);
            let az = (rot * Vec3::Z).normalize_or(Vec3::Z);
            let active_h = poly_edit.active_drag.as_ref().map(|d| d.handle);
            let hovered_h = match poly_edit.hovered {
                Some(PolygonHoverTarget::GizmoHandle(h)) => Some(h),
                _ => None,
            };

            // Extended axis guide while dragging X or Z
            if let Some(d) = &poly_edit.active_drag
                && matches!(
                    d.handle,
                    PolygonVertexGizmoHandle::AxisX | PolygonVertexGizmoHandle::AxisZ
                )
                && let (Ok(p_neg), Ok(p_pos)) = (
                    camera.world_to_viewport(cam_gtf, d.start_world_pos - d.axis_world * radius * 4.0),
                    camera.world_to_viewport(cam_gtf, d.start_world_pos + d.axis_world * radius * 4.0),
                )
            {
                let gcol = if d.handle == PolygonVertexGizmoHandle::AxisX {
                    egui::Color32::from_rgba_unmultiplied(239, 68, 68, 120)
                } else {
                    egui::Color32::from_rgba_unmultiplied(59, 130, 246, 120)
                };
                painter.line_segment([to_pos2(p_neg), to_pos2(p_pos)], egui::Stroke::new(1.5, gcol));
            }

            // Draw X & Z Move Arrows
            for (h, dir, base_col, lbl) in [
                (PolygonVertexGizmoHandle::AxisX, ax, COL_X, "X"),
                (PolygonVertexGizmoHandle::AxisZ, az, COL_Z, "Z"),
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
                        egui::Color32::from_rgba_unmultiplied(base_col.r(), base_col.g(), base_col.b(), 80)
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

            // Center Free-Move Handle on selected vertex
            let free_hot = active_h == Some(PolygonVertexGizmoHandle::Free)
                || (active_h.is_none() && hovered_h == Some(PolygonVertexGizmoHandle::Free));
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
        }
    }

    // Tooltip / Live Readout Pill
    let hud_painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("twg_poly_edit_hud"),
    ));
    if let Some(d) = &poly_edit.active_drag
        && let Some(&pt) = cfg.polygon_points.get(d.vertex_idx)
    {
        let txt = format!(
            "Titik #{}: X {:.2} m, Z {:.2} m",
            d.vertex_idx + 1,
            pt[0],
            pt[1]
        );
        draw_hud_pill(
            &hud_painter,
            to_pos2(d.prev_cursor_px) + egui::vec2(15.0, -24.0),
            &txt,
            COL_HOT,
        );
    } else if let Some(hov) = poly_edit.hovered
        && let Some(ptr) = ctx.pointer_latest_pos()
    {
        let txt = match hov {
            PolygonHoverTarget::Vertex(i) => {
                format!("Titik #{} — Klik untuk tampilkan Move Gizmo", i + 1)
            }
            PolygonHoverTarget::Midpoint(e) => {
                format!("+ Sisipkan titik baru di sisi #{}", e + 1)
            }
            PolygonHoverTarget::GizmoHandle(PolygonVertexGizmoHandle::AxisX) => {
                "Geser Titik Sumbu X".to_string()
            }
            PolygonHoverTarget::GizmoHandle(PolygonVertexGizmoHandle::AxisZ) => {
                "Geser Titik Sumbu Z".to_string()
            }
            PolygonHoverTarget::GizmoHandle(PolygonVertexGizmoHandle::Free) => {
                "Free Move Titik Polygon (XZ)".to_string()
            }
        };
        draw_hud_pill(
            &hud_painter,
            ptr + egui::vec2(14.0, -22.0),
            &txt,
            egui::Color32::from_rgb(196, 181, 253),
        );
    }
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
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(167, 139, 250, 180)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, text_color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_insert_splits_edge_cleanly() {
        let mut pts = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let idx = insert_midpoint_after(&mut pts, 0);
        assert_eq!(idx, 1);
        assert_eq!(pts.len(), 5);
        assert_eq!(pts[1], [5.0, 0.0]);
    }

    #[test]
    fn best_edge_insert_chooses_closest_side() {
        let mut pts = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        // Point near the right edge (x=10, z=5) -> edge 1 (between [10,0] and [10,10])
        let idx = insert_point_at_best_edge(&mut pts, [11.0, 5.0]);
        assert_eq!(idx, 2);
        assert_eq!(pts[2], [11.0, 5.0]);
    }
}
