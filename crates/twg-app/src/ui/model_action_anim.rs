use bevy_egui::egui;
use twg_shared::{
    api::CreateAnimRequest,
    model_action_anim::{
        ANIM_TYPE_MODEL_ACTION, ModelActionAnimConfig, ModelActionPlayMode, is_model_action_anim,
        parse_model_action_config, write_model_action_config,
    },
    models::{Anim, ModelAssetType, ModelCatalogEntry},
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

/// Returns all selectable `(action_slug, display_label)` options for the 3D catalog model
/// backing `marker_model_path`. Includes uploaded `extra_animations` as well as any
/// multi-clip embedded names if present.
pub fn available_actions_for_model(
    state: &AppState,
    marker_model_path: &str,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(entry) = find_catalog_entry(state, marker_model_path) {
        for act in &entry.extra_animations {
            out.push((act.slug.clone(), act.name.clone()));
        }
    }
    out
}

pub fn find_catalog_entry<'a>(
    state: &'a AppState,
    marker_model_path: &str,
) -> Option<&'a ModelCatalogEntry> {
    state
        .catalog
        .iter()
        .find(|c| c.model_path == marker_model_path)
}

pub fn action_display_name(state: &AppState, anim: &Anim) -> String {
    let cfg = parse_model_action_config(anim);
    if cfg.action_slug.trim().is_empty() {
        return "Belum pilih aksi".to_string();
    }
    if let Some(clip) = cfg.action_slug.strip_prefix("embedded:") {
        return format!("Clip: {clip}");
    }
    if let Some(mid) = anim.marker_id
        && let Some(marker) = state.markers.iter().find(|m| m.id == mid)
        && let Some(entry) = find_catalog_entry(state, &marker.model_path)
        && let Some(act) = entry
            .extra_animations
            .iter()
            .find(|a| a.slug == cfg.action_slug)
    {
        return act.name.clone();
    }
    cfg.action_slug
}

pub fn anim_summary(state: &AppState, anim: &Anim) -> String {
    let cfg = parse_model_action_config(anim);
    let target = anim
        .marker_id
        .and_then(|id| state.markers.iter().find(|m| m.id == id))
        .map(|m| m.name.as_str())
        .unwrap_or("Belum pilih objek");
    let act_name = action_display_name(state, anim);
    format!("{target} · {act_name} · {}", cfg.play_mode.short_label())
}

pub fn create_model_action_anim(
    state: &mut AppState,
    api: &ApiClientBridge,
    marker_id: Option<Uuid>,
) {
    let Some(sk) = state.active_skenario.clone() else {
        return;
    };
    let mut name = state.new_model_action_name.trim().to_string();
    if name.is_empty() {
        let count = state
            .anims
            .iter()
            .filter(|a| is_model_action_anim(a))
            .count()
            + 1;
        name = format!("Aksi Model {count}");
    }

    let mut cfg = ModelActionAnimConfig::default();
    if let Some(mid) = marker_id
        && let Some(m) = state.markers.iter().find(|m| m.id == mid)
        && let Some(first_act) = available_actions_for_model(state, &m.model_path).into_iter().next()
    {
        cfg.action_slug = first_act.0;
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
        marker_id,
        name: name.clone(),
        anim_type: ANIM_TYPE_MODEL_ACTION.to_string(),
        sort_order,
        speed: cfg.speed,
        duration_sec: 0.0,
        replay: cfg.play_mode == ModelActionPlayMode::LoopForever,
        autoplay: false,
        geom: Vec::new(),
        data: serde_json::json!({}),
        created_at: now,
        updated_at: now,
    };
    write_model_action_config(&mut anim_out, &cfg);

    if state.offline_preview {
        let new_id = anim_out.id;
        state.anims.push(anim_out.clone());
        state.sort_anims_by_order();
        state.selected = SelectedEntity::Anim(new_id);
        state.anim_edit = Some(anim_out);
        state.marker_edit = None;
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.status_banner = Some(format!("Aksi Model 3D '{name}' dibuat."));
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateAnim {
            auth,
            req: CreateAnimRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id,
                name: name.clone(),
                anim_type: ANIM_TYPE_MODEL_ACTION.to_string(),
                sort_order,
                speed: cfg.speed,
                duration_sec: 0.0,
                replay: cfg.play_mode == ModelActionPlayMode::LoopForever,
                autoplay: false,
                geom: Vec::new(),
                data: anim_out.data,
            },
        });
    }

    let next_idx = state
        .anims
        .iter()
        .filter(|a| is_model_action_anim(a))
        .count()
        + 2;
    state.new_model_action_name = format!("Aksi Model {next_idx}");
}

