use bevy_egui::egui;
use twg_shared::{
    api::CreateAnimRequest,
    dialog_anim::{
        ANIM_TYPE_DIALOG, DialogAnimConfig, DialogLine, DialogTextEffect, is_dialog_anim,
        parse_dialog_anim_config, write_dialog_anim_config,
    },
    models::Anim,
};
use uuid::Uuid;

use crate::{
    net::client::{ApiClientBridge, ApiCommand},
    scene::markers::current_auth,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, ToolMode},
    ui::{
        right_panel::inspector_card,
        spinner::{SpinnerConfig, numeric_spinner_w},
    },
};

fn hex_to_color32(hex: &str, fallback: egui::Color32) -> egui::Color32 {
    let s = hex.trim().trim_start_matches('#');
    if s.len() >= 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        )
    {
        return egui::Color32::from_rgb(r, g, b);
    }
    fallback
}

fn color32_to_hex(c: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

fn draw_hex_color_picker(
    ui: &mut egui::Ui,
    label: &str,
    hex: &mut String,
    fallback: egui::Color32,
) -> bool {
    let mut col = hex_to_color32(hex, fallback);
    let orig = col;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(10.5));
        ui.color_edit_button_srgba(&mut col);
        ui.label(
            egui::RichText::new(color32_to_hex(col))
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
        );
    });
    if col != orig {
        *hex = color32_to_hex(col);
        true
    } else {
        false
    }
}

pub fn anim_summary(state: &AppState, anim: &Anim) -> String {
    let cfg = parse_dialog_anim_config(anim);
    let line_cnt = cfg.lines.len();
    let speakers = cfg.unique_marker_ids();
    let total_dur = cfg.total_duration_sec(anim.replay);
    let spk_desc = if speakers.is_empty() {
        "Belum pilih pembicara".to_string()
    } else if speakers.len() == 1 {
        state
            .markers
            .iter()
            .find(|m| m.id == speakers[0])
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "1 Pembicara".into())
    } else {
        format!("{} Pembicara", speakers.len())
    };
    format!("{spk_desc} · {line_cnt} baris · {total_dur:.1} dtk")
}

pub fn create_dialog_anim(
    state: &mut AppState,
    api: &ApiClientBridge,
    first_marker_id: Option<Uuid>,
) {
    let Some(sk) = state.active_skenario.clone() else {
        return;
    };
    let count = state.anims.iter().filter(|a| is_dialog_anim(a)).count() + 1;
    let name = if state.new_dialog_name.trim().is_empty() {
        format!("Dialog {count}")
    } else {
        state.new_dialog_name.trim().to_string()
    };

    let mut cfg = DialogAnimConfig::default();
    if let Some(mid) = first_marker_id {
        cfg.lines.push(DialogLine {
            marker_id: Some(mid),
            text: "Selamat pagi, mohon tetap tertib.".to_string(),
            duration_sec: 3.5,
            pause_after_sec: 0.6,
            ..Default::default()
        });
    } else {
        let first_m = state.markers.first().map(|m| m.id);
        cfg.lines.push(DialogLine {
            marker_id: first_m,
            text: "Selamat pagi, mohon tetap tertib.".to_string(),
            duration_sec: 3.5,
            pause_after_sec: 0.6,
            ..Default::default()
        });
    }

    let sort_order = state
        .anims
        .iter()
        .map(|a| a.sort_order)
        .max()
        .unwrap_or(-1)
        + 1;
    let now = chrono::Utc::now();
    let mut anim_out = Anim {
        id: Uuid::new_v4(),
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: cfg.unique_marker_ids().first().copied(),
        name: name.clone(),
        anim_type: ANIM_TYPE_DIALOG.to_string(),
        sort_order,
        speed: 1.0,
        duration_sec: cfg.total_duration_sec(false),
        replay: false,
        autoplay: false,
        geom: Vec::new(),
        data: serde_json::json!({}),
        created_at: now,
        updated_at: now,
    };
    write_dialog_anim_config(&mut anim_out, &cfg);

    if state.offline_preview {
        let new_id = anim_out.id;
        state.anims.push(anim_out.clone());
        state.sort_anims_by_order();
        state.selected = SelectedEntity::Anim(new_id);
        state.anim_edit = Some(anim_out);
        state.dialog_preview_line_idx = Some((new_id, 0));
        state.marker_edit = None;
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.status_banner = Some(format!(
            "Animasi Dialog '{name}' dibuat — tambahkan baris percakapan di Inspector."
        ));
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateAnim {
            auth,
            req: CreateAnimRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: anim_out.marker_id,
                name: name.clone(),
                anim_type: ANIM_TYPE_DIALOG.to_string(),
                sort_order,
                speed: 1.0,
                duration_sec: anim_out.duration_sec,
                replay: false,
                autoplay: false,
                geom: Vec::new(),
                data: anim_out.data,
            },
        });
    }

    state.new_dialog_name = format!("Dialog {}", count + 1);
}

