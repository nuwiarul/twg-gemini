use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Anim;

pub const ANIM_TYPE_CAMERA_STATIC: &str = "camera_static";
pub const ANIM_TYPE_CAMERA_FOLLOW: &str = "camera_follow";

pub fn is_camera_static_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_CAMERA_STATIC
}

pub fn is_camera_follow_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_CAMERA_FOLLOW
}

pub fn is_camera_anim(anim: &Anim) -> bool {
    is_camera_static_anim(anim) || is_camera_follow_anim(anim)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraEasing {
    #[default]
    Smooth,
    EaseOut,
    Linear,
}

impl CameraEasing {
    pub const ALL: &'static [Self] = &[Self::Smooth, Self::EaseOut, Self::Linear];

    pub fn label(self) -> &'static str {
        match self {
            Self::Smooth => "Smooth (Ease In-Out)",
            Self::EaseOut => "Cepat lalu Melambat (Ease Out)",
            Self::Linear => "Konstan (Linear)",
        }
    }

    pub fn eval(self, t: f64) -> f64 {
        let u = t.clamp(0.0, 1.0);
        match self {
            Self::Smooth => u * u * (3.0 - 2.0 * u),
            Self::EaseOut => 1.0 - (1.0 - u).powi(3),
            Self::Linear => u,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraStaticHoldMode {
    /// Fly to target, hold/orbit for `hold_duration_sec`, then finish cleanly.
    #[default]
    HoldDuration,
    /// Stay/orbit at target continuously until stopped or interrupted by user.
    Continuous,
}

impl CameraStaticHoldMode {
    pub const ALL: &'static [Self] = &[Self::HoldDuration, Self::Continuous];

    pub fn label(self) -> &'static str {
        match self {
            Self::HoldDuration => "Tahan N Detik lalu Selesai",
            Self::Continuous => "Terus Aktif (Sampai Stop / Orbit Terus)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraFollowPreset {
    #[default]
    Rear,
    Front,
    Right,
    Left,
    TopDown,
    Custom,
}

impl CameraFollowPreset {
    pub const ALL: &'static [Self] = &[
        Self::Rear,
        Self::Front,
        Self::Right,
        Self::Left,
        Self::TopDown,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rear => "Belakang (Chase)",
            Self::Front => "Depan (Front)",
            Self::Right => "Samping Kanan",
            Self::Left => "Samping Kiri",
            Self::TopDown => "Atas / Drone",
            Self::Custom => "Kustom",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Rear => "Belakang",
            Self::Front => "Depan",
            Self::Right => "Kanan",
            Self::Left => "Kiri",
            Self::TopDown => "Drone",
            Self::Custom => "Kustom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraFollowStopMode {
    /// Keep following until stopped manually, replaced by another camera anim, or interrupted.
    #[default]
    UntilStopped,
    /// Finish automatically once the bound target marker's active route animation finishes.
    UntilTargetRouteFinishes,
    /// Follow for a fixed duration (seconds) then finish cleanly (useful for multi-angle Groups).
    FixedDuration,
}

impl CameraFollowStopMode {
    pub const ALL: &'static [Self] = &[
        Self::UntilStopped,
        Self::UntilTargetRouteFinishes,
        Self::FixedDuration,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::UntilStopped => "Ikuti Terus (Sampai Stop)",
            Self::UntilTargetRouteFinishes => "Berhenti Saat Rute Target Selesai",
            Self::FixedDuration => "Durasi Tertentu (Detik)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraStaticConfig {
    #[serde(default)]
    pub lng: f64,
    #[serde(default)]
    pub lat: f64,
    #[serde(default)]
    pub target_altitude_m: f64,
    #[serde(default = "default_static_distance")]
    pub distance_m: f64,
    #[serde(default = "default_static_pitch")]
    pub pitch_deg: f64,
    #[serde(default)]
    pub bearing_deg: f64,
    #[serde(default = "default_fly_duration")]
    pub fly_duration_sec: f64,
    #[serde(default)]
    pub easing: CameraEasing,
    #[serde(default)]
    pub fly_arc_m: f64,
    #[serde(default)]
    pub orbit_enabled: bool,
    #[serde(default = "default_orbit_speed")]
    pub orbit_speed_deg_per_sec: f64,
    #[serde(default)]
    pub hold_mode: CameraStaticHoldMode,
    #[serde(default)]
    pub hold_duration_sec: f64,
    #[serde(default)]
    pub delay_sec: f64,
}

fn default_static_distance() -> f64 {
    250.0
}
fn default_static_pitch() -> f64 {
    48.0
}
fn default_fly_duration() -> f64 {
    2.5
}
fn default_orbit_speed() -> f64 {
    12.0
}

impl Default for CameraStaticConfig {
    fn default() -> Self {
        Self {
            lng: 0.0,
            lat: 0.0,
            target_altitude_m: 0.0,
            distance_m: default_static_distance(),
            pitch_deg: default_static_pitch(),
            bearing_deg: 0.0,
            fly_duration_sec: default_fly_duration(),
            easing: CameraEasing::Smooth,
            fly_arc_m: 0.0,
            orbit_enabled: false,
            orbit_speed_deg_per_sec: default_orbit_speed(),
            hold_mode: CameraStaticHoldMode::HoldDuration,
            hold_duration_sec: 0.0,
            delay_sec: 0.0,
        }
    }
}

impl CameraStaticConfig {
    pub fn sanitized(mut self) -> Self {
        if !self.lng.is_finite() {
            self.lng = 0.0;
        }
        if !self.lat.is_finite() {
            self.lat = 0.0;
        }
        self.lng = self.lng.clamp(-180.0, 180.0);
        self.lat = self.lat.clamp(-85.0, 85.0);
        self.target_altitude_m = if self.target_altitude_m.is_finite() {
            self.target_altitude_m.clamp(-100.0, 5000.0)
        } else {
            0.0
        };
        self.distance_m = if self.distance_m.is_finite() {
            self.distance_m.clamp(3.0, 80_000.0)
        } else {
            default_static_distance()
        };
        self.pitch_deg = if self.pitch_deg.is_finite() {
            self.pitch_deg.clamp(5.0, 82.0)
        } else {
            default_static_pitch()
        };
        self.bearing_deg = if self.bearing_deg.is_finite() {
            self.bearing_deg.rem_euclid(360.0)
        } else {
            0.0
        };
        self.fly_duration_sec = if self.fly_duration_sec.is_finite() {
            self.fly_duration_sec.clamp(0.0, 60.0)
        } else {
            default_fly_duration()
        };
        self.fly_arc_m = if self.fly_arc_m.is_finite() {
            self.fly_arc_m.clamp(0.0, 5000.0)
        } else {
            0.0
        };
        self.orbit_speed_deg_per_sec = if self.orbit_speed_deg_per_sec.is_finite() {
            self.orbit_speed_deg_per_sec.clamp(-120.0, 120.0)
        } else {
            default_orbit_speed()
        };
        self.hold_duration_sec = if self.hold_duration_sec.is_finite() {
            self.hold_duration_sec.clamp(0.0, 3600.0)
        } else {
            0.0
        };
        self.delay_sec = if self.delay_sec.is_finite() {
            self.delay_sec.clamp(0.0, 3600.0)
        } else {
            0.0
        };
        self
    }

    pub fn is_continuous(&self) -> bool {
        self.hold_mode == CameraStaticHoldMode::Continuous
    }

    pub fn total_finite_duration_sec(&self) -> Option<f64> {
        if self.is_continuous() {
            None
        } else {
            Some(self.delay_sec + self.fly_duration_sec + self.hold_duration_sec)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraFollowConfig {
    #[serde(default)]
    pub target_marker_id: Option<Uuid>,
    #[serde(default = "default_follow_height_offset")]
    pub height_offset_m: f64,
    #[serde(default)]
    pub preset: CameraFollowPreset,
    #[serde(default = "default_true")]
    pub follow_heading: bool,
    /// Relative offset from target heading when `follow_heading == true`
    /// (`0°` = Rear/Chase, `180°` = Front, `-90°` = Right side, `+90°` = Left side),
    /// or absolute compass bearing (`0..360°`) when `follow_heading == false`.
    #[serde(default)]
    pub bearing_offset_deg: f64,
    #[serde(default = "default_follow_distance")]
    pub distance_m: f64,
    #[serde(default = "default_follow_pitch")]
    pub pitch_deg: f64,
    #[serde(default = "default_fly_in")]
    pub fly_in_sec: f64,
    #[serde(default = "default_pos_smooth")]
    pub position_smoothness: f64,
    #[serde(default = "default_rot_smooth")]
    pub rotation_smoothness: f64,
    #[serde(default)]
    pub stop_mode: CameraFollowStopMode,
    #[serde(default = "default_follow_duration")]
    pub duration_sec: f64,
    #[serde(default)]
    pub delay_sec: f64,
}

fn default_true() -> bool {
    true
}
fn default_follow_height_offset() -> f64 {
    2.0
}
fn default_follow_distance() -> f64 {
    65.0
}
fn default_follow_pitch() -> f64 {
    52.0
}
fn default_fly_in() -> f64 {
    1.2
}
fn default_pos_smooth() -> f64 {
    8.0
}
fn default_rot_smooth() -> f64 {
    4.5
}
fn default_follow_duration() -> f64 {
    10.0
}

impl Default for CameraFollowConfig {
    fn default() -> Self {
        Self {
            target_marker_id: None,
            height_offset_m: default_follow_height_offset(),
            preset: CameraFollowPreset::Rear,
            follow_heading: true,
            bearing_offset_deg: 0.0,
            distance_m: default_follow_distance(),
            pitch_deg: default_follow_pitch(),
            fly_in_sec: default_fly_in(),
            position_smoothness: default_pos_smooth(),
            rotation_smoothness: default_rot_smooth(),
            stop_mode: CameraFollowStopMode::UntilStopped,
            duration_sec: default_follow_duration(),
            delay_sec: 0.0,
        }
    }
}

impl CameraFollowConfig {
    pub fn apply_preset(&mut self, preset: CameraFollowPreset) {
        self.preset = preset;
        match preset {
            CameraFollowPreset::Rear => {
                self.follow_heading = true;
                self.bearing_offset_deg = 0.0;
                self.pitch_deg = 52.0;
                self.distance_m = 65.0;
            }
            CameraFollowPreset::Front => {
                self.follow_heading = true;
                self.bearing_offset_deg = 180.0;
                self.pitch_deg = 56.0;
                self.distance_m = 55.0;
            }
            CameraFollowPreset::Right => {
                self.follow_heading = true;
                self.bearing_offset_deg = -90.0;
                self.pitch_deg = 54.0;
                self.distance_m = 70.0;
            }
            CameraFollowPreset::Left => {
                self.follow_heading = true;
                self.bearing_offset_deg = 90.0;
                self.pitch_deg = 54.0;
                self.distance_m = 70.0;
            }
            CameraFollowPreset::TopDown => {
                self.follow_heading = true;
                self.bearing_offset_deg = 0.0;
                self.pitch_deg = 14.0;
                self.distance_m = 180.0;
            }
            CameraFollowPreset::Custom => {}
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.height_offset_m = if self.height_offset_m.is_finite() {
            self.height_offset_m.clamp(-20.0, 200.0)
        } else {
            default_follow_height_offset()
        };
        self.bearing_offset_deg = if self.bearing_offset_deg.is_finite() {
            if self.follow_heading {
                let mut v = (self.bearing_offset_deg + 180.0).rem_euclid(360.0) - 180.0;
                if v <= -180.0 {
                    v = 180.0;
                }
                v
            } else {
                self.bearing_offset_deg.rem_euclid(360.0)
            }
        } else {
            0.0
        };
        self.distance_m = if self.distance_m.is_finite() {
            self.distance_m.clamp(3.0, 80_000.0)
        } else {
            default_follow_distance()
        };
        self.pitch_deg = if self.pitch_deg.is_finite() {
            self.pitch_deg.clamp(5.0, 82.0)
        } else {
            default_follow_pitch()
        };
        self.fly_in_sec = if self.fly_in_sec.is_finite() {
            self.fly_in_sec.clamp(0.0, 30.0)
        } else {
            default_fly_in()
        };
        self.position_smoothness = if self.position_smoothness.is_finite() {
            self.position_smoothness.clamp(1.0, 25.0)
        } else {
            default_pos_smooth()
        };
        self.rotation_smoothness = if self.rotation_smoothness.is_finite() {
            self.rotation_smoothness.clamp(0.5, 20.0)
        } else {
            default_rot_smooth()
        };
        self.duration_sec = if self.duration_sec.is_finite() {
            self.duration_sec.clamp(0.5, 3600.0)
        } else {
            default_follow_duration()
        };
        self.delay_sec = if self.delay_sec.is_finite() {
            self.delay_sec.clamp(0.0, 3600.0)
        } else {
            0.0
        };
        self
    }

    pub fn is_continuous(&self) -> bool {
        self.stop_mode == CameraFollowStopMode::UntilStopped
    }

    /// Computes desired TacticalCamera compass bearing (`0..360°`, where `0°` looks North / `-Z`
    /// and `90°` looks East / `+X`) given a marker's `rot_y` in degrees (where `0°` faces South
    /// / `+Z` and `180°` faces North / `-Z`).
    pub fn desired_bearing_deg(&self, marker_rot_y_deg: f64) -> f64 {
        if self.follow_heading {
            let chase_bearing = (180.0 - marker_rot_y_deg).rem_euclid(360.0);
            (chase_bearing + self.bearing_offset_deg).rem_euclid(360.0)
        } else {
            self.bearing_offset_deg.rem_euclid(360.0)
        }
    }
}

/// Signed shortest angular difference in degrees (`-180..=180`) from `from_deg` to `to_deg`.
pub fn shortest_angle_delta_deg(from_deg: f64, to_deg: f64) -> f64 {
    let mut d = (to_deg - from_deg) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

/// Interpolates compass angles along the shortest arc.
pub fn lerp_angle_deg(from_deg: f64, to_deg: f64, t: f64) -> f64 {
    (from_deg + shortest_angle_delta_deg(from_deg, to_deg) * t.clamp(0.0, 1.0)).rem_euclid(360.0)
}

pub fn parse_camera_static_config(anim: &Anim) -> CameraStaticConfig {
    let mut cfg: CameraStaticConfig = anim
        .data
        .get("camera_static")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.lng.abs() < 1e-9
        && cfg.lat.abs() < 1e-9
        && let Some(p) = anim.geom.first()
    {
        cfg.lng = p[0];
        cfg.lat = p[1];
        if cfg.target_altitude_m.abs() < 1e-9 {
            cfg.target_altitude_m = p[2];
        }
    }
    cfg.sanitized()
}

pub fn write_camera_static_config(anim: &mut Anim, cfg: &CameraStaticConfig) {
    let clean = cfg.clone().sanitized();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "camera_static".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    anim.geom = vec![[clean.lng, clean.lat, clean.target_altitude_m]];
    anim.duration_sec = clean.total_finite_duration_sec().unwrap_or(0.0);
}

pub fn parse_camera_follow_config(anim: &Anim) -> CameraFollowConfig {
    let mut cfg: CameraFollowConfig = anim
        .data
        .get("camera_follow")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.target_marker_id.is_none() {
        cfg.target_marker_id = anim.marker_id;
    }
    cfg.sanitized()
}

pub fn write_camera_follow_config(anim: &mut Anim, cfg: &CameraFollowConfig) {
    let clean = cfg.clone().sanitized();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "camera_follow".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    anim.marker_id = clean.target_marker_id;
    anim.duration_sec = match clean.stop_mode {
        CameraFollowStopMode::FixedDuration => clean.delay_sec + clean.duration_sec,
        CameraFollowStopMode::UntilStopped | CameraFollowStopMode::UntilTargetRouteFinishes => 0.0,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn dummy_anim(anim_type: &str) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Cam Test".into(),
            anim_type: anim_type.into(),
            sort_order: 0,
            speed: 1.0,
            duration_sec: 0.0,
            replay: false,
            autoplay: false,
            geom: vec![],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn camera_static_roundtrips_and_computes_duration() {
        let mut anim = dummy_anim(ANIM_TYPE_CAMERA_STATIC);
        let cfg = CameraStaticConfig {
            lng: 106.827,
            lat: -6.175,
            target_altitude_m: 12.0,
            distance_m: 180.0,
            pitch_deg: 55.0,
            bearing_deg: 395.0,
            fly_duration_sec: 3.0,
            easing: CameraEasing::EaseOut,
            fly_arc_m: 60.0,
            orbit_enabled: true,
            orbit_speed_deg_per_sec: -15.0,
            hold_mode: CameraStaticHoldMode::HoldDuration,
            hold_duration_sec: 4.0,
            delay_sec: 1.0,
        };
        write_camera_static_config(&mut anim, &cfg);
        let parsed = parse_camera_static_config(&anim);
        assert!((parsed.bearing_deg - 35.0).abs() < 1e-6);
        assert_eq!(parsed.total_finite_duration_sec(), Some(8.0));
        assert_eq!(anim.geom, vec![[106.827, -6.175, 12.0]]);
    }

    #[test]
    fn camera_follow_presets_align_with_marker_heading_for_all_sides() {
        let mut cfg = CameraFollowConfig::default();
        // Marker facing North (rot_y = 180 deg)
        cfg.apply_preset(CameraFollowPreset::Rear);
        assert!((cfg.desired_bearing_deg(180.0) - 0.0).abs() < 1e-6);

        cfg.apply_preset(CameraFollowPreset::Front);
        assert!((cfg.desired_bearing_deg(180.0) - 180.0).abs() < 1e-6);

        // Right side of North-facing marker is East -> camera sits East and looks West (270 deg)
        cfg.apply_preset(CameraFollowPreset::Right);
        assert!((cfg.desired_bearing_deg(180.0) - 270.0).abs() < 1e-6);

        // Left side of North-facing marker is West -> camera sits West and looks East (90 deg)
        cfg.apply_preset(CameraFollowPreset::Left);
        assert!((cfg.desired_bearing_deg(180.0) - 90.0).abs() < 1e-6);

        // Marker facing East (rot_y = 90 deg) -> Rear camera looks East (90 deg)
        cfg.apply_preset(CameraFollowPreset::Rear);
        assert!((cfg.desired_bearing_deg(90.0) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn shortest_angle_wraps_across_zero_seam() {
        assert!((shortest_angle_delta_deg(350.0, 10.0) - 20.0).abs() < 1e-6);
        assert!((shortest_angle_delta_deg(10.0, 350.0) + 20.0).abs() < 1e-6);
        assert!((lerp_angle_deg(350.0, 10.0, 0.5) - 0.0).abs() < 1e-6);
    }
}
