use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::Anim;

pub const ANIM_TYPE_TACTICAL_ARROW: &str = "tactical_arrow";

pub fn is_tactical_arrow_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_TACTICAL_ARROW
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TacticalArrowPreset {
    #[default]
    Attack,
    Evacuation,
    CrowdMovement,
    Cordon,
    Custom,
}

impl TacticalArrowPreset {
    pub const ALL: [Self; 5] = [
        Self::Attack,
        Self::Evacuation,
        Self::CrowdMovement,
        Self::Cordon,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Attack => "Serangan / Dorongan",
            Self::Evacuation => "Jalur Evakuasi",
            Self::CrowdMovement => "Pergerakan Massa",
            Self::Cordon => "Sektor Penyekatan",
            Self::Custom => "Kustom",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Attack => "SERANGAN",
            Self::Evacuation => "EVAKUASI",
            Self::CrowdMovement => "MASSA",
            Self::Cordon => "SEKAT",
            Self::Custom => "ALUR 3D",
        }
    }

    pub fn default_colors(self) -> (&'static str, &'static str) {
        match self {
            Self::Attack => ("#ef4444", "#fef08a"),
            Self::Evacuation => ("#22c55e", "#ecfdf5"),
            Self::CrowdMovement => ("#f59e0b", "#fef9c3"),
            Self::Cordon => ("#38bdf8", "#e0f2fe"),
            Self::Custom => ("#a855f7", "#f3e8ff"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TacticalArrowConfig {
    pub preset: TacticalArrowPreset,
    /// Primary ribbon body color hex (e.g. `"#ef4444"`).
    pub color: String,
    /// Animated marching chevron & glow accent color hex (e.g. `"#fef08a"`).
    pub accent_color: String,
    /// Ribbon body width in meters.
    pub width_m: f32,
    /// Elevation above terrain in meters.
    pub altitude_m: f32,
    /// Smooth curved path (Chaikin subdivision) vs sharp polyline corners.
    pub smooth_curve: bool,
    /// Marching chevron flow speed (cycles per second).
    pub flow_speed: f32,
    /// Distance between consecutive marching chevrons along the path (meters).
    pub chevron_spacing_m: f32,
    /// Arrowhead width multiplier relative to `width_m`.
    pub head_size_mult: f32,
    /// Grow/reveal animation duration when first played (seconds; `0` = instant full arrow).
    pub grow_duration_sec: f32,
    /// Hold duration after full reveal when `continuous == false` (seconds).
    pub hold_duration_sec: f32,
    /// Keep flowing continuously until manually stopped.
    pub continuous: bool,
    /// Optional start delay in seconds before appearing.
    pub delay_sec: f64,
    /// Overall opacity (`0.2..=1.0`).
    pub opacity: f32,
    /// Show optional floating tactical label at the path midpoint.
    pub show_label: bool,
    /// Optional label text displayed above the tactical arrow midpoint.
    pub label_text: String,
}

impl Default for TacticalArrowConfig {
    fn default() -> Self {
        let (c, a) = TacticalArrowPreset::Attack.default_colors();
        Self {
            preset: TacticalArrowPreset::Attack,
            color: c.to_string(),
            accent_color: a.to_string(),
            width_m: 3.5,
            altitude_m: 0.45,
            smooth_curve: true,
            flow_speed: 1.25,
            chevron_spacing_m: 10.0,
            head_size_mult: 1.95,
            grow_duration_sec: 1.2,
            hold_duration_sec: 10.0,
            continuous: true,
            delay_sec: 0.0,
            opacity: 0.88,
            show_label: true,
            label_text: String::new(),
        }
    }
}

impl TacticalArrowConfig {
    pub fn apply_preset(&mut self, preset: TacticalArrowPreset) {
        self.preset = preset;
        if preset != TacticalArrowPreset::Custom {
            let (c, a) = preset.default_colors();
            self.color = c.to_string();
            self.accent_color = a.to_string();
        }
    }

    pub fn sanitized(mut self) -> Self {
        if self.color.trim().is_empty() {
            self.color = self.preset.default_colors().0.to_string();
        }
        if self.accent_color.trim().is_empty() {
            self.accent_color = self.preset.default_colors().1.to_string();
        }
        self.width_m = if self.width_m.is_finite() {
            self.width_m.clamp(0.8, 30.0)
        } else {
            3.5
        };
        self.altitude_m = if self.altitude_m.is_finite() {
            self.altitude_m.clamp(0.15, 80.0)
        } else {
            0.45
        };
        self.flow_speed = if self.flow_speed.is_finite() {
            self.flow_speed.clamp(0.0, 5.0)
        } else {
            1.25
        };
        self.chevron_spacing_m = if self.chevron_spacing_m.is_finite() {
            self.chevron_spacing_m.clamp(3.0, 60.0)
        } else {
            10.0
        };
        self.head_size_mult = if self.head_size_mult.is_finite() {
            self.head_size_mult.clamp(1.2, 3.5)
        } else {
            1.95
        };
        self.grow_duration_sec = if self.grow_duration_sec.is_finite() {
            self.grow_duration_sec.clamp(0.0, 15.0)
        } else {
            1.2
        };
        self.hold_duration_sec = if self.hold_duration_sec.is_finite() {
            self.hold_duration_sec.clamp(0.5, 600.0)
        } else {
            10.0
        };
        self.delay_sec = if self.delay_sec.is_finite() {
            self.delay_sec.clamp(0.0, 600.0)
        } else {
            0.0
        };
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.2, 1.0)
        } else {
            0.88
        };
        self
    }

    pub fn effective_label(&self) -> Option<&str> {
        if !self.show_label {
            return None;
        }
        let t = self.label_text.trim();
        if t.is_empty() { None } else { Some(t) }
    }

    /// Finite total duration when `continuous == false` (seconds). Returns `0.0` when `continuous == true`.
    pub fn finite_duration_sec(&self) -> f64 {
        if self.continuous {
            return 0.0;
        }
        self.delay_sec + (self.grow_duration_sec as f64) + (self.hold_duration_sec as f64)
    }

    /// Deterministic evaluation at `elapsed_sec` for rendering both Operator & Layar Besar identically.
    pub fn evaluate_at(&self, elapsed_sec: f64) -> TacticalArrowEval {
        let elapsed = if elapsed_sec.is_finite() {
            elapsed_sec.max(0.0)
        } else {
            0.0
        };
        if elapsed < self.delay_sec {
            return TacticalArrowEval {
                visible: false,
                reveal_fraction: 0.0,
                flow_phase: 0.0,
                alpha: 0.0,
                finished: false,
            };
        }
        let local_t = elapsed - self.delay_sec;
        let grow = self.grow_duration_sec as f64;
        let reveal_fraction = if grow <= 1e-4 {
            1.0
        } else {
            let p = (local_t / grow).clamp(0.0, 1.0);
            // Smooth ease-out cubic so the arrow sweeps briskly and lands cleanly
            let inv = 1.0 - p;
            (1.0 - inv * inv * inv) as f32
        };

        let mut alpha = self.opacity;
        let mut finished = false;
        if !self.continuous {
            let active_total = grow + (self.hold_duration_sec as f64);
            if local_t >= active_total {
                finished = true;
                alpha = 0.0;
            } else {
                let rem = active_total - local_t;
                if rem < 0.4 {
                    alpha *= (rem / 0.4).clamp(0.0, 1.0) as f32;
                }
            }
        }
        let fade_in = (local_t / 0.25).clamp(0.0, 1.0) as f32;
        alpha *= fade_in;

        let flow_phase = ((local_t * self.flow_speed as f64) % 1.0) as f32;

        TacticalArrowEval {
            visible: !finished && alpha > 0.01 && reveal_fraction > 0.005,
            reveal_fraction,
            flow_phase,
            alpha,
            finished,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TacticalArrowEval {
    pub visible: bool,
    pub reveal_fraction: f32,
    pub flow_phase: f32,
    pub alpha: f32,
    pub finished: bool,
}

pub fn parse_tactical_arrow_config(anim: &Anim) -> TacticalArrowConfig {
    let raw = anim
        .data
        .as_object()
        .and_then(|o| o.get("tactical_arrow"))
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::from_value::<TacticalArrowConfig>(raw)
        .unwrap_or_default()
        .sanitized()
}

pub fn write_tactical_arrow_config(anim: &mut Anim, cfg: &TacticalArrowConfig) {
    let clean = cfg.clone().sanitized();
    let mut root = anim
        .data
        .as_object()
        .cloned()
        .unwrap_or_else(Map::new);
    if let Ok(v) = serde_json::to_value(&clean) {
        root.insert("tactical_arrow".to_string(), v);
    }
    anim.data = Value::Object(root);
    anim.duration_sec = if clean.continuous {
        0.0
    } else {
        clean.finite_duration_sec().max(0.5)
    };
}

/// Applies Chaikin corner-cutting subdivision to a 3D polyline while preserving endpoints.
/// Produces smooth, natural tactical curves without Catmull-Rom overshoot.
pub fn smooth_polyline_points(points: &[[f32; 3]], smooth: bool) -> Vec<[f32; 3]> {
    if !smooth || points.len() < 3 {
        return points.to_vec();
    }
    let mut cur = points.to_vec();
    for _ in 0..3 {
        let mut next = Vec::with_capacity(cur.len() * 2);
        next.push(cur[0]);
        for i in 0..cur.len() - 1 {
            let a = cur[i];
            let b = cur[i + 1];
            let q = [
                a[0] * 0.75 + b[0] * 0.25,
                a[1] * 0.75 + b[1] * 0.25,
                a[2] * 0.75 + b[2] * 0.25,
            ];
            let r = [
                a[0] * 0.25 + b[0] * 0.75,
                a[1] * 0.25 + b[1] * 0.75,
                a[2] * 0.25 + b[2] * 0.75,
            ];
            if i > 0 {
                next.push(q);
            }
            if i + 1 < cur.len() - 1 {
                next.push(r);
            }
        }
        next.push(*cur.last().unwrap());
        cur = next;
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn dummy_arrow_anim() -> Anim {
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Dorongan PHH".into(),
            anim_type: ANIM_TYPE_TACTICAL_ARROW.into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 0.0,
            replay: false,
            autoplay: false,
            geom: vec![[115.21, -8.67, 0.0], [115.211, -8.67, 0.0], [115.212, -8.671, 0.0]],
            data: Value::Object(Map::new()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn tactical_arrow_config_roundtrips_and_evaluates_grow_and_finish() {
        let mut anim = dummy_arrow_anim();
        let mut cfg = TacticalArrowConfig::default();
        cfg.apply_preset(TacticalArrowPreset::Evacuation);
        cfg.continuous = false;
        cfg.delay_sec = 1.0;
        cfg.grow_duration_sec = 2.0;
        cfg.hold_duration_sec = 4.0;
        cfg.label_text = "EVAKUASI VIP".into();

        write_tactical_arrow_config(&mut anim, &cfg);
        let parsed = parse_tactical_arrow_config(&anim);
        assert_eq!(parsed.preset, TacticalArrowPreset::Evacuation);
        assert_eq!(parsed.color, "#22c55e");
        assert_eq!(parsed.effective_label(), Some("EVAKUASI VIP"));
        assert!((parsed.finite_duration_sec() - 7.0).abs() < 1e-6);

        // Before delay
        let e0 = parsed.evaluate_at(0.5);
        assert!(!e0.visible);
        assert!(!e0.finished);

        // Mid-grow
        let e1 = parsed.evaluate_at(2.0);
        assert!(e1.visible);
        assert!(e1.reveal_fraction > 0.5 && e1.reveal_fraction < 1.0);

        // Full hold
        let e2 = parsed.evaluate_at(4.0);
        assert!(e2.visible);
        assert!((e2.reveal_fraction - 1.0).abs() < 1e-4);

        // Finished
        let e3 = parsed.evaluate_at(7.5);
        assert!(!e3.visible);
        assert!(e3.finished);

        // Chaikin smoothing preserves exact endpoints and increases sample density
        let raw = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0]];
        let smoothed = smooth_polyline_points(&raw, true);
        assert!(smoothed.len() > raw.len());
        assert_eq!(smoothed[0], raw[0]);
        assert_eq!(*smoothed.last().unwrap(), raw[2]);
    }
}
