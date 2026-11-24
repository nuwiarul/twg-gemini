use std::collections::HashMap;

use bevy::prelude::*;
use twg_shared::{
    geo::haversine_distance_m,
    models::Anim,
    route_formation::{
        anim_bound_marker_ids, is_multi_route_anim, parse_multi_route_config,
        sample_multi_route_poses,
    },
};
use uuid::Uuid;

use crate::state::{AnimRunStatus, AppScreen, AppState, PlaybackState};

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

/// Drives per-animation route movement in Play Skenario mode based on each animation's own
/// `Play / Pause / Resume / Stop` state, `speed` (m/s), and `replay` (Repeat) flag.
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

    if !playback.play_test_mode {
        return;
    }

    // Remove stale tracks for deleted animations
    playback
        .tracks
        .retain(|aid, _| state.anims.iter().any(|a| a.id == *aid));

    let dt = time.delta_secs().min(0.1) as f64;
    let speed_mult = playback.speed_multiplier.max(0.1) as f64;
    let mut any_playing = false;

    for anim in &state.anims {
        let total_len = route_total_length_m(&anim.geom);
        let track = playback.tracks.entry(anim.id).or_default();
        track.total_length_m = total_len;
        let has_targets = !anim_bound_marker_ids(anim).is_empty();

        if track.status == AnimRunStatus::Playing {
            if total_len <= 0.01 || !has_targets {
                track.status = AnimRunStatus::Stopped;
            } else {
                let step_m = dt * anim.speed.max(0.1) * speed_mult;
                track.distance_m += step_m;
                if track.distance_m >= total_len {
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

        if track.status == AnimRunStatus::Playing {
            any_playing = true;
        }

        if track.has_pose_override && has_targets {
            let is_moving = track.status == AnimRunStatus::Playing;
            let dist = track.distance_m.clamp(0.0, total_len);
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

    playback.playing = any_playing;
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
}
