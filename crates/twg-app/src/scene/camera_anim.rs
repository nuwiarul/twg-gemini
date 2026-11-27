use std::collections::HashSet;

use bevy::prelude::*;
use twg_shared::{
    camera_anim::{
        CameraFollowConfig, CameraFollowStopMode, CameraStaticConfig, CameraStaticHoldMode,
        is_camera_anim, is_camera_follow_anim, is_camera_static_anim, lerp_angle_deg,
        parse_camera_follow_config, parse_camera_static_config, shortest_angle_delta_deg,
    },
    group_anim::collect_group_descendant_ids,
    models::Anim,
    route_formation::anim_bound_marker_ids,
    ws::TacticalCommand,
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    net::ws_client::{WsClientBridge, WsOutCommand},
    scene::{play_rts::PlayRtsState, playback::PlaybackOverrides},
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity},
};

#[derive(Debug, Clone, Copy)]
struct CameraPoseSnapshot {
    target_xz: Vec2,
    alt_m: f32,
    distance_m: f32,
    pitch_deg: f32,
    bearing_deg: f32,
}

impl CameraPoseSnapshot {
    fn from_tcam(tcam: &TacticalCamera) -> Self {
        Self {
            target_xz: tcam.smooth_target,
            alt_m: tcam.smooth_alt_m,
            distance_m: tcam.smooth_distance,
            pitch_deg: tcam.smooth_pitch,
            bearing_deg: tcam.smooth_bearing,
        }
    }

    fn apply_to_tcam(self, tcam: &mut TacticalCamera, tf: &mut Transform) {
        tcam.target_local = self.target_xz;
        tcam.smooth_target = self.target_xz;
        tcam.target_alt_m = self.alt_m;
        tcam.smooth_alt_m = self.alt_m;
        tcam.distance_m = self.distance_m.clamp(3.0, 80_000.0);
        tcam.smooth_distance = tcam.distance_m;
        tcam.pitch_deg = self.pitch_deg.clamp(5.0, 82.0);
        tcam.smooth_pitch = tcam.pitch_deg;
        tcam.bearing_deg = self.bearing_deg.rem_euclid(360.0);
        tcam.smooth_bearing = tcam.bearing_deg;
        tcam.apply_transform(tf);
    }
}

struct ActiveCameraRun {
    anim_id: Uuid,
    trigger_seq: u64,
    delay_elapsed: f64,
    motion_elapsed: f64,
    start_pose: Option<CameraPoseSnapshot>,
    follow_current: Option<CameraPoseSnapshot>,
    saw_target_route_active: bool,
}

#[derive(Resource, Default)]
pub struct CameraAnimRuntime {
    session_generation: Option<u64>,
    active_run: Option<ActiveCameraRun>,
}

/// Captures the live `TacticalCamera` pose directly into a `CameraStaticConfig`.
pub fn capture_camera_into_static(
    state: &AppState,
    tcam: &TacticalCamera,
    cfg: &mut CameraStaticConfig,
) {
    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
        tcam.smooth_target.x,
        0.0,
        tcam.smooth_target.y,
    ]);
    cfg.lng = (lng * 1_000_000.0).round() / 1_000_000.0;
    cfg.lat = (lat * 1_000_000.0).round() / 1_000_000.0;
    cfg.target_altitude_m = ((tcam.smooth_alt_m as f64) * 100.0).round() / 100.0;
    cfg.distance_m = ((tcam.smooth_distance as f64) * 10.0).round() / 10.0;
    cfg.pitch_deg = ((tcam.smooth_pitch as f64) * 10.0).round() / 10.0;
    cfg.bearing_deg = ((tcam.smooth_bearing.rem_euclid(360.0) as f64) * 10.0).round() / 10.0;
}

/// Immediately jumps the Editor `TacticalCamera` to the saved `CameraStaticConfig` view.
pub fn jump_camera_to_static(
    state: &AppState,
    tcam: &mut TacticalCamera,
    cfg: &CameraStaticConfig,
) {
    let local = state
        .geo_anchor
        .lng_lat_alt_to_local(cfg.lng, cfg.lat, cfg.target_altitude_m);
    tcam.target_local = Vec2::new(local[0], local[2]);
    tcam.target_alt_m = local[1];
    tcam.distance_m = (cfg.distance_m as f32).clamp(3.0, 80_000.0);
    tcam.pitch_deg = (cfg.pitch_deg as f32).clamp(5.0, 82.0);
    tcam.bearing_deg = (cfg.bearing_deg as f32).rem_euclid(360.0);
}

