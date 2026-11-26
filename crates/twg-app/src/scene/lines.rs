use std::collections::HashMap;

use bevy::prelude::*;
use twg_shared::{
    group_anim::GroupAnimMode,
    particle_anim::{is_particle_anim, particle_anim_position},
    spawn_hide_anim::SpawnHideAction,
    traffic_anim::{is_traffic_anim, parse_traffic_anim_config},
};
use uuid::Uuid;

use crate::{
    scene::{
        gizmo::TransformGizmoState,
        markers::MarkerWorldBounds,
        parse_hex_color,
        play_rts::PlayRtsState,
        playback::PlaybackOverrides,
        resolve_group_highlight_entries,
        route_edit::RouteEditState,
        traffic::resolve_traffic_scale_preview_poses,
    },
    state::{AppState, PlaybackState, SelectedEntity, ToolMode},
};

#[allow(clippy::too_many_arguments)]
pub fn draw_lines_and_routes_system(
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    play_rts: Res<PlayRtsState>,
    marker_bounds: Res<MarkerWorldBounds>,
    gizmo_state: Res<TransformGizmoState>,
    route_edit: Res<RouteEditState>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        return;
    }

    // Route animation paths: always hidden during Play Skenario for a clean tactical view
    let show_route_gizmos = state.show_gizmos && !playback.play_test_mode;
    if show_route_gizmos {
        let group_hl = resolve_group_highlight_entries(&state);
        let group_hl_mode = group_hl.as_ref().map(|(m, _)| *m);
        let group_hl_map: HashMap<Uuid, (Option<usize>, bool)> = group_hl
            .as_ref()
            .map(|(_, entries)| {
                entries
                    .iter()
                    .map(|e| (e.anim_id, (e.step_number, e.is_focused_candidate)))
                    .collect()
            })
            .unwrap_or_default();

        for anim in &state.anims {
            if state.hidden_entity_ids.contains(&anim.id) {
                continue;
            }
            if is_traffic_anim(anim) {
                let cfg = parse_traffic_anim_config(anim);
                let is_selected =
                    matches!(state.selected, SelectedEntity::Anim(id) if id == anim.id);
                let editing_lane = if route_edit.is_editing(anim.id) {
                    route_edit.traffic_lane_idx
                } else {
                    None
                };
                let is_hovered = matches!(
                    gizmo_state.hovered_entity,
                    Some(SelectedEntity::Anim(id)) if id == anim.id
                );
                let grp_info = group_hl_map.get(&anim.id).copied();
                let is_grp_focused = grp_info.is_some_and(|(_, foc)| foc);
                let is_grp_member = grp_info.is_some();
                let lane_palette = [
                    Color::srgb(0.08, 0.85, 0.65),
                    Color::srgb(0.98, 0.68, 0.16),
                    Color::srgb(0.22, 0.74, 0.98),
                    Color::srgb(0.76, 0.48, 0.98),
                    Color::srgb(0.96, 0.42, 0.56),
                ];
                let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                for (lane_idx, lane) in cfg.lanes.iter().enumerate() {
                    if lane.geom.len() < 2 {
                        continue;
                    }
                    let is_this_lane_editing = editing_lane == Some(lane_idx);
                    let base_col = if !lane.enabled {
                        Color::srgba(0.45, 0.50, 0.58, 0.45)
                    } else if is_this_lane_editing {
                        Color::srgb(0.20, 0.98, 0.62)
                    } else if is_grp_focused {
                        Color::srgb(1.0, 0.88, 0.16)
                    } else if is_grp_member {
                        match group_hl_mode.unwrap_or_default() {
                            GroupAnimMode::Combine => Color::srgb(0.28, 0.68, 1.0),
                            GroupAnimMode::Sequence => Color::srgb(0.72, 0.44, 1.0),
                        }
                    } else if is_hovered {
                        Color::srgb(0.98, 0.88, 0.28)
                    } else {
                        lane_palette[lane_idx % lane_palette.len()]
                    };
                    let pts: Vec<Vec3> = lane
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
                        gizmos.line(w[0], w[1], base_col);
                        if is_selected || is_hovered || is_grp_member || is_this_lane_editing {
                            gizmos.line(w[0] + Vec3::Y * 0.12, w[1] + Vec3::Y * 0.12, base_col);
                        }
                        // Draw directional chevron arrows along each segment so lane direction is obvious
                        let seg = w[1] - w[0];
                        let seg_len = Vec2::new(seg.x, seg.z).length();
                        if seg_len >= 4.0 {
                            let fwd = Vec3::new(seg.x, 0.0, seg.z).normalize_or_zero();
                            let right = Vec3::new(-fwd.z, 0.0, fwd.x);
                            let arrow_count = ((seg_len / 24.0).floor() as usize).clamp(1, 10);
                            for k in 0..arrow_count {
                                let t = (k as f32 + 0.5) / arrow_count as f32;
                                let tip = w[0].lerp(w[1], t) + fwd * 0.9;
                                let wing_back = tip - fwd * 1.9;
                                gizmos.line(tip, wing_back + right * 0.95, base_col);
                                gizmos.line(tip, wing_back - right * 0.95, base_col);
                            }
                        }
                    }
                    // Start spawn ring (greenish) & end despawn ring (warm)
                    if !is_this_lane_editing {
                        if let Some(&start_p) = pts.first() {
                            gizmos.circle(
                                Isometry3d::new(start_p, flat_rot),
                                if is_selected { 1.35 } else { 0.95 },
                                Color::srgb(0.16, 0.96, 0.56),
                            );
                        }
                        if let Some(&end_p) = pts.last() {
                            gizmos.circle(
                                Isometry3d::new(end_p, flat_rot),
                                if is_selected { 1.1 } else { 0.8 },
                                Color::srgb(0.96, 0.36, 0.36),
                            );
                        }
                    }
                }
                // Draw on-map vehicle length & road-width footprint guide for up to 2 preview models
                for sp in resolve_traffic_scale_preview_poses(&state) {
                    if sp.anim_id != anim.id {
                        continue;
                    }
                    let yaw = sp.road_yaw_deg.to_radians();
                    let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
                    let half_l = (sp.visual_length_m * 0.5).max(0.75);
                    let half_w = (sp.visual_width_m * 0.5).max(0.6);
                    let base = Vec3::new(sp.world_pos.x, sp.world_pos.y + 0.14, sp.world_pos.z);
                    let fl = base + fwd * half_l - right * half_w;
                    let fr = base + fwd * half_l + right * half_w;
                    let br = base - fwd * half_l + right * half_w;
                    let bl = base - fwd * half_l - right * half_w;
                    let box_col = if sp.is_active_gizmo {
                        Color::srgb(0.22, 0.92, 0.98)
                    } else {
                        Color::srgb(0.98, 0.68, 0.16)
                    };
                    let arrow_col = if sp.is_active_gizmo {
                        Color::srgb(0.98, 0.85, 0.2)
                    } else {
                        Color::srgb(0.98, 0.68, 0.16)
                    };
                    gizmos.line(fl, fr, box_col);
                    gizmos.line(fr, br, box_col);
                    gizmos.line(br, bl, box_col);
                    gizmos.line(bl, fl, box_col);
                    // Forward heading arrow in front of the bounding footprint
                    let tip = base + fwd * (half_l + 1.6);
                    gizmos.line(base + fwd * half_l, tip, arrow_col);
                    gizmos.line(tip, tip - fwd * 0.6 + right * 0.45, arrow_col);
                    gizmos.line(tip, tip - fwd * 0.6 - right * 0.45, arrow_col);
                }
                continue;
            }
            if anim.geom.len() < 2 {
                continue;
            }
            let is_editing = route_edit.is_editing(anim.id);
            let is_selected = matches!(state.selected, SelectedEntity::Anim(id) if id == anim.id);
            let is_hovered =
                matches!(gizmo_state.hovered_entity, Some(SelectedEntity::Anim(id)) if id == anim.id);
            let grp_info = group_hl_map.get(&anim.id).copied();
            let is_grp_focused = grp_info.is_some_and(|(_, foc)| foc);
            let is_grp_member = grp_info.is_some();

            let color = if is_grp_focused {
                Color::srgb(1.0, 0.88, 0.16)
            } else if is_grp_member {
                match group_hl_mode.unwrap_or_default() {
                    GroupAnimMode::Combine => Color::srgb(0.28, 0.68, 1.0),
                    GroupAnimMode::Sequence => Color::srgb(0.72, 0.44, 1.0),
                }
            } else if is_selected {
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
                if is_hovered || is_selected || is_grp_member {
                    gizmos.line(w[0] + Vec3::Y * 0.10, w[1] + Vec3::Y * 0.10, color);
                }
                if is_grp_focused {
                    gizmos.line(w[0] + Vec3::Y * 0.22, w[1] + Vec3::Y * 0.22, color);
                }
            }
            // Skip chunky wireframe spheres when crisp 2D numbered pins are active in route editor
            if !is_editing {
                let sphere_r = if is_grp_focused {
                    0.95
                } else if is_hovered || is_grp_member {
                    0.82
                } else {
                    0.55
                };
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

        // Highlight Particle member locations belonging to a selected Group (or hovered candidate)
        if let Some((g_mode, ref entries)) = group_hl {
            let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            for entry in entries {
                let Some(anim) = state.anims.iter().find(|a| a.id == entry.anim_id) else {
                    continue;
                };
                if !is_particle_anim(anim) || anim.geom.is_empty() {
                    continue;
                }
                let [lng, lat, alt] = particle_anim_position(anim);
                let loc = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt.max(0.18));
                let center = Vec3::from_array(loc);
                let p_cfg = twg_shared::particle_anim::parse_particle_anim_config(anim);
                let r = (p_cfg.radius_m as f32).clamp(1.6, 26.0);
                let col = if entry.is_focused_candidate {
                    Color::srgba(1.0, 0.88, 0.16, 0.96)
                } else {
                    match g_mode {
                        GroupAnimMode::Combine => Color::srgba(0.28, 0.68, 1.0, 0.92),
                        GroupAnimMode::Sequence => Color::srgba(0.72, 0.44, 1.0, 0.92),
                    }
                };
                gizmos.circle(Isometry3d::new(center, flat_rot), r, col);
                gizmos.circle(Isometry3d::new(center + Vec3::Y * 0.08, flat_rot), r * 0.65, col);
                gizmos.line(center, center + Vec3::Y * (p_cfg.height_m as f32).clamp(3.0, 18.0), col);
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

    // Active DrawRoute / DrawTrafficLane draft
    if matches!(
        state.tool_mode,
        ToolMode::DrawRoute | ToolMode::DrawTrafficLane
    ) && !state.draft_points_lnglat.is_empty()
    {
        let color = if state.tool_mode == ToolMode::DrawTrafficLane {
            Color::srgb(0.98, 0.72, 0.14)
        } else {
            Color::srgb(0.10, 0.92, 0.58)
        };
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

    // Active Spawn & Hide expanding tactical ground FX rings (Play Skenario & Editor Preview)
    if !overrides.spawn_hide_fx_rings.is_empty() {
        let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        for ring in &overrides.spawn_hide_fx_rings {
            let Some(m) = state.markers.iter().find(|m| m.id == ring.marker_id) else {
                continue;
            };
            let (lng, lat, alt) = if let Some(ov) = overrides.markers.get(&m.id) {
                (ov.lng, ov.lat, ov.altitude)
            } else {
                (m.lng, m.lat, m.altitude)
            };
            let loc = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
            let base_r = marker_bounds
                .bounds
                .get(&m.id)
                .map(|b| {
                    ((b.world_max.x - b.world_min.x)
                        .hypot(b.world_max.z - b.world_min.z)
                        * 0.55)
                        .clamp(1.6, 32.0)
                })
                .unwrap_or(2.4);
            let p = ring.progress.clamp(0.0, 1.0);
            let alpha = (1.0 - p).powf(1.25).clamp(0.0, 1.0);
            let (r_outer, r_inner, rgb) = match ring.action {
                SpawnHideAction::Spawn => {
                    let expand = 0.55 + 1.35 * (1.0 - (1.0 - p).powi(2));
                    (base_r * expand, base_r * (expand * 0.72), (0.12, 0.96, 0.62))
                }
                SpawnHideAction::Hide => {
                    let contract = 1.65 - 1.10 * p;
                    (
                        base_r * contract,
                        base_r * (contract * 0.68),
                        (0.98, 0.28, 0.42),
                    )
                }
            };
            let c_main = Color::srgba(rgb.0, rgb.1, rgb.2, (alpha * 0.95).clamp(0.0, 1.0));
            let c_soft = Color::srgba(rgb.0, rgb.1, rgb.2, (alpha * 0.55).clamp(0.0, 1.0));
            let center = Vec3::new(loc[0], loc[1] + 0.14, loc[2]);
            gizmos.circle(Isometry3d::new(center, flat_rot), r_outer, c_main);
            gizmos.circle(
                Isometry3d::new(center + Vec3::Y * 0.06, flat_rot),
                r_inner,
                c_soft,
            );
            // 4 subtle rising tactical shimmer ticks around the ring
            let tick_h = (base_r * 0.85).clamp(1.2, 7.0) * alpha;
            let tick_y0 = center.y + (base_r * 0.45 * p);
            for k in 0..4 {
                let ang = (k as f32) * std::f32::consts::FRAC_PI_2 + p * 0.8;
                let dir = Vec3::new(ang.cos(), 0.0, ang.sin());
                let p0 = Vec3::new(
                    center.x + dir.x * r_inner,
                    tick_y0,
                    center.z + dir.z * r_inner,
                );
                let p1 = p0 + Vec3::Y * tick_h;
                gizmos.line(p0, p1, c_soft);
            }
        }
    }

    // Play Skenario interactive RTS active movement paths & Right-Click destination ripples
    if playback.play_test_mode {
        let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);

        // Subtle tactical paths for moving RTS units
        for (&uid, order) in &play_rts.unit_orders {
            if !order.moving || order.waypoints_xz.is_empty() {
                continue;
            }
            let is_selected = play_rts.selected_unit_ids.contains(&uid);
            let line_col = if is_selected {
                Color::srgba(0.18, 0.98, 0.68, 0.72)
            } else {
                Color::srgba(0.18, 0.90, 0.62, 0.34)
            };
            let loc = state
                .geo_anchor
                .lng_lat_alt_to_local(order.lng, order.lat, order.altitude);
            let mut prev = Vec3::new(loc[0], 0.18, loc[2]);
            for wp in order.waypoints_xz.iter().skip(order.waypoint_idx) {
                let next = Vec3::new(wp[0] as f32, 0.18, wp[1] as f32);
                gizmos.line(prev, next, line_col);
                prev = next;
            }
            // Small destination slot marker on the ground
            let dest_r = (order.agent_radius_m as f32 * 0.45).clamp(0.32, 0.95);
            gizmos.circle(Isometry3d::new(prev, flat_rot), dest_r, line_col);
        }

        // Expanding target ripple pings on Right-Click move command
        for ping in &play_rts.move_pings {
            let p = (ping.age_sec / ping.duration_sec.max(0.05)).clamp(0.0, 1.0);
            let alpha = (1.0 - p).powf(1.3).clamp(0.0, 1.0);
            let c_main = Color::srgba(0.16, 0.98, 0.66, alpha * 0.95);
            let c_soft = Color::srgba(0.35, 0.96, 0.88, alpha * 0.65);
            let r_outer = 0.45 + p * 3.4;
            let r_inner = (2.0 * (1.0 - p * 0.72)).max(0.25);
            let center = Vec3::new(ping.world_pos.x, 0.16, ping.world_pos.z);
            gizmos.circle(Isometry3d::new(center, flat_rot), r_outer, c_main);
            gizmos.circle(
                Isometry3d::new(center + Vec3::Y * 0.03, flat_rot),
                r_inner,
                c_soft,
            );
            let tick_h = (2.2 * (1.0 - p)).max(0.15);
            gizmos.line(center, center + Vec3::Y * tick_h, c_main);
        }
    }
}
