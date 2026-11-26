//! Global scenario audio. Configuration lives in `Anim.data.audio`.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Anim;

pub const ANIM_TYPE_PLAY_SOUND: &str = "play_sound";

pub fn is_audio_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_PLAY_SOUND
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRepeat {
    #[default]
    Once,
    Count,
    Forever,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioAnimConfig {
    pub asset_id: Option<Uuid>,
    pub volume: f32,
    pub repeat: AudioRepeat,
    /// Total plays, including the first play.
    pub repeat_count: u32,
    pub delay_sec: f64,
    pub clip_start_sec: f64,
    pub clip_end_sec: Option<f64>,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}

impl Default for AudioAnimConfig {
    fn default() -> Self {
        Self {
            asset_id: None,
            volume: 1.0,
            repeat: AudioRepeat::Once,
            repeat_count: 2,
            delay_sec: 0.0,
            clip_start_sec: 0.0,
            clip_end_sec: None,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

impl AudioAnimConfig {
    pub fn sanitize(&mut self) {
        fn positive(v: f64) -> f64 {
            if v.is_finite() {
                v.clamp(0.0, 86400.0)
            } else {
                0.0
            }
        }
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.repeat_count = self.repeat_count.clamp(1, 10000);
        self.delay_sec = positive(self.delay_sec);
        self.clip_start_sec = positive(self.clip_start_sec);
        self.clip_end_sec = self.clip_end_sec.map(positive);
        self.fade_in_sec = positive(self.fade_in_sec);
        self.fade_out_sec = positive(self.fade_out_sec);
    }

    pub fn clip_duration(&self, source_duration: f64) -> Option<f64> {
        if !source_duration.is_finite() || source_duration <= 0.0 {
            return None;
        }
        let end = self
            .clip_end_sec
            .unwrap_or(source_duration)
            .min(source_duration);
        let duration = end - self.clip_start_sec;
        (duration > 0.001).then_some(duration)
    }

    pub fn plays(&self) -> Option<u32> {
        match self.repeat {
            AudioRepeat::Once => Some(1),
            AudioRepeat::Count => Some(self.repeat_count.max(1)),
            AudioRepeat::Forever => None,
        }
    }

    pub fn total_duration(&self, source_duration: f64) -> Option<f64> {
        Some(self.delay_sec + self.clip_duration(source_duration)? * self.plays()? as f64)
    }

    /// Fades apply to each trimmed clip, including each repetition.
    pub fn gain(&self, position: f64, duration: f64) -> f32 {
        let fade_in = if self.fade_in_sec > 0.0 {
            (position / self.fade_in_sec).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let fade_out = if self.fade_out_sec > 0.0 {
            ((duration - position) / self.fade_out_sec).clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.volume * fade_in.min(fade_out) as f32
    }
}

pub fn parse_audio_anim_config(anim: &Anim) -> AudioAnimConfig {
    let mut cfg: AudioAnimConfig = anim
        .data
        .get("audio")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}

pub fn write_audio_anim_config(anim: &mut Anim, cfg: &AudioAnimConfig) {
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    let mut cfg = cfg.clone();
    cfg.sanitize();
    anim.data["audio"] = serde_json::to_value(cfg).expect("sanitized audio configuration");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_configuration_survives_server_payload_roundtrip() {
        let now = chrono::Utc::now();
        let mut anim = Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Play Suara".into(),
            anim_type: ANIM_TYPE_PLAY_SOUND.into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 0.0,
            replay: false,
            autoplay: true,
            geom: vec![],
            data: serde_json::json!({"unrelated": "preserved"}),
            created_at: now,
            updated_at: now,
        };
        let cfg = AudioAnimConfig {
            asset_id: Some(Uuid::new_v4()),
            volume: 0.35,
            repeat: AudioRepeat::Count,
            repeat_count: 4,
            delay_sec: 1.5,
            clip_start_sec: 2.0,
            clip_end_sec: Some(9.0),
            fade_in_sec: 0.5,
            fade_out_sec: 1.0,
        };
        write_audio_anim_config(&mut anim, &cfg);
        let restored: Anim = serde_json::from_str(&serde_json::to_string(&anim).unwrap()).unwrap();
        assert_eq!(parse_audio_anim_config(&restored), cfg);
        assert_eq!(restored.data["unrelated"], "preserved");
        assert!(crate::route_formation::anim_is_playable(&restored, &[]));
        assert!(!crate::group_anim::anim_is_continuous_unbounded(&restored));
        assert!(restored.autoplay);
    }

    #[test]
    fn trimmed_repeat_duration_includes_delay_once() {
        let cfg = AudioAnimConfig {
            clip_start_sec: 2.0,
            clip_end_sec: Some(6.0),
            delay_sec: 3.0,
            repeat: AudioRepeat::Count,
            repeat_count: 3,
            ..Default::default()
        };
        assert_eq!(cfg.total_duration(10.0), Some(15.0));
        assert_eq!(cfg.clip_duration(4.0), Some(2.0));
        assert_eq!(cfg.clip_duration(1.0), None);
        assert_eq!(
            AudioAnimConfig {
                repeat: AudioRepeat::Forever,
                ..cfg
            }
            .total_duration(10.0),
            None
        );
    }

    #[test]
    fn fades_and_invalid_numbers_are_bounded() {
        let mut cfg = AudioAnimConfig {
            volume: f32::NAN,
            delay_sec: f64::INFINITY,
            repeat_count: 0,
            fade_in_sec: 2.0,
            fade_out_sec: 2.0,
            ..Default::default()
        };
        cfg.sanitize();
        assert_eq!(cfg.volume, 1.0);
        assert_eq!(cfg.delay_sec, 0.0);
        assert_eq!(cfg.repeat_count, 1);
        assert_eq!(cfg.gain(0.0, 8.0), 0.0);
        assert_eq!(cfg.gain(1.0, 8.0), 0.5);
        assert_eq!(cfg.gain(4.0, 8.0), 1.0);
        assert_eq!(cfg.gain(7.0, 8.0), 0.5);
    }
}
