use crate::{
    net::client::{ApiClientBridge, ApiCommand},
    scene::markers::current_auth,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, ToolMode},
};
use bevy_egui::egui;
use twg_shared::{
    api::CreateAnimRequest,
    audio_anim::{
        ANIM_TYPE_PLAY_SOUND, AudioAnimConfig, AudioRepeat, parse_audio_anim_config,
        write_audio_anim_config,
    },
    models::{Anim, MediaAssetType},
};
use uuid::Uuid;

use super::{
    right_panel::inspector_card,
    spinner::{SpinnerConfig, numeric_spinner_u32_w, numeric_spinner_w},
};

fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 62, 92)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
    ui.add_space(6.0);
}

pub fn draw_create(ui: &mut egui::Ui, state: &mut AppState, api: &ApiClientBridge, editable: bool) {
    card(ui, |ui| {
        ui.strong("Audio · Play Suara");
        ui.label("Putar Sound dari Assets sebagai bagian dari skenario atau Grup.");
        let button = egui::Button::new("+ Play Suara")
            .fill(egui::Color32::from_rgb(109, 40, 217))
            .min_size(egui::vec2(ui.available_width(), 28.0));
        if ui.add_enabled(editable, button).clicked()
            && let Some(sk) = state.active_skenario.clone()
        {
            let number = state
                .anims
                .iter()
                .filter(|a| a.anim_type == ANIM_TYPE_PLAY_SOUND)
                .count()
                + 1;
            let now = chrono::Utc::now();
            let anim = Anim {
                id: Uuid::new_v4(),
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                marker_id: None,
                name: format!("Play Suara {number}"),
                anim_type: ANIM_TYPE_PLAY_SOUND.into(),
                sort_order: (state.anims.len() + 1) as i32,
                speed: 1.0,
                duration_sec: 0.0,
                replay: false,
                autoplay: false,
                geom: Vec::new(),
                data: serde_json::json!({"audio": AudioAnimConfig::default()}),
                created_by: state.current_user_id(),
                allowed_user_ids: Vec::new(),
                created_at: now,
                updated_at: now,
            };
            state.tool_mode = ToolMode::Select;
            state.draft_points_lnglat.clear();
            state.picking_marker_for_anim_id = None;
            state.picking_shoot_for_anim_id = None;
            state.picking_pointer_for_anim_id = None;
            state.picking_spawn_hide_for_anim_id = None;
            state.audio_asset_search.clear();
            if state.offline_preview {
                state.selected = SelectedEntity::Anim(anim.id);
                state.anim_edit = Some(anim.clone());
                state.marker_edit = None;
                state.building_edit = None;
                state.line_edit = None;
                state.text_edit = None;
                state.anims.push(anim);
                state.sort_anims_by_order();
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::CreateAnim {
                    auth,
                    req: CreateAnimRequest {
                        operasi_id: anim.operasi_id,
                        skenario_id: anim.skenario_id,
                        marker_id: None,
                        name: anim.name,
                        anim_type: anim.anim_type,
                        sort_order: anim.sort_order,
                        speed: 1.0,
                        duration_sec: 0.0,
                        replay: false,
                        autoplay: false,
                        geom: vec![],
                        data: anim.data,
                        allowed_user_ids: Vec::new(),
                    },
                });
            }
        }
    });
}

pub fn sound_name(state: &AppState, anim: &Anim) -> String {
    let cfg = parse_audio_anim_config(anim);
    state
        .media_assets
        .iter()
        .find(|m| Some(m.id) == cfg.asset_id)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Belum memilih Sound / aset tidak tersedia".into())
}

