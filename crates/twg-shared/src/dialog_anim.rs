use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{Anim, Marker};

pub const ANIM_TYPE_DIALOG: &str = "dialog";

pub fn is_dialog_anim(anim: &Anim) -> bool {
    anim.anim_type == ANIM_TYPE_DIALOG
}

/// Visual reveal style for text inside a 3D speech bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogTextEffect {
    /// Characters appear progressively like a game dialog box, with smooth fade-in/out.
    #[default]
    Typewriter,
    /// Entire sentence fades in and out smoothly at once.
    FadeIn,
}

impl DialogTextEffect {
    pub const ALL: &'static [Self] = &[Self::Typewriter, Self::FadeIn];

    pub fn label(self) -> &'static str {
        match self {
            Self::Typewriter => "Efek Mengetik (Typewriter)",
            Self::FadeIn => "Muncul Langsung (Fade)",
        }
    }
}

/// A single spoken line inside a `dialog` animation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogLine {
    /// Speaking 3D Marker UUID.
    #[serde(default)]
    pub marker_id: Option<Uuid>,
    /// Optional display name override (if empty, uses the bound marker's name).
    #[serde(default)]
    pub speaker_override: String,
    /// Spoken text rendered inside the speech bubble (supports multi-line wrapping).
    #[serde(default)]
    pub text: String,
    /// How long the bubble stays visible for this line (seconds).
    #[serde(default = "default_line_duration_sec")]
    pub duration_sec: f64,
    /// Silent gap after this line finishes before the next speaker's bubble appears (seconds).
    #[serde(default = "default_pause_after_sec")]
    pub pause_after_sec: f64,
    /// Optional hex color override for this line's bubble background (empty = use global).
    #[serde(default)]
    pub bg_color_override: String,
    /// Optional hex color override for this line's text (empty = use global).
    #[serde(default)]
    pub text_color_override: String,
    /// Optional hex color override for this line's border / speaker header (empty = use global).
    #[serde(default)]
    pub border_color_override: String,
}

fn default_line_duration_sec() -> f64 {
    3.5
}

fn default_pause_after_sec() -> f64 {
    0.6
}

impl Default for DialogLine {
    fn default() -> Self {
        Self {
            marker_id: None,
            speaker_override: String::new(),
            text: String::new(),
            duration_sec: default_line_duration_sec(),
            pause_after_sec: default_pause_after_sec(),
            bg_color_override: String::new(),
            text_color_override: String::new(),
            border_color_override: String::new(),
        }
    }
}

impl DialogLine {
    pub fn sanitized(mut self) -> Self {
        self.speaker_override = self.speaker_override.trim().to_string();
        if !self.duration_sec.is_finite() {
            self.duration_sec = default_line_duration_sec();
        }
        self.duration_sec = self.duration_sec.clamp(0.3, 120.0);
        if !self.pause_after_sec.is_finite() {
            self.pause_after_sec = default_pause_after_sec();
        }
        self.pause_after_sec = self.pause_after_sec.clamp(0.0, 60.0);
        self.bg_color_override = self.bg_color_override.trim().to_string();
        self.text_color_override = self.text_color_override.trim().to_string();
        self.border_color_override = self.border_color_override.trim().to_string();
        self
    }

    pub fn is_playable(&self) -> bool {
        self.marker_id.is_some() && !self.text.trim().is_empty()
    }

    /// Estimates a comfortable reading duration (seconds) from text length.
    pub fn estimated_reading_duration_sec(text: &str) -> f64 {
        let chars = text.trim().chars().count();
        if chars == 0 {
            return 2.5;
        }
        let raw = 1.4 + (chars as f64 / 15.0);
        ((raw * 2.0).round() / 2.0).clamp(1.5, 25.0)
    }
}

