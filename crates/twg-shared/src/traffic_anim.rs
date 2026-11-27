use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    geo::{GeoAnchor, haversine_distance_m},
    models::Anim,
};

pub const ANIM_TYPE_TRAFFIC_FLOW: &str = "traffic_flow";
pub const ANIM_TYPE_TRAFFIC_SIGNAL: &str = "traffic_signal";

/// Fixed deterministic simulation timestep (30 Hz) so Operator & Layar Besar
/// produce identical traffic trajectories from the same seed & elapsed time.
pub const TRAFFIC_SIM_FIXED_DT: f64 = 1.0 / 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OvertakeSide {
    /// Indonesian left-hand traffic standard: overtake on the right side.
    #[default]
    Right,
    /// Right-hand traffic standard: overtake on the left side.
    Left,
}

impl OvertakeSide {
    pub const ALL: &'static [Self] = &[Self::Right, Self::Left];

    pub fn label(self) -> &'static str {
        match self {
            Self::Right => "Kanan (Lajur Kiri / ID)",
            Self::Left => "Kiri (Lajur Kanan)",
        }
    }

    /// Sign along the lane's right-hand normal `right = (-fz, fx)`.
    pub fn lateral_sign(self) -> f64 {
        match self {
            Self::Right => 1.0,
            Self::Left => -1.0,
        }
    }
}

/// One vehicle model available inside a Traffic animation's shared pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficVehicleEntry {
    pub model_path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_vehicle_scale")]
    pub scale: f64,
    /// Approximate bumper-to-bumper vehicle length in meters for accurate spacing & overtaking.
    #[serde(default = "default_vehicle_length_m")]
    pub length_m: f64,
    /// Relative random pickup probability weight (`> 0`).
    #[serde(default = "default_vehicle_weight")]
    pub weight: f32,
    /// Optional yaw adjustment in degrees if a model does not face standard glTF `+Z`.
    #[serde(default)]
    pub rot_y_offset_deg: f64,
}

fn default_vehicle_scale() -> f64 {
    1.0
}
fn default_vehicle_length_m() -> f64 {
    4.5
}
fn default_vehicle_weight() -> f32 {
    1.0
}

impl TrafficVehicleEntry {
    pub fn sanitized(&self) -> Self {
        Self {
            model_path: self.model_path.trim().to_string(),
            name: if self.name.trim().is_empty() {
                self.model_path
                    .rsplit('/')
                    .next()
                    .unwrap_or("Kendaraan")
                    .to_string()
            } else {
                self.name.trim().to_string()
            },
            scale: self.scale.clamp(0.01, 20.0),
            length_m: self.length_m.clamp(1.2, 24.0),
            weight: self.weight.clamp(0.05, 100.0),
            rot_y_offset_deg: self.rot_y_offset_deg.rem_euclid(360.0),
        }
    }

    /// Heuristically estimates vehicle length in meters from its catalog name/path.
    pub fn estimate_length_m(name: &str, model_path: &str) -> f64 {
        let lower = format!("{} {}", name.to_lowercase(), model_path.to_lowercase());
        if lower.contains("motor")
            || lower.contains("vespa")
            || lower.contains("sepeda")
            || lower.contains("bike")
            || lower.contains("patwal_motor")
        {
            2.3
        } else if lower.contains("bus")
            || lower.contains("bis")
            || lower.contains("truk")
            || lower.contains("truck")
            || lower.contains("container")
            || lower.contains("damkar")
            || lower.contains("water_cannon")
            || lower.contains("awc")
        {
            8.5
        } else if lower.contains("barracuda")
            || lower.contains("ransus")
            || lower.contains("ambulan")
            || lower.contains("angkot")
            || lower.contains("elf")
            || lower.contains("hiace")
            || lower.contains("box")
            || lower.contains("van")
            || lower.contains("pickup")
            || lower.contains("hilux")
        {
            5.6
        } else {
            4.5
        }
    }
}

/// One directional traffic lane (route polyline) inside a Traffic animation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficLane {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    /// Polyline waypoints `[lng, lat, alt]`.
    #[serde(default)]
    pub geom: Vec<[f64; 3]>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional per-lane override for maximum simultaneous vehicles.
    #[serde(default)]
    pub max_vehicles_override: Option<u32>,
    /// Optional per-lane minimum speed override (m/s).
    #[serde(default)]
    pub min_speed_override: Option<f64>,
    /// Optional per-lane maximum speed override (m/s).
    #[serde(default)]
    pub max_speed_override: Option<f64>,
    /// Optional subset of `model_path`s allowed on this lane. Empty = use entire shared pool.
    #[serde(default)]
    pub allowed_model_paths: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl TrafficLane {
    pub fn new(name: impl Into<String>, geom: Vec<[f64; 3]>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            geom,
            enabled: true,
            max_vehicles_override: None,
            min_speed_override: None,
            max_speed_override: None,
            allowed_model_paths: Vec::new(),
        }
    }

    pub fn total_length_m(&self) -> f64 {
        if self.geom.len() < 2 {
            return 0.0;
        }
        self.geom
            .windows(2)
            .map(|w| haversine_distance_m(w[0][0], w[0][1], w[1][0], w[1][1]).max(0.01))
            .sum()
    }

    pub fn sanitized(&self, fallback_idx: usize) -> Self {
        let min_spd = self.min_speed_override.map(|v| v.clamp(1.0, 60.0));
        let max_spd = self
            .max_speed_override
            .map(|v| v.clamp(min_spd.unwrap_or(1.0), 60.0));
        Self {
            id: if self.id.is_nil() {
                Uuid::new_v4()
            } else {
                self.id
            },
            name: if self.name.trim().is_empty() {
                format!("Jalur {}", fallback_idx + 1)
            } else {
                self.name.trim().to_string()
            },
            geom: self.geom.clone(),
            enabled: self.enabled,
            max_vehicles_override: self.max_vehicles_override.map(|v| v.clamp(1, 25)),
            min_speed_override: min_spd,
            max_speed_override: max_spd,
            allowed_model_paths: self
                .allowed_model_paths
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficLightColor {
    #[default]
    Red,
    Yellow,
    Green,
}

impl TrafficLightColor {
    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Merah",
            Self::Yellow => "Kuning",
            Self::Green => "Hijau",
        }
    }
}

/// One ordered signal phase in a Traffic Signal (`traffic_signal`) intersection cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficSignalPhase {
    #[serde(default)]
    pub name: String,
    /// Green duration in seconds (`2.0..=180.0`).
    #[serde(default = "default_signal_green_sec")]
    pub green_sec: f64,
    /// Yellow caution duration in seconds (`1.0..=15.0`).
    #[serde(default = "default_signal_yellow_sec")]
    pub yellow_sec: f64,
    /// All-red safety clearance pause after yellow before the next phase turns green (`0.0..=10.0`).
    #[serde(default = "default_signal_all_red_sec")]
    pub all_red_sec: f64,
}

fn default_signal_green_sec() -> f64 {
    12.0
}
fn default_signal_yellow_sec() -> f64 {
    3.0
}
fn default_signal_all_red_sec() -> f64 {
    1.5
}

impl Default for TrafficSignalPhase {
    fn default() -> Self {
        Self {
            name: "Fase 1".to_string(),
            green_sec: default_signal_green_sec(),
            yellow_sec: default_signal_yellow_sec(),
            all_red_sec: default_signal_all_red_sec(),
        }
    }
}

impl TrafficSignalPhase {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn sanitized(&self, fallback_idx: usize) -> Self {
        Self {
            name: if self.name.trim().is_empty() {
                format!("Fase {}", fallback_idx + 1)
            } else {
                self.name.trim().to_string()
            },
            green_sec: self.green_sec.clamp(2.0, 180.0),
            yellow_sec: self.yellow_sec.clamp(1.0, 15.0),
            all_red_sec: self.all_red_sec.clamp(0.0, 10.0),
        }
    }

    pub fn span_sec(&self) -> f64 {
        self.green_sec + self.yellow_sec + self.all_red_sec
    }
}

/// One 3D Traffic Light (APILL) bound to a specific `TrafficLane` stop line (`stop_s_m`)
/// and a signal phase (`phase_idx`), with full independent 3D Move / Rotate / Scale transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficLightSignal {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    /// Bound `TrafficLane.id` that this signal controls.
    pub lane_id: Uuid,
    /// Longitudinal stop-line distance from the start of the bound lane (meters).
    #[serde(default)]
    pub stop_s_m: f64,
    /// Signal phase index (`0, 1, 2, ...`). Signals sharing `phase_idx` turn Green together.
    #[serde(default)]
    pub phase_idx: usize,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// World geographic position `[lng, lat, alt]` of the 3D traffic light pole base.
    #[serde(default)]
    pub pos_lnglat: [f64; 3],
    /// World Euler rotation `[rot_x, rot_y, rot_z]` in degrees (YXZ order, matching 3D Markers).
    #[serde(default)]
    pub rot_deg: [f64; 3],
    /// 3D scale `[sx, sy, sz]` of the procedural traffic light.
    #[serde(default = "default_traffic_light_scale")]
    pub scale: [f64; 3],
}

fn default_traffic_light_scale() -> [f64; 3] {
    [1.0, 1.0, 1.0]
}

impl TrafficLightSignal {
    pub fn sanitized(&self, fallback_idx: usize) -> Self {
        let clean_axis = |v: f64| if v.is_finite() { v.clamp(0.15, 15.0) } else { 1.0 };
        let clean_rot = |v: f64| if v.is_finite() { v } else { 0.0 };
        let clean_alt = if self.pos_lnglat[2].is_finite() {
            self.pos_lnglat[2].clamp(-100.0, 20_000.0)
        } else {
            0.0
        };
        Self {
            id: if self.id.is_nil() {
                Uuid::new_v4()
            } else {
                self.id
            },
            name: if self.name.trim().is_empty() {
                format!("Lampu {}", fallback_idx + 1)
            } else {
                self.name.trim().to_string()
            },
            lane_id: self.lane_id,
            stop_s_m: if self.stop_s_m.is_finite() {
                self.stop_s_m.max(1.0)
            } else {
                10.0
            },
            phase_idx: self.phase_idx.min(15),
            enabled: self.enabled,
            pos_lnglat: [
                if self.pos_lnglat[0].is_finite() {
                    self.pos_lnglat[0]
                } else {
                    0.0
                },
                if self.pos_lnglat[1].is_finite() {
                    self.pos_lnglat[1]
                } else {
                    0.0
                },
                clean_alt,
            ],
            rot_deg: [
                clean_rot(self.rot_deg[0]),
                clean_rot(self.rot_deg[1]),
                clean_rot(self.rot_deg[2]),
            ],
            scale: [
                clean_axis(self.scale[0]),
                clean_axis(self.scale[1]),
                clean_axis(self.scale[2]),
            ],
        }
    }
}

/// Resolved 3D world pose & active lamp color for one procedural Traffic Light.
#[derive(Debug, Clone)]
pub struct TrafficLightVisualPose {
    pub light_id: Uuid,
    pub name: String,
    pub lane_id: Uuid,
    pub phase_idx: usize,
    pub color: TrafficLightColor,
    pub remaining_sec: f64,
    pub pos: [f32; 3],
    pub rot_deg: [f32; 3],
    pub scale: [f32; 3],
    pub stop_line_a: [f32; 3],
    pub stop_line_b: [f32; 3],
    pub stop_center: [f32; 3],
    pub enabled: bool,
}

/// Live runtime telemetry for a `traffic_signal` (`Lalu Lintas Lampu Merah`) animation.
#[derive(Debug, Clone, Default)]
pub struct TrafficSignalLiveStatus {
    pub active_phase_idx: Option<usize>,
    pub active_phase_name: String,
    pub active_color: TrafficLightColor,
    pub is_all_red: bool,
    pub remaining_sec: f64,
    pub total_vehicles: usize,
    pub stopped_vehicles: usize,
    pub light_states: Vec<TrafficLightVisualPose>,
}

