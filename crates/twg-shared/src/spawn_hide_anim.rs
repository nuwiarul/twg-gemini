use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Anim;

pub const ANIM_TYPE_SPAWN: &str = "spawn_hide_spawn";
pub const ANIM_TYPE_HIDE: &str = "spawn_hide_hide";
pub const ANIM_TYPE_SPAWN_HIDE_LEGACY: &str = "spawn_hide";

/// Duration (seconds) of the brief ground highlight ring pulse for `Instant` transitions.
pub const INSTANT_FX_RING_DURATION_SEC: f64 = 0.45;

/// Action performed when a `Spawn & Hide` animation is played in Play Skenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnHideAction {
    /// Target markers start hidden on entering Play Skenario and appear when played.
    #[default]
    Spawn,
    /// Target markers start visible on entering Play Skenario and disappear when played.
    Hide,
}

impl SpawnHideAction {
    pub const ALL: &'static [Self] = &[Self::Spawn, Self::Hide];

    pub fn anim_type_str(self) -> &'static str {
        match self {
            Self::Spawn => ANIM_TYPE_SPAWN,
            Self::Hide => ANIM_TYPE_HIDE,
        }
    }

    pub fn from_anim_type(anim_type: &str) -> Option<Self> {
        match anim_type {
            ANIM_TYPE_SPAWN | "spawn" => Some(Self::Spawn),
            ANIM_TYPE_HIDE | "hide" => Some(Self::Hide),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Spawn => "Spawn (Munculkan)",
            Self::Hide => "Hide (Sembunyikan)",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Spawn => "SPAWN",
            Self::Hide => "HIDE",
        }
    }

    pub fn default_name(self, index: usize) -> String {
        match self {
            Self::Spawn => format!("Spawn Objek {index}"),
            Self::Hide => format!("Hide Objek {index}"),
        }
    }
}

/// Visual transition effect applied to each target 3D marker during Spawn / Hide playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnHideTransition {
    /// Appears or disappears instantaneously with zero scale/vertical transition.
    Instant,
    /// Smoothly scales from `0% -> 100%` (Spawn) or `100% -> 0%` (Hide).
    #[default]
    PopScale,
    /// Drops smoothly from above (Spawn) or sinks downward (Hide) combined with scaling.
    DropVertical,
}

impl SpawnHideTransition {
    pub const ALL: &'static [Self] = &[Self::PopScale, Self::Instant, Self::DropVertical];

    pub fn label(self) -> &'static str {
        match self {
            Self::Instant => "Instan (Langsung)",
            Self::PopScale => "Pop Scale (Halus)",
            Self::DropVertical => "Turun / Naik Vertikal",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Instant => "Instan",
            Self::PopScale => "Pop Scale",
            Self::DropVertical => "Vertikal",
        }
    }
}

/// Returns `true` if `anim` belongs to the `Spawn & Hide` animation category.
#[inline]
pub fn is_spawn_hide_anim(anim: &Anim) -> bool {
    SpawnHideAction::from_anim_type(&anim.anim_type).is_some()
        || anim.anim_type == ANIM_TYPE_SPAWN_HIDE_LEGACY
        || anim.anim_type.starts_with("spawn_hide_")
}

/// Resolves the action (`Spawn` or `Hide`) for a `Spawn & Hide` animation.
pub fn spawn_hide_action(anim: &Anim) -> Option<SpawnHideAction> {
    if !is_spawn_hide_anim(anim) {
        return None;
    }
    if let Some(a) = SpawnHideAction::from_anim_type(&anim.anim_type) {
        return Some(a);
    }
    Some(parse_spawn_hide_config(anim).action)
}

fn default_transition_sec() -> f64 {
    0.45
}

fn default_drop_height_m() -> f64 {
    6.0
}

fn default_true() -> bool {
    true
}

/// Configuration for a multi-target `Spawn` or `Hide` animation stored inside
/// `anim.data["spawn_hide"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnHideConfig {
    /// Action mode (`Spawn` = appear from hidden, `Hide` = disappear from visible).
    #[serde(default)]
    pub action: SpawnHideAction,
    /// Ordered list of bound 3D marker / object UUIDs targeted by this animation.
    #[serde(default)]
    pub marker_ids: Vec<Uuid>,
    /// Visual transition style (`PopScale`, `Instant`, `DropVertical`).
    #[serde(default)]
    pub transition: SpawnHideTransition,
    /// Per-object transition duration in seconds (`0.05..=3.0`; ignored for `Instant`).
    #[serde(default = "default_transition_sec")]
    pub transition_sec: f64,
    /// Delay between consecutive objects in `marker_ids` (`0.0` = all simultaneous; up to `5.0`s).
    #[serde(default)]
    pub stagger_sec: f64,
    /// Initial delay after Play before the first object triggers (`0.0..=60.0`s).
    #[serde(default)]
    pub delay_sec: f64,
    /// Vertical offset height in meters used by `DropVertical` (`1.0..=40.0`m).
    #[serde(default = "default_drop_height_m")]
    pub drop_height_m: f64,
    /// Renders an expanding tactical highlight ring on the ground when objects spawn/hide.
    #[serde(default = "default_true")]
    pub show_fx_ring: bool,
    /// If `true`, clicking master `Play Semua` triggers this animation automatically.
    /// Defaults to `false` so Spawn/Hide cues are triggered deliberately unless opted in.
    #[serde(default)]
    pub include_in_play_all: bool,
}

