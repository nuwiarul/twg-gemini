use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use twg_shared::{
    geo::haversine_distance_m,
    group_anim::{
        GroupAnimMode, SequenceTriggerMode, anim_is_continuous_unbounded,
        collect_group_descendant_ids, is_group_anim, parse_group_anim_config,
        would_create_group_cycle,
    },
    models::Anim,
    particle_anim::{is_particle_anim, parse_particle_anim_config},
    pointer_anim::{is_pointer_arrow_anim, parse_pointer_arrow_config},
    route_formation::{
        RouteMoveAnimConfig, anim_bound_marker_ids, is_multi_route_anim, parse_multi_route_config,
        parse_route_move_anim_config, sample_multi_route_poses,
    },
    shoot_anim::{is_shoot_anim, shoot_anim_effective_duration_sec, shoot_anim_is_continuous},
    spawn_hide_anim::{SpawnHideAction, is_spawn_hide_anim, parse_spawn_hide_config},
    traffic_anim::{is_traffic_flow_or_signal, parse_traffic_anim_config},
    traffic_diversion::{is_traffic_diversion_anim, parse_traffic_diversion_config},
    traffic_queue::{is_traffic_queue_anim, parse_traffic_queue_config},
};
use uuid::Uuid;

use crate::state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, ToolMode};