/// Complete configuration for a 3D Speech Bubble Dialog animation (`anim_type = "dialog"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogAnimConfig {
    #[serde(default)]
    pub lines: Vec<DialogLine>,
    /// Initial delay before the first bubble appears (seconds).
    #[serde(default)]
    pub delay_sec: f64,
    /// Font size in logical pixels (`10.0..=36.0`).
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// Maximum bubble text wrap width in logical pixels (`160.0..=600.0`).
    #[serde(default = "default_max_width_px")]
    pub max_width_px: f32,
    #[serde(default = "default_text_color")]
    pub text_color: String,
    #[serde(default = "default_bg_color")]
    pub bg_color: String,
    /// Bubble background opacity (`0.2..=1.0`).
    #[serde(default = "default_bg_opacity")]
    pub bg_opacity: f32,
    #[serde(default = "default_border_color")]
    pub border_color: String,
    /// Whether to display the speaker's name header inside the speech bubble.
    #[serde(default = "default_true")]
    pub show_speaker_name: bool,
    /// Vertical clearance (meters) above the speaking marker's head AABB.
    #[serde(default = "default_height_offset_m")]
    pub height_offset_m: f64,
    #[serde(default)]
    pub text_effect: DialogTextEffect,
    /// Target characters per second when `text_effect == DialogTextEffect::Typewriter`.
    #[serde(default = "default_typewriter_cps")]
    pub typewriter_cps: f32,
}

fn default_font_size() -> f32 {
    14.0
}
fn default_max_width_px() -> f32 {
    280.0
}
fn default_text_color() -> String {
    "#F8FAFC".to_string()
}
fn default_bg_color() -> String {
    "#0F172A".to_string()
}
fn default_bg_opacity() -> f32 {
    0.92
}
fn default_border_color() -> String {
    "#38BDF8".to_string()
}
fn default_true() -> bool {
    true
}
fn default_height_offset_m() -> f64 {
    0.6
}
fn default_typewriter_cps() -> f32 {
    42.0
}

impl Default for DialogAnimConfig {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            delay_sec: 0.0,
            font_size: default_font_size(),
            max_width_px: default_max_width_px(),
            text_color: default_text_color(),
            bg_color: default_bg_color(),
            bg_opacity: default_bg_opacity(),
            border_color: default_border_color(),
            show_speaker_name: true,
            height_offset_m: default_height_offset_m(),
            text_effect: DialogTextEffect::Typewriter,
            typewriter_cps: default_typewriter_cps(),
        }
    }
}

/// Deterministic render snapshot of the active speech bubble at a given playback timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveDialogSample {
    pub line_index: usize,
    pub total_lines: usize,
    pub marker_id: Uuid,
    pub speaker_override: String,
    pub visible_text: String,
    pub full_text: String,
    /// Overall fade alpha (`0.0..=1.0`) for smooth pop-in/fade-out.
    pub alpha: f32,
    pub bg_color: String,
    pub text_color: String,
    pub border_color: String,
}

impl DialogAnimConfig {
    pub fn sanitized(mut self) -> Self {
        self.lines = self.lines.into_iter().map(DialogLine::sanitized).collect();
        if !self.delay_sec.is_finite() {
            self.delay_sec = 0.0;
        }
        self.delay_sec = self.delay_sec.clamp(0.0, 3600.0);
        if !self.font_size.is_finite() {
            self.font_size = default_font_size();
        }
        self.font_size = self.font_size.clamp(10.0, 36.0);
        if !self.max_width_px.is_finite() {
            self.max_width_px = default_max_width_px();
        }
        self.max_width_px = self.max_width_px.clamp(160.0, 600.0);
        if !self.bg_opacity.is_finite() {
            self.bg_opacity = default_bg_opacity();
        }
        self.bg_opacity = self.bg_opacity.clamp(0.2, 1.0);
        if !self.height_offset_m.is_finite() {
            self.height_offset_m = default_height_offset_m();
        }
        self.height_offset_m = self.height_offset_m.clamp(0.0, 30.0);
        if !self.typewriter_cps.is_finite() {
            self.typewriter_cps = default_typewriter_cps();
        }
        self.typewriter_cps = self.typewriter_cps.clamp(8.0, 120.0);
        if self.text_color.trim().is_empty() {
            self.text_color = default_text_color();
        }
        if self.bg_color.trim().is_empty() {
            self.bg_color = default_bg_color();
        }
        if self.border_color.trim().is_empty() {
            self.border_color = default_border_color();
        }
        self
    }

    /// Returns all unique marker IDs referenced across dialog lines (in order of first appearance).
    pub fn unique_marker_ids(&self) -> Vec<Uuid> {
        let mut out = Vec::new();
        for l in &self.lines {
            if let Some(mid) = l.marker_id
                && !out.contains(&mid)
            {
                out.push(mid);
            }
        }
        out
    }