impl Default for SpawnHideConfig {
    fn default() -> Self {
        Self {
            action: SpawnHideAction::Spawn,
            marker_ids: Vec::new(),
            transition: SpawnHideTransition::PopScale,
            transition_sec: default_transition_sec(),
            stagger_sec: 0.0,
            delay_sec: 0.0,
            drop_height_m: default_drop_height_m(),
            show_fx_ring: true,
            include_in_play_all: false,
        }
    }
}

/// Sampled visual state of a single target marker at a given animation `elapsed_sec`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnHideTargetSample {
    /// Whether the marker should be rendered at this instant.
    pub visible: bool,
    /// Uniform 3D scale multiplier (`0.0..=1.05`, `1.0` at full rest).
    pub scale_factor: f32,
    /// Vertical altitude delta in meters (`0.0` at rest).
    pub altitude_offset_m: f64,
    /// Normalized `[0.0, 1.0]` ground-ring FX progress while active, or `None` when inactive.
    pub active_fx_progress: Option<f32>,
}

impl SpawnHideConfig {
    pub fn default_for(action: SpawnHideAction) -> Self {
        Self {
            action,
            ..Default::default()
        }
    }

    pub fn sanitize(&mut self) {
        let mut seen = Vec::with_capacity(self.marker_ids.len());
        for id in self.marker_ids.drain(..) {
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        self.marker_ids = seen;

        let clamp_f = |v: &mut f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() {
                *v = v.clamp(lo, hi);
            } else {
                *v = def;
            }
        };
        clamp_f(&mut self.transition_sec, 0.05, 3.0, default_transition_sec());
        clamp_f(&mut self.stagger_sec, 0.0, 5.0, 0.0);
        clamp_f(&mut self.delay_sec, 0.0, 60.0, 0.0);
        clamp_f(&mut self.drop_height_m, 1.0, 40.0, default_drop_height_m());
    }

    /// Toggles `marker_id` in `marker_ids`. Returns `true` if added, `false` if removed.
    pub fn toggle_marker(&mut self, marker_id: Uuid) -> bool {
        if self.marker_ids.contains(&marker_id) {
            self.marker_ids.retain(|id| *id != marker_id);
            false
        } else {
            self.marker_ids.push(marker_id);
            true
        }
    }

    /// Removes `marker_id` if present. Returns `true` if removed.
    pub fn remove_marker(&mut self, marker_id: Uuid) -> bool {
        if let Some(pos) = self.marker_ids.iter().position(|id| *id == marker_id) {
            self.marker_ids.remove(pos);
            true
        } else {
            false
        }
    }

    /// Effective per-item geometric transition duration (`0.0` for `Instant`).
    #[inline]
    pub fn effective_item_transition_sec(&self) -> f64 {
        match self.transition {
            SpawnHideTransition::Instant => 0.0,
            SpawnHideTransition::PopScale | SpawnHideTransition::DropVertical => {
                self.transition_sec.clamp(0.05, 3.0)
            }
        }
    }

    /// Effective per-item ground FX ring duration.
    #[inline]
    pub fn effective_fx_ring_sec(&self) -> f64 {
        if !self.show_fx_ring {
            return 0.0;
        }
        match self.transition {
            SpawnHideTransition::Instant => INSTANT_FX_RING_DURATION_SEC,
            SpawnHideTransition::PopScale | SpawnHideTransition::DropVertical => {
                self.effective_item_transition_sec().max(0.35)
            }
        }
    }

    /// Total playback duration in seconds until all targets finish transitioning (and any FX ring completes).
    pub fn total_duration_sec(&self) -> f64 {
        if self.marker_ids.is_empty() {
            return 0.0;
        }
        let stagger_span = self.stagger_sec.max(0.0) * (self.marker_ids.len() - 1) as f64;
        let tail = self
            .effective_item_transition_sec()
            .max(self.effective_fx_ring_sec());
        self.delay_sec.max(0.0) + stagger_span + tail
    }