pub fn draw_create(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    editable: bool,
) {
    let is_picking = state.tool_mode == ToolMode::PickModelActionMarker
        && state.picking_model_action_for_anim_id.is_none();

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(24, 22, 36))
        .stroke(egui::Stroke::new(
            1.0,
            if is_picking {
                egui::Color32::from_rgb(250, 204, 21)
            } else {
                egui::Color32::from_rgb(245, 158, 11)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Aksi Model 3D (Klip Animasi Tambahan)")
                    .size(11.5)
                    .strong()
                    .color(egui::Color32::from_rgb(251, 191, 36)),
            );
            ui.label(
                egui::RichText::new(
                    "Putar animasi tambahan milik model 3D (melambai, menembak, pingsan, hormat). Otomatis berhenti & pindah ke Walk saat objek berjalan di Rute.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Nama:").size(10.5));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut state.new_model_action_name)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(5.0);
            let pick_lbl = if is_picking {
                "Batal Pilih Objek (Esc)"
            } else {
                "+ Pilih Objek 3D di Peta"
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
                        egui::Color32::from_rgb(217, 119, 6)
                    })
                    .min_size(egui::vec2(ui.available_width(), 24.0)),
                )
                .clicked()
            {
                if is_picking {
                    state.tool_mode = ToolMode::Select;
                    state.picking_model_action_for_anim_id = None;
                } else {
                    state.picking_model_action_for_anim_id = None;
                    state.tool_mode = ToolMode::PickModelActionMarker;
                    state.status_banner = Some(
                        "Klik objek 3D (model Animasi) di peta untuk membuat Aksi Model 3D (Esc batal)."
                            .into(),
                    );
                }
            }
            ui.add_space(3.0);
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(
                        egui::RichText::new("+ Buat Kosong (Pilih Objek di Inspector)")
                            .size(10.5)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    )
                    .fill(egui::Color32::from_rgb(30, 41, 59))
                    .min_size(egui::vec2(ui.available_width(), 22.0)),
                )
                .clicked()
            {
                create_model_action_anim(state, api, None);
            }
        });
}

