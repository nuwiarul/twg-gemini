use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Anim;

pub const ANIM_TYPE_COMBINE: &str = "group_combine";
pub const ANIM_TYPE_SEQUENCE: &str = "group_sequence";
pub const ANIM_TYPE_GROUP_LEGACY: &str = "group";

/// Orchestration mode of a `Grup` animation combining 2 or more animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupAnimMode {
    /// Plays all member animations simultaneously (parallel).
    #[default]
    Combine,
    /// Plays member animations one after another in order (sequential).
    Sequence,
}

impl GroupAnimMode {
    pub const ALL: &'static [Self] = &[Self::Combine, Self::Sequence];

    pub fn anim_type_str(self) -> &'static str {
        match self {
            Self::Combine => ANIM_TYPE_COMBINE,
            Self::Sequence => ANIM_TYPE_SEQUENCE,
        }
    }

    pub fn from_anim_type(anim_type: &str) -> Option<Self> {
        match anim_type {
            ANIM_TYPE_COMBINE | "combine" => Some(Self::Combine),
            ANIM_TYPE_SEQUENCE | "sequence" => Some(Self::Sequence),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Combine => "Combine (Bareng)",
            Self::Sequence => "Sequence (Berurutan)",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Combine => "COMBINE",
            Self::Sequence => "SEQUENCE",
        }
    }

    pub fn default_name(self, index: usize) -> String {
        match self {
            Self::Combine => format!("Combine {index}"),
            Self::Sequence => format!("Sequence {index}"),
        }
    }
}

/// How a `Sequence` decides when to launch the next step after launching the previous step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceTriggerMode {
    /// Wait until the previous step finishes 1 full cycle (if the previous step has `replay = true`,
    /// once its 1st cycle completes, the next step starts while the repeating step keeps looping).
    #[default]
    WaitComplete,
    /// Wait a fixed delay in seconds after the previous step *started* (e.g. `0.25`s) before
    /// launching the next step, without waiting for the previous step to finish.
    WaitDelay,
}

impl SequenceTriggerMode {
    pub const ALL: &'static [Self] = &[Self::WaitComplete, Self::WaitDelay];

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitComplete => "Tunggu Selesai",
            Self::WaitDelay => "Jeda Waktu (Detik)",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::WaitComplete => "Tunggu Selesai",
            Self::WaitDelay => "Jeda Detik",
        }
    }
}

/// One member animation inside a `Combine` or `Sequence` group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupStepItem {
    /// Referenced child animation UUID.
    pub anim_id: Uuid,
    /// Optional per-step trigger override (applies when starting steps at index `>= 1` in `Sequence`).
    /// `None` inherits `GroupAnimConfig::sequence_trigger`.
    #[serde(default)]
    pub trigger_override: Option<SequenceTriggerMode>,
    /// Optional per-step delay/gap override in seconds (`0.0..=120.0`).
    /// - For `WaitDelay`: seconds after previous step started.
    /// - For `WaitComplete`: extra gap pause after previous step completed 1 cycle.
    ///
    /// `None` inherits `sequence_delay_sec` (for `WaitDelay`) or `sequence_gap_sec` (for `WaitComplete`).
    #[serde(default)]
    pub delay_override_sec: Option<f64>,
}

impl GroupStepItem {
    pub fn new(anim_id: Uuid) -> Self {
        Self {
            anim_id,
            trigger_override: None,
            delay_override_sec: None,
        }
    }
}

fn default_sequence_delay_sec() -> f64 {
    0.5
}

fn default_continuous_wait_sec() -> f64 {
    1.0
}

