use std::path::PathBuf;

use bevy_egui::egui;
use twg_shared::models::{MediaAssetDto, MediaAssetType, ModelAssetDto, ModelAssetType};

use crate::{
    map::camera::TacticalCamera,
    net::client::{ApiClientBridge, ApiCommand, MediaAssetUploadPayload, ModelAssetUploadPayload},
    scene::{
        asset_preview::AssetPreviewStage,
        markers::{current_auth, spawn_marker_at_map_center},
        media_preview::MediaPreviewPlayer,
    },
    state::{AppState, AssetsSection, PlaybackState},
    ui::{
        assets_page::{
            draw_asset_file_stats_card, draw_asset_preview_viewport_hud, draw_preview_controls_card,
        },
        draw_search_input_with_reset,
        spinner::{SpinnerConfig, numeric_spinner},
    },
};

fn format_bytes(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.2} MB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_duration(sec: f32) -> String {
    let total = sec.max(0.0).floor() as u32;
    let m = total / 60;
    let s = total % 60;
    format!("{m:02}:{s:02}")
}

fn pick_gltf_file(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("3D glTF / GLB Model", &["glb", "gltf"])
        .pick_file()
}

fn pick_media_file(media_type: MediaAssetType) -> Option<PathBuf> {
    let dialog = match media_type {
        MediaAssetType::Texture => rfd::FileDialog::new()
            .set_title("Pilih Image Texture (PNG / JPG / WebP)")
            .add_filter("Image Texture", &["png", "jpg", "jpeg", "webp"]),
        MediaAssetType::Sound => rfd::FileDialog::new()
            .set_title("Pilih File Sound / Audio")
            .add_filter("Audio File", &["mp3", "wav", "ogg", "flac", "aac", "m4a"]),
        MediaAssetType::Video => rfd::FileDialog::new()
            .set_title("Pilih File Video")
            .add_filter("Video File", &["mp4", "webm", "mov", "mkv", "avi"]),
    };
    dialog.pick_file()
}

fn draw_compact_file_picker(
    ui: &mut egui::Ui,
    btn_label: &str,
    slot: &mut Option<PathBuf>,
    pick_fn: impl FnOnce() -> Option<PathBuf>,
) {
    ui.horizontal(|ui| {
        if ui.button(btn_label).clicked()
            && let Some(picked) = pick_fn()
        {
            *slot = Some(picked);
        }
        if slot.is_some() && ui.small_button("X").on_hover_text("Kosongkan file").clicked() {
            *slot = None;
        }
    });
    if let Some(p) = slot {
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("File: {fname}"))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(74, 222, 128)),
            )
            .truncate(),
        );
    }
}