    /// Evaluates the visual state of target at `target_index` at animation time `elapsed_sec`.
    pub fn evaluate_target(&self, target_index: usize, elapsed_sec: f64) -> SpawnHideTargetSample {
        let start_t = self.delay_sec.max(0.0) + self.stagger_sec.max(0.0) * target_index as f64;
        let dur = self.effective_item_transition_sec();
        let fx_dur = self.effective_fx_ring_sec();
        let local_t = elapsed_sec - start_t;

        let active_fx_progress = if fx_dur > 1e-4 && local_t >= 0.0 && local_t < fx_dur {
            Some((local_t / fx_dur).clamp(0.0, 1.0) as f32)
        } else {
            None
        };

        match self.action {
            SpawnHideAction::Spawn => {
                if local_t < 0.0 {
                    return SpawnHideTargetSample {
                        visible: false,
                        scale_factor: 0.0,
                        altitude_offset_m: 0.0,
                        active_fx_progress: None,
                    };
                }
                if dur <= 1e-4 || local_t >= dur {
                    return SpawnHideTargetSample {
                        visible: true,
                        scale_factor: 1.0,
                        altitude_offset_m: 0.0,
                        active_fx_progress,
                    };
                }
                let u = (local_t / dur).clamp(0.0, 1.0);
                match self.transition {
                    SpawnHideTransition::Instant => SpawnHideTargetSample {
                        visible: true,
                        scale_factor: 1.0,
                        altitude_offset_m: 0.0,
                        active_fx_progress,
                    },
                    SpawnHideTransition::PopScale => {
                        let s = ease_out_back_subtle(u).clamp(0.04, 1.04) as f32;
                        SpawnHideTargetSample {
                            visible: true,
                            scale_factor: s,
                            altitude_offset_m: 0.0,
                            active_fx_progress,
                        }
                    }
                    SpawnHideTransition::DropVertical => {
                        let ease = ease_out_cubic(u);
                        let s = (0.18 + 0.82 * ease) as f32;
                        let alt_off = self.drop_height_m.clamp(1.0, 40.0) * (1.0 - ease);
                        SpawnHideTargetSample {
                            visible: true,
                            scale_factor: s,
                            altitude_offset_m: alt_off,
                            active_fx_progress,
                        }
                    }
                }
            }
            SpawnHideAction::Hide => {
                if local_t < 0.0 {
                    return SpawnHideTargetSample {
                        visible: true,
                        scale_factor: 1.0,
                        altitude_offset_m: 0.0,
                        active_fx_progress: None,
                    };
                }
                if dur <= 1e-4 || local_t >= dur {
                    return SpawnHideTargetSample {
                        visible: false,
                        scale_factor: 0.0,
                        altitude_offset_m: 0.0,
                        active_fx_progress,
                    };
                }
                let u = (local_t / dur).clamp(0.0, 1.0);
                let sm = smoothstep(u);
                match self.transition {
                    SpawnHideTransition::Instant => SpawnHideTargetSample {
                        visible: false,
                        scale_factor: 0.0,
                        altitude_offset_m: 0.0,
                        active_fx_progress,
                    },
                    SpawnHideTransition::PopScale => {
                        let s = (1.0 - sm).max(0.02) as f32;
                        SpawnHideTargetSample {
                            visible: true,
                            scale_factor: s,
                            altitude_offset_m: 0.0,
                            active_fx_progress,
                        }
                    }
                    SpawnHideTransition::DropVertical => {
                        let s = (1.0 - 0.85 * sm).max(0.05) as f32;
                        let alt_off = -self.drop_height_m.clamp(1.0, 40.0) * 0.45 * sm;
                        SpawnHideTargetSample {
                            visible: true,
                            scale_factor: s,
                            altitude_offset_m: alt_off,
                            active_fx_progress,
                        }
                    }
                }
            }
        }
    }
}

#[inline]
fn ease_out_cubic(u: f64) -> f64 {
    1.0 - (1.0 - u).powi(3)
}

#[inline]
fn smoothstep(u: f64) -> f64 {
    u * u * (3.0 - 2.0 * u)
}

/// Subtle ease-out-back curve: smoothly reaches ~1.035 near `u = 0.78` and settles cleanly at `1.0`.
#[inline]
fn ease_out_back_subtle(u: f64) -> f64 {
    const C1: f64 = 0.85;
    const C3: f64 = C1 + 1.0;
    let t = u - 1.0;
    1.0 + C3 * t.powi(3) + C1 * t.powi(2)
}

