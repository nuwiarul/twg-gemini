use bevy::prelude::*;

use crate::{
    scene::parse_hex_color,
    state::{AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode},
};

pub fn draw_lines_and_routes_system(
    state: Res<AppState>,
    playback: Res<PlaybackState>,
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

    // Route animation paths (hidden in Clean Play View or when Gizmos toggled off)
    let show_route_gizmos =
        state.show_gizmos && !(playback.play_test_mode && playback.hide_gizmos_in_play);
    if show_route_gizmos {
        for anim in &state.anims {
            if state.hidden_entity_ids.contains(&anim.id) || anim.geom.len() < 2 {
                continue;
            }
            let is_selected = matches!(state.selected, SelectedEntity::Anim(id) if id == anim.id);
            let color = if is_selected {
                Color::srgb(0.15, 0.98, 0.65)
            } else {
                Color::srgba(0.15, 0.85, 0.95, 0.75)
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
            }
            for p in &pts {
                gizmos.sphere(Isometry3d::from_translation(*p), 0.7, color);
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