pub fn draw_create(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    editable: bool,
) {
    let is_picking =
        state.tool_mode == ToolMode::PickDialogSpeaker && state.picking_dialog_speaker.is_none();

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(16, 26, 44))
        .stroke(egui::Stroke::new(
            1.0,
            if is_picking {
                egui::Color32::from_rgb(250, 204, 21)
            } else {
                egui::Color32::from_rgb(56, 189, 248)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Animasi Dialog (Balon Bicara 3D)")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            );
            ui.label(
                egui::RichText::new(
                    "Balon bicara menunjuk ke Marker 3D yang sedang berbicara & berpindah otomatis saat gantian bicara (bisa 1 atau banyak marker).",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_dialog_name)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(5.0);
            let pick_lbl = if is_picking {
                "Batal Pilih Pembicara (Esc)"
            } else {
                "+ Pilih Pembicara Pertama di Peta"
            };
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(
                        egui::RichText::new(pick_lbl)
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if is_picking {
                        egui::Color32::from_rgb(180, 83, 9)
                    } else {
                        egui::Color32::from_rgb(2, 132, 199)
                    })
                    .min_size(egui::vec2(ui.available_width(), 24.0)),
                )
                .clicked()
            {
                if is_picking {
                    state.tool_mode = ToolMode::Select;
                    state.picking_dialog_speaker = None;
                } else {
                    state.picking_dialog_speaker = None;
                    state.tool_mode = ToolMode::PickDialogSpeaker;
                    state.status_banner = Some(
                        "Klik 1 Marker / Objek 3D di peta sebagai pembicara pertama Dialog (Esc batal)."
                            .into(),
                    );
                }
            }
            ui.add_space(3.0);
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(
                        egui::RichText::new("+ Buat Animasi Dialog")
                            .size(10.5)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    )
                    .fill(egui::Color32::from_rgb(30, 41, 59))
                    .min_size(egui::vec2(ui.available_width(), 22.0)),
                )
                .clicked()
            {
                create_dialog_anim(state, api, None);
            }
        });
}

