use bevy_egui::egui;
use twg_shared::{
    models::Anim,
    tactical_arrow_anim::{
        TacticalArrowConfig, TacticalArrowPreset, is_tactical_arrow_anim,
        parse_tactical_arrow_config, write_tactical_arrow_config,
    },
};

use crate::{
    scene::playback::route_total_length_m,
    state::{AnimRunStatus, AppState, PlaybackState, ToolMode},
    ui::spinner::{SpinnerConfig, numeric_spinner_f32_w, numeric_spinner_w},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TacticalArrowPlayAction {
    Play,
    Pause,
    Resume,
    Stop,
    Focus,
}

pub fn anim_summary(anim: &Anim) -> String {
    let cfg = parse_tactical_arrow_config(anim);
    let pts = anim.geom.len();
    if pts < 2 {
        return format!("{}  •  Belum ada jalur", cfg.preset.label());
    }
    let len_m = route_total_length_m(&anim.geom);
    let dur_str = if cfg.continuous {
        "Terus mengalir".to_string()
    } else {
        format!("{:.1} dtk", cfg.finite_duration_sec())
    };
    format!("{}  •  {len_m:.0} m ({pts} titik)  •  {dur_str}", cfg.preset.label())
}

pub fn draw_create(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    let is_drawing = state.tool_mode == ToolMode::DrawTacticalArrow
        && state.redrawing_tactical_arrow_anim_id.is_none();

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .stroke(egui::Stroke::new(
            1.0,
            if is_drawing {
                egui::Color32::from_rgb(248, 113, 113)
            } else {
                egui::Color32::from_rgb(64, 42, 46)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Panah Alur Taktis 3D (Serangan / Evakuasi)")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(252, 165, 165)),
            );
            ui.label(
                egui::RichText::new(
                    "Garis lengkung dengan panah berjalan di atas peta untuk arah dorongan, evakuasi, atau penyekatan.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Jenis:").size(10.5));
                egui::ComboBox::from_id_salt("new_tactical_arrow_preset_cb")
                    .width(ui.available_width().max(120.0))
                    .selected_text(state.new_tactical_arrow_preset.label())
                    .show_ui(ui, |ui| {
                        for p in TacticalArrowPreset::ALL {
                            if ui
                                .selectable_value(&mut state.new_tactical_arrow_preset, p, p.label())
                                .changed()
                            {
                                let next_n = state
                                    .anims
                                    .iter()
                                    .filter(|a| is_tactical_arrow_anim(a))
                                    .count()
                                    + 1;
                                state.new_tactical_arrow_name = format!("{} {next_n}", p.label());
                            }
                        }
                    });
            });
            ui.add_space(3.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_tactical_arrow_name)
                        .desired_width(f32::INFINITY)
                        .hint_text("Dorongan PHH / Evakuasi VIP"),
                );
            });
            ui.add_space(5.0);

            let btn_txt = if is_drawing {
                format!(
                    "Sedang Gambar ({} titik)... Klik Batal",
                    state.draft_points_lnglat.len()
                )
            } else {
                "+ Gambar Panah Taktis di Peta".to_string()
            };
            let btn = egui::Button::new(
                egui::RichText::new(btn_txt)
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(if is_drawing {
                egui::Color32::from_rgb(185, 28, 28)
            } else {
                egui::Color32::from_rgb(220, 38, 38)
            });

            if ui
                .add_enabled(editable, btn)
                .on_hover_text("Klik minimal 2 titik di peta untuk jalur panah, lalu tekan Enter")
                .clicked()
            {
                if is_drawing {
                    state.draft_points_lnglat.clear();
                    state.tool_mode = ToolMode::Select;
                } else {
                    state.draft_points_lnglat.clear();
                    state.redrawing_tactical_arrow_anim_id = None;
                    state.tool_mode = ToolMode::DrawTacticalArrow;
                    state.status_banner = Some(
                        "Klik titik-titik jalur Panah Taktis di peta (min 2 titik), lalu tekan Enter untuk selesai."
                            .into(),
                    );
                }
            }
        });
}