/// Immediately jumps the Editor `TacticalCamera` to preview a `CameraFollowConfig` angle around
/// its target marker.
pub fn jump_camera_to_follow_preview(
    state: &AppState,
    tcam: &mut TacticalCamera,
    cfg: &CameraFollowConfig,
) -> bool {
    let Some(mid) = cfg.target_marker_id else {
        return false;
    };
    let Some(marker) = state.markers.iter().find(|m| m.id == mid) else {
        return false;
    };
    let local = state.geo_anchor.lng_lat_alt_to_local(
        marker.lng,
        marker.lat,
        marker.altitude + cfg.height_offset_m,
    );
    tcam.target_local = Vec2::new(local[0], local[2]);
    tcam.target_alt_m = local[1];
    tcam.distance_m = (cfg.distance_m as f32).clamp(3.0, 80_000.0);
    tcam.pitch_deg = (cfg.pitch_deg as f32).clamp(5.0, 82.0);
    tcam.bearing_deg = cfg.desired_bearing_deg(marker.rot_y) as f32;
    true
}

/// Checks whether a marker has at least one bound route/multi-route animation in the scenario.
pub fn marker_has_route_anim(marker_id: Uuid, anims: &[Anim]) -> bool {
    anims.iter().any(|a| {
        !is_camera_anim(a)
            && !twg_shared::group_anim::is_group_anim(a)
            && !twg_shared::shoot_anim::is_shoot_anim(a)
            && !twg_shared::pointer_anim::is_pointer_arrow_anim(a)
            && !twg_shared::spawn_hide_anim::is_spawn_hide_anim(a)
            && !twg_shared::particle_anim::is_particle_anim(a)
            && !twg_shared::audio_anim::is_audio_anim(a)
            && !twg_shared::billboard_anim::is_billboard_anim(a)
            && !twg_shared::traffic_anim::is_traffic_flow_or_signal(a)
            && !twg_shared::traffic_queue::is_traffic_queue_anim(a)
            && !twg_shared::traffic_diversion::is_traffic_diversion_anim(a)
            && a.geom.len() >= 2
            && anim_bound_marker_ids(a).contains(&marker_id)
    })
}

fn target_route_state(marker_id: Uuid, anims: &[Anim], playback: &PlaybackState) -> (bool, bool) {
    let mut any_playing = false;
    let mut any_finished = false;
    for a in anims {
        if is_camera_anim(a)
            || twg_shared::group_anim::is_group_anim(a)
            || twg_shared::shoot_anim::is_shoot_anim(a)
            || twg_shared::pointer_anim::is_pointer_arrow_anim(a)
            || twg_shared::spawn_hide_anim::is_spawn_hide_anim(a)
            || twg_shared::particle_anim::is_particle_anim(a)
            || twg_shared::audio_anim::is_audio_anim(a)
            || twg_shared::billboard_anim::is_billboard_anim(a)
            || twg_shared::traffic_anim::is_traffic_flow_or_signal(a)
            || twg_shared::traffic_queue::is_traffic_queue_anim(a)
            || twg_shared::traffic_diversion::is_traffic_diversion_anim(a)
            || a.geom.len() < 2
        {
            continue;
        }
        if !anim_bound_marker_ids(a).contains(&marker_id) {
            continue;
        }
        if let Some(tr) = playback.tracks.get(&a.id) {
            match tr.status {
                AnimRunStatus::Playing | AnimRunStatus::Paused => any_playing = true,
                AnimRunStatus::Finished => any_finished = true,
                AnimRunStatus::Stopped => {}
            }
        }
    }
    (any_playing, any_finished)
}

