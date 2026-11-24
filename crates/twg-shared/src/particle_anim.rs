use serde::{Deserialize, Serialize};

use crate::models::Anim;

pub const ANIM_TYPE_PARTICLE_FIRE: &str = "particle_fire";
pub const ANIM_TYPE_PARTICLE_SMOKE: &str = "particle_smoke";
pub const ANIM_TYPE_PARTICLE_EXPLOSION: &str = "particle_explosion";

/// Supported tactical 3D particle animation sub-types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleAnimKind {
    #[default]
    Fire,
    Smoke,
    Explosion,
}

impl ParticleAnimKind {
    pub const ALL: &'static [Self] = &[Self::Fire, Self::Smoke, Self::Explosion];

    pub fn anim_type_str(self) -> &'static str {
        match self {
            Self::Fire => ANIM_TYPE_PARTICLE_FIRE,
            Self::Smoke => ANIM_TYPE_PARTICLE_SMOKE,
            Self::Explosion => ANIM_TYPE_PARTICLE_EXPLOSION,
        }
    }

    pub fn from_anim_type(anim_type: &str) -> Option<Self> {
        match anim_type {
            ANIM_TYPE_PARTICLE_FIRE | "fire" => Some(Self::Fire),
            ANIM_TYPE_PARTICLE_SMOKE | "smoke" => Some(Self::Smoke),
            ANIM_TYPE_PARTICLE_EXPLOSION | "explosion" => Some(Self::Explosion),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Fire => "Api (Fire)",
            Self::Smoke => "Asap (Smoke)",
            Self::Explosion => "Ledakan (Explosion)",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Fire => "API",
            Self::Smoke => "ASAP",
            Self::Explosion => "LEDAKAN",
        }
    }

    pub fn default_name(self, index: usize) -> String {
        match self {
            Self::Fire => format!("Api {index}"),
            Self::Smoke => format!("Asap {index}"),
            Self::Explosion => format!("Ledakan {index}"),
        }
    }
}

/// Returns `true` if `anim` is a Particle animation (`Fire`, `Smoke`, or `Explosion`).
#[inline]
pub fn is_particle_anim(anim: &Anim) -> bool {
    ParticleAnimKind::from_anim_type(&anim.anim_type).is_some()
        || anim.anim_type == "particle"
        || anim.anim_type.starts_with("particle_")
}

fn default_radius_m() -> f64 {
    3.0
}
fn default_height_m() -> f64 {
    8.0
}
fn default_intensity() -> f64 {
    1.0
}
fn default_rise_speed() -> f64 {
    1.0
}
fn default_spread() -> f64 {
    0.55
}
fn default_color_hex() -> String {
    "#ff6a00".to_string()
}
fn default_wind_dir_deg() -> f64 {
    45.0
}
fn default_wind_speed_mps() -> f64 {
    1.5
}
fn default_true() -> bool {
    true
}

