use serde::{Deserialize, Serialize};

use crate::models::Anim;

pub const ANIM_TYPE_SHOOT_WATER_CANNON: &str = "shoot_water_cannon";
pub const ANIM_TYPE_SHOOT_STONE_THROW: &str = "shoot_stone_throw";
pub const ANIM_TYPE_SHOOT_TEAR_GAS: &str = "shoot_tear_gas";
pub const ANIM_TYPE_SHOOT_BULLET: &str = "shoot_bullet";
pub const ANIM_TYPE_SHOOT_APAR: &str = "shoot_apar";

/// Supported tactical 3D Shoot animation sub-types (each bound 1-to-1 to a 3D Marker).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShootAnimKind {
    #[default]
    WaterCannon,
    StoneThrow,
    TearGas,
    Bullet,
    Apar,
}

impl ShootAnimKind {
    pub const ALL: &'static [Self] = &[
        Self::WaterCannon,
        Self::StoneThrow,
        Self::TearGas,
        Self::Bullet,
        Self::Apar,
    ];

    pub fn anim_type_str(self) -> &'static str {
        match self {
            Self::WaterCannon => ANIM_TYPE_SHOOT_WATER_CANNON,
            Self::StoneThrow => ANIM_TYPE_SHOOT_STONE_THROW,
            Self::TearGas => ANIM_TYPE_SHOOT_TEAR_GAS,
            Self::Bullet => ANIM_TYPE_SHOOT_BULLET,
            Self::Apar => ANIM_TYPE_SHOOT_APAR,
        }
    }

    pub fn from_anim_type(anim_type: &str) -> Option<Self> {
        match anim_type {
            ANIM_TYPE_SHOOT_WATER_CANNON | "shoot" => Some(Self::WaterCannon),
            ANIM_TYPE_SHOOT_STONE_THROW | "stone_throw" => Some(Self::StoneThrow),
            ANIM_TYPE_SHOOT_TEAR_GAS | "tear_gas" => Some(Self::TearGas),
            ANIM_TYPE_SHOOT_BULLET | "bullet" => Some(Self::Bullet),
            ANIM_TYPE_SHOOT_APAR | "apar" => Some(Self::Apar),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaterCannon => "Water Cannon (AWC / Damkar)",
            Self::StoneThrow => "Lempar Batu",
            Self::TearGas => "Gas Air Mata",
            Self::Bullet => "Tembak Peluru",
            Self::Apar => "Semprot APAR",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::WaterCannon => "WATER CANNON",
            Self::StoneThrow => "BATU",
            Self::TearGas => "GAS AIR MATA",
            Self::Bullet => "PELURU",
            Self::Apar => "APAR",
        }
    }

    pub fn playing_status_label(self) -> &'static str {
        match self {
            Self::WaterCannon => "MENYEMPROT",
            Self::StoneThrow => "MELEMPAR",
            Self::TearGas => "GAS AIR MATA",
            Self::Bullet => "MENEMBAK",
            Self::Apar => "SEMPROT APAR",
        }
    }

    pub fn default_name(self, index: usize) -> String {
        match self {
            Self::WaterCannon => format!("Water Cannon {index}"),
            Self::StoneThrow => format!("Lempar Batu {index}"),
            Self::TearGas => format!("Gas Air Mata {index}"),
            Self::Bullet => format!("Tembak Peluru {index}"),
            Self::Apar => format!("Semprot APAR {index}"),
        }
    }

    /// Fraction of bound marker's 3D bounding height used for default launch/nozzle height.
    pub fn origin_height_ratio(self) -> f64 {
        match self {
            Self::WaterCannon => 0.96,
            Self::StoneThrow => 0.80,
            Self::TearGas => 0.78,
            Self::Bullet => 0.76,
            Self::Apar => 0.62,
        }
    }

    /// Fallback launch/nozzle height (meters) when marker bounding box is not yet measured.
    pub fn fallback_origin_height_m(self) -> f64 {
        match self {
            Self::WaterCannon => 3.2,
            Self::StoneThrow => 1.5,
            Self::TearGas => 1.6,
            Self::Bullet => 1.4,
            Self::Apar => 1.1,
        }
    }

    pub fn default_duration_sec(self) -> f64 {
        match self {
            Self::WaterCannon => 15.0,
            Self::StoneThrow => 15.0,
            Self::TearGas => TearGasConfig::default().total_cycle_duration_sec(),
            Self::Bullet => 10.0,
            Self::Apar => 12.0,
        }
    }
}

/// Returns `true` if `anim` belongs to the Shoot animation category.
#[inline]
pub fn is_shoot_anim(anim: &Anim) -> bool {
    ShootAnimKind::from_anim_type(&anim.anim_type).is_some()
        || anim.anim_type == "shoot"
        || anim.anim_type.starts_with("shoot_")
}

/// Resolves the specific `ShootAnimKind` of an animation, if it belongs to the Shoot category.
#[inline]
pub fn shoot_anim_kind(anim: &Anim) -> Option<ShootAnimKind> {
    ShootAnimKind::from_anim_type(&anim.anim_type).or_else(|| {
        if anim.anim_type.starts_with("shoot_") {
            Some(ShootAnimKind::WaterCannon)
        } else {
            None
        }
    })
}

/// Returns `true` if `anim` is a Water Cannon shoot animation.
#[inline]
pub fn is_water_cannon_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_SHOOT_WATER_CANNON || anim.anim_type == "shoot"
}

/// Returns `true` if the shoot animation runs continuously until Stopped in Play Skenario.
pub fn shoot_anim_is_continuous(anim: &Anim) -> bool {
    match shoot_anim_kind(anim) {
        Some(ShootAnimKind::WaterCannon) => parse_water_cannon_config(anim).continuous,
        Some(ShootAnimKind::StoneThrow) => {
            let cfg = parse_stone_throw_config(anim);
            cfg.continuous && !cfg.single_shot
        }
        Some(ShootAnimKind::TearGas) => parse_tear_gas_config(anim).continuous,
        Some(ShootAnimKind::Bullet) => {
            let cfg = parse_bullet_shoot_config(anim);
            cfg.continuous && !cfg.single_shot
        }
        Some(ShootAnimKind::Apar) => parse_apar_spray_config(anim).continuous,
        None => false,
    }
}

/// Returns the effective finite cycle duration (seconds) for any non-continuous shoot animation.
pub fn shoot_anim_effective_duration_sec(anim: &Anim) -> f64 {
    match shoot_anim_kind(anim) {
        Some(ShootAnimKind::StoneThrow) => {
            let cfg = parse_stone_throw_config(anim);
            if cfg.single_shot {
                cfg.single_throw_duration_sec()
            } else {
                anim.duration_sec.max(0.5)
            }
        }
        Some(ShootAnimKind::Bullet) => {
            let cfg = parse_bullet_shoot_config(anim);
            if cfg.single_shot {
                cfg.single_shot_duration_sec()
            } else {
                anim.duration_sec.max(0.5)
            }
        }
        _ => anim.duration_sec.max(0.5),
    }
}