    pub fn playable_lines(&self) -> Vec<(usize, &DialogLine)> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.is_playable())
            .collect()
    }

    pub fn has_playable_setup(&self, markers: &[Marker]) -> bool {
        self.lines.iter().any(|l| {
            l.is_playable()
                && l.marker_id
                    .is_some_and(|mid| markers.iter().any(|m| m.id == mid))
        })
    }

    /// Total duration of one complete pass of all playable dialog lines (seconds).
    /// When `include_final_pause` is `true` (e.g. when `anim.replay == true`), keeps the
    /// final line's `pause_after_sec` so looping has a natural breathing gap.
    pub fn total_duration_sec(&self, include_final_pause: bool) -> f64 {
        let playable = self.playable_lines();
        if playable.is_empty() {
            return 0.0;
        }
        let mut total = self.delay_sec;
        for (i, (_, line)) in playable.iter().enumerate() {
            total += line.duration_sec;
            if include_final_pause || i + 1 < playable.len() {
                total += line.pause_after_sec;
            }
        }
        total.max(0.3)
    }

    /// Evaluates which dialog line (if any) should be rendered at `elapsed_sec`.
    /// Returns `None` during initial `delay_sec`, during silent `pause_after_sec` gaps,
    /// or after the final line finishes.
    pub fn evaluate_at(&self, elapsed_sec: f64) -> Option<ActiveDialogSample> {
        let playable = self.playable_lines();
        if playable.is_empty() || elapsed_sec < self.delay_sec {
            return None;
        }
        let mut cursor = self.delay_sec;
        let total_lines = playable.len();
        for (pos, (_, line)) in playable.iter().enumerate() {
            let line_end = cursor + line.duration_sec;
            if elapsed_sec <= line_end {
                let local_t = (elapsed_sec - cursor).clamp(0.0, line.duration_sec);
                let marker_id = line.marker_id?;
                let fade_in = (line.duration_sec * 0.12).clamp(0.08, 0.22);
                let fade_out = (line.duration_sec * 0.14).clamp(0.10, 0.28);
                let alpha = if local_t < fade_in {
                    (local_t / fade_in) as f32
                } else if local_t > line.duration_sec - fade_out {
                    ((line.duration_sec - local_t) / fade_out).max(0.0) as f32
                } else {
                    1.0
                }
                .clamp(0.0, 1.0);

                let full_text = line.text.trim().to_string();
                let visible_text = match self.text_effect {
                    DialogTextEffect::FadeIn => full_text.clone(),
                    DialogTextEffect::Typewriter => {
                        let total_chars = full_text.chars().count();
                        if total_chars == 0 {
                            String::new()
                        } else {
                            // Guarantee full text finishes typing within <= 68% of line duration
                            let max_type_time = (line.duration_sec * 0.68).max(0.15);
                            let natural_type_time =
                                (total_chars as f64) / (self.typewriter_cps.max(8.0) as f64);
                            let eff_type_time = natural_type_time.min(max_type_time).max(0.05);
                            let frac = (local_t / eff_type_time).clamp(0.0, 1.0);
                            let count = ((frac * total_chars as f64).ceil() as usize)
                                .clamp(1, total_chars);
                            full_text.chars().take(count).collect()
                        }
                    }
                };

                let bg_color = if line.bg_color_override.is_empty() {
                    self.bg_color.clone()
                } else {
                    line.bg_color_override.clone()
                };
                let text_color = if line.text_color_override.is_empty() {
                    self.text_color.clone()
                } else {
                    line.text_color_override.clone()
                };
                let border_color = if line.border_color_override.is_empty() {
                    self.border_color.clone()
                } else {
                    line.border_color_override.clone()
                };

                return Some(ActiveDialogSample {
                    line_index: pos,
                    total_lines,
                    marker_id,
                    speaker_override: line.speaker_override.clone(),
                    visible_text,
                    full_text,
                    alpha,
                    bg_color,
                    text_color,
                    border_color,
                });
            }
            let gap_end = line_end + line.pause_after_sec;
            if elapsed_sec < gap_end {
                return None;
            }
            cursor = gap_end;
        }
        None
    }

    /// Builds a full-opacity preview sample for a specific raw line index (used in Plot Editor).
    pub fn preview_sample_for_line(&self, raw_idx: usize) -> Option<ActiveDialogSample> {
        let line = self.lines.get(raw_idx)?;
        let marker_id = line.marker_id?;
        let full_text = if line.text.trim().is_empty() {
            "(Ketik isi dialog di Inspector...)".to_string()
        } else {
            line.text.trim().to_string()
        };
        Some(ActiveDialogSample {
            line_index: raw_idx,
            total_lines: self.lines.len().max(1),
            marker_id,
            speaker_override: line.speaker_override.clone(),
            visible_text: full_text.clone(),
            full_text,
            alpha: 1.0,
            bg_color: if line.bg_color_override.is_empty() {
                self.bg_color.clone()
            } else {
                line.bg_color_override.clone()
            },
            text_color: if line.text_color_override.is_empty() {
                self.text_color.clone()
            } else {
                line.text_color_override.clone()
            },
            border_color: if line.border_color_override.is_empty() {
                self.border_color.clone()
            } else {
                line.border_color_override.clone()
            },
        })
    }
}