/// Drives **Static Camera (Fly-To & Orbit)** and **Camera Follow** animations across Plot Editor
/// Preview, Play Skenario, Group Animations, and Layar Besar.
///
/// Key behaviors:
/// - **Single Active Camera (Auto-Handoff)**: launching a new camera animation smoothly takes over
///   from the current camera pose and stops any previous camera animation.
/// - **Manual Interruption**: any local mouse/trackpad/keyboard camera movement immediately stops
///   the active camera animation and leaves the camera where the user moved it.
/// - **Keep Last Position on Stop**: stopping or finishing a follow/static animation never snaps
///   the camera back; it stays cleanly at its final position.
#[allow(clippy::too_many_arguments)]
pub fn camera_anim_system(
    time: Res<Time>,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    play_rts: Res<PlayRtsState>,
    ws: Res<WsClientBridge>,
    mut runtime: ResMut<CameraAnimRuntime>,
    mut cam_q: Query<(&mut TacticalCamera, &mut Transform)>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        runtime.active_run = None;
        state.camera_status.clear();
        return;
    }

    let Ok((mut tcam, mut cam_tf)) = cam_q.single_mut() else {
        return;
    };

    // Draw subtle 3D target reticle & view ray in Plot Editor when a Static Camera is selected
    if !playback.play_test_mode
        && playback.editor_preview_anim_id.is_none()
        && let SelectedEntity::Anim(sel_id) = state.selected
        && let Some(anim) = state
            .anims
            .iter()
            .find(|a| a.id == sel_id && is_camera_static_anim(a))
    {
        let cfg = parse_camera_static_config(anim);
        let target = Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(
            cfg.lng,
            cfg.lat,
            cfg.target_altitude_m,
        ));
        let tilt = (cfg.pitch_deg as f32).to_radians();
        let bearing = (cfg.bearing_deg as f32).to_radians();
        let preview_len = (cfg.distance_m as f32).clamp(12.0, 220.0);
        let cam_pos = target
            + Vec3::new(
                -bearing.sin() * preview_len * tilt.sin(),
                (preview_len * tilt.cos()).max(1.0),
                bearing.cos() * preview_len * tilt.sin(),
            );
        let cyan = Color::srgb(0.22, 0.74, 0.97);
        let amber = Color::srgb(0.98, 0.75, 0.14);
        let ring_r = (preview_len * 0.06).clamp(1.8, 14.0);
        let iso = Isometry3d::new(target + Vec3::Y * 0.12, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
        gizmos.circle(iso, ring_r, cyan);
        gizmos.line(target, cam_pos, amber);
        gizmos.sphere(Isometry3d::from_translation(cam_pos), (ring_r * 0.35).clamp(0.8, 4.0), amber);
    }

    // Reset runtime when playback session changes
    let cur_gen = Some(playback.session_generation);
    if runtime.session_generation != cur_gen {
        runtime.session_generation = cur_gen;
        runtime.active_run = None;
    }

    let is_editor_preview =
        !playback.play_test_mode && playback.editor_preview_anim_id.is_some();
    let preview_scope: Option<HashSet<Uuid>> = if is_editor_preview {
        playback.editor_preview_anim_id.map(|pid| {
            let mut set: HashSet<Uuid> = collect_group_descendant_ids(pid, &state.anims)
                .into_iter()
                .collect();
            set.insert(pid);
            set
        })
    } else {
        None
    };

    // Clean status for removed camera animations
    let valid_cam_ids: HashSet<Uuid> = state
        .anims
        .iter()
        .filter(|a| is_camera_anim(a))
        .map(|a| a.id)
        .collect();
    state.camera_status.retain(|id, _| valid_cam_ids.contains(id));

    // Handle manual user interruption (drag/pan/orbit/zoom/WASD/QE/R)
    if tcam.user_interrupted_this_frame {
        tcam.user_interrupted_this_frame = false;
        if let Some(active) = runtime.active_run.take() {
            if let Some(tr) = playback.tracks.get_mut(&active.anim_id) {
                tr.status = AnimRunStatus::Stopped;
            }
            state
                .camera_status
                .insert(active.anim_id, "Dihentikan (kontrol manual)".into());
            // Keep Layar Besar synchronized if operator interrupts during Tactical Live
            if playback.play_test_mode
                && state.is_tactical_live()
                && !state.offline_preview
                && state.can_control_active_big_screen()
                && let Some(sk) = &state.active_skenario
            {
                ws.send(WsOutCommand::SendCommand(TacticalCommand {
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    marker_id: None,
                    command: "stop_anim".to_string(),
                    payload: serde_json::json!({ "anim_id": active.anim_id.to_string() }),
                    save_history: false,
                }));
            }
            playback.playing = playback
                .tracks
                .values()
                .any(|t| t.status == AnimRunStatus::Playing);
            return;
        }
    }

    if !playback.play_test_mode && !is_editor_preview {
        runtime.active_run = None;
        return;
    }

    // Collect active camera animations (Playing or Paused) and enforce Single Active Camera
    let mut active_candidates: Vec<(Uuid, AnimRunStatus, u64)> = Vec::new();
    for anim in &state.anims {
        if !is_camera_anim(anim) {
            continue;
        }
        if let Some(scope) = &preview_scope
            && !scope.contains(&anim.id)
        {
            continue;
        }
        if let Some(tr) = playback.tracks.get(&anim.id)
            && matches!(tr.status, AnimRunStatus::Playing | AnimRunStatus::Paused)
        {
            active_candidates.push((anim.id, tr.status, tr.trigger_seq));
        }
    }

    if active_candidates.is_empty() {
        // Leave camera right at its last position when stopped!
        runtime.active_run = None;
        return;
    }

    // Prefer the most recently triggered Playing camera anim (or Paused if none are Playing)
    active_candidates.sort_by_key(|(_, st, seq)| (if *st == AnimRunStatus::Playing { 1u8 } else { 0u8 }, *seq));
    let (winner_id, winner_status, winner_seq) = *active_candidates.last().unwrap();

    // Stop any superseded camera animations cleanly (Auto-Handoff!)
    for (cid, _, _) in &active_candidates {
        if *cid != winner_id {
            if let Some(tr) = playback.tracks.get_mut(cid) {
                tr.status = AnimRunStatus::Stopped;
            }
            state
                .camera_status
                .insert(*cid, "Digantikan kamera lain".into());
        }
    }

    let Some(anim) = state.anims.iter().find(|a| a.id == winner_id).cloned() else {
        runtime.active_run = None;
        return;
    };

    // Start new run when switching camera anim or retriggering the same anim
    let needs_new_run = runtime
        .active_run
        .as_ref()
        .is_none_or(|r| r.anim_id != winner_id || r.trigger_seq != winner_seq);
    if needs_new_run {
        runtime.active_run = Some(ActiveCameraRun {
            anim_id: winner_id,
            trigger_seq: winner_seq,
            delay_elapsed: 0.0,
            motion_elapsed: 0.0,
            start_pose: None,
            follow_current: None,
            saw_target_route_active: false,
        });
    }

    let Some(run) = runtime.active_run.as_mut() else {
        return;
    };

    if winner_status == AnimRunStatus::Paused {
        state.camera_status.insert(winner_id, "Dijeda".into());
        return;
    }

    let dt = (time.delta_secs().min(0.1) as f64) * (playback.speed_multiplier.max(0.1) as f64);
    let mut finished = false;

    if is_camera_static_anim(&anim) {
        let cfg = parse_camera_static_config(&anim);
        if run.delay_elapsed < cfg.delay_sec {
            run.delay_elapsed = (run.delay_elapsed + dt).min(cfg.delay_sec);
            if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                tr.distance_m = run.delay_elapsed;
                tr.total_length_m = cfg.total_finite_duration_sec().unwrap_or(0.0);
            }
            state.camera_status.insert(
                winner_id,
                format!("Jeda awal: {:.1} dtk", cfg.delay_sec - run.delay_elapsed),
            );
            return;
        }

        let start = *run
            .start_pose
            .get_or_insert_with(|| CameraPoseSnapshot::from_tcam(&tcam));

        let dst_local = state.geo_anchor.lng_lat_alt_to_local(
            cfg.lng,
            cfg.lat,
            cfg.target_altitude_m,
        );
        let dst_xz = Vec2::new(dst_local[0], dst_local[2]);
        let dst_alt = dst_local[1];
        let dst_dist = cfg.distance_m as f32;
        let dst_pitch = cfg.pitch_deg as f32;
        let dst_bearing = cfg.bearing_deg;

        run.motion_elapsed += dt;

        let pose = if cfg.fly_duration_sec > 1e-4 && run.motion_elapsed < cfg.fly_duration_sec {
            let t = (run.motion_elapsed / cfg.fly_duration_sec).clamp(0.0, 1.0);
            let e = cfg.easing.eval(t) as f32;
            let arc = (cfg.fly_arc_m as f32) * (std::f32::consts::PI * t as f32).sin();
            state.camera_status.insert(
                winner_id,
                format!(
                    "Fly-To · {:.1} / {:.1} dtk",
                    run.motion_elapsed, cfg.fly_duration_sec
                ),
            );
            CameraPoseSnapshot {
                target_xz: start.target_xz.lerp(dst_xz, e),
                alt_m: start.alt_m + (dst_alt - start.alt_m) * e,
                distance_m: start.distance_m + (dst_dist - start.distance_m) * e + arc,
                pitch_deg: start.pitch_deg + (dst_pitch - start.pitch_deg) * e,
                bearing_deg: lerp_angle_deg(start.bearing_deg as f64, dst_bearing, e as f64) as f32,
            }
        } else {
            let hold_elapsed = (run.motion_elapsed - cfg.fly_duration_sec).max(0.0);
            let bearing_now = if cfg.orbit_enabled {
                (dst_bearing + cfg.orbit_speed_deg_per_sec * hold_elapsed).rem_euclid(360.0)
            } else {
                dst_bearing
            };

            match cfg.hold_mode {
                CameraStaticHoldMode::HoldDuration => {
                    if hold_elapsed >= cfg.hold_duration_sec {
                        finished = true;
                        state.camera_status.insert(winner_id, "Selesai".into());
                    } else if cfg.orbit_enabled {
                        state.camera_status.insert(
                            winner_id,
                            format!(
                                "Orbit · {:.1} / {:.1} dtk",
                                hold_elapsed, cfg.hold_duration_sec
                            ),
                        );
                    } else {
                        state.camera_status.insert(
                            winner_id,
                            format!(
                                "Tahan · {:.1} / {:.1} dtk",
                                hold_elapsed, cfg.hold_duration_sec
                            ),
                        );
                    }
                }
                CameraStaticHoldMode::Continuous => {
                    state.camera_status.insert(
                        winner_id,
                        if cfg.orbit_enabled {
                            format!("Orbit aktif · {:.1} dtk", hold_elapsed)
                        } else {
                            format!("Terkunci di posisi · {:.1} dtk", hold_elapsed)
                        },
                    );
                }
            }

            CameraPoseSnapshot {
                target_xz: dst_xz,
                alt_m: dst_alt,
                distance_m: dst_dist,
                pitch_deg: dst_pitch,
                bearing_deg: bearing_now as f32,
            }
        };

        pose.apply_to_tcam(&mut tcam, &mut cam_tf);

        if let Some(tr) = playback.tracks.get_mut(&winner_id) {
            tr.distance_m = cfg.delay_sec + run.motion_elapsed;
            tr.total_length_m = cfg.total_finite_duration_sec().unwrap_or(0.0);
        }
    } else if is_camera_follow_anim(&anim) {
        let cfg = parse_camera_follow_config(&anim);
        let Some(target_mid) = cfg.target_marker_id else {
            state
                .camera_status
                .insert(winner_id, "Belum memilih objek target".into());
            if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                tr.status = AnimRunStatus::Stopped;
            }
            runtime.active_run = None;
            return;
        };
        let Some(marker) = state.markers.iter().find(|m| m.id == target_mid).cloned() else {
            state
                .camera_status
                .insert(winner_id, "Objek target tidak ditemukan".into());
            if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                tr.status = AnimRunStatus::Stopped;
            }
            runtime.active_run = None;
            return;
        };

        if run.delay_elapsed < cfg.delay_sec {
            run.delay_elapsed = (run.delay_elapsed + dt).min(cfg.delay_sec);
            if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                tr.distance_m = run.delay_elapsed;
            }
            state.camera_status.insert(
                winner_id,
                format!("Jeda awal: {:.1} dtk", cfg.delay_sec - run.delay_elapsed),
            );
            return;
        }

        let (m_lng, m_lat, m_alt, m_rot_y) = play_rts.marker_runtime_pose(&marker, &overrides);
        let local = state.geo_anchor.lng_lat_alt_to_local(
            m_lng,
            m_lat,
            m_alt + cfg.height_offset_m,
        );
        let desired = CameraPoseSnapshot {
            target_xz: Vec2::new(local[0], local[2]),
            alt_m: local[1],
            distance_m: cfg.distance_m as f32,
            pitch_deg: cfg.pitch_deg as f32,
            bearing_deg: cfg.desired_bearing_deg(m_rot_y) as f32,
        };

        let start = *run
            .start_pose
            .get_or_insert_with(|| CameraPoseSnapshot::from_tcam(&tcam));

        run.motion_elapsed += dt;

        let pose = if cfg.fly_in_sec > 1e-4 && run.motion_elapsed < cfg.fly_in_sec {
            let t = (run.motion_elapsed / cfg.fly_in_sec).clamp(0.0, 1.0);
            let e = (t * t * (3.0 - 2.0 * t)) as f32;
            let p = CameraPoseSnapshot {
                target_xz: start.target_xz.lerp(desired.target_xz, e),
                alt_m: start.alt_m + (desired.alt_m - start.alt_m) * e,
                distance_m: start.distance_m + (desired.distance_m - start.distance_m) * e,
                pitch_deg: start.pitch_deg + (desired.pitch_deg - start.pitch_deg) * e,
                bearing_deg: lerp_angle_deg(
                    start.bearing_deg as f64,
                    desired.bearing_deg as f64,
                    e as f64,
                ) as f32,
            };
            run.follow_current = Some(p);
            p
        } else {
            let prev = run.follow_current.unwrap_or(desired);
            let alpha_pos = (1.0 - (-cfg.position_smoothness * dt).exp()).clamp(0.0, 1.0) as f32;
            let alpha_rot = (1.0 - (-cfg.rotation_smoothness * dt).exp()).clamp(0.0, 1.0);
            let d_bearing =
                shortest_angle_delta_deg(prev.bearing_deg as f64, desired.bearing_deg as f64);
            let next_pose = CameraPoseSnapshot {
                target_xz: prev.target_xz.lerp(desired.target_xz, alpha_pos),
                alt_m: prev.alt_m + (desired.alt_m - prev.alt_m) * alpha_pos,
                distance_m: prev.distance_m + (desired.distance_m - prev.distance_m) * alpha_pos,
                pitch_deg: prev.pitch_deg + (desired.pitch_deg - prev.pitch_deg) * alpha_pos,
                bearing_deg: (prev.bearing_deg as f64 + d_bearing * alpha_rot).rem_euclid(360.0)
                    as f32,
            };
            run.follow_current = Some(next_pose);
            next_pose
        };

        pose.apply_to_tcam(&mut tcam, &mut cam_tf);

        // Evaluate stop condition
        match cfg.stop_mode {
            CameraFollowStopMode::FixedDuration => {
                if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                    tr.distance_m = cfg.delay_sec + run.motion_elapsed;
                    tr.total_length_m = cfg.delay_sec + cfg.duration_sec;
                }
                if run.motion_elapsed >= cfg.duration_sec {
                    finished = true;
                    state.camera_status.insert(winner_id, "Selesai".into());
                } else {
                    state.camera_status.insert(
                        winner_id,
                        format!(
                            "Mengikuti {} ({}) · {:.1} / {:.1} dtk",
                            marker.name,
                            cfg.preset.short_label(),
                            run.motion_elapsed,
                            cfg.duration_sec
                        ),
                    );
                }
            }
            CameraFollowStopMode::UntilTargetRouteFinishes => {
                let (any_playing, any_finished) =
                    target_route_state(target_mid, &state.anims, &playback);
                if any_playing {
                    run.saw_target_route_active = true;
                }
                if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                    tr.distance_m = cfg.delay_sec + run.motion_elapsed;
                    tr.total_length_m = 0.0;
                }
                if (run.saw_target_route_active && !any_playing)
                    || (!any_playing && any_finished && run.motion_elapsed > 0.25)
                {
                    finished = true;
                    state
                        .camera_status
                        .insert(winner_id, "Selesai (rute target selesai)".into());
                } else {
                    state.camera_status.insert(
                        winner_id,
                        format!(
                            "Mengikuti {} ({}) · {:.1} dtk",
                            marker.name,
                            cfg.preset.short_label(),
                            run.motion_elapsed
                        ),
                    );
                }
            }
            CameraFollowStopMode::UntilStopped => {
                if let Some(tr) = playback.tracks.get_mut(&winner_id) {
                    tr.distance_m = cfg.delay_sec + run.motion_elapsed;
                    tr.total_length_m = 0.0;
                }
                state.camera_status.insert(
                    winner_id,
                    format!(
                        "Mengikuti {} ({}) · {:.1} dtk",
                        marker.name,
                        cfg.preset.short_label(),
                        run.motion_elapsed
                    ),
                );
            }
        }
    }

    if finished {
        if let Some(tr) = playback.tracks.get_mut(&winner_id) {
            tr.status = AnimRunStatus::Finished;
            tr.completed_cycles = tr.completed_cycles.saturating_add(1);
        }
        runtime.active_run = None;
        playback.playing = playback
            .tracks
            .values()
            .any(|t| t.status == AnimRunStatus::Playing);
    }
}