/// Configuration for a `Grup` (`Combine` or `Sequence`) animation stored inside `anim.data["group"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupAnimConfig {
    /// `Combine` (parallel) or `Sequence` (sequential).
    #[serde(default)]
    pub mode: GroupAnimMode,
    /// Ordered list of child animations orchestrated by this group.
    #[serde(default)]
    pub items: Vec<GroupStepItem>,
    /// Default trigger mode for `Sequence` (`WaitComplete` vs `WaitDelay`).
    #[serde(default)]
    pub sequence_trigger: SequenceTriggerMode,
    /// Default delay in seconds after previous step start when `sequence_trigger == WaitDelay`.
    #[serde(default = "default_sequence_delay_sec")]
    pub sequence_delay_sec: f64,
    /// Optional extra gap pause in seconds after previous step finishes when `WaitComplete`.
    #[serde(default)]
    pub sequence_gap_sec: f64,
    /// Safe handoff wait (seconds) in `WaitComplete` mode if a child is unbounded `continuous = true`
    /// (such as continuous Water Cannon / Fire / Pointer Arrow) so Sequence never stalls indefinitely.
    #[serde(default = "default_continuous_wait_sec")]
    pub continuous_wait_sec: f64,
    /// Optional initial delay before starting the Combine or Step #1 of Sequence (`0.0..=60.0`s).
    #[serde(default)]
    pub initial_delay_sec: f64,
    /// Whether clicking master `Play Semua` also triggers this group (default `false` so children
    /// are not double-triggered unless opted in).
    #[serde(default)]
    pub include_in_play_all: bool,
}

impl Default for GroupAnimConfig {
    fn default() -> Self {
        Self {
            mode: GroupAnimMode::Combine,
            items: Vec::new(),
            sequence_trigger: SequenceTriggerMode::WaitComplete,
            sequence_delay_sec: default_sequence_delay_sec(),
            sequence_gap_sec: 0.0,
            continuous_wait_sec: default_continuous_wait_sec(),
            initial_delay_sec: 0.0,
            include_in_play_all: false,
        }
    }
}

impl GroupAnimConfig {
    pub fn default_for(mode: GroupAnimMode) -> Self {
        Self {
            mode,
            ..Default::default()
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
        clamp_f(
            &mut self.sequence_delay_sec,
            0.0,
            120.0,
            default_sequence_delay_sec(),
        );
        clamp_f(&mut self.sequence_gap_sec, 0.0, 60.0, 0.0);
        clamp_f(
            &mut self.continuous_wait_sec,
            0.0,
            60.0,
            default_continuous_wait_sec(),
        );
        clamp_f(&mut self.initial_delay_sec, 0.0, 60.0, 0.0);

        for item in &mut self.items {
            if let Some(ref mut d) = item.delay_override_sec {
                if d.is_finite() {
                    *d = d.clamp(0.0, 120.0);
                } else {
                    *d = 0.0;
                }
            }
        }
        // In Combine mode, duplicate entries for the exact same animation are redundant -> dedupe
        if self.mode == GroupAnimMode::Combine {
            let mut seen = HashSet::new();
            self.items.retain(|it| seen.insert(it.anim_id));
        }
    }

    /// Resolves the effective `(SequenceTriggerMode, delay_or_gap_sec)` for step `step_idx` (`>= 1`).
    pub fn effective_step_trigger(&self, step_idx: usize) -> (SequenceTriggerMode, f64) {
        let Some(item) = self.items.get(step_idx) else {
            return (self.sequence_trigger, self.sequence_delay_sec);
        };
        let trigger = item.trigger_override.unwrap_or(self.sequence_trigger);
        let default_sec = match trigger {
            SequenceTriggerMode::WaitComplete => self.sequence_gap_sec.max(0.0),
            SequenceTriggerMode::WaitDelay => self.sequence_delay_sec.max(0.0),
        };
        let sec = item.delay_override_sec.unwrap_or(default_sec).clamp(0.0, 120.0);
        (trigger, sec)
    }

    /// Unique child animation IDs referenced by this group, preserving first-seen order.
    pub fn unique_child_ids(&self) -> Vec<Uuid> {
        let mut out = Vec::with_capacity(self.items.len());
        for it in &self.items {
            if !out.contains(&it.anim_id) {
                out.push(it.anim_id);
            }
        }
        out
    }
}

/// Returns `true` if `anim` belongs to the `Grup` (`Combine` / `Sequence`) category.
#[inline]
pub fn is_group_anim(anim: &Anim) -> bool {
    GroupAnimMode::from_anim_type(&anim.anim_type).is_some()
        || anim.anim_type == ANIM_TYPE_GROUP_LEGACY
        || anim.anim_type.starts_with("group_")
}

/// Resolves whether a `Grup` animation is `Combine` or `Sequence`.
pub fn group_anim_mode(anim: &Anim) -> Option<GroupAnimMode> {
    if !is_group_anim(anim) {
        return None;
    }
    if let Some(m) = GroupAnimMode::from_anim_type(&anim.anim_type) {
        return Some(m);
    }
    Some(parse_group_anim_config(anim).mode)
}

pub fn parse_group_anim_config(anim: &Anim) -> GroupAnimConfig {
    let mut cfg: GroupAnimConfig = anim
        .data
        .get("group")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if let Some(from_type) = GroupAnimMode::from_anim_type(&anim.anim_type) {
        cfg.mode = from_type;
    }
    cfg.sanitize();
    cfg
}

pub fn write_group_anim_config(anim: &mut Anim, cfg: &GroupAnimConfig) {
    let mut clean = cfg.clone();
    clean.sanitize();
    anim.anim_type = clean.mode.anim_type_str().to_string();
    anim.marker_id = None;
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("group".into(), val);
    }
}

