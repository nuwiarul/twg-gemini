use bevy::prelude::*;

use crate::{
    scene::{
        gizmo::TransformGizmoState,
        parse_hex_color,
        route_edit::RouteEditState,
    },
    state::{AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode},
};

pub fn draw_lines_and_routes_system(
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    gizmo_state: Res<TransformGizmoState>,
    route_edit: Res<RouteEditState>,
    mut gizmos: Gizmos,
) {
    if state.screen != AppScreen::PlotSkenario {
        return;
    }

    // Saved tactical plot lines
    for line in &state.lines {
        if state.hidden_entity_ids.contains(&line.id) || line.geom.len() < 2 {
            continue;
        }
        let is_selected = matches!(state.selected, SelectedEntity::Line(id) if id == line.id);
        let color = if is_selected {
            Color::srgb(0.15, 0.98, 0.65)
        } else {
            parse_hex_color(&line.color, line.alpha as f32)
        };

        let pts: Vec<Vec3> = line
            .geom
            .iter()
            .map(|p| Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.35)))
            .collect();
        for w in pts.windows(2) {
            gizmos.line(w[0], w[1], color);
            // Draw slightly offset companion lines to suggest width in 3D view
            if line.width > 2.5 {
                gizmos.line(w[0] + Vec3::Y * 0.12, w[1] + Vec3::Y * 0.12, color);
            }
        }
    }

    // Route animation paths: always hidden during Play Skenario for a clean tactical view
    let show_route_gizmos = state.show_gizmos && !playback.play_test_mode;
    if show_route_gizmos {
        for anim in &state.anims {
            if state.hidden_entity_ids.contains(&anim.id) || anim.geom.len() < 2 {
                continue;
            }
            let is_editing = route_edit.is_editing(anim.id);
            let is_selected = matches!(state.selected, SelectedEntity::Anim(id) if id == anim.id);
            let is_hovered =
                matches!(gizmo_state.hovered_entity, Some(SelectedEntity::Anim(id)) if id == anim.id);
            let color = if is_selected {
                Color::srgb(0.15, 0.98, 0.65)
            } else if is_hovered {
                Color::srgb(0.98, 0.84, 0.25)
            } else {
                Color::srgba(0.15, 0.85, 0.95, 0.82)
            };
            let pts: Vec<Vec3> = anim
                .geom
                .iter()
                .map(|p| {
                    Vec3::from_array(
                        state
                            .geo_anchor
                            .lng_lat_alt_to_local(p[0], p[1], p[2].max(0.45)),
                    )
                })
                .collect();
            for w in pts.windows(2) {
                gizmos.line(w[0], w[1], color);
                if is_hovered || is_selected {
                    gizmos.line(w[0] + Vec3::Y * 0.08, w[1] + Vec3::Y * 0.08, color);
                }
            }
            // Skip chunky wireframe spheres when crisp 2D numbered pins are active in route editor
            if !is_editing {
                let sphere_r = if is_hovered { 0.85 } else { 0.55 };
                for (pt_geo, p) in anim.geom.iter().zip(pts.iter()) {
                    gizmos.sphere(Isometry3d::from_translation(*p), sphere_r, color);
                    if pt_geo[2] > 0.5 {
                        let ground = Vec3::new(p.x, 0.05, p.z);
                        gizmos.line(ground, *p, Color::srgba(0.20, 0.88, 0.62, 0.45));
                    }
                }
            }

            // Live 3D Formation Ghost Preview at Route Start (#1) for selected Multi-Unit Route
            if is_selected && twg_shared::route_formation::is_multi_route_anim(anim) {
                let cfg = twg_shared::route_formation::parse_multi_route_config(anim);
                let poses =
                    twg_shared::route_formation::sample_multi_route_poses(&anim.geom, &cfg, 0.0);
                let start_anchor = pts[0];
                let ring_col = Color::srgba(0.22, 0.88, 0.98, 0.88);
                let inner_col = Color::srgba(0.22, 0.88, 0.98, 0.42);
                let link_col = Color::srgba(0.22, 0.88, 0.98, 0.32);
                let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                for (idx, pose) in poses.iter().enumerate() {
                    let loc = state
                        .geo_anchor
                        .lng_lat_alt_to_local(pose.lng, pose.lat, pose.altitude.max(0.12));
                    let center = Vec3::from_array(loc);
                    let r = if idx == 0 { 0.95 } else { 0.78 };
                    gizmos.circle(Isometry3d::new(center, flat_rot), r, ring_col);
                    gizmos.circle(Isometry3d::new(center, flat_rot), r * 0.55, inner_col);
                    // Forward heading arrow lying on the ground plane
                    let yaw = (pose.rot_y as f32).to_radians();
                    let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
                    let tip = center + fwd * (r * 1.45);
                    gizmos.line(center, tip, ring_col);
                    gizmos.line(tip, tip - fwd * 0.42 + right * 0.28, ring_col);
                    gizmos.line(tip, tip - fwd * 0.42 - right * 0.28, ring_col);
                    // Subtle tether from formation ghost slot to Waypoint #1
                    gizmos.line(
                        Vec3::new(center.x, start_anchor.y.min(center.y + 0.15), center.z),
                        start_anchor,
                        link_col,
                    );
                }
            }
        }
    }

    // Active DrawLine draft
    if state.tool_mode == ToolMode::DrawLine && !state.draft_points_lnglat.is_empty() {
        let color = parse_hex_color(&state.new_line_color, 1.0);
        let pts: Vec<Vec3> = state
            .draft_points_lnglat
            .iter()
            .map(|p| Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.45)))
            .collect();
        for p in &pts {
            gizmos.sphere(Isometry3d::from_translation(*p), 1.0, color);
        }
        for w in pts.windows(2) {
            gizmos.line(w[0], w[1], color);
        }
        if let (Some(&last_pt), Some([clng, clat])) = (pts.last(), state.cursor_lnglat) {
            let cur = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.55));
            gizmos.line(last_pt, cur, Color::srgba(0.25, 0.96, 0.68, 0.72));
        }
    }

    // Active DrawRoute draft
    if state.tool_mode == ToolMode::DrawRoute && !state.draft_points_lnglat.is_empty() {
        let color = Color::srgb(0.10, 0.92, 0.58);
        let pts: Vec<Vec3> = state
            .draft_points_lnglat
            .iter()
            .map(|p| Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.55)))
            .collect();
        for p in &pts {
            gizmos.sphere(Isometry3d::from_translation(*p), 1.1, color);
        }
        for w in pts.windows(2) {
            gizmos.line(w[0], w[1], color);
        }
        if let (Some(&last_pt), Some([clng, clat])) = (pts.last(), state.cursor_lnglat) {
            let cur = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.55));
            gizmos.line(last_pt, cur, Color::srgba(0.25, 0.96, 0.68, 0.72));
        }
    }

    // Active Measure tool segments
    if !state.measure_points_lnglat.is_empty() {
        let color = Color::srgb(0.98, 0.35, 0.65);
        let pts: Vec<Vec3> = state
            .measure_points_lnglat
            .iter()
            .map(|p| Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.5)))
            .collect();
        for p in &pts {
            gizmos.sphere(Isometry3d::from_translation(*p), 1.1, color);
        }
        for w in pts.windows(2) {
            gizmos.line(w[0], w[1], color);
        }
    }
}
