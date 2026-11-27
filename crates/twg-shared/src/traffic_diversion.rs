//! Traffic Diversion / Rekayasa Pengalihan Arus Simulation (`traffic_diversion`).
//!
//! Models Y-junction diversion corridors where each corridor consists of 3 connected polylines:
//! - `approach_geom`: incoming road before the diversion junction
//! - `main_geom`: normal continuation past the junction
//! - `divert_geom`: alternate/detour route past the junction
//!
//! At runtime, operators can live-toggle **"Alihkan Arus"** <-> **"Normalkan Arus"** (synced
//! deterministically to Layar Besar via WebSocket). Vehicles already past the split point finish
//! exiting their committed branch smoothly, while approaching vehicles seamlessly route into the
//! newly selected branch!

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    geo::GeoAnchor,
    models::Anim,
    traffic_anim::{OvertakeSide, TrafficBehaviorState, TrafficVehicleEntry, TrafficVehiclePose},
};

pub const TRAFFIC_DIVERSION_ANIM_TYPE: &str = "traffic_diversion";

/// Traffic flow behavior style for `Pengalihan Arus` (`traffic_diversion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiversionFlowMode {
    /// Constant uniform convoy speed without overtaking (ideal for narrow/coned detour lanes).
    #[default]
    Convoy,
    /// Realistic free-flowing traffic with randomized speeds, randomized spawn intervals, and
    /// smart overtaking (matching `Simulasi Lalu Lintas`), while still cleanly obeying live diversion.
    FreeFlow,
}

impl DiversionFlowMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Convoy => "Konvoi (Konstan)",
            Self::FreeFlow => "Arus Normal (Acak & Salip)",
        }
    }
}

/// One Y-junction diversion corridor: approach road + normal main continuation + alternate detour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficDiversionCorridor {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    #[serde(default = "default_diversion_corridor_name")]
    pub name: String,
    /// Incoming road up to the diversion split point (`[lng, lat, alt]`).
    #[serde(default)]
    pub approach_geom: Vec<[f64; 3]>,
    /// Normal main route past the split point (`[lng, lat, alt]`).
    #[serde(default)]
    pub main_geom: Vec<[f64; 3]>,
    /// Alternate / diversion route past the split point (`[lng, lat, alt]`).
    #[serde(default)]
    pub divert_geom: Vec<[f64; 3]>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_diversion_corridor_name() -> String {
    "Koridor 1".into()
}

fn default_true() -> bool {
    true
}

impl TrafficDiversionCorridor {
    pub fn new(name: impl Into<String>, approach_geom: Vec<[f64; 3]>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            approach_geom,
            main_geom: Vec::new(),
            divert_geom: Vec::new(),
            enabled: true,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.approach_geom.len() >= 2
            && self.main_geom.len() >= 2
            && self.divert_geom.len() >= 2
    }

    pub fn missing_parts_label(&self) -> Option<String> {
        let mut missing = Vec::new();
        if self.approach_geom.len() < 2 {
            missing.push("Jalur Pendekat");
        }
        if self.main_geom.len() < 2 {
            missing.push("Jalur Utama");
        }
        if self.divert_geom.len() < 2 {
            missing.push("Jalur Alternatif");
        }
        if missing.is_empty() {
            None
        } else {
            Some(format!("{} (belum ada {})", self.name, missing.join(" & ")))
        }
    }
}

/// Complete configuration for a `Pengalihan Arus` (`traffic_diversion`) animation,
/// persisted inside `anim.data["traffic_diversion"]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficDiversionConfig {
    #[serde(default)]
    pub corridors: Vec<TrafficDiversionCorridor>,
    #[serde(default)]
    pub vehicle_pool: Vec<TrafficVehicleEntry>,
    /// Flow style: uniform `Convoy` or realistic `FreeFlow` (random speed + overtaking).
    #[serde(default)]
    pub flow_mode: DiversionFlowMode,
    /// Constant convoy speed on the approach & normal main route in `Convoy` mode (m/s).
    #[serde(default = "default_normal_speed_mps")]
    pub normal_speed_mps: f64,
    /// Speed on the diversion / alternate route in `Convoy` mode (m/s).
    #[serde(default = "default_divert_speed_mps")]
    pub divert_speed_mps: f64,
    /// Minimum randomized vehicle speed in `FreeFlow` mode (m/s).
    #[serde(default = "default_min_speed_mps")]
    pub min_speed_mps: f64,
    /// Maximum randomized vehicle speed in `FreeFlow` mode (m/s).
    #[serde(default = "default_max_speed_mps")]
    pub max_speed_mps: f64,
    /// Speed percentage applied while driving along `divert_geom` in `FreeFlow` mode (`30..=120`).
    #[serde(default = "default_divert_speed_pct")]
    pub divert_speed_pct: f64,
    /// Maximum active vehicles per corridor on the approach+active branch.
    #[serde(default = "default_max_vehicles_per_corridor")]
    pub max_vehicles_per_corridor: u32,
    /// Bumper-to-bumper safe clearance (meters).
    #[serde(default = "default_min_safe_gap_m")]
    pub min_safe_gap_m: f64,
    /// Average spawn interval per corridor in `Convoy` mode (seconds).
    #[serde(default = "default_spawn_interval_sec")]
    pub spawn_interval_sec: f64,
    /// Minimum random spawn interval in `FreeFlow` mode (seconds).
    #[serde(default = "default_spawn_interval_min_sec")]
    pub spawn_interval_min_sec: f64,
    /// Maximum random spawn interval in `FreeFlow` mode (seconds).
    #[serde(default = "default_spawn_interval_max_sec")]
    pub spawn_interval_max_sec: f64,
    /// Lateral lane width for overtaking in `FreeFlow` mode (meters).
    #[serde(default = "default_lane_width_m")]
    pub lane_width_m: f64,
    /// Enables smart overtaking in `FreeFlow` mode.
    #[serde(default = "default_true")]
    pub allow_overtaking: bool,
    /// Side used to overtake slower vehicles in `FreeFlow` mode.
    #[serde(default)]
    pub overtake_side: OvertakeSide,
    /// Courtesy slowdown percentage for a slower front vehicle while being overtaken (`0..=60`).
    #[serde(default = "default_front_yield_slowdown_pct")]
    pub front_yield_slowdown_pct: f64,
    /// Whether the animation starts already diverted (`true`) or normal on main route (`false`).
    #[serde(default)]
    pub start_diverted: bool,
    /// When `true`, pre-populates vehicles along approach + initial branch right at `t = 0`.
    #[serde(default = "default_true")]
    pub prefill_initial: bool,
    /// Deterministic PRNG seed so Operator & Layar Besar always match identically.
    #[serde(default = "default_seed")]
    pub seed: u64,
}

fn default_normal_speed_mps() -> f64 {
    10.0
}
fn default_divert_speed_mps() -> f64 {
    8.5
}
fn default_min_speed_mps() -> f64 {
    6.5
}
fn default_max_speed_mps() -> f64 {
    14.5
}
fn default_divert_speed_pct() -> f64 {
    85.0
}
fn default_max_vehicles_per_corridor() -> u32 {
    14
}
fn default_min_safe_gap_m() -> f64 {
    3.0
}
fn default_spawn_interval_sec() -> f64 {
    1.5
}
fn default_spawn_interval_min_sec() -> f64 {
    1.0
}
fn default_spawn_interval_max_sec() -> f64 {
    3.2
}
fn default_lane_width_m() -> f64 {
    3.6
}
fn default_front_yield_slowdown_pct() -> f64 {
    20.0
}
fn default_seed() -> u64 {
    20_261_129
}

