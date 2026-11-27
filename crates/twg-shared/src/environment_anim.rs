use serde::{Deserialize, Serialize};

use crate::models::Anim;

pub const ANIM_TYPE_WATER_SIM: &str = "water_sim";
pub const ANIM_TYPE_RAIN_SIM: &str = "rain_sim";

pub fn is_water_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_WATER_SIM
}

pub fn is_rain_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_RAIN_SIM
}

pub fn is_environment_anim(anim: &Anim) -> bool {
    is_water_anim(anim) || is_rain_anim(anim)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterSimKind {
    #[default]
    Ocean,
    River,
    Flood,
}

impl WaterSimKind {
    pub const ALL: &'static [Self] = &[Self::Ocean, Self::River, Self::Flood];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ocean => "Laut (Ombak)",
            Self::River => "Sungai (Arus Mengalir)",
            Self::Flood => "Banjir (Genangan Naik)",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Ocean => "LAUT",
            Self::River => "SUNGAI",
            Self::Flood => "BANJIR",
        }
    }

    pub fn default_name(self, idx: usize) -> String {
        match self {
            Self::Ocean => format!("Laut {idx}"),
            Self::River => format!("Sungai {idx}"),
            Self::Flood => format!("Banjir {idx}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaterAnimConfig {
    #[serde(default)]
    pub kind: WaterSimKind,
    /// Starting water surface elevation (meters above ground) at t=0.
    #[serde(default = "default_water_start_level")]
    pub start_level_m: f64,
    /// Target / peak water surface elevation (meters above ground) during playback.
    /// For Ocean/River this equals `start_level_m`; for Flood water rises from start -> target.
    #[serde(default = "default_water_target_level")]
    pub target_level_m: f64,
    /// Seconds to rise from `start_level_m` to `target_level_m` (used for Flood).
    #[serde(default = "default_rise_duration")]
    pub rise_duration_sec: f64,
    /// If true on Flood mode, water stays at peak briefly then recedes back to `start_level_m`.
    #[serde(default)]
    pub recede_after_peak: bool,
    /// Hold seconds at peak before receding when `recede_after_peak` is true.
    #[serde(default = "default_peak_hold")]
    pub peak_hold_sec: f64,
    /// Wave crest amplitude in meters (`0.0..=4.0`).
    #[serde(default = "default_wave_height")]
    pub wave_height_m: f64,
    /// Wave spatial density multiplier (`0.2..=4.0`).
    #[serde(default = "default_wave_freq")]
    pub wave_frequency: f64,
    /// Current / wave travel speed in m/s (`0.0..=15.0`).
    #[serde(default = "default_flow_speed")]
    pub flow_speed_mps: f64,
    /// Compass direction (0° = North, 90° = East, 180° = South, 270° = West) of current / swell.
    #[serde(default = "default_flow_dir")]
    pub flow_dir_deg: f64,
    /// Surface tint hex (`#0e6ba8` Ocean, `#1d84b5` River, `#6b5b3e` Flood).
    #[serde(default = "default_ocean_color")]
    pub color_hex: String,
    /// Surface alpha opacity (`0.25..=0.98`).
    #[serde(default = "default_water_opacity")]
    pub opacity: f64,
    /// Crest foam & specular highlight intensity (`0.0..=1.5`).
    #[serde(default = "default_foam")]
    pub foam_intensity: f64,
    /// Keep animating continuously until stopped.
    #[serde(default = "default_true")]
    pub continuous: bool,
    /// Finite duration when `continuous == false`.
    #[serde(default = "default_water_duration")]
    pub duration_sec: f64,
    /// Optional start delay in seconds.
    #[serde(default)]
    pub delay_sec: f64,
    /// Show animated water preview directly in Plot Editor even when not in Play mode.
    #[serde(default = "default_true")]
    pub always_preview_in_editor: bool,
}

fn default_true() -> bool {
    true
}
fn default_water_start_level() -> f64 {
    0.25
}
fn default_water_target_level() -> f64 {
    0.25
}
fn default_rise_duration() -> f64 {
    15.0
}
fn default_peak_hold() -> f64 {
    8.0
}
fn default_wave_height() -> f64 {
    0.45
}
fn default_wave_freq() -> f64 {
    1.0
}
fn default_flow_speed() -> f64 {
    2.2
}
fn default_flow_dir() -> f64 {
    90.0
}
fn default_ocean_color() -> String {
    "#0c5d8a".to_string()
}
fn default_water_opacity() -> f64 {
    0.84
}
fn default_foam() -> f64 {
    0.7
}
fn default_water_duration() -> f64 {
    30.0
}

impl Default for WaterAnimConfig {
    fn default() -> Self {
        Self::default_for(WaterSimKind::Ocean)
    }
}

impl WaterAnimConfig {
    pub fn default_for(kind: WaterSimKind) -> Self {
        match kind {
            WaterSimKind::Ocean => Self {
                kind,
                start_level_m: 0.25,
                target_level_m: 0.25,
                rise_duration_sec: 10.0,
                recede_after_peak: false,
                peak_hold_sec: 8.0,
                wave_height_m: 0.55,
                wave_frequency: 1.0,
                flow_speed_mps: 2.5,
                flow_dir_deg: 65.0,
                color_hex: "#0c5d8a".to_string(),
                opacity: 0.86,
                foam_intensity: 0.85,
                continuous: true,
                duration_sec: 30.0,
                delay_sec: 0.0,
                always_preview_in_editor: true,
            },
            WaterSimKind::River => Self {
                kind,
                start_level_m: 0.20,
                target_level_m: 0.20,
                rise_duration_sec: 10.0,
                recede_after_peak: false,
                peak_hold_sec: 8.0,
                wave_height_m: 0.16,
                wave_frequency: 1.6,
                flow_speed_mps: 4.5,
                flow_dir_deg: 90.0,
                color_hex: "#197278".to_string(),
                opacity: 0.82,
                foam_intensity: 0.65,
                continuous: true,
                duration_sec: 30.0,
                delay_sec: 0.0,
                always_preview_in_editor: true,
            },
            WaterSimKind::Flood => Self {
                kind,
                start_level_m: 0.02,
                target_level_m: 2.2,
                rise_duration_sec: 15.0,
                recede_after_peak: false,
                peak_hold_sec: 8.0,
                wave_height_m: 0.12,
                wave_frequency: 1.3,
                flow_speed_mps: 1.4,
                flow_dir_deg: 45.0,
                color_hex: "#6e5c3f".to_string(),
                opacity: 0.80,
                foam_intensity: 0.35,
                continuous: true,
                duration_sec: 35.0,
                delay_sec: 0.0,
                always_preview_in_editor: true,
            },
        }
    }

    pub fn sanitized(mut self) -> Self {
        let clamp_f = |v: f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() { v.clamp(lo, hi) } else { def }
        };
        self.start_level_m = clamp_f(self.start_level_m, -5.0, 50.0, 0.2);
        self.target_level_m = clamp_f(self.target_level_m, -5.0, 50.0, self.start_level_m);
        self.rise_duration_sec = clamp_f(self.rise_duration_sec, 0.5, 600.0, 15.0);
        self.peak_hold_sec = clamp_f(self.peak_hold_sec, 0.0, 600.0, 8.0);
        self.wave_height_m = clamp_f(self.wave_height_m, 0.0, 5.0, 0.35);
        self.wave_frequency = clamp_f(self.wave_frequency, 0.15, 5.0, 1.0);
        self.flow_speed_mps = clamp_f(self.flow_speed_mps, 0.0, 20.0, 2.0);
        self.flow_dir_deg = if self.flow_dir_deg.is_finite() {
            self.flow_dir_deg.rem_euclid(360.0)
        } else {
            90.0
        };
        self.opacity = clamp_f(self.opacity, 0.20, 0.98, 0.84);
        self.foam_intensity = clamp_f(self.foam_intensity, 0.0, 1.5, 0.6);
        self.duration_sec = clamp_f(self.duration_sec, 1.0, 3600.0, 30.0);
        self.delay_sec = clamp_f(self.delay_sec, 0.0, 3600.0, 0.0);
        if self.color_hex.trim().is_empty() {
            self.color_hex = default_ocean_color();
        }
        self
    }

    /// Computes instantaneous water elevation (meters) at elapsed playback time `elapsed_sec`
    /// (excluding `delay_sec`).
    pub fn water_level_at(&self, elapsed_sec: f64) -> f64 {
        let t = elapsed_sec.max(0.0);
        if self.kind != WaterSimKind::Flood {
            return self.start_level_m;
        }
        let rise_d = self.rise_duration_sec.max(0.1);
        if t < rise_d {
            let u = (t / rise_d).clamp(0.0, 1.0);
            let smooth = u * u * (3.0 - 2.0 * u);
            self.start_level_m + (self.target_level_m - self.start_level_m) * smooth
        } else if !self.recede_after_peak {
            self.target_level_m
        } else {
            let after_rise = t - rise_d;
            if after_rise < self.peak_hold_sec {
                self.target_level_m
            } else {
                let rec = ((after_rise - self.peak_hold_sec) / rise_d).clamp(0.0, 1.0);
                let smooth = rec * rec * (3.0 - 2.0 * rec);
                self.target_level_m + (self.start_level_m - self.target_level_m) * smooth
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RainPreset {
    Drizzle,
    #[default]
    Moderate,
    Heavy,
    Storm,
}

impl RainPreset {
    pub const ALL: &'static [Self] = &[Self::Drizzle, Self::Moderate, Self::Heavy, Self::Storm];

    pub fn label(self) -> &'static str {
        match self {
            Self::Drizzle => "Gerimis",
            Self::Moderate => "Hujan Sedang",
            Self::Heavy => "Hujan Lebat",
            Self::Storm => "Badai & Petir",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Drizzle => "GERIMIS",
            Self::Moderate => "HUJAN",
            Self::Heavy => "HUJAN LEBAT",
            Self::Storm => "BADAI",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RainCoverage {
    /// Rain follows the active camera across the entire map.
    #[default]
    Global,
    /// Rain falls strictly inside a circular local zone around `[lng, lat]` with `radius_m`.
    LocalArea,
}

impl RainCoverage {
    pub const ALL: &'static [Self] = &[Self::Global, Self::LocalArea];

    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "Seluruh Area (Ikut Kamera)",
            Self::LocalArea => "Area Lokal di Peta",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RainAnimConfig {
    #[serde(default)]
    pub preset: RainPreset,
    #[serde(default)]
    pub coverage: RainCoverage,
    /// Center coordinate for `RainCoverage::LocalArea`.
    #[serde(default)]
    pub lng: f64,
    #[serde(default)]
    pub lat: f64,
    /// Local rain cloud radius in meters (`20..=3000`).
    #[serde(default = "default_rain_radius")]
    pub radius_m: f64,
    /// Density / heaviness multiplier (`0.15..=2.0`).
    #[serde(default = "default_rain_intensity")]
    pub intensity: f64,
    /// Downward raindrop velocity in m/s (`12..=55`).
    #[serde(default = "default_rain_fall_speed")]
    pub fall_speed_mps: f64,
    /// Streak length of each falling raindrop in meters (`0.5..=6.0`).
    #[serde(default = "default_rain_drop_len")]
    pub drop_length_m: f64,
    /// Wind compass direction (0° = North, 90° = East).
    #[serde(default = "default_rain_wind_dir")]
    pub wind_dir_deg: f64,
    /// Lateral wind speed tilting rain streaks (m/s, `0..=25`).
    #[serde(default = "default_rain_wind_speed")]
    pub wind_speed_mps: f64,
    /// Show water splash rings on the ground surface.
    #[serde(default = "default_true")]
    pub splash_enabled: bool,
    /// Enable periodic deterministic 3D lightning bolts & sky flashes.
    #[serde(default)]
    pub lightning_enabled: bool,
    /// Average seconds between lightning bolts (`2.0..=25.0`, lower = more frequent).
    #[serde(default = "default_lightning_interval")]
    pub lightning_interval_sec: f64,
    /// Overcast sky / sun dimming factor (`0.0..=1.0`).
    #[serde(default = "default_overcast")]
    pub overcast_strength: f64,
    /// Rain streak color hex.
    #[serde(default = "default_rain_color")]
    pub color_hex: String,
    /// Smooth ramp-up duration when rain starts (`0.0..=15.0`).
    #[serde(default = "default_rain_ramp")]
    pub ramp_up_sec: f64,
    /// Keep raining until stopped manually.
    #[serde(default = "default_true")]
    pub continuous: bool,
    /// Finite duration when `continuous == false`.
    #[serde(default = "default_rain_duration")]
    pub duration_sec: f64,
    /// Optional start delay in seconds.
    #[serde(default)]
    pub delay_sec: f64,
    /// Preview rain live in Plot Editor when selected or always.
    #[serde(default)]
    pub always_preview_in_editor: bool,
}

fn default_rain_radius() -> f64 {
    160.0
}
fn default_rain_intensity() -> f64 {
    1.0
}
fn default_rain_fall_speed() -> f64 {
    28.0
}
fn default_rain_drop_len() -> f64 {
    2.4
}
fn default_rain_wind_dir() -> f64 {
    60.0
}
fn default_rain_wind_speed() -> f64 {
    4.0
}
fn default_lightning_interval() -> f64 {
    7.0
}
fn default_overcast() -> f64 {
    0.35
}
fn default_rain_color() -> String {
    "#cfe8ff".to_string()
}
fn default_rain_ramp() -> f64 {
    1.5
}
fn default_rain_duration() -> f64 {
    45.0
}

impl Default for RainAnimConfig {
    fn default() -> Self {
        Self::default_for(RainPreset::Moderate)
    }
}

impl RainAnimConfig {
    pub fn default_for(preset: RainPreset) -> Self {
        let mut cfg = Self {
            preset,
            coverage: RainCoverage::Global,
            lng: 0.0,
            lat: 0.0,
            radius_m: default_rain_radius(),
            intensity: default_rain_intensity(),
            fall_speed_mps: default_rain_fall_speed(),
            drop_length_m: default_rain_drop_len(),
            wind_dir_deg: default_rain_wind_dir(),
            wind_speed_mps: default_rain_wind_speed(),
            splash_enabled: true,
            lightning_enabled: false,
            lightning_interval_sec: default_lightning_interval(),
            overcast_strength: default_overcast(),
            color_hex: default_rain_color(),
            ramp_up_sec: default_rain_ramp(),
            continuous: true,
            duration_sec: default_rain_duration(),
            delay_sec: 0.0,
            always_preview_in_editor: false,
        };
        cfg.apply_preset(preset);
        cfg
    }

    pub fn apply_preset(&mut self, preset: RainPreset) {
        self.preset = preset;
        match preset {
            RainPreset::Drizzle => {
                self.intensity = 0.45;
                self.fall_speed_mps = 19.0;
                self.drop_length_m = 1.4;
                self.wind_speed_mps = 1.8;
                self.splash_enabled = true;
                self.lightning_enabled = false;
                self.overcast_strength = 0.20;
            }
            RainPreset::Moderate => {
                self.intensity = 1.0;
                self.fall_speed_mps = 28.0;
                self.drop_length_m = 2.4;
                self.wind_speed_mps = 4.2;
                self.splash_enabled = true;
                self.lightning_enabled = false;
                self.overcast_strength = 0.40;
            }
            RainPreset::Heavy => {
                self.intensity = 1.55;
                self.fall_speed_mps = 38.0;
                self.drop_length_m = 3.6;
                self.wind_speed_mps = 9.0;
                self.splash_enabled = true;
                self.lightning_enabled = false;
                self.overcast_strength = 0.62;
            }
            RainPreset::Storm => {
                self.intensity = 1.95;
                self.fall_speed_mps = 46.0;
                self.drop_length_m = 4.5;
                self.wind_speed_mps = 15.0;
                self.splash_enabled = true;
                self.lightning_enabled = true;
                self.lightning_interval_sec = 5.5;
                self.overcast_strength = 0.78;
            }
        }
    }

    pub fn sanitized(mut self) -> Self {
        let clamp_f = |v: f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() { v.clamp(lo, hi) } else { def }
        };
        self.lng = clamp_f(self.lng, -180.0, 180.0, 0.0);
        self.lat = clamp_f(self.lat, -85.0, 85.0, 0.0);
        self.radius_m = clamp_f(self.radius_m, 15.0, 4000.0, 160.0);
        self.intensity = clamp_f(self.intensity, 0.15, 2.0, 1.0);
        self.fall_speed_mps = clamp_f(self.fall_speed_mps, 10.0, 60.0, 28.0);
        self.drop_length_m = clamp_f(self.drop_length_m, 0.4, 7.0, 2.4);
        self.wind_dir_deg = if self.wind_dir_deg.is_finite() {
            self.wind_dir_deg.rem_euclid(360.0)
        } else {
            60.0
        };
        self.wind_speed_mps = clamp_f(self.wind_speed_mps, 0.0, 28.0, 4.0);
        self.lightning_interval_sec = clamp_f(self.lightning_interval_sec, 1.5, 30.0, 7.0);
        self.overcast_strength = clamp_f(self.overcast_strength, 0.0, 1.0, 0.35);
        self.ramp_up_sec = clamp_f(self.ramp_up_sec, 0.0, 20.0, 1.5);
        self.duration_sec = clamp_f(self.duration_sec, 1.0, 3600.0, 45.0);
        self.delay_sec = clamp_f(self.delay_sec, 0.0, 3600.0, 0.0);
        if self.color_hex.trim().is_empty() {
            self.color_hex = default_rain_color();
        }
        self
    }

    pub fn has_playable_setup(&self, anim: &Anim) -> bool {
        match self.coverage {
            RainCoverage::Global => true,
            RainCoverage::LocalArea => {
                !anim.geom.is_empty() || self.lng.abs() > 1e-6 || self.lat.abs() > 1e-6
            }
        }
    }
}

pub fn parse_water_anim_config(anim: &Anim) -> WaterAnimConfig {
    anim.data
        .get("water_sim")
        .and_then(|v| serde_json::from_value::<WaterAnimConfig>(v.clone()).ok())
        .unwrap_or_default()
        .sanitized()
}

pub fn write_water_anim_config(anim: &mut Anim, cfg: &WaterAnimConfig) {
    let clean = cfg.clone().sanitized();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "water_sim".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    anim.duration_sec = if clean.continuous {
        0.0
    } else {
        clean.delay_sec + clean.duration_sec
    };
}

pub fn parse_rain_anim_config(anim: &Anim) -> RainAnimConfig {
    let mut cfg: RainAnimConfig = anim
        .data
        .get("rain_sim")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.lng.abs() < 1e-9
        && cfg.lat.abs() < 1e-9
        && let Some(p) = anim.geom.first()
    {
        cfg.lng = p[0];
        cfg.lat = p[1];
    }
    cfg.sanitized()
}

pub fn write_rain_anim_config(anim: &mut Anim, cfg: &RainAnimConfig) {
    let clean = cfg.clone().sanitized();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "rain_sim".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    if clean.coverage == RainCoverage::LocalArea {
        anim.geom = vec![[clean.lng, clean.lat, 0.0]];
    }
    anim.duration_sec = if clean.continuous {
        0.0
    } else {
        clean.delay_sec + clean.duration_sec
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn dummy_anim(anim_type: &str) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Env Test".into(),
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
    fn water_flood_rises_and_recedes_smoothly() {
        let mut anim = dummy_anim(ANIM_TYPE_WATER_SIM);
        let mut cfg = WaterAnimConfig::default_for(WaterSimKind::Flood);
        cfg.start_level_m = 0.0;
        cfg.target_level_m = 3.0;
        cfg.rise_duration_sec = 10.0;
        cfg.recede_after_peak = true;
        cfg.peak_hold_sec = 5.0;
        write_water_anim_config(&mut anim, &cfg);

        let parsed = parse_water_anim_config(&anim);
        assert!((parsed.water_level_at(0.0) - 0.0).abs() < 1e-6);
        assert!((parsed.water_level_at(5.0) - 1.5).abs() < 1e-6);
        assert!((parsed.water_level_at(10.0) - 3.0).abs() < 1e-6);
        assert!((parsed.water_level_at(15.0) - 3.0).abs() < 1e-6);
        assert!((parsed.water_level_at(20.0) - 1.5).abs() < 1e-6);
        assert!((parsed.water_level_at(25.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn rain_presets_and_global_vs_local_playability() {
        let mut anim = dummy_anim(ANIM_TYPE_RAIN_SIM);
        let mut cfg = RainAnimConfig::default_for(RainPreset::Storm);
        assert!(cfg.lightning_enabled);
        assert!(cfg.has_playable_setup(&anim));

        cfg.coverage = RainCoverage::LocalArea;
        assert!(!cfg.has_playable_setup(&anim));
        cfg.lng = 106.82;
        cfg.lat = -6.17;
        write_rain_anim_config(&mut anim, &cfg);
        let parsed = parse_rain_anim_config(&anim);
        assert!(parsed.has_playable_setup(&anim));
        assert_eq!(anim.geom.len(), 1);
    }
}
