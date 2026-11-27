use bevy_egui::egui;
use twg_shared::{
    api::CreateAnimRequest,
    hud_banner_anim::{
        ANIM_TYPE_HUD_BANNER, HudBannerConfig, HudBannerPosition, HudBannerStyle, HudBannerTheme,
        is_hud_banner_anim, parse_hud_banner_config, write_hud_banner_config,
    },
    models::Anim,
};
use uuid::Uuid;

use crate::{
    net::client::{ApiClientBridge, ApiCommand},
    scene::markers::current_auth,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity},
    ui::spinner::{SpinnerConfig, numeric_spinner_f32_w, numeric_spinner_w},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudBannerPlayAction {
    Play,
    Pause,
    Resume,
    Stop,
}

pub fn anim_summary(anim: &Anim) -> String {
    let cfg = parse_hud_banner_config(anim);
    let dur = if cfg.hold_until_stop {
        "Tampil s/d Stop".to_string()
    } else {
        format!("{:.1} dtk", cfg.duration_sec)
    };
    let preview_txt = match cfg.style {
        HudBannerStyle::RunningText => cfg.ticker_text.trim(),
        _ => cfg.title_text.trim(),
    };
    let clipped: String = preview_txt.chars().take(26).collect();
    if clipped.is_empty() {
        format!("{}  •  {dur}", cfg.style.label())
    } else {
        format!("{}  •  \"{clipped}\"  •  {dur}", cfg.style.short_tag())
    }
}

pub fn create_hud_banner_anim(state: &mut AppState, api: &ApiClientBridge) {
    let Some(sk) = state.active_skenario.clone() else {
        return;
    };
    let style = state.new_hud_banner_style;
    let cnt = state
        .anims
        .iter()
        .filter(|a| is_hud_banner_anim(a))
        .count()
        + 1;
    let name = if state.new_hud_banner_name.trim().is_empty() {
        format!("{} {cnt}", style.label())
    } else {
        state.new_hud_banner_name.trim().to_string()
    };

    let mut cfg = HudBannerConfig::default();
    cfg.apply_style_defaults(style);
    if style == HudBannerStyle::TitleCard {
        cfg.badge_text = format!("TAHAP {cnt}");
    }

    let sort_order = (state.anims.len() + 1) as i32;
    let now = chrono::Utc::now();
    let mut anim = Anim {
        id: Uuid::new_v4(),
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: None,
        name: name.clone(),
        anim_type: ANIM_TYPE_HUD_BANNER.to_string(),
        sort_order,
        speed: 1.0,
        duration_sec: 6.0,
        replay: false,
        autoplay: false,
        geom: vec![],
        data: serde_json::json!({}),
        created_by: state.current_user_id(),
        allowed_user_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    write_hud_banner_config(&mut anim, &cfg);

    if state.offline_preview {
        let aid = anim.id;
        state.selected = SelectedEntity::Anim(aid);
        state.anim_edit = Some(anim.clone());
        state.marker_edit = None;
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.anims.push(anim);
        state.sort_anims_by_order();
        state.status_banner = Some(format!(
            "{} '{name}' dibuat — atur teks, tema & posisi di Inspector!",
            style.label()
        ));
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateAnim {
            auth,
            req: CreateAnimRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: None,
                name,
                anim_type: ANIM_TYPE_HUD_BANNER.to_string(),
                sort_order,
                speed: 1.0,
                duration_sec: anim.duration_sec,
                replay: false,
                autoplay: false,
                geom: vec![],
                data: anim.data,
                allowed_user_ids: Vec::new(),
            },
        });
    }

    state.new_hud_banner_name = format!("Banner Tahap {}", cnt + 1);
}

pub fn draw_create(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    editable: bool,
) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(18, 28, 44))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(49, 68, 98)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Banner Tahap / Lower-Third / Running Text")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(165, 180, 252)),
            );
            ui.label(
                egui::RichText::new(
                    "Kartu judul babak atau pita informasi berjalan di Layar Besar; dikontrol dari Panel Animasi.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Model:").size(10.5));
                egui::ComboBox::from_id_salt("new_hud_banner_style_cb")
                    .width(ui.available_width().max(120.0))
                    .selected_text(state.new_hud_banner_style.label())
                    .show_ui(ui, |ui| {
                        for st in HudBannerStyle::ALL {
                            if ui
                                .selectable_value(&mut state.new_hud_banner_style, st, st.label())
                                .changed()
                            {
                                let next_n = state
                                    .anims
                                    .iter()
                                    .filter(|a| is_hud_banner_anim(a))
                                    .count()
                                    + 1;
                                state.new_hud_banner_name = format!("{} {next_n}", st.label());
                            }
                        }
                    });
            });
            ui.add_space(3.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_hud_banner_name)
                        .desired_width(f32::INFINITY)
                        .hint_text("Tahap II — Penyekatan"),
                );
            });
            ui.add_space(5.0);

            let btn = egui::Button::new(
                egui::RichText::new("+ Tambah Banner / Running Text")
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(79, 70, 229));

            if ui
                .add_enabled(editable, btn)
                .on_hover_text("Buat kartu judul babak, lower-third, atau running text ticker")
                .clicked()
            {
                create_hud_banner_anim(state, api);
            }
        });
}