/// Returns `true` if adding `candidate_child_id` into `parent_id` would create a cycle
/// (either direct self-reference or transitive cycle through nested groups).
pub fn would_create_group_cycle(
    parent_id: Uuid,
    candidate_child_id: Uuid,
    all_anims: &[Anim],
) -> bool {
    if parent_id == candidate_child_id {
        return true;
    }
    let mut stack = vec![candidate_child_id];
    let mut visited = HashSet::new();
    while let Some(curr) = stack.pop() {
        if !visited.insert(curr) {
            continue;
        }
        if let Some(a) = all_anims.iter().find(|x| x.id == curr)
            && is_group_anim(a)
        {
            let cfg = parse_group_anim_config(a);
            for item in cfg.items {
                if item.anim_id == parent_id {
                    return true;
                }
                stack.push(item.anim_id);
            }
        }
    }
    false
}

/// Collects all direct and transitive child animation IDs under `root_id` (cycle-safe).
/// Does not include `root_id` itself.
pub fn collect_group_descendant_ids(root_id: Uuid, all_anims: &[Anim]) -> Vec<Uuid> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    visited.insert(root_id);

    let mut stack = Vec::new();
    if let Some(root) = all_anims.iter().find(|x| x.id == root_id)
        && is_group_anim(root)
    {
        for item in parse_group_anim_config(root).items {
            stack.push(item.anim_id);
        }
    }

    while let Some(curr) = stack.pop() {
        if !visited.insert(curr) {
            continue;
        }
        out.push(curr);
        if let Some(a) = all_anims.iter().find(|x| x.id == curr)
            && is_group_anim(a)
        {
            for item in parse_group_anim_config(a).items {
                stack.push(item.anim_id);
            }
        }
    }
    out
}