impl Default for TrafficDiversionConfig {
    fn default() -> Self {
        Self {
            corridors: Vec::new(),
            vehicle_pool: Vec::new(),
            flow_mode: DiversionFlowMode::default(),
            normal_speed_mps: default_normal_speed_mps(),
            divert_speed_mps: default_divert_speed_mps(),
            min_speed_mps: default_min_speed_mps(),
            max_speed_mps: default_max_speed_mps(),
            divert_speed_pct: default_divert_speed_pct(),
            max_vehicles_per_corridor: default_max_vehicles_per_corridor(),
            min_safe_gap_m: default_min_safe_gap_m(),
            spawn_interval_sec: default_spawn_interval_sec(),
            spawn_interval_min_sec: default_spawn_interval_min_sec(),
            spawn_interval_max_sec: default_spawn_interval_max_sec(),
            lane_width_m: default_lane_width_m(),
            allow_overtaking: true,
            overtake_side: OvertakeSide::Right,
            front_yield_slowdown_pct: default_front_yield_slowdown_pct(),
            start_diverted: false,
            prefill_initial: true,
            seed: default_seed(),
        }
    }
}

impl TrafficDiversionConfig {
    pub fn sanitized(&self) -> Self {
        let min_spd = self.min_speed_mps.clamp(1.5, 45.0);
        let max_spd = self.max_speed_mps.clamp(min_spd, 55.0);
        let spw_min = self.spawn_interval_min_sec.clamp(0.3, 30.0);
        let spw_max = self.spawn_interval_max_sec.clamp(spw_min, 60.0);
        Self {
            corridors: self.corridors.clone(),
            vehicle_pool: self.vehicle_pool.clone(),
            flow_mode: self.flow_mode,
            normal_speed_mps: self.normal_speed_mps.clamp(1.5, 45.0),
            divert_speed_mps: self.divert_speed_mps.clamp(1.5, 45.0),
            min_speed_mps: min_spd,
            max_speed_mps: max_spd,
            divert_speed_pct: self.divert_speed_pct.clamp(30.0, 120.0),
            max_vehicles_per_corridor: self.max_vehicles_per_corridor.clamp(1, 60),
            min_safe_gap_m: self.min_safe_gap_m.clamp(0.8, 25.0),
            spawn_interval_sec: self.spawn_interval_sec.clamp(0.3, 20.0),
            spawn_interval_min_sec: spw_min,
            spawn_interval_max_sec: spw_max,
            lane_width_m: if self.lane_width_m.is_finite() {
                self.lane_width_m.max(0.5)
            } else {
                default_lane_width_m()
            },
            allow_overtaking: self.allow_overtaking,
            overtake_side: self.overtake_side,
            front_yield_slowdown_pct: self.front_yield_slowdown_pct.clamp(0.0, 60.0),
            start_diverted: self.start_diverted,
            prefill_initial: self.prefill_initial,
            seed: if self.seed == 0 { default_seed() } else { self.seed },
        }
    }

    pub fn incomplete_corridor_labels(&self) -> Vec<String> {
        self.corridors
            .iter()
            .filter(|c| c.enabled)
            .filter_map(|c| c.missing_parts_label())
            .collect()
    }

    pub fn has_playable_setup(&self) -> bool {
        let enabled: Vec<&TrafficDiversionCorridor> =
            self.corridors.iter().filter(|c| c.enabled).collect();
        !enabled.is_empty()
            && enabled.iter().all(|c| c.is_complete())
            && !self.vehicle_pool.is_empty()
    }

    pub fn config_signature(&self, anchor: GeoAnchor) -> u64 {
        let mut h = DefaultHasher::new();
        anchor.origin_mx.to_bits().hash(&mut h);
        anchor.origin_my.to_bits().hash(&mut h);
        (self.flow_mode as u8).hash(&mut h);
        self.normal_speed_mps.to_bits().hash(&mut h);
        self.divert_speed_mps.to_bits().hash(&mut h);
        self.min_speed_mps.to_bits().hash(&mut h);
        self.max_speed_mps.to_bits().hash(&mut h);
        self.divert_speed_pct.to_bits().hash(&mut h);
        self.max_vehicles_per_corridor.hash(&mut h);
        self.min_safe_gap_m.to_bits().hash(&mut h);
        self.spawn_interval_sec.to_bits().hash(&mut h);
        self.spawn_interval_min_sec.to_bits().hash(&mut h);
        self.spawn_interval_max_sec.to_bits().hash(&mut h);
        self.lane_width_m.to_bits().hash(&mut h);
        self.allow_overtaking.hash(&mut h);
        (self.overtake_side as u8).hash(&mut h);
        self.front_yield_slowdown_pct.to_bits().hash(&mut h);
        self.start_diverted.hash(&mut h);
        self.prefill_initial.hash(&mut h);
        self.seed.hash(&mut h);
        self.corridors.len().hash(&mut h);
        for c in &self.corridors {
            c.id.hash(&mut h);
            c.enabled.hash(&mut h);
            for geom in [&c.approach_geom, &c.main_geom, &c.divert_geom] {
                geom.len().hash(&mut h);
                for pt in geom {
                    pt[0].to_bits().hash(&mut h);
                    pt[1].to_bits().hash(&mut h);
                    pt[2].to_bits().hash(&mut h);
                }
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
        h.finish()
    }
}

pub fn is_traffic_diversion_anim(anim: &Anim) -> bool {
    anim.anim_type
        .eq_ignore_ascii_case(TRAFFIC_DIVERSION_ANIM_TYPE)
}

pub fn parse_traffic_diversion_config(anim: &Anim) -> TrafficDiversionConfig {
    let mut cfg: TrafficDiversionConfig = anim
        .data
        .get("traffic_diversion")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.corridors.is_empty() && anim.geom.len() >= 2 {
        cfg.corridors.push(TrafficDiversionCorridor::new(
            "Koridor 1",
            anim.geom.clone(),
        ));
    }
    cfg.sanitized()
}

pub fn write_traffic_diversion_config(anim: &mut Anim, cfg: &TrafficDiversionConfig) {
    let clean = cfg.sanitized();
    anim.anim_type = TRAFFIC_DIVERSION_ANIM_TYPE.to_string();
    anim.marker_id = None;
    if let Some(first) = clean
        .corridors
        .iter()
        .find(|c| c.enabled && c.approach_geom.len() >= 2)
        .or_else(|| clean.corridors.iter().find(|c| c.approach_geom.len() >= 2))
    {
        anim.geom = first.approach_geom.clone();
    } else {
        anim.geom.clear();
    }
    let mut obj = anim.data.as_object().cloned().unwrap_or_default();
    if let Ok(val) = serde_json::to_value(&clean) {
        obj.insert("traffic_diversion".into(), val);
    }
    anim.data = serde_json::Value::Object(obj);
}

/// Live telemetry snapshot for Play Panel & Plot Editor Preview UI.
#[derive(Debug, Clone, Default)]
pub struct TrafficDiversionLiveStatus {
    pub is_diverted: bool,
    pub total_vehicles: usize,
    pub on_approach: usize,
    pub on_main: usize,
    pub on_divert: usize,
}

// ---------------------------------------------------------------------------
// Deterministic SplitMix64 PRNG
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }

    fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        if hi <= lo {
            lo
        } else {
            lo + (hi - lo) * self.next_f64()
        }
    }
}

// ---------------------------------------------------------------------------
// Compiled Y-Junction Geometry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct PolySample {
    x: f64,
    y: f64,
    z: f64,
    fx: f64,
    fz: f64,
}

#[derive(Debug, Clone)]
struct CompiledPoly {
    pts: Vec<[f64; 3]>,
    cum_dist: Vec<f64>,
    total_len: f64,
}