/// Renders the `Import` tab inside the Plot Editor left dock.
/// Allows all non-big-screen roles to upload, preview, place, and manage assets scoped to the active Operasi.
#[allow(clippy::too_many_arguments)]
pub fn draw_import_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    playback: &PlaybackState,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
) {
    let editable = !playback.play_test_mode;
    let Some(active_op_id) = state.active_operasi_id() else {
        ui.colored_label(
            egui::Color32::from_rgb(250, 204, 21),
            "Buka skenario terlebih dahulu untuk menggunakan fitur Import Operasi.",
        );
        return;
    };

    // Header + Toggle Upload Form button
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Import Operasi")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
            ui.label(
                egui::RichText::new("Tersedia untuk semua role di Operasi ini")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let toggle_txt = if state.import_panel.show_upload_form {
                "Tutup Form"
            } else {
                "+ Upload"
            };
            let btn_fill = if state.import_panel.show_upload_form {
                egui::Color32::from_rgb(51, 65, 85)
            } else {
                egui::Color32::from_rgb(217, 119, 6)
            };
            if ui
                .add_enabled(
                    editable && !state.offline_preview,
                    egui::Button::new(
                        egui::RichText::new(toggle_txt)
                            .size(10.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(btn_fill),
                )
                .on_hover_text("Upload aset 3D, Texture, Sound, atau Video khusus Operasi ini")
                .clicked()
            {
                state.import_panel.show_upload_form = !state.import_panel.show_upload_form;
                state.import_panel.error = None;
                state.import_panel.success = None;
                if state.import_panel.show_upload_form
                    && !state
                        .model_categories
                        .iter()
                        .any(|c| c.category == state.import_panel.model_category)
                    && let Some(first_cat) = state.model_categories.first()
                {
                    state.import_panel.model_category = first_cat.category.clone();
                }
            }
        });
    });

    if state.offline_preview {
        ui.add_space(4.0);
        ui.colored_label(
            egui::Color32::from_rgb(250, 204, 21),
            "Mode Offline Preview: upload Import membutuhkan koneksi server.",
        );
    }

    ui.add_space(5.0);

    // Count operasi-scoped assets per type
    let cnt_3d = state
        .model_assets
        .iter()
        .filter(|m| m.operasi_id == Some(active_op_id))
        .count();
    let cnt_tex = state
        .media_assets
        .iter()
        .filter(|m| m.operasi_id == Some(active_op_id) && m.media_type == MediaAssetType::Texture)
        .count();
    let cnt_snd = state
        .media_assets
        .iter()
        .filter(|m| m.operasi_id == Some(active_op_id) && m.media_type == MediaAssetType::Sound)
        .count();
    let cnt_vid = state
        .media_assets
        .iter()
        .filter(|m| m.operasi_id == Some(active_op_id) && m.media_type == MediaAssetType::Video)
        .count();

    let prev_sec = state.import_panel.section;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.selectable_value(
            &mut state.import_panel.section,
            AssetsSection::Models3d,
            format!("3D ({cnt_3d})"),
        );
        ui.selectable_value(
            &mut state.import_panel.section,
            AssetsSection::Texture,
            format!("Texture ({cnt_tex})"),
        );
        ui.selectable_value(
            &mut state.import_panel.section,
            AssetsSection::Sound,
            format!("Sound ({cnt_snd})"),
        );
        ui.selectable_value(
            &mut state.import_panel.section,
            AssetsSection::Video,
            format!("Video ({cnt_vid})"),
        );
    });

    if state.import_panel.section != prev_sec {
        media_player.stop_all_playback();
        state.import_panel.confirm_delete_id = None;
        state.import_panel.error = None;
        state.import_panel.success = None;
        // If preview was open, switch preview smoothly to first asset of new section (or close)
        if state.import_panel.preview_open {
            match state.import_panel.section {
                AssetsSection::Models3d => {
                    if let Some(first) = state
                        .model_assets
                        .iter()
                        .find(|m| m.operasi_id == Some(active_op_id))
                        .map(|m| m.id)
                    {
                        state.set_plot_import_preview_model(first);
                    } else {
                        state.import_panel.close_preview();
                    }
                }
                sec => {
                    if let Some(mt) = sec.media_type()
                        && let Some(first) = state
                            .media_assets
                            .iter()
                            .find(|m| m.operasi_id == Some(active_op_id) && m.media_type == mt)
                            .map(|m| m.id)
                    {
                        state.set_plot_import_preview_media(first, mt);
                    } else {
                        state.import_panel.close_preview();
                    }
                }
            }
        }
    }

    if state.import_panel.preview_open {
        ui.add_space(4.0);
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("< Kembali ke Peta (Tutup Preview)")
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(14, 116, 144))
                .min_size(egui::vec2(ui.available_width(), 24.0)),
            )
            .clicked()
        {
            media_player.stop_all_playback();
            state.import_panel.close_preview();
        }
    }

    if let Some(err) = state.import_panel.error.clone() {
        ui.add_space(4.0);
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
    }
    if let Some(ok) = state.import_panel.success.clone() {
        ui.add_space(4.0);
        ui.colored_label(egui::Color32::from_rgb(74, 222, 128), ok);
    }

    // Upload Form Card
    if state.import_panel.show_upload_form && !state.offline_preview {
        ui.add_space(5.0);
        draw_import_upload_form(ui, state, active_op_id, api);
    }

    ui.add_space(5.0);
    draw_search_input_with_reset(
        ui,
        &mut state.import_panel.search,
        "Cari aset import operasi ini...",
        false,
        "Reset pencarian import",
    );
    ui.separator();

    match state.import_panel.section {
        AssetsSection::Models3d => {
            draw_import_3d_list(ui, state, active_op_id, tcam, editable, media_player, api);
        }
        sec => {
            if let Some(mt) = sec.media_type() {
                draw_import_media_list(ui, state, active_op_id, mt, media_player, api);
            }
        }
    }
}