fn color_edit_hex(ui: &mut egui::Ui, label: &str, hex: &mut String, editable: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(10.5));
        let clean = hex.trim().trim_start_matches('#');
        let mut srgb = [239u8, 68, 68];
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
    let mut cfg: TacticalArrowConfig = parse_tactical_arrow_config(anim);
    let mut dirty = false;

    egui::CollapsingHeader::new(
        egui::RichText::new("Panah Alur Taktis 3D — Jalur & Gaya")
            .size(12.0)
            .strong()
            .color(egui::Color32::from_rgb(252, 165, 165)),
    )
    .id_salt(("tactical_arrow_insp_main", anim.id))
    .default_open(true)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Nama:").size(11.0));
            ui.add_enabled(
                editable,
                egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY),
            );
        });
        ui.add_space(4.0);

        // Path status & quick actions
        let len_m = route_total_length_m(&anim.geom);
        ui.label(
            egui::RichText::new(format!(
                "Jalur: {} titik  •  Panjang: {:.1} m",
                anim.geom.len(),
                len_m
            ))
            .size(10.5)
            .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let redrawing_this = state.tool_mode == ToolMode::DrawTacticalArrow
                && state.redrawing_tactical_arrow_anim_id == Some(anim.id);
            let redraw_btn = egui::Button::new(
                egui::RichText::new(if redrawing_this {
                    "Batal Gambar"
                } else {
                    "Gambar Ulang Jalur"
                })
                .size(10.5)
                .strong()
                .color(egui::Color32::WHITE),
            )
            .fill(if redrawing_this {
                egui::Color32::from_rgb(185, 28, 28)
            } else {
                egui::Color32::from_rgb(14, 116, 144)
            });
            if ui.add_enabled(editable, redraw_btn).clicked() {
                if redrawing_this {
                    state.draft_points_lnglat.clear();
                    state.redrawing_tactical_arrow_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                } else {
                    state.draft_points_lnglat.clear();
                    state.redrawing_tactical_arrow_anim_id = Some(anim.id);
                    state.tool_mode = ToolMode::DrawTacticalArrow;
                    state.status_banner = Some(format!(
                        "Klik titik jalur baru untuk '{}' di peta (min 2 titik, Enter selesai).",
                        anim.name
                    ));
                }
            }

            if ui
                .add_enabled(
                    editable && anim.geom.len() >= 2,
                    egui::Button::new(egui::RichText::new("Balik Arah").size(10.5)),
                )
                .on_hover_text("Membalik arah panah dari ujung ke pangkal")
                .clicked()
            {
                anim.geom.reverse();
                dirty = true;
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // Preset selector
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Preset Taktis:").size(11.0));
            egui::ComboBox::from_id_salt(("tactical_arrow_preset_cb", anim.id))
                .width(ui.available_width().max(130.0))
                .selected_text(cfg.preset.label())
                .show_ui(ui, |ui| {
                    for p in TacticalArrowPreset::ALL {
                        let sel = cfg.preset == p;
                        if ui.selectable_label(sel, p.label()).clicked() && !sel {
                            cfg.apply_preset(p);
                            dirty = true;
                        }
                    }
                });
        });

        ui.add_space(4.0);
        if color_edit_hex(ui, "Warna Badan:", &mut cfg.color, editable) {
            cfg.preset = TacticalArrowPreset::Custom;
            dirty = true;
        }
        if color_edit_hex(ui, "Warna Panah Berjalan:", &mut cfg.accent_color, editable) {
            cfg.preset = TacticalArrowPreset::Custom;
            dirty = true;
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Lebar Jalur (m):").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_f32_w(
                    ui,
                    ("ta_w", anim.id),
                    &mut cfg.width_m,
                    92.0,
                    SpinnerConfig {
                        min: 0.8,
                        max: 30.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " m",
                        ..Default::default()
                    },
                ) {
                    dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Ketinggian (m):").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_f32_w(
                    ui,
                    ("ta_alt", anim.id),
                    &mut cfg.altitude_m,
                    92.0,
                    SpinnerConfig {
                        min: 0.15,
                        max: 80.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 2,
                        suffix: " m",
                        ..Default::default()
                    },
                ) {
                    dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Kecepatan Aliran:").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_f32_w(
                    ui,
                    ("ta_spd", anim.id),
                    &mut cfg.flow_speed,
                    92.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 5.0,
                        step: 0.15,
                        drag_speed: 0.05,
                        decimals: 2,
                        suffix: "x",
                        ..Default::default()
                    },
                ) {
                    dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Jarak Antar Panah:").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_f32_w(
                    ui,
                    ("ta_spc", anim.id),
                    &mut cfg.chevron_spacing_m,
                    92.0,
                    SpinnerConfig {
                        min: 3.0,
                        max: 60.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 0,
                        suffix: " m",
                        ..Default::default()
                    },
                ) {
                    dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Opasitas:").size(10.5));
            if ui
                .add_enabled(editable, egui::Slider::new(&mut cfg.opacity, 0.2..=1.0).fixed_decimals(2))
                .changed()
            {
                dirty = true;
            }
        });

        if ui
            .add_enabled(
                editable,
                egui::Checkbox::new(&mut cfg.smooth_curve, "Kurva Lengkung Halus (Smooth)"),
            )
            .changed()
        {
            dirty = true;
        }
    });

    ui.add_space(6.0);

    egui::CollapsingHeader::new(
        egui::RichText::new("Waktu Tampil & Label Jalur")
            .size(12.0)
            .strong()
            .color(egui::Color32::from_rgb(186, 230, 253)),
    )
    .id_salt(("tactical_arrow_insp_timing", anim.id))
    .default_open(true)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Durasi Muncul / Menjulur:").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_f32_w(
                    ui,
                    ("ta_grow", anim.id),
                    &mut cfg.grow_duration_sec,
                    92.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 15.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " dtk",
                        ..Default::default()
                    },
                ) {
                    dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Jeda Awal (Delay):").size(10.5));
            ui.add_enabled_ui(editable, |ui| {
                if numeric_spinner_w(
                    ui,
                    ("ta_delay", anim.id),
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

        if ui
            .add_enabled(
                editable,
                egui::Checkbox::new(&mut cfg.continuous, "Alirkan Terus Sampai Dihentikan (Stop)"),
            )
            .changed()
        {
            dirty = true;
        }

        if !cfg.continuous {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Lama Tampil Penuh:").size(10.5));
                ui.add_enabled_ui(editable, |ui| {
                    if numeric_spinner_f32_w(
                        ui,
                        ("ta_hold", anim.id),
                        &mut cfg.hold_duration_sec,
                        92.0,
                        SpinnerConfig {
                            min: 0.5,
                            max: 600.0,
                            step: 1.0,
                            drag_speed: 0.2,
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

        ui.add_space(4.0);
        if ui
            .add_enabled(
                editable,
                egui::Checkbox::new(&mut cfg.show_label, "Tampilkan Label di Tengah Jalur"),
            )
            .changed()
        {
            dirty = true;
        }
        if cfg.show_label {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Teks Label:").size(10.5));
                if ui
                    .add_enabled(
                        editable,
                        egui::TextEdit::singleline(&mut cfg.label_text)
                            .desired_width(f32::INFINITY)
                            .hint_text("Contoh: DORONGAN PHH / EVAKUASI VIP"),
                    )
                    .changed()
                {
                    dirty = true;
                }
            });
        }
    });

    if dirty {
        write_tactical_arrow_config(anim, &cfg);
        if let Some(live) = state.anims.iter_mut().find(|a| a.id == anim.id) {
            live.name = anim.name.clone();
            live.geom = anim.geom.clone();
            live.replay = anim.replay;
            live.duration_sec = anim.duration_sec;
            live.data = anim.data.clone();
        }
        state.dirty_anim_ids.insert(anim.id);
    }
}

/// 100% uniform Play Panel card for `tactical_arrow`.
pub fn draw_play_card(
    ui: &mut egui::Ui,
    playback: &PlaybackState,
    anim: &Anim,
    display_idx: usize,
) -> Option<TacticalArrowPlayAction> {
    let mut action = None;
    let cfg = parse_tactical_arrow_config(anim);
    let can_play = anim.geom.len() >= 2;
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
    let progress = if !cfg.continuous && total_dur > 1e-4 {
        (elapsed / total_dur).clamp(0.0, 1.0) as f32
    } else if is_playing {
        ((elapsed * 0.5) % 1.0) as f32
    } else if is_finished {
        1.0
    } else {
        0.0
    };

    let (status_label, status_bg, status_fg) = if is_playing {
        (
            "AKTIF",
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
        egui::Color32::from_rgb(248, 113, 113)
    } else if is_paused {
        egui::Color32::from_rgb(245, 158, 11)
    } else {
        egui::Color32::from_rgb(38, 52, 74)
    };

    egui::Frame::new()
        .fill(if is_playing {
            egui::Color32::from_rgb(28, 22, 34)
        } else {
            egui::Color32::from_rgb(18, 27, 42)
        })
        .stroke(egui::Stroke::new(if is_playing { 1.4 } else { 1.0 }, border_col))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // Row 1: #N + [ALUR 3D] + Name + Status Pill + Go
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(
                    egui::RichText::new(format!("#{display_idx}"))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(185, 28, 28))
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(5, 1))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("ALUR 3D")
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
                    if can_play
                        && ui
                            .small_button("Go")
                            .on_hover_text("Arahkan kamera ke Panah Taktis ini")
                            .clicked()
                    {
                        action = Some(TacticalArrowPlayAction::Focus);
                    }
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
            let len_m = route_total_length_m(&anim.geom);
            ui.label(
                egui::RichText::new(format!(
                    "{}  •  {:.0} m ({} titik)",
                    cfg.preset.label(),
                    len_m,
                    anim.geom.len()
                ))
                .size(10.5)
                .color(egui::Color32::from_rgb(203, 213, 225)),
            );

            let line2 = if is_playing || is_paused {
                if cfg.continuous {
                    format!("Aliran aktif ({:.1} dtk)", elapsed)
                } else {
                    format!("{:.1} / {:.1} dtk", elapsed.min(total_dur), total_dur)
                }
            } else if cfg.continuous {
                "Aliran terus-menerus hingga Stop".to_string()
            } else {
                format!("Durasi {:.1} dtk", total_dur)
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
                            TacticalArrowPlayAction::Pause
                        } else {
                            TacticalArrowPlayAction::Resume
                        });
                    }

                    let stop_btn = egui::Button::new(
                        egui::RichText::new("Stop")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(185, 28, 28));
                    if cols[1]
                        .add_sized([cols[1].available_width(), 24.0], stop_btn)
                        .clicked()
                    {
                        action = Some(TacticalArrowPlayAction::Stop);
                    }
                });
            } else {
                let play_btn = egui::Button::new(
                    egui::RichText::new(if is_finished { "Putar Ulang" } else { "Play" })
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
                    action = Some(TacticalArrowPlayAction::Play);
                }
            }
        });
    action
}
