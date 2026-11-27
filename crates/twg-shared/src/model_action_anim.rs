use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Anim;

pub const ANIM_TYPE_MODEL_ACTION: &str = "model_action";

pub fn is_model_action_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_MODEL_ACTION
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelActionPlayMode {
    /// Loop the action clip forever until stopped or interrupted by route movement.
    #[default]
    LoopForever,
    /// Play once from frame 0 to the final frame and remain frozen at the final pose (e.g., fainting).
    OnceHoldLastFrame,
    /// Play once and automatically return to Idle pose when finished (e.g., shooting / saluting).
    OnceReturnIdle,
    /// Repeat `repeat_count` times and then return to Idle pose.
    RepeatCountReturnIdle,
}

impl ModelActionPlayMode {
    pub const ALL: &'static [Self] = &[
        Self::LoopForever,
        Self::OnceHoldLastFrame,
        Self::OnceReturnIdle,
        Self::RepeatCountReturnIdle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::LoopForever => "Loop Terus (Repeat)",
            Self::OnceHoldLastFrame => "Sekali & Tahan Pose Akhir (Pingsan)",
            Self::OnceReturnIdle => "Sekali & Kembali ke Idle (Menembak)",
            Self::RepeatCountReturnIdle => "Ulangi N× lalu Kembali ke Idle",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::LoopForever => "Loop Terus",
            Self::OnceHoldLastFrame => "Sekali (Tahan Akhir)",
            Self::OnceReturnIdle => "Sekali -> Idle",
            Self::RepeatCountReturnIdle => "Ulangi N× -> Idle",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::LoopForever => {
                "Memutar animasi berulang terus-menerus hingga tombol Stop ditekan atau unit berjalan (contoh: polisi melambai mengatur lalu lintas)."
            }
            Self::OnceHoldLastFrame => {
                "Memutar 1 kali hingga frame terakhir lalu tetap diam di pose akhir tersebut sampai Stop atau unit berjalan (contoh: pingsan berdiri -> rebah)."
            }
            Self::OnceReturnIdle => {
                "Memutar 1 kali hingga selesai lalu otomatis kembali ke pose Idle (contoh: menembak / hormat — cocok di-Combine dengan Shoot)."
            }
            Self::RepeatCountReturnIdle => {
                "Mengulang animasi sebanyak N kali lalu otomatis kembali ke pose Idle."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelActionAnimConfig {
    #[serde(default)]
    pub marker_id: Option<Uuid>,
    /// Matches `ModelCatalogActionClip::slug` for uploaded extra action GLBs,
    /// or `embedded:<clip_name>` for multi-clip NLA animations inside the model's GLB.
    #[serde(default)]
    pub action_slug: String,
    /// Friendly display name cached for summaries even before catalog lookup.
    #[serde(default)]
    pub action_label: String,
    #[serde(default)]
    pub play_mode: ModelActionPlayMode,
    #[serde(default = "default_repeat_count")]
    pub repeat_count: u32,
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// Optional fine-tune scale multiplier (`0.10..=5.0`, default `1.0`) applied on top of
    /// automatic Idle height calibration.
    #[serde(default = "default_scale_multiplier")]
    pub scale_multiplier: f64,
    #[serde(default)]
    pub delay_sec: f64,
    /// Automatically stop this action and switch to Walk whenever the bound marker starts moving
    /// along any Route, Formation, or RTS move order.
    #[serde(default = "default_true")]
    pub stop_on_route_move: bool,
}

fn default_repeat_count() -> u32 {
    2
}
fn default_speed() -> f64 {
    1.0
}
fn default_scale_multiplier() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}

impl Default for ModelActionAnimConfig {
    fn default() -> Self {
        Self {
            marker_id: None,
            action_slug: String::new(),
            action_label: String::new(),
            play_mode: ModelActionPlayMode::LoopForever,
            repeat_count: default_repeat_count(),
            speed: default_speed(),
            scale_multiplier: default_scale_multiplier(),
            delay_sec: 0.0,
            stop_on_route_move: true,
        }
    }
}

impl ModelActionAnimConfig {
    pub fn sanitized(mut self) -> Self {
        self.action_slug = self.action_slug.trim().to_string();
        self.action_label = self.action_label.trim().to_string();
        self.repeat_count = self.repeat_count.clamp(1, 1000);
        self.speed = if self.speed.is_finite() {
            self.speed.clamp(0.15, 4.0)
        } else {
            1.0
        };
        self.scale_multiplier = if self.scale_multiplier.is_finite() {
            self.scale_multiplier.clamp(0.1, 5.0)
        } else {
            1.0
        };
        self.delay_sec = if self.delay_sec.is_finite() {
            self.delay_sec.clamp(0.0, 3600.0)
        } else {
            0.0
        };
        self
    }

    pub fn is_continuous(&self) -> bool {
        self.play_mode == ModelActionPlayMode::LoopForever
    }

    pub fn plays(&self) -> Option<u32> {
        match self.play_mode {
            ModelActionPlayMode::LoopForever => None,
            ModelActionPlayMode::OnceHoldLastFrame | ModelActionPlayMode::OnceReturnIdle => Some(1),
            ModelActionPlayMode::RepeatCountReturnIdle => Some(self.repeat_count.max(1)),
        }
    }

    pub fn has_playable_setup(&self) -> bool {
        self.marker_id.is_some() && !self.action_slug.is_empty()
    }
}

pub fn parse_model_action_config(anim: &Anim) -> ModelActionAnimConfig {
    let mut cfg: ModelActionAnimConfig = anim
        .data
        .get("model_action")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.marker_id.is_none() {
        cfg.marker_id = anim.marker_id;
    }
    cfg.sanitized()
}

pub fn write_model_action_config(anim: &mut Anim, cfg: &ModelActionAnimConfig) {
    let clean = cfg.clone().sanitized();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "model_action".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    anim.marker_id = clean.marker_id;
    anim.replay = clean.play_mode == ModelActionPlayMode::LoopForever;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn model_action_config_roundtrips_all_modes() {
        let now = Utc::now();
        let mid = Uuid::new_v4();
        let mut anim = Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Polisi Melambai".into(),
            anim_type: ANIM_TYPE_MODEL_ACTION.into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 0.0,
            replay: false,
            autoplay: false,
            geom: vec![],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };
        let cfg = ModelActionAnimConfig {
            marker_id: Some(mid),
            action_slug: "pingsan".into(),
            action_label: "Pingsan".into(),
            play_mode: ModelActionPlayMode::OnceHoldLastFrame,
            repeat_count: 1,
            speed: 1.25,
            scale_multiplier: 1.2,
            delay_sec: 0.5,
            stop_on_route_move: true,
        };
        write_model_action_config(&mut anim, &cfg);
        assert_eq!(anim.marker_id, Some(mid));
        let parsed = parse_model_action_config(&anim);
        assert_eq!(parsed, cfg);
        assert!(parsed.has_playable_setup());
        assert!(!parsed.is_continuous());
        assert_eq!(parsed.plays(), Some(1));
    }
}
