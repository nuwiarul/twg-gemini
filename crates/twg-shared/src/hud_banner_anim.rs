use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::Anim;

pub const ANIM_TYPE_HUD_BANNER: &str = "hud_banner";

pub fn is_hud_banner_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_HUD_BANNER
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudBannerStyle {
    /// Dramatic chapter / stage title card (e.g. "TAHAP II — Eskalasi Massa").
    #[default]
    TitleCard,
    /// Broadcast-style lower-third bar at the bottom-left / bottom of the screen.
    LowerThird,
    /// Continuous horizontal scrolling ticker bar at the bottom of the screen.
    RunningText,
}

impl HudBannerStyle {
    pub const ALL: [Self; 3] = [Self::TitleCard, Self::LowerThird, Self::RunningText];

    pub fn label(self) -> &'static str {
        match self {
            Self::TitleCard => "Kartu Judul Babak",
            Self::LowerThird => "Lower-Third Informasi",
            Self::RunningText => "Running Text / Ticker",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::TitleCard => "BANNER",
            Self::LowerThird => "LOWER-3RD",
            Self::RunningText => "TICKER",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudBannerPosition {
    #[default]
    TopCenter,
    Center,
    BottomLeft,
    BottomCenter,
}

impl HudBannerPosition {
    pub const ALL: [Self; 4] = [
        Self::TopCenter,
        Self::Center,
        Self::BottomLeft,
        Self::BottomCenter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TopCenter => "Atas Tengah",
            Self::Center => "Tengah Layar",
            Self::BottomLeft => "Bawah Kiri",
            Self::BottomCenter => "Bawah Tengah",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudBannerTheme {
    #[default]
    CommandBlue,
    AlertRed,
    WarningAmber,
    SafeGreen,
    TacticalDark,
    Custom,
}

impl HudBannerTheme {
    pub const ALL: [Self; 6] = [
        Self::CommandBlue,
        Self::AlertRed,
        Self::WarningAmber,
        Self::SafeGreen,
        Self::TacticalDark,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CommandBlue => "Komando Biru",
            Self::AlertRed => "Siaga Merah",
            Self::WarningAmber => "Waspada Kuning",
            Self::SafeGreen => "Kondusif Hijau",
            Self::TacticalDark => "Taktis Gelap",
            Self::Custom => "Kustom",
        }
    }

    /// Returns `(accent_hex, bg_hex, text_hex)`.
    pub fn colors(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::CommandBlue => ("#38bdf8", "#0a1628", "#f8fafc"),
            Self::AlertRed => ("#ef4444", "#240a10", "#fef2f2"),
            Self::WarningAmber => ("#f59e0b", "#221706", "#fffbeb"),
            Self::SafeGreen => ("#10b981", "#071f18", "#ecfdf5"),
            Self::TacticalDark => ("#94a3b8", "#0f172a", "#f8fafc"),
            Self::Custom => ("#a855f7", "#140d24", "#f8fafc"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HudBannerConfig {
    pub style: HudBannerStyle,
    pub position: HudBannerPosition,
    pub theme: HudBannerTheme,
    /// Compact badge tag (e.g. `"TAHAP I"`, `"SITUASI LIVE"`).
    pub badge_text: String,
    /// Primary title text for `TitleCard` & `LowerThird`.
    pub title_text: String,
    /// Subtitle / time / location metadata for `TitleCard` & `LowerThird`.
    pub subtitle_text: String,
    /// Scrolling content for `RunningText` (can separate items with `"  •  "`).
    pub ticker_text: String,
    /// Horizontal scroll speed in logical pixels/sec for `RunningText`.
    pub ticker_speed_px: f32,
    /// Font size multiplier (`0.7..=1.8`).
    pub font_scale: f32,
    pub accent_color: String,
    pub bg_color: String,
    pub bg_opacity: f32,
    pub text_color: String,
    /// Display duration in seconds when `hold_until_stop == false`.
    pub duration_sec: f64,
    /// If `true`, stays visible on screen until the operator clicks Stop.
    pub hold_until_stop: bool,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
    pub delay_sec: f64,
}

impl Default for HudBannerConfig {
    fn default() -> Self {
        let (acc, bg, txt) = HudBannerTheme::CommandBlue.colors();
        Self {
            style: HudBannerStyle::TitleCard,
            position: HudBannerPosition::TopCenter,
            theme: HudBannerTheme::CommandBlue,
            badge_text: "TAHAP I".into(),
            title_text: "Eskalasi Massa & Penyekatan Simpang".into(),
            subtitle_text: "Pukul 14.30 WIB  •  Sektor Pengamanan Utama".into(),
            ticker_text: "SITUASI TERKINI: Pasukan PHH menempati garis penyekatan utama  •  Arus lalu lintas dari arah timur dialihkan menuju jalur alternatif  •  Unit kesehatan & pemadam siaga di sektor belakang".into(),
            ticker_speed_px: 95.0,
            font_scale: 1.0,
            accent_color: acc.into(),
            bg_color: bg.into(),
            bg_opacity: 0.90,
            text_color: txt.into(),
            duration_sec: 6.0,
            hold_until_stop: false,
            fade_in_sec: 0.45,
            fade_out_sec: 0.45,
            delay_sec: 0.0,
        }
    }
}

impl HudBannerConfig {
    pub fn apply_theme(&mut self, theme: HudBannerTheme) {
        self.theme = theme;
        if theme != HudBannerTheme::Custom {
            let (acc, bg, txt) = theme.colors();
            self.accent_color = acc.into();
            self.bg_color = bg.into();
            self.text_color = txt.into();
        }
    }

    pub fn apply_style_defaults(&mut self, style: HudBannerStyle) {
        self.style = style;
        match style {
            HudBannerStyle::TitleCard => {
                self.position = HudBannerPosition::TopCenter;
                self.hold_until_stop = false;
                if self.badge_text.trim().is_empty() || self.badge_text == "SITUASI LIVE" {
                    self.badge_text = "TAHAP I".into();
                }
            }
            HudBannerStyle::LowerThird => {
                self.position = HudBannerPosition::BottomLeft;
                self.hold_until_stop = false;
            }
            HudBannerStyle::RunningText => {
                self.position = HudBannerPosition::BottomCenter;
                self.hold_until_stop = true;
                if self.badge_text.trim().is_empty() || self.badge_text == "TAHAP I" {
                    self.badge_text = "SITUASI LIVE".into();
                }
            }
        }
    }

    pub fn sanitized(mut self) -> Self {
        let (acc, bg, txt) = self.theme.colors();
        if self.accent_color.trim().is_empty() {
            self.accent_color = acc.into();
        }
        if self.bg_color.trim().is_empty() {
            self.bg_color = bg.into();
        }
        if self.text_color.trim().is_empty() {
            self.text_color = txt.into();
        }
        self.ticker_speed_px = if self.ticker_speed_px.is_finite() {
            self.ticker_speed_px.clamp(25.0, 320.0)
        } else {
            95.0
        };
        self.font_scale = if self.font_scale.is_finite() {
            self.font_scale.clamp(0.7, 1.8)
        } else {
            1.0
        };
        self.bg_opacity = if self.bg_opacity.is_finite() {
            self.bg_opacity.clamp(0.25, 1.0)
        } else {
            0.90
        };
        self.duration_sec = if self.duration_sec.is_finite() {
            self.duration_sec.clamp(1.0, 600.0)
        } else {
            6.0
        };
        self.fade_in_sec = if self.fade_in_sec.is_finite() {
            self.fade_in_sec.clamp(0.0, 3.0)
        } else {
            0.45
        };
        self.fade_out_sec = if self.fade_out_sec.is_finite() {
            self.fade_out_sec.clamp(0.0, 3.0)
        } else {
            0.45
        };
        self.delay_sec = if self.delay_sec.is_finite() {
            self.delay_sec.clamp(0.0, 600.0)
        } else {
            0.0
        };
        self
    }

    pub fn is_playable(&self) -> bool {
        match self.style {
            HudBannerStyle::RunningText => !self.ticker_text.trim().is_empty(),
            HudBannerStyle::TitleCard | HudBannerStyle::LowerThird => {
                !self.title_text.trim().is_empty() || !self.subtitle_text.trim().is_empty()
            }
        }
    }

    pub fn finite_duration_sec(&self) -> f64 {
        if self.hold_until_stop {
            0.0
        } else {
            self.delay_sec + self.duration_sec
        }
    }

    pub fn evaluate_at(&self, elapsed_sec: f64) -> HudBannerEval {
        if !self.is_playable() {
            return HudBannerEval {
                visible: false,
                alpha: 0.0,
                enter_progress: 0.0,
                scroll_sec: 0.0,
                finished: true,
            };
        }
        let elapsed = if elapsed_sec.is_finite() {
            elapsed_sec.max(0.0)
        } else {
            0.0
        };
        if elapsed < self.delay_sec {
            return HudBannerEval {
                visible: false,
                alpha: 0.0,
                enter_progress: 0.0,
                scroll_sec: 0.0,
                finished: false,
            };
        }
        let local_t = elapsed - self.delay_sec;
        let enter = if self.fade_in_sec <= 1e-4 {
            1.0
        } else {
            let p = (local_t / self.fade_in_sec).clamp(0.0, 1.0);
            let inv = 1.0 - p;
            (1.0 - inv * inv * inv) as f32
        };

        let mut alpha = enter;
        let mut finished = false;
        if !self.hold_until_stop {
            if local_t >= self.duration_sec {
                finished = true;
                alpha = 0.0;
            } else if self.fade_out_sec > 1e-4 {
                let rem = self.duration_sec - local_t;
                if rem < self.fade_out_sec {
                    alpha = alpha.min((rem / self.fade_out_sec).clamp(0.0, 1.0) as f32);
                }
            }
        }

        HudBannerEval {
            visible: !finished && alpha > 0.01,
            alpha,
            enter_progress: enter,
            scroll_sec: local_t,
            finished,
        }
    }

    pub fn preview_eval(&self, wall_clock_sec: f64) -> HudBannerEval {
        HudBannerEval {
            visible: self.is_playable(),
            alpha: 1.0,
            enter_progress: 1.0,
            scroll_sec: wall_clock_sec.max(0.0),
            finished: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudBannerEval {
    pub visible: bool,
    pub alpha: f32,
    pub enter_progress: f32,
    pub scroll_sec: f64,
    pub finished: bool,
}

pub fn parse_hud_banner_config(anim: &Anim) -> HudBannerConfig {
    let raw = anim
        .data
        .as_object()
        .and_then(|o| o.get("hud_banner"))
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::from_value::<HudBannerConfig>(raw)
        .unwrap_or_default()
        .sanitized()
}

pub fn write_hud_banner_config(anim: &mut Anim, cfg: &HudBannerConfig) {
    let clean = cfg.clone().sanitized();
    let mut root = anim
        .data
        .as_object()
        .cloned()
        .unwrap_or_else(Map::new);
    if let Ok(v) = serde_json::to_value(&clean) {
        root.insert("hud_banner".to_string(), v);
    }
    anim.data = Value::Object(root);
    anim.duration_sec = if clean.hold_until_stop {
        0.0
    } else {
        clean.finite_duration_sec().max(0.5)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn dummy_banner_anim() -> Anim {
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Tahap II".into(),
            anim_type: ANIM_TYPE_HUD_BANNER.into(),
            sort_order: 1,
            speed: 1.0,
            duration_sec: 6.0,
            replay: false,
            autoplay: false,
            geom: vec![],
            data: Value::Object(Map::new()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn hud_banner_roundtrips_and_handles_timed_vs_hold_until_stop() {
        let mut anim = dummy_banner_anim();
        let mut cfg = HudBannerConfig::default();
        cfg.apply_theme(HudBannerTheme::AlertRed);
        cfg.delay_sec = 0.5;
        cfg.duration_sec = 5.0;
        cfg.fade_in_sec = 0.5;
        cfg.fade_out_sec = 0.5;
        cfg.hold_until_stop = false;

        write_hud_banner_config(&mut anim, &cfg);
        let parsed = parse_hud_banner_config(&anim);
        assert_eq!(parsed.theme, HudBannerTheme::AlertRed);
        assert_eq!(parsed.accent_color, "#ef4444");
        assert!((parsed.finite_duration_sec() - 5.5).abs() < 1e-6);

        assert!(!parsed.evaluate_at(0.25).visible);
        let mid = parsed.evaluate_at(2.5);
        assert!(mid.visible);
        assert!((mid.alpha - 1.0).abs() < 1e-4);
        let fade_out = parsed.evaluate_at(5.25);
        assert!(fade_out.visible);
        assert!(fade_out.alpha > 0.3 && fade_out.alpha < 0.7);
        assert!(parsed.evaluate_at(5.6).finished);

        // RunningText defaults to hold_until_stop = true
        let mut ticker = HudBannerConfig::default();
        ticker.apply_style_defaults(HudBannerStyle::RunningText);
        assert!(ticker.hold_until_stop);
        assert_eq!(ticker.finite_duration_sec(), 0.0);
        let long_run = ticker.evaluate_at(120.0);
        assert!(long_run.visible);
        assert!(!long_run.finished);
    }
}
