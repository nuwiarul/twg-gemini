use bevy::prelude::*;
use bevy_egui::egui;
use uuid::Uuid;

use crate::{
    scene::gizmo::{closest_axis_param, compute_gizmo_world_radius, intersect_ray_plane},
    state::{AppScreen, AppState, PlaybackState, SelectedEntity},
};

// Keep this in lockstep with the route waypoint gizmo, so both editors have
// the same screen-space reach regardless of map zoom or camera angle.
const GIZMO_SCREEN_RADIUS_PX: f32 = 58.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotLineGizmoHandle {
    AxisX,
    AxisZ,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotLineHoverTarget {
    GizmoHandle(PlotLineGizmoHandle),
    Point(usize),
    Midpoint(usize),
    DeletePoint(usize),
}

#[derive(Debug, Clone, Copy)]
pub struct ActivePlotLineDrag {
    pub line_id: Uuid,
    pub point_index: usize,
    pub handle: PlotLineGizmoHandle,
    pub start_world_pos: Vec3,
    pub axis_world: Vec3,
    pub start_axis_param: f32,
    pub free_plane_offset: Vec3,
    pub start_cursor_px: Vec2,
    pub moved: bool,
}

#[derive(Resource, Default)]
pub struct PlotLineEditState {
    pub editing_line_id: Option<Uuid>,
    pub selected_point: Option<usize>,
    pub hovered: Option<PlotLineHoverTarget>,
    pub active_drag: Option<ActivePlotLineDrag>,
}

impl PlotLineEditState {
    pub fn clear(&mut self) {
        self.editing_line_id = None;
        self.selected_point = None;
        self.hovered = None;
        self.active_drag = None;
    }
}

fn screen_point(
    state: &AppState,
    point: [f64; 2],
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<Vec2> {
    let local = state.geo_anchor.lng_lat_alt_to_local(point[0], point[1], 0.45);
    camera.world_to_viewport(cam_gtf, Vec3::from_array(local)).ok()
}

fn point_world(state: &AppState, point: [f64; 2]) -> Vec3 {
    Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(point[0], point[1], 0.45))
}

fn point_to_segment_dist(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_sq = segment.length_squared();
    if length_sq < 1e-5 {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / length_sq).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

pub fn hit_test_any_plot_line(
    cursor_px: Vec2,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<Uuid> {
    let mut best: Option<(Uuid, f32)> = None;
    for line in &state.lines {
        if state.hidden_entity_ids.contains(&line.id)
            || state.locked_entity_ids.contains(&line.id)
            || line.geom.len() < 2
        {
            continue;
        }
        let points: Vec<Option<Vec2>> = line
            .geom
            .iter()
            .map(|&point| screen_point(state, point, camera, cam_gtf))
            .collect();
        let threshold = (line.width as f32 * 0.5 + 8.0).clamp(10.0, 22.0);
        for point in points.iter().flatten() {
            let distance = cursor_px.distance(*point);
            if distance <= threshold && best.is_none_or(|current| distance < current.1) {
                best = Some((line.id, distance));
            }
        }
        for pair in points.windows(2) {
            if let (Some(start), Some(end)) = (pair[0], pair[1]) {
                let distance = point_to_segment_dist(cursor_px, start, end);
                if distance <= threshold && best.is_none_or(|current| distance < current.1) {
                    best = Some((line.id, distance));
                }
            }
        }
    }
    best.map(|(id, _)| id)
}

pub fn hit_test_plot_line_editor(
    cursor_px: Vec2,
    state: &AppState,
    line_id: Uuid,
    selected_point: Option<usize>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<PlotLineHoverTarget> {
    let line = state.lines.iter().find(|line| line.id == line_id)?;
    if line.geom.len() < 2 {
        return None;
    }
    if let Some(index) = selected_point
        && let Some(&point) = line.geom.get(index)
    {
        let world = point_world(state, point);
        let screen = camera.world_to_viewport(cam_gtf, world).ok()?;
        let delete_pos = screen + Vec2::new(-23.0, 22.0);
        if line.geom.len() > 2 && cursor_px.distance(delete_pos) <= 11.5 {
            return Some(PlotLineHoverTarget::DeletePoint(index));
        }
        if cursor_px.distance(screen) <= 12.0 {
            return Some(PlotLineHoverTarget::GizmoHandle(PlotLineGizmoHandle::Free));
        }
        let radius = compute_gizmo_world_radius(camera, cam_gtf, world, GIZMO_SCREEN_RADIUS_PX);
        let mut best: Option<(PlotLineGizmoHandle, f32)> = None;
        for (handle, direction) in [
            (PlotLineGizmoHandle::AxisX, Vec3::X),
            (PlotLineGizmoHandle::AxisZ, Vec3::Z),
        ] {
            if let Ok(tip) = camera.world_to_viewport(cam_gtf, world + direction * radius) {
                let distance = point_to_segment_dist(cursor_px, screen + (tip - screen).normalize() * 10.0, tip)
                    .min(cursor_px.distance(tip) * 0.85);
                if distance <= 12.0 && best.is_none_or(|(_, current)| distance < current) {
                    best = Some((handle, distance));
                }
            }
        }
        if let Some((handle, _)) = best {
            return Some(PlotLineHoverTarget::GizmoHandle(handle));
        }
    }
    let mut closest_point = None;
    for (index, &point) in line.geom.iter().enumerate() {
        if let Some(screen) = screen_point(state, point, camera, cam_gtf) {
            let distance = cursor_px.distance(screen);
            if distance <= 13.0 && closest_point.is_none_or(|(_, best)| distance < best) {
                closest_point = Some((index, distance));
            }
        }
    }
    if let Some((index, _)) = closest_point {
        return Some(PlotLineHoverTarget::Point(index));
    }
    let mut closest_midpoint = None;
    for index in 0..line.geom.len() - 1 {
        let a = line.geom[index];
        let b = line.geom[index + 1];
        let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        if let Some(screen) = screen_point(state, midpoint, camera, cam_gtf) {
            let distance = cursor_px.distance(screen);
            if distance <= 11.5 && closest_midpoint.is_none_or(|(_, best)| distance < best) {
                closest_midpoint = Some((index, distance));
            }
        }
    }
    closest_midpoint.map(|(index, _)| PlotLineHoverTarget::Midpoint(index))
}

fn sync_line_edit(state: &mut AppState, line_id: Uuid) {
    let updated = state.lines.iter().find(|line| line.id == line_id).cloned();
    if state.line_edit.as_ref().is_some_and(|line| line.id == line_id) {
        state.line_edit = updated;
    }
    state.dirty_line_ids.insert(line_id);
}

pub fn move_plot_line_point(
    state: &mut AppState,
    line_id: Uuid,
    point_index: usize,
    handle: PlotLineGizmoHandle,
    world: Vec3,
) {
    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([world.x, 0.0, world.z]);
    let Some(line) = state.lines.iter_mut().find(|line| line.id == line_id) else {
        return;
    };
    let Some(point) = line.geom.get_mut(point_index) else {
        return;
    };
    match handle {
        PlotLineGizmoHandle::AxisX => point[0] = lng,
        PlotLineGizmoHandle::AxisZ => point[1] = lat,
        PlotLineGizmoHandle::Free => {
            point[0] = lng;
            point[1] = lat;
        }
    }
    sync_line_edit(state, line_id);
}

/// Starts a Plot Line point drag using the same ray/axis setup as route waypoint editing.
/// Keeping the click-time intersection prevents the point from jumping on mouse-down.
pub fn begin_plot_line_drag(
    state: &AppState,
    line_id: Uuid,
    point_index: usize,
    handle: PlotLineGizmoHandle,
    cursor_px: Vec2,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<ActivePlotLineDrag> {
    let line = state.lines.iter().find(|line| line.id == line_id)?;
    let &point = line.geom.get(point_index)?;
    let start_world_pos = point_world(state, point);
    let axis_world = match handle {
        PlotLineGizmoHandle::AxisX => Vec3::X,
        PlotLineGizmoHandle::AxisZ => Vec3::Z,
        PlotLineGizmoHandle::Free => Vec3::X,
    };
    let ray = camera.viewport_to_world(cam_gtf, cursor_px).ok()?;
    let start_axis_param = closest_axis_param(ray, start_world_pos, axis_world).unwrap_or(0.0);
    let free_plane_offset = if handle == PlotLineGizmoHandle::Free {
        intersect_ray_plane(ray, start_world_pos, Vec3::Y)
            .map(|hit| start_world_pos - hit)
            .unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO
    };

    Some(ActivePlotLineDrag {
        line_id,
        point_index,
        handle,
        start_world_pos,
        axis_world,
        start_axis_param,
        free_plane_offset,
        start_cursor_px: cursor_px,
        moved: false,
    })
}

/// Updates a Plot Line drag from its original mouse-down reference point.
/// A small threshold deliberately distinguishes a click (selection only) from a drag.
pub fn update_plot_line_drag(
    drag: &mut ActivePlotLineDrag,
    cursor_px: Vec2,
    state: &mut AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> bool {
    let total_px = cursor_px - drag.start_cursor_px;
    if !drag.moved && total_px.length_squared() < 2.25 {
        return false;
    }
    let Ok(ray) = camera.viewport_to_world(cam_gtf, cursor_px) else {
        return false;
    };
    let next_world = match drag.handle {
        PlotLineGizmoHandle::AxisX | PlotLineGizmoHandle::AxisZ => {
            let Some(axis_param) = closest_axis_param(ray, drag.start_world_pos, drag.axis_world)
            else {
                return false;
            };
            drag.start_world_pos + drag.axis_world * (axis_param - drag.start_axis_param)
        }
        PlotLineGizmoHandle::Free => {
            let Some(hit) = intersect_ray_plane(ray, drag.start_world_pos, Vec3::Y) else {
                return false;
            };
            hit + drag.free_plane_offset
        }
    };

    move_plot_line_point(
        state,
        drag.line_id,
        drag.point_index,
        drag.handle,
        next_world,
    );
    drag.moved = true;
    true
}

pub fn insert_plot_line_midpoint(
    state: &mut AppState,
    line_id: Uuid,
    segment_index: usize,
) -> Option<usize> {
    let line = state.lines.iter_mut().find(|line| line.id == line_id)?;
    if segment_index + 1 >= line.geom.len() {
        return None;
    }
    let a = line.geom[segment_index];
    let b = line.geom[segment_index + 1];
    let index = segment_index + 1;
    line.geom
        .insert(index, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
    sync_line_edit(state, line_id);
    Some(index)
}

pub fn remove_plot_line_point(state: &mut AppState, line_id: Uuid, point_index: usize) -> bool {
    let Some(line) = state.lines.iter_mut().find(|line| line.id == line_id) else {
        return false;
    };
    if line.geom.len() <= 2 || point_index >= line.geom.len() {
        state.status_banner = Some("Plot Line membutuhkan minimal 2 titik.".into());
        return false;
    }
    line.geom.remove(point_index);
    sync_line_edit(state, line_id);
    true
}

fn egui_line_color(hex: &str, alpha: f32) -> egui::Color32 {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6
        && let (Ok(red), Ok(green), Ok(blue)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        )
    {
        return egui::Color32::from_rgba_unmultiplied(red, green, blue, (alpha * 255.0) as u8);
    }
    egui::Color32::from_rgba_unmultiplied(56, 189, 248, (alpha * 255.0) as u8)
}

pub fn draw_plot_line_overlay(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    edit: &PlotLineEditState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    if state.screen != AppScreen::PlotSkenario {
        return;
    }
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("twg_plot_line_overlay"),
        ))
        .with_clip_rect(clip_rect);

    for line in &state.lines {
        if state.hidden_entity_ids.contains(&line.id) || line.geom.len() < 2 {
            continue;
        }
        let selected = state.selected == SelectedEntity::Line(line.id);
        let color = if selected {
            egui::Color32::from_rgb(52, 211, 153)
        } else {
            egui_line_color(&line.color, line.alpha as f32)
        };
        let width = line.width.clamp(1.0, 20.0) as f32;
        let points: Vec<Option<egui::Pos2>> = line
            .geom
            .iter()
            .map(|&point| screen_point(state, point, camera, cam_gtf).map(|p| egui::pos2(p.x, p.y)))
            .collect();
        for pair in points.windows(2) {
            if let (Some(start), Some(end)) = (pair[0], pair[1]) {
                painter.line_segment([start, end], egui::Stroke::new(width + 2.0, egui::Color32::from_rgba_unmultiplied(7, 13, 24, 185)));
                painter.line_segment([start, end], egui::Stroke::new(width, color));
            }
        }
    }

    if playback.play_test_mode {
        return;
    }

    let Some(line_id) = edit.editing_line_id else { return; };
    if state.selected != SelectedEntity::Line(line_id) { return; }
    let Some(line) = state.lines.iter().find(|line| line.id == line_id) else { return; };
    for index in 0..line.geom.len() - 1 {
        let a = line.geom[index];
        let b = line.geom[index + 1];
        let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        if let Some(point) = screen_point(state, midpoint, camera, cam_gtf) {
            let point = egui::pos2(point.x, point.y);
            let radius = if edit.hovered == Some(PlotLineHoverTarget::Midpoint(index)) { 9.0 } else { 7.0 };
            painter.circle_filled(point, radius + 1.0, egui::Color32::from_rgb(9, 13, 22));
            painter.circle_filled(point, radius, egui::Color32::from_rgb(14, 165, 233));
            painter.text(point, egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(13.0), egui::Color32::WHITE);
        }
    }
    for (index, &point) in line.geom.iter().enumerate() {
        let Some(point) = screen_point(state, point, camera, cam_gtf) else { continue; };
        let point = egui::pos2(point.x, point.y);
        let selected = edit.selected_point == Some(index);
        let hovered = edit.hovered == Some(PlotLineHoverTarget::Point(index));
        let radius = if selected { 10.5 } else if hovered { 9.5 } else { 8.2 };
        painter.circle_filled(point, radius + 1.6, egui::Color32::from_rgb(9, 13, 22));
        painter.circle_filled(point, radius, if selected { egui::Color32::from_rgb(250, 204, 21) } else { egui::Color32::from_rgb(248, 250, 252) });
        painter.circle_stroke(point, radius, egui::Stroke::new(2.0, egui::Color32::from_rgb(14, 165, 233)));
        painter.text(point, egui::Align2::CENTER_CENTER, format!("{}", index + 1), egui::FontId::proportional(10.0), egui::Color32::from_rgb(15, 23, 42));
        if selected && line.geom.len() > 2 {
            let delete_pos = point + egui::vec2(-23.0, 22.0);
            let color = if edit.hovered == Some(PlotLineHoverTarget::DeletePoint(index)) { egui::Color32::from_rgb(239, 68, 68) } else { egui::Color32::from_rgb(185, 28, 28) };
            painter.circle_filled(delete_pos, 9.2, egui::Color32::from_rgb(9, 13, 22));
            painter.circle_filled(delete_pos, 7.8, color);
            painter.text(delete_pos, egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(13.0), egui::Color32::WHITE);
        }
    }

    if let Some(index) = edit.selected_point
        && let Some(&point) = line.geom.get(index)
    {
        let world = point_world(state, point);
        if let Ok(origin) = camera.world_to_viewport(cam_gtf, world) {
            let origin = egui::pos2(origin.x, origin.y);
            let radius =
                compute_gizmo_world_radius(camera, cam_gtf, world, GIZMO_SCREEN_RADIUS_PX);
            let hovered_handle = match edit.hovered {
                Some(PlotLineHoverTarget::GizmoHandle(handle)) => Some(handle),
                _ => None,
            };
            let active_handle = edit.active_drag.map(|drag| drag.handle);
            for (handle, direction, color, label) in [
                (
                    PlotLineGizmoHandle::AxisX,
                    Vec3::X,
                    egui::Color32::from_rgb(239, 68, 68),
                    "X",
                ),
                (
                    PlotLineGizmoHandle::AxisZ,
                    Vec3::Z,
                    egui::Color32::from_rgb(59, 130, 246),
                    "Z",
                ),
            ] {
                if let Ok(tip) = camera.world_to_viewport(cam_gtf, world + direction * radius) {
                    let tip = egui::pos2(tip.x, tip.y);
                    let delta = tip - origin;
                    if delta.length() < 10.0 {
                        continue;
                    }
                    let unit = delta.normalized();
                    let perpendicular = egui::vec2(-unit.y, unit.x);
                    let hot = active_handle == Some(handle)
                        || (active_handle.is_none() && hovered_handle == Some(handle));
                    let draw_color = if hot {
                        egui::Color32::from_rgb(253, 224, 71)
                    } else {
                        color
                    };
                    let shaft_start = origin + unit * 11.0;
                    let arrow_length = if hot { 14.5 } else { 13.0 };
                    let shaft_end = tip - unit * (arrow_length - 2.0);
                    painter.line_segment(
                        [shaft_start, shaft_end],
                        egui::Stroke::new(if hot { 5.8 } else { 4.5 }, egui::Color32::from_rgb(9, 13, 22)),
                    );
                    painter.line_segment(
                        [shaft_start, shaft_end],
                        egui::Stroke::new(if hot { 3.6 } else { 2.6 }, draw_color),
                    );
                    let base = tip - unit * arrow_length;
                    painter.add(egui::Shape::convex_polygon(
                        vec![tip, base + perpendicular * 4.8, base - perpendicular * 4.8],
                        draw_color,
                        egui::Stroke::new(1.1, egui::Color32::from_rgb(9, 13, 22)),
                    ));
                    painter.text(
                        tip + unit * 10.0,
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(10.5),
                        draw_color,
                    );
                }
            }
            let free_hot = active_handle == Some(PlotLineGizmoHandle::Free)
                || (active_handle.is_none()
                    && hovered_handle == Some(PlotLineGizmoHandle::Free));
            let radius = if free_hot { 10.5 } else { 9.0 };
            painter.circle_filled(origin, radius + 1.5, egui::Color32::from_rgb(9, 13, 22));
            painter.circle_filled(
                origin,
                radius,
                if free_hot {
                    egui::Color32::from_rgb(253, 224, 71)
                } else {
                    egui::Color32::from_rgb(250, 204, 21)
                },
            );
            painter.circle_stroke(origin, radius, egui::Stroke::new(1.8, egui::Color32::WHITE));
            painter.line_segment(
                [origin - egui::vec2(3.8, 0.0), origin + egui::vec2(3.8, 0.0)],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(15, 23, 42)),
            );
            painter.line_segment(
                [origin - egui::vec2(0.0, 3.8), origin + egui::vec2(0.0, 3.8)],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(15, 23, 42)),
            );
        }
    }
}