/// Returns the primary reach / range in meters of any shoot animation (for compact Layer rows).
pub fn shoot_anim_range_m(anim: &Anim) -> f64 {
    match shoot_anim_kind(anim) {
        Some(ShootAnimKind::WaterCannon) => parse_water_cannon_config(anim).range_m,
        Some(ShootAnimKind::StoneThrow) => parse_stone_throw_config(anim).range_m,
        Some(ShootAnimKind::TearGas) => parse_tear_gas_config(anim).range_m,
        Some(ShootAnimKind::Bullet) => parse_bullet_shoot_config(anim).max_range_m,
        Some(ShootAnimKind::Apar) => parse_apar_spray_config(anim).range_m,
        None => 0.0,
    }
}

/// Computes quadratic parabola coefficients `(a, b, c)` for `y(t) = a*t^2 + b*t + c`
/// on `t ∈ [0.0, 1.0]` such that:
/// - `y(0) = start_y`
/// - `y(1) = end_y`
/// - `max_{t ∈ [0,1]} y(t) = start_y.max(end_y) + arc_above_max`
pub fn parabola_coeffs_between(start_y: f64, end_y: f64, arc_above_max: f64) -> (f64, f64, f64) {
    let dh = arc_above_max.max(0.0);
    let y_peak = start_y.max(end_y) + dh;
    let h_rise = (y_peak - start_y).max(0.0);
    let h_drop = (y_peak - end_y).max(0.0);
    if h_rise <= 1e-6 && h_drop <= 1e-6 {
        return (0.0, 0.0, start_y);
    }
    if h_rise <= 1e-6 {
        return (end_y - start_y, 0.0, start_y);
    }
    let b = 2.0 * h_rise + 2.0 * (h_rise * h_drop).sqrt();
    let a = (end_y - start_y) - b;
    (a, b, start_y)
}

/// Evaluates `(horizontal_dist_m, world_y, tangent_horiz, tangent_vert)` along a parabola
/// spanning `horizontal ∈ [0, range_m]` from `y(0) = start_y` to `y(1) = end_y` with peak
/// `start_y.max(end_y) + arc_above_max`.
pub fn eval_parabola_between(
    range_m: f64,
    start_y: f64,
    end_y: f64,
    arc_above_max: f64,
    t: f64,
) -> (f64, f64, f64, f64) {
    let tc = t.clamp(0.0, 1.0);
    let range = range_m.max(0.5);
    let (a, b, c) = parabola_coeffs_between(start_y, end_y, arc_above_max);
    let horiz = range * tc;
    let y = a * tc * tc + b * tc + c;
    let dh_dt = range;
    let dy_dt = 2.0 * a * tc + b;
    let len = dh_dt.hypot(dy_dt).max(1e-6);
    (horiz, y, dh_dt / len, dy_dt / len)
}

#[inline]
fn clamp_finite(v: &mut f64, lo: f64, hi: f64, def: f64) {
    if v.is_finite() {
        *v = v.clamp(lo, hi);
    } else {
        *v = def;
    }
}

#[inline]
fn sanitize_hex(hex: &mut String, fallback: &str) {
    if !hex.starts_with('#') || hex.len() < 7 {
        *hex = fallback.to_string();
    }
}

// ===========================================================================
// 1. Water Cannon Config (Existing — 100% Backward Compatible)
// ===========================================================================

fn default_nozzle_height_m() -> f64 {
    3.2
}
fn default_nozzle_forward_m() -> f64 {
    0.8
}
fn default_range_m() -> f64 {
    28.0
}
fn default_arc_height_m() -> f64 {
    4.5
}
fn default_pressure_bar() -> f64 {
    6.0
}
fn default_spread() -> f64 {
    0.25
}
fn default_sweep_angle_deg() -> f64 {
    25.0
}
fn default_sweep_speed() -> f64 {
    0.35
}
fn default_color_hex() -> String {
    "#d8f3ff".to_string()
}
fn default_true() -> bool {
    true
}

/// Configuration for a 3D Water Cannon (AWC / Damkar) shoot animation stored inside
/// `anim.data["water_cannon"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaterCannonConfig {
    /// Height of the cannon nozzle above the bound marker's base (meters).
    #[serde(default = "default_nozzle_height_m")]
    pub nozzle_height_m: f64,
    /// Offset forward along the marker's facing axis (`+Z` in glTF) in meters.
    #[serde(default = "default_nozzle_forward_m")]
    pub nozzle_forward_m: f64,
    /// Offset right along the marker's lateral axis in meters.
    #[serde(default)]
    pub nozzle_right_m: f64,
    /// Horizontal reach from nozzle to ground impact point (meters).
    #[serde(default = "default_range_m")]
    pub range_m: f64,
    /// Peak height of the parabolic water arc ABOVE the nozzle height (meters).
    #[serde(default = "default_arc_height_m")]
    pub arc_height_m: f64,
    /// Spray pressure / force (`1.0..10.0` bar): drives stream speed, thickness & splash intensity.
    #[serde(default = "default_pressure_bar")]
    pub pressure_bar: f64,
    /// Horizontal nozzle aim relative to marker heading in degrees (`-180..+180`, `0` = straight ahead).
    #[serde(default)]
    pub yaw_offset_deg: f64,
    /// Nozzle dispersion: `0.0` = concentrated high-pressure jet, `1.0` = wide firefighting spray/mist.
    #[serde(default = "default_spread")]
    pub spread: f64,
    /// Enables automatic left–right turret sweeping oscillation.
    #[serde(default)]
    pub sweep_enabled: bool,
    /// Half-angle of left–right sweep in degrees (`±sweep_angle_deg`).
    #[serde(default = "default_sweep_angle_deg")]
    pub sweep_angle_deg: f64,
    /// Sweep oscillation speed in cycles/second (Hz).
    #[serde(default = "default_sweep_speed")]
    pub sweep_speed: f64,
    /// Hex color for water / firefighting foam / tear-gas dye tint.
    #[serde(default = "default_color_hex")]
    pub color_hex: String,
    /// When `true`, sprays continuously in Play Skenario until Paused/Stopped.
    /// When `false`, sprays for `anim.duration_sec` seconds (with optional `anim.replay`).
    #[serde(default = "default_true")]
    pub continuous: bool,
}

impl Default for WaterCannonConfig {
    fn default() -> Self {
        Self {
            nozzle_height_m: default_nozzle_height_m(),
            nozzle_forward_m: default_nozzle_forward_m(),
            nozzle_right_m: 0.0,
            range_m: default_range_m(),
            arc_height_m: default_arc_height_m(),
            pressure_bar: default_pressure_bar(),
            yaw_offset_deg: 0.0,
            spread: default_spread(),
            sweep_enabled: false,
            sweep_angle_deg: default_sweep_angle_deg(),
            sweep_speed: default_sweep_speed(),
            color_hex: default_color_hex(),
            continuous: true,
        }
    }
}