fn draw_import_upload_form(
    ui: &mut egui::Ui,
    state: &mut AppState,
    active_op_id: uuid::Uuid,
    api: &ApiClientBridge,
) {
    let section = state.import_panel.section;
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 24, 38))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 83, 9)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            match section {
                AssetsSection::Models3d => {
                    ui.label(
                        egui::RichText::new("Upload Model 3D (.glb / .gltf)")
                            .size(11.5)
                            .strong()
                            .color(egui::Color32::from_rgb(253, 224, 71)),
                    );
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Nama Model:").size(10.5));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.import_panel.model_name)
                            .hint_text("Contoh: Posko Taktis Op A")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Kategori:").size(10.5));
                    let cats = state.model_categories.clone();
                    let prev_cat = state.import_panel.model_category.clone();
                    egui::ComboBox::from_id_salt("import_3d_cat_cb")
                        .selected_text(&state.import_panel.model_category)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for c in &cats {
                                ui.selectable_value(
                                    &mut state.import_panel.model_category,
                                    c.category.clone(),
                                    c.category.as_str(),
                                );
                            }
                        });
                    if state.import_panel.model_category != prev_cat {
                        state.import_panel.model_type =
                            if state.category_supports_idle_walk(&state.import_panel.model_category)
                            {
                                ModelAssetType::Animated
                            } else {
                                ModelAssetType::Static
                            };
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut state.import_panel.model_type,
                            ModelAssetType::Static,
                            "Statis",
                        );
                        ui.radio_value(
                            &mut state.import_panel.model_type,
                            ModelAssetType::Animated,
                            "Animasi (Idle+Walk)",
                        );
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Tinggi Default:").size(10.5));
                        numeric_spinner(
                            ui,
                            "import_3d_height",
                            &mut state.import_panel.model_default_height,
                            SpinnerConfig {
                                min: 0.1,
                                max: 200.0,
                                step: 0.5,
                                drag_speed: 0.1,
                                decimals: 1,
                                suffix: " m",
                                wrap_around: false,
                            },
                        );
                    });
                    ui.add_space(4.0);
                    match state.import_panel.model_type {
                        ModelAssetType::Static => {
                            draw_compact_file_picker(
                                ui,
                                "Pilih Model (.glb/.gltf)",
                                &mut state.import_panel.model_file_path,
                                || pick_gltf_file("Pilih File Model 3D (.glb/.gltf)"),
                            );
                        }
                        ModelAssetType::Animated => {
                            ui.label(
                                egui::RichText::new("1. Idle (tanpa animasi):")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            );
                            draw_compact_file_picker(
                                ui,
                                "Pilih Idle (*_idle)...",
                                &mut state.import_panel.idle_file_path,
                                || pick_gltf_file("Pilih File Idle (.glb/.gltf)"),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new("2. Walk (dengan animasi jalan):")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(192, 132, 252)),
                            );
                            draw_compact_file_picker(
                                ui,
                                "Pilih Walk (*_walk)...",
                                &mut state.import_panel.walk_file_path,
                                || pick_gltf_file("Pilih File Walk (.glb/.gltf)"),
                            );
                        }
                    }

                    ui.add_space(6.0);
                    let lbl = if state.import_panel.submitting {
                        "Mengunggah & Konversi..."
                    } else {
                        "Upload 3D ke Operasi Ini"
                    };
                    if ui
                        .add_enabled(
                            !state.import_panel.submitting,
                            egui::Button::new(
                                egui::RichText::new(lbl)
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105))
                            .min_size(egui::vec2(ui.available_width(), 26.0)),
                        )
                        .clicked()
                    {
                        let name = state.import_panel.model_name.trim().to_string();
                        if name.is_empty() {
                            state.import_panel.error = Some("Nama model wajib diisi.".into());
                        } else if state.import_panel.model_type == ModelAssetType::Static
                            && state.import_panel.model_file_path.is_none()
                        {
                            state.import_panel.error =
                                Some("Pilih file model 3D (.glb/.gltf) terlebih dahulu.".into());
                        } else if state.import_panel.model_type == ModelAssetType::Animated
                            && (state.import_panel.idle_file_path.is_none()
                                || state.import_panel.walk_file_path.is_none())
                        {
                            state.import_panel.error = Some(
                                "Model Animasi wajib memilih file Idle dan file Walk.".into(),
                            );
                        } else if let Some(auth) = current_auth(state) {
                            state.import_panel.submitting = true;
                            state.import_panel.error = None;
                            state.import_panel.success = None;
                            api.send(ApiCommand::CreateModelAsset {
                                auth,
                                payload: ModelAssetUploadPayload {
                                    name,
                                    category: state.import_panel.model_category.clone(),
                                    operasi_id: Some(active_op_id),
                                    model_type: state.import_panel.model_type,
                                    default_height: state.import_panel.model_default_height,
                                    default_rot_x: 0.0,
                                    default_rot_y: 0.0,
                                    default_rot_z: 0.0,
                                    model_file_path: if state.import_panel.model_type
                                        == ModelAssetType::Static
                                    {
                                        state.import_panel.model_file_path.clone()
                                    } else {
                                        None
                                    },
                                    idle_file_path: if state.import_panel.model_type
                                        == ModelAssetType::Animated
                                    {
                                        state.import_panel.idle_file_path.clone()
                                    } else {
                                        None
                                    },
                                    walk_file_path: if state.import_panel.model_type
                                        == ModelAssetType::Animated
                                    {
                                        state.import_panel.walk_file_path.clone()
                                    } else {
                                        None
                                    },
                                },
                            });
                        }
                    }
                }
                sec => {
                    let Some(mt) = sec.media_type() else {
                        return;
                    };
                    ui.label(
                        egui::RichText::new(format!("Upload {} ke Operasi", mt.label()))
                            .size(11.5)
                            .strong()
                            .color(egui::Color32::from_rgb(253, 224, 71)),
                    );
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Nama Asset:").size(10.5));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.import_panel.media_name)
                            .hint_text(format!("Contoh: {} Operasi", mt.label()))
                            .desired_width(f32::INFINITY),
                    );
                    if mt == MediaAssetType::Texture {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut state.import_panel.media_crop_mode,
                                "crop".to_string(),
                                "Center-Crop",
                            );
                            ui.radio_value(
                                &mut state.import_panel.media_crop_mode,
                                "resize".to_string(),
                                "Resize Fit",
                            );
                        });
                    }
                    ui.add_space(4.0);
                    draw_compact_file_picker(
                        ui,
                        &format!("Pilih File {}...", mt.label()),
                        &mut state.import_panel.media_file_path,
                        || pick_media_file(mt),
                    );
                    ui.add_space(6.0);
                    let lbl = if state.import_panel.submitting {
                        "Mengunggah...".to_string()
                    } else {
                        format!("Upload {} ke Operasi Ini", mt.label())
                    };
                    if ui
                        .add_enabled(
                            !state.import_panel.submitting,
                            egui::Button::new(
                                egui::RichText::new(lbl)
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105))
                            .min_size(egui::vec2(ui.available_width(), 26.0)),
                        )
                        .clicked()
                    {
                        let name = state.import_panel.media_name.trim().to_string();
                        if name.is_empty() {
                            state.import_panel.error = Some("Nama asset wajib diisi.".into());
                        } else if state.import_panel.media_file_path.is_none() {
                            state.import_panel.error =
                                Some(format!("Pilih file {} terlebih dahulu.", mt.label()));
                        } else if let Some(auth) = current_auth(state) {
                            state.import_panel.submitting = true;
                            state.import_panel.error = None;
                            state.import_panel.success = None;
                            api.send(ApiCommand::CreateMediaAsset {
                                auth,
                                payload: MediaAssetUploadPayload {
                                    name,
                                    media_type: mt,
                                    category: "operasi_import".to_string(),
                                    operasi_id: Some(active_op_id),
                                    crop_mode: state.import_panel.media_crop_mode.clone(),
                                    file_path: state.import_panel.media_file_path.clone(),
                                },
                            });
                        }
                    }
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_import_3d_list(
    ui: &mut egui::Ui,
    state: &mut AppState,
    active_op_id: uuid::Uuid,
    tcam: &TacticalCamera,
    editable: bool,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
) {
    let needle = state.import_panel.search.trim().to_lowercase();
    let items: Vec<ModelAssetDto> = state
        .model_assets
        .iter()
        .filter(|m| {
            if m.operasi_id != Some(active_op_id) {
                return false;
            }
            if !needle.is_empty()
                && !m.name.to_lowercase().contains(&needle)
                && !m.category.to_lowercase().contains(&needle)
            {
                return false;
            }
            true
        })
        .cloned()
        .collect();

    let mut preview_id: Option<uuid::Uuid> = None;
    let mut place_model: Option<ModelAssetDto> = None;
    let mut start_drag: Option<twg_shared::models::ModelCatalogEntry> = None;
    let mut delete_id: Option<uuid::Uuid> = None;
    let dragging_id = state.dragging_catalog_entry.as_ref().map(|e| e.id.clone());

    egui::ScrollArea::vertical()
        .id_salt("plot_import_3d_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if items.is_empty() {
                ui.add_space(14.0);
                ui.label(
                    egui::RichText::new(
                        "Belum ada Model 3D yang di-import di Operasi ini. Klik '+ Upload' di atas untuk menambahkan.",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                return;
            }

            ui.label(
                egui::RichText::new("Tarik (drag) kartu model ke peta 3D atau klik '+ Peta'")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            );
            ui.add_space(4.0);

            for asset in &items {
                let is_dragging = dragging_id.as_deref() == Some(asset.id.to_string().as_str());
                let is_previewing = state.is_plot_import_3d_preview_active()
                    && state.import_panel.preview_model_id == Some(asset.id);
                let ready = state
                    .available_model_paths
                    .contains(&asset.model_file.rel_path)
                    || state.assets_root.join(&asset.model_file.rel_path).exists();
                let can_delete =
                    state.can_manage_imported_asset(asset.operasi_id, asset.created_by);

                egui::Frame::new()
                    .fill(if is_dragging {
                        egui::Color32::from_rgb(12, 74, 110)
                    } else if is_previewing {
                        egui::Color32::from_rgb(14, 74, 102)
                    } else {
                        egui::Color32::from_rgb(24, 33, 49)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if is_dragging {
                            egui::Color32::from_rgb(34, 211, 238)
                        } else if is_previewing {
                            egui::Color32::from_rgb(56, 189, 248)
                        } else {
                            egui::Color32::from_rgb(44, 58, 82)
                        },
                    ))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        // Draggable top info header (does NOT overlap the action buttons below)
                        let info_resp = ui
                            .vertical(|ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (tag, col) = match asset.model_type {
                                        ModelAssetType::Animated => {
                                            ("[ANIM]", egui::Color32::from_rgb(192, 132, 252))
                                        }
                                        ModelAssetType::Static => {
                                            ("[STATIS]", egui::Color32::from_rgb(56, 189, 248))
                                        }
                                    };
                                    ui.colored_label(
                                        col,
                                        egui::RichText::new(tag).size(9.5).strong(),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&asset.name)
                                                .size(11.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.colored_label(
                                                if ready {
                                                    egui::Color32::from_rgb(74, 222, 128)
                                                } else {
                                                    egui::Color32::from_rgb(250, 204, 21)
                                                },
                                                egui::RichText::new(if ready {
                                                    "OK"
                                                } else {
                                                    "DL"
                                                })
                                                .size(9.5)
                                                .strong(),
                                            );
                                        },
                                    );
                                });
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} • {}",
                                        asset.category,
                                        format_bytes(asset.model_file.size_bytes),
                                    ))
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            })
                            .response;

                        let drag_resp = ui.interact(
                            info_resp.rect,
                            ui.make_persistent_id(("import_3d_drag_item", asset.id)),
                            egui::Sense::click_and_drag(),
                        );
                        if editable && drag_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                        }
                        if drag_resp.drag_started() && editable {
                            start_drag = Some(asset.to_catalog_entry(ready));
                        } else if drag_resp.clicked() {
                            preview_id = Some(asset.id);
                        }
                        let _ = drag_resp.on_hover_text(if editable {
                            "Tarik (drag) ke peta 3D untuk menempatkan model ini, atau klik untuk Preview 3D"
                        } else {
                            "Hentikan Play Mode (Stop) untuk menambah model"
                        });

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(if is_previewing {
                                            "Previewing"
                                        } else {
                                            "Preview 3D"
                                        })
                                        .size(10.0)
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if is_previewing {
                                        egui::Color32::from_rgb(14, 116, 144)
                                    } else {
                                        egui::Color32::from_rgb(30, 41, 59)
                                    }),
                                )
                                .clicked()
                            {
                                preview_id = Some(asset.id);
                            }

                            if ui
                                .add_enabled(
                                    editable,
                                    egui::Button::new(
                                        egui::RichText::new("+ Peta")
                                            .size(10.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(5, 150, 105)),
                                )
                                .on_hover_text("Tambahkan model 3D ini ke tengah peta")
                                .clicked()
                            {
                                place_model = Some(asset.clone());
                            }

                            if can_delete {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if state.import_panel.confirm_delete_id == Some(asset.id) {
                                            if ui
                                                .add_enabled(
                                                    !state.import_panel.submitting,
                                                    egui::Button::new(
                                                        egui::RichText::new("Ya")
                                                            .size(10.0)
                                                            .color(egui::Color32::WHITE),
                                                    )
                                                    .fill(egui::Color32::from_rgb(185, 28, 28)),
                                                )
                                                .clicked()
                                            {
                                                delete_id = Some(asset.id);
                                            }
                                            if ui.small_button("Batal").clicked() {
                                                state.import_panel.confirm_delete_id = None;
                                            }
                                        } else if ui
                                            .add_enabled(
                                                !state.import_panel.submitting,
                                                egui::Button::new(
                                                    egui::RichText::new("Hapus")
                                                        .size(10.0)
                                                        .color(egui::Color32::from_rgb(
                                                            252, 165, 165,
                                                        )),
                                                )
                                                .fill(egui::Color32::from_rgb(69, 20, 26)),
                                            )
                                            .clicked()
                                        {
                                            state.import_panel.confirm_delete_id = Some(asset.id);
                                        }
                                    },
                                );
                            }
                        });
                    });
                ui.add_space(4.0);
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Tips: Model 3D import juga otomatis muncul di Tab Catalog (label [IMP]) untuk semua user di Operasi ini.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(125, 211, 252)),
            );
        });

    if let Some(entry) = start_drag {
        media_player.stop_all_playback();
        state.import_panel.close_preview();
        state.selected_catalog_id = Some(entry.id.clone());
        state.dragging_catalog_entry = Some(entry);
    } else if let Some(pid) = preview_id {
        media_player.stop_all_playback();
        state.set_plot_import_preview_model(pid);
    }
    if let Some(asset) = place_model {
        media_player.stop_all_playback();
        state.import_panel.close_preview();
        let entry = asset.to_catalog_entry(true);
        state.selected_catalog_id = Some(entry.id.clone());
        spawn_marker_at_map_center(state, tcam, api, &entry);
    }
    if let Some(did) = delete_id
        && let Some(auth) = current_auth(state)
    {
        state.import_panel.submitting = true;
        state.import_panel.error = None;
        state.import_panel.confirm_delete_id = None;
        api.send(ApiCommand::DeleteModelAsset { auth, id: did });
    }
}