fn color_edit_hex(ui: &mut egui::Ui, label: &str, hex: &mut String, editable: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(10.5));
        let clean = hex.trim().trim_start_matches('#');
        let mut srgb = [56u8, 189, 248];
        if clean.len() == 6
            && let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&clean[0..2], 16),
                u8::from_str_radix(&clean[2..4], 16),
                u8::from_str_radix(&clean[4..6], 16),
            )
        {
            srgb = [r, g, b];
        }
        let mut col = egui::Color32::from_rgb(srgb[0], srgb[1], srgb[2]);
        ui.add_enabled_ui(editable, |ui| {
            if ui.color_edit_button_srgba(&mut col).changed() {
                *hex = format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b());
                changed = true;
            }
        });
        if ui
            .add_enabled(
                editable,
                egui::TextEdit::singleline(hex).desired_width(68.0),
            )
            .changed()
        {
            changed = true;
        }
    });
    changed
}

pub fn draw_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    anim: &mut Anim,
    editable: bool,
) {
    let mut cfg = parse_hud_banner_config(anim);
    let mut dirty = false;

    egui::CollapsingHeader::new(
        egui::RichText::new("HUD Banner / Running Text — Konten")
            .size(12.0)
            .strong()
            .color(egui::Color32::from_rgb(165, 180, 252)),
    )
    .id_salt(("hud_banner_insp_content", anim.id))
    .default_open(true)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Nama Animasi:").size(11.0));
            ui.add_enabled(
                editable,
                egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY),
            );
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Model HUD:").size(11.0));
            egui::ComboBox::from_id_salt(("hud_banner_style_cb", anim.id))
                .width(ui.available_width().max(130.0))
                .selected_text(cfg.style.label())
                .show_ui(ui, |ui| {
                    for st in HudBannerStyle::ALL {
                        let sel = cfg.style == st;
                        if ui.selectable_label(sel, st.label()).clicked() && !sel {
                            cfg.apply_style_defaults(st);
                            dirty = true;
                        }
                    }
                });
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Badge / Label:").size(10.5));
            if ui
                .add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut cfg.badge_text)
                        .desired_width(f32::INFINITY)
                        .hint_text("TAHAP II / SITUASI LIVE"),
                )
                .changed()
            {
                dirty = true;
            }
        });

        if cfg.style == HudBannerStyle::RunningText {
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new("Teks Running Text / Ticker (pisah item dengan  •  ):")
                    .size(10.5)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
            );
            if ui
                .add_enabled(
                    editable,
                    egui::TextEdit::multiline(&mut cfg.ticker_text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                dirty = true;
            }
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Kecepatan Geser:").size(10.5));
                ui.add_enabled_ui(editable, |ui| {
                    if numeric_spinner_f32_w(
                        ui,
                        ("hb_tspd", anim.id),
                        &mut cfg.ticker_speed_px,
                        98.0,
                        SpinnerConfig {
                            min: 25.0,
                            max: 320.0,
                            step: 10.0,
                            drag_speed: 1.0,
                            decimals: 0,
                            suffix: " px/s",
                            ..Default::default()
                        },
                    ) {
                        dirty = true;
                    }
                });
            });
        } else {
            ui.add_space(3.0);
            ui.label(egui::RichText::new("Judul Utama:").size(10.5));
            if ui
                .add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut cfg.title_text)
                        .desired_width(f32::INFINITY)
                        .hint_text("Eskalasi Massa & Penyekatan Simpang"),
                )
                .changed()
            {
                dirty = true;
            }
            ui.add_space(3.0);
            ui.label(egui::RichText::new("Subjudul / Waktu / Lokasi:").size(10.5));
            if ui
                .add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut cfg.subtitle_text)
                        .desired_width(f32::INFINITY)
                        .hint_text("Pukul 14.30 WIB • Sektor Timur"),
                )
                .changed()
            {
                dirty = true;
            }
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Posisi Layar:").size(10.5));
                egui::ComboBox::from_id_salt(("hud_banner_pos_cb", anim.id))
                    .width(ui.available_width().max(120.0))
                    .selected_text(cfg.position.label())
                    .show_ui(ui, |ui| {
                        for pos in HudBannerPosition::ALL {
                            if ui
                                .selectable_value(&mut cfg.position, pos, pos.label())
                                .changed()
                            {
                                dirty = true;
                            }
                        }
                    });
            });
        }
    });

    ui.add_space(6.0);

    egui::CollapsingHeader::new(
        egui::RichText::new("Tema Warna & Durasi Kontrol Operator")
            .size(12.0)
            .strong()
            .color(egui::Color32::from_rgb(186, 230, 253)),
    )
    .id_salt(("hud_banner_insp_style", anim.id))
    .default_open(true)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Tema Warna:").size(10.5));
            egui::ComboBox::from_id_salt(("hud_banner_theme_cb", anim.id))
                .width(ui.available_width().max(120.0))
                .selected_text(cfg.theme.label())
                .show_ui(ui, |ui| {
                    for th in HudBannerTheme::ALL {
                        let sel = cfg.theme == th;
                        if ui.selectable_label(sel, th.label()).clicked() && !sel {
                            cfg.apply_theme(th);
                            dirty = true;
                        }
                    }
                });
        });
        ui.add_space(3.0);
        if color_edit_hex(ui, "Warna Aksen:", &mut cfg.accent_color, editable) {
            cfg.theme = HudBannerTheme::Custom;
            dirty = true;
        }
        if color_edit_hex(ui, "Warna Latar:", &mut cfg.bg_color, editable) {
            cfg.theme = HudBannerTheme::Custom;
            dirty = true;
        }
        if color_edit_hex(ui, "Warna Teks:", &mut cfg.text_color, editable) {
            cfg.theme = HudBannerTheme::Custom;
            dirty = true;
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Skala Font:").size(10.5));
            if ui
                .add_enabled(
                    editable,
                    egui::Slider::new(&mut cfg.font_scale, 0.7..=1.8).fixed_decimals(2),
                )
                .changed()
            {
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Opasitas Latar:").size(10.5));
            if ui
                .add_enabled(
                    editable,
                    egui::Slider::new(&mut cfg.bg_opacity, 0.25..=1.0).fixed_decimals(2),
                )
                .changed()
            {
                dirty = true;
            }
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        if ui
            .add_enabled(
                editable,
                egui::Checkbox::new(
                    &mut cfg.hold_until_stop,
                    "Tampil Terus Sampai Operator Klik Stop",
                ),
            )
            .changed()
        {
            dirty = true;
        }

        if !cfg.hold_until_stop {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Durasi Tampil:").size(10.5));
                ui.add_enabled_ui(editable, |ui| {
                    if numeric_spinner_w(
                        ui,
                        ("hb_dur", anim.id),
                        &mut cfg.duration_sec,
                        92.0,
                        SpinnerConfig {
                            min: 1.0,
                            max: 600.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " dtk",
                            ..Default::default()
                        },
                    ) {
                        dirty = true;
                    }
                });
            });
            if ui
                .add_enabled(editable, egui::Checkbox::new(&mut anim.replay, "Ulangi (Loop)"))
                .changed()
            {
                dirty = true;
            }
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Jeda Awal (Delay):").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_w(
                    ui,
                    ("hb_delay", anim.id),
                    &mut cfg.delay_sec,
                    92.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 600.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " dtk",
                        ..Default::default()
                    },
                ) {
                    dirty = true;
                }
            });
        });
    });

    if dirty {
        write_hud_banner_config(anim, &cfg);
        if let Some(live) = state.anims.iter_mut().find(|a| a.id == anim.id) {
            live.name = anim.name.clone();
            live.replay = anim.replay;
            live.duration_sec = anim.duration_sec;
            live.data = anim.data.clone();
        }
        state.dirty_anim_ids.insert(anim.id);
    }
}