impl WaterCannonConfig {
    pub fn sanitize(&mut self) {
        clamp_finite(&mut self.nozzle_height_m, 0.2, 200.0, default_nozzle_height_m());
        clamp_finite(&mut self.nozzle_forward_m, -50.0, 50.0, default_nozzle_forward_m());
        clamp_finite(&mut self.nozzle_right_m, -50.0, 50.0, 0.0);
        clamp_finite(&mut self.range_m, 2.0, 180.0, default_range_m());
        clamp_finite(&mut self.arc_height_m, 0.0, 80.0, default_arc_height_m());
        clamp_finite(&mut self.pressure_bar, 1.0, 10.0, default_pressure_bar());
        clamp_finite(&mut self.yaw_offset_deg, -180.0, 180.0, 0.0);
        clamp_finite(&mut self.spread, 0.0, 1.0, default_spread());
        clamp_finite(&mut self.sweep_angle_deg, 5.0, 120.0, default_sweep_angle_deg());
        clamp_finite(&mut self.sweep_speed, 0.05, 2.5, default_sweep_speed());
        sanitize_hex(&mut self.color_hex, "#d8f3ff");
    }

    /// Computes instantaneous horizontal aim offset in degrees at `time_sec`, including optional
    /// sinusoidal left–right turret sweep.
    pub fn effective_yaw_offset_deg(&self, time_sec: f64) -> f64 {
        if self.sweep_enabled {
            let phase = time_sec * self.sweep_speed.clamp(0.05, 2.5) * std::f64::consts::TAU;
            self.yaw_offset_deg + phase.sin() * self.sweep_angle_deg.clamp(5.0, 120.0)
        } else {
            self.yaw_offset_deg
        }
    }

    /// Computes quadratic parabola coefficients `(a, b, c)` for `y(t) = a*t^2 + b*t + c`
    /// where `t ∈ [0.0, 1.0]` satisfies:
    /// - `y(0) = nozzle_height_m`
    /// - `y(1) = 0.0` (ground impact at `horizontal = range_m`)
    /// - `max_{t ∈ [0,1]} y(t) = nozzle_height_m + arc_height_m`
    pub fn parabola_coeffs(&self) -> (f64, f64, f64) {
        let h0 = self.nozzle_height_m.clamp(0.2, 200.0);
        let dh = self.arc_height_m.clamp(0.0, 80.0);
        parabola_coeffs_between(h0, 0.0, dh)
    }

    /// Evaluates `(horizontal_dist_m, height_m, tangent_horiz, tangent_vert)` along the exact
    /// parabolic trajectory for normalized parameter `t ∈ [0.0, 1.0]`.
    pub fn eval_parabola(&self, t: f64) -> (f64, f64, f64, f64) {
        let range = self.range_m.clamp(2.0, 180.0);
        let h0 = self.nozzle_height_m.clamp(0.2, 200.0);
        let dh = self.arc_height_m.clamp(0.0, 80.0);
        let (horiz, y, th, tv) = eval_parabola_between(range, h0, 0.0, dh, t);
        (horiz, y.max(0.0), th, tv)
    }
}

pub fn parse_water_cannon_config(anim: &Anim) -> WaterCannonConfig {
    let mut cfg: WaterCannonConfig = anim
        .data
        .get("water_cannon")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_water_cannon_config(anim: &mut Anim, cfg: &WaterCannonConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("water_cannon".into(), val);
    }
}

// ===========================================================================
// 2. Stone Throw Config (`shoot_stone_throw` -> `anim.data["stone_throw"]`)
// ===========================================================================

fn default_stone_origin_h() -> f64 {
    1.5
}
fn default_stone_origin_fwd() -> f64 {
    0.4
}
fn default_stone_origin_right() -> f64 {
    0.2
}
fn default_stone_range_m() -> f64 {
    18.0
}
fn default_stone_arc_h() -> f64 {
    3.5
}
fn default_stone_speed() -> f64 {
    14.0
}
fn default_stone_rate() -> f64 {
    1.2
}
fn default_stones_per_throw() -> u32 {
    1
}
fn default_stone_size_m() -> f64 {
    0.24
}
fn default_stone_spread_deg() -> f64 {
    14.0
}
fn default_stone_color() -> String {
    "#78716c".to_string()
}

/// Configuration for a 3D Stone Throwing shoot animation stored inside `anim.data["stone_throw"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoneThrowConfig {
    /// Launch height above bound marker base (meters, e.g. raised hand/shoulder).
    #[serde(default = "default_stone_origin_h")]
    pub origin_height_m: f64,
    /// Offset forward along marker facing axis (`+Z`) in meters.
    #[serde(default = "default_stone_origin_fwd")]
    pub origin_forward_m: f64,
    /// Offset right along marker lateral axis in meters.
    #[serde(default = "default_stone_origin_right")]
    pub origin_right_m: f64,
    /// Average horizontal throwing distance (meters).
    #[serde(default = "default_stone_range_m")]
    pub range_m: f64,
    /// Peak parabolic arc height ABOVE the launch height (meters).
    #[serde(default = "default_stone_arc_h")]
    pub arc_height_m: f64,
    /// Horizontal flight speed in m/s (`flight_time = range_m / throw_speed_mps`).
    #[serde(default = "default_stone_speed")]
    pub throw_speed_mps: f64,
    /// Throws per second (Hz).
    #[serde(default = "default_stone_rate")]
    pub throw_rate_hz: f64,
    /// Number of rocks thrown per volley (`1..6`).
    #[serde(default = "default_stones_per_throw")]
    pub stones_per_throw: u32,
    /// Visual radius of thrown rocks in meters (`0.08..1.5`).
    #[serde(default = "default_stone_size_m")]
    pub stone_size_m: f64,
    /// Horizontal throwing aim relative to marker heading in degrees (`-180..+180`).
    #[serde(default)]
    pub yaw_offset_deg: f64,
    /// Random horizontal cone spread in degrees (`0..60`).
    #[serde(default = "default_stone_spread_deg")]
    pub spread_deg: f64,
    /// Rock material tint color (`#RRGGBB`).
    #[serde(default = "default_stone_color")]
    pub color_hex: String,
    /// Show subtle dust puff when stones hit the ground.
    #[serde(default = "default_true")]
    pub dust_impact: bool,
    /// When `true`, throws continuously in Play Skenario until Stopped.
    #[serde(default = "default_true")]
    pub continuous: bool,
    /// When `true`, throws only ONCE per Play trigger and finishes once the stone lands.
    #[serde(default)]
    pub single_shot: bool,
}

impl Default for StoneThrowConfig {
    fn default() -> Self {
        Self {
            origin_height_m: default_stone_origin_h(),
            origin_forward_m: default_stone_origin_fwd(),
            origin_right_m: default_stone_origin_right(),
            range_m: default_stone_range_m(),
            arc_height_m: default_stone_arc_h(),
            throw_speed_mps: default_stone_speed(),
            throw_rate_hz: default_stone_rate(),
            stones_per_throw: default_stones_per_throw(),
            stone_size_m: default_stone_size_m(),
            yaw_offset_deg: 0.0,
            spread_deg: default_stone_spread_deg(),
            color_hex: default_stone_color(),
            dust_impact: true,
            continuous: true,
            single_shot: false,
        }
    }
}

