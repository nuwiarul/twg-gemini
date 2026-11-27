//! Procedural 3D Billboard / Videotron animation configuration stored in `anim.data["billboard"]`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Anim;

pub const ANIM_TYPE_BILLBOARD: &str = "billboard";

pub fn is_billboard_anim(anim: &Anim) -> bool {
    anim.anim_type.eq_ignore_ascii_case(ANIM_TYPE_BILLBOARD)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillboardMediaMode {
    Image,
    #[default]
    Video,
}

impl BillboardMediaMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "Gambar (Statis)",
            Self::Video => "Video (Videotron)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillboardStructureKind {
    #[default]
    Monopole,
    TwinPole,
    WallMount,
}

impl BillboardStructureKind {
    pub const ALL: &'static [Self] = &[Self::Monopole, Self::TwinPole, Self::WallMount];

    pub fn label(self) -> &'static str {
        match self {
            Self::Monopole => "Tiang Tunggal (Monopole)",
            Self::TwinPole => "Tiang Ganda (2 Kaki)",
            Self::WallMount => "Tempel Dinding (Tanpa Tiang)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillboardRepeat {
    #[default]
    Once,
    Count,
    Forever,
}

impl BillboardRepeat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Once => "Sekali",
            Self::Count => "Ulangi N kali",
            Self::Forever => "Terus-menerus (Loop)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillboardIdleMode {
    Dark,
    #[default]
    Thumbnail,
    HoldLastFrame,
}

impl BillboardIdleMode {
    pub const ALL: &'static [Self] = &[Self::Thumbnail, Self::Dark, Self::HoldLastFrame];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Layar Gelap (Standby)",
            Self::Thumbnail => "Thumbnail / Frame Awal",
            Self::HoldLastFrame => "Tahan Frame Terakhir",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BillboardAnimConfig {
    pub media_mode: BillboardMediaMode,
    pub image_asset_id: Option<Uuid>,
    pub video_asset_id: Option<Uuid>,
    pub structure: BillboardStructureKind,
    pub screen_width_m: f64,
    pub screen_height_m: f64,
    pub pole_height_m: f64,
    pub double_sided: bool,
    pub brightness: f32,
    pub frame_color_hex: String,
    pub pos_lnglat: [f64; 3],
    pub rot_deg: [f64; 3],
    pub scale: [f64; 3],
    pub repeat: BillboardRepeat,
    pub repeat_count: u32,
    pub delay_sec: f64,
    pub play_audio: bool,
    pub volume: f32,
    pub idle_mode: BillboardIdleMode,
}

impl Default for BillboardAnimConfig {
    fn default() -> Self {
        Self {
            media_mode: BillboardMediaMode::Video,
            image_asset_id: None,
            video_asset_id: None,
            structure: BillboardStructureKind::Monopole,
            screen_width_m: 8.0,
            screen_height_m: 4.5,
            pole_height_m: 5.0,
            double_sided: false,
            brightness: 1.25,
            frame_color_hex: "#1e293b".to_string(),
            pos_lnglat: [0.0, 0.0, 0.0],
            rot_deg: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            repeat: BillboardRepeat::Once,
            repeat_count: 2,
            delay_sec: 0.0,
            play_audio: false,
            volume: 0.85,
            idle_mode: BillboardIdleMode::Thumbnail,
        }
    }
}

impl BillboardAnimConfig {
    pub fn sanitized(&self) -> Self {
        let clean_dim = |v: f64, lo: f64, hi: f64, fb: f64| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fb
            }
        };
        let clean_rot = |v: f64| if v.is_finite() { v } else { 0.0 };
        let clean_scale = |v: f64| if v.is_finite() { v.clamp(0.05, 25.0) } else { 1.0 };
        let hex = self.frame_color_hex.trim();
        let frame_color_hex = if hex.len() == 7 && hex.starts_with('#') {
            hex.to_string()
        } else {
            "#1e293b".to_string()
        };
        Self {
            media_mode: self.media_mode,
            image_asset_id: self.image_asset_id,
            video_asset_id: self.video_asset_id,
            structure: self.structure,
            screen_width_m: clean_dim(self.screen_width_m, 0.5, 120.0, 8.0),
            screen_height_m: clean_dim(self.screen_height_m, 0.5, 80.0, 4.5),
            pole_height_m: clean_dim(self.pole_height_m, 0.0, 60.0, 5.0),
            double_sided: self.double_sided,
            brightness: if self.brightness.is_finite() {
                self.brightness.clamp(0.15, 4.0)
            } else {
                1.25
            },
            frame_color_hex,
            pos_lnglat: [
                if self.pos_lnglat[0].is_finite() {
                    self.pos_lnglat[0]
                } else {
                    0.0
                },
                if self.pos_lnglat[1].is_finite() {
                    self.pos_lnglat[1]
                } else {
                    0.0
                },
                clean_dim(self.pos_lnglat[2], -100.0, 20_000.0, 0.0),
            ],
            rot_deg: [
                clean_rot(self.rot_deg[0]),
                clean_rot(self.rot_deg[1]),
                clean_rot(self.rot_deg[2]),
            ],
            scale: [
                clean_scale(self.scale[0]),
                clean_scale(self.scale[1]),
                clean_scale(self.scale[2]),
            ],
            repeat: self.repeat,
            repeat_count: self.repeat_count.clamp(1, 10_000),
            delay_sec: clean_dim(self.delay_sec, 0.0, 3600.0, 0.0),
            play_audio: self.play_audio,
            volume: if self.volume.is_finite() {
                self.volume.clamp(0.0, 1.0)
            } else {
                0.85
            },
            idle_mode: self.idle_mode,
        }
    }

    /// Only Video mode with a selected video asset actively plays when triggered.
    /// Image mode displays statically on the 3D Billboard and triggers no animation on Play.
    pub fn is_playable_video(&self) -> bool {
        self.media_mode == BillboardMediaMode::Video && self.video_asset_id.is_some()
    }

    pub fn is_continuous(&self) -> bool {
        self.media_mode == BillboardMediaMode::Video && self.repeat == BillboardRepeat::Forever
    }

    pub fn plays(&self) -> Option<u32> {
        match self.repeat {
            BillboardRepeat::Once => Some(1),
            BillboardRepeat::Count => Some(self.repeat_count.max(1)),
            BillboardRepeat::Forever => None,
        }
    }

    pub fn total_height_m(&self) -> f64 {
        let pole = match self.structure {
            BillboardStructureKind::WallMount => 0.0,
            BillboardStructureKind::Monopole | BillboardStructureKind::TwinPole => {
                self.pole_height_m
            }
        };
        pole + self.screen_height_m + 0.5
    }
}