/// Full configuration for a multi-lane pooled Traffic simulation animation stored in `anim.data["traffic"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficAnimConfig {
    #[serde(default)]
    pub lanes: Vec<TrafficLane>,
    #[serde(default)]
    pub vehicle_pool: Vec<TrafficVehicleEntry>,
    #[serde(default = "default_max_vehicles_per_lane")]
    pub max_vehicles_per_lane: u32,
    #[serde(default = "default_max_total_vehicles")]
    pub max_total_vehicles: u32,
    #[serde(default = "default_min_speed_mps")]
    pub min_speed_mps: f64,
    #[serde(default = "default_max_speed_mps")]
    pub max_speed_mps: f64,
    #[serde(default = "default_spawn_interval_min")]
    pub spawn_interval_min_sec: f64,
    #[serde(default = "default_spawn_interval_max")]
    pub spawn_interval_max_sec: f64,
    #[serde(default = "default_lane_width_m")]
    pub lane_width_m: f64,
    #[serde(default = "default_min_safe_gap_m")]
    pub min_safe_gap_m: f64,
    #[serde(default = "default_true")]
    pub allow_overtaking: bool,
    #[serde(default)]
    pub overtake_side: OvertakeSide,
    /// Percentage by which a slower front vehicle temporarily slows down while being overtaken (`0..=60`).
    #[serde(default = "default_front_yield_slowdown_pct")]
    pub front_yield_slowdown_pct: f64,
    #[serde(default = "default_true")]
    pub yield_at_intersections: bool,
    #[serde(default = "default_true")]
    pub continuous: bool,
    #[serde(default = "default_traffic_seed")]
    pub seed: u64,
    /// Placed 3D traffic light signals (used by `traffic_signal` animations).
    #[serde(default)]
    pub traffic_lights: Vec<TrafficLightSignal>,
    /// Ordered signal phases (`Fase 1`, `Fase 2`, ...) controlling green/yellow/all-red rotation.
    #[serde(default)]
    pub signal_phases: Vec<TrafficSignalPhase>,
    /// Optional start time offset in seconds for the signal cycle (green-wave coordination).
    #[serde(default)]
    pub signal_cycle_offset_sec: f64,
    /// Distance before each traffic light stop line where overtaking is suppressed (`0..=1000` m, default `35` m; `0` disables).
    #[serde(default = "default_signal_no_overtake_zone_m")]
    pub signal_no_overtake_zone_m: f64,
}

fn default_signal_no_overtake_zone_m() -> f64 {
    35.0
}

fn default_max_vehicles_per_lane() -> u32 {
    5
}
fn default_max_total_vehicles() -> u32 {
    24
}
fn default_min_speed_mps() -> f64 {
    6.5
}
fn default_max_speed_mps() -> f64 {
    14.5
}
fn default_spawn_interval_min() -> f64 {
    1.5
}
fn default_spawn_interval_max() -> f64 {
    3.8
}
fn default_lane_width_m() -> f64 {
    3.6
}
fn default_min_safe_gap_m() -> f64 {
    5.0
}
fn default_front_yield_slowdown_pct() -> f64 {
    20.0
}
fn default_traffic_seed() -> u64 {
    20_261_126
}

impl Default for TrafficAnimConfig {
    fn default() -> Self {
        Self {
            lanes: Vec::new(),
            vehicle_pool: Vec::new(),
            max_vehicles_per_lane: default_max_vehicles_per_lane(),
            max_total_vehicles: default_max_total_vehicles(),
            min_speed_mps: default_min_speed_mps(),
            max_speed_mps: default_max_speed_mps(),
            spawn_interval_min_sec: default_spawn_interval_min(),
            spawn_interval_max_sec: default_spawn_interval_max(),
            lane_width_m: default_lane_width_m(),
            min_safe_gap_m: default_min_safe_gap_m(),
            allow_overtaking: true,
            overtake_side: OvertakeSide::Right,
            front_yield_slowdown_pct: default_front_yield_slowdown_pct(),
            yield_at_intersections: true,
            continuous: true,
            seed: default_traffic_seed(),
            traffic_lights: Vec::new(),
            signal_phases: Vec::new(),
            signal_cycle_offset_sec: 0.0,
            signal_no_overtake_zone_m: default_signal_no_overtake_zone_m(),
        }
    }
}

impl TrafficAnimConfig {
    pub fn sanitized(&self) -> Self {
        let min_speed = self.min_speed_mps.clamp(1.0, 55.0);
        let max_speed = self.max_speed_mps.clamp(min_speed, 60.0);
        let spawn_min = self.spawn_interval_min_sec.clamp(0.3, 60.0);
        let spawn_max = self.spawn_interval_max_sec.clamp(spawn_min, 90.0);
        let lanes: Vec<TrafficLane> = self
            .lanes
            .iter()
            .enumerate()
            .map(|(i, l)| l.sanitized(i))
            .collect();
        let vehicle_pool: Vec<TrafficVehicleEntry> = self
            .vehicle_pool
            .iter()
            .map(TrafficVehicleEntry::sanitized)
            .filter(|v| !v.model_path.is_empty())
            .collect();
        let traffic_lights: Vec<TrafficLightSignal> = self
            .traffic_lights
            .iter()
            .enumerate()
            .map(|(i, l)| l.sanitized(i))
            .collect();
        let signal_phases: Vec<TrafficSignalPhase> = self
            .signal_phases
            .iter()
            .enumerate()
            .map(|(i, p)| p.sanitized(i))
            .collect();
        Self {
            lanes,
            vehicle_pool,
            max_vehicles_per_lane: self.max_vehicles_per_lane.clamp(1, 25),
            max_total_vehicles: self.max_total_vehicles.clamp(1, 100),
            min_speed_mps: min_speed,
            max_speed_mps: max_speed,
            spawn_interval_min_sec: spawn_min,
            spawn_interval_max_sec: spawn_max,
            lane_width_m: if self.lane_width_m.is_finite() {
                self.lane_width_m.max(0.5)
            } else {
                default_lane_width_m()
            },
            min_safe_gap_m: self.min_safe_gap_m.clamp(1.5, 25.0),
            allow_overtaking: self.allow_overtaking,
            overtake_side: self.overtake_side,
            front_yield_slowdown_pct: self.front_yield_slowdown_pct.clamp(0.0, 60.0),
            yield_at_intersections: self.yield_at_intersections,
            continuous: self.continuous,
            seed: if self.seed == 0 {
                default_traffic_seed()
            } else {
                self.seed
            },
            traffic_lights,
            signal_phases,
            signal_cycle_offset_sec: if self.signal_cycle_offset_sec.is_finite() {
                self.signal_cycle_offset_sec.clamp(0.0, 600.0)
            } else {
                0.0
            },
            signal_no_overtake_zone_m: if self.signal_no_overtake_zone_m.is_finite() {
                self.signal_no_overtake_zone_m.clamp(0.0, 1000.0)
            } else {
                default_signal_no_overtake_zone_m()
            },
        }
    }

    /// Returns the effective ordered list of signal phases (synthesizing defaults if empty
    /// whenever at least one traffic light is present).
    pub fn effective_signal_phases(&self) -> Vec<TrafficSignalPhase> {
        let max_used = self
            .traffic_lights
            .iter()
            .filter(|l| l.enabled)
            .map(|l| l.phase_idx)
            .max();
        let mut phases: Vec<TrafficSignalPhase> = self
            .signal_phases
            .iter()
            .enumerate()
            .map(|(i, p)| p.sanitized(i))
            .collect();
        if let Some(m) = max_used {
            while phases.len() <= m {
                let idx = phases.len();
                phases.push(TrafficSignalPhase::new(format!("Fase {}", idx + 1)));
            }
        }
        phases
    }

    pub fn has_playable_setup(&self) -> bool {
        let has_lane = self
            .lanes
            .iter()
            .any(|l| l.enabled && l.geom.len() >= 2 && l.total_length_m() >= 5.0);
        has_lane && !self.vehicle_pool.is_empty()
    }