impl StoneThrowConfig {
    pub fn sanitize(&mut self) {
        clamp_finite(&mut self.origin_height_m, 0.2, 200.0, default_stone_origin_h());
        clamp_finite(&mut self.origin_forward_m, -50.0, 50.0, default_stone_origin_fwd());
        clamp_finite(&mut self.origin_right_m, -50.0, 50.0, default_stone_origin_right());
        clamp_finite(&mut self.range_m, 2.0, 120.0, default_stone_range_m());
        clamp_finite(&mut self.arc_height_m, 0.2, 50.0, default_stone_arc_h());
        clamp_finite(&mut self.throw_speed_mps, 4.0, 45.0, default_stone_speed());
        clamp_finite(&mut self.throw_rate_hz, 0.2, 6.0, default_stone_rate());
        self.stones_per_throw = self.stones_per_throw.clamp(1, 6);
        clamp_finite(&mut self.stone_size_m, 0.08, 1.5, default_stone_size_m());
        clamp_finite(&mut self.yaw_offset_deg, -180.0, 180.0, 0.0);
        clamp_finite(&mut self.spread_deg, 0.0, 60.0, default_stone_spread_deg());
        sanitize_hex(&mut self.color_hex, "#78716c");
        if self.single_shot {
            self.continuous = false;
        }
    }

    pub fn flight_time_sec(&self) -> f64 {
        (self.range_m.clamp(2.0, 120.0) / self.throw_speed_mps.clamp(4.0, 45.0)).clamp(0.25, 8.0)
    }

    /// Total duration of a single stone throw (flight + ground bounce + brief dust fade).
    pub fn single_throw_duration_sec(&self) -> f64 {
        let volley_span = (self.stones_per_throw.clamp(1, 6).saturating_sub(1) as f64) * 0.055;
        (volley_span + self.flight_time_sec() + 0.42).max(0.5)
    }
}

pub fn parse_stone_throw_config(anim: &Anim) -> StoneThrowConfig {
    let mut cfg: StoneThrowConfig = anim
        .data
        .get("stone_throw")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_stone_throw_config(anim: &mut Anim, cfg: &StoneThrowConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if clean.single_shot {
        anim.duration_sec = clean.single_throw_duration_sec();
    }
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("stone_throw".into(), val);
    }
}

// ===========================================================================
// 3. Tear Gas Config (`shoot_tear_gas` -> `anim.data["tear_gas"]`)
// ===========================================================================

fn default_tg_origin_h() -> f64 {
    1.6
}
fn default_tg_origin_fwd() -> f64 {
    0.5
}
fn default_tg_origin_right() -> f64 {
    0.15
}
fn default_tg_range_m() -> f64 {
    32.0
}
fn default_tg_arc_h() -> f64 {
    12.0
}
fn default_tg_flight_sec() -> f64 {
    2.2
}
fn default_tg_salvo_count() -> u32 {
    1
}
fn default_tg_salvo_interval() -> f64 {
    0.5
}
fn default_tg_salvo_spread() -> f64 {
    16.0
}
fn default_tg_smoke_radius() -> f64 {
    6.5
}
fn default_tg_smoke_height() -> f64 {
    5.5
}
fn default_tg_smoke_duration() -> f64 {
    12.0
}
fn default_tg_smoke_density() -> f64 {
    1.2
}
fn default_tg_smoke_color() -> String {
    "#f8fafc".to_string()
}
fn default_tg_wind_dir() -> f64 {
    45.0
}
fn default_tg_wind_speed() -> f64 {
    1.0
}
fn default_tg_refire_interval() -> f64 {
    5.0
}

/// Configuration for a 3D Tear Gas canister launch animation stored inside `anim.data["tear_gas"]`.
/// Canisters launch upward in a high ballistic arc; upon reaching ground (`altitude = 0.0`),
/// dense white smoke erupts and slowly dissipates away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TearGasConfig {
    /// Launch height above bound marker base (meters).
    #[serde(default = "default_tg_origin_h")]
    pub origin_height_m: f64,
    /// Offset forward along marker facing axis (`+Z`) in meters.
    #[serde(default = "default_tg_origin_fwd")]
    pub origin_forward_m: f64,
    /// Offset right along marker lateral axis in meters.
    #[serde(default = "default_tg_origin_right")]
    pub origin_right_m: f64,
    /// Horizontal landing distance from launcher (meters).
    #[serde(default = "default_tg_range_m")]
    pub range_m: f64,
    /// High ballistic arc apex ABOVE launch height (meters).
    #[serde(default = "default_tg_arc_h")]
    pub arc_height_m: f64,
    /// Time in seconds from launch until the canister impacts ground (`altitude = 0`).
    #[serde(default = "default_tg_flight_sec")]
    pub flight_time_sec: f64,
    /// Horizontal launch aim relative to marker heading in degrees (`-180..+180`).
    #[serde(default)]
    pub yaw_offset_deg: f64,
    /// Number of canisters fired per volley (`1..5`).
    #[serde(default = "default_tg_salvo_count")]
    pub salvo_count: u32,
    /// Delay between consecutive canisters inside a multi-canister salvo (seconds).
    #[serde(default = "default_tg_salvo_interval")]
    pub salvo_interval_sec: f64,
    /// Fan spread across canisters in a multi-canister salvo (degrees).
    #[serde(default = "default_tg_salvo_spread")]
    pub salvo_spread_deg: f64,
    /// Maximum ground smoke cloud radius after impact (meters).
    #[serde(default = "default_tg_smoke_radius")]
    pub smoke_radius_m: f64,
    /// Maximum billowing smoke plume height after impact (meters).
    #[serde(default = "default_tg_smoke_height")]
    pub smoke_height_m: f64,
    /// Duration (seconds) that the white smoke lingers and slowly fades to invisible after impact.
    #[serde(default = "default_tg_smoke_duration")]
    pub smoke_duration_sec: f64,
    /// Smoke opacity/thickness multiplier (`0.3..2.5`).
    #[serde(default = "default_tg_smoke_density")]
    pub smoke_density: f64,
    /// Smoke color (`#f8fafc` pure white by default).
    #[serde(default = "default_tg_smoke_color")]
    pub smoke_color_hex: String,
    /// Compass wind bearing (`0° = North, 90° = East`) gently drifting the smoke cloud.
    #[serde(default = "default_tg_wind_dir")]
    pub wind_dir_deg: f64,
    /// Wind drift speed in m/s (`0..15`).
    #[serde(default = "default_tg_wind_speed")]
    pub wind_speed_mps: f64,
    /// When `false` (default): fires one salvo, smoke blooms at altitude 0 and slowly fades away.
    /// When `true`: repeatedly fires canisters every `refire_interval_sec` until Stopped.
    #[serde(default)]
    pub continuous: bool,
    /// Interval between repeated launches when `continuous` is `true` (seconds).
    #[serde(default = "default_tg_refire_interval")]
    pub refire_interval_sec: f64,
}

