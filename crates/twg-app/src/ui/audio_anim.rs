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
    card(ui, |ui| {
        ui.strong("Play Suara · Sound");
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama animasi");
            ui.add(egui::TextEdit::singleline(&mut anim.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);
            ui.label(format!("Sound: {}", sound_name(state, anim)));
            ui.add(
                egui::TextEdit::singleline(&mut state.audio_asset_search)
                    .hint_text("Cari Sound dari Assets...")
                    .desired_width(f32::INFINITY),
            );
            let search = state.audio_asset_search.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt(("audio_assets", anim.id))
                .max_height(170.0)
                .show(ui, |ui| {
                    let mut found = false;
                    for asset in &state.media_assets {
                        if asset.media_type != MediaAssetType::Sound
                            || !asset.name.to_lowercase().contains(&search)
                        {
                            continue;
                        }
                        found = true;
                        let duration = asset
                            .duration_sec
                            .map(|d| format!("{d:.1} dtk"))
                            .unwrap_or_else(|| "durasi saat preview".into());
                        if ui
                            .selectable_label(
                                cfg.asset_id == Some(asset.id),
                                format!("{} · {duration}", asset.name),
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
                        ui.label("Tidak ada Sound. Upload terlebih dahulu di halaman Assets.");
                    }
                });
            ui.checkbox(&mut anim.autoplay, "Autoplay saat skenario dimulai");
        });
    });
    let duration = state
        .media_assets
        .iter()
        .find(|m| Some(m.id) == cfg.asset_id)
        .and_then(|m| m.duration_sec);
    card(ui, |ui| {
        ui.strong("Pemutaran");
        ui.add_enabled_ui(editable, |ui| {
            let mut volume = cfg.volume * 100.0;
            if ui.add(egui::Slider::new(&mut volume, 0.0..=100.0).text("Volume %")).changed() { cfg.volume = volume / 100.0; }
            egui::ComboBox::from_id_salt(("audio_repeat", anim.id))
                .selected_text(match cfg.repeat { AudioRepeat::Once => "Sekali", AudioRepeat::Count => "Jumlah tertentu", AudioRepeat::Forever => "Sampai dihentikan" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut cfg.repeat, AudioRepeat::Once, "Sekali");
                    ui.selectable_value(&mut cfg.repeat, AudioRepeat::Count, "Jumlah tertentu");
                    ui.selectable_value(&mut cfg.repeat, AudioRepeat::Forever, "Sampai dihentikan");
                });
            if cfg.repeat == AudioRepeat::Count {
                ui.horizontal(|ui| { ui.label("Total pemutaran"); ui.add(egui::DragValue::new(&mut cfg.repeat_count).range(1..=10000)); });
            }
            if cfg.repeat == AudioRepeat::Forever {
                ui.colored_label(egui::Color32::from_rgb(251, 191, 36), "Sequence dengan Tunggu Selesai akan menunggu sampai suara dihentikan. Gunakan pemicu jeda waktu untuk lanjut otomatis.");
            }
            seconds(ui, "Jeda awal", &mut cfg.delay_sec);
            seconds(ui, "Mulai dari", &mut cfg.clip_start_sec);
            let mut until_end = cfg.clip_end_sec.is_none();
            if ui.checkbox(&mut until_end, "Sampai akhir file").changed() {
                cfg.clip_end_sec = if until_end { None } else { Some(duration.unwrap_or(cfg.clip_start_sec + 1.0)) };
            }
            if let Some(end) = &mut cfg.clip_end_sec { seconds(ui, "Berakhir pada", end); }
            seconds(ui, "Fade in", &mut cfg.fade_in_sec);
            seconds(ui, "Fade out", &mut cfg.fade_out_sec);
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

fn seconds(ui: &mut egui::Ui, name: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(name);
        ui.add(
            egui::DragValue::new(value)
                .range(0.0..=86400.0)
                .speed(0.1)
                .suffix(" dtk"),
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
    card(ui, |ui| {
        ui.colored_label(
            egui::Color32::from_rgb(192, 132, 252),
            format!("{} · AUDIO / PLAY SUARA", index + 1),
        );
        ui.strong(&anim.name);
        ui.label(sound_name(state, anim));
        let status = playback.track_status(anim.id);
        let cfg = parse_audio_anim_config(anim);
        ui.label(
            state
                .audio_status
                .get(&anim.id)
                .map(String::as_str)
                .unwrap_or(match status {
                    AnimRunStatus::Playing => "Menyiapkan...",
                    AnimRunStatus::Paused => "Dijeda",
                    AnimRunStatus::Finished => "Selesai",
                    AnimRunStatus::Stopped => "Siap",
                }),
        );
        if let Some(track) = playback.tracks.get(&anim.id) {
            let progress = if track.total_length_m > 0.0 {
                (track.distance_m / track.total_length_m).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(progress).desired_width(ui.available_width()));
        }
        ui.horizontal(|ui| {
            match status {
                AnimRunStatus::Stopped | AnimRunStatus::Finished => {
                    if ui
                        .add_enabled(
                            cfg.asset_id.is_some(),
                            egui::Button::new("Play").fill(egui::Color32::from_rgb(5, 150, 105)),
                        )
                        .clicked()
                    {
                        action = Some(AudioAction::Play);
                    }
                }
                AnimRunStatus::Playing => {
                    if ui.button("Pause").clicked() {
                        action = Some(AudioAction::Pause);
                    }
                }
                AnimRunStatus::Paused => {
                    if ui.button("Resume").clicked() {
                        action = Some(AudioAction::Resume);
                    }
                }
            }
            if status != AnimRunStatus::Stopped && ui.button("Stop").clicked() {
                action = Some(AudioAction::Stop);
            }
        });
    });
    action
}

pub fn draw_output_toggle(ui: &mut egui::Ui, state: &mut AppState) {
    if ui.checkbox(&mut state.scenario_audio_enabled, "Suara skenario di perangkat ini")
        .on_hover_text("Aktifkan hanya pada perangkat yang menjadi speaker agar Tactical dan Layar Besar tidak berbunyi ganda. Preview Inspector tetap bersuara.").changed()
        && let Err(error) = crate::settings::save_audio_output(state.scenario_audio_enabled) {
        state.status_banner = Some(error);
    }
}