pub fn parse_spawn_hide_config(anim: &Anim) -> SpawnHideConfig {
    let mut cfg: SpawnHideConfig = anim
        .data
        .get("spawn_hide")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if let Some(from_type) = SpawnHideAction::from_anim_type(&anim.anim_type) {
        cfg.action = from_type;
    }
    if cfg.marker_ids.is_empty()
        && let Some(mid) = anim.marker_id
    {
        cfg.marker_ids.push(mid);
    }
    cfg.sanitize();
    cfg
}

pub fn write_spawn_hide_config(anim: &mut Anim, cfg: &SpawnHideConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    anim.anim_type = clean.action.anim_type_str().to_string();
    anim.marker_id = clean.marker_ids.first().copied();
    anim.duration_sec = clean.total_duration_sec();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("spawn_hide".into(), val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_anim(action: SpawnHideAction) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: action.default_name(1),
            anim_type: action.anim_type_str().into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 0.45,
            replay: false,
            autoplay: false,
            geom: Vec::new(),
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn spawn_hide_config_roundtrips_and_deduplicates_markers() {
        let m1 = Uuid::new_v4();
        let m2 = Uuid::new_v4();
        let mut anim = sample_anim(SpawnHideAction::Hide);
        assert!(is_spawn_hide_anim(&anim));
        assert_eq!(spawn_hide_action(&anim), Some(SpawnHideAction::Hide));

        let mut cfg = SpawnHideConfig::default_for(SpawnHideAction::Hide);
        cfg.marker_ids = vec![m1, m2, m1];
        cfg.transition = SpawnHideTransition::DropVertical;
        cfg.transition_sec = 0.6;
        cfg.stagger_sec = 0.25;
        cfg.delay_sec = 0.5;
        cfg.include_in_play_all = true;

        write_spawn_hide_config(&mut anim, &cfg);
        assert_eq!(anim.marker_id, Some(m1));
        assert!((anim.duration_sec - (0.5 + 0.25 + 0.6)).abs() < 1e-6);

        let mut parsed = parse_spawn_hide_config(&anim);
        assert_eq!(parsed.action, SpawnHideAction::Hide);
        assert_eq!(parsed.marker_ids, vec![m1, m2]);
        assert_eq!(parsed.transition, SpawnHideTransition::DropVertical);
        assert!(parsed.include_in_play_all);

        assert!(!parsed.toggle_marker(m1));
        assert_eq!(parsed.marker_ids, vec![m2]);
        assert!(parsed.toggle_marker(m1));
        assert_eq!(parsed.marker_ids, vec![m2, m1]);
    }

    #[test]
    fn spawn_and_hide_target_evaluation_handles_delay_stagger_and_transitions() {
        let m1 = Uuid::new_v4();
        let m2 = Uuid::new_v4();
        let spawn_cfg = SpawnHideConfig {
            action: SpawnHideAction::Spawn,
            marker_ids: vec![m1, m2],
            transition: SpawnHideTransition::DropVertical,
            transition_sec: 0.5,
            stagger_sec: 0.3,
            delay_sec: 0.2,
            drop_height_m: 8.0,
            show_fx_ring: true,
            include_in_play_all: false,
        };

        // Before delay: target 0 hidden
        let before = spawn_cfg.evaluate_target(0, 0.1);
        assert!(!before.visible);
        assert!(before.active_fx_progress.is_none());

        // Mid-transition for target 0 (t = 0.45 -> local 0.25 -> u = 0.5): visible, dropping from above
        let mid0 = spawn_cfg.evaluate_target(0, 0.45);
        assert!(mid0.visible);
        assert!(mid0.scale_factor > 0.5 && mid0.scale_factor < 1.0);
        assert!(mid0.altitude_offset_m > 0.0 && mid0.altitude_offset_m < 8.0);
        assert!(mid0.active_fx_progress.is_some());

        // Target 1 hasn't started yet at t = 0.45 (starts at 0.2 + 0.3 = 0.5)
        let pre1 = spawn_cfg.evaluate_target(1, 0.45);
        assert!(!pre1.visible);

        // After total duration: both visible at full scale and 0 altitude offset
        let end = spawn_cfg.total_duration_sec() + 0.1;
        let done1 = spawn_cfg.evaluate_target(1, end);
        assert!(done1.visible);
        assert!((done1.scale_factor - 1.0).abs() < 1e-5);
        assert!(done1.altitude_offset_m.abs() < 1e-5);

        // Hide Instant disappears immediately on trigger
        let hide_instant = SpawnHideConfig {
            action: SpawnHideAction::Hide,
            marker_ids: vec![m1],
            transition: SpawnHideTransition::Instant,
            show_fx_ring: false,
            ..Default::default()
        };
        assert_eq!(hide_instant.total_duration_sec(), 0.0);
        assert!(!hide_instant.evaluate_target(0, 0.0).visible);
    }
}