impl Default for TearGasConfig {
    fn default() -> Self {
        Self {
            origin_height_m: default_tg_origin_h(),
            origin_forward_m: default_tg_origin_fwd(),
            origin_right_m: default_tg_origin_right(),
            range_m: default_tg_range_m(),
            arc_height_m: default_tg_arc_h(),
            flight_time_sec: default_tg_flight_sec(),
            yaw_offset_deg: 0.0,
            salvo_count: default_tg_salvo_count(),
            salvo_interval_sec: default_tg_salvo_interval(),
            salvo_spread_deg: default_tg_salvo_spread(),
            smoke_radius_m: default_tg_smoke_radius(),
            smoke_height_m: default_tg_smoke_height(),
            smoke_duration_sec: default_tg_smoke_duration(),
            smoke_density: default_tg_smoke_density(),
            smoke_color_hex: default_tg_smoke_color(),
            wind_dir_deg: default_tg_wind_dir(),
            wind_speed_mps: default_tg_wind_speed(),
            continuous: false,
            refire_interval_sec: default_tg_refire_interval(),
        }
    }
}

impl TearGasConfig {
    pub fn sanitize(&mut self) {
        clamp_finite(&mut self.origin_height_m, 0.2, 200.0, default_tg_origin_h());
        clamp_finite(&mut self.origin_forward_m, -50.0, 50.0, default_tg_origin_fwd());
        clamp_finite(&mut self.origin_right_m, -50.0, 50.0, default_tg_origin_right());
        clamp_finite(&mut self.range_m, 4.0, 180.0, default_tg_range_m());
        clamp_finite(&mut self.arc_height_m, 1.0, 90.0, default_tg_arc_h());
        clamp_finite(&mut self.flight_time_sec, 0.5, 10.0, default_tg_flight_sec());
        clamp_finite(&mut self.yaw_offset_deg, -180.0, 180.0, 0.0);
        self.salvo_count = self.salvo_count.clamp(1, 5);
        clamp_finite(&mut self.salvo_interval_sec, 0.1, 5.0, default_tg_salvo_interval());
        clamp_finite(&mut self.salvo_spread_deg, 0.0, 60.0, default_tg_salvo_spread());
        clamp_finite(&mut self.smoke_radius_m, 1.5, 40.0, default_tg_smoke_radius());
        clamp_finite(&mut self.smoke_height_m, 1.5, 30.0, default_tg_smoke_height());
        clamp_finite(&mut self.smoke_duration_sec, 2.0, 60.0, default_tg_smoke_duration());
        clamp_finite(&mut self.smoke_density, 0.3, 2.5, default_tg_smoke_density());
        sanitize_hex(&mut self.smoke_color_hex, "#f8fafc");
        if self.wind_dir_deg.is_finite() {
            self.wind_dir_deg = self.wind_dir_deg.rem_euclid(360.0);
        } else {
            self.wind_dir_deg = default_tg_wind_dir();
        }
        clamp_finite(&mut self.wind_speed_mps, 0.0, 15.0, default_tg_wind_speed());
        clamp_finite(&mut self.refire_interval_sec, 1.5, 30.0, default_tg_refire_interval());
    }

    /// Total duration of one full salvo from first launch until the final smoke cloud completely fades.
    pub fn total_cycle_duration_sec(&self) -> f64 {
        let salvo_span = (self.salvo_count.saturating_sub(1) as f64) * self.salvo_interval_sec;
        (salvo_span + self.flight_time_sec + self.smoke_duration_sec).max(1.0)
    }

    /// Evaluates canister position `(horiz_dist_m, world_y, tang_horiz, tang_vert)` along the
    /// high ballistic arc from `start_y` landing precisely at ground (`altitude = 0.0`).
    pub fn eval_canister_arc(&self, start_y: f64, t: f64) -> (f64, f64, f64, f64) {
        let (h, y, th, tv) =
            eval_parabola_between(self.range_m, start_y.max(0.2), 0.0, self.arc_height_m, t);
        (h, y.max(0.0), th, tv)
    }

    /// Horizontal wind unit vector `(wx, wz)` in Bevy world XZ (`+X = East, -Z = North`).
    pub fn wind_dir_xz(&self) -> (f64, f64) {
        let rad = self.wind_dir_deg.to_radians();
        (rad.sin(), -rad.cos())
    }
}

pub fn parse_tear_gas_config(anim: &Anim) -> TearGasConfig {
    let mut cfg: TearGasConfig = anim
        .data
        .get("tear_gas")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_tear_gas_config(anim: &mut Anim, cfg: &TearGasConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if !clean.continuous {
        anim.duration_sec = clean.total_cycle_duration_sec();
    }
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("tear_gas".into(), val);
    }
}

// ===========================================================================
// 4. Bullet Shoot Config (`shoot_bullet` -> `anim.data["bullet"]`)
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulletFireMode {
    #[default]
    Auto,
    Burst,
    Single,
}

impl BulletFireMode {
    pub const ALL: &'static [Self] = &[Self::Auto, Self::Burst, Self::Single];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Otomatis (Auto)",
            Self::Burst => "Burst (Rentetan)",
            Self::Single => "Tunggal (Semi-Auto)",
        }
    }
}

fn default_bullet_origin_h() -> f64 {
    1.4
}
fn default_bullet_origin_fwd() -> f64 {
    0.7
}
fn default_bullet_origin_right() -> f64 {
    0.15
}
fn default_bullet_max_range() -> f64 {
    60.0
}
fn default_bullet_speed() -> f64 {
    85.0
}
fn default_bullet_rate() -> f64 {
    5.0
}
fn default_burst_count() -> u32 {
    3
}
fn default_burst_pause() -> f64 {
    0.65
}
fn default_bullet_spread() -> f64 {
    1.8
}
fn default_tracer_length() -> f64 {
    2.2
}
fn default_tracer_thickness() -> f64 {
    0.11
}
fn default_bullet_color() -> String {
    "#fde047".to_string()
}

