use std::collections::HashMap;

use bevy::prelude::*;
use twg_shared::{
    group_anim::GroupAnimMode,
    particle_anim::{is_particle_anim, particle_anim_position},
    spawn_hide_anim::SpawnHideAction,
    traffic_anim::{
        TrafficLightColor, default_traffic_light_transform, is_traffic_flow_or_signal,
        is_traffic_signal_anim, parse_traffic_anim_config, project_point_onto_lane,
        resolve_traffic_light_visual_poses, sample_lane_local_frame,
    },
    traffic_diversion::{is_traffic_diversion_anim, parse_traffic_diversion_config},
    traffic_queue::{is_traffic_queue_anim, parse_traffic_queue_config},
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
            if is_traffic_flow_or_signal(anim) {
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

                // Draw Stop-Line guides, pole-to-stop-line tethers, and selected Traffic Light ring
                if is_traffic_signal_anim(anim) {
                    let light_poses = state
                        .traffic_signal_live_status
                        .get(&anim.id)
                        .map(|s| s.light_states.clone())
                        .unwrap_or_else(|| {
                            resolve_traffic_light_visual_poses(&cfg, state.geo_anchor, 0.0)
                        });
                    for lp in &light_poses {
                        let is_tl_selected =
                            state.selected_traffic_light == Some((anim.id, lp.light_id));
                        let sig_col = if !lp.enabled {
                            Color::srgba(0.55, 0.60, 0.68, 0.5)
                        } else {
                            match lp.color {
                                TrafficLightColor::Red => Color::srgb(0.98, 0.24, 0.24),
                                TrafficLightColor::Yellow => Color::srgb(0.98, 0.80, 0.14),
                                TrafficLightColor::Green => Color::srgb(0.14, 0.94, 0.46),
                            }
                        };
                        let sl_a = Vec3::from_array(lp.stop_line_a) + Vec3::Y * 0.14;
                        let sl_b = Vec3::from_array(lp.stop_line_b) + Vec3::Y * 0.14;
                        let sl_c = Vec3::from_array(lp.stop_center) + Vec3::Y * 0.16;
                        let pole = Vec3::from_array(lp.pos) + Vec3::Y * 0.16;
                        gizmos.line(sl_a, sl_b, sig_col);
                        gizmos.line(sl_a + Vec3::Y * 0.08, sl_b + Vec3::Y * 0.08, sig_col);
                        // Subtle tether from stop-line center to its 3D pole base
                        gizmos.line(
                            sl_c,
                            pole,
                            if is_tl_selected {
                                Color::srgb(0.98, 0.86, 0.22)
                            } else {
                                Color::srgba(0.95, 0.96, 1.0, 0.45)
                            },
                        );
                        if is_tl_selected {
                            gizmos.circle(
                                Isometry3d::new(pole, flat_rot),
                                1.45,
                                Color::srgb(0.98, 0.86, 0.22),
                            );
                            gizmos.circle(
                                Isometry3d::new(pole, flat_rot),
                                0.85,
                                sig_col,
                            );
                        } else if is_selected {
                            gizmos.circle(Isometry3d::new(pole, flat_rot), 0.95, sig_col);
                        }

                        // Preview the No-Overtake Zone ahead of the stop line on the map when selected
                        if (is_selected || is_tl_selected)
                            && lp.enabled
                            && cfg.allow_overtaking
                            && cfg.signal_no_overtake_zone_m > 0.1
                            && let Some(tl) =
                                cfg.traffic_lights.iter().find(|t| t.id == lp.light_id)
                            && let Some(lane) = cfg.lanes.iter().find(|l| l.id == tl.lane_id)
                        {
                            let stop_s = tl.stop_s_m;
                            let start_s = (stop_s - cfg.signal_no_overtake_zone_m).max(0.0);
                            if stop_s - start_s > 0.5 {
                                let zone_col = if is_tl_selected {
                                    Color::srgba(0.98, 0.36, 0.28, 0.95)
                                } else {
                                    Color::srgba(0.98, 0.36, 0.28, 0.45)
                                };
                                let half_w = (cfg.lane_width_m * 0.48).clamp(1.1, 4.2);
                                // Transverse boundary bar at start of no-overtake zone
                                if let Some((bp, _, br)) =
                                    sample_lane_local_frame(lane, state.geo_anchor, start_s)
                                {
                                    let ba = Vec3::new(
                                        (bp[0] - br.0 * half_w) as f32,
                                        bp[1] as f32 + 0.16,
                                        (bp[2] - br.1 * half_w) as f32,
                                    );
                                    let bb = Vec3::new(
                                        (bp[0] + br.0 * half_w) as f32,
                                        bp[1] as f32 + 0.16,
                                        (bp[2] + br.1 * half_w) as f32,
                                    );
                                    gizmos.line(ba, bb, zone_col);
                                }
                                // Dashed side rails along the no-overtake zone
                                let span = stop_s - start_s;
                                let dash_step = (span / 18.0).clamp(3.0, 22.0);
                                let mut s_cur = start_s;
                                while s_cur < stop_s - 0.5 {
                                    let s_end = (s_cur + dash_step * 0.55).min(stop_s);
                                    if let (Some((p0, _, r0)), Some((p1, _, r1))) = (
                                        sample_lane_local_frame(lane, state.geo_anchor, s_cur),
                                        sample_lane_local_frame(lane, state.geo_anchor, s_end),
                                    ) {
                                        for sign in [-1.0_f64, 1.0_f64] {
                                            let q0 = Vec3::new(
                                                (p0[0] + r0.0 * sign * half_w) as f32,
                                                p0[1] as f32 + 0.14,
                                                (p0[2] + r0.1 * sign * half_w) as f32,
                                            );
                                            let q1 = Vec3::new(
                                                (p1[0] + r1.0 * sign * half_w) as f32,
                                                p1[1] as f32 + 0.14,
                                                (p1[2] + r1.1 * sign * half_w) as f32,
                                            );
                                            gizmos.line(q0, q1, zone_col);
                                        }
                                    }
                                    s_cur += dash_step;
                                }
                            }
                        }
                    }
                }
                continue;
            }
            if is_traffic_queue_anim(anim) {
                let cfg = parse_traffic_queue_config(anim);
                let is_selected =
                    matches!(state.selected, SelectedEntity::Anim(id) if id == anim.id);
                let editing_encoded = if route_edit.is_editing(anim.id) {
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
                let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);

                for (c_idx, cor) in cfg.corridors.iter().enumerate() {
                    let q_encoded = c_idx * 2;
                    let cl_encoded = c_idx * 2 + 1;
                    let editing_q = editing_encoded == Some(q_encoded);
                    let editing_cl = editing_encoded == Some(cl_encoded);

                    // 1. Jalur Antre (`queue_geom`): warm amber/orange + bold stop-line bar at end
                    if cor.queue_geom.len() >= 2 {
                        let q_col = if !cor.enabled {
                            Color::srgba(0.45, 0.50, 0.58, 0.45)
                        } else if editing_q {
                            Color::srgb(0.20, 0.98, 0.62)
                        } else if is_grp_focused {
                            Color::srgb(1.0, 0.88, 0.16)
                        } else if is_grp_member {
                            match group_hl_mode.unwrap_or_default() {
                                GroupAnimMode::Combine => Color::srgb(0.28, 0.68, 1.0),
                                GroupAnimMode::Sequence => Color::srgb(0.72, 0.44, 1.0),
                            }
                        } else if is_hovered {
                            Color::srgb(1.0, 0.82, 0.24)
                        } else {
                            Color::srgb(0.98, 0.52, 0.14)
                        };
                        let q_pts: Vec<Vec3> = cor
                            .queue_geom
                            .iter()
                            .map(|p| {
                                Vec3::from_array(
                                    state
                                        .geo_anchor
                                        .lng_lat_alt_to_local(p[0], p[1], p[2].max(0.45)),
                                )
                            })
                            .collect();
                        for w in q_pts.windows(2) {
                            gizmos.line(w[0], w[1], q_col);
                            if is_selected || is_hovered || is_grp_member || editing_q {
                                gizmos.line(w[0] + Vec3::Y * 0.12, w[1] + Vec3::Y * 0.12, q_col);
                            }
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
                                    gizmos.line(tip, wing_back + right * 0.95, q_col);
                                    gizmos.line(tip, wing_back - right * 0.95, q_col);
                                }
                            }
                        }
                        if !editing_q {
                            if let Some(&start_p) = q_pts.first() {
                                gizmos.circle(
                                    Isometry3d::new(start_p, flat_rot),
                                    if is_selected { 1.3 } else { 0.9 },
                                    Color::srgb(0.98, 0.68, 0.16),
                                );
                            }
                            // Draw crisp perpendicular STOP LINE bar at end of `queue_geom`
                            if q_pts.len() >= 2 {
                                let p_prev = q_pts[q_pts.len() - 2];
                                let p_end = q_pts[q_pts.len() - 1];
                                let seg = p_end - p_prev;
                                let fwd = Vec3::new(seg.x, 0.0, seg.z).normalize_or_zero();
                                let right = Vec3::new(-fwd.z, 0.0, fwd.x);
                                let stop_col = if cor.clear_geom.len() >= 2 {
                                    Color::srgb(0.98, 0.24, 0.28)
                                } else {
                                    Color::srgb(1.0, 0.85, 0.15)
                                };
                                gizmos.line(p_end - right * 2.4, p_end + right * 2.4, stop_col);
                                gizmos.line(
                                    p_end - right * 2.4 + Vec3::Y * 0.14,
                                    p_end + right * 2.4 + Vec3::Y * 0.14,
                                    stop_col,
                                );
                                if cor.clear_geom.len() < 2 {
                                    gizmos.circle(
                                        Isometry3d::new(p_end, flat_rot),
                                        1.6,
                                        Color::srgb(1.0, 0.85, 0.15),
                                    );
                                }
                            }
                        }
                    }

                    // 2. Jalur Lancar (`clear_geom`): bright emerald/cyan continuation to exit
                    if cor.clear_geom.len() >= 2 {
                        let cl_col = if !cor.enabled {
                            Color::srgba(0.35, 0.55, 0.50, 0.4)
                        } else if editing_cl {
                            Color::srgb(0.22, 0.94, 0.98)
                        } else if is_hovered {
                            Color::srgb(0.30, 0.98, 0.72)
                        } else {
                            Color::srgb(0.08, 0.86, 0.62)
                        };
                        let cl_pts: Vec<Vec3> = cor
                            .clear_geom
                            .iter()
                            .map(|p| {
                                Vec3::from_array(
                                    state
                                        .geo_anchor
                                        .lng_lat_alt_to_local(p[0], p[1], p[2].max(0.45)),
                                )
                            })
                            .collect();
                        for w in cl_pts.windows(2) {
                            gizmos.line(w[0], w[1], cl_col);
                            if is_selected || is_hovered || is_grp_member || editing_cl {
                                gizmos.line(w[0] + Vec3::Y * 0.12, w[1] + Vec3::Y * 0.12, cl_col);
                            }
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
                                    gizmos.line(tip, wing_back + right * 0.95, cl_col);
                                    gizmos.line(tip, wing_back - right * 0.95, cl_col);
                                }
                            }
                        }
                        if !editing_cl
                            && let Some(&end_p) = cl_pts.last()
                        {
                            gizmos.circle(
                                Isometry3d::new(end_p, flat_rot),
                                if is_selected { 1.15 } else { 0.8 },
                                Color::srgb(0.10, 0.92, 0.66),
                            );
                        }
                    }
                }

                // Draw on-map vehicle footprint guides for up to 2 preview models
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
                    let tip = base + fwd * (half_l + 1.6);
                    gizmos.line(base + fwd * half_l, tip, arrow_col);
                    gizmos.line(tip, tip - fwd * 0.6 + right * 0.45, arrow_col);
                    gizmos.line(tip, tip - fwd * 0.6 - right * 0.45, arrow_col);
                }
                continue;
            }
            if is_traffic_diversion_anim(anim) {
                let cfg = parse_traffic_diversion_config(anim);
                let is_selected =
                    matches!(state.selected, SelectedEntity::Anim(id) if id == anim.id);
                let editing_encoded = if route_edit.is_editing(anim.id) {
                    route_edit.traffic_lane_idx
                } else {
                    None
                };
                let is_hovered = matches!(
                    gizmo_state.hovered_entity,
                    Some(SelectedEntity::Anim(id)) if id == anim.id
                );
                let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);

                for (c_idx, cor) in cfg.corridors.iter().enumerate() {
                    let parts = [
                        (0usize, &cor.approach_geom, Color::srgb(0.22, 0.74, 0.98)),
                        (1usize, &cor.main_geom, Color::srgb(0.08, 0.88, 0.56)),
                        (2usize, &cor.divert_geom, Color::srgb(0.98, 0.62, 0.12)),
                    ];
                    for (part, geom, default_col) in parts {
                        if geom.len() < 2 {
                            continue;
                        }
                        let encoded = c_idx * 3 + part;
                        let is_editing_part = editing_encoded == Some(encoded);
                        let col = if !cor.enabled {
                            Color::srgba(0.45, 0.50, 0.58, 0.45)
                        } else if is_editing_part {
                            Color::srgb(0.20, 0.98, 0.68)
                        } else if is_hovered {
                            Color::srgb(1.0, 0.86, 0.28)
                        } else {
                            default_col
                        };
                        let pts: Vec<Vec3> = geom
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
                            gizmos.line(w[0], w[1], col);
                            if is_selected || is_hovered || is_editing_part {
                                gizmos.line(w[0] + Vec3::Y * 0.12, w[1] + Vec3::Y * 0.12, col);
                            }
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
                                    gizmos.line(tip, wing_back + right * 0.95, col);
                                    gizmos.line(tip, wing_back - right * 0.95, col);
                                }
                            }
                        }
                        if !is_editing_part {
                            if part == 0 {
                                if let Some(&start_p) = pts.first() {
                                    gizmos.circle(
                                        Isometry3d::new(start_p, flat_rot),
                                        if is_selected { 1.3 } else { 0.9 },
                                        Color::srgb(0.22, 0.74, 0.98),
                                    );
                                }
                                // Bold split-point junction ring at the end of `approach_geom`
                                if let Some(&split_p) = pts.last() {
                                    let split_col = if cor.is_complete() {
                                        Color::srgb(0.98, 0.84, 0.18)
                                    } else {
                                        Color::srgb(0.98, 0.30, 0.35)
                                    };
                                    gizmos.circle(
                                        Isometry3d::new(split_p, flat_rot),
                                        if is_selected { 1.85 } else { 1.4 },
                                        split_col,
                                    );
                                }
                            } else if let Some(&end_p) = pts.last() {
                                gizmos.circle(
                                    Isometry3d::new(end_p, flat_rot),
                                    if is_selected { 1.1 } else { 0.8 },
                                    col,
                                );
                            }
                        }
                    }
                }

                // Draw on-map vehicle footprint guides for up to 2 preview models
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

    // Active DrawRoute / DrawTrafficLane / DrawTrafficQueueLane / DrawTrafficDiversionLane draft
    if matches!(
        state.tool_mode,
        ToolMode::DrawRoute
            | ToolMode::DrawTrafficLane
            | ToolMode::DrawTrafficQueueLane
            | ToolMode::DrawTrafficDiversionLane
    ) && !state.draft_points_lnglat.is_empty()
    {
        let color = if state.tool_mode == ToolMode::DrawTrafficDiversionLane {
            match state.drawing_diversion_target {
                Some((_, Some((_, 1)))) => Color::srgb(0.08, 0.92, 0.58),
                Some((_, Some((_, 2)))) => Color::srgb(0.98, 0.62, 0.12),
                _ => Color::srgb(0.22, 0.74, 0.98),
            }
        } else if state.tool_mode == ToolMode::DrawTrafficQueueLane {
            if matches!(state.drawing_traffic_queue_target, Some((_, Some(_)))) {
                Color::srgb(0.08, 0.92, 0.64)
            } else {
                Color::srgb(0.98, 0.52, 0.14)
            }
        } else if state.tool_mode == ToolMode::DrawTrafficLane {
            if state.drawing_traffic_is_signal {
                Color::srgb(0.96, 0.32, 0.46)
            } else {
                Color::srgb(0.98, 0.72, 0.14)
            }
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

    // Active PlaceTrafficLight live snap preview onto the nearest lane of the target animation
    if state.tool_mode == ToolMode::PlaceTrafficLight
        && let Some(target_aid) = state.placing_traffic_light_for_anim_id
        && let Some([clng, clat]) = state.cursor_lnglat
        && let Some(anim) = state
            .anims
            .iter()
            .find(|a| a.id == target_aid && is_traffic_signal_anim(a))
    {
        let cfg = parse_traffic_anim_config(anim);
        let cur_local = state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.0);
        let mut best: Option<(usize, f64, f64)> = None;
        for (li, lane) in cfg.lanes.iter().enumerate() {
            if let Some((s, dist)) = project_point_onto_lane(
                lane,
                state.geo_anchor,
                (cur_local[0] as f64, cur_local[2] as f64),
            ) && best.is_none_or(|b| dist < b.2)
            {
                best = Some((li, s, dist));
            }
        }
        if let Some((li, stop_s, _)) = best
            && let Some((stop_p, _fwd, right)) =
                sample_lane_local_frame(&cfg.lanes[li], state.geo_anchor, stop_s)
            && let Some((pole_geo, _)) = default_traffic_light_transform(
                &cfg.lanes[li],
                state.geo_anchor,
                stop_s,
                cfg.lane_width_m,
                cfg.overtake_side,
            )
        {
            let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            let half_w = (cfg.lane_width_m * 0.55).clamp(1.2, 4.5);
            let a = Vec3::new(
                (stop_p[0] - right.0 * half_w) as f32,
                stop_p[1] as f32 + 0.18,
                (stop_p[2] - right.1 * half_w) as f32,
            );
            let b = Vec3::new(
                (stop_p[0] + right.0 * half_w) as f32,
                stop_p[1] as f32 + 0.18,
                (stop_p[2] + right.1 * half_w) as f32,
            );
            let c = (a + b) * 0.5;
            let pole_l = state
                .geo_anchor
                .lng_lat_alt_to_local(pole_geo[0], pole_geo[1], pole_geo[2]);
            let pole_base = Vec3::new(pole_l[0], pole_l[1] + 0.15, pole_l[2]);
            let preview_col = Color::srgb(0.98, 0.85, 0.20);
            gizmos.line(a, b, preview_col);
            gizmos.line(a + Vec3::Y * 0.1, b + Vec3::Y * 0.1, preview_col);
            gizmos.line(c, pole_base, Color::srgba(0.98, 0.85, 0.20, 0.75));
            gizmos.circle(Isometry3d::new(pole_base, flat_rot), 1.1, preview_col);
            gizmos.line(pole_base, pole_base + Vec3::Y * 5.2, preview_col);
            gizmos.sphere(
                Isometry3d::from_translation(pole_base + Vec3::Y * 5.2),
                0.35,
                Color::srgb(0.98, 0.24, 0.24),
            );
        }
    }

    // Active PlaceBillboard cursor footprint preview on the 3D map
    if state.tool_mode == ToolMode::PlaceBillboard
        && let Some([clng, clat]) = state.cursor_lnglat
    {
        let p = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(clng, clat, 0.25));
        let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let col = Color::srgb(0.22, 0.88, 0.98);
        gizmos.circle(Isometry3d::new(p, flat_rot), 1.6, col);
        gizmos.line(p, p + Vec3::Y * 5.0, col);
        let top = p + Vec3::Y * 7.25;
        let bl = top + Vec3::new(-4.0, -2.25, 0.0);
        let br = top + Vec3::new(4.0, -2.25, 0.0);
        let tr = top + Vec3::new(4.0, 2.25, 0.0);
        let tl = top + Vec3::new(-4.0, 2.25, 0.0);
        gizmos.line(bl, br, col);
        gizmos.line(br, tr, col);
        gizmos.line(tr, tl, col);
        gizmos.line(tl, bl, col);
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
