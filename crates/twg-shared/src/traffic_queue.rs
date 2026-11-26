//! Queue & Clearance Intersection Traffic Simulation (`traffic_queue`).
//!
//! Models paired corridors where each approach/queue lane (`queue_geom`) **must** have a matching
//! clearance lane (`clear_geom`). Vehicles spawn in convoy order at constant speed (no overtaking),
//! queue neatly behind the stop line while other corridors take their timed turn, continue all the
//! way down their `clear_geom` once committed past the stop line, and rapidly drain to zero when
//! the operator triggers the live **"Lancar"** clearance command.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    geo::GeoAnchor,
    models::Anim,
    traffic_anim::{TrafficBehaviorState, TrafficVehicleEntry, TrafficVehiclePose},
};

pub const TRAFFIC_QUEUE_ANIM_TYPE: &str = "traffic_queue";
pub const TRAFFIC_GRIDLOCK_ANIM_TYPE: &str = "traffic_gridlock";
pub const TRAFFIC_MANUAL_ANIM_TYPE: &str = "traffic_manual";

/// Live operator command for `Kontrol Simpang Manual` (`traffic_manual`), synced deterministically
/// via WebSocket to Layar Besar with its exact `elapsed_sec` timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum QueueManualAction {
    /// Open ONLY the corridor with original index `corridor_idx` (all others hold RED).
    OpenCorridor { corridor_idx: usize },
    /// Hold ALL corridors RED (vehicles already in `clear_geom` finish exiting cleanly).
    HoldAll,
    /// Activate live **"Lancar"** clearance (stop spawning globally, boost to `clear_speed_mps`)
    /// and drain ONLY `corridor_idx` without auto-switching to other corridors.
    DrainCorridor { corridor_idx: usize },
}

/// Strategy used when the operator clicks the live **"Lancar"** button to drain remaining vehicles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueClearanceMode {
    /// Drain all remaining queued vehicles in the active corridor first before switching to the next.
    #[default]
    DrainCorridor,
    /// Keep alternating between corridors by `turn_duration_sec` at `clear_speed_mps`
    /// (switching early as soon as a corridor empties) until all corridors reach 0 vehicles.
    AlternatingTimed,
}

impl QueueClearanceMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::DrainCorridor => "Habiskan Per Jalur",
            Self::AlternatingTimed => "Bergantian Per Durasi",
        }
    }
}

/// One paired intersection corridor: an approach/queue lane + its mandatory clearance lane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficQueueCorridor {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    #[serde(default = "default_corridor_name")]
    pub name: String,
    /// Approach / waiting lane (`[lng, lat, alt]`): vehicles spawn at start, drive in convoy,
    /// and stop cleanly at the end when this corridor is red.
    #[serde(default)]
    pub queue_geom: Vec<[f64; 3]>,
    /// Mandatory clearance lane (`[lng, lat, alt]`): continuation past the stop line.
    /// Vehicles already inside `clear_geom` always continue to the end even when the corridor turns red.
    #[serde(default)]
    pub clear_geom: Vec<[f64; 3]>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional per-corridor cap on simultaneous vehicles.
    #[serde(default)]
    pub max_vehicles_override: Option<u32>,
    /// Optional per-corridor green duration override (seconds).
    #[serde(default)]
    pub turn_duration_override_sec: Option<f64>,
}

fn default_corridor_name() -> String {
    "Jalur A".into()
}

fn default_true() -> bool {
    true
}

impl TrafficQueueCorridor {
    pub fn new(name: impl Into<String>, queue_geom: Vec<[f64; 3]>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            queue_geom,
            clear_geom: Vec::new(),
            enabled: true,
            max_vehicles_override: None,
            turn_duration_override_sec: None,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.queue_geom.len() >= 2 && self.clear_geom.len() >= 2
    }

    /// Generates an automatic lettered default name (`Jalur A`, `Jalur B`, ..., `Jalur Z`, `Jalur 27`).
    pub fn default_label_for_index(idx: usize) -> String {
        if idx < 26 {
            let ch = (b'A' + idx as u8) as char;
            format!("Jalur {ch}")
        } else {
            format!("Jalur {}", idx + 1)
        }
    }
}

/// Complete configuration for an `Antrean & Urai Simpang` (`traffic_queue`) animation,
/// persisted inside `anim.data["traffic_queue"]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficQueueConfig {
    #[serde(default)]
    pub corridors: Vec<TrafficQueueCorridor>,
    #[serde(default)]
    pub vehicle_pool: Vec<TrafficVehicleEntry>,
    /// Constant convoy speed during normal alternating turns (m/s).
    #[serde(default = "default_convoy_speed_mps")]
    pub convoy_speed_mps: f64,
    /// Boosted speed when live **"Lancar"** clearance mode is triggered (m/s).
    #[serde(default = "default_clear_speed_mps")]
    pub clear_speed_mps: f64,
    /// Duration of each corridor's green turn during normal alternating mode (seconds).
    #[serde(default = "default_turn_duration_sec")]
    pub turn_duration_sec: f64,
    /// Brief all-red safety pause when switching turns so the intersection clears cleanly (seconds).
    #[serde(default = "default_switch_pause_sec")]
    pub switch_pause_sec: f64,
    /// Maximum simultaneous vehicles per corridor during normal phase.
    #[serde(default = "default_max_vehicles_per_corridor")]
    pub max_vehicles_per_corridor: u32,
    /// Bumper-to-bumper safe clearance during convoy travel & queued stopping (meters).
    #[serde(default = "default_min_safe_gap_m")]
    pub min_safe_gap_m: f64,
    /// Average time gap between consecutive spawned vehicles per corridor (seconds).
    #[serde(default = "default_spawn_interval_sec")]
    pub spawn_interval_sec: f64,
    /// When `true`, pre-populates vehicles near each corridor's stop line right at `t = 0`
    /// so the intersection immediately looks active on Play instead of starting empty.
    #[serde(default)]
    pub prefill_initial_queue: bool,
    /// Strategy used when live **"Lancar"** clearance mode is triggered.
    #[serde(default)]
    pub clearance_mode: QueueClearanceMode,
    /// When `true` (`Macet Total & Urai` / `traffic_gridlock`), ALL corridors remain RED/stuck
    /// during the initial phase until the operator clicks **"Lancar"**.
    #[serde(default)]
    pub gridlock_mode: bool,
    /// When `true` (`Kontrol Simpang Manual` / `traffic_manual`), turns do NOT rotate automatically
    /// on a timer — the operator explicitly opens/holds corridors via live buttons until **"Lancar"**.
    #[serde(default)]
    pub manual_control_mode: bool,
    /// Deterministic PRNG seed so Operator & Layar Besar always match identically.
    #[serde(default = "default_seed")]
    pub seed: u64,
}

fn default_convoy_speed_mps() -> f64 {
    8.0
}
fn default_clear_speed_mps() -> f64 {
    15.0
}
fn default_turn_duration_sec() -> f64 {
    12.0
}
fn default_switch_pause_sec() -> f64 {
    1.5
}
fn default_max_vehicles_per_corridor() -> u32 {
    8
}
fn default_min_safe_gap_m() -> f64 {
    2.5
}
fn default_spawn_interval_sec() -> f64 {
    1.6
}
fn default_seed() -> u64 {
    20_261_127
}

impl Default for TrafficQueueConfig {
    fn default() -> Self {
        Self {
            corridors: Vec::new(),
            vehicle_pool: Vec::new(),
            convoy_speed_mps: default_convoy_speed_mps(),
            clear_speed_mps: default_clear_speed_mps(),
            turn_duration_sec: default_turn_duration_sec(),
            switch_pause_sec: default_switch_pause_sec(),
            max_vehicles_per_corridor: default_max_vehicles_per_corridor(),
            min_safe_gap_m: default_min_safe_gap_m(),
            spawn_interval_sec: default_spawn_interval_sec(),
            prefill_initial_queue: false,
            clearance_mode: QueueClearanceMode::default(),
            gridlock_mode: false,
            manual_control_mode: false,
            seed: default_seed(),
        }
    }
}

impl TrafficQueueConfig {
    pub fn sanitized(&self) -> Self {
        let convoy = self.convoy_speed_mps.clamp(1.5, 40.0);
        let clear = self.clear_speed_mps.clamp(convoy, 60.0);
        Self {
            corridors: self.corridors.clone(),
            vehicle_pool: self.vehicle_pool.clone(),
            convoy_speed_mps: convoy,
            clear_speed_mps: clear,
            turn_duration_sec: self.turn_duration_sec.clamp(2.0, 180.0),
            switch_pause_sec: self.switch_pause_sec.clamp(0.0, 15.0),
            max_vehicles_per_corridor: self.max_vehicles_per_corridor.clamp(1, 80),
            min_safe_gap_m: self.min_safe_gap_m.clamp(0.8, 25.0),
            spawn_interval_sec: self.spawn_interval_sec.clamp(0.3, 20.0),
            prefill_initial_queue: self.prefill_initial_queue,
            clearance_mode: self.clearance_mode,
            gridlock_mode: self.gridlock_mode,
            manual_control_mode: self.manual_control_mode,
            seed: if self.seed == 0 { default_seed() } else { self.seed },
        }
    }