/// Configuration for a 3D Gunfire / Bullet shoot animation stored inside `anim.data["bullet"]`.
/// Bullets fly straight forward from the weapon muzzle and disappear once reaching `max_range_m`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BulletShootConfig {
    /// Muzzle height above bound marker base (meters).
    #[serde(default = "default_bullet_origin_h")]
    pub origin_height_m: f64,
    /// Offset forward along marker facing axis (`+Z`) in meters.
    #[serde(default = "default_bullet_origin_fwd")]
    pub origin_forward_m: f64,
    /// Offset right along marker lateral axis in meters.
    #[serde(default = "default_bullet_origin_right")]
    pub origin_right_m: f64,
    /// Maximum travel distance in meters; bullets disappear upon reaching this distance.
    #[serde(default = "default_bullet_max_range")]
    pub max_range_m: f64,
    /// Visual bullet flight speed in m/s (`15..250`).
    #[serde(default = "default_bullet_speed")]
    pub bullet_speed_mps: f64,
    /// Firing pattern: continuous auto, 3-round bursts, or deliberate single shots.
    #[serde(default)]
    pub fire_mode: BulletFireMode,
    /// Rounds fired per second (`0.5..18.0`).
    #[serde(default = "default_bullet_rate")]
    pub fire_rate_rps: f64,
    /// Rounds per burst when `fire_mode == Burst` (`2..6`).
    #[serde(default = "default_burst_count")]
    pub burst_count: u32,
    /// Pause duration between bursts in seconds (`0.2..3.0`).
    #[serde(default = "default_burst_pause")]
    pub burst_pause_sec: f64,
    /// Horizontal aim relative to marker heading in degrees (`-180..+180`).
    #[serde(default)]
    pub yaw_offset_deg: f64,
    /// Vertical aim elevation angle in degrees (`-45..+60`, `0` = level forward).
    #[serde(default)]
    pub pitch_deg: f64,
    /// Angular dispersion cone in degrees (`0..20`).
    #[serde(default = "default_bullet_spread")]
    pub spread_deg: f64,
    /// Visual length of bullet/tracer streak in meters (`0.4..8.0`).
    #[serde(default = "default_tracer_length")]
    pub tracer_length_m: f64,
    /// Visual radius/thickness of bullet core in meters (`0.04..0.4`).
    #[serde(default = "default_tracer_thickness")]
    pub tracer_thickness_m: f64,
    /// Tracer/bullet glow color (`#RRGGBB`).
    #[serde(default = "default_bullet_color")]
    pub color_hex: String,
    /// Show dynamic muzzle flash burst at weapon origin on each fired round.
    #[serde(default = "default_true")]
    pub muzzle_flash: bool,
    /// When `true`, fires continuously in Play Skenario until Stopped.
    #[serde(default = "default_true")]
    pub continuous: bool,
    /// When `true`, fires only ONE bullet per Play trigger and finishes once it reaches `max_range_m`.
    #[serde(default)]
    pub single_shot: bool,
}

impl Default for BulletShootConfig {
    fn default() -> Self {
        Self {
            origin_height_m: default_bullet_origin_h(),
            origin_forward_m: default_bullet_origin_fwd(),
            origin_right_m: default_bullet_origin_right(),
            max_range_m: default_bullet_max_range(),
            bullet_speed_mps: default_bullet_speed(),
            fire_mode: BulletFireMode::Auto,
            fire_rate_rps: default_bullet_rate(),
            burst_count: default_burst_count(),
            burst_pause_sec: default_burst_pause(),
            yaw_offset_deg: 0.0,
            pitch_deg: 0.0,
            spread_deg: default_bullet_spread(),
            tracer_length_m: default_tracer_length(),
            tracer_thickness_m: default_tracer_thickness(),
            color_hex: default_bullet_color(),
            muzzle_flash: true,
            continuous: true,
            single_shot: false,
        }
    }
}

impl BulletShootConfig {
    pub fn sanitize(&mut self) {
        clamp_finite(&mut self.origin_height_m, 0.2, 200.0, default_bullet_origin_h());
        clamp_finite(&mut self.origin_forward_m, -50.0, 50.0, default_bullet_origin_fwd());
        clamp_finite(&mut self.origin_right_m, -50.0, 50.0, default_bullet_origin_right());
        clamp_finite(&mut self.max_range_m, 5.0, 300.0, default_bullet_max_range());
        clamp_finite(&mut self.bullet_speed_mps, 15.0, 250.0, default_bullet_speed());
        clamp_finite(&mut self.fire_rate_rps, 0.5, 18.0, default_bullet_rate());
        self.burst_count = self.burst_count.clamp(2, 6);
        clamp_finite(&mut self.burst_pause_sec, 0.2, 3.0, default_burst_pause());
        clamp_finite(&mut self.yaw_offset_deg, -180.0, 180.0, 0.0);
        clamp_finite(&mut self.pitch_deg, -45.0, 60.0, 0.0);
        clamp_finite(&mut self.spread_deg, 0.0, 20.0, default_bullet_spread());
        clamp_finite(&mut self.tracer_length_m, 0.4, 8.0, default_tracer_length());
        clamp_finite(&mut self.tracer_thickness_m, 0.04, 0.4, default_tracer_thickness());
        sanitize_hex(&mut self.color_hex, "#fde047");
        if self.single_shot {
            self.continuous = false;
        }
    }

    /// Time in seconds until a bullet reaches `max_range_m` and disappears.
    pub fn bullet_lifetime_sec(&self) -> f64 {
        (self.max_range_m.clamp(5.0, 300.0) / self.bullet_speed_mps.clamp(15.0, 250.0)).max(0.05)
    }

    /// Total duration of a single-shot firing cycle (bullet flight + muzzle flash/smoke fade).
    pub fn single_shot_duration_sec(&self) -> f64 {
        self.bullet_lifetime_sec().max(0.28) + 0.08
    }

    /// Returns the exact firing timestamp (seconds) of the `shot_index`-th bullet (0-based).
    pub fn shot_fire_time_sec(&self, shot_index: u64) -> f64 {
        let rate = self.fire_rate_rps.clamp(0.5, 18.0);
        match self.fire_mode {
            BulletFireMode::Auto => (shot_index as f64) / rate,
            BulletFireMode::Single => {
                // Deliberate semi-auto spacing: minimum 0.45s between shots
                let interval = (1.0 / rate).max(0.45);
                (shot_index as f64) * interval
            }
            BulletFireMode::Burst => {
                let bc = self.burst_count.clamp(2, 6) as u64;
                let intra = (1.0 / rate.max(4.0)).min(0.18);
                let pause = self.burst_pause_sec.clamp(0.2, 3.0);
                let cycle = (bc as f64) * intra + pause;
                let burst_idx = shot_index / bc;
                let in_burst = shot_index % bc;
                (burst_idx as f64) * cycle + (in_burst as f64) * intra
            }
        }
    }

    /// Returns the traveled distance in meters for a bullet fired `age_sec` ago, or `None`
    /// if the bullet hasn't been fired yet or has already reached `max_range_m` and disappeared.
    pub fn bullet_distance_at_age(&self, age_sec: f64) -> Option<f64> {
        if age_sec < 0.0 {
            return None;
        }
        let dist = age_sec * self.bullet_speed_mps.clamp(15.0, 250.0);
        if dist >= self.max_range_m.clamp(5.0, 300.0) {
            None
        } else {
            Some(dist)
        }
    }
}

pub fn parse_bullet_shoot_config(anim: &Anim) -> BulletShootConfig {
    let mut cfg: BulletShootConfig = anim
        .data
        .get("bullet")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_bullet_shoot_config(anim: &mut Anim, cfg: &BulletShootConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if clean.single_shot {
        anim.duration_sec = clean.single_shot_duration_sec();
    }
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("bullet".into(), val);
    }
}

// ===========================================================================
// 5. APAR Spray Config (`shoot_apar` -> `anim.data["apar"]`)
// ===========================================================================