fn draw_import_media_list(
    ui: &mut egui::Ui,
    state: &mut AppState,
    active_op_id: uuid::Uuid,
    media_type: MediaAssetType,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
) {
    let needle = state.import_panel.search.trim().to_lowercase();
    let items: Vec<MediaAssetDto> = state
        .media_assets
        .iter()
        .filter(|m| {
            if m.operasi_id != Some(active_op_id) || m.media_type != media_type {
                return false;
            }
            if !needle.is_empty() && !m.name.to_lowercase().contains(&needle) {
                return false;
            }
            true
        })
        .cloned()
        .collect();

    let mut preview_id: Option<uuid::Uuid> = None;
    let mut delete_id: Option<uuid::Uuid> = None;

    egui::ScrollArea::vertical()
        .id_salt(("plot_import_media_scroll", media_type.as_str()))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if items.is_empty() {
                ui.add_space(14.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Belum ada {} yang di-import di Operasi ini. Klik '+ Upload' di atas.",
                        media_type.label()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                return;
            }

            for item in &items {
                let is_previewing = state.is_plot_import_media_preview_active() == Some(media_type)
                    && state.import_panel.preview_media_id == Some(item.id);
                let ready = state
                    .available_model_paths
                    .contains(&item.active_file.rel_path)
                    || state.assets_root.join(&item.active_file.rel_path).exists();
                let can_delete =
                    state.can_manage_imported_asset(item.operasi_id, item.created_by);

                egui::Frame::new()
                    .fill(if is_previewing {
                        egui::Color32::from_rgb(14, 74, 102)
                    } else {
                        egui::Color32::from_rgb(24, 33, 49)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if is_previewing {
                            egui::Color32::from_rgb(56, 189, 248)
                        } else {
                            egui::Color32::from_rgb(44, 58, 82)
                        },
                    ))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&item.name)
                                        .size(11.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .truncate(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.colored_label(
                                        if ready {
                                            egui::Color32::from_rgb(74, 222, 128)
                                        } else {
                                            egui::Color32::from_rgb(250, 204, 21)
                                        },
                                        egui::RichText::new(if ready { "OK" } else { "DL" })
                                            .size(9.5)
                                            .strong(),
                                    );
                                },
                            );
                        });
                        let detail = match media_type {
                            MediaAssetType::Texture => format!(
                                "{}x{} • {}",
                                item.active_width.unwrap_or(0),
                                item.active_height.unwrap_or(0),
                                format_bytes(item.active_file.size_bytes)
                            ),
                            MediaAssetType::Sound => {
                                let dur = item
                                    .duration_sec
                                    .map(|d| format_duration(d as f32))
                                    .unwrap_or_else(|| "--:--".into());
                                format!("{dur} • {}", format_bytes(item.active_file.size_bytes))
                            }
                            MediaAssetType::Video => {
                                let dims = match (item.active_width, item.active_height) {
                                    (Some(w), Some(h)) => format!("{w}x{h} • "),
                                    _ => String::new(),
                                };
                                let dur = item
                                    .duration_sec
                                    .map(|d| format_duration(d as f32))
                                    .unwrap_or_else(|| "--:--".into());
                                format!(
                                    "{dims}{dur} • {}",
                                    format_bytes(item.active_file.size_bytes)
                                )
                            }
                        };
                        ui.label(
                            egui::RichText::new(detail)
                                .size(10.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(if is_previewing {
                                            "Previewing"
                                        } else {
                                            "Preview"
                                        })
                                        .size(10.0)
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if is_previewing {
                                        egui::Color32::from_rgb(14, 116, 144)
                                    } else {
                                        egui::Color32::from_rgb(30, 41, 59)
                                    }),
                                )
                                .clicked()
                            {
                                preview_id = Some(item.id);
                            }

                            if can_delete {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if state.import_panel.confirm_delete_id == Some(item.id) {
                                            if ui
                                                .add_enabled(
                                                    !state.import_panel.submitting,
                                                    egui::Button::new(
                                                        egui::RichText::new("Ya")
                                                            .size(10.0)
                                                            .color(egui::Color32::WHITE),
                                                    )
                                                    .fill(egui::Color32::from_rgb(185, 28, 28)),
                                                )
                                                .clicked()
                                            {
                                                delete_id = Some(item.id);
                                            }
                                            if ui.small_button("Batal").clicked() {
                                                state.import_panel.confirm_delete_id = None;
                                            }
                                        } else if ui
                                            .add_enabled(
                                                !state.import_panel.submitting,
                                                egui::Button::new(
                                                    egui::RichText::new("Hapus")
                                                        .size(10.0)
                                                        .color(egui::Color32::from_rgb(
                                                            252, 165, 165,
                                                        )),
                                                )
                                                .fill(egui::Color32::from_rgb(69, 20, 26)),
                                            )
                                            .clicked()
                                        {
                                            state.import_panel.confirm_delete_id = Some(item.id);
                                        }
                                    },
                                );
                            }
                        });
                    });
                ui.add_space(4.0);
            }

            ui.add_space(4.0);
            let usage_tip = match media_type {
                MediaAssetType::Texture => {
                    "Tips: Texture ini otomatis tersedia di Basic Mesh & Animasi Billboard/Videotron pada Operasi ini."
                }
                MediaAssetType::Sound => {
                    "Tips: Sound ini otomatis tersedia di Animasi -> Play Suara pada Operasi ini."
                }
                MediaAssetType::Video => {
                    "Tips: Video ini otomatis tersedia di Animasi -> Billboard/Videotron pada Operasi ini."
                }
            };
            ui.label(
                egui::RichText::new(usage_tip)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            );
        });

    if let Some(pid) = preview_id {
        if state.import_panel.preview_media_id != Some(pid) {
            media_player.stop_all_playback();
        }
        state.set_plot_import_preview_media(pid, media_type);
    }
    if let Some(did) = delete_id
        && let Some(auth) = current_auth(state)
    {
        media_player.stop_all_playback();
        state.import_panel.submitting = true;
        state.import_panel.error = None;
        state.import_panel.confirm_delete_id = None;
        api.send(ApiCommand::DeleteMediaAsset { auth, id: did });
    }
}