    /// Computes a stable hash of the traffic config + anchor so runtime visual pools
    /// automatically rebuild whenever lanes, pool models, signals, or parameters are edited.
    pub fn config_signature(&self, anchor: GeoAnchor) -> u64 {
        let mut h = DefaultHasher::new();
        anchor.center_lng.to_bits().hash(&mut h);
        anchor.center_lat.to_bits().hash(&mut h);
        self.max_vehicles_per_lane.hash(&mut h);
        self.max_total_vehicles.hash(&mut h);
        self.min_speed_mps.to_bits().hash(&mut h);
        self.max_speed_mps.to_bits().hash(&mut h);
        self.spawn_interval_min_sec.to_bits().hash(&mut h);
        self.spawn_interval_max_sec.to_bits().hash(&mut h);
        self.lane_width_m.to_bits().hash(&mut h);
        self.min_safe_gap_m.to_bits().hash(&mut h);
        self.allow_overtaking.hash(&mut h);
        (self.overtake_side as u8).hash(&mut h);
        self.front_yield_slowdown_pct.to_bits().hash(&mut h);
        self.yield_at_intersections.hash(&mut h);
        self.continuous.hash(&mut h);
        self.seed.hash(&mut h);
        self.signal_cycle_offset_sec.to_bits().hash(&mut h);
        self.signal_no_overtake_zone_m.to_bits().hash(&mut h);
        self.lanes.len().hash(&mut h);
        for l in &self.lanes {
            l.id.hash(&mut h);
            l.enabled.hash(&mut h);
            l.max_vehicles_override.hash(&mut h);
            l.min_speed_override.map(f64::to_bits).hash(&mut h);
            l.max_speed_override.map(f64::to_bits).hash(&mut h);
            l.allowed_model_paths.hash(&mut h);
            l.geom.len().hash(&mut h);
            for p in &l.geom {
                p[0].to_bits().hash(&mut h);
                p[1].to_bits().hash(&mut h);
                p[2].to_bits().hash(&mut h);
            }
        }
        self.vehicle_pool.len().hash(&mut h);
        for v in &self.vehicle_pool {
            v.model_path.hash(&mut h);
            v.scale.to_bits().hash(&mut h);
            v.length_m.to_bits().hash(&mut h);
            v.weight.to_bits().hash(&mut h);
            v.rot_y_offset_deg.to_bits().hash(&mut h);
        }
        self.signal_phases.len().hash(&mut h);
        for p in &self.signal_phases {
            p.name.hash(&mut h);
            p.green_sec.to_bits().hash(&mut h);
            p.yellow_sec.to_bits().hash(&mut h);
            p.all_red_sec.to_bits().hash(&mut h);
        }
        self.traffic_lights.len().hash(&mut h);
        for tl in &self.traffic_lights {
            tl.id.hash(&mut h);
            tl.lane_id.hash(&mut h);
            tl.stop_s_m.to_bits().hash(&mut h);
            tl.phase_idx.hash(&mut h);
            tl.enabled.hash(&mut h);
            for c in tl.pos_lnglat {
                c.to_bits().hash(&mut h);
            }
            for r in tl.rot_deg {
                r.to_bits().hash(&mut h);
            }
            for s in tl.scale {
                s.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }
}

pub fn is_traffic_anim(anim: &Anim) -> bool {
    anim.anim_type.eq_ignore_ascii_case(ANIM_TYPE_TRAFFIC_FLOW)
}

pub fn is_traffic_signal_anim(anim: &Anim) -> bool {
    anim.anim_type
        .eq_ignore_ascii_case(ANIM_TYPE_TRAFFIC_SIGNAL)
}

pub fn is_traffic_flow_or_signal(anim: &Anim) -> bool {
    is_traffic_anim(anim) || is_traffic_signal_anim(anim)
}

pub fn parse_traffic_anim_config(anim: &Anim) -> TrafficAnimConfig {
    let mut cfg: TrafficAnimConfig = anim
        .data
        .get("traffic")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // Backward/convenience fallback: if `lanes` is empty but `anim.geom` has a polyline, expose it as Lane 1.
    if cfg.lanes.is_empty() && anim.geom.len() >= 2 {
        cfg.lanes.push(TrafficLane::new("Jalur 1", anim.geom.clone()));
    }
    cfg.sanitized()
}

pub fn write_traffic_anim_config(anim: &mut Anim, cfg: &TrafficAnimConfig) {
    let keep_signal = is_traffic_signal_anim(anim);
    write_traffic_anim_config_with_type(
        anim,
        cfg,
        if keep_signal {
            ANIM_TYPE_TRAFFIC_SIGNAL
        } else {
            ANIM_TYPE_TRAFFIC_FLOW
        },
    );
}

pub fn write_traffic_signal_config(anim: &mut Anim, cfg: &TrafficAnimConfig) {
    write_traffic_anim_config_with_type(anim, cfg, ANIM_TYPE_TRAFFIC_SIGNAL);
}

fn write_traffic_anim_config_with_type(
    anim: &mut Anim,
    cfg: &TrafficAnimConfig,
    anim_type: &str,
) {
    let clean = cfg.sanitized();
    anim.anim_type = anim_type.to_string();
    anim.marker_id = None;
    // Mirror first lane geometry into `anim.geom` so focus/search/map bounds work seamlessly.
    if let Some(first_lane) = clean.lanes.iter().find(|l| l.geom.len() >= 2) {
        anim.geom = first_lane.geom.clone();
    } else {
        anim.geom.clear();
    }
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "traffic".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
}

/// Builds a parallel reverse-direction lane offset laterally by `lane_width_m`
/// according to `overtake_side` (for Indonesia left-hand traffic, oncoming lane lies to the right).
pub fn build_reverse_lane(
    source: &TrafficLane,
    lane_width_m: f64,
    overtake_side: OvertakeSide,
    new_name: impl Into<String>,
) -> Option<TrafficLane> {
    if source.geom.len() < 2 {
        return None;
    }
    let anchor = GeoAnchor::new(source.geom[0][0], source.geom[0][1]);
    let local_pts: Vec<[f64; 3]> = source
        .geom
        .iter()
        .map(|p| {
            let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
            [l[0] as f64, l[1] as f64, l[2] as f64]
        })
        .collect();

    let n = local_pts.len();
    let mut seg_rights: Vec<(f64, f64)> = Vec::with_capacity(n - 1);
    for w in local_pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dz = w[1][2] - w[0][2];
        let len = dx.hypot(dz);
        if len > 1e-4 {
            let fx = dx / len;
            let fz = dz / len;
            // Right normal in Bevy ground frame (+X east, +Z south): (-fz, fx)
            seg_rights.push((-fz, fx));
        } else {
            seg_rights.push((0.0, 1.0));
        }
    }

    let offset_signed = lane_width_m.max(0.5) * overtake_side.lateral_sign();
    let mut offset_geo: Vec<[f64; 3]> = Vec::with_capacity(n);
    for i in 0..n {
        let (rx, rz) = if i == 0 {
            seg_rights[0]
        } else if i == n - 1 {
            seg_rights[n - 2]
        } else {
            let a = seg_rights[i - 1];
            let b = seg_rights[i];
            let sx = a.0 + b.0;
            let sz = a.1 + b.1;
            let sl = sx.hypot(sz);
            if sl > 1e-4 {
                (sx / sl, sz / sl)
            } else {
                a
            }
        };
        let ox = local_pts[i][0] + rx * offset_signed;
        let oy = local_pts[i][1];
        let oz = local_pts[i][2] + rz * offset_signed;
        let (lng, lat, alt) = anchor.local_to_lng_lat_alt([ox as f32, oy as f32, oz as f32]);
        offset_geo.push([lng, lat, alt]);
    }
    offset_geo.reverse();

    Some(TrafficLane {
        id: Uuid::new_v4(),
        name: new_name.into(),
        geom: offset_geo,
        enabled: true,
        max_vehicles_override: source.max_vehicles_override,
        min_speed_override: source.min_speed_override,
        max_speed_override: source.max_speed_override,
        allowed_model_paths: source.allowed_model_paths.clone(),
    })
}

/// Projects a local XZ point onto `lane` and returns `(stop_s_m, perp_distance_m)`.
pub fn project_point_onto_lane(
    lane: &TrafficLane,
    anchor: GeoAnchor,
    world_xz: (f64, f64),
) -> Option<(f64, f64)> {
    if lane.geom.len() < 2 {
        return None;
    }
    let pts: Vec<[f64; 3]> = lane
        .geom
        .iter()
        .map(|p| {
            let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
            [l[0] as f64, l[1] as f64, l[2] as f64]
        })
        .collect();
    let mut cum = 0.0;
    let mut best: Option<(f64, f64)> = None;
    for w in pts.windows(2) {
        let ax = w[0][0];
        let az = w[0][2];
        let bx = w[1][0];
        let bz = w[1][2];
        let dx = bx - ax;
        let dz = bz - az;
        let seg_len_sq = dx * dx + dz * dz;
        let seg_len = seg_len_sq.sqrt().max(0.01);
        let t = if seg_len_sq > 1e-8 {
            (((world_xz.0 - ax) * dx + (world_xz.1 - az) * dz) / seg_len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let px = ax + dx * t;
        let pz = az + dz * t;
        let dist = (world_xz.0 - px).hypot(world_xz.1 - pz);
        let s = cum + t * seg_len;
        if best.is_none_or(|b| dist < b.1) {
            best = Some((s, dist));
        }
        cum += seg_len;
    }
    let total = cum;
    best.map(|(s, d)| (s.clamp(2.0, (total - 2.0).max(2.0)), d))
}

/// Samples local `(pos, forward_xz, right_xz)` at distance `s_m` along `lane`.
pub fn sample_lane_local_frame(
    lane: &TrafficLane,
    anchor: GeoAnchor,
    s_m: f64,
) -> Option<([f64; 3], (f64, f64), (f64, f64))> {
    if lane.geom.len() < 2 {
        return None;
    }
    let pts: Vec<[f64; 3]> = lane
        .geom
        .iter()
        .map(|p| {
            let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
            [l[0] as f64, l[1] as f64, l[2] as f64]
        })
        .collect();
    let mut seg_lens = Vec::with_capacity(pts.len() - 1);
    let mut total = 0.0;
    for w in pts.windows(2) {
        let l = (w[1][0] - w[0][0]).hypot(w[1][2] - w[0][2]).max(0.01);
        seg_lens.push(l);
        total += l;
    }
    let target = s_m.clamp(0.0, total);
    let mut acc = 0.0;
    for (idx, &seg_len) in seg_lens.iter().enumerate() {
        if acc + seg_len >= target || idx + 1 == seg_lens.len() {
            let t = ((target - acc) / seg_len).clamp(0.0, 1.0);
            let a = pts[idx];
            let b = pts[idx + 1];
            let dx = b[0] - a[0];
            let dz = b[2] - a[2];
            let len = dx.hypot(dz).max(1e-4);
            let fwd = (dx / len, dz / len);
            let right = (-fwd.1, fwd.0);
            let pos = [
                a[0] + dx * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + dz * t,
            ];
            return Some((pos, fwd, right));
        }
        acc += seg_len;
    }
    None
}

/// Computes the default pole `[lng, lat, alt]` and `[rot_x, rot_y, rot_z]` (degrees) placed on the
/// outer road shoulder beside `stop_s_m`, facing directly toward oncoming vehicles on `lane`.
pub fn default_traffic_light_transform(
    lane: &TrafficLane,
    anchor: GeoAnchor,
    stop_s_m: f64,
    lane_width_m: f64,
    overtake_side: OvertakeSide,
) -> Option<([f64; 3], [f64; 3])> {
    let (stop_pos, fwd, right) = sample_lane_local_frame(lane, anchor, stop_s_m)?;
    // Place pole on the outer curb side (left side for Indonesia left-hand traffic where overtake is Right)
    let shoulder_sign = -overtake_side.lateral_sign();
    let shoulder_offset = (lane_width_m * 0.55 + 1.35).clamp(2.2, 6.0);
    let forward_lead = 1.25;
    let pole_x = stop_pos[0] + right.0 * shoulder_sign * shoulder_offset + fwd.0 * forward_lead;
    let pole_y = stop_pos[1];
    let pole_z = stop_pos[2] + right.1 * shoulder_sign * shoulder_offset + fwd.1 * forward_lead;
    let (lng, lat, alt) = anchor.local_to_lng_lat_alt([pole_x as f32, pole_y as f32, pole_z as f32]);
    // Local +Z of the procedural signal box holds the 3 glowing lenses -> face `-fwd` (incoming cars).
    let face_yaw_deg = (-fwd.0).atan2(-fwd.1).to_degrees();
    Some(([lng, lat, alt], [0.0, face_yaw_deg, 0.0]))
}

/// Builds a new `TrafficLightSignal` snapped to `lane` at `stop_s_m` with default pole placement.
pub fn build_traffic_light_on_lane(
    lane: &TrafficLane,
    anchor: GeoAnchor,
    stop_s_m: f64,
    phase_idx: usize,
    name: impl Into<String>,
    lane_width_m: f64,
    overtake_side: OvertakeSide,
) -> Option<TrafficLightSignal> {
    let total = lane.total_length_m();
    let clamped_s = stop_s_m.clamp(2.0, (total - 2.0).max(2.0));
    let (pos_lnglat, rot_deg) =
        default_traffic_light_transform(lane, anchor, clamped_s, lane_width_m, overtake_side)?;
    Some(TrafficLightSignal {
        id: Uuid::new_v4(),
        name: name.into(),
        lane_id: lane.id,
        stop_s_m: clamped_s,
        phase_idx,
        enabled: true,
        pos_lnglat,
        rot_deg,
        scale: [1.0, 1.0, 1.0],
    })
}

/// Automatically detects lane crossings (or falls back to mid-lane) and places traffic lights
/// ~10 m before each intersection, grouping parallel/opposite lanes into shared phases.
/// Returns the number of newly added traffic lights.
pub fn auto_place_intersection_traffic_lights(
    cfg: &mut TrafficAnimConfig,
    anchor: GeoAnchor,
) -> usize {
    let mut rng = SplitMix64::new(cfg.seed);
    let mut compiled = Vec::new();
    for (idx, lane) in cfg.lanes.iter().enumerate() {
        if let Some(c) = CompiledLane::from_lane(idx, lane, cfg, anchor, &mut rng) {
            compiled.push(c);
        }
    }
    if compiled.is_empty() {
        return 0;
    }

    let crossings = compute_lane_crossings(&compiled);
    let mut stop_per_compiled: Vec<Option<f64>> = vec![None; compiled.len()];
    for cr in &crossings {
        let sa = (cr.s_a - 10.0).clamp(4.0, (compiled[cr.lane_a].total_len - 4.0).max(4.0));
        let sb = (cr.s_b - 10.0).clamp(4.0, (compiled[cr.lane_b].total_len - 4.0).max(4.0));
        stop_per_compiled[cr.lane_a] = Some(match stop_per_compiled[cr.lane_a] {
            Some(prev) => prev.min(sa),
            None => sa,
        });
        stop_per_compiled[cr.lane_b] = Some(match stop_per_compiled[cr.lane_b] {
            Some(prev) => prev.min(sb),
            None => sb,
        });
    }

    // Group lanes by axis orientation (parallel/opposite lanes share a phase)
    let mut phase_axes: Vec<(f64, f64)> = Vec::new();
    let mut added = 0usize;

    for (ci, cl) in compiled.iter().enumerate() {
        let lane = &cfg.lanes[cl.source_idx];
        // Skip lanes that already have at least one light
        if cfg.traffic_lights.iter().any(|tl| tl.lane_id == lane.id) {
            continue;
        }
        let stop_s = stop_per_compiled[ci]
            .unwrap_or_else(|| (cl.total_len * 0.45).clamp(4.0, (cl.total_len - 4.0).max(4.0)));
        let smp = cl.sample(stop_s);
        let dir = (smp.fx, smp.fz);

        let phase_idx = if let Some(found) = phase_axes
            .iter()
            .position(|a| (a.0 * dir.0 + a.1 * dir.1).abs() > 0.78)
        {
            found
        } else {
            let new_idx = phase_axes.len();
            phase_axes.push(dir);
            new_idx
        };

        let light_num = cfg.traffic_lights.len() + 1;
        let name = format!("Lampu {} ({})", light_num, lane.name);
        if let Some(tl) = build_traffic_light_on_lane(
            lane,
            anchor,
            stop_s,
            phase_idx,
            name,
            cfg.lane_width_m,
            cfg.overtake_side,
        ) {
            cfg.traffic_lights.push(tl);
            added += 1;
        }
    }

    let needed_phases = cfg
        .traffic_lights
        .iter()
        .map(|l| l.phase_idx + 1)
        .max()
        .unwrap_or(0);
    while cfg.signal_phases.len() < needed_phases {
        let idx = cfg.signal_phases.len();
        cfg.signal_phases
            .push(TrafficSignalPhase::new(format!("Fase {}", idx + 1)));
    }
    added
}

/// Evaluates the deterministic signal state at `elapsed_sec`.
/// Returns `(active_phase_idx, active_phase_name, active_color, is_all_red, remaining_sec)`.
pub fn evaluate_signal_phase_at_time(
    cfg: &TrafficAnimConfig,
    elapsed_sec: f64,
) -> (Option<usize>, String, TrafficLightColor, bool, f64) {
    let phases = cfg.effective_signal_phases();
    if phases.is_empty() {
        return (None, String::new(), TrafficLightColor::Green, false, 0.0);
    }

    // Special case: a single configured phase still alternates Green -> Yellow -> Red
    // so single-lane traffic light tests cycle visibly!
    if phases.len() == 1 {
        let p = &phases[0];
        let red_hold = (p.all_red_sec + p.green_sec * 0.75).max(4.0);
        let total = p.green_sec + p.yellow_sec + red_hold;
        let t = (elapsed_sec + cfg.signal_cycle_offset_sec).rem_euclid(total);
        if t < p.green_sec {
            return (
                Some(0),
                p.name.clone(),
                TrafficLightColor::Green,
                false,
                (p.green_sec - t).max(0.0),
            );
        } else if t < p.green_sec + p.yellow_sec {
            return (
                Some(0),
                p.name.clone(),
                TrafficLightColor::Yellow,
                false,
                (p.green_sec + p.yellow_sec - t).max(0.0),
            );
        } else {
            return (
                None,
                p.name.clone(),
                TrafficLightColor::Red,
                true,
                (total - t).max(0.0),
            );
        }
    }

    let total_cycle: f64 = phases.iter().map(TrafficSignalPhase::span_sec).sum();
    if total_cycle <= 1e-4 {
        return (
            Some(0),
            phases[0].name.clone(),
            TrafficLightColor::Green,
            false,
            0.0,
        );
    }
    let t = (elapsed_sec + cfg.signal_cycle_offset_sec).rem_euclid(total_cycle);
    let mut acc = 0.0;
    for (idx, p) in phases.iter().enumerate() {
        let span = p.span_sec();
        if t < acc + span || idx + 1 == phases.len() {
            let local_t = (t - acc).max(0.0);
            if local_t < p.green_sec {
                return (
                    Some(idx),
                    p.name.clone(),
                    TrafficLightColor::Green,
                    false,
                    (p.green_sec - local_t).max(0.0),
                );
            } else if local_t < p.green_sec + p.yellow_sec {
                return (
                    Some(idx),
                    p.name.clone(),
                    TrafficLightColor::Yellow,
                    false,
                    (p.green_sec + p.yellow_sec - local_t).max(0.0),
                );
            } else {
                return (
                    None,
                    p.name.clone(),
                    TrafficLightColor::Red,
                    true,
                    (span - local_t).max(0.0),
                );
            }
        }
        acc += span;
    }
    (None, String::new(), TrafficLightColor::Red, true, 0.0)
}

/// Resolves world-space poses and lamp colors for every placed `TrafficLightSignal`
/// at `elapsed_sec` (usable in both Plot Editor static view at `t = 0.0` and live playback).
pub fn resolve_traffic_light_visual_poses(
    cfg: &TrafficAnimConfig,
    anchor: GeoAnchor,
    elapsed_sec: f64,
) -> Vec<TrafficLightVisualPose> {
    let (active_phase_idx, _, active_color, is_all_red, remaining_sec) =
        evaluate_signal_phase_at_time(cfg, elapsed_sec);
    let half_w = (cfg.lane_width_m * 0.52).clamp(1.2, 4.5);

    let mut out = Vec::with_capacity(cfg.traffic_lights.len());
    for tl in &cfg.traffic_lights {
        let pos = anchor.lng_lat_alt_to_local(
            tl.pos_lnglat[0],
            tl.pos_lnglat[1],
            tl.pos_lnglat[2],
        );
        let (stop_center, stop_line_a, stop_line_b) = cfg
            .lanes
            .iter()
            .find(|l| l.id == tl.lane_id)
            .and_then(|l| sample_lane_local_frame(l, anchor, tl.stop_s_m))
            .map(|(p, _fwd, right)| {
                let c = [p[0] as f32, p[1] as f32, p[2] as f32];
                let a = [
                    (p[0] - right.0 * half_w) as f32,
                    p[1] as f32,
                    (p[2] - right.1 * half_w) as f32,
                ];
                let b = [
                    (p[0] + right.0 * half_w) as f32,
                    p[1] as f32,
                    (p[2] + right.1 * half_w) as f32,
                ];
                (c, a, b)
            })
            .unwrap_or((pos, pos, pos));

        let color = if !tl.enabled {
            TrafficLightColor::Green
        } else if !is_all_red && active_phase_idx == Some(tl.phase_idx) {
            active_color
        } else {
            TrafficLightColor::Red
        };

        out.push(TrafficLightVisualPose {
            light_id: tl.id,
            name: tl.name.clone(),
            lane_id: tl.lane_id,
            phase_idx: tl.phase_idx,
            color,
            remaining_sec,
            pos,
            rot_deg: [tl.rot_deg[0] as f32, tl.rot_deg[1] as f32, tl.rot_deg[2] as f32],
            scale: [tl.scale[0] as f32, tl.scale[1] as f32, tl.scale[2] as f32],
            stop_line_a,
            stop_line_b,
            stop_center,
            enabled: tl.enabled,
        });
    }
    out
}

// =============================================================================
// Deterministic PRNG (SplitMix64) for identical Operator <-> Layar Besar sync
// =============================================================================

#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform float in `[0.0, 1.0)`.
    fn next_f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }

    fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        if max <= min {
            min
        } else {
            min + (max - min) * self.next_f64()
        }
    }
}