fn default_apar_origin_h() -> f64 {
    1.1
}
fn default_apar_origin_fwd() -> f64 {
    0.5
}
fn default_apar_origin_right() -> f64 {
    0.15
}
fn default_apar_range_m() -> f64 {
    7.0
}
fn default_apar_pitch_deg() -> f64 {
    -8.0
}
fn default_apar_pressure() -> f64 {
    1.2
}
fn default_apar_cone_spread() -> f64 {
    0.55
}
fn default_apar_density() -> f64 {
    1.2
}
fn default_apar_color() -> String {
    "#f8fafc".to_string()
}
fn default_apar_sweep_angle() -> f64 {
    18.0
}
fn default_apar_sweep_speed() -> f64 {
    0.55
}

/// Configuration for a 3D APAR (Fire Extinguisher) spray animation stored inside `anim.data["apar"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AparSprayConfig {
    /// Nozzle height above bound marker base (meters).
    #[serde(default = "default_apar_origin_h")]
    pub origin_height_m: f64,
    /// Offset forward along marker facing axis (`+Z`) in meters.
    #[serde(default = "default_apar_origin_fwd")]
    pub origin_forward_m: f64,
    /// Offset right along marker lateral axis in meters.
    #[serde(default = "default_apar_origin_right")]
    pub origin_right_m: f64,
    /// Forward spray reach in meters (`1.5..30.0`).
    #[serde(default = "default_apar_range_m")]
    pub range_m: f64,
    /// Vertical aim pitch in degrees (`-60..+45`, default `-8°` toward fire base).
    #[serde(default = "default_apar_pitch_deg")]
    pub pitch_deg: f64,
    /// Horizontal aim relative to marker heading in degrees (`-180..+180`).
    #[serde(default)]
    pub yaw_offset_deg: f64,
    /// Discharge pressure / jet velocity multiplier (`0.4..2.5`).
    #[serde(default = "default_apar_pressure")]
    pub pressure: f64,
    /// Conical mist/powder expansion width factor (`0.15..1.5`).
    #[serde(default = "default_apar_cone_spread")]
    pub cone_spread: f64,
    /// Cloud density / opacity multiplier (`0.3..2.2`).
    #[serde(default = "default_apar_density")]
    pub density: f64,
    /// Extinguishing agent color (Dry powder white `#f8fafc`, CO2 `#e0f2fe`, Foam `#fefce8`).
    #[serde(default = "default_apar_color")]
    pub color_hex: String,
    /// Enables left–right PASS sweeping motion at the base of the fire.
    #[serde(default = "default_true")]
    pub sweep_enabled: bool,
    /// Half-angle of left–right sweep in degrees (`5..90`).
    #[serde(default = "default_apar_sweep_angle")]
    pub sweep_angle_deg: f64,
    /// Sweep oscillation speed in Hz (`0.1..2.0`).
    #[serde(default = "default_apar_sweep_speed")]
    pub sweep_speed: f64,
    /// When `true`, sprays continuously in Play Skenario until Stopped.
    #[serde(default = "default_true")]
    pub continuous: bool,
}

impl Default for AparSprayConfig {
    fn default() -> Self {
        Self {
            origin_height_m: default_apar_origin_h(),
            origin_forward_m: default_apar_origin_fwd(),
            origin_right_m: default_apar_origin_right(),
            range_m: default_apar_range_m(),
            pitch_deg: default_apar_pitch_deg(),
            yaw_offset_deg: 0.0,
            pressure: default_apar_pressure(),
            cone_spread: default_apar_cone_spread(),
            density: default_apar_density(),
            color_hex: default_apar_color(),
            sweep_enabled: true,
            sweep_angle_deg: default_apar_sweep_angle(),
            sweep_speed: default_apar_sweep_speed(),
            continuous: true,
        }
    }
}

impl AparSprayConfig {
    pub fn sanitize(&mut self) {
        clamp_finite(&mut self.origin_height_m, 0.2, 200.0, default_apar_origin_h());
        clamp_finite(&mut self.origin_forward_m, -50.0, 50.0, default_apar_origin_fwd());
        clamp_finite(&mut self.origin_right_m, -50.0, 50.0, default_apar_origin_right());
        clamp_finite(&mut self.range_m, 1.5, 30.0, default_apar_range_m());
        clamp_finite(&mut self.pitch_deg, -60.0, 45.0, default_apar_pitch_deg());
        clamp_finite(&mut self.yaw_offset_deg, -180.0, 180.0, 0.0);
        clamp_finite(&mut self.pressure, 0.4, 2.5, default_apar_pressure());
        clamp_finite(&mut self.cone_spread, 0.15, 1.5, default_apar_cone_spread());
        clamp_finite(&mut self.density, 0.3, 2.2, default_apar_density());
        sanitize_hex(&mut self.color_hex, "#f8fafc");
        clamp_finite(&mut self.sweep_angle_deg, 5.0, 90.0, default_apar_sweep_angle());
        clamp_finite(&mut self.sweep_speed, 0.1, 2.0, default_apar_sweep_speed());
    }

    pub fn effective_yaw_offset_deg(&self, time_sec: f64) -> f64 {
        if self.sweep_enabled {
            let phase = time_sec * self.sweep_speed.clamp(0.1, 2.0) * std::f64::consts::TAU;
            self.yaw_offset_deg + phase.sin() * self.sweep_angle_deg.clamp(5.0, 90.0)
        } else {
            self.yaw_offset_deg
        }
    }
}