/// Returns `true` if `anim` runs unbounded continuously (`continuous = true`) and therefore
/// never naturally completes a cycle on its own (e.g., continuous Water Cannon, Fire, Pointer Arrow).
pub fn anim_is_continuous_unbounded(anim: &Anim) -> bool {
    if crate::audio_anim::is_audio_anim(anim) {
        return crate::audio_anim::parse_audio_anim_config(anim).plays().is_none();
    }
    if crate::billboard_anim::is_billboard_anim(anim) {
        return crate::billboard_anim::parse_billboard_anim_config(anim).is_continuous();
    }
    if crate::camera_anim::is_camera_static_anim(anim) {
        return crate::camera_anim::parse_camera_static_config(anim).is_continuous();
    }
    if crate::camera_anim::is_camera_follow_anim(anim) {
        return crate::camera_anim::parse_camera_follow_config(anim).is_continuous();
    }
    if crate::environment_anim::is_water_anim(anim) {
        return crate::environment_anim::parse_water_anim_config(anim).continuous;
    }
    if crate::environment_anim::is_rain_anim(anim) {
        return crate::environment_anim::parse_rain_anim_config(anim).continuous;
    }
    if crate::tactical_arrow_anim::is_tactical_arrow_anim(anim) {
        return crate::tactical_arrow_anim::parse_tactical_arrow_config(anim).continuous;
    }
    if crate::hud_banner_anim::is_hud_banner_anim(anim) {
        return crate::hud_banner_anim::parse_hud_banner_config(anim).hold_until_stop;
    }
    if crate::model_action_anim::is_model_action_anim(anim) {
        return crate::model_action_anim::parse_model_action_config(anim).is_continuous();
    }
    if crate::dialog_anim::is_dialog_anim(anim) {
        return false;
    }
    if crate::shoot_anim::is_shoot_anim(anim) {
        return crate::shoot_anim::shoot_anim_is_continuous(anim);
    }
    if crate::particle_anim::is_particle_anim(anim) {
        return crate::particle_anim::parse_particle_anim_config(anim).continuous;
    }
    if crate::pointer_anim::is_pointer_arrow_anim(anim) {
        return crate::pointer_anim::parse_pointer_arrow_config(anim).continuous;
    }
    if crate::traffic_anim::is_traffic_flow_or_signal(anim) {
        return crate::traffic_anim::parse_traffic_anim_config(anim).continuous;
    }
    if crate::traffic_queue::is_traffic_queue_anim(anim)
        || crate::traffic_diversion::is_traffic_diversion_anim(anim)
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_group(id: Uuid, mode: GroupAnimMode, children: &[Uuid]) -> Anim {
        let now = Utc::now();
        let mut anim = Anim {
            id,
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: mode.default_name(1),
            anim_type: mode.anim_type_str().into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 0.0,
            replay: false,
            autoplay: false,
            geom: Vec::new(),
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };
        let mut cfg = GroupAnimConfig::default_for(mode);
        cfg.items = children.iter().map(|&c| GroupStepItem::new(c)).collect();
        write_group_anim_config(&mut anim, &cfg);
        anim
    }

    #[test]
    fn group_config_roundtrips_and_resolves_step_overrides() {
        let g_id = Uuid::new_v4();
        let a_id = Uuid::new_v4();
        let b_id = Uuid::new_v4();
        let c_id = Uuid::new_v4();
        let mut anim = sample_group(g_id, GroupAnimMode::Sequence, &[a_id, b_id, c_id]);
        assert!(is_group_anim(&anim));
        assert_eq!(group_anim_mode(&anim), Some(GroupAnimMode::Sequence));

        let mut cfg = parse_group_anim_config(&anim);
        cfg.sequence_trigger = SequenceTriggerMode::WaitComplete;
        cfg.sequence_gap_sec = 0.2;
        cfg.sequence_delay_sec = 0.5;
        cfg.items[2].trigger_override = Some(SequenceTriggerMode::WaitDelay);
        cfg.items[2].delay_override_sec = Some(0.25);
        write_group_anim_config(&mut anim, &cfg);

        let parsed = parse_group_anim_config(&anim);
        assert_eq!(
            parsed.effective_step_trigger(1),
            (SequenceTriggerMode::WaitComplete, 0.2)
        );
        assert_eq!(
            parsed.effective_step_trigger(2),
            (SequenceTriggerMode::WaitDelay, 0.25)
        );
    }

    #[test]
    fn cycle_detection_blocks_direct_and_transitive_loops() {
        let leaf = Uuid::new_v4();
        let g1 = Uuid::new_v4();
        let g2 = Uuid::new_v4();
        let g3 = Uuid::new_v4();

        // g1 -> g2 -> g3 -> leaf
        let anims = vec![
            sample_group(g1, GroupAnimMode::Sequence, &[g2]),
            sample_group(g2, GroupAnimMode::Combine, &[g3]),
            sample_group(g3, GroupAnimMode::Combine, &[leaf]),
        ];

        assert!(would_create_group_cycle(g1, g1, &anims));
        assert!(would_create_group_cycle(g3, g1, &anims));
        assert!(would_create_group_cycle(g2, g1, &anims));
        assert!(!would_create_group_cycle(g1, leaf, &anims));

        let desc = collect_group_descendant_ids(g1, &anims);
        assert!(desc.contains(&g2));
        assert!(desc.contains(&g3));
        assert!(desc.contains(&leaf));
        assert!(!desc.contains(&g1));
    }
}