// =============================================================================
// Traffic Simulation Engine (Frenet 1D + Lateral Offset + Overtaking + Oncoming)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficBehaviorState {
    Cruising,
    Following,
    Overtaking,
    Returning,
    YieldingCrossing,
    WaitingSignal,
}

#[derive(Debug, Clone, Copy)]
struct LaneSample {
    x: f64,
    y: f64,
    z: f64,
    fx: f64,
    fz: f64,
    rx: f64,
    rz: f64,
}

#[derive(Debug, Clone)]
struct CompiledLane {
    source_idx: usize,
    pts: Vec<[f64; 3]>,
    seg_lens: Vec<f64>,
    seg_dirs: Vec<(f64, f64)>,
    cum_lens: Vec<f64>,
    total_len: f64,
    max_vehicles: usize,
    min_speed_mps: f64,
    max_speed_mps: f64,
    pool_indices: Vec<usize>,
    next_spawn_timer: f64,
}

impl CompiledLane {
    fn from_lane(
        source_idx: usize,
        lane: &TrafficLane,
        cfg: &TrafficAnimConfig,
        anchor: GeoAnchor,
        rng: &mut SplitMix64,
    ) -> Option<Self> {
        if !lane.enabled || lane.geom.len() < 2 {
            return None;
        }
        let mut pts: Vec<[f64; 3]> = Vec::with_capacity(lane.geom.len());
        for p in &lane.geom {
            let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
            pts.push([l[0] as f64, l[1] as f64, l[2] as f64]);
        }
        let mut seg_lens = Vec::with_capacity(pts.len() - 1);
        let mut seg_dirs = Vec::with_capacity(pts.len() - 1);
        let mut cum_lens = Vec::with_capacity(pts.len());
        cum_lens.push(0.0);
        let mut total_len = 0.0;
        let mut last_dir = (0.0, 1.0);
        for w in pts.windows(2) {
            let dx = w[1][0] - w[0][0];
            let dz = w[1][2] - w[0][2];
            let len = dx.hypot(dz);
            let dir = if len > 1e-4 {
                let d = (dx / len, dz / len);
                last_dir = d;
                d
            } else {
                last_dir
            };
            let clamped_len = len.max(0.01);
            seg_lens.push(clamped_len);
            seg_dirs.push(dir);
            total_len += clamped_len;
            cum_lens.push(total_len);
        }
        if total_len < 5.0 {
            return None;
        }

        let pool_indices: Vec<usize> = if lane.allowed_model_paths.is_empty() {
            (0..cfg.vehicle_pool.len()).collect()
        } else {
            let filtered: Vec<usize> = cfg
                .vehicle_pool
                .iter()
                .enumerate()
                .filter(|(_, v)| {
                    lane.allowed_model_paths
                        .iter()
                        .any(|p| p == &v.model_path)
                })
                .map(|(i, _)| i)
                .collect();
            if filtered.is_empty() {
                (0..cfg.vehicle_pool.len()).collect()
            } else {
                filtered
            }
        };
        if pool_indices.is_empty() {
            return None;
        }

        let min_spd = lane.min_speed_override.unwrap_or(cfg.min_speed_mps);
        let max_spd = lane
            .max_speed_override
            .unwrap_or(cfg.max_speed_mps)
            .max(min_spd);
        // Stagger initial spawn slightly per lane so traffic appears quickly after Play
        let initial_delay = rng.range_f64(0.0, 0.45);

        Some(Self {
            source_idx,
            pts,
            seg_lens,
            seg_dirs,
            cum_lens,
            total_len,
            max_vehicles: lane
                .max_vehicles_override
                .unwrap_or(cfg.max_vehicles_per_lane) as usize,
            min_speed_mps: min_spd,
            max_speed_mps: max_spd,
            pool_indices,
            next_spawn_timer: initial_delay,
        })
    }

    fn sample(&self, s: f64) -> LaneSample {
        let clamped_s = s.clamp(0.0, self.total_len);
        let mut seg_idx = match self
            .cum_lens
            .binary_search_by(|v| v.total_cmp(&clamped_s))
        {
            Ok(i) => i.min(self.seg_lens.len() - 1),
            Err(i) => i.saturating_sub(1).min(self.seg_lens.len() - 1),
        };
        if seg_idx >= self.seg_lens.len() {
            seg_idx = self.seg_lens.len() - 1;
        }
        let seg_start = self.cum_lens[seg_idx];
        let seg_len = self.seg_lens[seg_idx];
        let t = ((clamped_s - seg_start) / seg_len).clamp(0.0, 1.0);
        let p0 = self.pts[seg_idx];
        let p1 = self.pts[seg_idx + 1];
        let (fx, fz) = self.seg_dirs[seg_idx];
        // Right normal in Bevy ground frame (+X east, +Z south)
        let (rx, rz) = (-fz, fx);
        LaneSample {
            x: p0[0] + (p1[0] - p0[0]) * t,
            y: p0[1] + (p1[1] - p0[1]) * t,
            z: p0[2] + (p1[2] - p0[2]) * t,
            fx,
            fz,
            rx,
            rz,
        }
    }

    /// Returns absolute heading turn rate (radians) within a lookahead window to prevent
    /// overtaking across sharp hairpin bends.
    fn max_turn_angle_rad_between(&self, s0: f64, s1: f64) -> f64 {
        let a = self.sample(s0);
        let b = self.sample(s1);
        let dot = (a.fx * b.fx + a.fz * b.fz).clamp(-1.0, 1.0);
        dot.acos()
    }
}

#[derive(Debug, Clone, Copy)]
struct LaneCrossing {
    lane_a: usize,
    s_a: f64,
    lane_b: usize,
    s_b: f64,
}