/// 100% uniform Play Panel card for `hud_banner` (Play / Pause / Stop, synced to Layar Besar).
pub fn draw_play_card(
    ui: &mut egui::Ui,
    playback: &PlaybackState,
    anim: &Anim,
    display_idx: usize,
) -> Option<HudBannerPlayAction> {
    let mut action = None;
    let cfg = parse_hud_banner_config(anim);
    let can_play = cfg.is_playable();
    let status = playback.track_status(anim.id);
    let is_playing = status == AnimRunStatus::Playing;
    let is_paused = status == AnimRunStatus::Paused;
    let is_finished = status == AnimRunStatus::Finished;

    let elapsed = playback
        .tracks
        .get(&anim.id)
        .map(|t| t.distance_m)
        .unwrap_or(0.0);
    let total_dur = cfg.finite_duration_sec();
    let progress = if !cfg.hold_until_stop && total_dur > 1e-4 {
        (elapsed / total_dur).clamp(0.0, 1.0) as f32
    } else if is_playing || is_finished {
        1.0
    } else {
        0.0
    };

    let (status_label, status_bg, status_fg) = if is_playing {
        (
            "ON AIR",
            egui::Color32::from_rgb(6, 95, 70),
            egui::Color32::from_rgb(167, 243, 208),
        )
    } else if is_paused {
        (
            "PAUSED",
            egui::Color32::from_rgb(120, 53, 15),
            egui::Color32::from_rgb(253, 230, 138),
        )
    } else if is_finished {
        (
            "SELESAI",
            egui::Color32::from_rgb(30, 58, 138),
            egui::Color32::from_rgb(191, 219, 254),
        )
    } else {
        (
            "SIAP",
            egui::Color32::from_rgb(30, 41, 59),
            egui::Color32::from_rgb(148, 163, 184),
        )
    };

    let border_col = if is_playing {
        egui::Color32::from_rgb(129, 140, 248)
    } else if is_paused {
        egui::Color32::from_rgb(245, 158, 11)
    } else {
        egui::Color32::from_rgb(38, 52, 74)
    };

    egui::Frame::new()
        .fill(if is_playing {
            egui::Color32::from_rgb(22, 26, 48)
        } else {
            egui::Color32::from_rgb(18, 27, 42)
        })
        .stroke(egui::Stroke::new(if is_playing { 1.4 } else { 1.0 }, border_col))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // Row 1: #N + [BANNER/TICKER] + Name + Status Pill
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(
                    egui::RichText::new(format!("#{display_idx}"))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(79, 70, 229))
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(5, 1))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(cfg.style.short_tag())
                                .size(9.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                    });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&anim.name)
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(status_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(status_label)
                                    .size(9.0)
                                    .strong()
                                    .color(status_fg),
                            );
                        });
                });
            });

            ui.add_space(3.0);
            let headline = match cfg.style {
                HudBannerStyle::RunningText => cfg.ticker_text.trim(),
                _ => cfg.title_text.trim(),
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(if headline.is_empty() {
                        cfg.style.label()
                    } else {
                        headline
                    })
                    .size(10.5)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                )
                .truncate(),
            );

            let line2 = if is_playing || is_paused {
                if cfg.hold_until_stop {
                    format!("Tampil di layar ({:.1} dtk) — klik Stop untuk tutup", elapsed)
                } else {
                    format!("{:.1} / {:.1} dtk", elapsed.min(total_dur), total_dur)
                }
            } else if cfg.hold_until_stop {
                format!("{}  •  Manual (Tampil s/d Stop)", cfg.style.label())
            } else {
                format!("{}  •  Durasi {:.1} dtk", cfg.style.label(), total_dur)
            };
            ui.label(
                egui::RichText::new(line2)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );

            ui.add_space(4.0);
            super::play_panel::draw_card_progress_bar(ui, status, progress);
            ui.add_space(5.0);

            if is_playing || is_paused {
                ui.columns(2, |cols| {
                    let toggle_lbl = if is_playing { "Pause" } else { "Lanjut" };
                    let toggle_btn = egui::Button::new(
                        egui::RichText::new(toggle_lbl)
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if is_playing {
                        egui::Color32::from_rgb(217, 119, 6)
                    } else {
                        egui::Color32::from_rgb(5, 150, 105)
                    });
                    if cols[0]
                        .add_sized([cols[0].available_width(), 24.0], toggle_btn)
                        .clicked()
                    {
                        action = Some(if is_playing {
                            HudBannerPlayAction::Pause
                        } else {
                            HudBannerPlayAction::Resume
                        });
                    }

                    let stop_btn = egui::Button::new(
                        egui::RichText::new("Tutup / Stop")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(185, 28, 28));
                    if cols[1]
                        .add_sized([cols[1].available_width(), 24.0], stop_btn)
                        .clicked()
                    {
                        action = Some(HudBannerPlayAction::Stop);
                    }
                });
            } else {
                let play_btn = egui::Button::new(
                    egui::RichText::new(if is_finished {
                        "Tampilkan Lagi"
                    } else {
                        "Tampilkan / Play"
                    })
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(5, 150, 105));
                if ui
                    .add_enabled_ui(can_play, |ui| {
                        ui.add_sized([ui.available_width(), 24.0], play_btn)
                    })
                    .inner
                    .clicked()
                {
                    action = Some(HudBannerPlayAction::Play);
                }
            }
        });
    action
}