pub fn parse_dialog_anim_config(anim: &Anim) -> DialogAnimConfig {
    anim.data
        .get("dialog")
        .and_then(|v| serde_json::from_value::<DialogAnimConfig>(v.clone()).ok())
        .map(DialogAnimConfig::sanitized)
        .unwrap_or_default()
}

pub fn write_dialog_anim_config(anim: &mut Anim, cfg: &DialogAnimConfig) {
    let clean = cfg.clone().sanitized();
    if !anim.data.is_object() {
        anim.data = serde_json::json!({});
    }
    if let Some(obj) = anim.data.as_object_mut()
        && let Ok(val) = serde_json::to_value(&clean)
    {
        obj.insert("dialog".to_string(), val);
    }
    anim.marker_id = clean.unique_marker_ids().first().copied();
    anim.duration_sec = clean.total_duration_sec(anim.replay);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn dialog_config_roundtrips_and_switches_speakers_with_pause_gaps() {
        let now = Utc::now();
        let m1 = Uuid::new_v4();
        let m2 = Uuid::new_v4();
        let mut anim = Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: "Negosiasi 1".into(),
            anim_type: ANIM_TYPE_DIALOG.into(),
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

        let cfg = DialogAnimConfig {
            delay_sec: 0.5,
            font_size: 16.0,
            max_width_px: 320.0,
            lines: vec![
                DialogLine {
                    marker_id: Some(m1),
                    speaker_override: "Kapolres".into(),
                    text: "Mohon tetap tertib!".into(),
                    duration_sec: 3.0,
                    pause_after_sec: 1.0,
                    ..Default::default()
                },
                DialogLine {
                    marker_id: Some(m2),
                    speaker_override: "Korlap Massa".into(),
                    text: "Siap Pak, kami tertib.".into(),
                    duration_sec: 2.5,
                    pause_after_sec: 0.5,
                    bg_color_override: "#450A0A".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        write_dialog_anim_config(&mut anim, &cfg);
        assert_eq!(anim.marker_id, Some(m1));
        // Non-repeating total: delay(0.5) + line1(3.0) + pause1(1.0) + line2(2.5) = 7.0
        assert!((anim.duration_sec - 7.0).abs() < 1e-6);

        let parsed = parse_dialog_anim_config(&anim);
        assert_eq!(parsed, cfg);
        assert_eq!(parsed.unique_marker_ids(), vec![m1, m2]);

        // During initial delay -> None
        assert!(parsed.evaluate_at(0.2).is_none());
        // Mid line 1 -> speaker m1, full alpha, typewriter progressing
        let s1 = parsed.evaluate_at(2.0).expect("line 1 active");
        assert_eq!(s1.marker_id, m1);
        assert_eq!(s1.line_index, 0);
        assert_eq!(s1.visible_text, "Mohon tetap tertib!");
        assert!((s1.alpha - 1.0).abs() < 1e-4);
        // During pause gap (3.5..4.5) -> None
        assert!(parsed.evaluate_at(4.0).is_none());
        // During line 2 (4.5..7.0) -> speaker m2 with custom bg_color_override
        let s2 = parsed.evaluate_at(5.5).expect("line 2 active");
        assert_eq!(s2.marker_id, m2);
        assert_eq!(s2.line_index, 1);
        assert_eq!(s2.bg_color, "#450A0A");
        // After end -> None
        assert!(parsed.evaluate_at(7.2).is_none());
    }
}