/// Full configuration for a 3D Particle Animation (`Fire`, `Smoke`, or `Explosion`), stored inside
/// `anim.data["particle"]`. Position `[lng, lat, alt]` is stored in `anim.geom[0]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleAnimConfig {
    #[serde(default)]
    pub kind: ParticleAnimKind,
    /// Base emitter or blast radius in meters.
    #[serde(default = "default_radius_m")]
    pub radius_m: f64,
    /// Flame height, smoke plume height, or explosion mushroom height in meters.
    #[serde(default = "default_height_m")]
    pub height_m: f64,
    /// Strength / density / brightness multiplier (`0.2..3.0`).
    #[serde(default = "default_intensity")]
    pub intensity: f64,
    /// Upward rise / animation speed multiplier (`0.2..5.0`).
    #[serde(default = "default_rise_speed")]
    pub rise_speed: f64,
    /// Upper plume expansion factor (`0.0..2.0`).
    #[serde(default = "default_spread")]
    pub spread: f64,
    /// Primary tint color (`#RRGGBB`).
    #[serde(default = "default_color_hex")]
    pub color_hex: String,
    /// Compass bearing (degrees, `0 = North, 90 = East`) toward which wind pushes flames/smoke.
    #[serde(default = "default_wind_dir_deg")]
    pub wind_dir_deg: f64,
    /// Wind speed in m/s (`0..25`).
    #[serde(default = "default_wind_speed_mps")]
    pub wind_speed_mps: f64,
    /// Fire: rising dark smoke above flames; Explosion: rising post-blast mushroom smoke.
    #[serde(default = "default_true")]
    pub with_smoke: bool,
    /// Fire: glowing rising embers; Explosion: ballistic flying fire/debris chunks.
    #[serde(default = "default_true")]
    pub with_embers: bool,
    /// Explosion: expanding ground shockwave ring on detonation.
    #[serde(default = "default_true")]
    pub shockwave_enabled: bool,
    /// When `true`, plays continuously in Play Skenario until Paused/Stopped.
    /// When `false`, plays for `anim.duration_sec` seconds (with optional `anim.replay`).
    #[serde(default = "default_true")]
    pub continuous: bool,
    /// When `true`, stays animated in Editor mode even when another entity is selected.
    #[serde(default)]
    pub always_preview_in_editor: bool,
}

impl Default for ParticleAnimConfig {
    fn default() -> Self {
        Self::default_for(ParticleAnimKind::Fire)
    }
}

impl ParticleAnimConfig {
    pub fn default_for(kind: ParticleAnimKind) -> Self {
        match kind {
            ParticleAnimKind::Fire => Self {
                kind,
                radius_m: 2.8,
                height_m: 6.5,
                intensity: 1.15,
                rise_speed: 1.1,
                spread: 0.35,
                color_hex: "#ff6a00".to_string(),
                wind_dir_deg: 45.0,
                wind_speed_mps: 1.5,
                with_smoke: true,
                with_embers: true,
                shockwave_enabled: false,
                continuous: true,
                always_preview_in_editor: false,
            },
            ParticleAnimKind::Smoke => Self {
                kind,
                radius_m: 3.2,
                height_m: 18.0,
                intensity: 1.0,
                rise_speed: 1.0,
                spread: 0.85,
                color_hex: "#334155".to_string(),
                wind_dir_deg: 55.0,
                wind_speed_mps: 3.0,
                with_smoke: true,
                with_embers: false,
                shockwave_enabled: false,
                continuous: true,
                always_preview_in_editor: false,
            },
            ParticleAnimKind::Explosion => Self {
                kind,
                radius_m: 10.0,
                height_m: 18.0,
                intensity: 1.25,
                rise_speed: 1.0,
                spread: 1.0,
                color_hex: "#ff7b00".to_string(),
                wind_dir_deg: 45.0,
                wind_speed_mps: 1.0,
                with_smoke: true,
                with_embers: true,
                shockwave_enabled: true,
                continuous: false,
                always_preview_in_editor: false,
            },
        }
    }

    pub fn default_duration_sec(kind: ParticleAnimKind) -> f64 {
        match kind {
            ParticleAnimKind::Fire | ParticleAnimKind::Smoke => 20.0,
            ParticleAnimKind::Explosion => 4.0,
        }
    }

    pub fn sanitize(&mut self) {
        let clamp_f = |v: &mut f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() {
                *v = v.clamp(lo, hi);
            } else {
                *v = def;
            }
        };
        let def = Self::default_for(self.kind);
        clamp_f(&mut self.radius_m, 0.3, 150.0, def.radius_m);
        clamp_f(&mut self.height_m, 0.5, 250.0, def.height_m);
        clamp_f(&mut self.intensity, 0.2, 3.0, def.intensity);
        clamp_f(&mut self.rise_speed, 0.2, 5.0, def.rise_speed);
        clamp_f(&mut self.spread, 0.0, 2.0, def.spread);
        if self.wind_dir_deg.is_finite() {
            self.wind_dir_deg = self.wind_dir_deg.rem_euclid(360.0);
        } else {
            self.wind_dir_deg = def.wind_dir_deg;
        }
        clamp_f(&mut self.wind_speed_mps, 0.0, 25.0, def.wind_speed_mps);
        if !self.color_hex.starts_with('#') || self.color_hex.len() < 7 {
            self.color_hex = def.color_hex;
        }
    }

    /// Returns horizontal wind unit vector `(wx, wz)` in Bevy world XZ coordinates
    /// (`+X = East`, `-Z = North`, `0° = North`, `90° = East`).
    pub fn wind_dir_xz(&self) -> (f64, f64) {
        let rad = self.wind_dir_deg.to_radians();
        (rad.sin(), -rad.cos())
    }
}