impl CompiledPoly {
    fn build(pts: Vec<[f64; 3]>) -> Option<Self> {
        if pts.len() < 2 {
            return None;
        }
        let mut cum = Vec::with_capacity(pts.len());
        cum.push(0.0);
        let mut total = 0.0;
        for w in pts.windows(2) {
            let seg = ((w[1][0] - w[0][0]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt();
            total += seg.max(1e-4);
            cum.push(total);
        }
        if total < 0.5 {
            return None;
        }
        Some(Self {
            pts,
            cum_dist: cum,
            total_len: total,
        })
    }

    fn sample(&self, s: f64) -> PolySample {
        let clamped = s.clamp(0.0, self.total_len);
        let idx = match self
            .cum_dist
            .binary_search_by(|d| d.partial_cmp(&clamped).unwrap_or(std::cmp::Ordering::Equal))
        {
            Ok(i) => i.min(self.pts.len() - 2),
            Err(i) => i.saturating_sub(1).min(self.pts.len() - 2),
        };
        let d0 = self.cum_dist[idx];
        let d1 = self.cum_dist[idx + 1];
        let t = if d1 > d0 + 1e-6 {
            ((clamped - d0) / (d1 - d0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let a = self.pts[idx];
        let b = self.pts[idx + 1];
        let dx = b[0] - a[0];
        let dz = b[2] - a[2];
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        PolySample {
            x: a[0] + dx * t,
            y: a[1] + (b[1] - a[1]) * t,
            z: a[2] + dz * t,
            fx: dx / len,
            fz: dz / len,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VehicleBranch {
    Approach,
    Main,
    Divert,
}

#[derive(Debug, Clone)]
struct CompiledDiversionCorridor {
    source_idx: usize,
    approach_poly: CompiledPoly,
    main_poly: CompiledPoly,
    divert_poly: CompiledPoly,
    spawn_cooldown: f64,
}

impl CompiledDiversionCorridor {
    fn poly_for(&self, branch: VehicleBranch) -> &CompiledPoly {
        match branch {
            VehicleBranch::Approach => &self.approach_poly,
            VehicleBranch::Main => &self.main_poly,
            VehicleBranch::Divert => &self.divert_poly,
        }
    }
}

#[derive(Debug, Clone)]
struct DiversionVehicle {
    id: u64,
    corridor_idx: usize,
    pool_idx: usize,
    branch: VehicleBranch,
    /// Distance along the current `branch` polyline (`0 .. poly.total_len`).
    s: f64,
    speed: f64,
    desired_speed: f64,
    length_m: f64,
    lateral_offset: f64,
    lateral_vel: f64,
    state: TrafficBehaviorState,
    overtake_target_id: Option<u64>,
    overtake_timer: f64,
    being_overtaken: bool,
    world_x: f64,
    world_z: f64,
    dir_x: f64,
    dir_z: f64,
}

/// Deterministic fixed-timestep (30 Hz) engine for `Pengalihan Arus` (`traffic_diversion`).
#[derive(Debug, Clone)]
pub struct TrafficDiversionSimEngine {
    cfg: TrafficDiversionConfig,
    corridors: Vec<CompiledDiversionCorridor>,
    vehicles: Vec<DiversionVehicle>,
    rng: SplitMix64,
    next_vehicle_id: u64,
    sim_time: f64,
    render_alpha_sec: f64,
    is_diverted: bool,
    applied_events: Vec<(f64, bool)>,
    next_event_cursor: usize,
}

const FIXED_DT: f64 = 1.0 / 30.0;

impl TrafficDiversionSimEngine {
    pub fn new(cfg: &TrafficDiversionConfig, anchor: GeoAnchor) -> Self {
        let clean = cfg.sanitized();
        let mut corridors = Vec::new();
        for (src_idx, c) in clean.corridors.iter().enumerate() {
            if !c.enabled || !c.is_complete() {
                continue;
            }
            let to_local = |geom: &[[f64; 3]]| -> Vec<[f64; 3]> {
                geom.iter()
                    .map(|p| {
                        let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
                        [l[0] as f64, l[1] as f64, l[2] as f64]
                    })
                    .collect()
            };
            let app_pts = to_local(&c.approach_geom);
            let mut main_pts = to_local(&c.main_geom);
            let mut div_pts = to_local(&c.divert_geom);

            // Bridge smoothly from end of approach into start of main & divert if separated
            if let Some(&split_pt) = app_pts.last() {
                if let Some(&m0) = main_pts.first() {
                    let d = ((m0[0] - split_pt[0]).powi(2) + (m0[2] - split_pt[2]).powi(2)).sqrt();
                    if d > 0.25 {
                        main_pts.insert(0, split_pt);
                    }
                }
                if let Some(&d0) = div_pts.first() {
                    let d = ((d0[0] - split_pt[0]).powi(2) + (d0[2] - split_pt[2]).powi(2)).sqrt();
                    if d > 0.25 {
                        div_pts.insert(0, split_pt);
                    }
                }
            }

            let (Some(approach_poly), Some(main_poly), Some(divert_poly)) = (
                CompiledPoly::build(app_pts),
                CompiledPoly::build(main_pts),
                CompiledPoly::build(div_pts),
            ) else {
                continue;
            };
            corridors.push(CompiledDiversionCorridor {
                source_idx: src_idx,
                approach_poly,
                main_poly,
                divert_poly,
                spawn_cooldown: 0.15 * (src_idx as f64),
            });
        }

        let rng = SplitMix64::new(clean.seed);
        let initial_diverted = clean.start_diverted;
        let mut engine = Self {
            cfg: clean,
            corridors,
            vehicles: Vec::new(),
            rng,
            next_vehicle_id: 1,
            sim_time: 0.0,
            render_alpha_sec: 0.0,
            is_diverted: initial_diverted,
            applied_events: Vec::new(),
            next_event_cursor: 0,
        };
        if engine.cfg.prefill_initial {
            engine.seed_initial_flow();
        }
        engine
    }

    pub fn reset(&mut self) {
        self.vehicles.clear();
        self.rng = SplitMix64::new(self.cfg.seed);
        self.next_vehicle_id = 1;
        self.sim_time = 0.0;
        self.render_alpha_sec = 0.0;
        self.is_diverted = self.cfg.start_diverted;
        self.applied_events.clear();
        self.next_event_cursor = 0;
        for (i, c) in self.corridors.iter_mut().enumerate() {
            c.spawn_cooldown = 0.15 * (i as f64);
        }
        if self.cfg.prefill_initial {
            self.seed_initial_flow();
        }
    }

    /// Pre-populates evenly spaced moving vehicles along approach + active initial branch so the
    /// road immediately looks alive on Play.
    fn seed_initial_flow(&mut self) {
        if self.cfg.vehicle_pool.is_empty() {
            return;
        }
        let active_branch = if self.cfg.start_diverted {
            VehicleBranch::Divert
        } else {
            VehicleBranch::Main
        };
        let n_corridors = self.corridors.len();
        for c_idx in 0..n_corridors {
            let app_len = self.corridors[c_idx].approach_poly.total_len;
            let br_len = self.corridors[c_idx].poly_for(active_branch).total_len;
            let total_len = app_len + br_len;
            let avg_speed = match self.cfg.flow_mode {
                DiversionFlowMode::Convoy => {
                    if self.cfg.start_diverted {
                        self.cfg.divert_speed_mps
                    } else {
                        self.cfg.normal_speed_mps
                    }
                }
                DiversionFlowMode::FreeFlow => {
                    0.5 * (self.cfg.min_speed_mps + self.cfg.max_speed_mps)
                }
            };
            let avg_spw = match self.cfg.flow_mode {
                DiversionFlowMode::Convoy => self.cfg.spawn_interval_sec,
                DiversionFlowMode::FreeFlow => {
                    0.5 * (self.cfg.spawn_interval_min_sec + self.cfg.spawn_interval_max_sec)
                }
            };
            let pitch = (avg_speed * avg_spw).max(11.0);
            let max_v = self.cfg.max_vehicles_per_corridor as usize;
            let mut cursor_s = total_len * 0.85;
            let mut count = 0;
            while cursor_s >= 4.0 && count < max_v {
                let pool_idx = self.pick_pool_index();
                let length_m = self.cfg.vehicle_pool[pool_idx].length_m.clamp(1.5, 22.0);
                let desired_spd = match self.cfg.flow_mode {
                    DiversionFlowMode::Convoy => self.cfg.normal_speed_mps,
                    DiversionFlowMode::FreeFlow => self
                        .rng
                        .range_f64(self.cfg.min_speed_mps, self.cfg.max_speed_mps),
                };
                let (branch, local_s, spd) = if cursor_s <= app_len {
                    (VehicleBranch::Approach, cursor_s, desired_spd)
                } else {
                    let br_spd = match self.cfg.flow_mode {
                        DiversionFlowMode::Convoy => {
                            if self.cfg.start_diverted {
                                self.cfg.divert_speed_mps
                            } else {
                                self.cfg.normal_speed_mps
                            }
                        }
                        DiversionFlowMode::FreeFlow => {
                            if active_branch == VehicleBranch::Divert {
                                desired_spd * (self.cfg.divert_speed_pct / 100.0)
                            } else {
                                desired_spd
                            }
                        }
                    };
                    (active_branch, cursor_s - app_len, br_spd)
                };
                let sample = self.corridors[c_idx].poly_for(branch).sample(local_s);
                let id = self.next_vehicle_id;
                self.next_vehicle_id += 1;
                self.vehicles.push(DiversionVehicle {
                    id,
                    corridor_idx: c_idx,
                    pool_idx,
                    branch,
                    s: local_s,
                    speed: spd,
                    desired_speed: desired_spd,
                    length_m,
                    lateral_offset: 0.0,
                    lateral_vel: 0.0,
                    state: TrafficBehaviorState::Cruising,
                    overtake_target_id: None,
                    overtake_timer: 0.0,
                    being_overtaken: false,
                    world_x: sample.x,
                    world_z: sample.z,
                    dir_x: sample.fx,
                    dir_z: sample.fz,
                });
                count += 1;
                cursor_s -= pitch.max(length_m + self.cfg.min_safe_gap_m + 2.0);
            }
        }
    }

    fn events_match(prev: &[(f64, bool)], next: &[(f64, bool)], sim_time: f64) -> bool {
        if next.len() < prev.len() {
            return false;
        }
        for (a, b) in prev.iter().zip(next.iter()) {
            if (a.0 - b.0).abs() > 1e-4 || a.1 != b.1 {
                return false;
            }
        }
        // Any newly appended event that falls strictly in the past requires deterministic replay
        for &(ev_t, _) in &next[prev.len()..] {
            if ev_t + 1e-4 < sim_time {
                return false;
            }
        }
        true
    }

    /// Deterministically advances simulation to `target_elapsed_sec` with live diversion toggle
    /// events `events: &[(elapsed_sec, is_diverted)]`.
    pub fn advance_to(&mut self, target_elapsed_sec: f64, events: &[(f64, bool)]) {
        if self.corridors.is_empty() || self.cfg.vehicle_pool.is_empty() {
            return;
        }
        let target = target_elapsed_sec.max(0.0);
        let events_changed = !Self::events_match(&self.applied_events, events, self.sim_time);
        if target + 1e-4 < self.sim_time || events_changed {
            self.reset();
        }
        self.applied_events.clear();
        self.applied_events.extend_from_slice(events);

        let max_steps = 18_000;
        let mut steps = 0;
        while self.sim_time + FIXED_DT <= target + 1e-9 && steps < max_steps {
            self.step(FIXED_DT);
            self.sim_time += FIXED_DT;
            steps += 1;
        }
        self.render_alpha_sec = (target - self.sim_time).clamp(0.0, FIXED_DT);
    }

    pub fn live_status(&self) -> TrafficDiversionLiveStatus {
        let mut on_approach = 0;
        let mut on_main = 0;
        let mut on_divert = 0;
        for v in &self.vehicles {
            match v.branch {
                VehicleBranch::Approach => on_approach += 1,
                VehicleBranch::Main => on_main += 1,
                VehicleBranch::Divert => on_divert += 1,
            }
        }
        TrafficDiversionLiveStatus {
            is_diverted: self.is_diverted,
            total_vehicles: self.vehicles.len(),
            on_approach,
            on_main,
            on_divert,
        }
    }

    pub fn active_poses(&self) -> Vec<TrafficVehiclePose> {
        let mut out = Vec::with_capacity(self.vehicles.len());
        let alpha = self.render_alpha_sec;
        for v in &self.vehicles {
            let Some(c) = self.corridors.get(v.corridor_idx) else {
                continue;
            };
            let Some(entry) = self.cfg.vehicle_pool.get(v.pool_idx) else {
                continue;
            };
            let poly = c.poly_for(v.branch);
            let render_s = (v.s + v.speed * alpha).clamp(0.0, poly.total_len + 0.5 * v.length_m);
            let sample = poly.sample(render_s);
            let rx = -sample.fz;
            let rz = sample.fx;
            let lat = v.lateral_offset + v.lateral_vel * alpha;
            let wx = sample.x + rx * lat;
            let wz = sample.z + rz * lat;
            let fade_in = if v.branch == VehicleBranch::Approach {
                (render_s / 2.5).clamp(0.05, 1.0)
            } else {
                1.0
            };
            let fade_out = if v.branch == VehicleBranch::Approach {
                1.0
            } else {
                ((poly.total_len + 0.5 * v.length_m - render_s) / 3.0).clamp(0.0, 1.0)
            };
            let fade_scale = fade_in.min(fade_out) as f32;
            if fade_scale <= 0.01 {
                continue;
            }
            let steer_rad = if v.speed > 0.5 {
                (v.lateral_vel / v.speed).clamp(-0.35, 0.35).atan()
            } else {
                0.0
            };
            let yaw_deg =
                sample.fx.atan2(sample.fz).to_degrees() + steer_rad.to_degrees() + entry.rot_y_offset_deg;
            let behavior = if self.cfg.flow_mode == DiversionFlowMode::FreeFlow {
                v.state
            } else {
                match v.branch {
                    VehicleBranch::Divert => TrafficBehaviorState::Overtaking,
                    VehicleBranch::Main => TrafficBehaviorState::Cruising,
                    VehicleBranch::Approach => TrafficBehaviorState::Following,
                }
            };
            out.push(TrafficVehiclePose {
                vehicle_id: v.id,
                lane_source_idx: c.source_idx,
                model_path: entry.model_path.clone(),
                x: wx as f32,
                y: sample.y as f32,
                z: wz as f32,
                rot_y_deg: yaw_deg as f32,
                scale: entry.scale as f32,
                fade_scale,
                speed_mps: v.speed as f32,
                behavior,
            });
        }
        out
    }

    fn pick_pool_index(&mut self) -> usize {
        let total_w: f64 = self
            .cfg
            .vehicle_pool
            .iter()
            .map(|v| (v.weight as f64).max(0.05))
            .sum();
        if total_w <= 0.0 {
            return 0;
        }
        let mut r = self.rng.next_f64() * total_w;
        for (i, v) in self.cfg.vehicle_pool.iter().enumerate() {
            r -= (v.weight as f64).max(0.05);
            if r <= 0.0 {
                return i;
            }
        }
        self.cfg.vehicle_pool.len().saturating_sub(1)
    }

    fn try_spawn_on_corridor(&mut self, corridor_idx: usize) {
        let max_v = self.cfg.max_vehicles_per_corridor as usize;
        let approach_cnt = self
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == corridor_idx && v.branch == VehicleBranch::Approach)
            .count();
        if approach_cnt >= max_v {
            return;
        }
        let total_cnt = self
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == corridor_idx)
            .count();
        if total_cnt >= max_v.saturating_mul(3).min(150) {
            return;
        }

        let pool_idx = self.pick_pool_index();
        let length_m = self.cfg.vehicle_pool[pool_idx].length_m.clamp(1.5, 22.0);
        let app_len = self.corridors[corridor_idx].approach_poly.total_len;
        let spawn_s = (0.5 * length_m).min((app_len * 0.25).max(0.1));
        let min_clear_back = spawn_s + 0.5 * length_m + self.cfg.min_safe_gap_m;

        let blocked = self.vehicles.iter().any(|v| {
            v.corridor_idx == corridor_idx
                && v.branch == VehicleBranch::Approach
                && (v.s - 0.5 * v.length_m) < min_clear_back
        });
        if blocked {
            return;
        }

        let desired_spd = match self.cfg.flow_mode {
            DiversionFlowMode::Convoy => self.cfg.normal_speed_mps,
            DiversionFlowMode::FreeFlow => self
                .rng
                .range_f64(self.cfg.min_speed_mps, self.cfg.max_speed_mps),
        };
        let sample = self.corridors[corridor_idx]
            .approach_poly
            .sample(spawn_s);
        let id = self.next_vehicle_id;
        self.next_vehicle_id += 1;
        self.vehicles.push(DiversionVehicle {
            id,
            corridor_idx,
            pool_idx,
            branch: VehicleBranch::Approach,
            s: spawn_s,
            speed: desired_spd,
            desired_speed: desired_spd,
            length_m,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: sample.x,
            world_z: sample.z,
            dir_x: sample.fx,
            dir_z: sample.fz,
        });
    }

    fn is_overtake_corridor_safe(
        cur: &DiversionVehicle,
        target_front_id: u64,
        sample: PolySample,
        all: &[DiversionVehicle],
        overtake_offset: f64,
        lane_width: f64,
        pass_speed: f64,
        est_pass_time: f64,
        est_world_travel: f64,
    ) -> bool {
        let rx = -sample.fz;
        let rz = sample.fx;
        let corridor_half_w = (lane_width * 0.85).max(2.4);
        for other in all {
            if other.id == cur.id || other.id == target_front_id {
                continue;
            }
            let rel_x = other.world_x - cur.world_x;
            let rel_z = other.world_z - cur.world_z;
            let rel_fwd = rel_x * sample.fx + rel_z * sample.fz;
            let rel_lat = rel_x * rx + rel_z * rz;
            if (rel_lat - overtake_offset).abs() > corridor_half_w {
                continue;
            }
            let dir_dot = sample.fx * other.dir_x + sample.fz * other.dir_z;
            if dir_dot < -0.35 {
                let closing_dist =
                    (pass_speed + other.speed.max(5.0)) * (est_pass_time + 2.0) + 16.0;
                if rel_fwd > -(cur.length_m + 3.0) && rel_fwd < closing_dist {
                    return false;
                }
            } else if rel_fwd > -(other.length_m + 8.0) && rel_fwd < est_world_travel + 12.0 {
                return false;
            }
        }
        true
    }

    fn has_imminent_oncoming_threat(
        cur: &DiversionVehicle,
        sample: PolySample,
        all: &[DiversionVehicle],
        overtake_offset: f64,
        lane_width: f64,
    ) -> bool {
        let rx = -sample.fz;
        let rz = sample.fx;
        let corridor_half_w = (lane_width * 0.85).max(2.4);
        for other in all {
            if other.id == cur.id {
                continue;
            }
            let rel_x = other.world_x - cur.world_x;
            let rel_z = other.world_z - cur.world_z;
            let rel_fwd = rel_x * sample.fx + rel_z * sample.fz;
            let rel_lat = rel_x * rx + rel_z * rz;
            if (rel_lat - overtake_offset).abs() > corridor_half_w {
                continue;
            }
            let dir_dot = sample.fx * other.dir_x + sample.fz * other.dir_z;
            if dir_dot < -0.35 {
                let closing_spd = (cur.speed + other.speed).max(8.0);
                let safe_abort_dist = closing_spd * 2.2 + 12.0;
                if rel_fwd > 2.0 && rel_fwd < safe_abort_dist {
                    return true;
                }
            }
        }
        false
    }

    fn step_freeflow(&mut self, dt: f64, active_target_branch: VehicleBranch) {
        // Refresh cached world positions & headings
        for v in &mut self.vehicles {
            if let Some(cor) = self.corridors.get(v.corridor_idx) {
                let sample = cor.poly_for(v.branch).sample(v.s);
                let rx = -sample.fz;
                let rz = sample.fx;
                v.world_x = sample.x + rx * v.lateral_offset;
                v.world_z = sample.z + rz * v.lateral_offset;
                v.dir_x = sample.fx;
                v.dir_z = sample.fz;
            }
        }

        // Refresh `being_overtaken` flag from active overtakers
        for v in &mut self.vehicles {
            v.being_overtaken = false;
        }
        let overtaken_ids: Vec<u64> = self
            .vehicles
            .iter()
            .filter(|v| {
                matches!(
                    v.state,
                    TrafficBehaviorState::Overtaking | TrafficBehaviorState::Returning
                )
            })
            .filter_map(|v| v.overtake_target_id)
            .collect();
        for v in &mut self.vehicles {
            if overtaken_ids.contains(&v.id) {
                v.being_overtaken = true;
            }
        }

        let snap = self.vehicles.clone();
        let lane_width = self.cfg.lane_width_m;
        let overtake_offset = lane_width * self.cfg.overtake_side.lateral_sign();
        let min_gap = self.cfg.min_safe_gap_m;
        let yield_mult = (1.0 - self.cfg.front_yield_slowdown_pct / 100.0).clamp(0.4, 1.0);
        let divert_mult = self.cfg.divert_speed_pct / 100.0;

        for i in 0..self.vehicles.len() {
            let cur = &snap[i];
            let cor = &self.corridors[cur.corridor_idx];
            let poly = cor.poly_for(cur.branch);
            let sample = poly.sample(cur.s);
            let rem_branch = (poly.total_len - cur.s).max(0.0);

            // Find closest vehicle ahead on the same corridor & branch (plus rear of active target branch when on Approach)
            let mut front_in_lane: Option<(usize, f64, f64)> = None; // (snap_idx, bumper_gap, effective_other_s)
            let mut front_in_band: Option<(usize, f64)> = None;

            for (j, other) in snap.iter().enumerate() {
                if other.id == cur.id || other.corridor_idx != cur.corridor_idx {
                    continue;
                }
                let eff_other_s = if other.branch == cur.branch {
                    if other.s <= cur.s {
                        continue;
                    }
                    other.s
                } else if cur.branch == VehicleBranch::Approach
                    && other.branch == active_target_branch
                {
                    cor.approach_poly.total_len + other.s
                } else {
                    continue;
                };
                let bumper_gap = (eff_other_s - cur.s) - 0.5 * (other.length_m + cur.length_m);
                let lat_diff = (other.lateral_offset - cur.lateral_offset).abs();
                if other.lateral_offset.abs() < lane_width * 0.55
                    && front_in_lane.is_none_or(|(_, g, _)| bumper_gap < g)
                {
                    front_in_lane = Some((j, bumper_gap, eff_other_s));
                }
                if lat_diff < lane_width * 0.65
                    && front_in_band.is_none_or(|(_, g)| bumper_gap < g)
                {
                    front_in_band = Some((j, bumper_gap));
                }
            }

            let mut next_state = cur.state;
            let mut next_target_id = cur.overtake_target_id;
            let mut next_timer = cur.overtake_timer;
            let mut desired_lat: f64;

            let base_cruise = if cur.branch == VehicleBranch::Divert
                || (cur.branch == VehicleBranch::Approach && self.is_diverted && rem_branch < 16.0)
            {
                (cur.desired_speed * divert_mult).max(2.0)
            } else {
                cur.desired_speed
            };
            let mut target_speed = base_cruise;
            if cur.being_overtaken && cur.state != TrafficBehaviorState::Overtaking {
                target_speed = (target_speed * yield_mult).max(2.0);
            }

            // Safety zone before Y-junction split & branch exit so turning is always smooth & centered
            let near_split_or_exit = rem_branch < 26.0;

            match cur.state {
                TrafficBehaviorState::Cruising
                | TrafficBehaviorState::Following
                | TrafficBehaviorState::YieldingCrossing
                | TrafficBehaviorState::WaitingSignal => {
                    desired_lat = 0.0;
                    next_target_id = None;
                    next_timer = 0.0;

                    if let Some((front_idx, front_gap, eff_front_s)) = front_in_lane {
                        let front = &snap[front_idx];
                        let follow_trigger = (min_gap + cur.speed * 1.45).max(min_gap + 6.0);
                        if front_gap < follow_trigger {
                            next_state = TrafficBehaviorState::Following;
                            let same_branch = front.branch == cur.branch;
                            let wants_overtake = self.cfg.allow_overtaking
                                && same_branch
                                && !near_split_or_exit
                                && !cur.being_overtaken
                                && base_cruise >= front.speed + 1.2
                                && front.state != TrafficBehaviorState::Overtaking
                                && front.state != TrafficBehaviorState::Returning
                                && front_gap < follow_trigger * 0.95
                                && front_gap > 1.0;

                            if wants_overtake {
                                let pass_speed = (base_cruise * 1.12)
                                    .min(self.cfg.max_speed_mps * 1.2)
                                    .max(front.speed + 2.4);
                                let front_yield_spd = (front.speed * yield_mult).max(1.8);
                                let rel_speed = (pass_speed - front_yield_spd).max(1.8);
                                let pass_dist = (eff_front_s - cur.s)
                                    + 0.5 * (front.length_m + cur.length_m)
                                    + min_gap * 1.2;
                                let est_pass_time = (pass_dist / rel_speed) + 1.6;
                                let est_world_travel = pass_speed * est_pass_time;

                                let merge_space_ok = !snap.iter().any(|other| {
                                    other.id != cur.id
                                        && other.id != front.id
                                        && other.corridor_idx == cur.corridor_idx
                                        && other.branch == cur.branch
                                        && other.s > front.s
                                        && (other.s - front.s)
                                            < (0.5 * (other.length_m + front.length_m)
                                                + cur.length_m
                                                + min_gap * 1.5)
                                });

                                let corridor_clear = Self::is_overtake_corridor_safe(
                                    cur,
                                    front.id,
                                    sample,
                                    &snap,
                                    overtake_offset,
                                    lane_width,
                                    pass_speed,
                                    est_pass_time,
                                    est_world_travel,
                                );

                                if rem_branch > est_world_travel + 12.0
                                    && merge_space_ok
                                    && corridor_clear
                                {
                                    next_state = TrafficBehaviorState::Overtaking;
                                    next_target_id = Some(front.id);
                                    next_timer = 0.0;
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
                    next_timer += dt;
                    desired_lat = overtake_offset;
                    let target_front = next_target_id
                        .and_then(|tid| snap.iter().find(|v| v.id == tid));
                    let pass_speed = (base_cruise * 1.12)
                        .min(self.cfg.max_speed_mps * 1.2)
                        .max(
                            target_front
                                .map(|f| f.speed + 2.4)
                                .unwrap_or(base_cruise),
                        );
                    target_speed = pass_speed;

                    let imminent = Self::has_imminent_oncoming_threat(
                        cur,
                        sample,
                        &snap,
                        overtake_offset,
                        lane_width,
                    );
                    let must_return_for_split =
                        cur.branch == VehicleBranch::Approach && rem_branch < 18.0;

                    if let Some(tf) = target_front {
                        if tf.branch != cur.branch {
                            next_state = TrafficBehaviorState::Returning;
                            desired_lat = 0.0;
                        } else {
                            let clear_ahead = (cur.s - tf.s)
                                - 0.5 * (cur.length_m + tf.length_m)
                                >= min_gap.max(3.5);
                            let still_behind = cur.s < tf.s;
                            if clear_ahead
                                || next_timer > 11.0
                                || must_return_for_split
                                || (imminent && still_behind)
                            {
                                next_state = TrafficBehaviorState::Returning;
                                desired_lat = 0.0;
                            }
                        }
                    } else {
                        next_state = TrafficBehaviorState::Returning;
                        desired_lat = 0.0;
                    }
                }
                TrafficBehaviorState::Returning => {
                    desired_lat = 0.0;
                    if cur.lateral_offset.abs() < 0.12 {
                        next_state = TrafficBehaviorState::Cruising;
                        next_target_id = None;
                        next_timer = 0.0;
                    }
                }
            }

            // Car-following speed cap in our active lateral band
            let follow_constraint = if next_state == TrafficBehaviorState::Overtaking {
                front_in_band.map(|(fi, g)| (&snap[fi], g))
            } else {
                front_in_lane
                    .map(|(fi, g, _)| (&snap[fi], g))
                    .or_else(|| front_in_band.map(|(fi, g)| (&snap[fi], g)))
            };
            if let Some((front, gap)) = follow_constraint {
                let desired_gap = min_gap + cur.speed * 0.55;
                if gap <= min_gap * 0.85 {
                    target_speed = target_speed.min((front.speed * 0.8).max(0.0));
                } else if gap < desired_gap * 1.4 {
                    let ratio = ((gap - min_gap * 0.85) / (desired_gap * 1.4 - min_gap * 0.85))
                        .clamp(0.0, 1.0);
                    let safe_spd = front.speed + (target_speed - front.speed).max(0.0) * ratio;
                    target_speed = target_speed.min(safe_spd.max(0.0));
                }
            }

            // Integrate longitudinal speed, lateral steering, and advance `s`
            let veh = &mut self.vehicles[i];
            let max_accel = if next_state == TrafficBehaviorState::Overtaking {
                5.8
            } else {
                4.2
            };
            let max_decel = 9.5;
            if target_speed > veh.speed {
                veh.speed = (veh.speed + max_accel * dt).min(target_speed);
            } else {
                veh.speed = (veh.speed - max_decel * dt).max(target_speed).max(0.0);
            }
            veh.s += veh.speed * dt;

            let lat_diff = desired_lat - veh.lateral_offset;
            let lat_rate = (lane_width * 0.85).max(2.6);
            let lat_step = lat_diff.clamp(-lat_rate * dt, lat_rate * dt);
            veh.lateral_offset += lat_step;
            veh.lateral_vel = lat_step / dt;
            if (desired_lat - veh.lateral_offset).abs() < 0.02 {
                veh.lateral_offset = desired_lat;
                veh.lateral_vel = 0.0;
            }

            veh.state = next_state;
            veh.overtake_target_id = next_target_id;
            veh.overtake_timer = next_timer;

            // Transition smoothly from Approach into `active_target_branch` at the split point!
            if veh.branch == VehicleBranch::Approach && veh.s >= cor.approach_poly.total_len {
                veh.s -= cor.approach_poly.total_len;
                veh.branch = active_target_branch;
                veh.overtake_target_id = None;
                if veh.state == TrafficBehaviorState::Overtaking {
                    veh.state = TrafficBehaviorState::Returning;
                }
            }
        }
    }

    fn step(&mut self, dt: f64) {
        // 1. Apply any deterministic diversion toggle events reached at `self.sim_time`
        while self.next_event_cursor < self.applied_events.len()
            && self.sim_time + 1e-6 >= self.applied_events[self.next_event_cursor].0
        {
            self.is_diverted = self.applied_events[self.next_event_cursor].1;
            self.next_event_cursor += 1;
        }

        // 2. Continuous spawning on every corridor's approach lane
        let n_corridors = self.corridors.len();
        for c_idx in 0..n_corridors {
            self.corridors[c_idx].spawn_cooldown -= dt;
            if self.corridors[c_idx].spawn_cooldown <= 0.0 {
                self.try_spawn_on_corridor(c_idx);
                self.corridors[c_idx].spawn_cooldown = match self.cfg.flow_mode {
                    DiversionFlowMode::Convoy => {
                        let jitter = self.rng.range_f64(0.85, 1.15);
                        (self.cfg.spawn_interval_sec * jitter).max(0.3)
                    }
                    DiversionFlowMode::FreeFlow => self
                        .rng
                        .range_f64(
                            self.cfg.spawn_interval_min_sec,
                            self.cfg.spawn_interval_max_sec,
                        )
                        .max(0.3),
                };
            }
        }

        // 3. Update motion branch-by-branch per corridor (`Convoy` vs `FreeFlow` random speed + overtaking)
        let active_target_branch = if self.is_diverted {
            VehicleBranch::Divert
        } else {
            VehicleBranch::Main
        };

        if self.cfg.flow_mode == DiversionFlowMode::FreeFlow {
            self.step_freeflow(dt, active_target_branch);
        } else {
            let max_accel = 8.0;
            let max_decel = 12.0;

            for c_idx in 0..n_corridors {
                let app_len = self.corridors[c_idx].approach_poly.total_len;

                // Update `Main` and `Divert` branches first, tracking rear-most vehicle of each branch
                let mut rear_of_active_branch: Option<(f64, f64, f64)> = None;
                for branch in [VehicleBranch::Main, VehicleBranch::Divert] {
                    let cruise = match branch {
                        VehicleBranch::Main => self.cfg.normal_speed_mps,
                        VehicleBranch::Divert => self.cfg.divert_speed_mps,
                        VehicleBranch::Approach => self.cfg.normal_speed_mps,
                    };
                    let mut indices: Vec<usize> = self
                        .vehicles
                        .iter()
                        .enumerate()
                        .filter_map(|(i, v)| {
                            (v.corridor_idx == c_idx && v.branch == branch).then_some(i)
                        })
                        .collect();
                    indices.sort_by(|&a, &b| {
                        self.vehicles[b]
                            .s
                            .partial_cmp(&self.vehicles[a].s)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let mut front_state: Option<(f64, f64, f64)> = None;
                    for idx in indices {
                        let v = &mut self.vehicles[idx];
                        let mut target_spd = cruise;
                        if let Some((front_s, front_len, front_spd)) = front_state {
                            let desired_rear_s =
                                front_s - 0.5 * (front_len + v.length_m) - self.cfg.min_safe_gap_m;
                            let dist = (desired_rear_s - v.s).max(0.0);
                            let follow_spd = (front_spd * front_spd + 2.0 * 10.0 * dist).sqrt();
                            target_spd = target_spd.min(follow_spd);
                        }
                        if target_spd > v.speed {
                            v.speed = (v.speed + max_accel * dt).min(target_spd);
                        } else {
                            v.speed = (v.speed - max_decel * dt).max(target_spd).max(0.0);
                        }
                        v.s += v.speed * dt;
                        if let Some((front_s, front_len, front_spd)) = front_state {
                            let hard_max = front_s - 0.5 * (front_len + v.length_m) - 0.35;
                            if v.s > hard_max {
                                v.s = hard_max.max(0.0);
                                v.speed = v.speed.min(front_spd);
                            }
                        }
                        front_state = Some((v.s, v.length_m, v.speed));
                    }
                    if branch == active_target_branch
                        && let Some((rear_s, rear_len, rear_spd)) = front_state
                    {
                        rear_of_active_branch = Some((app_len + rear_s, rear_len, rear_spd));
                    }
                }

                // Now update `Approach` vehicles front-to-back, transitioning smoothly into `active_target_branch` at `app_len`
                let mut app_indices: Vec<usize> = self
                    .vehicles
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| {
                        (v.corridor_idx == c_idx && v.branch == VehicleBranch::Approach)
                            .then_some(i)
                    })
                    .collect();
                app_indices.sort_by(|&a, &b| {
                    self.vehicles[b]
                        .s
                        .partial_cmp(&self.vehicles[a].s)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let mut front_state = rear_of_active_branch;
                for idx in app_indices {
                    let v = &mut self.vehicles[idx];
                    let mut target_spd = self.cfg.normal_speed_mps;
                    if self.is_diverted && v.s > (app_len - 18.0).max(0.0) {
                        target_spd = target_spd.min(self.cfg.divert_speed_mps);
                    }
                    if let Some((front_s, front_len, front_spd)) = front_state {
                        let desired_rear_s =
                            front_s - 0.5 * (front_len + v.length_m) - self.cfg.min_safe_gap_m;
                        let dist = (desired_rear_s - v.s).max(0.0);
                        let follow_spd = (front_spd * front_spd + 2.0 * 10.0 * dist).sqrt();
                        target_spd = target_spd.min(follow_spd);
                    }
                    if target_spd > v.speed {
                        v.speed = (v.speed + max_accel * dt).min(target_spd);
                    } else {
                        v.speed = (v.speed - max_decel * dt).max(target_spd).max(0.0);
                    }
                    v.s += v.speed * dt;
                    if let Some((front_s, front_len, front_spd)) = front_state {
                        let hard_max = front_s - 0.5 * (front_len + v.length_m) - 0.35;
                        if v.s > hard_max {
                            v.s = hard_max.max(0.0);
                            v.speed = v.speed.min(front_spd);
                        }
                    }
                    let effective_s_for_next = v.s;
                    let len_for_next = v.length_m;
                    let spd_for_next = v.speed;

                    if v.s >= app_len {
                        v.branch = active_target_branch;
                        v.s -= app_len;
                    }
                    front_state = Some((effective_s_for_next, len_for_next, spd_for_next));
                }
            }
        }

        // 4. Despawn vehicles that have exited their `Main` or `Divert` branch
        let corridors = &self.corridors;
        self.vehicles.retain(|v| match v.branch {
            VehicleBranch::Approach => true,
            VehicleBranch::Main | VehicleBranch::Divert => corridors
                .get(v.corridor_idx)
                .is_some_and(|c| v.s < c.poly_for(v.branch).total_len + 0.5 * v.length_m),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_anchor() -> GeoAnchor {
        GeoAnchor::new(115.2167, -8.6705)
    }

    fn sample_corridor(anchor: GeoAnchor) -> TrafficDiversionCorridor {
        let p0 = anchor.local_to_lng_lat_alt([-80.0, 0.0, 0.0]);
        let p_split = anchor.local_to_lng_lat_alt([0.0, 0.0, 0.0]);
        let p_main = anchor.local_to_lng_lat_alt([100.0, 0.0, 0.0]);
        let p_div = anchor.local_to_lng_lat_alt([0.0, 0.0, 100.0]);
        TrafficDiversionCorridor {
            id: Uuid::new_v4(),
            name: "Koridor 1".into(),
            approach_geom: vec![[p0.0, p0.1, p0.2], [p_split.0, p_split.1, p_split.2]],
            main_geom: vec![[p_split.0, p_split.1, p_split.2], [p_main.0, p_main.1, p_main.2]],
            divert_geom: vec![[p_split.0, p_split.1, p_split.2], [p_div.0, p_div.1, p_div.2]],
            enabled: true,
        }
    }

    fn sample_pool() -> Vec<TrafficVehicleEntry> {
        vec![TrafficVehicleEntry {
            model_path: "models/car.glb".into(),
            name: "Sedan".into(),
            scale: 1.0,
            length_m: 4.5,
            weight: 1.0,
            rot_y_offset_deg: 0.0,
        }]
    }

    #[test]
    fn diversion_requires_all_three_polylines() {
        let anchor = test_anchor();
        let mut cor = sample_corridor(anchor);
        cor.divert_geom.clear();
        let cfg = TrafficDiversionConfig {
            corridors: vec![cor],
            vehicle_pool: sample_pool(),
            ..Default::default()
        };
        assert!(!cfg.has_playable_setup());
        assert_eq!(cfg.incomplete_corridor_labels().len(), 1);
    }

    #[test]
    fn toggle_diversion_routes_new_vehicles_to_alternate_while_main_finishes_cleanly() {
        let anchor = test_anchor();
        let cfg = TrafficDiversionConfig {
            corridors: vec![sample_corridor(anchor)],
            vehicle_pool: sample_pool(),
            normal_speed_mps: 10.0,
            divert_speed_mps: 8.0,
            start_diverted: false,
            prefill_initial: true,
            seed: 42,
            ..Default::default()
        };
        let mut engine = TrafficDiversionSimEngine::new(&cfg, anchor);
        engine.advance_to(3.0, &[]);
        let st1 = engine.live_status();
        assert!(!st1.is_diverted);
        assert!(st1.on_main > 0);
        assert_eq!(st1.on_divert, 0);

        // Toggle "Alihkan Arus" at t = 3.0s
        let events = vec![(3.0, true)];
        engine.advance_to(8.0, &events);
        let st2 = engine.live_status();
        assert!(st2.is_diverted);
        assert!(st2.on_divert > 0, "Vehicles must divert onto alternate lane");
        assert!(
            st2.on_main > 0,
            "Vehicles already on main lane must keep moving cleanly until exit"
        );

        // After enough time, old main-lane vehicles completely clear out while divert stays active
        engine.advance_to(16.0, &events);
        let st3 = engine.live_status();
        assert_eq!(st3.on_main, 0, "Main lane must clear out completely once diverted");
        assert!(st3.on_divert > 0);

        // Toggle back "Normalkan Arus" at t = 16.0s -> vehicles return to main lane!
        let events2 = vec![(3.0, true), (16.0, false)];
        engine.advance_to(22.0, &events2);
        let st4 = engine.live_status();
        assert!(!st4.is_diverted);
        assert!(st4.on_main > 0, "Normalizing must route vehicles back onto main lane");
    }

    #[test]
    fn diversion_is_100_percent_deterministic_for_layar_besar() {
        let anchor = test_anchor();
        let cfg = TrafficDiversionConfig {
            corridors: vec![sample_corridor(anchor)],
            vehicle_pool: sample_pool(),
            seed: 888,
            ..Default::default()
        };
        let events = vec![(4.0, true), (11.5, false)];
        let mut op = TrafficDiversionSimEngine::new(&cfg, anchor);
        let mut big = TrafficDiversionSimEngine::new(&cfg, anchor);
        op.advance_to(18.0, &events);
        big.advance_to(18.0, &events);
        let pa = op.active_poses();
        let pb = big.active_poses();
        assert_eq!(pa.len(), pb.len());
        for (a, b) in pa.iter().zip(pb.iter()) {
            assert_eq!(a.vehicle_id, b.vehicle_id);
            assert!((a.x - b.x).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4);
        }
    }

    #[test]
    fn freeflow_mode_overtakes_with_courtesy_slowdown_and_diverts_cleanly() {
        let anchor = test_anchor();
        // Long 220m approach lane so a faster rear car has plenty of room to overtake before the split!
        let p0 = anchor.local_to_lng_lat_alt([-220.0, 0.0, 0.0]);
        let p_split = anchor.local_to_lng_lat_alt([0.0, 0.0, 0.0]);
        let p_main = anchor.local_to_lng_lat_alt([140.0, 0.0, 0.0]);
        let p_div = anchor.local_to_lng_lat_alt([0.0, 0.0, 140.0]);
        let cor = TrafficDiversionCorridor {
            id: Uuid::new_v4(),
            name: "Koridor 1".into(),
            approach_geom: vec![[p0.0, p0.1, p0.2], [p_split.0, p_split.1, p_split.2]],
            main_geom: vec![[p_split.0, p_split.1, p_split.2], [p_main.0, p_main.1, p_main.2]],
            divert_geom: vec![[p_split.0, p_split.1, p_split.2], [p_div.0, p_div.1, p_div.2]],
            enabled: true,
        };
        let cfg = TrafficDiversionConfig {
            corridors: vec![cor],
            vehicle_pool: sample_pool(),
            flow_mode: DiversionFlowMode::FreeFlow,
            min_speed_mps: 6.0,
            max_speed_mps: 16.0,
            allow_overtaking: true,
            front_yield_slowdown_pct: 25.0,
            prefill_initial: false,
            seed: 77,
            ..Default::default()
        };
        let mut engine = TrafficDiversionSimEngine::new(&cfg, anchor);
        // Seed a slow car ahead and a fast car right behind on Approach
        let s1 = engine.corridors[0].approach_poly.sample(45.0);
        let s2 = engine.corridors[0].approach_poly.sample(22.0);
        engine.vehicles.push(DiversionVehicle {
            id: 1,
            corridor_idx: 0,
            pool_idx: 0,
            branch: VehicleBranch::Approach,
            s: 45.0,
            speed: 7.0,
            desired_speed: 7.0,
            length_m: 4.5,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: s1.x,
            world_z: s1.z,
            dir_x: s1.fx,
            dir_z: s1.fz,
        });
        engine.vehicles.push(DiversionVehicle {
            id: 2,
            corridor_idx: 0,
            pool_idx: 0,
            branch: VehicleBranch::Approach,
            s: 22.0,
            speed: 15.0,
            desired_speed: 15.0,
            length_m: 4.5,
            lateral_offset: 0.0,
            lateral_vel: 0.0,
            state: TrafficBehaviorState::Cruising,
            overtake_target_id: None,
            overtake_timer: 0.0,
            being_overtaken: false,
            world_x: s2.x,
            world_z: s2.z,
            dir_x: s2.fx,
            dir_z: s2.fz,
        });
        engine.next_vehicle_id = 3;

        let mut saw_overtake = false;
        let mut saw_front_slowdown = false;
        for step_i in 1..=360 {
            let t = step_i as f64 * FIXED_DT;
            engine.advance_to(t, &[(9.0, true)]);
            if let Some(fast) = engine.vehicles.iter().find(|v| v.id == 2)
                && fast.state == TrafficBehaviorState::Overtaking
            {
                saw_overtake = true;
            }
            if let Some(slow) = engine.vehicles.iter().find(|v| v.id == 1)
                && slow.being_overtaken
                && slow.speed < 6.4
            {
                saw_front_slowdown = true;
            }
        }
        assert!(saw_overtake, "Fast car must overtake slow car in FreeFlow diversion mode");
        assert!(
            saw_front_slowdown,
            "Slow front car must temporarily yield speed while being overtaken"
        );
        // Advance past split with diversion active at t=9.0 -> fast car must divert onto alternate branch!
        engine.advance_to(18.0, &[(9.0, true)]);
        assert!(
            engine.live_status().on_divert > 0,
            "FreeFlow mode must cleanly route vehicles onto divert_geom after Alihkan Arus"
        );
    }
}
