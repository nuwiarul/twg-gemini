use std::collections::HashMap;

use bevy::prelude::*;
use twg_shared::{geo::haversine_distance_m, models::Anim};
use uuid::Uuid;

use crate::state::{AppScreen, AppState, PlaybackState};

#[derive(Debug, Clone, Copy)]
pub struct MarkerPlaybackPose {
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
    pub rot_y: f64,
    pub moving: bool,
}

#[derive(Resource, Default)]
pub struct PlaybackOverrides {
    pub markers: HashMap<Uuid, MarkerPlaybackPose>,
}

pub fn scenario_total_duration(state: &AppState, playback: &PlaybackState) -> f32 {
    if playback.play_all_anims {
        state
            .anims
            .iter()
            .map(|a| a.duration_sec as f32)
            .fold(0.0_f32, f32::max)
            .max(10.0)
    } else if let Some(aid) = playback.active_anim_id
        && let Some(anim) = state.anims.iter().find(|a| a.id == aid)
    {
        (anim.duration_sec as f32).max(1.0)
    } else {
        state
            .anims
            .first()
            .map(|a| (a.duration_sec as f32).max(1.0))
            .unwrap_or(10.0)
    }
}

pub fn playback_system(
    time: Res<Time>,
    state: Res<AppState>,
    mut playback: ResMut<PlaybackState>,
    mut overrides: ResMut<PlaybackOverrides>,
) {
    overrides.markers.clear();
    if state.screen != AppScreen::PlotSkenario {
        playback.playing = false;
        return;
    }

    // Ensure an active_anim_id exists when single-track mode is chosen
    if playback.active_anim_id.is_none()
        && let Some(first) = state.anims.first()
    {
        playback.active_anim_id = Some(first.id);
    }

    let total_dur = scenario_total_duration(&state, &playback);

    if playback.play_test_mode && playback.playing {
        playback.elapsed_sec += time.delta_secs() * playback.speed_multiplier.max(0.1);
        if playback.elapsed_sec >= total_dur {
            if playback.loop_anim {
                playback.elapsed_sec %= total_dur;
            } else {
                playback.elapsed_sec = total_dur;
                playback.playing = false;
            }
        }
    }

    let should_apply_overrides = playback.play_test_mode || playback.elapsed_sec > 0.001;
    if !should_apply_overrides {
        return;
    }

    if playback.play_all_anims {
        for anim in &state.anims {
            let Some(marker_id) = anim.marker_id else {
                continue;
            };
            let dur = (anim.duration_sec as f32).max(1.0);
            let t = if anim.replay || playback.loop_anim {
                (playback.elapsed_sec % dur) / dur
            } else {
                (playback.elapsed_sec / dur).clamp(0.0, 1.0)
            };
            let is_moving = playback.playing && (anim.replay || playback.loop_anim || playback.elapsed_sec < dur);
            if let Some(pose) = sample_route_anim(anim, t.clamp(0.0, 1.0), is_moving) {
                overrides.markers.insert(marker_id, pose);
            }
        }
    } else if let Some(anim_id) = playback.active_anim_id
        && let Some(anim) = state.anims.iter().find(|a| a.id == anim_id)
        && let Some(marker_id) = anim.marker_id
    {
        let dur = (anim.duration_sec as f32).max(1.0);
        let t = (playback.elapsed_sec / dur).clamp(0.0, 1.0);
        let is_moving = playback.playing && (playback.loop_anim || anim.replay || playback.elapsed_sec < dur);
        if let Some(pose) = sample_route_anim(anim, t, is_moving) {
            overrides.markers.insert(marker_id, pose);
        }
    }
}

fn sample_route_anim(anim: &Anim, t: f32, moving: bool) -> Option<MarkerPlaybackPose> {
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

    let target_dist = total_len * (t as f64);
    let mut acc = 0.0_f64;

    for (idx, seg_len) in seg_lengths.iter().enumerate() {
        if acc + seg_len >= target_dist || idx == seg_lengths.len() - 1 {
            let local_t = ((target_dist - acc) / seg_len).clamp(0.0, 1.0);
            let p0 = anim.geom[idx];
            let p1 = anim.geom[idx + 1];

            let lng = p0[0] + (p1[0] - p0[0]) * local_t;
            let lat = p0[1] + (p1[1] - p0[1]) * local_t;
            let alt = p0[2] + (p1[2] - p0[2]) * local_t;

            // Compute heading yaw around Bevy +Y (East = +X, North = -Z)
            let dx = (p1[0] - p0[0]) * lat.to_radians().cos();
            let dz = -(p1[1] - p0[1]);
            // atan2(-dx, -dz) aligns model forward (-Z) with travel vector
            let rot_y = (-dx).atan2(-dz).to_degrees();

            return Some(MarkerPlaybackPose {
                lng,
                lat,
                altitude: alt,
                rot_y,
                moving,
            });
        }
        acc += seg_len;
    }

    None
}
