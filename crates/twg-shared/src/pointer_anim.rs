use serde::{Deserialize, Serialize};

use crate::models::Anim;

pub const ANIM_TYPE_POINTER_ARROW: &str = "pointer_arrow";

/// Returns `true` if `anim` is a 3D Pointer Arrow highlight animation (`pointer_arrow`).
#[inline]
pub fn is_pointer_arrow_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_POINTER_ARROW || anim.anim_type.starts_with("pointer_")
}

fn default_height_above_m() -> f64 {
    2.2
}
fn default_arrow_scale() -> f64 {
    1.2
}
fn default_bounce_amplitude_m() -> f64 {
    1.2
}
fn default_bounce_speed_hz() -> f64 {
    1.1
}
fn default_spin_speed_deg() -> f64 {
    45.0
}
fn default_arrow_color_hex() -> String {
    "#facc15".to_string()
}
fn default_font_size() -> f64 {
    16.0
}
fn default_text_color_hex() -> String {
    "#ffffff".to_string()
}
fn default_text_offset_m() -> f64 {
    1.1
}
fn default_true() -> bool {
    true
}

/// Configuration for a 3D animated downward Pointer Arrow placed above a bound 3D Marker/Object,
/// stored inside `anim.data["pointer_arrow"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointerArrowConfig {
    /// Clearance height between the top of the bound 3D marker and the lowest arrow tip (meters).
    #[serde(default = "default_height_above_m")]
    pub height_above_m: f64,
    /// Overall 3D scale multiplier of the downward arrow mesh (`0.3..10.0`).
    #[serde(default = "default_arrow_scale")]
    pub arrow_scale: f64,
    /// Vertical up-and-down bounce travel amplitude in meters (`0.0..30.0`).
    #[serde(default = "default_bounce_amplitude_m")]
    pub bounce_amplitude_m: f64,
    /// Up-and-down bobbing frequency in cycles/second (`0.1..5.0` Hz).
    #[serde(default = "default_bounce_speed_hz")]
    pub bounce_speed_hz: f64,
    /// Optional gentle spin around vertical Y-axis in degrees/second (`0.0..360.0`; `0` = static heading).
    #[serde(default = "default_spin_speed_deg")]
    pub spin_speed_deg: f64,
    /// Arrow 3D body color (`#RRGGBB`).
    #[serde(default = "default_arrow_color_hex")]
    pub arrow_color_hex: String,
    /// Optional pulsing tactical highlight ring at the base of the target marker.
    #[serde(default)]
    pub show_ground_ring: bool,
    /// Optional floating text shown above the 3D arrow (leave empty for arrow-only with no text).
    #[serde(default)]
    pub label_text: String,
    /// Master visibility toggle for the floating text label.
    #[serde(default = "default_true")]
    pub show_label: bool,
    /// Scalable text font size in logical pixels (`8.0..56.0`).
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// Text foreground color (`#RRGGBB`) customizable via color picker.
    #[serde(default = "default_text_color_hex")]
    pub text_color_hex: String,
    /// Vertical gap between the top of the 3D arrow and the floating text (meters).
    #[serde(default = "default_text_offset_m")]
    pub text_offset_m: f64,
    /// When `true`, renders a dark translucent tactical pill behind the text for high contrast.
    #[serde(default = "default_true")]
    pub show_text_bg: bool,
    /// When `true`, stays active continuously in Play Skenario until Stopped.
    /// When `false`, plays for `anim.duration_sec` seconds (with optional `anim.replay`).
    #[serde(default = "default_true")]
    pub continuous: bool,
}

impl Default for PointerArrowConfig {
    fn default() -> Self {
        Self {
            height_above_m: default_height_above_m(),
            arrow_scale: default_arrow_scale(),
            bounce_amplitude_m: default_bounce_amplitude_m(),
            bounce_speed_hz: default_bounce_speed_hz(),
            spin_speed_deg: default_spin_speed_deg(),
            arrow_color_hex: default_arrow_color_hex(),
            show_ground_ring: false,
            label_text: String::new(),
            show_label: true,
            font_size: default_font_size(),
            text_color_hex: default_text_color_hex(),
            text_offset_m: default_text_offset_m(),
            show_text_bg: true,
            continuous: true,
        }
    }
}