pub fn draw_inspector(ui: &mut egui::Ui, state: &mut AppState, anim: &mut Anim, editable: bool) {
    let mut cfg = parse_audio_anim_config(anim);
    let original = cfg.clone();
    inspector_card(ui, "Play Suara · Sound", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama animasi");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);
            ui.label("Sound dari Assets");
            let selected = state
                .media_assets
                .iter()
                .find(|m| Some(m.id) == cfg.asset_id && m.media_type == MediaAssetType::Sound);
            let selected_text = selected
                .map(|m| m.name.as_str())
                .unwrap_or("Pilih Sound...");
            egui::ComboBox::from_id_salt(("audio_asset", anim.id))
                .width(ui.available_width())
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    let mut found = false;
                    for asset in &state.media_assets {
                        if asset.media_type != MediaAssetType::Sound {
                            continue;
                        }
                        found = true;
                        let duration = asset
                            .duration_sec
                            .map(|d| format!(" · {d:.1} dtk"))
                            .unwrap_or_default();
                        if ui
                            .selectable_label(
                                cfg.asset_id == Some(asset.id),
                                format!("{}{duration}", asset.name),
                            )
                            .clicked()
                            && cfg.asset_id != Some(asset.id)
                        {
                            cfg.asset_id = Some(asset.id);
                            cfg.clip_start_sec = 0.0;
                            cfg.clip_end_sec = None;
                        }
                    }
                    if !found {
                        ui.label("Belum ada Sound — upload di halaman Assets.");
                    }
                });
            if let Some(asset) = selected {
                if let Some(seconds) = asset.duration_sec {
                    ui.small(format!("Durasi file: {seconds:.1} dtk"));
                }
            }
            ui.checkbox(&mut anim.autoplay, "Autoplay saat skenario dimulai");
        });
    });
    let duration = state
        .media_assets
        .iter()
        .find(|m| Some(m.id) == cfg.asset_id)
        .and_then(|m| m.duration_sec);
    inspector_card(ui, "Pemutaran", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            let mut volume = cfg.volume as f64 * 100.0;
            seconds_with_range(ui, ("audio_volume", anim.id), "Volume", &mut volume, 0.0, 100.0, 1.0, " %");
            cfg.volume = (volume / 100.0) as f32;
            ui.label("Pengulangan");
            egui::ComboBox::from_id_salt(("audio_repeat", anim.id))
                .width(ui.available_width())
                .selected_text(match cfg.repeat { AudioRepeat::Once => "Sekali", AudioRepeat::Count => "Jumlah tertentu", AudioRepeat::Forever => "Sampai dihentikan" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut cfg.repeat, AudioRepeat::Once, "Sekali");
                    ui.selectable_value(&mut cfg.repeat, AudioRepeat::Count, "Jumlah tertentu");
                    ui.selectable_value(&mut cfg.repeat, AudioRepeat::Forever, "Sampai dihentikan");
                });
            if cfg.repeat == AudioRepeat::Count {
                ui.horizontal(|ui| {
                    ui.label("Total pemutaran");
                    let width = ui.available_width().max(78.0);
                    numeric_spinner_u32_w(ui, ("audio_repeat_count", anim.id), &mut cfg.repeat_count, width, 1, 10000, 1, 0.5);
                });
            }
            if cfg.repeat == AudioRepeat::Forever {
                ui.colored_label(egui::Color32::from_rgb(251, 191, 36), "Sequence dengan Tunggu Selesai akan menunggu sampai suara dihentikan. Gunakan pemicu jeda waktu untuk lanjut otomatis.");
            }
            seconds(ui, ("audio_delay", anim.id), "Jeda awal", &mut cfg.delay_sec);
            seconds(ui, ("audio_start", anim.id), "Mulai dari", &mut cfg.clip_start_sec);
            let mut until_end = cfg.clip_end_sec.is_none();
            if ui.checkbox(&mut until_end, "Sampai akhir file").changed() {
                cfg.clip_end_sec = if until_end { None } else { Some(duration.unwrap_or(cfg.clip_start_sec + 1.0)) };
            }
            if let Some(end) = &mut cfg.clip_end_sec { seconds(ui, ("audio_end", anim.id), "Berakhir pada", end); }
            seconds(ui, ("audio_fade_in", anim.id), "Fade in", &mut cfg.fade_in_sec);
            seconds(ui, ("audio_fade_out", anim.id), "Fade out", &mut cfg.fade_out_sec);
        });
        if let Some(duration) = duration {
            match cfg.clip_duration(duration) {
                None => {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        "Awal potongan harus lebih kecil dari akhir dan durasi file.",
                    );
                }
                Some(clip) => {
                    let total = cfg
                        .total_duration(duration)
                        .map(|v| format!("{v:.1} dtk"))
                        .unwrap_or_else(|| "tanpa batas".into());
                    ui.label(format!("Potongan: {clip:.1} dtk · Total: {total}"));
                }
            }
        }
        ui.small("Suara global; kecepatan dan nada tetap normal saat kecepatan skenario berubah. Fade berlaku pada setiap putaran.");
        ui.small(
            "Pengaturan berlaku pada Play berikutnya. Klik Simpan untuk menyimpannya ke server.",
        );
    });
    if editable && cfg != original {
        cfg.sanitize();
        anim.duration_sec = duration.and_then(|d| cfg.total_duration(d)).unwrap_or(0.0);
        anim.replay = false;
        write_audio_anim_config(anim, &cfg);
    }
    if let Some(status) = state.audio_status.get(&anim.id) {
        ui.label(status);
    }
}

fn seconds(ui: &mut egui::Ui, id: impl egui::AsIdSalt, name: &str, value: &mut f64) {
    seconds_with_range(ui, id, name, value, 0.0, 86400.0, 0.1, " dtk");
}

fn seconds_with_range(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    name: &str,
    value: &mut f64,
    min: f64,
    max: f64,
    step: f64,
    suffix: &'static str,
) {
    ui.horizontal(|ui| {
        ui.label(name);
        let width = ui.available_width().max(78.0);
        numeric_spinner_w(
            ui,
            id,
            value,
            width,
            SpinnerConfig {
                min,
                max,
                step,
                drag_speed: step,
                decimals: if step < 1.0 { 1 } else { 0 },
                suffix,
                wrap_around: false,
            },
        );
    });
}