pub fn draw_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    anim: &mut Anim,
    editable: bool,
) -> bool {
    let mut cfg = parse_dialog_anim_config(anim);
    let orig_cfg = cfg.clone();
    let orig_replay = anim.replay;

    let markers_snapshot: Vec<(Uuid, String)> = state
        .markers
        .iter()
        .map(|m| (m.id, m.name.clone()))
        .collect();

    // Ensure preview line index tracks this anim
    let active_preview_idx = state
        .dialog_preview_line_idx
        .filter(|(aid, _)| *aid == anim.id)
        .map(|(_, i)| i.min(cfg.lines.len().saturating_sub(1)))
        .unwrap_or(0);

    // Card 1: Dialog Lines (Speakers, Text, Duration, Pause, Ordering)
    let lines_title = format!(
        "Daftar Baris Dialog ({} baris · {:.1} dtk)",
        cfg.lines.len(),
        cfg.total_duration_sec(anim.replay)
    );
    inspector_card(ui, &lines_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            // Smart "+ Tambah Baris" alternating between last 2 speakers for effortless back-and-forth dialogs!
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("+ Tambah Baris Dialog")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(2, 132, 199))
                        .min_size(egui::vec2(ui.available_width(), 25.0)),
                    )
                    .clicked()
                {
                    let suggested_speaker = if cfg.lines.len() >= 2 {
                        // Alternate cleanly between speaker N-2 and N-1
                        cfg.lines[cfg.lines.len() - 2]
                            .marker_id
                            .or_else(|| cfg.lines.last().and_then(|l| l.marker_id))
                    } else if let Some(first_mid) = cfg.lines.first().and_then(|l| l.marker_id) {
                        // Pick a different marker if available, else same marker
                        markers_snapshot
                            .iter()
                            .find(|(id, _)| *id != first_mid)
                            .map(|(id, _)| *id)
                            .or(Some(first_mid))
                    } else {
                        markers_snapshot.first().map(|(id, _)| *id)
                    };
                    let new_idx = cfg.lines.len();
                    cfg.lines.push(DialogLine {
                        marker_id: suggested_speaker,
                        text: String::new(),
                        duration_sec: 3.5,
                        pause_after_sec: 0.6,
                        ..Default::default()
                    });
                    state.dialog_preview_line_idx = Some((anim.id, new_idx));
                }
            });

            if cfg.lines.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Belum ada baris dialog. Klik '+ Tambah Baris Dialog' untuk memulai.",
                    )
                    .size(10.5)
                    .color(egui::Color32::from_rgb(251, 191, 36)),
                );
            } else {
                ui.add_space(6.0);
                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;
                let mut remove_idx: Option<usize> = None;
                let total_lines = cfg.lines.len();

                for (idx, line) in cfg.lines.iter_mut().enumerate() {
                    let is_previewed = idx == active_preview_idx;
                    let card_fill = if is_previewed {
                        egui::Color32::from_rgb(20, 38, 58)
                    } else {
                        egui::Color32::from_rgb(17, 25, 40)
                    };
                    let card_stroke = if is_previewed {
                        egui::Color32::from_rgb(56, 189, 248)
                    } else {
                        egui::Color32::from_rgb(41, 53, 72)
                    };

                    egui::Frame::new()
                        .fill(card_fill)
                        .stroke(egui::Stroke::new(1.0, card_stroke))
                        .corner_radius(5.0)
                        .inner_margin(egui::Margin::same(7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            // Line header: #N + Preview pill + Reorder/Delete
                            ui.horizontal(|ui| {
                                let preview_lbl = if is_previewed {
                                    format!("#{} (Preview Peta)", idx + 1)
                                } else {
                                    format!("#{} · Lihat", idx + 1)
                                };
                                if ui
                                    .selectable_label(
                                        is_previewed,
                                        egui::RichText::new(preview_lbl).size(10.5).strong().color(
                                            if is_previewed {
                                                egui::Color32::from_rgb(125, 211, 252)
                                            } else {
                                                egui::Color32::from_rgb(203, 213, 225)
                                            },
                                        ),
                                    )
                                    .on_hover_text("Tampilkan balon baris ini di peta 3D")
                                    .clicked()
                                {
                                    state.dialog_preview_line_idx = Some((anim.id, idx));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("x")
                                                        .size(9.5)
                                                        .strong()
                                                        .color(egui::Color32::from_rgb(
                                                            254, 202, 202,
                                                        )),
                                                )
                                                .fill(egui::Color32::from_rgb(110, 28, 28))
                                                .small(),
                                            )
                                            .on_hover_text("Hapus baris dialog")
                                            .clicked()
                                        {
                                            remove_idx = Some(idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                idx + 1 < total_lines,
                                                egui::Button::new(
                                                    egui::RichText::new("v").size(9.5),
                                                )
                                                .small(),
                                            )
                                            .on_hover_text("Turunkan urutan")
                                            .clicked()
                                        {
                                            move_down = Some(idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                idx > 0,
                                                egui::Button::new(
                                                    egui::RichText::new("^").size(9.5),
                                                )
                                                .small(),
                                            )
                                            .on_hover_text("Naikkan urutan")
                                            .clicked()
                                        {
                                            move_up = Some(idx);
                                        }
                                    },
                                );
                            });

                            ui.add_space(4.0);

                            // Speaker marker picker (dropdown + Pick on Map button)
                            let cur_spk_name = line
                                .marker_id
                                .and_then(|id| {
                                    markers_snapshot
                                        .iter()
                                        .find(|(mid, _)| *mid == id)
                                        .map(|(_, n)| n.clone())
                                })
                                .unwrap_or_else(|| "-- Pilih Pembicara --".to_string());
                            let picking_this = state.tool_mode == ToolMode::PickDialogSpeaker
                                && state.picking_dialog_speaker == Some((anim.id, idx));

                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Pembicara:").size(10.0));
                                let combo_w = (ui.available_width() - 56.0).max(90.0);
                                egui::ComboBox::from_id_salt(("dialog_line_spk", anim.id, idx))
                                    .selected_text(cur_spk_name)
                                    .width(combo_w)
                                    .show_ui(ui, |ui| {
                                        for (mid, mname) in &markers_snapshot {
                                            if ui
                                                .selectable_value(
                                                    &mut line.marker_id,
                                                    Some(*mid),
                                                    mname,
                                                )
                                                .clicked()
                                            {
                                                state.dialog_preview_line_idx =
                                                    Some((anim.id, idx));
                                            }
                                        }
                                    });
                                let pick_btn = egui::Button::new(
                                    egui::RichText::new(if picking_this { "Batal" } else { "Peta" })
                                        .size(9.5)
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(if picking_this {
                                    egui::Color32::from_rgb(180, 83, 9)
                                } else {
                                    egui::Color32::from_rgb(30, 58, 138)
                                })
                                .small();
                                if ui
                                    .add(pick_btn)
                                    .on_hover_text("Klik Marker 3D di peta untuk baris ini")
                                    .clicked()
                                {
                                    if picking_this {
                                        state.tool_mode = ToolMode::Select;
                                        state.picking_dialog_speaker = None;
                                    } else {
                                        state.picking_dialog_speaker = Some((anim.id, idx));
                                        state.dialog_preview_line_idx = Some((anim.id, idx));
                                        state.tool_mode = ToolMode::PickDialogSpeaker;
                                        state.status_banner = Some(format!(
                                            "Klik Marker 3D di peta untuk Pembicara baris #{} (Esc batal).",
                                            idx + 1
                                        ));
                                    }
                                }
                            });

                            ui.add_space(3.0);

                            // Optional display name override
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Nama Tampil:").size(10.0));
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut line.speaker_override)
                                        .hint_text("(Default: nama marker)")
                                        .desired_width(ui.available_width()),
                                );
                                if resp.gained_focus() || resp.changed() {
                                    state.dialog_preview_line_idx = Some((anim.id, idx));
                                }
                            });

                            ui.add_space(3.0);

                            // Multi-line dialog text
                            let text_resp = ui.add(
                                egui::TextEdit::multiline(&mut line.text)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Tulis kalimat yang diucapkan pembicara..."),
                            );
                            if text_resp.gained_focus() || text_resp.changed() {
                                state.dialog_preview_line_idx = Some((anim.id, idx));
                            }

                            ui.add_space(4.0);

                            // Duration + Auto button + Pause after
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Tampil:").size(10.0));
                                numeric_spinner_w(
                                    ui,
                                    ("dialog_dur", anim.id, idx),
                                    &mut line.duration_sec,
                                    70.0,
                                    SpinnerConfig {
                                        min: 0.3,
                                        max: 120.0,
                                        step: 0.5,
                                        drag_speed: 0.1,
                                        decimals: 1,
                                        suffix: " dtk",
                                        wrap_around: false,
                                    },
                                );
                                if ui
                                    .small_button("Auto")
                                    .on_hover_text(
                                        "Hitung durasi baca otomatis dari panjang teks",
                                    )
                                    .clicked()
                                {
                                    line.duration_sec =
                                        DialogLine::estimated_reading_duration_sec(&line.text);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Jeda setelah ini:").size(10.0));
                                numeric_spinner_w(
                                    ui,
                                    ("dialog_pause", anim.id, idx),
                                    &mut line.pause_after_sec,
                                    70.0,
                                    SpinnerConfig {
                                        min: 0.0,
                                        max: 60.0,
                                        step: 0.2,
                                        drag_speed: 0.05,
                                        decimals: 1,
                                        suffix: " dtk",
                                        wrap_around: false,
                                    },
                                );
                            });

                            // Optional per-line color override
                            let has_col_ov = !line.bg_color_override.is_empty()
                                || !line.text_color_override.is_empty()
                                || !line.border_color_override.is_empty();
                            egui::CollapsingHeader::new(
                                egui::RichText::new(if has_col_ov {
                                    "Warna Khusus Baris (Aktif)"
                                } else {
                                    "Warna Khusus Baris (Opsional)"
                                })
                                .size(9.5)
                                .color(if has_col_ov {
                                    egui::Color32::from_rgb(251, 191, 36)
                                } else {
                                    egui::Color32::from_rgb(148, 163, 184)
                                }),
                            )
                            .id_salt(("dialog_line_colors", anim.id, idx))
                            .default_open(false)
                            .show(ui, |ui| {
                                let mut use_custom = has_col_ov;
                                if ui
                                    .checkbox(&mut use_custom, "Pakai warna khusus untuk baris ini")
                                    .changed()
                                {
                                    if use_custom {
                                        line.bg_color_override = cfg.bg_color.clone();
                                        line.text_color_override = cfg.text_color.clone();
                                        line.border_color_override = cfg.border_color.clone();
                                    } else {
                                        line.bg_color_override.clear();
                                        line.text_color_override.clear();
                                        line.border_color_override.clear();
                                    }
                                    state.dialog_preview_line_idx = Some((anim.id, idx));
                                }
                                if use_custom {
                                    let bg_fb = hex_to_color32(
                                        &cfg.bg_color,
                                        egui::Color32::from_rgb(15, 23, 42),
                                    );
                                    let tx_fb = hex_to_color32(&cfg.text_color, egui::Color32::WHITE);
                                    let bd_fb = hex_to_color32(
                                        &cfg.border_color,
                                        egui::Color32::from_rgb(56, 189, 248),
                                    );
                                    if draw_hex_color_picker(
                                        ui,
                                        "BG Balon:",
                                        &mut line.bg_color_override,
                                        bg_fb,
                                    ) || draw_hex_color_picker(
                                        ui,
                                        "Teks:",
                                        &mut line.text_color_override,
                                        tx_fb,
                                    ) || draw_hex_color_picker(
                                        ui,
                                        "Border:",
                                        &mut line.border_color_override,
                                        bd_fb,
                                    ) {
                                        state.dialog_preview_line_idx = Some((anim.id, idx));
                                    }
                                }
                            });
                        });
                    ui.add_space(5.0);
                }

                if let Some(i) = move_up
                    && i > 0
                {
                    cfg.lines.swap(i, i - 1);
                    state.dialog_preview_line_idx = Some((anim.id, i - 1));
                }
                if let Some(i) = move_down
                    && i + 1 < cfg.lines.len()
                {
                    cfg.lines.swap(i, i + 1);
                    state.dialog_preview_line_idx = Some((anim.id, i + 1));
                }
                if let Some(i) = remove_idx
                    && i < cfg.lines.len()
                {
                    cfg.lines.remove(i);
                    let next_i = i.min(cfg.lines.len().saturating_sub(1));
                    state.dialog_preview_line_idx = Some((anim.id, next_i));
                }
            }
        });
    });

    // Card 2: Global Bubble Appearance & Timing
    inspector_card(ui, "Gaya Balon Bicara, Font & Waktu", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.checkbox(&mut cfg.show_speaker_name, "Tampilkan Nama Pembicara di Balon");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Ukuran Font:");
                let mut fs = cfg.font_size as f64;
                numeric_spinner_w(
                    ui,
                    ("dialog_font_sz", anim.id),
                    &mut fs,
                    82.0,
                    SpinnerConfig {
                        min: 10.0,
                        max: 36.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 0,
                        suffix: " px",
                        wrap_around: false,
                    },
                );
                cfg.font_size = fs.clamp(10.0, 36.0) as f32;
            });
            ui.horizontal(|ui| {
                ui.label("Lebar Maks Balon:");
                let mut mw = cfg.max_width_px as f64;
                numeric_spinner_w(
                    ui,
                    ("dialog_max_w", anim.id),
                    &mut mw,
                    82.0,
                    SpinnerConfig {
                        min: 160.0,
                        max: 600.0,
                        step: 20.0,
                        drag_speed: 1.0,
                        decimals: 0,
                        suffix: " px",
                        wrap_around: false,
                    },
                );
                cfg.max_width_px = mw.clamp(160.0, 600.0) as f32;
            });
            ui.horizontal(|ui| {
                ui.label("Tinggi di Atas Kepala:");
                numeric_spinner_w(
                    ui,
                    ("dialog_height_off", anim.id),
                    &mut cfg.height_offset_m,
                    82.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 30.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            draw_hex_color_picker(
                ui,
                "Warna Teks:",
                &mut cfg.text_color,
                egui::Color32::from_rgb(248, 250, 252),
            );
            draw_hex_color_picker(
                ui,
                "Warna BG Balon:",
                &mut cfg.bg_color,
                egui::Color32::from_rgb(15, 23, 42),
            );
            ui.horizontal(|ui| {
                ui.label("Opasitas BG:");
                ui.add(
                    egui::Slider::new(&mut cfg.bg_opacity, 0.2..=1.0)
                        .fixed_decimals(2),
                );
            });
            draw_hex_color_picker(
                ui,
                "Warna Border & Nama:",
                &mut cfg.border_color,
                egui::Color32::from_rgb(56, 189, 248),
            );

            ui.add_space(6.0);
            ui.label("Animasi Muncul Teks:");
            egui::ComboBox::from_id_salt(("dialog_text_fx", anim.id))
                .selected_text(cfg.text_effect.label())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for &fx in DialogTextEffect::ALL {
                        ui.selectable_value(&mut cfg.text_effect, fx, fx.label());
                    }
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Jeda Awal (Delay):");
                numeric_spinner_w(
                    ui,
                    ("dialog_delay", anim.id),
                    &mut cfg.delay_sec,
                    82.0,
                    SpinnerConfig {
                        min: 0.0,
                        max: 60.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.checkbox(&mut anim.replay, "Repeat / Ulangi Dialog dari Awal");
        });
    });

    if cfg != orig_cfg || anim.replay != orig_replay {
        write_dialog_anim_config(anim, &cfg);
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogPlayAction {
    Play,
    Pause,
    Resume,
    Stop,
    Focus,
}

/// 100% uniform Play Panel card matching all other animation cards (`#N`, `[DIALOG]`,
/// Name, Status Pill, `Go`, 2 info lines, `draw_card_progress_bar`, full-width `Play` /
/// 2-column `Pause` + `Stop`).
#[allow(clippy::too_many_arguments)]
pub fn draw_play_card(
    ui: &mut egui::Ui,
    state: &AppState,
    playback: &PlaybackState,
    anim: &Anim,
    global_idx: usize,
    can_control: bool,
) -> Option<DialogPlayAction> {
    let cfg = parse_dialog_anim_config(anim);
    let track = playback.tracks.get(&anim.id);
    let status = track.map(|t| t.status).unwrap_or(AnimRunStatus::Stopped);
    let playable = twg_shared::route_formation::anim_is_playable(anim, &state.markers);

    let (status_txt, status_bg, status_fg, card_stroke) = match status {
        AnimRunStatus::Stopped => (
            "SIAP",
            egui::Color32::from_rgb(30, 41, 59),
            egui::Color32::from_rgb(148, 163, 184),
            egui::Color32::from_rgb(44, 58, 82),
        ),
        AnimRunStatus::Playing => (
            "BERBICARA",
            egui::Color32::from_rgb(12, 74, 110),
            egui::Color32::from_rgb(125, 211, 252),
            egui::Color32::from_rgb(14, 165, 233),
        ),
        AnimRunStatus::Paused => (
            "DIJEDA",
            egui::Color32::from_rgb(120, 53, 15),
            egui::Color32::from_rgb(253, 230, 138),
            egui::Color32::from_rgb(245, 158, 11),
        ),
        AnimRunStatus::Finished => (
            "SELESAI",
            egui::Color32::from_rgb(30, 58, 138),
            egui::Color32::from_rgb(191, 219, 254),
            egui::Color32::from_rgb(56, 189, 248),
        ),
    };

    let mut action = None;
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .stroke(egui::Stroke::new(1.2, card_stroke))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Row 1: #Order + [DIALOG] + Name + Status + Go
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("#{}", global_idx + 1))
                        .size(10.0)
                        .strong()
                        .color(egui::Color32::from_rgb(125, 211, 252)),
                );
                ui.label(
                    egui::RichText::new("[DIALOG]")
                        .size(9.5)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&anim.name)
                            .size(12.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !cfg.unique_marker_ids().is_empty(),
                            egui::Button::new(
                                egui::RichText::new("Go")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            )
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Fokus kamera ke pembicara dialog")
                        .clicked()
                    {
                        action = Some(DialogPlayAction::Focus);
                    }
                    egui::Frame::new()
                        .fill(status_bg)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(status_txt)
                                    .size(9.0)
                                    .strong()
                                    .color(status_fg),
                            );
                        });
                });
            });

            ui.add_space(3.0);

            // Info line 1: Speakers & line count summary
            ui.add(
                egui::Label::new(
                    egui::RichText::new(anim_summary(state, anim))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                )
                .truncate(),
            );

            // Info line 2: Live speaking line or effect mode
            let detail_line = if status != AnimRunStatus::Stopped
                && let Some(live) = state.dialog_status.get(&anim.id)
            {
                live.clone()
            } else {
                format!(
                    "Gaya: {}{}",
                    cfg.text_effect.label(),
                    if anim.replay { " · Repeat" } else { "" }
                )
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail_line)
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                )
                .truncate(),
            );

            let progress = if status == AnimRunStatus::Finished {
                1.0
            } else if let Some(tr) = track {
                if tr.total_length_m > 0.0 {
                    (tr.distance_m / tr.total_length_m).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };
            ui.add_space(4.0);
            super::play_panel::draw_card_progress_bar(ui, status, progress);
            ui.add_space(7.0);

            match status {
                AnimRunStatus::Stopped => {
                    let play_btn = egui::Button::new(
                        egui::RichText::new("Play")
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if can_control && playable {
                        egui::Color32::from_rgb(5, 150, 105)
                    } else {
                        egui::Color32::from_rgb(30, 41, 59)
                    })
                    .min_size(egui::vec2(ui.available_width(), 28.0));
                    if ui.add_enabled(can_control && playable, play_btn).clicked() {
                        action = Some(DialogPlayAction::Play);
                    }
                }
                AnimRunStatus::Playing => {
                    ui.columns(2, |cols| {
                        let pause_btn = egui::Button::new(
                            egui::RichText::new("Pause")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(180, 83, 9))
                        .min_size(egui::vec2(cols[0].available_width(), 28.0));
                        if cols[0].add_enabled(can_control, pause_btn).clicked() {
                            action = Some(DialogPlayAction::Pause);
                        }

                        let stop_btn = egui::Button::new(
                            egui::RichText::new("Stop")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(185, 28, 28))
                        .min_size(egui::vec2(cols[1].available_width(), 28.0));
                        if cols[1].add_enabled(can_control, stop_btn).clicked() {
                            action = Some(DialogPlayAction::Stop);
                        }
                    });
                }
                AnimRunStatus::Paused => {
                    ui.columns(2, |cols| {
                        let resume_btn = egui::Button::new(
                            egui::RichText::new("Resume")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(cols[0].available_width(), 28.0));
                        if cols[0].add_enabled(can_control, resume_btn).clicked() {
                            action = Some(DialogPlayAction::Resume);
                        }

                        let stop_btn = egui::Button::new(
                            egui::RichText::new("Stop")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(185, 28, 28))
                        .min_size(egui::vec2(cols[1].available_width(), 28.0));
                        if cols[1].add_enabled(can_control, stop_btn).clicked() {
                            action = Some(DialogPlayAction::Stop);
                        }
                    });
                }
                AnimRunStatus::Finished => {
                    ui.columns(2, |cols| {
                        let replay_btn = egui::Button::new(
                            egui::RichText::new("Play")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(cols[0].available_width(), 28.0));
                        if cols[0].add_enabled(can_control && playable, replay_btn).clicked() {
                            action = Some(DialogPlayAction::Play);
                        }

                        let stop_btn = egui::Button::new(
                            egui::RichText::new("Stop")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(185, 28, 28))
                        .min_size(egui::vec2(cols[1].available_width(), 28.0));
                        if cols[1].add_enabled(can_control, stop_btn).clicked() {
                            action = Some(DialogPlayAction::Stop);
                        }
                    });
                }
            }
        });
    ui.add_space(6.0);
    action
}