impl PointerArrowConfig {
    pub fn sanitize(&mut self) {
        let clamp_f = |v: &mut f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() {
                *v = v.clamp(lo, hi);
            } else {
                *v = def;
            }
        };
        clamp_f(&mut self.height_above_m, 0.2, 150.0, default_height_above_m());
        clamp_f(&mut self.arrow_scale, 0.3, 10.0, default_arrow_scale());
        clamp_f(&mut self.bounce_amplitude_m, 0.0, 30.0, default_bounce_amplitude_m());
        clamp_f(&mut self.bounce_speed_hz, 0.1, 5.0, default_bounce_speed_hz());
        clamp_f(&mut self.spin_speed_deg, 0.0, 360.0, default_spin_speed_deg());
        if !self.arrow_color_hex.starts_with('#') || self.arrow_color_hex.len() < 7 {
            self.arrow_color_hex = default_arrow_color_hex();
        }
        clamp_f(&mut self.font_size, 8.0, 56.0, default_font_size());
        if !self.text_color_hex.starts_with('#') || self.text_color_hex.len() < 7 {
            self.text_color_hex = default_text_color_hex();
        }
        clamp_f(&mut self.text_offset_m, 0.0, 40.0, default_text_offset_m());
    }

    /// Evaluates smooth vertical bounce displacement in `[0.0, bounce_amplitude_m]` at `time_sec`.
    /// At `t = 0`, starts at the lowest dip (`0.0`) and smoothly oscillates upward and back down.
    pub fn bounce_offset_m(&self, time_sec: f64) -> f64 {
        let amp = self.bounce_amplitude_m.clamp(0.0, 30.0);
        if amp <= 1e-6 {
            return 0.0;
        }
        let phase = time_sec * self.bounce_speed_hz.clamp(0.1, 5.0) * std::f64::consts::TAU;
        0.5 * (1.0 - phase.cos()) * amp
    }

    /// Returns the trimmed floating text if label display is enabled and non-empty.
    pub fn effective_label(&self) -> Option<&str> {
        if !self.show_label {
            return None;
        }
        let t = self.label_text.trim();
        if t.is_empty() { None } else { Some(t) }
    }
}

pub fn parse_pointer_arrow_config(anim: &Anim) -> PointerArrowConfig {
    let mut cfg: PointerArrowConfig = anim
        .data
        .get("pointer_arrow")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_pointer_arrow_config(anim: &mut Anim, cfg: &PointerArrowConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("pointer_arrow".into(), val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_pointer_anim(marker_id: Option<Uuid>) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id,
            name: "Panah Penunjuk 1".into(),
            anim_type: ANIM_TYPE_POINTER_ARROW.into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 15.0,
            replay: false,
            autoplay: false,
            geom: Vec::new(),
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn pointer_arrow_config_roundtrips_and_handles_optional_label() {
        let mid = Uuid::new_v4();
        let mut anim = sample_pointer_anim(Some(mid));
        assert!(is_pointer_arrow_anim(&anim));

        // Default has empty label -> effective_label() is None (arrow-only)
        let def = parse_pointer_arrow_config(&anim);
        assert_eq!(def.effective_label(), None);

        // Custom text, scaled font & custom color roundtrip cleanly
        let cfg = PointerArrowConfig {
            label_text: "  POSKO VIP  ".into(),
            font_size: 28.0,
            text_color_hex: "#38bdf8".into(),
            arrow_color_hex: "#ef4444".into(),
            bounce_amplitude_m: 2.0,
            bounce_speed_hz: 1.0,
            ..Default::default()
        };
        write_pointer_arrow_config(&mut anim, &cfg);
        let parsed = parse_pointer_arrow_config(&anim);
        assert_eq!(parsed.effective_label(), Some("POSKO VIP"));
        assert!((parsed.font_size - 28.0).abs() < 1e-6);
        assert_eq!(parsed.text_color_hex, "#38bdf8");
        assert_eq!(parsed.arrow_color_hex, "#ef4444");

        // Bounce offset stays smoothly bounded in [0, amplitude]
        assert!(parsed.bounce_offset_m(0.0).abs() < 1e-6);
        assert!((parsed.bounce_offset_m(0.5) - 2.0).abs() < 1e-6);
        assert!(parsed.bounce_offset_m(1.0).abs() < 1e-6);
    }
}