fn segment_intersection_2d(
    a0: (f64, f64),
    a1: (f64, f64),
    b0: (f64, f64),
    b1: (f64, f64),
) -> Option<(f64, f64)> {
    let r = (a1.0 - a0.0, a1.1 - a0.1);
    let s = (b1.0 - b0.0, b1.1 - b0.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < 1e-6 {
        return None;
    }
    let qp = (b0.0 - a0.0, b0.1 - a0.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denom;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some((t, u))
    } else {
        None
    }
}

fn compute_lane_crossings(lanes: &[CompiledLane]) -> Vec<LaneCrossing> {
    let mut out = Vec::new();
    for i in 0..lanes.len() {
        for j in (i + 1)..lanes.len() {
            let la = &lanes[i];
            let lb = &lanes[j];
            for sa in 0..la.seg_lens.len() {
                let da = la.seg_dirs[sa];
                let a0 = (la.pts[sa][0], la.pts[sa][2]);
                let a1 = (la.pts[sa + 1][0], la.pts[sa + 1][2]);
                for sb in 0..lb.seg_lens.len() {
                    let db = lb.seg_dirs[sb];
                    // Ignore parallel / opposite overlapping lanes; detect actual crossings.
                    if (da.0 * db.0 + da.1 * db.1).abs() > 0.72 {
                        continue;
                    }
                    let b0 = (lb.pts[sb][0], lb.pts[sb][2]);
                    let b1 = (lb.pts[sb + 1][0], lb.pts[sb + 1][2]);
                    if let Some((t, u)) = segment_intersection_2d(a0, a1, b0, b1) {
                        out.push(LaneCrossing {
                            lane_a: i,
                            s_a: la.cum_lens[sa] + t * la.seg_lens[sa],
                            lane_b: j,
                            s_b: lb.cum_lens[sb] + u * lb.seg_lens[sb],
                        });
                    }
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
struct SimVehicle {
    id: u64,
    lane_idx: usize,
    pool_idx: usize,
    s: f64,
    lateral_offset: f64,
    lateral_vel: f64,
    speed: f64,
    desired_speed: f64,
    length_m: f64,
    state: TrafficBehaviorState,
    overtake_target_id: Option<u64>,
    overtake_timer: f64,
    being_overtaken: bool,
    // Cached world kinematics updated each step
    world_x: f64,
    world_y: f64,
    world_z: f64,
    dir_x: f64,
    dir_z: f64,
    rot_y_deg: f64,
}

/// Output pose of one active simulated traffic vehicle in Bevy local meters.
#[derive(Debug, Clone)]
pub struct TrafficVehiclePose {
    pub vehicle_id: u64,
    pub lane_source_idx: usize,
    pub model_path: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// glTF `+Z`-forward yaw in degrees around Bevy `+Y`.
    pub rot_y_deg: f32,
    pub scale: f32,
    /// Smooth fade-in/fade-out scale multiplier `[0..1]` near lane start/end.
    pub fade_scale: f32,
    pub speed_mps: f32,
    pub behavior: TrafficBehaviorState,
}

#[derive(Debug, Clone, Copy)]
struct CompiledSignal {
    lane_idx: usize,
    stop_s_m: f64,
    phase_idx: usize,
}

/// Deterministic multi-lane traffic simulation engine.
#[derive(Debug, Clone)]
pub struct TrafficSimEngine {
    cfg: TrafficAnimConfig,
    anchor: GeoAnchor,
    lanes: Vec<CompiledLane>,
    signals: Vec<CompiledSignal>,
    crossings: Vec<LaneCrossing>,
    vehicles: Vec<SimVehicle>,
    rng: SplitMix64,
    next_vehicle_id: u64,
    sim_time: f64,
}

impl TrafficSimEngine {
    pub fn new(config: &TrafficAnimConfig, anchor: GeoAnchor) -> Self {
        let cfg = config.sanitized();
        let mut rng = SplitMix64::new(cfg.seed);
        let mut lanes = Vec::new();
        for (idx, lane) in cfg.lanes.iter().enumerate() {
            if let Some(compiled) = CompiledLane::from_lane(idx, lane, &cfg, anchor, &mut rng) {
                lanes.push(compiled);
            }
        }
        let mut signals = Vec::new();
        for tl in &cfg.traffic_lights {
            if !tl.enabled {
                continue;
            }
            if let Some(ci) = lanes
                .iter()
                .position(|cl| cfg.lanes[cl.source_idx].id == tl.lane_id)
            {
                let clamped_s = tl
                    .stop_s_m
                    .clamp(2.0, (lanes[ci].total_len - 2.0).max(2.0));
                signals.push(CompiledSignal {
                    lane_idx: ci,
                    stop_s_m: clamped_s,
                    phase_idx: tl.phase_idx,
                });
            }
        }
        signals.sort_by(|a, b| {
            a.lane_idx
                .cmp(&b.lane_idx)
                .then_with(|| a.stop_s_m.total_cmp(&b.stop_s_m))
        });
        let crossings = if cfg.yield_at_intersections {
            compute_lane_crossings(&lanes)
        } else {
            Vec::new()
        };
        let mut engine = Self {
            cfg,
            anchor,
            lanes,
            signals,
            crossings,
            vehicles: Vec::new(),
            rng,
            next_vehicle_id: 1,
            sim_time: 0.0,
        };
        engine.seed_initial_traffic();
        engine
    }

    pub fn sim_time(&self) -> f64 {
        self.sim_time
    }

    pub fn reset(&mut self) {
        *self = Self::new(&self.cfg, self.anchor);
    }

    /// Pre-populates 1–2 well-spaced vehicles on each lane at `t = 0` so traffic looks alive
    /// immediately on Play without waiting several seconds for the first car to enter.
    fn seed_initial_traffic(&mut self) {
        for lane_idx in 0..self.lanes.len() {
            if self.vehicles.len() >= self.cfg.max_total_vehicles as usize {
                break;
            }
            let lane_len = self.lanes[lane_idx].total_len;
            let lane_max = self.lanes[lane_idx].max_vehicles;
            if lane_len < 25.0 || lane_max == 0 {
                continue;
            }
            // Spawn 1 starter vehicle near the start of each lane, plus a 2nd mid-lane if long enough.
            let initial_spots: &[f64] = if lane_len >= 95.0 && lane_max >= 3 {
                &[0.08, 0.42]
            } else {
                &[0.06]
            };
            for &frac in initial_spots {
                if self.vehicles.len() >= self.cfg.max_total_vehicles as usize {
                    break;
                }
                let jitter = self.rng.range_f64(-0.03, 0.04);
                let s = (lane_len * (frac + jitter)).clamp(2.0, (lane_len - 12.0).max(2.0));
                self.try_spawn_vehicle_on_lane(lane_idx, s);
            }
        }
    }

    /// Deterministically advances the simulation to `target_elapsed_sec` in fixed `TRAFFIC_SIM_FIXED_DT` steps.
    pub fn advance_to(&mut self, target_elapsed_sec: f64) {
        let target = target_elapsed_sec.max(0.0);
        if target + 1e-6 < self.sim_time {
            self.reset();
        }
        // Cap max catch-up steps per call to keep large timeline jumps responsive (max 60s of sim catch-up).
        let max_steps = 1_800;
        let mut steps = 0;
        while self.sim_time + TRAFFIC_SIM_FIXED_DT <= target + 1e-9 && steps < max_steps {
            self.step(TRAFFIC_SIM_FIXED_DT);
            steps += 1;
        }
    }

    fn pick_weighted_pool_index(&mut self, candidates: &[usize]) -> Option<usize> {
        let total_w: f64 = candidates
            .iter()
            .filter_map(|&idx| self.cfg.vehicle_pool.get(idx))
            .map(|v| (v.weight as f64).max(0.01))
            .sum();
        if total_w <= 0.0 {
            return None;
        }
        let mut pick = self.rng.next_f64() * total_w;
        for &idx in candidates {
            let w = self
                .cfg
                .vehicle_pool
                .get(idx)
                .map(|v| (v.weight as f64).max(0.01))
                .unwrap_or(0.0);
            pick -= w;
            if pick <= 0.0 {
                return Some(idx);
            }
        }
        candidates.last().copied()
    }

    fn try_spawn_vehicle_on_lane(&mut self, lane_idx: usize, start_s: f64) -> bool {
        if self.vehicles.len() >= self.cfg.max_total_vehicles as usize {
            return false;
        }
        let lane_max = self.lanes[lane_idx].max_vehicles;
        let current_on_lane = self
            .vehicles
            .iter()
            .filter(|v| v.lane_idx == lane_idx)
            .count();
        if current_on_lane >= lane_max {
            return false;
        }
        let candidates = self.lanes[lane_idx].pool_indices.clone();
        let Some(pool_idx) = self.pick_weighted_pool_index(&candidates) else {
            return false;
        };
        let entry = &self.cfg.vehicle_pool[pool_idx];
        let length_m = entry.length_m;
        let min_clear = length_m + self.cfg.min_safe_gap_m + 3.0;

        // Ensure start zone on this lane is clear so newly spawned vehicles never overlap
        let blocked = self.vehicles.iter().any(|v| {
            v.lane_idx == lane_idx && (v.s - start_s).abs() < (0.5 * (v.length_m + length_m) + min_clear * 0.65)
        });
        if blocked {
            return false;
        }

        let min_spd = self.lanes[lane_idx].min_speed_mps;
        let max_spd = self.lanes[lane_idx].max_speed_mps;
        let desired_speed = self.rng.range_f64(min_spd, max_spd);

        // Match front vehicle speed if spawning close behind a slower vehicle
        let init_speed = self
            .vehicles
            .iter()
            .filter(|v| v.lane_idx == lane_idx && v.s > start_s && (v.s - start_s) < 28.0)
            .map(|v| v.speed)
            .fold(desired_speed, f64::min);

        let sample = self.lanes[lane_idx].sample(start_s);
        let rot_y_deg = sample.fx.atan2(sample.fz).to_degrees() + entry.rot_y_offset_deg;
        let id = self.next_vehicle_id;
        self.next_vehicle_id += 1;

        self.vehicles.push(SimVehicle {
            id,
            lane_idx,
            pool_idx,
            s: start_s,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            speed: init_speed,
            desired_speed,
            length_m,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: sample.x,
            world_y: sample.y,
            world_z: sample.z,
            dir_x: sample.fx,
            dir_z: sample.fz,
            rot_y_deg,
        });
        true
    }

    /// Executes one deterministic fixed-dt simulation step.
    pub fn step(&mut self, dt: f64) {
        self.sim_time += dt;
        if self.lanes.is_empty() || self.cfg.vehicle_pool.is_empty() {
            self.vehicles.clear();
            return;
        }

        // 1. Spawn new vehicles on lanes whose timer elapsed
        for lane_idx in 0..self.lanes.len() {
            self.lanes[lane_idx].next_spawn_timer -= dt;
            if self.lanes[lane_idx].next_spawn_timer <= 0.0 {
                let spawned = self.try_spawn_vehicle_on_lane(lane_idx, 0.0);
                self.lanes[lane_idx].next_spawn_timer = if spawned {
                    self.rng
                        .range_f64(self.cfg.spawn_interval_min_sec, self.cfg.spawn_interval_max_sec)
                } else {
                    // Retry shortly once start zone clears
                    0.5
                };
            }
        }

        // 2. Refresh `being_overtaken` flag from active overtakers
        for v in &mut self.vehicles {
            v.being_overtaken = false;
        }
        let overtaken_ids: Vec<u64> = self
            .vehicles
            .iter()
            .filter(|v| v.state == TrafficBehaviorState::Overtaking)
            .filter_map(|v| v.overtake_target_id)
            .collect();
        for v in &mut self.vehicles {
            if overtaken_ids.contains(&v.id) {
                v.being_overtaken = true;
            }
        }

        // Snapshot current vehicle state for clean simultaneous decision-making
        let snap = self.vehicles.clone();
        let lane_width = self.cfg.lane_width_m;
        let overtake_offset = lane_width * self.cfg.overtake_side.lateral_sign();
        let min_gap = self.cfg.min_safe_gap_m;
        let yield_mult = (1.0 - self.cfg.front_yield_slowdown_pct / 100.0).clamp(0.4, 1.0);
        let (active_signal_phase, _, active_signal_color, signal_all_red, _) =
            evaluate_signal_phase_at_time(&self.cfg, self.sim_time);

        for i in 0..self.vehicles.len() {
            let cur = &snap[i];
            let lane = &self.lanes[cur.lane_idx];
            let sample = lane.sample(cur.s);

            // Evaluate upcoming traffic lights on this lane (if any)
            let cur_front_s = cur.s + 0.5 * cur.length_m;
            let mut active_red_stop_front_s: Option<f64> = None;
            let mut near_signal_no_overtake = false;
            let mut signal_forces_return = false;
            for sig in &self.signals {
                if sig.lane_idx != cur.lane_idx {
                    continue;
                }
                let stop_front_s = (sig.stop_s_m - 0.6).max(1.5);
                let dist_to_stop = stop_front_s - cur_front_s;
                let sig_color = if !signal_all_red && active_signal_phase == Some(sig.phase_idx) {
                    active_signal_color
                } else {
                    TrafficLightColor::Red
                };
                // Configurable No-Overtake Zone near traffic light stop line (0 m disables completely)
                let zone_m = self.cfg.signal_no_overtake_zone_m.clamp(0.0, 1000.0);
                if zone_m > 0.01 {
                    let no_ot_ahead = match sig_color {
                        TrafficLightColor::Green => (zone_m * 0.55).max(8.0).min(zone_m),
                        TrafficLightColor::Yellow | TrafficLightColor::Red => zone_m,
                    };
                    let no_ot_past = 10.0_f64.min(zone_m * 0.5);
                    if dist_to_stop > -no_ot_past && dist_to_stop < no_ot_ahead {
                        near_signal_no_overtake = true;
                    }
                }
                let must_stop = match sig_color {
                    TrafficLightColor::Green => false,
                    TrafficLightColor::Red => dist_to_stop > -0.35,
                    TrafficLightColor::Yellow => {
                        let safe_stop_dist = (cur.speed * cur.speed) / (2.0 * 4.8) + 1.5;
                        dist_to_stop > safe_stop_dist && dist_to_stop < 45.0
                    }
                };
                if must_stop {
                    if zone_m > 0.01 {
                        let force_ret_dist = (zone_m * 0.75).max(12.0).min(zone_m);
                        if dist_to_stop < force_ret_dist {
                            signal_forces_return = true;
                        }
                    }
                    active_red_stop_front_s = Some(match active_red_stop_front_s {
                        Some(prev) => prev.min(stop_front_s),
                        None => stop_front_s,
                    });
                }
            }

            // Find closest vehicle ahead on the SAME lane occupying the main lane (or same lateral band)
            let mut front_in_lane: Option<&SimVehicle> = None;
            let mut front_gap = f64::INFINITY;
            // Also track closest vehicle ahead in the overtake corridor on the same lane
            let mut front_in_overtake_band: Option<&SimVehicle> = None;
            let mut overtake_band_gap = f64::INFINITY;

            for other in &snap {
                if other.id == cur.id || other.lane_idx != cur.lane_idx || other.s <= cur.s {
                    continue;
                }
                let bumper_gap = (other.s - cur.s) - 0.5 * (other.length_m + cur.length_m);
                let lat_diff = (other.lateral_offset - cur.lateral_offset).abs();
                if other.lateral_offset.abs() < lane_width * 0.55 && bumper_gap < front_gap {
                    front_gap = bumper_gap;
                    front_in_lane = Some(other);
                }
                if lat_diff < lane_width * 0.65 && bumper_gap < overtake_band_gap {
                    overtake_band_gap = bumper_gap;
                    front_in_overtake_band = Some(other);
                }
            }

            let mut next_state = cur.state;
            let mut next_target_id = cur.overtake_target_id;
            let mut next_overtake_timer = cur.overtake_timer;
            let mut desired_lat: f64;
            let mut target_speed = cur.desired_speed;

            // Front vehicle temporarily slows slightly while another vehicle is overtaking it
            if cur.being_overtaken && cur.state != TrafficBehaviorState::Overtaking {
                target_speed = (target_speed * yield_mult).max(2.0);
            }

            match cur.state {
                TrafficBehaviorState::Cruising
                | TrafficBehaviorState::Following
                | TrafficBehaviorState::YieldingCrossing
                | TrafficBehaviorState::WaitingSignal => {
                    desired_lat = 0.0;
                    next_target_id = None;
                    next_overtake_timer = 0.0;

                    if let Some(front) = front_in_lane {
                        let follow_trigger = (min_gap + cur.speed * 1.45).max(min_gap + 6.0);
                        if front_gap < follow_trigger {
                            next_state = TrafficBehaviorState::Following;
                            // Evaluate whether overtaking is desirable & safe (blocked in signal no-overtake zones!)
                            let wants_overtake = self.cfg.allow_overtaking
                                && !near_signal_no_overtake
                                && !cur.being_overtaken
                                && cur.desired_speed >= front.speed + 1.4
                                && front.state != TrafficBehaviorState::Overtaking
                                && front.state != TrafficBehaviorState::Returning
                                && front.state != TrafficBehaviorState::WaitingSignal
                                && front.speed > 1.5
                                && front_gap < follow_trigger * 0.95
                                && front_gap > 1.0;

                            if wants_overtake {
                                let pass_speed = (cur.desired_speed * 1.12)
                                    .min(lane.max_speed_mps * 1.25)
                                    .max(front.speed + 2.5);
                                let front_yield_spd = (front.speed * yield_mult).max(1.8);
                                let rel_speed = (pass_speed - front_yield_spd).max(1.8);
                                let pass_dist = (front.s - cur.s)
                                    + 0.5 * (front.length_m + cur.length_m)
                                    + min_gap * 1.2;
                                let est_pass_time = (pass_dist / rel_speed) + 1.8;
                                let est_world_travel = pass_speed * est_pass_time;

                                let remaining_lane = lane.total_len - cur.s;
                                let bend_ok = lane.max_turn_angle_rad_between(
                                    cur.s,
                                    (cur.s + est_world_travel.min(45.0)).min(lane.total_len),
                                ) < 0.75;

                                // Check space ahead of `front` to safely merge back in
                                let merge_space_ok = !snap.iter().any(|other| {
                                    other.id != cur.id
                                        && other.id != front.id
                                        && other.lane_idx == cur.lane_idx
                                        && other.s > front.s
                                        && (other.s - front.s)
                                            - 0.5 * (other.length_m + front.length_m)
                                            < cur.length_m + min_gap * 1.4
                                });

                                // Check oncoming traffic (e.g. West <-> East) and anyone occupying the overtake corridor!
                                let corridor_clear = Self::is_overtake_corridor_safe(
                                    cur,
                                    front.id,
                                    &sample,
                                    &snap,
                                    overtake_offset,
                                    lane_width,
                                    pass_speed,
                                    est_pass_time,
                                    est_world_travel,
                                );

                                if remaining_lane > est_world_travel + 12.0
                                    && bend_ok
                                    && merge_space_ok
                                    && corridor_clear
                                {
                                    next_state = TrafficBehaviorState::Overtaking;
                                    next_target_id = Some(front.id);
                                    next_overtake_timer = 0.0;
                                    desired_lat = overtake_offset;
                                    target_speed = pass_speed;
                                }
                            }
                        } else {
                            next_state = TrafficBehaviorState::Cruising;
                        }
                    } else {
                        next_state = TrafficBehaviorState::Cruising;
                    }
                }
                TrafficBehaviorState::Overtaking => {
                    next_overtake_timer += dt;
                    desired_lat = overtake_offset;
                    target_speed = (cur.desired_speed * 1.12).min(lane.max_speed_mps * 1.25);

                    let target_opt = next_target_id.and_then(|tid| snap.iter().find(|v| v.id == tid));
                    let remaining_lane = lane.total_len - cur.s;

                    // Emergency abort check if an oncoming vehicle is dangerously close before we passed front
                    let oncoming_danger = Self::has_imminent_oncoming_threat(
                        cur,
                        &sample,
                        &snap,
                        overtake_offset,
                        lane_width,
                    );

                    match target_opt {
                        Some(target) => {
                            let cleared_front = cur.s
                                > target.s
                                    + 0.5 * (target.length_m + cur.length_m)
                                    + min_gap * 0.85;
                            let not_yet_alongside = cur.s < target.s - 0.35 * (target.length_m + cur.length_m);
                            if cleared_front
                                || remaining_lane < 14.0
                                || next_overtake_timer > 11.0
                                || (oncoming_danger && not_yet_alongside)
                                || (signal_forces_return && not_yet_alongside)
                            {
                                next_state = TrafficBehaviorState::Returning;
                                next_target_id = None;
                                desired_lat = 0.0;
                            }
                        }
                        None => {
                            next_state = TrafficBehaviorState::Returning;
                            next_target_id = None;
                            desired_lat = 0.0;
                        }
                    }
                }
                TrafficBehaviorState::Returning => {
                    next_target_id = None;
                    // Never slide sideways into a vehicle occupying the main lane beside us!
                    let side_occupied = snap.iter().any(|other| {
                        other.id != cur.id
                            && other.lane_idx == cur.lane_idx
                            && other.lateral_offset.abs() < lane_width * 0.55
                            && (other.s - cur.s).abs()
                                < 0.5 * (other.length_m + cur.length_m) + 1.2
                    });
                    if side_occupied && cur.lateral_offset.abs() > lane_width * 0.35 {
                        desired_lat = cur.lateral_offset;
                    } else {
                        desired_lat = 0.0;
                        if cur.lateral_offset.abs() < 0.12 {
                            next_state = TrafficBehaviorState::Cruising;
                        }
                    }
                }
            }

            // 3. Car-following anti-collision braking (ALWAYS enforced for whoever is directly in our band!)
            let active_blocker = if matches!(
                next_state,
                TrafficBehaviorState::Overtaking
            ) || cur.lateral_offset.abs() > lane_width * 0.45
            {
                front_in_overtake_band.map(|f| (f, overtake_band_gap))
            } else {
                front_in_lane.map(|f| (f, front_gap))
            };

            if let Some((blocker, gap)) = active_blocker {
                let desired_gap = min_gap + cur.speed * 0.85;
                if gap <= min_gap * 0.55 {
                    target_speed = 0.0;
                } else if gap < desired_gap * 1.65 {
                    let ratio = ((gap - min_gap * 0.55) / (desired_gap * 1.65 - min_gap * 0.55))
                        .clamp(0.0, 1.0);
                    let safe_spd = blocker.speed * 0.95 + (target_speed - blocker.speed * 0.95) * ratio;
                    target_speed = target_speed.min(safe_spd.max(0.0));
                }
            }

            // 3b. Traffic Light (APILL) stop-line braking & queue holding
            if let Some(stop_front_s) = active_red_stop_front_s {
                let dist_to_stop = stop_front_s - cur_front_s;
                let brake_horizon = ((cur.speed * cur.speed) / (2.0 * 3.6) + 6.0).max(26.0);
                if dist_to_stop < brake_horizon {
                    if dist_to_stop <= 0.45 {
                        target_speed = 0.0;
                    } else {
                        let v_stop = (2.0 * 3.4 * dist_to_stop).sqrt();
                        target_speed = target_speed.min(v_stop);
                    }
                    if !matches!(next_state, TrafficBehaviorState::Overtaking) {
                        next_state = TrafficBehaviorState::WaitingSignal;
                    }
                }
            }

            // 4. Intersection crossing yield (when two lanes cross each other)
            if self.cfg.yield_at_intersections
                && next_state != TrafficBehaviorState::Overtaking
            {
                for cross in &self.crossings {
                    let (my_cross_s, other_lane, other_cross_s) = if cross.lane_a == cur.lane_idx {
                        (cross.s_a, cross.lane_b, cross.s_b)
                    } else if cross.lane_b == cur.lane_idx {
                        (cross.s_b, cross.lane_a, cross.s_a)
                    } else {
                        continue;
                    };

                    // Check whether this intersection crossing is governed by Traffic Lights (APILL).
                    let crossing_signalized = self.signals.iter().any(|sig| {
                        (sig.lane_idx == cross.lane_a
                            && sig.stop_s_m <= cross.s_a + 1.5
                            && cross.s_a - sig.stop_s_m < 35.0)
                            || (sig.lane_idx == cross.lane_b
                                && sig.stop_s_m <= cross.s_b + 1.5
                                && cross.s_b - sig.stop_s_m < 35.0)
                    });

                    let dist_to_cross = my_cross_s - cur.s - cur.length_m * 0.5;
                    if dist_to_cross > 1.5 && dist_to_cross < 18.0 {
                        let cross_occupied = snap.iter().any(|other| {
                            if other.lane_idx != other_lane {
                                return false;
                            }
                            let other_front_s = other.s + 0.5 * other.length_m;
                            let other_rear_s = other.s - 0.5 * other.length_m;
                            let other_front_dist = other_cross_s - other_front_s;

                            if crossing_signalized {
                                // At a signalized intersection, NEVER yield to a vehicle waiting
                                // behind its red stop line! Only yield if `other` is ALREADY
                                // physically inside the crossing conflict box and actively clearing.
                                let inside_conflict_box = other_front_s > other_cross_s - 2.0
                                    && other_rear_s < other_cross_s + 3.0;
                                inside_conflict_box && other.speed > 0.4
                            } else {
                                // Un-signalized crossing: ignore vehicles already halted well before crossing
                                if other.speed < 0.3 && other_front_dist > 2.5 {
                                    return false;
                                }
                                let other_dist = other_cross_s - other.s;
                                (other_dist > -9.0 && other_dist < 10.0)
                                    || (other_dist > 0.0 && other_dist + 2.0 < dist_to_cross)
                            }
                        });
                        if cross_occupied {
                            let ratio = ((dist_to_cross - 3.0) / 15.0).clamp(0.0, 1.0);
                            target_speed = target_speed.min(cur.desired_speed * ratio * 0.45);
                            if dist_to_cross < 4.5 {
                                target_speed = 0.0;
                            }
                            next_state = TrafficBehaviorState::YieldingCrossing;
                        }
                    }
                }
            }

            // 5. Integrate longitudinal speed & position smoothly
            let max_accel = 3.2; // m/s^2
            let max_decel = 7.5; // m/s^2
            let new_speed = if target_speed > cur.speed {
                (cur.speed + max_accel * dt).min(target_speed)
            } else {
                (cur.speed - max_decel * dt).max(target_speed).max(0.0)
            };

            let mut new_s = cur.s + new_speed * dt;
            // Hard anti-clip clamp against immediate blocker in the same lateral band
            if let Some((blocker, _)) = active_blocker {
                let max_s = blocker.s - 0.5 * (blocker.length_m + cur.length_m) - 0.8;
                if new_s > max_s {
                    new_s = max_s.max(cur.s);
                }
            }
            // Hard stop-line clamp when held by an active Red/Yellow traffic signal
            if let Some(stop_front_s) = active_red_stop_front_s {
                let max_center_s = stop_front_s - 0.5 * cur.length_m;
                if cur.s <= max_center_s + 0.15 && new_s > max_center_s {
                    new_s = max_center_s.max(cur.s);
                }
            }

            // 6. Integrate lateral lane-change offset smoothly
            let lat_err = desired_lat - cur.lateral_offset;
            let max_lat_speed = (lane_width * 0.85).max(2.2);
            let desired_lat_vel = (lat_err * 2.8).clamp(-max_lat_speed, max_lat_speed);
            let new_lat_vel = cur.lateral_vel + (desired_lat_vel - cur.lateral_vel) * (dt * 6.5).min(1.0);
            let mut new_lat = cur.lateral_offset + new_lat_vel * dt;
            if desired_lat.abs() < 1e-4 && new_lat.abs() < 0.03 {
                new_lat = 0.0;
            }

            // 7. Recompute world pose & smooth steered heading
            let new_sample = lane.sample(new_s);
            let wx = new_sample.x + new_sample.rx * new_lat;
            let wy = new_sample.y;
            let wz = new_sample.z + new_sample.rz * new_lat;

            let move_spd = new_speed.max(0.8);
            let vx = new_sample.fx * move_spd + new_sample.rx * new_lat_vel;
            let vz = new_sample.fz * move_spd + new_sample.rz * new_lat_vel;
            let vlen = vx.hypot(vz).max(1e-4);
            let (dx, dz) = (vx / vlen, vz / vlen);
            let entry_rot = self
                .cfg
                .vehicle_pool
                .get(cur.pool_idx)
                .map(|e| e.rot_y_offset_deg)
                .unwrap_or(0.0);
            let rot_y_deg = dx.atan2(dz).to_degrees() + entry_rot;

            let veh = &mut self.vehicles[i];
            veh.s = new_s;
            veh.speed = new_speed;
            veh.lateral_offset = new_lat;
            veh.lateral_vel = new_lat_vel;
            veh.state = next_state;
            veh.overtake_target_id = next_target_id;
            veh.overtake_timer = next_overtake_timer;
            veh.world_x = wx;
            veh.world_y = wy;
            veh.world_z = wz;
            veh.dir_x = dx;
            veh.dir_z = dz;
            veh.rot_y_deg = rot_y_deg;
        }

        // 8. Despawn (recycle to pool) vehicles that reached the end of their lane
        self.vehicles.retain(|v| {
            let len = self.lanes[v.lane_idx].total_len;
            v.s < len
        });
    }

    /// Checks whether the overtake corridor is clear of oncoming (West <-> East) and adjacent traffic.
    #[allow(clippy::too_many_arguments)]
    fn is_overtake_corridor_safe(
        cur: &SimVehicle,
        target_front_id: u64,
        sample: &LaneSample,
        all: &[SimVehicle],
        overtake_offset: f64,
        lane_width: f64,
        pass_speed: f64,
        est_pass_time: f64,
        est_world_travel: f64,
    ) -> bool {
        let corridor_half_w = (lane_width * 0.85).max(2.4);
        for other in all {
            if other.id == cur.id || other.id == target_front_id {
                continue;
            }
            let rel_x = other.world_x - cur.world_x;
            let rel_z = other.world_z - cur.world_z;
            let rel_fwd = rel_x * sample.fx + rel_z * sample.fz;
            let rel_lat = rel_x * sample.rx + rel_z * sample.rz;

            // Only care about vehicles inside or bordering the overtake corridor
            if (rel_lat - overtake_offset).abs() > corridor_half_w {
                continue;
            }

            let dir_dot = sample.fx * other.dir_x + sample.fz * other.dir_z;
            if dir_dot < -0.35 {
                // Oncoming vehicle (e.g., East -> West while we go West -> East)!
                // Required clearance = distance we travel + distance oncoming travels during pass + safety buffer.
                let closing_dist = (pass_speed + other.speed.max(5.0)) * (est_pass_time + 2.0) + 16.0;
                if rel_fwd > -(cur.length_m + 3.0) && rel_fwd < closing_dist {
                    return false;
                }
            } else {
                // Same-direction vehicle already using the overtake corridor
                if rel_fwd > -(other.length_m + 8.0) && rel_fwd < est_world_travel + 12.0 {
                    return false;
                }
            }
        }
        true
    }

    fn has_imminent_oncoming_threat(
        cur: &SimVehicle,
        sample: &LaneSample,
        all: &[SimVehicle],
        overtake_offset: f64,
        lane_width: f64,
    ) -> bool {
        let corridor_half_w = (lane_width * 0.85).max(2.4);
        for other in all {
            if other.id == cur.id {
                continue;
            }
            let rel_x = other.world_x - cur.world_x;
            let rel_z = other.world_z - cur.world_z;
            let rel_fwd = rel_x * sample.fx + rel_z * sample.fz;
            let rel_lat = rel_x * sample.rx + rel_z * sample.rz;
            if (rel_lat - overtake_offset).abs() > corridor_half_w {
                continue;
            }
            let dir_dot = sample.fx * other.dir_x + sample.fz * other.dir_z;
            if dir_dot < -0.35 {
                let danger_dist = (cur.speed + other.speed.max(5.0)) * 2.4 + 10.0;
                if rel_fwd > 0.0 && rel_fwd < danger_dist {
                    return true;
                }
            }
        }
        false
    }

    /// Returns live signal phase telemetry + per-light visual states for `traffic_signal` UI & 3D rendering.
    pub fn live_signal_status(&self) -> TrafficSignalLiveStatus {
        let (active_phase_idx, active_phase_name, active_color, is_all_red, remaining_sec) =
            evaluate_signal_phase_at_time(&self.cfg, self.sim_time);
        let light_states =
            resolve_traffic_light_visual_poses(&self.cfg, self.anchor, self.sim_time);
        let stopped_vehicles = self
            .vehicles
            .iter()
            .filter(|v| v.speed < 0.6 || v.state == TrafficBehaviorState::WaitingSignal)
            .count();
        TrafficSignalLiveStatus {
            active_phase_idx,
            active_phase_name,
            active_color,
            is_all_red,
            remaining_sec,
            total_vehicles: self.vehicles.len(),
            stopped_vehicles,
            light_states,
        }
    }

    /// Returns world-space poses for all currently active vehicles (for rendering via pooled entities).
    pub fn active_poses(&self) -> Vec<TrafficVehiclePose> {
        self.vehicles
            .iter()
            .filter_map(|v| {
                let entry = self.cfg.vehicle_pool.get(v.pool_idx)?;
                let lane = &self.lanes[v.lane_idx];
                let fade_dist = 4.5_f64.min(lane.total_len * 0.15).max(1.0);
                let fade_in = (v.s / fade_dist).clamp(0.0, 1.0);
                let fade_out = ((lane.total_len - v.s) / fade_dist).clamp(0.0, 1.0);
                let fade_scale = fade_in.min(fade_out).max(0.05) as f32;
                Some(TrafficVehiclePose {
                    vehicle_id: v.id,
                    lane_source_idx: lane.source_idx,
                    model_path: entry.model_path.clone(),
                    x: v.world_x as f32,
                    y: v.world_y as f32,
                    z: v.world_z as f32,
                    rot_y_deg: v.rot_y_deg as f32,
                    scale: entry.scale as f32,
                    fade_scale,
                    speed_mps: v.speed as f32,
                    behavior: v.state,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_anchor() -> GeoAnchor {
        GeoAnchor::new(115.2167, -8.6705)
    }

    fn straight_lane_local(
        anchor: GeoAnchor,
        name: &str,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
    ) -> TrafficLane {
        let (lng0, lat0, alt0) = anchor.local_to_lng_lat_alt([x0, 0.0, z0]);
        let (lng1, lat1, alt1) = anchor.local_to_lng_lat_alt([x1, 0.0, z1]);
        TrafficLane::new(name, vec![[lng0, lat0, alt0], [lng1, lat1, alt1]])
    }

    fn sample_pool() -> Vec<TrafficVehicleEntry> {
        vec![
            TrafficVehicleEntry {
                model_path: "models/sedan.glb".into(),
                name: "Sedan".into(),
                scale: 1.0,
                length_m: 4.5,
                weight: 1.0,
                rot_y_offset_deg: 0.0,
            },
            TrafficVehicleEntry {
                model_path: "models/truck.glb".into(),
                name: "Truk".into(),
                scale: 1.0,
                length_m: 8.0,
                weight: 0.5,
                rot_y_offset_deg: 0.0,
            },
        ]
    }

    #[test]
    fn traffic_engine_is_fully_deterministic_for_layar_besar_sync() {
        let anchor = test_anchor();
        let lane_east = straight_lane_local(anchor, "Barat -> Timur", -120.0, 0.0, 120.0, 0.0);
        let lane_west = build_reverse_lane(&lane_east, 3.6, OvertakeSide::Right, "Timur -> Barat")
            .expect("reverse lane");
        let cfg = TrafficAnimConfig {
            lanes: vec![lane_east, lane_west],
            vehicle_pool: sample_pool(),
            seed: 999_123,
            ..Default::default()
        };

        let mut sim_a = TrafficSimEngine::new(&cfg, anchor);
        let mut sim_b = TrafficSimEngine::new(&cfg, anchor);
        sim_a.advance_to(12.5);
        sim_b.advance_to(12.5);

        let poses_a = sim_a.active_poses();
        let poses_b = sim_b.active_poses();
        assert!(!poses_a.is_empty());
        assert_eq!(poses_a.len(), poses_b.len());
        for (a, b) in poses_a.iter().zip(poses_b.iter()) {
            assert_eq!(a.vehicle_id, b.vehicle_id);
            assert_eq!(a.model_path, b.model_path);
            assert!((a.x - b.x).abs() < 1e-5);
            assert!((a.z - b.z).abs() < 1e-5);
            assert!((a.rot_y_deg - b.rot_y_deg).abs() < 1e-4);
        }
    }

    #[test]
    fn reverse_lane_offsets_to_right_side_for_indonesian_left_hand_traffic() {
        let anchor = test_anchor();
        let lane_east = straight_lane_local(anchor, "Barat -> Timur", -100.0, 0.0, 100.0, 0.0);
        let rev = build_reverse_lane(&lane_east, 4.0, OvertakeSide::Right, "Timur -> Barat")
            .expect("reverse lane");
        let p_start = anchor.lng_lat_alt_to_local(rev.geom[0][0], rev.geom[0][1], rev.geom[0][2]);
        let p_end = anchor.lng_lat_alt_to_local(rev.geom[1][0], rev.geom[1][1], rev.geom[1][2]);
        // Moving West -> East (+X): driver's right is South (+Z). Reverse lane starts at +100, ends at -100.
        assert!(p_start[0] > 95.0 && p_end[0] < -95.0);
        assert!((p_start[2] - 4.0).abs() < 0.2);
        assert!((p_end[2] - 4.0).abs() < 0.2);
    }

    #[test]
    fn faster_rear_vehicle_overtakes_and_front_vehicle_slows_down_when_clear() {
        let anchor = test_anchor();
        let lane_east = straight_lane_local(anchor, "Barat -> Timur", 0.0, 0.0, 300.0, 0.0);
        let cfg = TrafficAnimConfig {
            lanes: vec![lane_east],
            vehicle_pool: sample_pool(),
            max_vehicles_per_lane: 2,
            max_total_vehicles: 2,
            allow_overtaking: true,
            front_yield_slowdown_pct: 25.0,
            lane_width_m: 3.6,
            ..Default::default()
        };
        let mut engine = TrafficSimEngine::new(&cfg, anchor);
        engine.vehicles.clear();

        // Manually place a slow vehicle ahead and a fast vehicle behind it
        let s_slow = 45.0;
        let s_fast = 22.0;
        let sample_slow = engine.lanes[0].sample(s_slow);
        let sample_fast = engine.lanes[0].sample(s_fast);
        engine.vehicles.push(SimVehicle {
            id: 1,
            lane_idx: 0,
            pool_idx: 1,
            s: s_slow,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            speed: 7.0,
            desired_speed: 7.0,
            length_m: 8.0,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: sample_slow.x,
            world_y: 0.0,
            world_z: sample_slow.z,
            dir_x: sample_slow.fx,
            dir_z: sample_slow.fz,
            rot_y_deg: 90.0,
        });
        engine.vehicles.push(SimVehicle {
            id: 2,
            lane_idx: 0,
            pool_idx: 0,
            s: s_fast,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            speed: 14.0,
            desired_speed: 14.0,
            length_m: 4.5,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: sample_fast.x,
            world_y: 0.0,
            world_z: sample_fast.z,
            dir_x: sample_fast.fx,
            dir_z: sample_fast.fz,
            rot_y_deg: 90.0,
        });

        let mut saw_overtake = false;
        let mut saw_front_slowdown = false;
        for _ in 0..360 {
            engine.step(TRAFFIC_SIM_FIXED_DT);
            if let Some(fast) = engine.vehicles.iter().find(|v| v.id == 2)
                && fast.state == TrafficBehaviorState::Overtaking
            {
                saw_overtake = true;
            }
            if let Some(slow) = engine.vehicles.iter().find(|v| v.id == 1)
                && slow.being_overtaken
                && slow.speed < 6.6
            {
                saw_front_slowdown = true;
            }
        }
        assert!(saw_overtake, "fast vehicle should initiate overtaking");
        assert!(
            saw_front_slowdown,
            "slower front vehicle should temporarily reduce speed while being overtaken"
        );
        let slow = engine.vehicles.iter().find(|v| v.id == 1).unwrap();
        let fast = engine.vehicles.iter().find(|v| v.id == 2).unwrap();
        assert!(
            fast.s > slow.s + 6.0,
            "fast vehicle should have cleanly passed the slower vehicle"
        );
        assert!(
            fast.lateral_offset.abs() < 0.3,
            "fast vehicle should return to its own lane after overtaking"
        );
    }

    #[test]
    fn overtaking_waits_for_oncoming_west_east_vehicle_then_passes_when_clear() {
        let anchor = test_anchor();
        let lane_east = straight_lane_local(anchor, "Barat -> Timur", 0.0, 0.0, 350.0, 0.0);
        let lane_west = build_reverse_lane(&lane_east, 3.6, OvertakeSide::Right, "Timur -> Barat")
            .expect("reverse lane");
        let cfg = TrafficAnimConfig {
            lanes: vec![lane_east, lane_west],
            vehicle_pool: sample_pool(),
            max_vehicles_per_lane: 3,
            max_total_vehicles: 6,
            allow_overtaking: true,
            lane_width_m: 3.6,
            ..Default::default()
        };
        let mut engine = TrafficSimEngine::new(&cfg, anchor);
        engine.vehicles.clear();
        // Prevent random extra spawns during this deterministic test
        for l in &mut engine.lanes {
            l.next_spawn_timer = 999.0;
        }

        let place = |engine: &mut TrafficSimEngine, id: u64, lane_idx: usize, s: f64, spd: f64| {
            let smp = engine.lanes[lane_idx].sample(s);
            engine.vehicles.push(SimVehicle {
                id,
                lane_idx,
                pool_idx: 0,
                s,
                lateral_offset: 0.0,
                lateral_vel: 0.0,
                speed: spd,
                desired_speed: spd,
                length_m: 4.5,
                state: TrafficBehaviorState::Cruising,
                overtake_target_id: None,
                overtake_timer: 0.0,
                being_overtaken: false,
                world_x: smp.x,
                world_y: 0.0,
                world_z: smp.z,
                dir_x: smp.fx,
                dir_z: smp.fz,
                rot_y_deg: smp.fx.atan2(smp.fz).to_degrees(),
            });
        };

        // Slow front car (id=1) at x=50, fast rear car (id=2) at x=34, oncoming car (id=3) on reverse lane approaching at x=120 (s=230)
        place(&mut engine, 1, 0, 50.0, 7.0);
        place(&mut engine, 2, 0, 34.0, 14.0);
        place(&mut engine, 3, 1, 230.0, 14.0);

        let mut slowed_while_oncoming = false;
        let mut overtook_after_clear = false;

        for _ in 0..420 {
            engine.step(TRAFFIC_SIM_FIXED_DT);
            let fast = engine.vehicles.iter().find(|v| v.id == 2).unwrap();
            let slow = engine.vehicles.iter().find(|v| v.id == 1).unwrap();
            let oncoming = engine.vehicles.iter().find(|v| v.id == 3);

            // Never rear-end clip through slow vehicle!
            if fast.lateral_offset.abs() < 1.5 {
                assert!(
                    (fast.s - slow.s).abs() > 4.0 || fast.s > slow.s,
                    "fast vehicle must not clip into slow vehicle (fast.s={}, slow.s={})",
                    fast.s,
                    slow.s
                );
            }

            if let Some(onc) = oncoming {
                // While oncoming car is still ahead of us (world_x > fast.world_x), we must wait & slow down!
                if onc.world_x > fast.world_x + 5.0 {
                    assert_ne!(
                        fast.state,
                        TrafficBehaviorState::Overtaking,
                        "must NOT overtake while oncoming car is approaching (fast.x={}, onc.x={})",
                        fast.world_x,
                        onc.world_x
                    );
                    if fast.speed < 11.0 {
                        slowed_while_oncoming = true;
                    }
                } else if fast.state == TrafficBehaviorState::Overtaking {
                    overtook_after_clear = true;
                }
            } else if fast.state == TrafficBehaviorState::Overtaking {
                overtook_after_clear = true;
            }
        }

        assert!(
            slowed_while_oncoming,
            "fast vehicle should reduce speed behind front car while waiting for oncoming car"
        );
        assert!(
            overtook_after_clear,
            "fast vehicle should overtake once oncoming traffic has passed"
        );
    }

    #[test]
    fn traffic_signal_stops_vehicles_on_red_and_releases_on_green() {
        let anchor = test_anchor();
        let lane_ew = straight_lane_local(anchor, "Barat -> Timur", -120.0, 0.0, 120.0, 0.0);
        let lane_ns = straight_lane_local(anchor, "Utara -> Selatan", 0.0, -120.0, 0.0, 120.0);

        let light_ew = build_traffic_light_on_lane(
            &lane_ew,
            anchor,
            105.0,
            0, // Phase 0: Green first (0..8s), Yellow (8..10s), All-Red (10..11s)
            "Lampu EW",
            3.6,
            OvertakeSide::Right,
        )
        .expect("light ew");
        let light_ns = build_traffic_light_on_lane(
            &lane_ns,
            anchor,
            105.0,
            1, // Phase 1: Red during 0..11s, then Green (11..19s)
            "Lampu NS",
            3.6,
            OvertakeSide::Right,
        )
        .expect("light ns");

        let cfg = TrafficAnimConfig {
            lanes: vec![lane_ew, lane_ns],
            vehicle_pool: sample_pool(),
            max_vehicles_per_lane: 4,
            max_total_vehicles: 8,
            traffic_lights: vec![light_ew, light_ns],
            signal_phases: vec![
                TrafficSignalPhase {
                    name: "Fase 1".into(),
                    green_sec: 8.0,
                    yellow_sec: 2.0,
                    all_red_sec: 1.0,
                },
                TrafficSignalPhase {
                    name: "Fase 2".into(),
                    green_sec: 8.0,
                    yellow_sec: 2.0,
                    all_red_sec: 1.0,
                },
            ],
            ..Default::default()
        };

        let mut engine = TrafficSimEngine::new(&cfg, anchor);
        engine.vehicles.clear();
        for l in &mut engine.lanes {
            l.next_spawn_timer = 999.0;
        }
        // Place one car on lane_ns (Phase 1 = RED initially) at s=70 approaching stop line s=105
        let smp = engine.lanes[1].sample(70.0);
        engine.vehicles.push(SimVehicle {
            id: 10,
            lane_idx: 1,
            pool_idx: 0,
            s: 70.0,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            speed: 12.0,
            desired_speed: 12.0,
            length_m: 4.5,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: smp.x,
            world_y: 0.0,
            world_z: smp.z,
            dir_x: smp.fx,
            dir_z: smp.fz,
            rot_y_deg: 0.0,
        });

        // Advance to t = 7.0s (still RED for Phase 1)
        engine.advance_to(7.0);
        let car_red = engine.vehicles.iter().find(|v| v.id == 10).expect("car still on lane");
        assert!(
            car_red.s + 0.5 * car_red.length_m <= 105.0,
            "vehicle must stop behind stop line on red (front_s={})",
            car_red.s + 0.5 * car_red.length_m
        );
        assert!(
            car_red.speed < 0.5,
            "vehicle should come to a halt at red light (speed={})",
            car_red.speed
        );

        // Check phase transitions: t=9.0 -> Phase 0 Yellow, t=10.5 -> All-Red, t=12.0 -> Phase 1 Green
        let (_, _, c9, ar9, _) = evaluate_signal_phase_at_time(&cfg, 9.0);
        assert_eq!(c9, TrafficLightColor::Yellow);
        assert!(!ar9);
        let (_, _, c105, ar105, _) = evaluate_signal_phase_at_time(&cfg, 10.5);
        assert_eq!(c105, TrafficLightColor::Red);
        assert!(ar105);

        // Advance to t = 16.0s (Phase 1 has been GREEN since t=11s)
        engine.advance_to(16.0);
        let car_green = engine.vehicles.iter().find(|v| v.id == 10).expect("car still on lane");
        assert!(
            car_green.s > 115.0,
            "vehicle should accelerate past stop line once green (s={})",
            car_green.s
        );
        assert!(
            car_green.speed > 8.0,
            "vehicle should regain configured cruising speed on green (speed={})",
            car_green.speed
        );
    }

    #[test]
    fn auto_place_intersection_groups_crossing_lanes_into_distinct_phases() {
        let anchor = test_anchor();
        let lane_ew = straight_lane_local(anchor, "Barat -> Timur", -120.0, 0.0, 120.0, 0.0);
        let lane_we = build_reverse_lane(&lane_ew, 3.6, OvertakeSide::Right, "Timur -> Barat")
            .expect("rev");
        let lane_ns = straight_lane_local(anchor, "Utara -> Selatan", 0.0, -120.0, 0.0, 120.0);
        let lane_sn = build_reverse_lane(&lane_ns, 3.6, OvertakeSide::Right, "Selatan -> Utara")
            .expect("rev");
        let mut cfg = TrafficAnimConfig {
            lanes: vec![lane_ew, lane_we, lane_ns, lane_sn],
            vehicle_pool: sample_pool(),
            ..Default::default()
        };
        let added = auto_place_intersection_traffic_lights(&mut cfg, anchor);
        assert_eq!(added, 4);
        assert_eq!(cfg.signal_phases.len(), 2);
        assert_eq!(cfg.traffic_lights[0].phase_idx, cfg.traffic_lights[1].phase_idx);
        assert_eq!(cfg.traffic_lights[2].phase_idx, cfg.traffic_lights[3].phase_idx);
        assert_ne!(cfg.traffic_lights[0].phase_idx, cfg.traffic_lights[2].phase_idx);
    }

    #[test]
    fn signalized_crossing_never_deadlocks_against_red_waiting_vehicles() {
        let anchor = test_anchor();
        // Two perpendicular crossing lanes intersecting at (0, 0), s = 100 on both lanes.
        let lane_ew = straight_lane_local(anchor, "Barat -> Timur", -100.0, 0.0, 100.0, 0.0);
        let lane_ns = straight_lane_local(anchor, "Utara -> Selatan", 0.0, -100.0, 0.0, 100.0);

        // Stop lines close to the crossing (6m before crossing = s=94), where the old yield check deadlocked!
        let light_ew = build_traffic_light_on_lane(
            &lane_ew,
            anchor,
            94.0,
            0,
            "Lampu EW",
            3.6,
            OvertakeSide::Right,
        )
        .expect("light ew");
        let light_ns = build_traffic_light_on_lane(
            &lane_ns,
            anchor,
            94.0,
            1,
            "Lampu NS",
            3.6,
            OvertakeSide::Right,
        )
        .expect("light ns");

        let cfg = TrafficAnimConfig {
            lanes: vec![lane_ew, lane_ns],
            vehicle_pool: sample_pool(),
            max_vehicles_per_lane: 4,
            max_total_vehicles: 8,
            yield_at_intersections: true,
            traffic_lights: vec![light_ew, light_ns],
            signal_phases: vec![
                TrafficSignalPhase {
                    name: "Fase 1".into(),
                    green_sec: 8.0,
                    yellow_sec: 2.0,
                    all_red_sec: 1.0,
                },
                TrafficSignalPhase {
                    name: "Fase 2".into(),
                    green_sec: 8.0,
                    yellow_sec: 2.0,
                    all_red_sec: 1.0,
                },
            ],
            ..Default::default()
        };

        let mut engine = TrafficSimEngine::new(&cfg, anchor);
        engine.vehicles.clear();
        for l in &mut engine.lanes {
            l.next_spawn_timer = 999.0;
        }
        // Place one car stopped right at EW stop line (GREEN) AND one car stopped right at NS stop line (RED)
        for (id, lane_idx, s) in [(1, 0, 91.0), (2, 1, 91.0)] {
            let smp = engine.lanes[lane_idx].sample(s);
            engine.vehicles.push(SimVehicle {
                id,
                lane_idx,
                pool_idx: 0,
                s,
                lateral_offset: 0.0,
                lateral_vel: 0.0,
                speed: 0.0,
                desired_speed: 12.0,
                length_m: 4.5,
                state: TrafficBehaviorState::WaitingSignal,
                overtake_target_id: None,
                overtake_timer: 0.0,
                being_overtaken: false,
                world_x: smp.x,
                world_y: 0.0,
                world_z: smp.z,
                dir_x: smp.fx,
                dir_z: smp.fz,
                rot_y_deg: 0.0,
            });
        }

        // Advance 5 seconds: EW (Phase 0 = GREEN) must drive cleanly across s=100 while NS stays behind s=94!
        engine.advance_to(5.0);
        let ew_car = engine.vehicles.iter().find(|v| v.id == 1).expect("ew car");
        let ns_car = engine.vehicles.iter().find(|v| v.id == 2).expect("ns car");
        assert!(
            ew_car.s > 120.0,
            "GREEN lane car must NOT deadlock against RED waiting car (ew.s={})",
            ew_car.s
        );
        assert!(
            ns_car.s + 0.5 * ns_car.length_m <= 94.0 && ns_car.speed < 0.2,
            "RED lane car must stay stopped behind stop line (ns.s={}, speed={})",
            ns_car.s,
            ns_car.speed
        );

        // Advance to t=16.0s (Phase 1 = GREEN since t=11.0s): NS car must drive cleanly across s=100!
        engine.advance_to(16.0);
        let ns_car2 = engine.vehicles.iter().find(|v| v.id == 2).expect("ns car2");
        assert!(
            ns_car2.s > 120.0,
            "Phase 2 car must accelerate cleanly through crossing on GREEN (ns.s={})",
            ns_car2.s
        );
    }
}