pub fn draw_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    anim: &mut Anim,
    editable: bool,
) -> bool {
    let mut cfg = parse_model_action_config(anim);
    let orig_cfg = cfg.clone();
    let orig_marker_id = anim.marker_id;

    // Card 1: Target 3D Marker & Action Clip
    inspector_card(ui, "Objek 3D & Klip Aksi", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            let cur_marker = anim
                .marker_id
                .and_then(|id| state.markers.iter().find(|m| m.id == id))
                .cloned();
            let selected_lbl = cur_marker
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "-- Pilih Objek 3D --".to_string());

            ui.label("Objek 3D Target:");
            let markers_snapshot: Vec<(Uuid, String, String, usize, bool)> = state
                .markers
                .iter()
                .map(|m| {
                    let entry = find_catalog_entry(state, &m.model_path);
                    let extra_cnt = entry.map(|e| e.extra_animations.len()).unwrap_or(0);
                    let is_anim = entry
                        .is_some_and(|e| e.model_type == ModelAssetType::Animated);
                    (m.id, m.name.clone(), m.model_path.clone(), extra_cnt, is_anim)
                })
                .collect();

            egui::ComboBox::from_id_salt(("model_action_marker", anim.id))
                .selected_text(selected_lbl)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (mid, mname, mpath, extra_cnt, is_anim) in &markers_snapshot {
                        let tag = if *extra_cnt > 0 {
                            format!("{mname} ({extra_cnt} aksi)")
                        } else if *is_anim {
                            format!("{mname} (Animasi)")
                        } else {
                            mname.clone()
                        };
                        if ui
                            .selectable_value(&mut anim.marker_id, Some(*mid), tag)
                            .clicked()
                        {
                            // Auto-pick first action clip if current slug isn't valid on newly picked model
                            let acts = available_actions_for_model(state, mpath);
                            if !acts.iter().any(|(s, _)| s == &cfg.action_slug) {
                                cfg.action_slug = acts
                                    .first()
                                    .map(|(s, _)| s.clone())
                                    .unwrap_or_default();
                            }
                        }
                    }
                });

            ui.add_space(4.0);
            let picking_this = state.tool_mode == ToolMode::PickModelActionMarker
                && state.picking_model_action_for_anim_id == Some(anim.id);
            let pick_lbl = if picking_this {
                "Batal Pilih di Peta (Esc)"
            } else {
                "Pilih Objek di Peta"
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(pick_lbl)
                            .size(11.0)
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if picking_this {
                        egui::Color32::from_rgb(180, 83, 9)
                    } else {
                        egui::Color32::from_rgb(30, 58, 138)
                    })
                    .min_size(egui::vec2(ui.available_width(), 24.0)),
                )
                .clicked()
            {
                if picking_this {
                    state.tool_mode = ToolMode::Select;
                    state.picking_model_action_for_anim_id = None;
                } else {
                    state.picking_model_action_for_anim_id = Some(anim.id);
                    state.tool_mode = ToolMode::PickModelActionMarker;
                    state.status_banner = Some(format!(
                        "Klik objek 3D di peta untuk target '{}' (Esc batal).",
                        anim.name
                    ));
                }
            }

            ui.add_space(8.0);
            ui.label("Klip Aksi Animasi:");
            if let Some(m) = cur_marker.as_ref() {
                let actions = available_actions_for_model(state, &m.model_path);
                if actions.is_empty() {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(45, 29, 14))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 83, 9)))
                        .corner_radius(5.0)
                        .inner_margin(egui::Margin::same(7))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Model ini belum memiliki klip aksi tambahan.\nBuka menu Assets -> 3D Models, pilih model ini, lalu klik '+ Tambah Aksi' untuk mengunggah file .glb animasi (mis. Melambai, Menembak, Pingsan).",
                                )
                                .size(10.5)
                                .color(egui::Color32::from_rgb(253, 230, 138)),
                            );
                        });
                } else {
                    let cur_act_lbl = actions
                        .iter()
                        .find(|(s, _)| s == &cfg.action_slug)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_else(|| {
                            if cfg.action_slug.is_empty() {
                                "-- Pilih Klip Aksi --".to_string()
                            } else {
                                cfg.action_slug.clone()
                            }
                        });
                    egui::ComboBox::from_id_salt(("model_action_clip", anim.id))
                        .selected_text(cur_act_lbl)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (slug, name) in &actions {
                                ui.selectable_value(&mut cfg.action_slug, slug.clone(), name);
                            }
                        });
                }
            } else {
                ui.label(
                    egui::RichText::new("Pilih Objek 3D terlebih dahulu untuk melihat daftar klip aksinya.")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
        });
    });

    // Card 2: Playback Mode & Timing
    inspector_card(ui, "Mode Putar & Kecepatan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Mode Putar Klip:");
            for &mode in ModelActionPlayMode::ALL {
                let selected = cfg.play_mode == mode;
                if ui
                    .selectable_label(selected, mode.label())
                    .on_hover_text(mode.description())
                    .clicked()
                {
                    cfg.play_mode = mode;
                }
            }
            ui.label(
                egui::RichText::new(cfg.play_mode.description())
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );

            if cfg.play_mode == ModelActionPlayMode::RepeatCountReturnIdle {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Jumlah Pengulangan:");
                    let mut rep_f = cfg.repeat_count as f64;
                    numeric_spinner_w(
                        ui,
                        ("model_act_repeat", anim.id),
                        &mut rep_f,
                        78.0,
                        SpinnerConfig {
                            min: 1.0,
                            max: 50.0,
                            step: 1.0,
                            drag_speed: 0.2,
                            decimals: 0,
                            suffix: "×",
                            wrap_around: false,
                        },
                    );
                    cfg.repeat_count = rep_f.round().clamp(1.0, 50.0) as u32;
                });
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Kecepatan Animasi:");
                numeric_spinner_w(
                    ui,
                    ("model_act_speed", anim.id),
                    &mut cfg.speed,
                    82.0,
                    SpinnerConfig {
                        min: 0.15,
                        max: 4.0,
                        step: 0.1,
                        drag_speed: 0.05,
                        decimals: 2,
                        suffix: "×",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label("Jeda Awal (Delay):");
                numeric_spinner_w(
                    ui,
                    ("model_act_delay", anim.id),
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
            ui.add_space(4.0);
            ui.checkbox(
                &mut cfg.stop_on_route_move,
                "Otomatis Stop & pindah ke Walk saat Rute berjalan",
            )
            .on_hover_text(
                "Direkomendasikan aktif: ketika objek ini mulai bergerak di Rute / Formasi / RTS, aksi otomatis berhenti dan beralih ke animasi jalan (Walk).",
            );
        });
    });

    if cfg != orig_cfg || anim.marker_id != orig_marker_id {
        write_model_action_config(anim, &cfg);
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelActionPlayAction {
    Play,
    Pause,
    Resume,
    Stop,
    Focus,
}

/// 100% uniform Play Panel card matching all other animation cards (`#N`, `[AKSI 3D]`,
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
) -> Option<ModelActionPlayAction> {
    let cfg = parse_model_action_config(anim);
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
            "AKSI AKTIF",
            egui::Color32::from_rgb(120, 53, 15),
            egui::Color32::from_rgb(253, 230, 138),
            egui::Color32::from_rgb(245, 158, 11),
        ),
        AnimRunStatus::Paused => (
            "DIJEDA",
            egui::Color32::from_rgb(120, 53, 15),
            egui::Color32::from_rgb(253, 230, 138),
            egui::Color32::from_rgb(245, 158, 11),
        ),
        AnimRunStatus::Finished => (
            if cfg.play_mode == ModelActionPlayMode::OnceHoldLastFrame {
                "TAHAN POSE"
            } else {
                "SELESAI"
            },
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

            // Row 1: #Order + [AKSI 3D] + Name + Status + Go
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("#{}", global_idx + 1))
                        .size(10.0)
                        .strong()
                        .color(egui::Color32::from_rgb(125, 211, 252)),
                );
                ui.label(
                    egui::RichText::new("[AKSI 3D]")
                        .size(9.5)
                        .strong()
                        .color(egui::Color32::from_rgb(251, 191, 36)),
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
                            anim.marker_id.is_some(),
                            egui::Button::new(
                                egui::RichText::new("Go")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            )
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Fokus kamera ke objek 3D")
                        .clicked()
                    {
                        action = Some(ModelActionPlayAction::Focus);
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

            // Info line 1: Target marker & clip summary
            ui.add(
                egui::Label::new(
                    egui::RichText::new(anim_summary(state, anim))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                )
                .truncate(),
            );

            // Info line 2: Live status or mode hint
            let detail_line = if status != AnimRunStatus::Stopped
                && let Some(live) = state.model_action_status.get(&anim.id)
            {
                live.clone()
            } else {
                let speed_txt = if (cfg.speed - 1.0).abs() > 0.01 {
                    format!(" · {:.2}×", cfg.speed)
                } else {
                    String::new()
                };
                format!("Mode: {}{speed_txt}", cfg.play_mode.label())
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
                } else if status == AnimRunStatus::Playing {
                    1.0
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
                        action = Some(ModelActionPlayAction::Play);
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
                            action = Some(ModelActionPlayAction::Pause);
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
                            action = Some(ModelActionPlayAction::Stop);
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
                            action = Some(ModelActionPlayAction::Resume);
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
                            action = Some(ModelActionPlayAction::Stop);
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
                            action = Some(ModelActionPlayAction::Play);
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
                            action = Some(ModelActionPlayAction::Stop);
                        }
                    });
                }
            }
        });
    ui.add_space(6.0);
    action
}