pub enum AudioAction {
    Play,
    Pause,
    Resume,
    Stop,
}

pub fn draw_play_card(
    ui: &mut egui::Ui,
    state: &AppState,
    playback: &PlaybackState,
    anim: &Anim,
    index: usize,
) -> Option<AudioAction> {
    let mut action = None;
    let status = playback.track_status(anim.id);
    let accent = match status {
        AnimRunStatus::Playing => egui::Color32::from_rgb(16, 185, 129),
        AnimRunStatus::Paused => egui::Color32::from_rgb(245, 158, 11),
        _ => egui::Color32::from_rgb(44, 62, 92),
    };
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .stroke(egui::Stroke::new(1.2, accent))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("#{}", index + 1))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.label(
                    egui::RichText::new("AUDIO / PLAY SUARA")
                        .size(9.5)
                        .strong()
                        .color(egui::Color32::from_rgb(192, 132, 252)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, color) = match status {
                        AnimRunStatus::Playing => {
                            ("BERJALAN", egui::Color32::from_rgb(110, 231, 183))
                        }
                        AnimRunStatus::Paused => ("DIJEDA", egui::Color32::from_rgb(253, 230, 138)),
                        AnimRunStatus::Finished => {
                            ("SELESAI", egui::Color32::from_rgb(191, 219, 254))
                        }
                        AnimRunStatus::Stopped => ("SIAP", egui::Color32::from_rgb(148, 163, 184)),
                    };
                    ui.label(egui::RichText::new(label).size(9.0).strong().color(color));
                });
            });
            ui.label(
                egui::RichText::new(&anim.name)
                    .size(12.5)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
            ui.label(
                egui::RichText::new(sound_name(state, anim))
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            let cfg = parse_audio_anim_config(anim);
            if status != AnimRunStatus::Stopped {
                if let Some(message) = state.audio_status.get(&anim.id) {
                    ui.label(
                        egui::RichText::new(message)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(186, 230, 253)),
                    );
                }
            }
            let progress = playback.tracks.get(&anim.id)
                .filter(|track| track.total_length_m > 0.0)
                .map(|track| (track.distance_m / track.total_length_m).clamp(0.0, 1.0) as f32)
                .unwrap_or(0.0);
            super::play_panel::draw_card_progress_bar(ui, status, progress);
            ui.add_space(6.0);
            if matches!(status, AnimRunStatus::Stopped | AnimRunStatus::Finished) {
                let button = egui::Button::new(
                    egui::RichText::new("Play")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(5, 150, 105))
                .min_size(egui::vec2(ui.available_width(), 28.0));
                if ui.add_enabled(cfg.asset_id.is_some(), button).clicked() {
                    action = Some(AudioAction::Play);
                }
            } else {
                ui.columns(2, |cols| {
                    match status {
                        AnimRunStatus::Playing => {
                            if cols[0]
                                .add(
                                    egui::Button::new("Pause")
                                        .fill(egui::Color32::from_rgb(180, 83, 9))
                                        .min_size(egui::vec2(cols[0].available_width(), 28.0)),
                                )
                                .clicked()
                            {
                                action = Some(AudioAction::Pause);
                            }
                        }
                        AnimRunStatus::Paused => {
                            if cols[0]
                                .add(
                                    egui::Button::new("Resume")
                                        .fill(egui::Color32::from_rgb(5, 150, 105))
                                        .min_size(egui::vec2(cols[0].available_width(), 28.0)),
                                )
                                .clicked()
                            {
                                action = Some(AudioAction::Resume);
                            }
                        }
                        _ => {}
                    }
                    if cols[1]
                        .add(
                            egui::Button::new("Stop")
                                .fill(egui::Color32::from_rgb(185, 28, 28))
                                .min_size(egui::vec2(cols[1].available_width(), 28.0)),
                        )
                        .clicked()
                    {
                        action = Some(AudioAction::Stop);
                    }
                });
            }
            if status == AnimRunStatus::Finished && ui.small_button("Stop / Reset").clicked() {
                action = Some(AudioAction::Stop);
            }
        });
    ui.add_space(6.0);
    action
}

pub fn draw_output_toggle(ui: &mut egui::Ui, state: &mut AppState) {
    if ui.checkbox(&mut state.scenario_audio_enabled, "Suara skenario di perangkat ini")
        .on_hover_text("Aktifkan hanya pada perangkat yang menjadi speaker agar Tactical dan Layar Besar tidak berbunyi ganda. Preview Inspector tetap bersuara.").changed()
        && let Err(error) = crate::settings::save_audio_output(state.scenario_audio_enabled) {
        state.status_banner = Some(error);
    }
}