pub fn parse_apar_spray_config(anim: &Anim) -> AparSprayConfig {
    let mut cfg: AparSprayConfig = anim
        .data
        .get("apar")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_apar_spray_config(anim: &mut Anim, cfg: &AparSprayConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("apar".into(), val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn dummy_shoot_anim(kind: ShootAnimKind) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: Some(Uuid::new_v4()),
            name: kind.default_name(1),
            anim_type: kind.anim_type_str().into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: kind.default_duration_sec(),
            replay: false,
            autoplay: false,
            geom: Vec::new(),
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn water_cannon_parabola_hits_exact_nozzle_peak_and_ground_range() {
        let cfg = WaterCannonConfig {
            nozzle_height_m: 3.0,
            range_m: 30.0,
            arc_height_m: 4.0,
            ..Default::default()
        };

        let (x0, y0, _, _) = cfg.eval_parabola(0.0);
        assert!((x0 - 0.0).abs() < 1e-6);
        assert!((y0 - 3.0).abs() < 1e-6);

        let (x1, y1, _, _) = cfg.eval_parabola(1.0);
        assert!((x1 - 30.0).abs() < 1e-6);
        assert!(y1.abs() < 1e-6);

        let mut max_y = 0.0_f64;
        for i in 0..=1000 {
            let (_, y, _, _) = cfg.eval_parabola(i as f64 / 1000.0);
            max_y = max_y.max(y);
        }
        assert!(
            (max_y - 7.0).abs() < 1e-3,
            "expected peak 7.0m, got {max_y}"
        );
    }

    #[test]
    fn water_cannon_flat_arc_zero_matches_gravity_drop() {
        let cfg = WaterCannonConfig {
            nozzle_height_m: 4.0,
            range_m: 20.0,
            arc_height_m: 0.0,
            ..Default::default()
        };
        let (x0, y0, th, tv) = cfg.eval_parabola(0.0);
        assert!((x0 - 0.0).abs() < 1e-6);
        assert!((y0 - 4.0).abs() < 1e-6);
        assert!((th - 1.0).abs() < 1e-6 && tv.abs() < 1e-6);

        let (xm, ym, _, _) = cfg.eval_parabola(0.5);
        assert!((xm - 10.0).abs() < 1e-6);
        assert!((ym - 3.0).abs() < 1e-6);

        let (x1, y1, _, _) = cfg.eval_parabola(1.0);
        assert!((x1 - 20.0).abs() < 1e-6);
        assert!(y1.abs() < 1e-6);
    }

    #[test]
    fn water_cannon_sweep_oscillates_symmetrically() {
        let cfg = WaterCannonConfig {
            yaw_offset_deg: 10.0,
            sweep_enabled: true,
            sweep_angle_deg: 30.0,
            sweep_speed: 1.0,
            ..Default::default()
        };
        assert!((cfg.effective_yaw_offset_deg(0.0) - 10.0).abs() < 1e-6);
        assert!((cfg.effective_yaw_offset_deg(0.25) - 40.0).abs() < 1e-6);
        assert!((cfg.effective_yaw_offset_deg(0.75) - (-20.0)).abs() < 1e-6);
    }

    #[test]
    fn tear_gas_arc_reaches_altitude_zero_from_ground_and_rooftop() {
        let cfg = TearGasConfig {
            range_m: 36.0,
            arc_height_m: 14.0,
            ..Default::default()
        };
        for start_y in [1.6_f64, 18.5_f64] {
            let (x0, y0, _, _) = cfg.eval_canister_arc(start_y, 0.0);
            assert!(x0.abs() < 1e-6);
            assert!((y0 - start_y).abs() < 1e-6);

            let (x1, y1, _, _) = cfg.eval_canister_arc(start_y, 1.0);
            assert!((x1 - 36.0).abs() < 1e-6);
            assert!(y1.abs() < 1e-6, "must land at altitude 0, got {y1}");

            let mut max_y = 0.0_f64;
            for i in 0..=1000 {
                let (_, y, _, _) = cfg.eval_canister_arc(start_y, i as f64 / 1000.0);
                max_y = max_y.max(y);
            }
            assert!(
                (max_y - (start_y + 14.0)).abs() < 1e-2,
                "expected peak {}, got {max_y}",
                start_y + 14.0
            );
        }
    }

    #[test]
    fn bullet_vanishes_at_exact_max_range_and_burst_timing_monotonic() {
        let cfg = BulletShootConfig {
            max_range_m: 50.0,
            bullet_speed_mps: 100.0,
            fire_mode: BulletFireMode::Burst,
            fire_rate_rps: 10.0,
            burst_count: 3,
            burst_pause_sec: 0.6,
            ..Default::default()
        };
        assert_eq!(cfg.bullet_distance_at_age(-0.1), None);
        assert!((cfg.bullet_distance_at_age(0.25).unwrap() - 25.0).abs() < 1e-6);
        assert_eq!(cfg.bullet_distance_at_age(0.5), None);
        assert_eq!(cfg.bullet_distance_at_age(0.8), None);

        let mut prev = -1.0_f64;
        for idx in 0..10 {
            let t = cfg.shot_fire_time_sec(idx);
            assert!(t > prev, "shot times must strictly increase");
            prev = t;
        }
        // Gap between shot #2 and shot #3 (burst pause) must exceed intra-burst spacing
        let intra = cfg.shot_fire_time_sec(1) - cfg.shot_fire_time_sec(0);
        let pause = cfg.shot_fire_time_sec(3) - cfg.shot_fire_time_sec(2);
        assert!(pause > intra * 2.0);
    }

    #[test]
    fn all_shoot_kinds_roundtrip_config_and_helpers() {
        for &kind in ShootAnimKind::ALL {
            let mut anim = dummy_shoot_anim(kind);
            assert!(is_shoot_anim(&anim));
            assert_eq!(shoot_anim_kind(&anim), Some(kind));
            match kind {
                ShootAnimKind::WaterCannon => {
                    let cfg = WaterCannonConfig {
                        range_m: 35.0,
                        ..Default::default()
                    };
                    write_water_cannon_config(&mut anim, &cfg);
                    assert_eq!(parse_water_cannon_config(&anim).range_m, 35.0);
                }
                ShootAnimKind::StoneThrow => {
                    let cfg = StoneThrowConfig {
                        range_m: 22.0,
                        stones_per_throw: 3,
                        ..Default::default()
                    };
                    write_stone_throw_config(&mut anim, &cfg);
                    let parsed = parse_stone_throw_config(&anim);
                    assert_eq!(parsed.range_m, 22.0);
                    assert_eq!(parsed.stones_per_throw, 3);
                }
                ShootAnimKind::TearGas => {
                    let cfg = TearGasConfig {
                        range_m: 40.0,
                        smoke_duration_sec: 14.0,
                        flight_time_sec: 2.5,
                        salvo_count: 2,
                        salvo_interval_sec: 0.5,
                        continuous: false,
                        ..Default::default()
                    };
                    write_tear_gas_config(&mut anim, &cfg);
                    let parsed = parse_tear_gas_config(&anim);
                    assert_eq!(parsed.range_m, 40.0);
                    assert!((anim.duration_sec - 17.0).abs() < 1e-6);
                    assert!(!shoot_anim_is_continuous(&anim));
                }
                ShootAnimKind::Bullet => {
                    let cfg = BulletShootConfig {
                        max_range_m: 75.0,
                        ..Default::default()
                    };
                    write_bullet_shoot_config(&mut anim, &cfg);
                    assert_eq!(parse_bullet_shoot_config(&anim).max_range_m, 75.0);

                    // Single-shot mode disables continuous and sets finite effective duration
                    let single_cfg = BulletShootConfig {
                        max_range_m: 80.0,
                        bullet_speed_mps: 80.0,
                        continuous: true,
                        single_shot: true,
                        ..Default::default()
                    };
                    write_bullet_shoot_config(&mut anim, &single_cfg);
                    let parsed_single = parse_bullet_shoot_config(&anim);
                    assert!(parsed_single.single_shot);
                    assert!(!parsed_single.continuous);
                    assert!(!shoot_anim_is_continuous(&anim));
                    assert!((shoot_anim_effective_duration_sec(&anim) - 1.08).abs() < 1e-6);
                }
                ShootAnimKind::Apar => {
                    let cfg = AparSprayConfig {
                        range_m: 9.5,
                        sweep_enabled: true,
                        sweep_angle_deg: 20.0,
                        sweep_speed: 1.0,
                        ..Default::default()
                    };
                    write_apar_spray_config(&mut anim, &cfg);
                    let parsed = parse_apar_spray_config(&anim);
                    assert_eq!(parsed.range_m, 9.5);
                    assert!((parsed.effective_yaw_offset_deg(0.25) - 20.0).abs() < 1e-6);
                }
            }
            assert!(shoot_anim_range_m(&anim) > 0.0);
        }
    }
}