    /// Names of enabled corridors missing either their approach lane or mandatory clearance lane.
    pub fn incomplete_corridor_names(&self) -> Vec<String> {
        self.corridors
            .iter()
            .filter(|c| c.enabled && !c.is_complete())
            .map(|c| c.name.clone())
            .collect()
    }

    /// Playable only when there is at least one enabled corridor, **every** enabled corridor has
    /// both `queue_geom` (>= 2 pts) and its mandatory `clear_geom` (>= 2 pts), and the vehicle pool
    /// is non-empty.
    pub fn has_playable_setup(&self) -> bool {
        let enabled: Vec<&TrafficQueueCorridor> =
            self.corridors.iter().filter(|c| c.enabled).collect();
        !enabled.is_empty()
            && enabled.iter().all(|c| c.is_complete())
            && !self.vehicle_pool.is_empty()
    }

    /// Deterministic hash signature of config + anchor so the runtime engine rebuilds cleanly
    /// whenever corridors, vehicle pool, or parameters change.
    pub fn config_signature(&self, anchor: GeoAnchor) -> u64 {
        let mut h = DefaultHasher::new();
        anchor.origin_mx.to_bits().hash(&mut h);
        anchor.origin_my.to_bits().hash(&mut h);
        self.convoy_speed_mps.to_bits().hash(&mut h);
        self.clear_speed_mps.to_bits().hash(&mut h);
        self.turn_duration_sec.to_bits().hash(&mut h);
        self.switch_pause_sec.to_bits().hash(&mut h);
        self.max_vehicles_per_corridor.hash(&mut h);
        self.min_safe_gap_m.to_bits().hash(&mut h);
        self.spawn_interval_sec.to_bits().hash(&mut h);
        self.prefill_initial_queue.hash(&mut h);
        (self.clearance_mode as u8).hash(&mut h);
        self.gridlock_mode.hash(&mut h);
        self.manual_control_mode.hash(&mut h);
        self.seed.hash(&mut h);
        self.corridors.len().hash(&mut h);
        for c in &self.corridors {
            c.id.hash(&mut h);
            c.enabled.hash(&mut h);
            c.max_vehicles_override.hash(&mut h);
            c.turn_duration_override_sec.map(f64::to_bits).hash(&mut h);
            c.queue_geom.len().hash(&mut h);
            for pt in &c.queue_geom {
                pt[0].to_bits().hash(&mut h);
                pt[1].to_bits().hash(&mut h);
                pt[2].to_bits().hash(&mut h);
            }
            c.clear_geom.len().hash(&mut h);
            for pt in &c.clear_geom {
                pt[0].to_bits().hash(&mut h);
                pt[1].to_bits().hash(&mut h);
                pt[2].to_bits().hash(&mut h);
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

pub fn is_traffic_gridlock_anim(anim: &Anim) -> bool {
    anim.anim_type.eq_ignore_ascii_case(TRAFFIC_GRIDLOCK_ANIM_TYPE)
}

pub fn is_traffic_manual_anim(anim: &Anim) -> bool {
    anim.anim_type.eq_ignore_ascii_case(TRAFFIC_MANUAL_ANIM_TYPE)
}

/// Returns `true` for `Antrean & Urai Simpang` (`traffic_queue`),
/// `Macet Total & Urai` (`traffic_gridlock`), and `Kontrol Simpang Manual` (`traffic_manual`),
/// which share paired queue+clearance corridors and the deterministic queue/clearance engine.
pub fn is_traffic_queue_anim(anim: &Anim) -> bool {
    anim.anim_type.eq_ignore_ascii_case(TRAFFIC_QUEUE_ANIM_TYPE)
        || is_traffic_gridlock_anim(anim)
        || is_traffic_manual_anim(anim)
}

pub fn parse_traffic_queue_config(anim: &Anim) -> TrafficQueueConfig {
    let mut cfg: TrafficQueueConfig = anim
        .data
        .get("traffic_queue")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if is_traffic_gridlock_anim(anim) {
        cfg.gridlock_mode = true;
        cfg.manual_control_mode = false;
    } else if is_traffic_manual_anim(anim) {
        cfg.manual_control_mode = true;
        cfg.gridlock_mode = false;
    }
    if cfg.corridors.is_empty() && anim.geom.len() >= 2 {
        cfg.corridors
            .push(TrafficQueueCorridor::new("Jalur A", anim.geom.clone()));
    }
    cfg.sanitized()
}

pub fn write_traffic_queue_config(anim: &mut Anim, cfg: &TrafficQueueConfig) {
    let mut clean = cfg.sanitized();
    if is_traffic_gridlock_anim(anim) {
        clean.gridlock_mode = true;
        clean.manual_control_mode = false;
    } else if is_traffic_manual_anim(anim) {
        clean.manual_control_mode = true;
        clean.gridlock_mode = false;
    }
    anim.anim_type = if clean.gridlock_mode {
        TRAFFIC_GRIDLOCK_ANIM_TYPE.to_string()
    } else if clean.manual_control_mode {
        TRAFFIC_MANUAL_ANIM_TYPE.to_string()
    } else {
        TRAFFIC_QUEUE_ANIM_TYPE.to_string()
    };
    anim.marker_id = None;
    if let Some(first) = clean
        .corridors
        .iter()
        .find(|c| c.enabled && c.queue_geom.len() >= 2)
        .or_else(|| clean.corridors.iter().find(|c| c.queue_geom.len() >= 2))
    {
        anim.geom = first.queue_geom.clone();
    } else {
        anim.geom.clear();
    }
    let mut obj = anim.data.as_object().cloned().unwrap_or_default();
    if let Ok(val) = serde_json::to_value(&clean) {
        obj.insert("traffic_queue".into(), val);
    }
    anim.data = serde_json::Value::Object(obj);
}

/// Live runtime telemetry for UI display in Play Panel and Plot Editor Preview.
#[derive(Debug, Clone, Default)]
pub struct TrafficQueueLiveStatus {
    pub active_corridor_name: Option<String>,
    /// Original corridor index in `cfg.corridors` that is currently OPEN (green), if any.
    pub active_corridor_source_idx: Option<usize>,
    pub remaining_turn_sec: f64,
    pub is_switch_pause: bool,
    pub is_all_stopped: bool,
    pub clearance_active: bool,
    pub is_completed: bool,
    pub total_vehicles: usize,
    pub queued_vehicles: usize,
    pub clearing_vehicles: usize,
    /// Per-corridor remaining vehicle counts as `(source_idx, queued_count, total_count)`.
    pub per_corridor_counts: Vec<(usize, usize, usize)>,
}

impl TrafficQueueLiveStatus {
    /// Returns `(queued_count, total_count)` for the corridor with original index `source_idx`.
    pub fn corridor_counts(&self, source_idx: usize) -> (usize, usize) {
        self.per_corridor_counts
            .iter()
            .find(|(idx, _, _)| *idx == source_idx)
            .map(|&(_, q, tot)| (q, tot))
            .unwrap_or((0, 0))
    }
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
// Compiled Corridor Geometry (Queue + Seamless Bridge + Clearance)
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

#[derive(Debug, Clone)]
struct CompiledCorridor {
    source_idx: usize,
    name: String,
    queue_poly: CompiledPoly,
    clear_poly: CompiledPoly,
    max_vehicles: u32,
    turn_duration_sec: f64,
    spawn_cooldown: f64,
}

impl CompiledCorridor {
    fn queue_len(&self) -> f64 {
        self.queue_poly.total_len
    }

    fn total_len(&self) -> f64 {
        self.queue_poly.total_len + self.clear_poly.total_len
    }

    fn sample(&self, s: f64) -> PolySample {
        if s <= self.queue_poly.total_len {
            self.queue_poly.sample(s)
        } else {
            self.clear_poly.sample(s - self.queue_poly.total_len)
        }
    }
}

#[derive(Debug, Clone)]
struct QueueVehicle {
    id: u64,
    corridor_idx: usize,
    pool_idx: usize,
    s: f64,
    speed: f64,
    length_m: f64,
    /// `true` once the vehicle's front bumper has passed the end of `queue_geom` into `clear_geom`.
    /// Committed vehicles ALWAYS keep moving to the end of `clear_geom` even when the corridor turns red.
    committed_to_clear: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum QueuePhase {
    /// Gridlock (`Macet Total & Urai`): ALL corridors are held RED until **"Lancar"** is clicked.
    AllStopped,
    NormalGreen {
        corridor_idx: usize,
        timer: f64,
        /// Becomes `true` once traffic from `corridor_idx` actually approaches or enters its stop line,
        /// so turn duration is never wasted while the corridor is still empty!
        engaged: bool,
    },
    SwitchPause {
        next_corridor_idx: usize,
        timer: f64,
        is_clearance: bool,
    },
    ClearanceDrain {
        corridor_idx: usize,
        timer: f64,
    },
    Completed,
}

/// Deterministic fixed-timestep (30 Hz) engine for `Antrean & Urai Simpang` (`traffic_queue`).
#[derive(Debug, Clone)]
pub struct TrafficQueueSimEngine {
    cfg: TrafficQueueConfig,
    corridors: Vec<CompiledCorridor>,
    vehicles: Vec<QueueVehicle>,
    rng: SplitMix64,
    next_vehicle_id: u64,
    sim_time: f64,
    /// Sub-step remainder `(target - sim_time)` in `[0, FIXED_DT]` for silky smooth 60–144 FPS rendering.
    render_alpha_sec: f64,
    phase: QueuePhase,
    clearance_active: bool,
    applied_clearance_trigger: Option<f64>,
    applied_manual_events: Vec<(f64, QueueManualAction)>,
    next_manual_cursor: usize,
}

const FIXED_DT: f64 = 1.0 / 30.0;

impl TrafficQueueSimEngine {
    pub fn new(cfg: &TrafficQueueConfig, anchor: GeoAnchor) -> Self {
        let clean = cfg.sanitized();
        let mut corridors = Vec::new();
        for (src_idx, c) in clean.corridors.iter().enumerate() {
            if !c.enabled || !c.is_complete() {
                continue;
            }
            let q_pts: Vec<[f64; 3]> = c
                .queue_geom
                .iter()
                .map(|p| {
                    let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
                    [l[0] as f64, l[1] as f64, l[2] as f64]
                })
                .collect();
            let mut cl_pts: Vec<[f64; 3]> = c
                .clear_geom
                .iter()
                .map(|p| {
                    let l = anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
                    [l[0] as f64, l[1] as f64, l[2] as f64]
                })
                .collect();
            // Seamlessly bridge from end of `queue_geom` into start of `clear_geom` if separated
            if let (Some(&q_end), Some(&cl_start)) = (q_pts.last(), cl_pts.first()) {
                let d = ((cl_start[0] - q_end[0]).powi(2) + (cl_start[2] - q_end[2]).powi(2)).sqrt();
                if d > 0.25 {
                    cl_pts.insert(0, q_end);
                }
            }
            let (Some(queue_poly), Some(clear_poly)) =
                (CompiledPoly::build(q_pts), CompiledPoly::build(cl_pts))
            else {
                continue;
            };
            corridors.push(CompiledCorridor {
                source_idx: src_idx,
                name: c.name.clone(),
                queue_poly,
                clear_poly,
                max_vehicles: c
                    .max_vehicles_override
                    .unwrap_or(clean.max_vehicles_per_corridor)
                    .clamp(1, 80),
                turn_duration_sec: c
                    .turn_duration_override_sec
                    .unwrap_or(clean.turn_duration_sec)
                    .clamp(2.0, 180.0),
                // Stagger initial spawns slightly so all corridors begin filling immediately
                spawn_cooldown: 0.12 * (src_idx as f64),
            });
        }

        let rng = SplitMix64::new(clean.seed);
        let initial_phase = if corridors.is_empty() {
            QueuePhase::Completed
        } else if clean.gridlock_mode || clean.manual_control_mode {
            QueuePhase::AllStopped
        } else {
            QueuePhase::NormalGreen {
                corridor_idx: 0,
                timer: 0.0,
                engaged: false,
            }
        };

        let mut engine = Self {
            cfg: clean,
            corridors,
            vehicles: Vec::new(),
            rng,
            next_vehicle_id: 1,
            sim_time: 0.0,
            render_alpha_sec: 0.0,
            phase: initial_phase,
            clearance_active: false,
            applied_clearance_trigger: None,
            applied_manual_events: Vec::new(),
            next_manual_cursor: 0,
        };
        if engine.cfg.prefill_initial_queue {
            engine.seed_prefill_queues();
        }
        engine
    }

    pub fn reset(&mut self) {
        self.vehicles.clear();
        self.rng = SplitMix64::new(self.cfg.seed);
        self.next_vehicle_id = 1;
        self.sim_time = 0.0;
        self.render_alpha_sec = 0.0;
        self.clearance_active = false;
        self.applied_clearance_trigger = None;
        self.applied_manual_events.clear();
        self.next_manual_cursor = 0;
        for (i, c) in self.corridors.iter_mut().enumerate() {
            c.spawn_cooldown = 0.12 * (i as f64);
        }
        self.phase = if self.corridors.is_empty() {
            QueuePhase::Completed
        } else if self.cfg.gridlock_mode || self.cfg.manual_control_mode {
            QueuePhase::AllStopped
        } else {
            QueuePhase::NormalGreen {
                corridor_idx: 0,
                timer: 0.0,
                engaged: false,
            }
        };
        if self.cfg.prefill_initial_queue {
            self.seed_prefill_queues();
        }
    }

    /// Pre-populates vehicles densely from the stop line (`queue_len`) all the way back to the
    /// start of each corridor's `queue_geom` right at `t = 0` when `prefill_initial_queue` is enabled.
    fn seed_prefill_queues(&mut self) {
        if self.cfg.vehicle_pool.is_empty() {
            return;
        }
        let n_corridors = self.corridors.len();
        for c_idx in 0..n_corridors {
            let queue_len = self.corridors[c_idx].queue_len();
            let max_v = self.corridors[c_idx].max_vehicles as usize;
            let fill_cap = max_v.max(60).min(80);
            let mut cursor_front_bumper = queue_len;
            for k in 0..fill_cap {
                let pool_idx = self.pick_pool_index();
                let length_m = self.cfg.vehicle_pool[pool_idx].length_m.clamp(1.5, 22.0);
                let s = cursor_front_bumper - 0.5 * length_m;
                if s < 0.5 * length_m {
                    break;
                }
                let id = self.next_vehicle_id;
                self.next_vehicle_id += 1;
                let init_speed = if self.cfg.gridlock_mode || self.cfg.manual_control_mode {
                    0.0
                } else if c_idx == 0 {
                    self.cfg.convoy_speed_mps * 0.65
                } else if k == 0 {
                    0.0
                } else {
                    self.cfg.convoy_speed_mps * 0.2
                };
                self.vehicles.push(QueueVehicle {
                    id,
                    corridor_idx: c_idx,
                    pool_idx,
                    s,
                    speed: init_speed,
                    length_m,
                    committed_to_clear: false,
                });
                cursor_front_bumper = s
                    - 0.5 * length_m
                    - self.cfg.min_safe_gap_m
                    - if self.cfg.gridlock_mode { 0.0 } else { 0.35 };
            }
        }
    }

    fn manual_events_compatible(
        prev: &[(f64, QueueManualAction)],
        next: &[(f64, QueueManualAction)],
        sim_time: f64,
    ) -> bool {
        if next.len() < prev.len() {
            return false;
        }
        for (a, b) in prev.iter().zip(next.iter()) {
            if (a.0 - b.0).abs() > 1e-4 || a.1 != b.1 {
                return false;
            }
        }
        for &(ev_t, _) in &next[prev.len()..] {
            if ev_t + 1e-4 < sim_time {
                return false;
            }
        }
        true
    }

    /// Deterministically advances simulation to `target_elapsed_sec` with optional live
    /// `clearance_triggered_at_sec` (exact elapsed second when **"Lancar"** was clicked).
    pub fn advance_to(&mut self, target_elapsed_sec: f64, clearance_triggered_at_sec: Option<f64>) {
        self.advance_to_with_manual(target_elapsed_sec, clearance_triggered_at_sec, &[]);
    }

    /// Deterministically advances simulation with both live **"Lancar"** trigger and live
    /// `Kontrol Simpang Manual` events (`(elapsed_sec, QueueManualAction)`).
    pub fn advance_to_with_manual(
        &mut self,
        target_elapsed_sec: f64,
        clearance_triggered_at_sec: Option<f64>,
        manual_events: &[(f64, QueueManualAction)],
    ) {
        if self.corridors.is_empty() || self.cfg.vehicle_pool.is_empty() {
            return;
        }
        let target = target_elapsed_sec.max(0.0);
        let trigger_changed = match (self.applied_clearance_trigger, clearance_triggered_at_sec) {
            (None, None) => false,
            (Some(a), Some(b)) => (a - b).abs() > 1e-4,
            (None, Some(t)) => t + 1e-4 < self.sim_time,
            (Some(_), None) => true,
        };
        let manual_changed =
            !Self::manual_events_compatible(&self.applied_manual_events, manual_events, self.sim_time);
        if target + 1e-4 < self.sim_time || trigger_changed || manual_changed {
            self.reset();
        }
        self.applied_clearance_trigger = clearance_triggered_at_sec;
        self.applied_manual_events.clear();
        self.applied_manual_events.extend_from_slice(manual_events);

        // Strictly never overshoot `target` (`sim_time + FIXED_DT <= target + 1e-9`) so
        // forward playback at ANY framerate (30–240 FPS) NEVER triggers false resets or flicker!
        let max_steps = 18_000;
        let mut steps = 0;
        while self.sim_time + FIXED_DT <= target + 1e-9 && steps < max_steps {
            self.step(FIXED_DT, clearance_triggered_at_sec);
            self.sim_time += FIXED_DT;
            steps += 1;
        }
        self.render_alpha_sec = (target - self.sim_time).clamp(0.0, FIXED_DT);
    }

    pub fn is_completely_cleared(&self) -> bool {
        self.clearance_active && self.phase == QueuePhase::Completed && self.vehicles.is_empty()
    }

    pub fn live_status(&self) -> TrafficQueueLiveStatus {
        let mut queued = 0usize;
        let mut clearing = 0usize;
        let mut per_corridor_counts: Vec<(usize, usize, usize)> = self
            .corridors
            .iter()
            .map(|c| (c.source_idx, 0usize, 0usize))
            .collect();
        for v in &self.vehicles {
            if v.committed_to_clear {
                clearing += 1;
            } else {
                queued += 1;
            }
            if let Some(slot) = per_corridor_counts.get_mut(v.corridor_idx) {
                if !v.committed_to_clear {
                    slot.1 += 1;
                }
                slot.2 += 1;
            }
        }
        let is_all_stopped = self.phase == QueuePhase::AllStopped;
        let (active_corridor_name, active_corridor_source_idx, remaining_turn_sec, is_switch_pause) =
            match self.phase {
                QueuePhase::AllStopped => (None, None, 0.0, false),
                QueuePhase::NormalGreen {
                    corridor_idx,
                    timer,
                    ..
                } => {
                    let c = self.corridors.get(corridor_idx);
                    let name = c.map(|x| x.name.clone());
                    let src_idx = c.map(|x| x.source_idx);
                    let rem = if self.cfg.manual_control_mode {
                        0.0
                    } else {
                        c.map(|x| (x.turn_duration_sec - timer).max(0.0))
                            .unwrap_or(0.0)
                    };
                    (name, src_idx, rem, false)
                }
                QueuePhase::SwitchPause {
                    next_corridor_idx,
                    timer,
                    is_clearance,
                } => {
                    let name = self
                        .corridors
                        .get(next_corridor_idx)
                        .map(|x| x.name.clone());
                    let pause_total = if is_clearance {
                        self.cfg.switch_pause_sec.min(1.0)
                    } else {
                        self.cfg.switch_pause_sec
                    };
                    (name, None, (pause_total - timer).max(0.0), true)
                }
                QueuePhase::ClearanceDrain {
                    corridor_idx,
                    timer,
                } => {
                    let c = self.corridors.get(corridor_idx);
                    let rem = if !self.cfg.manual_control_mode
                        && self.cfg.clearance_mode == QueueClearanceMode::AlternatingTimed
                    {
                        c.map(|x| (x.turn_duration_sec - timer).max(0.0))
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    (c.map(|x| x.name.clone()), c.map(|x| x.source_idx), rem, false)
                }
                QueuePhase::Completed => (None, None, 0.0, false),
            };
        TrafficQueueLiveStatus {
            active_corridor_name,
            active_corridor_source_idx,
            remaining_turn_sec,
            is_switch_pause,
            is_all_stopped,
            clearance_active: self.clearance_active,
            is_completed: self.is_completely_cleared(),
            total_vehicles: self.vehicles.len(),
            queued_vehicles: queued,
            clearing_vehicles: clearing,
            per_corridor_counts,
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
            let corridor_is_green = matches!(
                self.phase,
                QueuePhase::NormalGreen { corridor_idx, .. }
                    | QueuePhase::ClearanceDrain { corridor_idx, .. }
                    if corridor_idx == v.corridor_idx
            );
            let mut render_s = v.s + v.speed * alpha;
            if !v.committed_to_clear && !corridor_is_green {
                let stop_s = (c.queue_len() - 0.5 * v.length_m).max(0.0);
                render_s = render_s.min(stop_s);
            }
            let total_len = c.total_len();
            render_s = render_s.clamp(0.0, total_len + 0.5 * v.length_m);

            let sample = c.sample(render_s);
            let fade_in = (render_s / 2.5).clamp(0.05, 1.0);
            let fade_out = ((total_len + 0.5 * v.length_m - render_s) / 3.0).clamp(0.0, 1.0);
            let fade_scale = fade_in.min(fade_out) as f32;
            if fade_scale <= 0.01 {
                continue;
            }
            let yaw_deg = sample.fx.atan2(sample.fz).to_degrees() + entry.rot_y_offset_deg;
            let behavior = if v.committed_to_clear {
                TrafficBehaviorState::Cruising
            } else if v.speed < 0.4 {
                TrafficBehaviorState::YieldingCrossing
            } else {
                TrafficBehaviorState::Following
            };
            out.push(TrafficVehiclePose {
                vehicle_id: v.id,
                lane_source_idx: c.source_idx,
                model_path: entry.model_path.clone(),
                x: sample.x as f32,
                y: sample.y as f32,
                z: sample.z as f32,
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

    fn corridor_has_queued_vehicles(&self, corridor_idx: usize) -> bool {
        self.vehicles
            .iter()
            .any(|v| v.corridor_idx == corridor_idx && !v.committed_to_clear)
    }

    /// Returns `true` when `corridor_idx` has a vehicle close enough to (or waiting at) its stop line
    /// to realistically require a green phase (actuated signal detector).
    fn corridor_has_approaching_or_queued(&self, corridor_idx: usize) -> bool {
        let Some(cor) = self.corridors.get(corridor_idx) else {
            return false;
        };
        let queue_len = cor.queue_len();
        let detect_zone = (queue_len * 0.45).clamp(15.0, 28.0);
        let threshold_s = (queue_len - detect_zone).max(0.0);
        self.vehicles.iter().any(|v| {
            v.corridor_idx == corridor_idx
                && !v.committed_to_clear
                && (v.s + 0.5 * v.length_m) >= threshold_s
        })
    }

    /// Finds the next OTHER corridor (excluding `exclude_idx`) that currently has a vehicle
    /// approaching or waiting at its stop line.
    fn find_next_other_demanding_corridor(&self, exclude_idx: usize) -> Option<usize> {
        let n = self.corridors.len();
        if n < 2 {
            return None;
        }
        for offset in 1..n {
            let idx = (exclude_idx + offset) % n;
            if self.corridor_has_approaching_or_queued(idx) {
                return Some(idx);
            }
        }
        None
    }

    fn find_next_queued_corridor(&self, start_idx: usize) -> Option<usize> {
        let n = self.corridors.len();
        if n == 0 {
            return None;
        }
        for offset in 0..n {
            let idx = (start_idx + offset) % n;
            if self.corridor_has_queued_vehicles(idx) {
                return Some(idx);
            }
        }
        None
    }

    fn try_spawn_on_corridor(&mut self, corridor_idx: usize) {
        let base_max = self.corridors[corridor_idx].max_vehicles as usize;
        let queue_cap = if self.cfg.prefill_initial_queue {
            base_max.max(60).min(80)
        } else {
            base_max
        };
        // Count ONLY vehicles still inside `queue_geom` (`!committed_to_clear`) so vehicles
        // driving away on `clear_geom` NEVER stall spawning or leave the approach lane empty!
        let queued_cnt = self
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == corridor_idx && !v.committed_to_clear)
            .count();
        if queued_cnt >= queue_cap {
            return;
        }
        // Soft safety ceiling for extraordinarily long clearance lanes
        let total_corridor_cnt = self
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == corridor_idx)
            .count();
        if total_corridor_cnt >= queue_cap.saturating_mul(3).min(180) {
            return;
        }

        let pool_idx = self.pick_pool_index();
        let length_m = self.cfg.vehicle_pool[pool_idx].length_m.clamp(1.5, 22.0);
        let queue_len = self.corridors[corridor_idx].queue_len();
        let spawn_s = (0.5 * length_m).min((queue_len * 0.25).max(0.1));
        let min_clear_back = spawn_s + 0.5 * length_m + self.cfg.min_safe_gap_m;

        let entrance_blocked = self.vehicles.iter().any(|v| {
            v.corridor_idx == corridor_idx && (v.s - 0.5 * v.length_m) < min_clear_back
        });
        if entrance_blocked {
            return;
        }

        let id = self.next_vehicle_id;
        self.next_vehicle_id += 1;

        self.vehicles.push(QueueVehicle {
            id,
            corridor_idx,
            pool_idx,
            s: spawn_s,
            speed: self.cfg.convoy_speed_mps,
            length_m,
            committed_to_clear: false,
        });
    }

    fn activate_clearance_mode(&mut self) {
        self.clearance_active = true;
        let preferred_start = match self.phase {
            QueuePhase::AllStopped | QueuePhase::Completed => 0,
            QueuePhase::NormalGreen { corridor_idx, .. }
            | QueuePhase::ClearanceDrain { corridor_idx, .. } => corridor_idx,
            QueuePhase::SwitchPause {
                next_corridor_idx, ..
            } => next_corridor_idx,
        };
        if let Some(c_idx) = self.find_next_queued_corridor(preferred_start) {
            self.phase = QueuePhase::ClearanceDrain {
                corridor_idx: c_idx,
                timer: 0.0,
            };
        } else if self.vehicles.is_empty() {
            self.phase = QueuePhase::Completed;
        } else {
            // No vehicles left in any queue_geom; just let remaining vehicles finish clear_geom
            self.phase = QueuePhase::ClearanceDrain {
                corridor_idx: preferred_start.min(self.corridors.len().saturating_sub(1)),
                timer: 0.0,
            };
        }
    }

    fn step(&mut self, dt: f64, clearance_triggered_at_sec: Option<f64>) {
        // 1a. Apply deterministic live manual officer commands (`Kontrol Simpang Manual`)
        if self.cfg.manual_control_mode {
            while self.next_manual_cursor < self.applied_manual_events.len()
                && self.sim_time + 1e-6 >= self.applied_manual_events[self.next_manual_cursor].0
            {
                let act = self.applied_manual_events[self.next_manual_cursor].1;
                self.next_manual_cursor += 1;
                match act {
                    QueueManualAction::HoldAll => {
                        if !self.clearance_active {
                            self.phase = QueuePhase::AllStopped;
                        }
                    }
                    QueueManualAction::OpenCorridor { corridor_idx } => {
                        if !self.clearance_active
                            && let Some(compiled_idx) = self
                                .corridors
                                .iter()
                                .position(|c| c.source_idx == corridor_idx)
                        {
                            self.phase = QueuePhase::NormalGreen {
                                corridor_idx: compiled_idx,
                                timer: 0.0,
                                engaged: true,
                            };
                        }
                    }
                    QueueManualAction::DrainCorridor { corridor_idx } => {
                        self.clearance_active = true;
                        if let Some(compiled_idx) = self
                            .corridors
                            .iter()
                            .position(|c| c.source_idx == corridor_idx)
                        {
                            self.phase = QueuePhase::ClearanceDrain {
                                corridor_idx: compiled_idx,
                                timer: 0.0,
                            };
                        } else if self.vehicles.is_empty() {
                            self.phase = QueuePhase::Completed;
                        }
                    }
                }
            }
        }

        // 1b. Activate live "Lancar" clearance mode at exact deterministic trigger timestamp
        if !self.clearance_active
            && let Some(trig) = clearance_triggered_at_sec
            && self.sim_time + 1e-6 >= trig
        {
            self.activate_clearance_mode();
        }

        // 2. Spawn vehicles ONLY during normal mode (Mode Lancar stops all generation,
        // and gridlock + pre-fill starts completely packed & frozen without extra spawning)
        if !self.clearance_active && !(self.cfg.gridlock_mode && self.cfg.prefill_initial_queue) {
            let n_corridors = self.corridors.len();
            for c_idx in 0..n_corridors {
                self.corridors[c_idx].spawn_cooldown -= dt;
                if self.corridors[c_idx].spawn_cooldown <= 0.0 {
                    self.try_spawn_on_corridor(c_idx);
                    let jitter = self.rng.range_f64(0.85, 1.15);
                    self.corridors[c_idx].spawn_cooldown =
                        (self.cfg.spawn_interval_sec * jitter).max(0.3);
                }
            }
        }

        // 3. Advance turn state machine (AllStopped gridlock vs Smart Actuated vs Lancar drain)
        let n_corridors = self.corridors.len();
        match self.phase {
            QueuePhase::AllStopped => {}
            QueuePhase::NormalGreen {
                corridor_idx,
                mut timer,
                mut engaged,
            } => {
                // In `Kontrol Simpang Manual`, never auto-rotate on timer — stay open until
                // the operator explicitly picks another corridor, HoldAll, or Lancar!
                if !self.cfg.manual_control_mode && n_corridors >= 2 {
                    let cur_active = self.corridor_has_approaching_or_queued(corridor_idx)
                        || self
                            .vehicles
                            .iter()
                            .any(|v| v.corridor_idx == corridor_idx && v.committed_to_clear);
                    if cur_active {
                        engaged = true;
                    }

                    if !engaged {
                        // If current green corridor has nobody near the stop line yet, hand green
                        // immediately to any OTHER corridor that already arrived at the intersection!
                        if let Some(early_next) =
                            self.find_next_other_demanding_corridor(corridor_idx)
                        {
                            self.phase = QueuePhase::NormalGreen {
                                corridor_idx: early_next,
                                timer: 0.0,
                                engaged: true,
                            };
                        } else {
                            self.phase = QueuePhase::NormalGreen {
                                corridor_idx,
                                timer: 0.0,
                                engaged: false,
                            };
                        }
                    } else {
                        timer += dt;
                        let turn_dur = self.corridors[corridor_idx].turn_duration_sec;
                        if timer >= turn_dur {
                            // Only stop this corridor if ANOTHER corridor actually has vehicles waiting/approaching!
                            if let Some(next_idx) =
                                self.find_next_other_demanding_corridor(corridor_idx)
                            {
                                if self.cfg.switch_pause_sec > 0.01 {
                                    self.phase = QueuePhase::SwitchPause {
                                        next_corridor_idx: next_idx,
                                        timer: 0.0,
                                        is_clearance: false,
                                    };
                                } else {
                                    self.phase = QueuePhase::NormalGreen {
                                        corridor_idx: next_idx,
                                        timer: 0.0,
                                        engaged: true,
                                    };
                                }
                            } else {
                                // Keep green smoothly until another corridor actually needs the intersection
                                self.phase = QueuePhase::NormalGreen {
                                    corridor_idx,
                                    timer: turn_dur,
                                    engaged: true,
                                };
                            }
                        } else {
                            self.phase = QueuePhase::NormalGreen {
                                corridor_idx,
                                timer,
                                engaged,
                            };
                        }
                    }
                }
            }
            QueuePhase::SwitchPause {
                next_corridor_idx,
                mut timer,
                is_clearance,
            } => {
                timer += dt;
                let pause_dur = if is_clearance {
                    self.cfg.switch_pause_sec.min(1.0)
                } else {
                    self.cfg.switch_pause_sec
                };
                if timer >= pause_dur {
                    if is_clearance {
                        if let Some(c_idx) = self.find_next_queued_corridor(next_corridor_idx) {
                            self.phase = QueuePhase::ClearanceDrain {
                                corridor_idx: c_idx,
                                timer: 0.0,
                            };
                        } else if self.vehicles.is_empty() {
                            self.phase = QueuePhase::Completed;
                        } else {
                            self.phase = QueuePhase::ClearanceDrain {
                                corridor_idx: next_corridor_idx,
                                timer: 0.0,
                            };
                        }
                    } else {
                        self.phase = QueuePhase::NormalGreen {
                            corridor_idx: next_corridor_idx,
                            timer: 0.0,
                            engaged: self.corridor_has_approaching_or_queued(next_corridor_idx),
                        };
                    }
                } else {
                    self.phase = QueuePhase::SwitchPause {
                        next_corridor_idx,
                        timer,
                        is_clearance,
                    };
                }
            }
            QueuePhase::ClearanceDrain {
                corridor_idx,
                mut timer,
            } => {
                timer += dt;
                if self.cfg.manual_control_mode {
                    // In `Kontrol Simpang Manual`, operator explicitly triggers `Lancar Jalur A/B/C`:
                    // NEVER auto-switch to another corridor when the current corridor finishes draining.
                    if self.vehicles.is_empty() {
                        self.phase = QueuePhase::Completed;
                    } else {
                        self.phase = QueuePhase::ClearanceDrain {
                            corridor_idx,
                            timer,
                        };
                    }
                } else {
                    let cur_has_queue = self.corridor_has_queued_vehicles(corridor_idx);
                    let turn_dur = self
                        .corridors
                        .get(corridor_idx)
                        .map(|c| c.turn_duration_sec)
                        .unwrap_or(self.cfg.turn_duration_sec);
                    let timed_switch_due = self.cfg.clearance_mode
                        == QueueClearanceMode::AlternatingTimed
                        && timer >= turn_dur;

                    if !cur_has_queue || timed_switch_due {
                        let next_search = if n_corridors > 0 {
                            (corridor_idx + 1) % n_corridors
                        } else {
                            0
                        };
                        if let Some(next_idx) = self.find_next_queued_corridor(next_search) {
                            if next_idx != corridor_idx {
                                let pause_dur = self.cfg.switch_pause_sec.min(1.0);
                                if pause_dur > 0.01 {
                                    self.phase = QueuePhase::SwitchPause {
                                        next_corridor_idx: next_idx,
                                        timer: 0.0,
                                        is_clearance: true,
                                    };
                                } else {
                                    self.phase = QueuePhase::ClearanceDrain {
                                        corridor_idx: next_idx,
                                        timer: 0.0,
                                    };
                                }
                            } else {
                                // Only this corridor still has queued vehicles; keep draining it
                                self.phase = QueuePhase::ClearanceDrain {
                                    corridor_idx,
                                    timer: 0.0,
                                };
                            }
                        } else if self.vehicles.is_empty() {
                            self.phase = QueuePhase::Completed;
                        } else {
                            self.phase = QueuePhase::ClearanceDrain {
                                corridor_idx,
                                timer,
                            };
                        }
                    } else {
                        self.phase = QueuePhase::ClearanceDrain {
                            corridor_idx,
                            timer,
                        };
                    }
                }
            }
            QueuePhase::Completed => {}
        }

        // 4. Update all vehicle motions front-to-back per corridor (pure constant-speed convoy, no overtaking)
        // In Gridlock/Manual + Pre-fill mode while AllStopped and no vehicles are mid-exit, keep vehicles frozen.
        if self.phase == QueuePhase::AllStopped
            && self.cfg.prefill_initial_queue
            && self.vehicles.iter().all(|v| !v.committed_to_clear)
        {
            return;
        }
        let cruise_speed = if self.clearance_active {
            self.cfg
                .clear_speed_mps
                .max(self.cfg.convoy_speed_mps * 1.15)
        } else {
            self.cfg.convoy_speed_mps
        };
        let max_accel = if self.clearance_active { 12.0 } else { 7.5 };
        let max_decel = 12.0;

        for c_idx in 0..n_corridors {
            let queue_len = self.corridors[c_idx].queue_len();
            let corridor_is_green = matches!(
                self.phase,
                QueuePhase::NormalGreen { corridor_idx, .. }
                    | QueuePhase::ClearanceDrain { corridor_idx, .. }
                    if corridor_idx == c_idx
            );

            // Sort indices in this corridor from front-most (`largest s`) to rear-most (`smallest s`)
            let mut indices: Vec<usize> = self
                .vehicles
                .iter()
                .enumerate()
                .filter_map(|(i, v)| (v.corridor_idx == c_idx).then_some(i))
                .collect();
            indices.sort_by(|&a, &b| {
                self.vehicles[b]
                    .s
                    .partial_cmp(&self.vehicles[a].s)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let mut front_state: Option<(f64, f64, f64)> = None; // (s, length_m, speed)
            for idx in indices {
                let v = &mut self.vehicles[idx];
                let stop_s = (queue_len - 0.5 * v.length_m).max(0.0);

                // If corridor is green and vehicle already nudged past stop line, mark committed
                if !v.committed_to_clear && corridor_is_green && v.s > stop_s + 0.02 {
                    v.committed_to_clear = true;
                }

                let mut target_speed = cruise_speed;

                // Stop line constraint: ONLY applies to vehicles NOT yet inside `clear_geom` when RED
                if !v.committed_to_clear && !corridor_is_green {
                    let dist_to_stop = (stop_s - v.s).max(0.0);
                    let stop_limit_spd = (2.0 * 9.0 * dist_to_stop).sqrt();
                    target_speed = target_speed.min(stop_limit_spd);
                }

                // Convoy car-following constraint against vehicle directly ahead on same corridor
                if let Some((front_s, front_len, front_spd)) = front_state {
                    let desired_rear_s =
                        front_s - 0.5 * (front_len + v.length_m) - self.cfg.min_safe_gap_m;
                    let dist_to_gap = (desired_rear_s - v.s).max(0.0);
                    let follow_spd = (front_spd * front_spd + 2.0 * 10.0 * dist_to_gap).sqrt();
                    target_speed = target_speed.min(follow_spd);
                }

                // Smooth velocity integration
                if target_speed > v.speed {
                    v.speed = (v.speed + max_accel * dt).min(target_speed);
                } else {
                    v.speed = (v.speed - max_decel * dt).max(target_speed).max(0.0);
                }

                v.s += v.speed * dt;

                // Hard anti-crossing / anti-overlap clamps
                if !v.committed_to_clear && !corridor_is_green && v.s > stop_s {
                    v.s = stop_s;
                    v.speed = 0.0;
                }
                if let Some((front_s, front_len, front_spd)) = front_state {
                    let hard_max = front_s - 0.5 * (front_len + v.length_m) - 0.35;
                    if v.s > hard_max {
                        v.s = hard_max.max(0.0);
                        v.speed = v.speed.min(front_spd);
                    }
                }

                // Commit vehicle to `clear_geom` as soon as its front bumper passes the stop line on green
                if !v.committed_to_clear && corridor_is_green && v.s > stop_s + 0.02 {
                    v.committed_to_clear = true;
                }

                front_state = Some((v.s, v.length_m, v.speed));
            }
        }

        // 5. Despawn vehicles that have completely finished their `clear_geom`
        let corridors = &self.corridors;
        self.vehicles.retain(|v| {
            corridors
                .get(v.corridor_idx)
                .is_some_and(|c| v.s < c.total_len() + 0.5 * v.length_m)
        });

        // Mark completed immediately once the very last vehicle exits in Mode Lancar
        if self.clearance_active && self.vehicles.is_empty() {
            self.phase = QueuePhase::Completed;
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_anchor() -> GeoAnchor {
        GeoAnchor::new(115.2167, -8.6705)
    }

    fn corridor_local(
        anchor: GeoAnchor,
        name: &str,
        q_start: [f32; 3],
        q_end: [f32; 3],
        c_end: [f32; 3],
    ) -> TrafficQueueCorridor {
        let p0 = anchor.local_to_lng_lat_alt(q_start);
        let p1 = anchor.local_to_lng_lat_alt(q_end);
        let p2 = anchor.local_to_lng_lat_alt(c_end);
        let mut cor = TrafficQueueCorridor::new(name, vec![[p0.0, p0.1, p0.2], [p1.0, p1.1, p1.2]]);
        cor.clear_geom = vec![[p1.0, p1.1, p1.2], [p2.0, p2.1, p2.2]];
        cor
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
    fn anim_type_predicates_are_mutually_exclusive() {
        let now = chrono::Utc::now();
        let make = |anim_type: &str| Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Test".into(),
            anim_type: anim_type.into(),
            sort_order: 1,
            speed: 8.0,
            duration_sec: 0.0,
            replay: false,
            autoplay: false,
            geom: vec![],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };
        let flow = make(crate::traffic_anim::ANIM_TYPE_TRAFFIC_FLOW);
        let queue = make(TRAFFIC_QUEUE_ANIM_TYPE);
        let gridlock = make(TRAFFIC_GRIDLOCK_ANIM_TYPE);
        assert!(crate::traffic_anim::is_traffic_anim(&flow));
        assert!(!is_traffic_queue_anim(&flow));
        assert!(!is_traffic_gridlock_anim(&flow));
        assert!(is_traffic_queue_anim(&queue));
        assert!(!is_traffic_gridlock_anim(&queue));
        assert!(!crate::traffic_anim::is_traffic_anim(&queue));
        assert!(is_traffic_queue_anim(&gridlock));
        assert!(is_traffic_gridlock_anim(&gridlock));
        assert!(!crate::traffic_anim::is_traffic_anim(&gridlock));
    }

    #[test]
    fn every_enabled_corridor_requires_mandatory_clear_lane() {
        let anchor = test_anchor();
        let mut cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-60.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [60.0, 0.0, 0.0],
        );
        let mut cor_b = TrafficQueueCorridor::new("Jalur B", cor_a.queue_geom.clone());
        // `cor_b.clear_geom` is empty!
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a.clone(), cor_b.clone()],
            vehicle_pool: sample_pool(),
            ..Default::default()
        };
        assert!(!cfg.has_playable_setup());
        assert_eq!(cfg.incomplete_corridor_names(), vec!["Jalur B".to_string()]);

        // Once Jalur B gets a clearance lane, setup becomes playable
        cor_b.clear_geom = cor_a.clear_geom.clone();
        cor_a.enabled = true;
        let cfg_ok = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            ..Default::default()
        };
        assert!(cfg_ok.has_playable_setup());
    }

    #[test]
    fn red_corridor_stops_at_queue_end_while_committed_clear_vehicles_finish() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-80.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [75.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -80.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 75.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 10.0,
            clear_speed_mps: 18.0,
            turn_duration_sec: 10.0,
            switch_pause_sec: 1.0,
            max_vehicles_per_corridor: 6,
            min_safe_gap_m: 2.5,
            spawn_interval_sec: 1.0,
            prefill_initial_queue: false,
            clearance_mode: QueueClearanceMode::DrainCorridor,
            gridlock_mode: false,
            manual_control_mode: false,
            seed: 42,
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);

        // During first 9.5 seconds, Jalur A (0) is green and Jalur B (1) is red.
        engine.advance_to(9.5, None);
        let q_len_b = engine.corridors[1].queue_len();
        let b_vehicles: Vec<&QueueVehicle> = engine
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == 1)
            .collect();
        assert!(!b_vehicles.is_empty(), "Jalur B should have queued vehicles");
        for v in &b_vehicles {
            assert!(
                !v.committed_to_clear && v.s <= q_len_b - 0.5 * v.length_m + 0.05,
                "Red corridor B must never cross stop line (s={}, q_len={})",
                v.s,
                q_len_b
            );
        }

        // Some Jalur A vehicles should already be committed into `clear_geom`
        let a_committed_ids: Vec<u64> = engine
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == 0 && v.committed_to_clear)
            .map(|v| v.id)
            .collect();
        assert!(
            !a_committed_ids.is_empty(),
            "Green corridor A should have vehicles moving into Jalur Lancar A"
        );

        // Advance past actuated turn switch (engaged ~5.0s + 10.0s turn + 1.0s pause => ~16s)
        engine.advance_to(19.5, None);
        // Any previously committed Jalur A vehicle still present must have kept moving forward (or exited)
        for id in &a_committed_ids {
            if let Some(v) = engine.vehicles.iter().find(|x| x.id == *id) {
                assert!(
                    v.speed > 5.0,
                    "Vehicle already in Jalur Lancar A must keep moving even after Jalur A turns red"
                );
            }
        }
        // Meanwhile Jalur B should now have vehicles moving into Jalur Lancar B
        assert!(
            engine
                .vehicles
                .iter()
                .any(|v| v.corridor_idx == 1 && v.committed_to_clear),
            "Jalur B should release vehicles into Jalur Lancar B on its turn"
        );
    }

    #[test]
    fn lancar_trigger_stops_spawning_boosts_speed_and_drains_all_to_zero() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-70.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [60.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -70.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 60.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 8.0,
            clear_speed_mps: 20.0,
            turn_duration_sec: 12.0,
            switch_pause_sec: 0.8,
            max_vehicles_per_corridor: 6,
            min_safe_gap_m: 2.0,
            spawn_interval_sec: 1.0,
            prefill_initial_queue: false,
            clearance_mode: QueueClearanceMode::DrainCorridor,
            gridlock_mode: false,
            manual_control_mode: false,
            seed: 99,
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        engine.advance_to(9.0, None);
        let max_id_before_lancar = engine.next_vehicle_id;
        assert!(
            !engine.vehicles.is_empty(),
            "Vehicles must exist on map before clicking Lancar"
        );

        // Trigger Mode Lancar at t = 9.0 s
        engine.advance_to(11.0, Some(9.0));
        assert_eq!(
            engine.next_vehicle_id, max_id_before_lancar,
            "Mode Lancar must NOT spawn any new vehicles"
        );
        assert!(
            engine.vehicles.iter().any(|v| v.speed > cfg.convoy_speed_mps + 2.0),
            "Mode Lancar must boost vehicle speed above normal convoy speed"
        );

        // Run until completely cleared
        engine.advance_to(40.0, Some(9.0));
        assert!(
            engine.is_completely_cleared(),
            "All vehicles on all corridors must drain completely to zero in Mode Lancar (remaining={})",
            engine.vehicles.len()
        );
        assert_eq!(engine.active_poses().len(), 0);
    }

    #[test]
    fn deterministic_playback_matches_between_operator_and_big_screen() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-60.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [60.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -60.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 60.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            seed: 123456,
            ..Default::default()
        };
        let mut op = TrafficQueueSimEngine::new(&cfg, anchor);
        let mut big = TrafficQueueSimEngine::new(&cfg, anchor);
        op.advance_to(14.0, Some(8.5));
        big.advance_to(14.0, Some(8.5));
        let p_op = op.active_poses();
        let p_big = big.active_poses();
        assert_eq!(p_op.len(), p_big.len());
        for (a, b) in p_op.iter().zip(p_big.iter()) {
            assert_eq!(a.vehicle_id, b.vehicle_id);
            assert!((a.x - b.x).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4);
        }
    }

    #[test]
    fn high_fps_forward_playback_never_resets_or_flickers_past_three_minutes() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-70.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [70.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -70.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 70.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            seed: 777,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        let dt_120fps = 1.0 / 120.0;
        let mut t = 0.0;
        let mut prev_sim_time = 0.0;
        let mut prev_next_id = 1;
        // Simulate 200 seconds (3m 20s) at 120 FPS (24,000 frames)
        for _ in 0..24_000 {
            t += dt_120fps;
            engine.advance_to(t, None);
            assert!(
                engine.sim_time + 1e-9 >= prev_sim_time,
                "sim_time must NEVER rewind or reset during forward 120fps playback (prev={}, now={}, t={})",
                prev_sim_time,
                engine.sim_time,
                t
            );
            assert!(
                engine.sim_time <= t + 1e-6,
                "sim_time must never overshoot target elapsed time"
            );
            assert!(
                engine.next_vehicle_id >= prev_next_id,
                "vehicle IDs must never reset during forward playback"
            );
            prev_sim_time = engine.sim_time;
            prev_next_id = engine.next_vehicle_id;
        }
        assert!(engine.sim_time > 199.5);
    }

    #[test]
    fn first_corridor_does_not_stop_prematurely_while_other_corridor_is_still_far() {
        let anchor = test_anchor();
        // Long Jalur A (120m -> takes ~15s at 8 m/s) & even longer Jalur B (240m -> takes ~30s)
        // with short turn_duration_sec = 6.0s!
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-125.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [80.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -245.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 80.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 8.0,
            turn_duration_sec: 6.0,
            switch_pause_sec: 1.0,
            prefill_initial_queue: false,
            seed: 55,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        // At t = 22s, Jalur A's first car reached the stop line at ~15s (well past 6s!),
        // while Jalur B's first car is still ~60m away from its stop line.
        // Jalur A MUST stay green and flow smoothly into Jalur Lancar A without stopping!
        engine.advance_to(22.0, None);
        assert!(
            engine
                .vehicles
                .iter()
                .any(|v| v.corridor_idx == 0 && v.committed_to_clear && v.speed > 6.0),
            "Jalur A must flow smoothly into Jalur Lancar A instead of stopping while Jalur B is still far away"
        );
    }

    #[test]
    fn prefill_initial_queue_populates_vehicles_immediately_at_t0() {
        let anchor = test_anchor();
        // 120m queue lane: with 4.5m car + 2.5m gap + 0.35m buffer (~7.35m pitch), ~16 cars fill end-to-start!
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-125.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [60.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -125.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 60.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            max_vehicles_per_corridor: 8,
            prefill_initial_queue: true,
            seed: 101,
            ..Default::default()
        };
        let engine = TrafficQueueSimEngine::new(&cfg, anchor);
        let a_vehs: Vec<&QueueVehicle> =
            engine.vehicles.iter().filter(|v| v.corridor_idx == 0).collect();
        let b_vehs: Vec<&QueueVehicle> =
            engine.vehicles.iter().filter(|v| v.corridor_idx == 1).collect();
        assert!(
            a_vehs.len() >= 12,
            "Pre-fill should fill the entire 120m Jalur A end-to-start (got {})",
            a_vehs.len()
        );
        assert!(
            b_vehs.len() >= 12,
            "Pre-fill should fill the entire 120m Jalur B end-to-start (got {})",
            b_vehs.len()
        );
        let min_s_a = a_vehs.iter().map(|v| v.s).fold(f64::INFINITY, f64::min);
        assert!(
            min_s_a < 15.0,
            "Pre-fill tail must reach back near the start of Jalur A (min_s={min_s_a})"
        );
    }

    #[test]
    fn continuous_spawning_never_pauses_while_vehicles_exit_clear_lane() {
        let anchor = test_anchor();
        // Short 60m approach lane + very long 300m clearance lane, with low max_vehicles_per_corridor = 4
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-65.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [295.0, 0.0, 0.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 10.0,
            max_vehicles_per_corridor: 4,
            spawn_interval_sec: 1.0,
            min_safe_gap_m: 2.0,
            prefill_initial_queue: false,
            seed: 202,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        // After 18s: ~10+ vehicles have passed into the 300m clear_geom (not yet exited at 36s!),
        // yet `queue_geom` MUST STILL continuously hold freshly spawned vehicles (not empty!).
        engine.advance_to(18.0, None);
        let in_queue = engine.vehicles.iter().filter(|v| !v.committed_to_clear).count();
        let in_clear = engine.vehicles.iter().filter(|v| v.committed_to_clear).count();
        assert!(
            in_clear >= 5,
            "Vehicles should be flowing down clear_geom (got {in_clear})"
        );
        assert!(
            in_queue >= 2,
            "queue_geom must keep spawning continuously while clear_geom is occupied (got {in_queue})"
        );
    }

    #[test]
    fn lancar_alternating_timed_rotates_between_corridors_until_empty() {
        let anchor = test_anchor();
        // Two long 180m pre-filled queues with short 3s turn duration so AlternatingTimed rotates BEFORE Jalur A empties!
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-185.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [75.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -185.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 75.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 8.0,
            clear_speed_mps: 16.0,
            turn_duration_sec: 3.0,
            switch_pause_sec: 0.5,
            prefill_initial_queue: true,
            clearance_mode: QueueClearanceMode::AlternatingTimed,
            seed: 303,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        // Trigger Mode Lancar right at t = 0.5s
        engine.advance_to(5.5, Some(0.5));
        // By t = 5.5s (after 3s on A + 0.5s pause + 1.5s on B), Jalur B MUST already have vehicles committed into clear_geom
        // EVEN THOUGH Jalur A still has remaining queued vehicles!
        let a_still_queued = engine
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == 0 && !v.committed_to_clear)
            .count();
        let b_committed = engine
            .vehicles
            .iter()
            .filter(|v| v.corridor_idx == 1 && v.committed_to_clear)
            .count();
        assert!(
            a_still_queued >= 5,
            "Jalur A should still have queued vehicles when AlternatingTimed hands turn to B (got {a_still_queued})"
        );
        assert!(
            b_committed >= 1,
            "Jalur B should already be releasing vehicles into Jalur Lancar B during AlternatingTimed Lancar (got {b_committed})"
        );

        // Advance further until completely drained to 0!
        engine.advance_to(65.0, Some(0.5));
        assert!(
            engine.is_completely_cleared(),
            "AlternatingTimed Lancar must eventually drain ALL corridors to 0 (remaining={})",
            engine.vehicles.len()
        );
    }

    #[test]
    fn gridlock_prefill_stays_completely_frozen_until_lancar_then_drains_all() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-125.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [65.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -125.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 65.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 8.0,
            clear_speed_mps: 18.0,
            turn_duration_sec: 5.0,
            switch_pause_sec: 0.5,
            prefill_initial_queue: true,
            clearance_mode: QueueClearanceMode::AlternatingTimed,
            gridlock_mode: true,
            seed: 404,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        let initial_positions: Vec<(u64, f64)> =
            engine.vehicles.iter().map(|v| (v.id, v.s)).collect();
        assert!(
            initial_positions.len() >= 24,
            "Gridlock pre-fill must fill both corridors end-to-start"
        );

        // Advance 20 seconds WITHOUT clicking Lancar: EVERY vehicle on EVERY corridor must stay 100% frozen!
        engine.advance_to(20.0, None);
        assert!(engine.live_status().is_all_stopped);
        assert_eq!(engine.vehicles.len(), initial_positions.len());
        for (v, &(id, s0)) in engine.vehicles.iter().zip(initial_positions.iter()) {
            assert_eq!(v.id, id);
            assert!(
                (v.s - s0).abs() < 1e-4 && v.speed < 1e-4 && !v.committed_to_clear,
                "Gridlock pre-fill vehicle must not move before Lancar (id={}, s0={}, s={}, spd={})",
                v.id,
                s0,
                v.s,
                v.speed
            );
        }

        // Now click Lancar at t = 20.0s -> must drain ALL vehicles to 0!
        engine.advance_to(75.0, Some(20.0));
        assert!(
            engine.is_completely_cleared(),
            "Gridlock Lancar must drain all corridors to 0 (remaining={})",
            engine.vehicles.len()
        );
    }

    #[test]
    fn gridlock_flow_then_stop_halts_all_corridors_at_stop_line_until_lancar() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-65.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [60.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -65.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 60.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 10.0,
            clear_speed_mps: 18.0,
            prefill_initial_queue: false,
            clearance_mode: QueueClearanceMode::DrainCorridor,
            gridlock_mode: true,
            seed: 505,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        assert!(engine.vehicles.is_empty());

        // Advance 20s without Lancar: vehicles drive in from start and stop at stop line on BOTH corridors!
        engine.advance_to(20.0, None);
        assert!(engine.vehicles.len() >= 6);
        assert!(
            engine.vehicles.iter().all(|v| !v.committed_to_clear),
            "No vehicle may cross stop line during Macet Total before Lancar is clicked"
        );
        assert!(
            engine
                .vehicles
                .iter()
                .any(|v| v.corridor_idx == 0 && v.speed < 0.1),
            "Front vehicle on Jalur A must stop at stop line"
        );
        assert!(
            engine
                .vehicles
                .iter()
                .any(|v| v.corridor_idx == 1 && v.speed < 0.1),
            "Front vehicle on Jalur B must stop at stop line"
        );

        // Trigger Lancar -> drains all to 0!
        engine.advance_to(50.0, Some(20.0));
        assert!(engine.is_completely_cleared());
    }

    #[test]
    fn manual_control_mode_opens_and_holds_corridors_explicitly_until_lancar() {
        let anchor = test_anchor();
        let cor_a = corridor_local(
            anchor,
            "Jalur A",
            [-70.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            [65.0, 0.0, 0.0],
        );
        let cor_b = corridor_local(
            anchor,
            "Jalur B",
            [0.0, 0.0, -70.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 65.0],
        );
        let cfg = TrafficQueueConfig {
            corridors: vec![cor_a, cor_b],
            vehicle_pool: sample_pool(),
            convoy_speed_mps: 9.0,
            clear_speed_mps: 18.0,
            turn_duration_sec: 4.0,
            prefill_initial_queue: true,
            manual_control_mode: true,
            seed: 606,
            ..Default::default()
        };
        let mut engine = TrafficQueueSimEngine::new(&cfg, anchor);
        // Starts AllStopped until operator opens a corridor
        engine.advance_to_with_manual(3.0, None, &[]);
        assert!(engine.live_status().is_all_stopped);
        assert!(engine.vehicles.iter().all(|v| !v.committed_to_clear));

        // Operator opens ONLY Jalur B (source_idx = 1) at t = 3.0s -> stays open past turn_duration_sec (4.0s)!
        let ev1 = vec![(3.0, QueueManualAction::OpenCorridor { corridor_idx: 1 })];
        engine.advance_to_with_manual(12.0, None, &ev1);
        assert_eq!(engine.live_status().active_corridor_source_idx, Some(1));
        assert!(engine.vehicles.iter().any(|v| v.corridor_idx == 1 && v.committed_to_clear));
        assert!(engine.vehicles.iter().all(|v| v.corridor_idx != 0 || !v.committed_to_clear));

        // Operator clicks HoldAll at t = 12.0s, then opens Jalur A at t = 14.0s
        let ev2 = vec![
            (3.0, QueueManualAction::OpenCorridor { corridor_idx: 1 }),
            (12.0, QueueManualAction::HoldAll),
            (14.0, QueueManualAction::OpenCorridor { corridor_idx: 0 }),
        ];
        engine.advance_to_with_manual(18.0, None, &ev2);
        assert_eq!(engine.live_status().active_corridor_source_idx, Some(0));
        assert!(engine.vehicles.iter().any(|v| v.corridor_idx == 0 && v.committed_to_clear));

        // Operator clicks Lancar Jalur A (DrainCorridor 0) at t = 18.0s:
        // - Stops spawning globally & drains Jalur A to 0
        // - Does NOT auto-switch to Jalur B (Jalur B stays queued waiting for operator!)
        // - Any subsequent OpenCorridor/HoldAll after DrainCorridor is ignored
        let ev3 = vec![
            (3.0, QueueManualAction::OpenCorridor { corridor_idx: 1 }),
            (12.0, QueueManualAction::HoldAll),
            (14.0, QueueManualAction::OpenCorridor { corridor_idx: 0 }),
            (18.0, QueueManualAction::DrainCorridor { corridor_idx: 0 }),
            (20.0, QueueManualAction::HoldAll),
        ];
        engine.advance_to_with_manual(35.0, None, &ev3);
        let st_after_a = engine.live_status();
        assert!(st_after_a.clearance_active);
        assert_eq!(st_after_a.active_corridor_source_idx, Some(0));
        assert_eq!(
            st_after_a.corridor_counts(0),
            (0, 0),
            "Jalur A should have completely drained to 0"
        );
        assert!(
            st_after_a.corridor_counts(1).0 > 0,
            "Jalur B must stay queued until operator clicks Lancar Jalur B"
        );
        assert!(!engine.is_completely_cleared());

        // Operator clicks Lancar Jalur B (DrainCorridor 1) at t = 35.0s -> drains remaining Jalur B to 0!
        let mut ev4 = ev3;
        ev4.push((35.0, QueueManualAction::DrainCorridor { corridor_idx: 1 }));
        engine.advance_to_with_manual(65.0, None, &ev4);
        assert!(engine.is_completely_cleared());
    }
}