pub fn parse_billboard_anim_config(anim: &Anim) -> BillboardAnimConfig {
    let mut cfg: BillboardAnimConfig = anim
        .data
        .get("billboard")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if cfg.pos_lnglat[0] == 0.0
        && cfg.pos_lnglat[1] == 0.0
        && let Some(&p) = anim.geom.first()
    {
        cfg.pos_lnglat = p;
    }
    cfg.sanitized()
}

pub fn write_billboard_anim_config(anim: &mut Anim, cfg: &BillboardAnimConfig) {
    let clean = cfg.sanitized();
    anim.anim_type = ANIM_TYPE_BILLBOARD.to_string();
    anim.marker_id = None;
    anim.geom = vec![clean.pos_lnglat];
    anim.replay = clean.is_continuous();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut() {
        obj.insert(
            "billboard".to_string(),
            serde_json::to_value(&clean).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
}

pub fn billboard_anim_position(anim: &Anim) -> [f64; 3] {
    parse_billboard_anim_config(anim).pos_lnglat
}

pub fn set_billboard_anim_position(anim: &mut Anim, lng: f64, lat: f64, alt: f64) {
    let mut cfg = parse_billboard_anim_config(anim);
    cfg.pos_lnglat = [lng, lat, alt];
    write_billboard_anim_config(anim, &cfg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_anim() -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Videotron 1".into(),
            anim_type: ANIM_TYPE_BILLBOARD.into(),
            sort_order: 1,
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
    fn billboard_config_roundtrips_and_distinguishes_image_vs_video_playability() {
        let mut anim = sample_anim();
        let img_id = Uuid::new_v4();
        let vid_id = Uuid::new_v4();

        let cfg_img = BillboardAnimConfig {
            media_mode: BillboardMediaMode::Image,
            image_asset_id: Some(img_id),
            video_asset_id: Some(vid_id),
            structure: BillboardStructureKind::TwinPole,
            screen_width_m: 12.0,
            screen_height_m: 6.75,
            pole_height_m: 6.5,
            double_sided: true,
            brightness: 1.6,
            pos_lnglat: [115.2167, -8.6705, 1.5],
            rot_deg: [0.0, 45.0, 0.0],
            scale: [1.5, 1.5, 1.5],
            ..Default::default()
        };
        write_billboard_anim_config(&mut anim, &cfg_img);
        let parsed = parse_billboard_anim_config(&anim);
        assert_eq!(parsed.media_mode, BillboardMediaMode::Image);
        assert!(!parsed.is_playable_video());
        assert!(!parsed.is_continuous());
        assert_eq!(anim.geom, vec![[115.2167, -8.6705, 1.5]]);

        let cfg_vid = BillboardAnimConfig {
            media_mode: BillboardMediaMode::Video,
            repeat: BillboardRepeat::Forever,
            play_audio: true,
            volume: 0.7,
            ..parsed
        };
        write_billboard_anim_config(&mut anim, &cfg_vid);
        let parsed_vid = parse_billboard_anim_config(&anim);
        assert!(parsed_vid.is_playable_video());
        assert!(parsed_vid.is_continuous());
        assert_eq!(parsed_vid.plays(), None);
        assert!(parsed_vid.play_audio);
    }
}