/// Right dock panel shown while Plot Editor is temporarily in Import Preview mode.
#[allow(clippy::too_many_arguments)]
pub fn draw_import_preview_right_panel(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    stage: &mut AssetPreviewStage,
    tcam: &TacticalCamera,
    playback: &PlaybackState,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
) -> f32 {
    egui::Panel::right("plot_import_preview_right_dock")
        .resizable(true)
        .default_size(320.0)
        .min_size(265.0)
        .max_size(440.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 23, 35))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::same(10)),
        )
        .show(viewport_ui, |ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("< Kembali ke Peta 3D")
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(14, 116, 144))
                    .min_size(egui::vec2(ui.available_width(), 30.0)),
                )
                .clicked()
            {
                media_player.stop_all_playback();
                state.import_panel.close_preview();
                return;
            }
            ui.add_space(8.0);

            egui::ScrollArea::vertical()
                .id_salt("plot_import_preview_right_scroll")
                .show(ui, |ui| {
                    if state.is_plot_import_3d_preview_active() {
                        let Some(asset) = state.active_preview_asset().cloned() else {
                            ui.label("Pilih model 3D di tab Import untuk melihat preview.");
                            return;
                        };
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(24, 33, 50))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(217, 119, 6)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new(&asset.name)
                                        .size(14.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Import Operasi • {}",
                                        asset.category
                                    ))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(253, 224, 71)),
                                );
                                ui.add_space(6.0);
                                if ui
                                    .add_enabled(
                                        !playback.play_test_mode,
                                        egui::Button::new(
                                            egui::RichText::new("+ Letakkan di Peta")
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(5, 150, 105))
                                        .min_size(egui::vec2(ui.available_width(), 28.0)),
                                    )
                                    .clicked()
                                {
                                    state.import_panel.close_preview();
                                    let entry = asset.to_catalog_entry(true);
                                    state.selected_catalog_id = Some(entry.id.clone());
                                    spawn_marker_at_map_center(state, tcam, api, &entry);
                                }
                            });

                        ui.add_space(8.0);
                        draw_preview_controls_card(ui, state, stage, &asset);
                        ui.add_space(8.0);
                        draw_asset_file_stats_card(ui, &asset);
                    } else if let Some(asset) = state.active_preview_media().cloned() {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(24, 33, 50))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(217, 119, 6)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new(&asset.name)
                                        .size(14.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Import Operasi • {}",
                                        asset.media_type.label()
                                    ))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(253, 224, 71)),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Ukuran: {} • MIME: {}",
                                        format_bytes(asset.active_file.size_bytes),
                                        asset.mime_type
                                    ))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                                if let (Some(w), Some(h)) = (asset.active_width, asset.active_height)
                                {
                                    ui.label(
                                        egui::RichText::new(format!("Resolusi: {w} x {h} px"))
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(186, 230, 253)),
                                    );
                                }
                                if let Some(dur) = asset.duration_sec {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Durasi: {}",
                                            format_duration(dur as f32)
                                        ))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(186, 230, 253)),
                                    );
                                }
                            });
                    }
                });
        })
        .response
        .rect
        .width()
}

/// Floating HUD overlay for 3D Model preview inside Plot Editor's center viewport.
pub fn draw_import_3d_center_hud(
    ctx: &egui::Context,
    center_rect: egui::Rect,
    state: &mut AppState,
) {
    if let Some(asset) = state.active_preview_asset() {
        draw_asset_preview_viewport_hud(
            ctx,
            center_rect,
            state,
            asset,
            Some((
                "IMPORT OPERASI",
                egui::Color32::from_rgb(120, 53, 15),
                egui::Color32::from_rgb(253, 230, 138),
            )),
        );
    }

    // Floating quick "< Kembali ke Peta" pill at top-right of center viewport
    let mut close_preview = false;
    egui::Area::new(egui::Id::new("import_3d_back_to_map_btn"))
        .fixed_pos(egui::pos2(
            (center_rect.right() - 164.0).max(center_rect.left() + 12.0),
            center_rect.top() + 14.0,
        ))
        .show(ctx, |ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("< Kembali ke Peta")
                            .size(11.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(14, 116, 144))
                    .min_size(egui::vec2(148.0, 28.0)),
                )
                .clicked()
            {
                close_preview = true;
            }
        });
    if close_preview {
        state.import_panel.close_preview();
    }
}
