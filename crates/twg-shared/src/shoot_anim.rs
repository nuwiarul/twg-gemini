use serde::{Deserialize, Serialize};

use crate::models::Anim;

pub const ANIM_TYPE_SHOOT_WATER_CANNON: &str = "shoot_water_cannon";

/// Returns `true` if `anim` belongs to the Shoot animation category.
#[inline]
pub fn is_shoot_anim(anim: &Anim) -> bool {
    anim.anim_type == "shoot" || anim.anim_type.starts_with("shoot_")
}

/// Returns `true` if `anim` is a Water Cannon shoot animation.
#[inline]
pub fn is_water_cannon_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_SHOOT_WATER_CANNON || anim.anim_type == "shoot"
}

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
fn default_continuous() -> bool {
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
    #[serde(default = "default_continuous")]
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
            continuous: default_continuous(),
        }
    }
}

impl WaterCannonConfig {
    pub fn sanitize(&mut self) {
        let clamp_f = |v: &mut f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() {
                *v = v.clamp(lo, hi);
            } else {
                *v = def;
            }
        };
        clamp_f(&mut self.nozzle_height_m, 0.2, 200.0, default_nozzle_height_m());
        clamp_f(&mut self.nozzle_forward_m, -50.0, 50.0, default_nozzle_forward_m());
        clamp_f(&mut self.nozzle_right_m, -50.0, 50.0, 0.0);
        clamp_f(&mut self.range_m, 2.0, 180.0, default_range_m());
        clamp_f(&mut self.arc_height_m, 0.0, 80.0, default_arc_height_m());
        clamp_f(&mut self.pressure_bar, 1.0, 10.0, default_pressure_bar());
        clamp_f(&mut self.yaw_offset_deg, -180.0, 180.0, 0.0);
        clamp_f(&mut self.spread, 0.0, 1.0, default_spread());
        clamp_f(&mut self.sweep_angle_deg, 5.0, 120.0, default_sweep_angle_deg());
        clamp_f(&mut self.sweep_speed, 0.05, 2.5, default_sweep_speed());
        if !self.color_hex.starts_with('#') || self.color_hex.len() < 7 {
            self.color_hex = default_color_hex();
        }
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
        let b = if dh <= 1e-6 {
            0.0
        } else {
            2.0 * dh + 2.0 * (dh * (dh + h0)).sqrt()
        };
        let a = -(h0 + b);
        (a, b, h0)
    }

    /// Evaluates `(horizontal_dist_m, height_m, tangent_horiz, tangent_vert)` along the exact
    /// parabolic trajectory for normalized parameter `t ∈ [0.0, 1.0]`.
    pub fn eval_parabola(&self, t: f64) -> (f64, f64, f64, f64) {
        let tc = t.clamp(0.0, 1.0);
        let range = self.range_m.clamp(2.0, 180.0);
        let (a, b, c) = self.parabola_coeffs();
        let horiz = range * tc;
        let height = (a * tc * tc + b * tc + c).max(0.0);
        let dh_dt = range;
        let dy_dt = 2.0 * a * tc + b;
        let len = dh_dt.hypot(dy_dt).max(1e-6);
        (horiz, height, dh_dt / len, dy_dt / len)
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

#[cfg(test)]
mod tests {
    use super::*;

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

        // Find numeric peak along 1000 samples — must match nozzle_height_m + arc_height_m = 7.0
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
}