pub fn parse_particle_anim_config(anim: &Anim) -> ParticleAnimConfig {
    let inferred_kind = ParticleAnimKind::from_anim_type(&anim.anim_type).unwrap_or_default();
    let mut cfg: ParticleAnimConfig = anim
        .data
        .get("particle")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| ParticleAnimConfig::default_for(inferred_kind));
    if let Some(k) = ParticleAnimKind::from_anim_type(&anim.anim_type) {
        cfg.kind = k;
    }
    cfg.sanitize();
    cfg
}

pub fn write_particle_anim_config(anim: &mut Anim, cfg: &ParticleAnimConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    anim.anim_type = clean.kind.anim_type_str().to_string();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("particle".into(), val);
    }
}

/// Reads `[lng, lat, altitude_m]` of a Particle animation from `anim.geom[0]`.
pub fn particle_anim_position(anim: &Anim) -> [f64; 3] {
    anim.geom.first().copied().unwrap_or([0.0, 0.0, 0.0])
}

/// Writes `[lng, lat, altitude_m]` of a Particle animation into `anim.geom`.
pub fn set_particle_anim_position(anim: &mut Anim, lng: f64, lat: f64, alt: f64) {
    anim.geom = vec![[lng, lat, alt]];
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_particle_anim(kind: ParticleAnimKind, pos: [f64; 3]) -> Anim {
        let now = Utc::now();
        let cfg = ParticleAnimConfig::default_for(kind);
        let mut a = Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: kind.default_name(1),
            anim_type: kind.anim_type_str().into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: ParticleAnimConfig::default_duration_sec(kind),
            replay: false,
            autoplay: false,
            geom: vec![pos],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };
        write_particle_anim_config(&mut a, &cfg);
        a
    }

    #[test]
    fn particle_configs_roundtrip_and_position_sync() {
        for &kind in ParticleAnimKind::ALL {
            let mut anim = sample_particle_anim(kind, [115.225, -8.649, 12.5]);
            assert!(is_particle_anim(&anim));
            assert!(crate::route_formation::anim_is_playable(&anim, &[]));

            let parsed = parse_particle_anim_config(&anim);
            assert_eq!(parsed.kind, kind);
            assert_eq!(parsed.continuous, kind != ParticleAnimKind::Explosion);

            let pos = particle_anim_position(&anim);
            assert!((pos[0] - 115.225).abs() < 1e-9);
            assert!((pos[1] - (-8.649)).abs() < 1e-9);
            assert!((pos[2] - 12.5).abs() < 1e-9);

            set_particle_anim_position(&mut anim, 115.230, -8.650, 24.0);
            assert_eq!(particle_anim_position(&anim), [115.230, -8.650, 24.0]);
        }
    }

    #[test]
    fn wind_compass_bearing_maps_to_bevy_xz() {
        let mut cfg = ParticleAnimConfig::default_for(ParticleAnimKind::Smoke);
        cfg.wind_dir_deg = 0.0; // North -> -Z
        let (wx, wz) = cfg.wind_dir_xz();
        assert!(wx.abs() < 1e-6 && (wz - (-1.0)).abs() < 1e-6);

        cfg.wind_dir_deg = 90.0; // East -> +X
        let (wx, wz) = cfg.wind_dir_xz();
        assert!((wx - 1.0).abs() < 1e-6 && wz.abs() < 1e-6);
    }
}