#[derive(Debug, Clone)]
pub struct MarkerPlaybackPose {
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
    pub rot_y: f64,
    pub moving: bool,
    pub move_anim: RouteMoveAnimConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisibilityOverride {
    pub visible: bool,
    pub scale_factor: f32,
    pub altitude_offset_m: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct SpawnHideFxRing {
    pub marker_id: Uuid,
    pub action: SpawnHideAction,
    pub progress: f32,
}

#[derive(Resource, Default)]
pub struct PlaybackOverrides {
    pub markers: HashMap<Uuid, MarkerPlaybackPose>,
    /// Runtime visibility & transition scale/altitude overrides driven by `Spawn & Hide` animations.
    pub visibility: HashMap<Uuid, MarkerVisibilityOverride>,
    /// Active expanding ground-ring pulses for `Spawn & Hide` transitions (Play mode & Editor preview).
    pub spawn_hide_fx_rings: Vec<SpawnHideFxRing>,
}

impl PlaybackOverrides {
    /// Returns whether `marker` is effectively visible right now, factoring in `Spawn & Hide`
    /// runtime overrides during Play Skenario (or active Editor transition preview).
    pub fn is_marker_visible(
        &self,
        marker: &twg_shared::models::Marker,
        play_test_mode: bool,
    ) -> bool {
        if let Some(ov) = self.visibility.get(&marker.id) {
            return ov.visible;
        }
        let _ = play_test_mode;
        marker.is_spawn
    }
}

/// Computes total horizontal metric length of a 3D route polyline (`[lng, lat, alt]`).
pub fn route_total_length_m(geom: &[[f64; 3]]) -> f64 {
    if geom.len() < 2 {
        return 0.0;
    }
    geom.windows(2)
        .map(|w| haversine_distance_m(w[0][0], w[0][1], w[1][0], w[1][1]).max(0.01))
        .sum()
}

/// Computes estimated travel duration (in seconds) given a route polyline and speed (m/s).
pub fn route_estimated_duration_sec(geom: &[[f64; 3]], speed_mps: f64) -> f64 {
    let len = route_total_length_m(geom);
    if len <= 0.0 {
        return 1.0;
    }
    (len / speed_mps.max(0.1)).max(0.5)
}

/// Drives per-animation route movement, Spawn & Hide visibility/transitions, Shoot, Particle,
/// and Pointer Arrow playback in Play Skenario mode (plus one-shot Spawn/Hide Editor preview).
pub fn playback_system(
    time: Res<Time>,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    mut overrides: ResMut<PlaybackOverrides>,
) {
    overrides.markers.clear();
    overrides.visibility.clear();
    overrides.spawn_hide_fx_rings.clear();
    if !state.is_3d_map_screen() {
        playback.playing = false;
        playback.stop_editor_preview();
        state.spawn_hide_preview = None;
        return;
    }

    let dt = time.delta_secs().min(0.1) as f64;

    // Validate any active Plot-Editor Route/Group preview session: auto-stop if deselected,
    // deleted, or if user switches to a route/marker editing tool so edits happen on rest poses.
    if !playback.play_test_mode
        && let Some(preview_id) = playback.editor_preview_anim_id
    {
        let still_selected = state.selected == SelectedEntity::Anim(preview_id);
        let still_exists = state.anims.iter().any(|a| a.id == preview_id);
        let editing_tool = matches!(
            state.tool_mode,
            ToolMode::DrawRoute
                | ToolMode::DrawTrafficLane
                | ToolMode::DrawTrafficQueueLane
                | ToolMode::DrawTrafficDiversionLane
                | ToolMode::PickRouteMarker
                | ToolMode::PickShootMarker
                | ToolMode::PickPointerMarker
                | ToolMode::PickSpawnHideMarker
                | ToolMode::PickCameraFollowMarker
                | ToolMode::PickModelActionMarker
                | ToolMode::PickDialogSpeaker
                | ToolMode::PlaceParticle
                | ToolMode::DrawWaterPolygon
                | ToolMode::PlaceRainZone
        );
        if !still_selected || !still_exists || editing_tool {
            playback.stop_editor_preview();
        }
    }

    let is_editor_preview_active =
        !playback.play_test_mode && playback.editor_preview_anim_id.is_some();

    if !playback.play_test_mode && !is_editor_preview_active {
        // Optional one-shot Editor transition preview for a selected Spawn & Hide animation
        if let Some((preview_id, elapsed_f32)) = state.spawn_hide_preview {
            let next_elapsed = elapsed_f32 as f64 + dt;
            if let Some(anim) = state
                .anims
                .iter()
                .find(|a| a.id == preview_id && is_spawn_hide_anim(a))
            {
                let cfg = parse_spawn_hide_config(anim);
                let total_dur = cfg.total_duration_sec();
                if cfg.marker_ids.is_empty() || next_elapsed > total_dur + 0.65 {
                    state.spawn_hide_preview = None;
                } else {
                    state.spawn_hide_preview = Some((preview_id, next_elapsed as f32));
                    let eval_t = next_elapsed.min(total_dur);
                    for (idx, mid) in cfg.marker_ids.iter().enumerate() {
                        let sample = cfg.evaluate_target(idx, eval_t);
                        overrides.visibility.insert(
                            *mid,
                            MarkerVisibilityOverride {
                                visible: sample.visible,
                                scale_factor: sample.scale_factor,
                                altitude_offset_m: sample.altitude_offset_m,
                            },
                        );
                        if let Some(p) = sample.active_fx_progress {
                            overrides.spawn_hide_fx_rings.push(SpawnHideFxRing {
                                marker_id: *mid,
                                action: cfg.action,
                                progress: p,
                            });
                        }
                    }
                }
            } else {
                state.spawn_hide_preview = None;
            }
        }
        return;
    }

    state.spawn_hide_preview = None;

    // Remove stale tracks for deleted animations
    playback
        .tracks
        .retain(|aid, _| state.anims.iter().any(|a| a.id == *aid));

    let speed_mult = playback.speed_multiplier.max(0.1) as f64;
    let step_dt = dt * speed_mult;

    // Phase A: Advance active Combine / Sequence groups & launch due children before ticking leaves
    advance_group_animations(&state.anims, &mut playback, step_dt);

    // Phase B: Tick all leaf animations (Spawn/Hide, Particle, Pointer, Shoot, Route)
    let mut dialog_status_updates: Vec<(Uuid, Option<String>)> = Vec::new();
    for anim in &state.anims {
        // Audio, Billboard video, and Camera animations manage their own playback loops.
        if twg_shared::audio_anim::is_audio_anim(anim)
            || twg_shared::billboard_anim::is_billboard_anim(anim)
            || twg_shared::camera_anim::is_camera_anim(anim)
        {
            continue;
        }
        if is_group_anim(anim) {
            continue;
        }

        // Water (Ocean/River/Flood) & Rain simulations track elapsed seconds in `track.distance_m`
        if twg_shared::environment_anim::is_water_anim(anim) {
            let cfg = twg_shared::environment_anim::parse_water_anim_config(anim);
            let dur_sec = cfg.delay_sec + cfg.duration_sec;
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = if cfg.continuous { 0.0 } else { dur_sec };
            if track.status == AnimRunStatus::Playing {
                if anim.geom.len() < 3 {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                    if !cfg.continuous && track.distance_m >= dur_sec {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= dur_sec.max(0.1);
                        } else {
                            track.distance_m = dur_sec;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                }
            }
            continue;
        }
        if twg_shared::environment_anim::is_rain_anim(anim) {
            let cfg = twg_shared::environment_anim::parse_rain_anim_config(anim);
            let dur_sec = cfg.delay_sec + cfg.duration_sec;
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = if cfg.continuous { 0.0 } else { dur_sec };
            if track.status == AnimRunStatus::Playing {
                if !cfg.has_playable_setup(anim) {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                    if !cfg.continuous && track.distance_m >= dur_sec {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= dur_sec.max(0.1);
                        } else {
                            track.distance_m = dur_sec;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                }
            }
            continue;
        }

        // Spawn & Hide animations track transition elapsed time in `track.distance_m`
        if is_spawn_hide_anim(anim) {
            let cfg = parse_spawn_hide_config(anim);
            let total_dur = cfg.total_duration_sec();
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = total_dur;

            if track.status == AnimRunStatus::Playing {
                if cfg.marker_ids.is_empty() {
                    track.status = AnimRunStatus::Stopped;
                    track.trigger_seq = 0;
                } else {
                    track.distance_m += step_dt;
                    if total_dur <= 1e-4 || track.distance_m >= total_dur {
                        track.distance_m = total_dur;
                        track.status = AnimRunStatus::Finished;
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                    }
                }
            }
            continue;
        }

        // Traffic simulation & Traffic Signal animations track elapsed seconds in `track.distance_m`
        if is_traffic_flow_or_signal(anim) {
            let cfg = parse_traffic_anim_config(anim);
            let dur_sec = anim.duration_sec.max(10.0);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = if cfg.continuous { 0.0 } else { dur_sec };

            if track.status == AnimRunStatus::Playing {
                if !cfg.has_playable_setup() {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                    if !cfg.continuous && track.distance_m >= dur_sec {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= dur_sec;
                        } else {
                            track.distance_m = dur_sec;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                }
            }
            continue;
        }

        // Antrean & Urai Simpang (`traffic_queue`) tracks elapsed seconds continuously until
        // live "Lancar" drains all remaining vehicles to zero (marked Finished by `traffic_visual_system`).
        if is_traffic_queue_anim(anim) {
            let cfg = parse_traffic_queue_config(anim);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = 0.0;

            if track.status == AnimRunStatus::Playing {
                if !cfg.has_playable_setup() {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                }
            }
            continue;
        }

        // Pengalihan Arus (`traffic_diversion`) tracks elapsed seconds continuously
        if is_traffic_diversion_anim(anim) {
            let cfg = parse_traffic_diversion_config(anim);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = 0.0;

            if track.status == AnimRunStatus::Playing {
                if !cfg.has_playable_setup() {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                }
            }
            continue;
        }

        // Particle animations (Fire / Smoke / Explosion) track elapsed seconds in `track.distance_m`
        if is_particle_anim(anim) {
            let cfg = parse_particle_anim_config(anim);
            let dur_sec = anim.duration_sec.max(0.5);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = if cfg.continuous { 0.0 } else { dur_sec };

            if track.status == AnimRunStatus::Playing {
                if anim.geom.is_empty() {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                    if !cfg.continuous && track.distance_m >= dur_sec {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= dur_sec;
                        } else {
                            track.distance_m = dur_sec;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                }
            }
            continue;
        }

        // Custom 3D Model Action Clips (`model_action`) advance elapsed seconds in `track.distance_m`.
        // Exact clip duration, mode transitions (Hold Last Frame / Return Idle), and Route Priority
        // handoff (auto-stop when marker moves) are enforced in `animate_markers_system`.
        if twg_shared::model_action_anim::is_model_action_anim(anim) {
            let cfg = twg_shared::model_action_anim::parse_model_action_config(anim);
            let track = playback.tracks.entry(anim.id).or_default();
            if cfg.play_mode == twg_shared::model_action_anim::ModelActionPlayMode::LoopForever {
                track.total_length_m = 0.0;
            }
            if track.status == AnimRunStatus::Playing {
                if anim.marker_id.is_none() || cfg.action_slug.trim().is_empty() {
                    track.status = AnimRunStatus::Stopped;
                    track.trigger_seq = 0;
                } else {
                    track.distance_m += step_dt;
                }
            }
            continue;
        }

        // 3D Speech Bubble Dialog animations (`dialog`) advance elapsed seconds in `track.distance_m`
        // without overriding marker movement — so speaking units can talk even while walking on a Route!
        if twg_shared::dialog_anim::is_dialog_anim(anim) {
            let cfg = twg_shared::dialog_anim::parse_dialog_anim_config(anim);
            let total_dur = cfg.total_duration_sec(anim.replay);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = total_dur;

            if track.status == AnimRunStatus::Playing {
                if total_dur <= 1e-4 || cfg.playable_lines().is_empty() {
                    track.status = AnimRunStatus::Stopped;
                    track.trigger_seq = 0;
                    dialog_status_updates.push((anim.id, None));
                } else {
                    track.distance_m += step_dt;
                    if track.distance_m >= total_dur {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= total_dur.max(0.1);
                        } else {
                            track.distance_m = total_dur;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                    if let Some(sample) = cfg.evaluate_at(track.distance_m) {
                        let speaker = if !sample.speaker_override.is_empty() {
                            sample.speaker_override
                        } else {
                            state
                                .markers
                                .iter()
                                .find(|m| m.id == sample.marker_id)
                                .map(|m| m.name.clone())
                                .unwrap_or_else(|| "Pembicara".into())
                        };
                        dialog_status_updates.push((
                            anim.id,
                            Some(format!(
                                "Baris {}/{} · {speaker}",
                                sample.line_index + 1,
                                sample.total_lines
                            )),
                        ));
                    } else if track.status == AnimRunStatus::Finished {
                        dialog_status_updates.push((anim.id, Some("Dialog selesai".into())));
                    } else if track.distance_m < cfg.delay_sec {
                        dialog_status_updates.push((anim.id, Some("Jeda awal...".into())));
                    } else {
                        dialog_status_updates.push((anim.id, Some("Jeda antar dialog...".into())));
                    }
                }
            } else if track.status == AnimRunStatus::Stopped {
                dialog_status_updates.push((anim.id, None));
            }
            continue;
        }

        // Pointer Arrow highlight animations advance elapsed time in `track.distance_m` without
        // overriding marker position — so a highlighted unit can simultaneously follow a Route!
        if is_pointer_arrow_anim(anim) {
            let cfg = parse_pointer_arrow_config(anim);
            let dur_sec = anim.duration_sec.max(0.5);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = if cfg.continuous { 0.0 } else { dur_sec };

            if track.status == AnimRunStatus::Playing {
                if anim.marker_id.is_none() {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                    if !cfg.continuous && track.distance_m >= dur_sec {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= dur_sec;
                        } else {
                            track.distance_m = dur_sec;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                }
            }
            continue;
        }

        // Shoot animations (Water Cannon, Stone Throw, Tear Gas, Bullet, APAR) advance elapsed
        // time in `track.distance_m` without overriding marker position — so a firing/spraying
        // unit can simultaneously move along a Route!
        if is_shoot_anim(anim) {
            let continuous = shoot_anim_is_continuous(anim);
            let dur_sec = shoot_anim_effective_duration_sec(anim);
            let track = playback.tracks.entry(anim.id).or_default();
            track.total_length_m = if continuous { 0.0 } else { dur_sec };

            if track.status == AnimRunStatus::Playing {
                if anim.marker_id.is_none() {
                    track.status = AnimRunStatus::Stopped;
                } else {
                    track.distance_m += step_dt;
                    if !continuous && track.distance_m >= dur_sec {
                        track.completed_cycles = track.completed_cycles.saturating_add(1);
                        if anim.replay {
                            track.distance_m %= dur_sec;
                        } else {
                            track.distance_m = dur_sec;
                            track.status = AnimRunStatus::Finished;
                        }
                    }
                }
            }
            continue;
        }

        let total_len = route_total_length_m(&anim.geom);
        let track = playback.tracks.entry(anim.id).or_default();
        track.total_length_m = total_len;
        let has_targets = !anim_bound_marker_ids(anim).is_empty();

        if track.status == AnimRunStatus::Playing {
            if total_len <= 0.01 || !has_targets {
                track.status = AnimRunStatus::Stopped;
            } else {
                let step_m = step_dt * anim.speed.max(0.1);
                track.distance_m += step_m;
                if track.distance_m >= total_len {
                    track.completed_cycles = track.completed_cycles.saturating_add(1);
                    if anim.replay {
                        track.distance_m %= total_len;
                    } else {
                        track.distance_m = total_len;
                        track.status = AnimRunStatus::Finished;
                    }
                }
                track.has_pose_override = true;
            }
        }

        if track.has_pose_override && has_targets {
            let is_moving = track.status == AnimRunStatus::Playing;
            let dist = track.distance_m.clamp(0.0, total_len);
            let move_anim = parse_route_move_anim_config(anim);
            if is_multi_route_anim(anim) {
                let cfg = parse_multi_route_config(anim);
                let poses = sample_multi_route_poses(&anim.geom, &cfg, dist);
                for p in poses {
                    let pose = MarkerPlaybackPose {
                        lng: p.lng,
                        lat: p.lat,
                        altitude: p.altitude,
                        rot_y: p.rot_y,
                        moving: is_moving,
                        move_anim: move_anim.effective_for_marker(p.marker_id),
                    };
                    let replace = overrides
                        .markers
                        .get(&p.marker_id)
                        .is_none_or(|prev| !prev.moving || is_moving);
                    if replace {
                        overrides.markers.insert(p.marker_id, pose);
                    }
                }
            } else if let Some(marker_id) = anim.marker_id
                && let Some(pose) = sample_route_at_distance(anim, dist, is_moving)
            {
                // If multiple routes reference the same marker, an actively moving route wins
                let replace = overrides
                    .markers
                    .get(&marker_id)
                    .is_none_or(|prev| !prev.moving || is_moving);
                if replace {
                    overrides.markers.insert(marker_id, pose);
                }
            }
        }
    }
    for (aid, status_opt) in dialog_status_updates {
        match status_opt {
            Some(msg) => {
                state.dialog_status.insert(aid, msg);
            }
            None => {
                state.dialog_status.remove(&aid);
            }
        }
    }

    // Phase C: Post-leaf Group handoff & completion detection (zero extra dt)
    advance_group_animations(&state.anims, &mut playback, 0.0);

    let preview_scope: Option<HashSet<Uuid>> = if is_editor_preview_active {
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

    let overrides_mut = &mut *overrides;
    resolve_spawn_hide_overrides_scoped(
        &state.anims,
        &state.hidden_entity_ids,
        &playback,
        preview_scope.as_ref(),
        &mut overrides_mut.visibility,
        &mut overrides_mut.spawn_hide_fx_rings,
    );

    playback.playing = playback
        .tracks
        .values()
        .any(|t| t.status == AnimRunStatus::Playing);
}

/// Evaluates whether child animation `child_id` has finished at least 1 cycle (or exceeded
/// `continuous_wait_sec` if it is unbounded continuous) so a `Sequence` / `Combine` can hand off.
fn is_group_child_done_for_handoff(
    child_id: Uuid,
    anims: &[Anim],
    playback: &PlaybackState,
    group_elapsed_sec: f64,
    step_start_sec: f64,
    continuous_wait_sec: f64,
) -> bool {
    let Some(child_anim) = anims.iter().find(|a| a.id == child_id) else {
        return true;
    };
    if twg_shared::audio_anim::is_audio_anim(child_anim)
        || twg_shared::billboard_anim::is_billboard_anim(child_anim)
    {
        return playback.tracks.get(&child_id).is_none_or(|t|
            matches!(t.status, AnimRunStatus::Finished | AnimRunStatus::Stopped));
    }
    if anim_is_continuous_unbounded(child_anim) {
        return (group_elapsed_sec - step_start_sec) >= continuous_wait_sec.max(0.0);
    }
    match playback.tracks.get(&child_id) {
        Some(t) => {
            t.completed_cycles >= 1
                || matches!(t.status, AnimRunStatus::Finished | AnimRunStatus::Stopped)
        }
        None => true,
    }
}

/// Advances all active `Grup` (`Combine` & `Sequence`) animations by `dt_scaled` seconds:
/// - `Combine`: launches all valid non-cyclic child animations simultaneously once `initial_delay_sec`
///   elapses, and completes once all finite/repeating children finish at least 1 cycle (or stays
///   `Playing` continuously if every child is unbounded continuous).
/// - `Sequence`: launches children strictly in order using either `WaitComplete` (waiting for 1 cycle
///   completion — so `replay` children continue looping after handoff!) or `WaitDelay` (waiting `X`s
///   after the previous step started), with per-step overrides and safe `continuous_wait_sec` handoff.
pub fn advance_group_animations(anims: &[Anim], playback: &mut PlaybackState, dt_scaled: f64) {
    let group_ids: Vec<Uuid> = anims
        .iter()
        .filter(|a| is_group_anim(a))
        .map(|a| a.id)
        .collect();
    if group_ids.is_empty() {
        return;
    }

    // Advance group elapsed clocks once (only when `dt_scaled > 0`)
    if dt_scaled > 0.0 {
        for &gid in &group_ids {
            if let Some(track) = playback.tracks.get_mut(&gid)
                && track.status == AnimRunStatus::Playing
            {
                track.distance_m += dt_scaled;
            }
        }
    }

    // Up to 4 propagation passes so nested groups (e.g. Sequence -> Combine -> Leaf) launch on the same frame
    for _pass in 0..4 {
        let mut any_launched_this_pass = false;

        for &gid in &group_ids {
            let Some(anim) = anims.iter().find(|a| a.id == gid) else {
                continue;
            };
            let cfg = parse_group_anim_config(anim);
            let total_steps = cfg.items.len();

            {
                let track = playback.tracks.entry(gid).or_default();
                track.total_length_m = total_steps as f64;
                if track.status != AnimRunStatus::Playing {
                    continue;
                }
                if total_steps == 0 {
                    track.status = AnimRunStatus::Stopped;
                    continue;
                }
            }

            let (elapsed, launched_count, last_start_sec, prev_done_at) = {
                let t = &playback.tracks[&gid];
                (
                    t.distance_m,
                    t.group_launched_count,
                    t.group_last_step_start_sec,
                    t.group_prev_done_at_sec,
                )
            };

            match cfg.mode {
                GroupAnimMode::Combine => {
                    if launched_count == 0 {
                        if elapsed >= cfg.initial_delay_sec.max(0.0) {
                            let mut launched_children = Vec::new();
                            for item in &cfg.items {
                                if !would_create_group_cycle(gid, item.anim_id, anims)
                                    && anims.iter().any(|a| a.id == item.anim_id)
                                {
                                    playback.play_anim(item.anim_id);
                                    launched_children.push(item.anim_id);
                                }
                            }
                            if let Some(t) = playback.tracks.get_mut(&gid) {
                                t.group_launched_count = total_steps;
                                t.group_last_step_start_sec = elapsed;
                                t.group_owned_children = launched_children;
                            }
                            any_launched_this_pass = true;
                        }
                    } else {
                        let owned: Vec<Uuid> = playback
                            .tracks
                            .get(&gid)
                            .map(|t| t.group_owned_children.clone())
                            .unwrap_or_default();
                        let finite_children: Vec<Uuid> = owned
                            .iter()
                            .copied()
                            .filter(|cid| {
                                anims
                                    .iter()
                                    .find(|a| a.id == *cid)
                                    .is_some_and(|a| !anim_is_continuous_unbounded(a)
                                        || twg_shared::audio_anim::is_audio_anim(a))
                            })
                            .collect();

                        if finite_children.is_empty() {
                            // All children are unbounded continuous -> stay Playing until Stopped,
                            // while marking 1 cycle after `continuous_wait_sec` for any parent Sequence.
                            if (elapsed - last_start_sec) >= cfg.continuous_wait_sec.max(0.0)
                                && let Some(t) = playback.tracks.get_mut(&gid)
                                && t.completed_cycles == 0
                            {
                                t.completed_cycles = 1;
                            }
                        } else {
                            let all_done = finite_children.iter().all(|cid| {
                                is_group_child_done_for_handoff(
                                    *cid,
                                    anims,
                                    playback,
                                    elapsed,
                                    last_start_sec,
                                    cfg.continuous_wait_sec,
                                )
                            });
                            if all_done && let Some(t) = playback.tracks.get_mut(&gid) {
                                t.completed_cycles = t.completed_cycles.saturating_add(1);
                                t.status = AnimRunStatus::Finished;
                            }
                        }
                    }
                }
                GroupAnimMode::Sequence => {
                    if launched_count == 0 {
                        if elapsed >= cfg.initial_delay_sec.max(0.0) {
                            let first_id = cfg.items[0].anim_id;
                            let mut launched_first = false;
                            if !would_create_group_cycle(gid, first_id, anims)
                                && anims.iter().any(|a| a.id == first_id)
                            {
                                playback.play_anim(first_id);
                                launched_first = true;
                            }
                            if let Some(t) = playback.tracks.get_mut(&gid) {
                                t.group_launched_count = 1;
                                t.group_last_step_start_sec = elapsed;
                                t.group_prev_done_at_sec = None;
                                if launched_first && !t.group_owned_children.contains(&first_id) {
                                    t.group_owned_children.push(first_id);
                                }
                            }
                            any_launched_this_pass = true;
                        }
                    } else if launched_count < total_steps {
                        let prev_id = cfg.items[launched_count - 1].anim_id;
                        let (trig_mode, trig_sec) = cfg.effective_step_trigger(launched_count);
                        let ready_to_launch = match trig_mode {
                            SequenceTriggerMode::WaitDelay => {
                                (elapsed - last_start_sec) + 1e-6 >= trig_sec
                            }
                            SequenceTriggerMode::WaitComplete => {
                                let prev_done = is_group_child_done_for_handoff(
                                    prev_id,
                                    anims,
                                    playback,
                                    elapsed,
                                    last_start_sec,
                                    cfg.continuous_wait_sec,
                                );
                                if !prev_done {
                                    false
                                } else {
                                    let done_at = prev_done_at.unwrap_or(elapsed);
                                    if prev_done_at.is_none()
                                        && let Some(t) = playback.tracks.get_mut(&gid)
                                    {
                                        t.group_prev_done_at_sec = Some(done_at);
                                    }
                                    (elapsed - done_at) + 1e-6 >= trig_sec
                                }
                            }
                        };

                        if ready_to_launch {
                            let next_id = cfg.items[launched_count].anim_id;
                            let mut launched_next = false;
                            if !would_create_group_cycle(gid, next_id, anims)
                                && anims.iter().any(|a| a.id == next_id)
                            {
                                playback.play_anim(next_id);
                                launched_next = true;
                            }
                            if let Some(t) = playback.tracks.get_mut(&gid) {
                                t.group_launched_count = launched_count + 1;
                                t.group_last_step_start_sec = elapsed;
                                t.group_prev_done_at_sec = None;
                                if launched_next && !t.group_owned_children.contains(&next_id) {
                                    t.group_owned_children.push(next_id);
                                }
                            }
                            any_launched_this_pass = true;
                        }
                    } else {
                        // All steps launched -> check if final step (and any finite earlier steps) completed
                        let last_id = cfg.items[total_steps - 1].anim_id;
                        let last_is_continuous = anims
                            .iter()
                            .find(|a| a.id == last_id)
                            .is_some_and(|a| anim_is_continuous_unbounded(a)
                                && !twg_shared::audio_anim::is_audio_anim(a));

                        if last_is_continuous && !anim.replay {
                            if (elapsed - last_start_sec) >= cfg.continuous_wait_sec.max(0.0)
                                && let Some(t) = playback.tracks.get_mut(&gid)
                                && t.completed_cycles == 0
                            {
                                t.completed_cycles = 1;
                            }
                        } else {
                            let last_done = is_group_child_done_for_handoff(
                                last_id,
                                anims,
                                playback,
                                elapsed,
                                last_start_sec,
                                cfg.continuous_wait_sec,
                            );
                            if last_done {
                                if anim.replay {
                                    if let Some(t) = playback.tracks.get_mut(&gid) {
                                        t.completed_cycles = t.completed_cycles.saturating_add(1);
                                        t.distance_m = 0.0;
                                        t.group_launched_count = 0;
                                        t.group_last_step_start_sec = 0.0;
                                        t.group_prev_done_at_sec = None;
                                    }
                                    any_launched_this_pass = true;
                                } else if let Some(t) = playback.tracks.get_mut(&gid) {
                                    t.completed_cycles = t.completed_cycles.saturating_add(1);
                                    t.status = AnimRunStatus::Finished;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !any_launched_this_pass {
            break;
        }
    }
}

/// Computes deterministic per-marker visibility, transition scale/altitude, and ground-ring FX
/// pulses for all `Spawn & Hide` animations in Play Skenario:
/// - Pass 1: All markers targeted by any active (non-hidden) `Spawn` animation start hidden
///   at the beginning of Play Skenario.
/// - Pass 2: Triggered (`Playing` / `Paused` / `Finished`) `Spawn` and `Hide` animations are
///   applied in ascending `trigger_seq` order so the most recent user action wins cleanly.
#[cfg(test)]
pub fn resolve_spawn_hide_overrides(
    anims: &[Anim],
    hidden_entity_ids: &std::collections::HashSet<Uuid>,
    playback: &PlaybackState,
    visibility_out: &mut HashMap<Uuid, MarkerVisibilityOverride>,
    fx_rings_out: &mut Vec<SpawnHideFxRing>,
) {
    resolve_spawn_hide_overrides_scoped(
        anims,
        hidden_entity_ids,
        playback,
        None,
        visibility_out,
        fx_rings_out,
    );
}

fn resolve_spawn_hide_overrides_scoped(
    anims: &[Anim],
    hidden_entity_ids: &std::collections::HashSet<Uuid>,
    playback: &PlaybackState,
    scope_ids: Option<&HashSet<Uuid>>,
    visibility_out: &mut HashMap<Uuid, MarkerVisibilityOverride>,
    fx_rings_out: &mut Vec<SpawnHideFxRing>,
) {
    // Pass 1: Baseline hidden state for all Spawn animation targets (scoped when in Editor Preview)
    for anim in anims {
        if hidden_entity_ids.contains(&anim.id) || !is_spawn_hide_anim(anim) {
            continue;
        }
        if let Some(scope) = scope_ids
            && !scope.contains(&anim.id)
        {
            continue;
        }
        let cfg = parse_spawn_hide_config(anim);
        if cfg.action == SpawnHideAction::Spawn {
            for mid in &cfg.marker_ids {
                visibility_out.insert(
                    *mid,
                    MarkerVisibilityOverride {
                        visible: false,
                        scale_factor: 0.0,
                        altitude_offset_m: 0.0,
                    },
                );
            }
        }
    }

    // Pass 2: Apply triggered Spawn/Hide tracks in ascending trigger sequence (last action wins)
    let mut triggered: Vec<(u64, i32, Uuid, &Anim, AnimRunStatus, f64)> = Vec::new();
    for anim in anims {
        if hidden_entity_ids.contains(&anim.id) || !is_spawn_hide_anim(anim) {
            continue;
        }
        if let Some(scope) = scope_ids
            && !scope.contains(&anim.id)
        {
            continue;
        }
        if let Some(track) = playback.tracks.get(&anim.id)
            && !matches!(track.status, AnimRunStatus::Stopped)
        {
            triggered.push((
                track.trigger_seq,
                anim.sort_order,
                anim.id,
                anim,
                track.status,
                track.distance_m,
            ));
        }
    }
    triggered.sort_by_key(|(seq, order, id, _, _, _)| (*seq, *order, *id));

    for (_, _, _, anim, status, elapsed_sec) in triggered {
        let cfg = parse_spawn_hide_config(anim);
        let emit_fx = matches!(status, AnimRunStatus::Playing | AnimRunStatus::Paused);
        for (idx, mid) in cfg.marker_ids.iter().enumerate() {
            let sample = cfg.evaluate_target(idx, elapsed_sec);
            visibility_out.insert(
                *mid,
                MarkerVisibilityOverride {
                    visible: sample.visible,
                    scale_factor: sample.scale_factor,
                    altitude_offset_m: sample.altitude_offset_m,
                },
            );
            if emit_fx
                && let Some(progress) = sample.active_fx_progress
            {
                fx_rings_out.push(SpawnHideFxRing {
                    marker_id: *mid,
                    action: cfg.action,
                    progress,
                });
            }
        }
    }
}

/// Samples position and glTF `+Z`-forward yaw heading along `anim.geom` at exact `distance_m`.
pub fn sample_route_at_distance(
    anim: &Anim,
    distance_m: f64,
    moving: bool,
) -> Option<MarkerPlaybackPose> {
    if anim.geom.len() < 2 {
        return None;
    }

    let mut seg_lengths = Vec::with_capacity(anim.geom.len() - 1);
    let mut total_len = 0.0_f64;
    for w in anim.geom.windows(2) {
        let len = haversine_distance_m(w[0][0], w[0][1], w[1][0], w[1][1]).max(0.01);
        seg_lengths.push(len);
        total_len += len;
    }
    if total_len <= 0.0 {
        return None;
    }

    let target_dist = distance_m.clamp(0.0, total_len);
    let mut acc = 0.0_f64;

    for (idx, seg_len) in seg_lengths.iter().enumerate() {
        if acc + seg_len >= target_dist || idx == seg_lengths.len() - 1 {
            let local_t = ((target_dist - acc) / seg_len).clamp(0.0, 1.0);
            let p0 = anim.geom[idx];
            let p1 = anim.geom[idx + 1];

            let lng = p0[0] + (p1[0] - p0[0]) * local_t;
            let lat = p0[1] + (p1[1] - p0[1]) * local_t;
            let alt = p0[2] + (p1[2] - p0[2]) * local_t;

            // Compute yaw around Bevy +Y (East = +X, South = +Z, North = -Z).
            // Standard glTF 2.0 character/vehicle models face +Z at yaw=0:
            // rotating +Z by yaw gives (+sin(yaw), 0, +cos(yaw)), so yaw = atan2(dx, dz).
            let dx = (p1[0] - p0[0]) * lat.to_radians().cos();
            let dz = -(p1[1] - p0[1]);
            let rot_y = dx.atan2(dz).to_degrees();

            return Some(MarkerPlaybackPose {
                lng,
                lat,
                altitude: alt,
                rot_y,
                moving,
                move_anim: parse_route_move_anim_config(anim),
            });
        }
        acc += seg_len;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn audio_sequence_waits_for_all_repeats_or_explicit_stop() {
        use twg_shared::{audio_anim::*, group_anim::{GroupAnimConfig, GroupAnimMode, GroupStepItem, SequenceTriggerMode, write_group_anim_config}};
        for repeat in [AudioRepeat::Count, AudioRepeat::Forever] {
            let mut sound = sample_anim(vec![], 1.0, false);
            sound.anim_type = ANIM_TYPE_PLAY_SOUND.into();
            sound.marker_id = None;
            write_audio_anim_config(&mut sound, &AudioAnimConfig { asset_id: Some(Uuid::new_v4()), repeat, repeat_count: 3, ..Default::default() });
            let next = sample_anim(vec![], 1.0, false);
            let mut group = sample_anim(vec![], 1.0, false);
            let mut cfg = GroupAnimConfig::default_for(GroupAnimMode::Sequence);
            cfg.sequence_trigger = SequenceTriggerMode::WaitComplete;
            cfg.sequence_gap_sec = 0.0;
            cfg.items = vec![GroupStepItem::new(sound.id), GroupStepItem::new(next.id)];
            write_group_anim_config(&mut group, &cfg);
            let anims = vec![sound.clone(), next.clone(), group.clone()];
            let mut pb = PlaybackState::default();
            pb.play_anim(group.id);
            advance_group_animations(&anims, &mut pb, 0.1);
            assert_eq!(pb.track_status(sound.id), AnimRunStatus::Playing);
            // A completed individual repetition must not hand off; neither may the
            // generic continuous-animation timeout skip an endless audio cue.
            pb.tracks.get_mut(&sound.id).unwrap().completed_cycles = 1;
            advance_group_animations(&anims, &mut pb, 120.0);
            assert_eq!(pb.track_status(next.id), AnimRunStatus::Stopped);
            pb.pause_anim_tree(group.id, &anims);
            assert_eq!(pb.track_status(sound.id), AnimRunStatus::Paused);
            pb.resume_anim_tree(group.id, &anims);
            assert_eq!(pb.track_status(sound.id), AnimRunStatus::Playing);
            if repeat == AudioRepeat::Forever { pb.stop_anim(sound.id); }
            else { pb.tracks.get_mut(&sound.id).unwrap().status = AnimRunStatus::Finished; }
            advance_group_animations(&anims, &mut pb, 0.0);
            assert_eq!(pb.track_status(next.id), AnimRunStatus::Playing);
        }
    }

    fn sample_anim(geom: Vec<[f64; 3]>, speed: f64, replay: bool) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: Some(Uuid::new_v4()),
            name: "Test Route".into(),
            anim_type: "route".into(),
            sort_order: 1,
            speed,
            duration_sec: 10.0,
            replay,
            autoplay: false,
            geom,
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn route_sampling_interpolates_distance_and_faces_forward() {
        // Straight line going East (+lng -> +X) at equator (lat=0)
        let anim = sample_anim(vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0]], 10.0, false);
        let total = route_total_length_m(&anim.geom);
        assert!(total > 100.0 && total < 120.0);

        let mid = sample_route_at_distance(&anim, total * 0.5, true).unwrap();
        assert!((mid.lng - 0.0005).abs() < 1e-6);
        assert!((mid.rot_y - 90.0).abs() < 0.5, "East should be +90 deg, got {}", mid.rot_y);
        assert!(mid.moving);

        let start = sample_route_at_distance(&anim, 0.0, false).unwrap();
        assert!((start.lng - 0.0).abs() < 1e-9);
        assert!(!start.moving);
    }

    #[test]
    fn playback_state_transitions_play_pause_resume_stop() {
        let aid = Uuid::new_v4();
        let mut pb = PlaybackState::default();
        pb.enter_play_mode();
        assert!(pb.play_test_mode);
        assert_eq!(pb.track_status(aid), AnimRunStatus::Stopped);

        pb.play_anim(aid);
        assert_eq!(pb.track_status(aid), AnimRunStatus::Playing);
        pb.tracks.get_mut(&aid).unwrap().distance_m = 25.0;

        pb.pause_anim(aid);
        assert_eq!(pb.track_status(aid), AnimRunStatus::Paused);
        assert!((pb.tracks[&aid].distance_m - 25.0).abs() < 1e-6);

        pb.resume_anim(aid);
        assert_eq!(pb.track_status(aid), AnimRunStatus::Playing);
        assert!((pb.tracks[&aid].distance_m - 25.0).abs() < 1e-6);

        pb.stop_anim(aid);
        assert_eq!(pb.track_status(aid), AnimRunStatus::Stopped);
        assert!((pb.tracks[&aid].distance_m - 0.0).abs() < 1e-6);
        assert!(pb.tracks[&aid].has_pose_override);
    }

    #[test]
    fn spawn_hide_conflict_resolution_follows_last_action_wins() {
        use std::collections::HashSet;
        use twg_shared::spawn_hide_anim::{
            SpawnHideAction, SpawnHideConfig, SpawnHideTransition, write_spawn_hide_config,
        };

        let m1 = Uuid::new_v4();
        let mut spawn_anim = sample_anim(vec![], 1.0, false);
        let mut scfg = SpawnHideConfig::default_for(SpawnHideAction::Spawn);
        scfg.marker_ids = vec![m1];
        scfg.transition = SpawnHideTransition::Instant;
        scfg.show_fx_ring = false;
        write_spawn_hide_config(&mut spawn_anim, &scfg);

        let mut hide_anim = sample_anim(vec![], 1.0, false);
        let mut hcfg = SpawnHideConfig::default_for(SpawnHideAction::Hide);
        hcfg.marker_ids = vec![m1];
        hcfg.transition = SpawnHideTransition::Instant;
        hcfg.show_fx_ring = false;
        write_spawn_hide_config(&mut hide_anim, &hcfg);

        let anims = vec![spawn_anim.clone(), hide_anim.clone()];
        let hidden = HashSet::new();
        let mut pb = PlaybackState::default();
        pb.enter_play_mode();

        let eval = |pb: &PlaybackState| -> Option<bool> {
            let mut vis = HashMap::new();
            let mut fx = Vec::new();
            resolve_spawn_hide_overrides(&anims, &hidden, pb, &mut vis, &mut fx);
            vis.get(&m1).map(|v| v.visible)
        };

        // 1. Fresh Play Skenario: marker targeted by Spawn starts hidden automatically
        assert_eq!(eval(&pb), Some(false));

        // 2. Play Spawn -> marker becomes visible
        pb.play_anim(spawn_anim.id);
        pb.tracks.get_mut(&spawn_anim.id).unwrap().status = AnimRunStatus::Finished;
        assert_eq!(eval(&pb), Some(true));

        // 3. Play Hide after Spawn -> marker disappears again
        pb.play_anim(hide_anim.id);
        pb.tracks.get_mut(&hide_anim.id).unwrap().status = AnimRunStatus::Finished;
        assert_eq!(eval(&pb), Some(false));

        // 4. Stop Hide -> Spawn (still Finished) restores marker visible
        pb.stop_anim(hide_anim.id);
        assert_eq!(eval(&pb), Some(true));

        // 5. Stop Spawn -> back to baseline hidden
        pb.stop_anim(spawn_anim.id);
        assert_eq!(eval(&pb), Some(false));
    }

    #[test]
    fn combine_group_launches_all_children_and_cascades_pause_resume_stop() {
        use twg_shared::group_anim::{
            GroupAnimConfig, GroupAnimMode, GroupStepItem, write_group_anim_config,
        };

        let a = sample_anim(vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0]], 10.0, false);
        let b = sample_anim(vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0]], 10.0, false);
        let mut combine = sample_anim(vec![], 1.0, false);
        let mut cfg = GroupAnimConfig::default_for(GroupAnimMode::Combine);
        cfg.items = vec![GroupStepItem::new(a.id), GroupStepItem::new(b.id)];
        write_group_anim_config(&mut combine, &cfg);

        let anims = vec![a.clone(), b.clone(), combine.clone()];
        let mut pb = PlaybackState::default();
        pb.enter_play_mode();
        pb.play_anim(combine.id);

        advance_group_animations(&anims, &mut pb, 0.016);
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(combine.id), AnimRunStatus::Playing);

        pb.pause_anim_tree(combine.id, &anims);
        assert_eq!(pb.track_status(combine.id), AnimRunStatus::Paused);
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Paused);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Paused);

        pb.resume_anim_tree(combine.id, &anims);
        assert_eq!(pb.track_status(combine.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Playing);

        // Both children complete 1 cycle -> Combine marks itself Finished
        pb.tracks.get_mut(&a.id).unwrap().status = AnimRunStatus::Finished;
        pb.tracks.get_mut(&a.id).unwrap().completed_cycles = 1;
        pb.tracks.get_mut(&b.id).unwrap().status = AnimRunStatus::Finished;
        pb.tracks.get_mut(&b.id).unwrap().completed_cycles = 1;
        advance_group_animations(&anims, &mut pb, 0.0);
        assert_eq!(pb.track_status(combine.id), AnimRunStatus::Finished);

        pb.stop_anim_tree(combine.id, &anims);
        assert_eq!(pb.track_status(combine.id), AnimRunStatus::Stopped);
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Stopped);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Stopped);
    }

    #[test]
    fn sequence_wait_complete_advances_after_one_cycle_of_repeating_child_and_wait_delay_fires_early()
    {
        use twg_shared::group_anim::{
            GroupAnimConfig, GroupAnimMode, GroupStepItem, SequenceTriggerMode,
            write_group_anim_config,
        };

        // Child A has replay = true; Child B is finite; Child C triggers via WaitDelay 0.25s after B starts
        let a = sample_anim(vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0]], 10.0, true);
        let b = sample_anim(vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0]], 10.0, false);
        let c = sample_anim(vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0]], 10.0, false);

        let mut seq = sample_anim(vec![], 1.0, false);
        let mut cfg = GroupAnimConfig::default_for(GroupAnimMode::Sequence);
        cfg.sequence_trigger = SequenceTriggerMode::WaitComplete;
        cfg.items = vec![
            GroupStepItem::new(a.id),
            GroupStepItem::new(b.id),
            GroupStepItem {
                anim_id: c.id,
                trigger_override: Some(SequenceTriggerMode::WaitDelay),
                delay_override_sec: Some(0.25),
            },
        ];
        write_group_anim_config(&mut seq, &cfg);

        let anims = vec![a.clone(), b.clone(), c.clone(), seq.clone()];
        let mut pb = PlaybackState::default();
        pb.enter_play_mode();
        pb.play_anim(seq.id);

        // Step 0 (A) starts immediately; B & C still stopped
        advance_group_animations(&anims, &mut pb, 0.05);
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Stopped);
        assert_eq!(pb.track_status(c.id), AnimRunStatus::Stopped);

        // Simulate A finishing its 1st repeat cycle (stays Playing, completed_cycles = 1)
        pb.tracks.get_mut(&a.id).unwrap().completed_cycles = 1;
        advance_group_animations(&anims, &mut pb, 0.0);
        // B must start while A KEEPS repeating!
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(c.id), AnimRunStatus::Stopped);

        // Advance 0.15s (< 0.25s): C still waiting
        advance_group_animations(&anims, &mut pb, 0.15);
        assert_eq!(pb.track_status(c.id), AnimRunStatus::Stopped);

        // Advance another 0.12s (total 0.27s >= 0.25s): C starts even though B is still Playing!
        advance_group_animations(&anims, &mut pb, 0.12);
        assert_eq!(pb.track_status(b.id), AnimRunStatus::Playing);
        assert_eq!(pb.track_status(c.id), AnimRunStatus::Playing);

        // Once final step C finishes -> Sequence finishes (while repeating A remains Playing!)
        pb.tracks.get_mut(&c.id).unwrap().status = AnimRunStatus::Finished;
        pb.tracks.get_mut(&c.id).unwrap().completed_cycles = 1;
        advance_group_animations(&anims, &mut pb, 0.0);
        assert_eq!(pb.track_status(seq.id), AnimRunStatus::Finished);
        assert_eq!(pb.track_status(a.id), AnimRunStatus::Playing);
    }
}
