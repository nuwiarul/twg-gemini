use bevy_egui::egui;
use twg_shared::{
    api::{
        CopySkenarioRequest, CreateLocationPresetRequest, CreateSkenarioRequest,
        MoveSkenarioRequest, UpdateLocationPresetRequest, UpdateSkenarioRequest,
    },
    models::{BaseLayer, IndoorConfig, LocationPreset, MapMode, Skenario, SkyboxPreset},
};
use uuid::Uuid;

use crate::{
    net::{
        asset_sync::AssetSyncBridge,
        client::{ApiClientBridge, ApiCommand},
        ws_client::WsClientBridge,
    },
    scene::markers::current_auth,
    state::{AppScreen, AppState, PlaybackState, SkenarioModalMode},
    ui::{
        nav_bar::{
            draw_global_top_bar, draw_modal_backdrop, format_timestamp, open_plot_skenario,
            trigger_load_operasi_options, trigger_load_operasis, trigger_load_skenarios,
        },
        pagination::draw_pagination_bar,
        spinner::{SpinnerConfig, numeric_spinner, numeric_spinner_i32},
        table::{
            TABLE_COL_GAP, TABLE_PAD_LEFT, TABLE_PAD_RIGHT, TABLE_SCROLLBAR_RESERVE,
            draw_aligned_row, draw_header_label, table_content_width,
        },
    },
};

pub struct SkenarioFormLayoutInsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_skenario_form_screen(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) -> SkenarioFormLayoutInsets {
    let editing_id = match state.skenario_table.modal_mode {
        SkenarioModalMode::Edit(id) => Some(id),
        _ => None,
    };

    let top_global = draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);
    let mut close = false;
    let mut submit = false;

    let top_sub = egui::Panel::top("skenario_form_subbar")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(18, 26, 42))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 49, 71)))
                .inner_margin(egui::Margin::symmetric(14, 7)),
        )
        .show(viewport_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !state.skenario_table.submitting,
                        egui::Button::new(
                            egui::RichText::new("< Daftar Skenario")
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        )
                        .fill(egui::Color32::from_rgb(30, 41, 59)),
                    )
                    .clicked()
                {
                    close = true;
                }

                ui.separator();
                let title = if editing_id.is_some() {
                    "Edit Skenario & Posisi Peta"
                } else {
                    "Tambah Skenario Baru & Pilih Area Peta"
                };
                ui.label(
                    egui::RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                ui.separator();

                // Quick base layer switcher in top subbar
                for (layer, mode, text) in [
                    (BaseLayer::Osm, MapMode::Outdoor, "OSM"),
                    (BaseLayer::Satellite, MapMode::Outdoor, "Satellite"),
                    (BaseLayer::GridDark, MapMode::Indoor, "Indoor Dark"),
                    (BaseLayer::GridLight, MapMode::Indoor, "Indoor Light"),
                ] {
                    let active = state.skenario_table.form_base_layer == layer
                        && state.skenario_table.form_map_mode == mode;
                    if ui.selectable_label(active, text).clicked() {
                        state.skenario_table.form_base_layer = layer;
                        state.skenario_table.form_map_mode = mode;
                        state.skenario_table.map_preview_dirty = true;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_btn = egui::Button::new(
                        egui::RichText::new(if state.skenario_table.submitting {
                            "Menyimpan..."
                        } else if editing_id.is_some() {
                            "Simpan Perubahan"
                        } else {
                            "Buat Skenario"
                        })
                        .strong()
                        .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .min_size(egui::vec2(138.0, 26.0));
                    if ui
                        .add_enabled(!state.skenario_table.submitting, save_btn)
                        .clicked()
                    {
                        submit = true;
                    }

                    if ui
                        .add_enabled(
                            !state.skenario_table.submitting,
                            egui::Button::new("Batal").min_size(egui::vec2(70.0, 26.0)),
                        )
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        })
        .response
        .rect
        .height();

    let bottom = egui::Panel::bottom("skenario_form_bottom_bar")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(13, 19, 31))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::symmetric(14, 6)),
        )
        .show(viewport_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(
                        "Kontrol Peta: Drag Kiri/Tengah = Pan  •  Scroll = Zoom  •  Drag Kanan = Pitch & Bearing  •  WASD/Q/E = Navigasi",
                    )
                    .size(11.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Center: {:.5}, {:.5}  |  Zoom {:.2} ({:.1}–{:.1})  |  Pitch {:.1}°  |  Bearing {:.1}°",
                            state.skenario_table.form_center_lat,
                            state.skenario_table.form_center_lng,
                            state.skenario_table.form_zoom,
                            state.skenario_table.form_min_zoom,
                            state.skenario_table.form_max_zoom,
                            state.skenario_table.form_pitch,
                            state.skenario_table.form_bearing,
                        ))
                        .size(11.5)
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                    );
                });
            });
        })
        .response
        .rect
        .height();

    let left = egui::Panel::left("skenario_form_left_panel")
        .resizable(true)
        .default_size(415.0)
        .min_size(340.0)
        .max_size(540.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 22, 36))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 49, 71)))
                .inner_margin(egui::Margin::same(14)),
        )
        .show(viewport_ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("skenario_form_left_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Card 1: Basic Info
                    form_section_card(ui, "Informasi Skenario", |ui| {
                        if editing_id.is_none() {
                            ui.label(
                                egui::RichText::new("Operasi *")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            let op_options = state.skenario_table.operasi_options.clone();
                            let selected_op_label = state
                                .skenario_table
                                .form_operasi_id
                                .and_then(|oid| {
                                    op_options
                                        .iter()
                                        .find(|o| o.id == oid)
                                        .map(|o| o.name.clone())
                                })
                                .unwrap_or_else(|| "(Pilih Operasi)".to_string());
                            egui::ComboBox::from_id_salt("skenario_form_operasi_select")
                                .selected_text(selected_op_label)
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    for op in &op_options {
                                        ui.selectable_value(
                                            &mut state.skenario_table.form_operasi_id,
                                            Some(op.id),
                                            &op.name,
                                        );
                                    }
                                });
                            ui.add_space(8.0);
                        }

                        ui.label(
                            egui::RichText::new("Nama Skenario *")
                                .size(11.5)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut state.skenario_table.form_name)
                                .hint_text("Contoh: Pengamanan Rute VIP Sektor Barat")
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(8.0);
                        let can_write_admin = state
                            .current_user
                            .as_ref()
                            .is_some_and(|u| u.role.can_write_admin());

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Preset Lokasi Cepat (Master Data)")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if can_write_admin && !state.offline_preview {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("Kelola Master")
                                            .on_hover_text("Buka Master Data Preset Lokasi (Tambah / Edit / Hapus)")
                                            .clicked()
                                        {
                                            let next_sort = state
                                                .location_presets
                                                .iter()
                                                .map(|p| p.sort_order)
                                                .max()
                                                .unwrap_or(50)
                                                + 10;
                                            state.preset_manager.reset_form(next_sort);
                                            state.preset_manager.copy_from_skenario_form(
                                                &state.skenario_table,
                                            );
                                            state.preset_manager.open = true;
                                            state.preset_manager.success = None;
                                        }
                                    },
                                );
                            }
                        });

                        let presets_snapshot: Vec<LocationPreset> = state.location_presets.clone();
                        let selected_preset_label = state
                            .skenario_table
                            .form_preset_id
                            .and_then(|pid| {
                                presets_snapshot
                                    .iter()
                                    .find(|p| p.id == pid)
                                    .map(|p| p.name.clone())
                            })
                            .unwrap_or_else(|| "Pilih Preset Lokasi...".to_string());

                        let mut chosen_preset: Option<LocationPreset> = None;
                        egui::ComboBox::from_id_salt("skenario_form_preset_master")
                            .selected_text(selected_preset_label)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for preset in &presets_snapshot {
                                    let is_sel =
                                        state.skenario_table.form_preset_id == Some(preset.id);
                                    if ui
                                        .selectable_label(
                                            is_sel,
                                            format!(
                                                "{}  [Z{:.1}]",
                                                preset.name, preset.zoom
                                            ),
                                        )
                                        .clicked()
                                    {
                                        chosen_preset = Some(preset.clone());
                                    }
                                }
                            });
                        if let Some(p) = chosen_preset {
                            state.skenario_table.apply_location_preset(&p);
                        }

                        // Quick 1-click "Simpan Posisi Peta Ini Jadi Preset"
                        if can_write_admin && !state.offline_preview {
                            ui.add_space(5.0);
                            if !state.preset_manager.quick_save_open {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("+ Simpan Posisi Peta Ini Jadi Preset")
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(125, 211, 252)),
                                        )
                                        .fill(egui::Color32::from_rgb(24, 38, 60))
                                        .min_size(egui::vec2(ui.available_width(), 24.0)),
                                    )
                                    .clicked()
                                {
                                    state.preset_manager.quick_save_open = true;
                                    state.preset_manager.quick_save_name.clear();
                                    state.preset_manager.error = None;
                                }
                            } else {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut state.preset_manager.quick_save_name,
                                        )
                                        .hint_text("Nama preset lokasi baru...")
                                        .desired_width((ui.available_width() - 108.0).max(110.0)),
                                    );
                                    if ui
                                        .add_enabled(
                                            !state.preset_manager.submitting,
                                            egui::Button::new("Simpan")
                                                .fill(egui::Color32::from_rgb(5, 150, 105)),
                                        )
                                        .clicked()
                                    {
                                        let pname =
                                            state.preset_manager.quick_save_name.trim().to_string();
                                        if pname.is_empty() {
                                            state.preset_manager.error =
                                                Some("Nama preset wajib diisi.".into());
                                        } else if let Some(auth) = current_auth(state) {
                                            let next_sort = state
                                                .location_presets
                                                .iter()
                                                .map(|p| p.sort_order)
                                                .max()
                                                .unwrap_or(50)
                                                + 10;
                                            state.preset_manager.submitting = true;
                                            state.preset_manager.error = None;
                                            api.send(ApiCommand::CreateLocationPreset {
                                                auth,
                                                req: CreateLocationPresetRequest {
                                                    name: pname,
                                                    center_lng: state.skenario_table.form_center_lng,
                                                    center_lat: state.skenario_table.form_center_lat,
                                                    zoom: state.skenario_table.form_zoom,
                                                    min_zoom: state.skenario_table.form_min_zoom,
                                                    max_zoom: state.skenario_table.form_max_zoom,
                                                    pitch: state.skenario_table.form_pitch,
                                                    bearing: state.skenario_table.form_bearing,
                                                    map_mode: state.skenario_table.form_map_mode,
                                                    base_layer: state.skenario_table.form_base_layer,
                                                    grid_size_m: state.skenario_table.form_grid_size_m,
                                                    sort_order: next_sort,
                                                },
                                            });
                                        }
                                    }
                                    if ui.button("Batal").clicked() {
                                        state.preset_manager.quick_save_open = false;
                                        state.preset_manager.error = None;
                                    }
                                });
                                if let Some(err) = &state.preset_manager.error {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(248, 113, 113),
                                        egui::RichText::new(err).size(11.0),
                                    );
                                }
                            }
                        }
                    });

                    // Card 2: Mode, Base Layer & Skybox
                    form_section_card(ui, "Mode Peta, Base Layer & Skybox", |ui| {
                        let prev_mode = state.skenario_table.form_map_mode;
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut state.skenario_table.form_map_mode,
                                MapMode::Outdoor,
                                "Outdoor (Geo Map)",
                            );
                            ui.selectable_value(
                                &mut state.skenario_table.form_map_mode,
                                MapMode::Indoor,
                                "Indoor (Tactical Grid)",
                            );
                        });
                        if state.skenario_table.form_map_mode != prev_mode {
                            state.skenario_table.form_base_layer =
                                match state.skenario_table.form_map_mode {
                                    MapMode::Outdoor => BaseLayer::Osm,
                                    MapMode::Indoor => BaseLayer::GridDark,
                                };
                            state.skenario_table.map_preview_dirty = true;
                        }

                        ui.add_space(6.0);
                        let prev_layer = state.skenario_table.form_base_layer;
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Base Layer:")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            egui::ComboBox::from_id_salt("skenario_form_base_layer")
                                .selected_text(state.skenario_table.form_base_layer.label())
                                .width((ui.available_width() - 8.0).max(150.0))
                                .show_ui(ui, |ui| {
                                    let layers: &[BaseLayer] =
                                        match state.skenario_table.form_map_mode {
                                            MapMode::Outdoor => {
                                                &[BaseLayer::Osm, BaseLayer::Satellite]
                                            }
                                            MapMode::Indoor => &[
                                                BaseLayer::GridDark,
                                                BaseLayer::GridLight,
                                                BaseLayer::BlankDark,
                                                BaseLayer::BlankLight,
                                                BaseLayer::FloorPlan,
                                            ],
                                        };
                                    for layer in layers {
                                        ui.selectable_value(
                                            &mut state.skenario_table.form_base_layer,
                                            *layer,
                                            layer.label(),
                                        );
                                    }
                                });
                        });
                        if state.skenario_table.form_base_layer != prev_layer {
                            state.skenario_table.map_preview_dirty = true;
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Skybox 3D:")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            egui::ComboBox::from_id_salt("skenario_form_skybox")
                                .selected_text(state.skenario_table.form_skybox.label())
                                .width((ui.available_width() - 8.0).max(150.0))
                                .show_ui(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("Tanpa Skybox")
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    ui.selectable_value(
                                        &mut state.skenario_table.form_skybox,
                                        SkyboxPreset::None,
                                        SkyboxPreset::None.label(),
                                    );
                                    ui.separator();
                                    ui.label(
                                        egui::RichText::new("Siang Hari")
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(253, 224, 71)),
                                    );
                                    for preset in [
                                        SkyboxPreset::DayClear,
                                        SkyboxPreset::DayCloudy,
                                        SkyboxPreset::DaySunset,
                                    ] {
                                        ui.selectable_value(
                                            &mut state.skenario_table.form_skybox,
                                            preset,
                                            preset.label(),
                                        );
                                    }
                                    ui.separator();
                                    ui.label(
                                        egui::RichText::new("Malam Hari")
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(147, 197, 253)),
                                    );
                                    for preset in [
                                        SkyboxPreset::NightStarry,
                                        SkyboxPreset::NightTactical,
                                    ] {
                                        ui.selectable_value(
                                            &mut state.skenario_table.form_skybox,
                                            preset,
                                            preset.label(),
                                        );
                                    }
                                });
                        });

                        if state.skenario_table.form_map_mode == MapMode::Indoor {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Ukuran Grid Indoor (m):")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            numeric_spinner(
                                ui,
                                "sk_form_grid_size",
                                &mut state.skenario_table.form_grid_size_m,
                                SpinnerConfig {
                                    min: 1.0,
                                    max: 200.0,
                                    step: 1.0,
                                    drag_speed: 0.25,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        }
                    });

                    // Card 3: Zoom Sekarang, Minimal Zoom, Maximal Zoom (up to 26, Custom Up/Down Spinner)
                    form_section_card(ui, "Pengaturan Zoom (Maks 26 • Sinkron Live)", |ui| {
                        let min_z = state.skenario_table.form_min_zoom;
                        let max_z = state.skenario_table.form_max_zoom;

                        ui.label(
                            egui::RichText::new("Zoom Sekarang:")
                                .size(11.5)
                                .strong()
                                .color(egui::Color32::from_rgb(125, 211, 252)),
                        );
                        if numeric_spinner(
                            ui,
                            "sk_form_zoom_now",
                            &mut state.skenario_table.form_zoom,
                            SpinnerConfig {
                                min: min_z,
                                max: max_z.max(min_z),
                                step: 0.25,
                                drag_speed: 0.05,
                                decimals: 2,
                                suffix: "",
                                wrap_around: false,
                            },
                        ) {
                            state.skenario_table.clamp_zoom_bounds();
                            state.skenario_table.map_preview_dirty = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(
                                    &mut state.skenario_table.form_zoom,
                                    min_z..=max_z.max(min_z),
                                )
                                .show_value(false),
                            )
                            .changed()
                        {
                            state.skenario_table.clamp_zoom_bounds();
                            state.skenario_table.map_preview_dirty = true;
                        }

                        ui.add_space(8.0);
                        ui.columns(2, |cols| {
                            cols[0].label(
                                egui::RichText::new("Minimal Zoom")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                &mut cols[0],
                                "sk_form_min_zoom",
                                &mut state.skenario_table.form_min_zoom,
                                SpinnerConfig {
                                    min: 1.0,
                                    max: 26.0,
                                    step: 0.5,
                                    drag_speed: 0.1,
                                    decimals: 1,
                                    suffix: "",
                                    wrap_around: false,
                                },
                            ) {
                                if state.skenario_table.form_max_zoom
                                    < state.skenario_table.form_min_zoom
                                {
                                    state.skenario_table.form_max_zoom =
                                        state.skenario_table.form_min_zoom;
                                }
                                state.skenario_table.clamp_zoom_bounds();
                                state.skenario_table.map_preview_dirty = true;
                            }

                            cols[1].label(
                                egui::RichText::new("Maximal Zoom (maks 26)")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                &mut cols[1],
                                "sk_form_max_zoom",
                                &mut state.skenario_table.form_max_zoom,
                                SpinnerConfig {
                                    min: 1.0,
                                    max: 26.0,
                                    step: 0.5,
                                    drag_speed: 0.1,
                                    decimals: 1,
                                    suffix: "",
                                    wrap_around: false,
                                },
                            ) {
                                if state.skenario_table.form_min_zoom
                                    > state.skenario_table.form_max_zoom
                                {
                                    state.skenario_table.form_min_zoom =
                                        state.skenario_table.form_max_zoom;
                                }
                                state.skenario_table.clamp_zoom_bounds();
                                state.skenario_table.map_preview_dirty = true;
                            }
                        });
                    });

                    // Card 4: Coordinates, Pitch & Bearing (Custom Up/Down Spinner + Live Sync)
                    form_section_card(ui, "Koordinat, Pitch & Bearing (Sinkron Live)", |ui| {
                        ui.columns(2, |cols| {
                            cols[0].label(
                                egui::RichText::new("Center Latitude")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                &mut cols[0],
                                "sk_form_lat",
                                &mut state.skenario_table.form_center_lat,
                                SpinnerConfig {
                                    min: -85.0,
                                    max: 85.0,
                                    step: 0.0005,
                                    drag_speed: 0.0002,
                                    decimals: 5,
                                    suffix: "",
                                    wrap_around: false,
                                },
                            ) {
                                state.skenario_table.map_preview_dirty = true;
                            }

                            cols[1].label(
                                egui::RichText::new("Center Longitude")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                &mut cols[1],
                                "sk_form_lng",
                                &mut state.skenario_table.form_center_lng,
                                SpinnerConfig {
                                    min: -180.0,
                                    max: 180.0,
                                    step: 0.0005,
                                    drag_speed: 0.0002,
                                    decimals: 5,
                                    suffix: "",
                                    wrap_around: false,
                                },
                            ) {
                                state.skenario_table.map_preview_dirty = true;
                            }
                        });

                        ui.add_space(8.0);
                        ui.columns(2, |cols| {
                            cols[0].label(
                                egui::RichText::new("Pitch / Kemiringan (°)")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                &mut cols[0],
                                "sk_form_pitch",
                                &mut state.skenario_table.form_pitch,
                                SpinnerConfig {
                                    min: 5.0,
                                    max: 82.0,
                                    step: 2.0,
                                    drag_speed: 0.5,
                                    decimals: 1,
                                    suffix: "°",
                                    wrap_around: false,
                                },
                            ) {
                                state.skenario_table.map_preview_dirty = true;
                            }

                            cols[1].label(
                                egui::RichText::new("Bearing / Kompas (°)")
                                    .size(11.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                &mut cols[1],
                                "sk_form_bearing",
                                &mut state.skenario_table.form_bearing,
                                SpinnerConfig {
                                    min: 0.0,
                                    max: 360.0,
                                    step: 5.0,
                                    drag_speed: 1.0,
                                    decimals: 1,
                                    suffix: "°",
                                    wrap_around: true,
                                },
                            ) {
                                state.skenario_table.map_preview_dirty = true;
                            }
                        });

                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.small_button("Top-Down 2D (6°)").clicked() {
                                state.skenario_table.form_pitch = 6.0;
                                state.skenario_table.form_bearing = 0.0;
                                state.skenario_table.map_preview_dirty = true;
                            }
                            if ui.small_button("Standar 3D (48°)").clicked() {
                                state.skenario_table.form_pitch = 48.0;
                                state.skenario_table.form_bearing = 0.0;
                                state.skenario_table.map_preview_dirty = true;
                            }
                            if ui.small_button("Utara (0°)").clicked() {
                                state.skenario_table.form_bearing = 0.0;
                                state.skenario_table.map_preview_dirty = true;
                            }
                        });
                    });

                    if let Some(err) = &state.skenario_table.form_error {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(69, 10, 10))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(185, 28, 28)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.colored_label(egui::Color32::from_rgb(252, 165, 165), err);
                            });
                        ui.add_space(8.0);
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let save_btn = egui::Button::new(
                            egui::RichText::new(if state.skenario_table.submitting {
                                "Menyimpan..."
                            } else if editing_id.is_some() {
                                "Simpan Perubahan"
                            } else {
                                "Buat Skenario"
                            })
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(150.0, 32.0));
                        if ui
                            .add_enabled(!state.skenario_table.submitting, save_btn)
                            .clicked()
                        {
                            submit = true;
                        }

                        if ui
                            .add_enabled(
                                !state.skenario_table.submitting,
                                egui::Button::new("Batal").min_size(egui::vec2(80.0, 32.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
        })
        .response
        .rect
        .width();

    let total_top = top_global + top_sub;
    let screen_rect = ctx.viewport_rect();
    let map_rect = egui::Rect::from_min_max(
        egui::pos2(left, total_top),
        egui::pos2(
            screen_rect.max.x.max(left + 64.0),
            (screen_rect.max.y - bottom).max(total_top + 64.0),
        ),
    );
    draw_skenario_map_hud(ctx, map_rect, state);
    if state.preset_manager.open {
        draw_location_preset_manager_modal(ctx, state, api, true);
    }

    if submit {
        state.skenario_table.clamp_zoom_bounds();
        let name = state.skenario_table.form_name.trim().to_string();
        if name.is_empty() {
            state.skenario_table.form_error = Some("Nama skenario wajib diisi.".into());
        } else if editing_id.is_none() && state.skenario_table.form_operasi_id.is_none() {
            state.skenario_table.form_error = Some("Pilih Operasi terlebih dahulu.".into());
        } else if let Some(auth) = current_auth(state) {
            state.skenario_table.submitting = true;
            state.skenario_table.form_error = None;
            let indoor_config = IndoorConfig {
                grid_size_m: state.skenario_table.form_grid_size_m.max(1.0),
                ..Default::default()
            };
            if let Some(id) = editing_id {
                api.send(ApiCommand::UpdateSkenario {
                    auth,
                    id,
                    req: UpdateSkenarioRequest {
                        name,
                        center_lng: state.skenario_table.form_center_lng,
                        center_lat: state.skenario_table.form_center_lat,
                        zoom: state.skenario_table.form_zoom,
                        min_zoom: state.skenario_table.form_min_zoom,
                        max_zoom: state.skenario_table.form_max_zoom,
                        pitch: state.skenario_table.form_pitch,
                        bearing: state.skenario_table.form_bearing,
                        map_mode: state.skenario_table.form_map_mode,
                        base_layer: state.skenario_table.form_base_layer,
                        skybox: state.skenario_table.form_skybox,
                        indoor_config,
                    },
                });
            } else if let Some(operasi_id) = state.skenario_table.form_operasi_id {
                api.send(ApiCommand::CreateSkenario {
                    auth,
                    req: CreateSkenarioRequest {
                        operasi_id,
                        name,
                        center_lng: state.skenario_table.form_center_lng,
                        center_lat: state.skenario_table.form_center_lat,
                        zoom: state.skenario_table.form_zoom,
                        min_zoom: state.skenario_table.form_min_zoom,
                        max_zoom: state.skenario_table.form_max_zoom,
                        pitch: state.skenario_table.form_pitch,
                        bearing: state.skenario_table.form_bearing,
                        map_mode: state.skenario_table.form_map_mode,
                        base_layer: state.skenario_table.form_base_layer,
                        skybox: state.skenario_table.form_skybox,
                        indoor_config,
                    },
                });
            }
        }
    } else if close {
        state.skenario_table.close_modal();
        state.restore_active_skenario_map_state();
    }

    SkenarioFormLayoutInsets {
        left,
        right: 0.0,
        top: total_top,
        bottom,
    }
}

fn form_section_card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(22, 31, 48))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::same(11))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(12.5)
                    .strong()
                    .color(egui::Color32::from_rgb(56, 189, 248)),
            );
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(6.0);
            body(ui);
        });
    ui.add_space(10.0);
}

fn draw_skenario_map_hud(ctx: &egui::Context, map_rect: egui::Rect, state: &mut AppState) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("skenario_form_map_crosshair"),
    ));
    let painter = painter.with_clip_rect(map_rect);

    // Center target crosshair so user can aim exact Center Lat / Lng
    let c = map_rect.center();
    let ring_col = egui::Color32::from_rgba_unmultiplied(56, 189, 248, 210);
    let shadow_col = egui::Color32::from_rgba_unmultiplied(8, 12, 20, 200);
    painter.circle_stroke(c, 13.0, egui::Stroke::new(3.0, shadow_col));
    painter.circle_stroke(c, 13.0, egui::Stroke::new(1.5, ring_col));
    painter.line_segment(
        [c - egui::vec2(20.0, 0.0), c - egui::vec2(6.0, 0.0)],
        egui::Stroke::new(2.0, ring_col),
    );
    painter.line_segment(
        [c + egui::vec2(6.0, 0.0), c + egui::vec2(20.0, 0.0)],
        egui::Stroke::new(2.0, ring_col),
    );
    painter.line_segment(
        [c - egui::vec2(0.0, 20.0), c - egui::vec2(0.0, 6.0)],
        egui::Stroke::new(2.0, ring_col),
    );
    painter.line_segment(
        [c + egui::vec2(0.0, 6.0), c + egui::vec2(0.0, 20.0)],
        egui::Stroke::new(2.0, ring_col),
    );
    painter.circle_filled(c, 2.5, egui::Color32::from_rgb(248, 113, 113));

    // Top-left live telemetry overlay (non-interactable so map drags pass straight through)
    egui::Area::new(egui::Id::new("skenario_map_telemetry_hud"))
        .fixed_pos(map_rect.left_top() + egui::vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(12, 18, 30, 220))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 62, 88)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("LIVE 3D MAP PICKER")
                            .size(10.5)
                            .strong()
                            .color(egui::Color32::from_rgb(52, 211, 153)),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Lat {:.5}  •  Lng {:.5}",
                            state.skenario_table.form_center_lat,
                            state.skenario_table.form_center_lng
                        ))
                        .size(12.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Zoom {:.2} (Min {:.1} - Max {:.1})  •  Pitch {:.1}°  •  Bearing {:.1}°",
                            state.skenario_table.form_zoom,
                            state.skenario_table.form_min_zoom,
                            state.skenario_table.form_max_zoom,
                            state.skenario_table.form_pitch,
                            state.skenario_table.form_bearing,
                        ))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(186, 230, 253)),
                    );
                });
        });

    // Top-right floating map controls (Compass + Zoom In / Out + 2D/3D toggle)
    egui::Area::new(egui::Id::new("skenario_map_floating_controls"))
        .fixed_pos(egui::pos2(
            (map_rect.max.x - 62.0).max(map_rect.min.x + 12.0),
            map_rect.min.y + 14.0,
        ))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 23, 42, 225))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        // Interactive Compass (click to reset bearing to 0 deg North)
                        let (compass_rect, compass_resp) =
                            ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::click());
                        let cp = ui.painter();
                        let cc = compass_rect.center();
                        cp.circle_filled(cc, 16.0, egui::Color32::from_rgb(24, 34, 53));
                        cp.circle_stroke(
                            cc,
                            16.0,
                            egui::Stroke::new(1.2, egui::Color32::from_rgb(71, 85, 105)),
                        );
                        let rad = -(state.skenario_table.form_bearing as f32).to_radians();
                        let dir = egui::vec2(rad.sin(), -rad.cos());
                        cp.line_segment(
                            [cc, cc + dir * 11.5],
                            egui::Stroke::new(2.5, egui::Color32::from_rgb(239, 68, 68)),
                        );
                        cp.line_segment(
                            [cc, cc - dir * 9.0],
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(148, 163, 184)),
                        );
                        cp.circle_filled(cc, 2.2, egui::Color32::WHITE);
                        if compass_resp
                            .on_hover_text(format!(
                                "Bearing {:.1}° — Klik untuk reset ke Utara (0°)",
                                state.skenario_table.form_bearing
                            ))
                            .clicked()
                        {
                            state.skenario_table.form_bearing = 0.0;
                            state.skenario_table.map_preview_dirty = true;
                        }

                        ui.add_space(4.0);
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new("+").size(15.0).strong())
                                    .min_size(egui::vec2(34.0, 26.0)),
                            )
                            .on_hover_text("Zoom In (+0.5)")
                            .clicked()
                        {
                            state.skenario_table.form_zoom += 0.5;
                            state.skenario_table.clamp_zoom_bounds();
                            state.skenario_table.map_preview_dirty = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new("-").size(15.0).strong())
                                    .min_size(egui::vec2(34.0, 26.0)),
                            )
                            .on_hover_text("Zoom Out (-0.5)")
                            .clicked()
                        {
                            state.skenario_table.form_zoom -= 0.5;
                            state.skenario_table.clamp_zoom_bounds();
                            state.skenario_table.map_preview_dirty = true;
                        }
                        ui.add_space(2.0);
                        let is_flat = state.skenario_table.form_pitch < 15.0;
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(if is_flat { "3D" } else { "2D" })
                                        .size(11.0)
                                        .strong(),
                                )
                                .min_size(egui::vec2(34.0, 24.0)),
                            )
                            .on_hover_text("Toggle sudut kamera 2D / 3D")
                            .clicked()
                        {
                            state.skenario_table.form_pitch = if is_flat { 48.0 } else { 6.0 };
                            state.skenario_table.map_preview_dirty = true;
                        }
                    });
                });
        });
}

#[allow(clippy::too_many_arguments)]
pub fn draw_skenario_screen(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);

    let can_write_admin = state
        .current_user
        .as_ref()
        .is_some_and(|u| u.role.can_write_admin());
    let can_edit_skenario = can_write_admin;

    let filtered_operasi = state
        .skenario_table
        .filter_operasi_id
        .and_then(|fid| {
            state
                .skenario_table
                .operasi_options
                .iter()
                .find(|o| o.id == fid)
                .cloned()
        })
        .or_else(|| {
            if state.skenario_table.filter_operasi_id.is_some() {
                state.skenario_table.operasi.clone()
            } else {
                None
            }
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(10, 15, 24))
                .inner_margin(egui::Margin::symmetric(24, 18)),
        )
        .show(viewport_ui, |ui| {
            // Header row
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("< Operasi")
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        )
                        .fill(egui::Color32::from_rgb(30, 41, 59))
                        .min_size(egui::vec2(88.0, 30.0)),
                    )
                    .clicked()
                {
                    state.screen = AppScreen::Operasis;
                    trigger_load_operasis(state, api);
                }

                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let title = if let Some(op) = &filtered_operasi {
                            format!("Daftar Skenario — {}", op.name)
                        } else {
                            "Daftar Skenario (Semua Operasi)".to_string()
                        };
                        ui.label(
                            egui::RichText::new(title)
                                .size(21.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        if let Some(satker) =
                            filtered_operasi.as_ref().and_then(|o| o.satker_name.as_ref())
                        {
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(30, 41, 59))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(8, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(satker)
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(125, 211, 252)),
                                    );
                                });
                        }
                    });
                    let subtitle = filtered_operasi
                        .as_ref()
                        .and_then(|o| o.description.as_deref())
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(
                            "Filter berdasarkan operasi, edit konfigurasi peta, atau buka Editor Plot Skenario 3D.",
                        );
                    ui.label(
                        egui::RichText::new(subtitle)
                            .size(12.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if can_write_admin && !state.offline_preview {
                        let add_btn = egui::Button::new(
                            egui::RichText::new("+ Tambah Skenario")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .min_size(egui::vec2(148.0, 32.0));
                        if ui.add(add_btn).clicked() {
                            let presets = state.location_presets.clone();
                            state.skenario_table.open_create(&presets);
                        }

                        let preset_btn = egui::Button::new(
                            egui::RichText::new("Master Preset Lokasi")
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        )
                        .fill(egui::Color32::from_rgb(30, 41, 59))
                        .min_size(egui::vec2(142.0, 32.0));
                        if ui.add(preset_btn).clicked() {
                            let next_sort = state
                                .location_presets
                                .iter()
                                .map(|p| p.sort_order)
                                .max()
                                .unwrap_or(50)
                                + 10;
                            state.preset_manager.reset_form(next_sort);
                            state.preset_manager.open = true;
                            state.preset_manager.success = None;
                        }
                    }

                    if ui
                        .add_enabled(
                            !state.skenario_table.loading,
                            egui::Button::new("Refresh").min_size(egui::vec2(82.0, 32.0)),
                        )
                        .clicked()
                    {
                        trigger_load_operasi_options(state, api);
                        trigger_load_skenarios(state, api);
                        if let Some(auth) = current_auth(state) {
                            api.send(ApiCommand::LoadLocationPresets { auth });
                        }
                    }
                });
            });

            ui.add_space(12.0);

            // Filter Operasi + Search toolbar
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 39))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 49, 71)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        // Filter Operasi ComboBox
                        ui.label(
                            egui::RichText::new("Filter Operasi:")
                                .size(12.0)
                                .strong()
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        );

                        let selected_op_text = state
                            .skenario_table
                            .filter_operasi_id
                            .and_then(|fid| {
                                state
                                    .skenario_table
                                    .operasi_options
                                    .iter()
                                    .find(|o| o.id == fid)
                                    .map(|o| format!("{} ({})", o.name, o.skenario_count))
                                    .or_else(|| {
                                        state
                                            .skenario_table
                                            .operasi
                                            .as_ref()
                                            .filter(|o| o.id == fid)
                                            .map(|o| o.name.clone())
                                    })
                            })
                            .unwrap_or_else(|| "Semua Operasi".to_string());

                        let prev_filter_op = state.skenario_table.filter_operasi_id;
                        let op_options = state.skenario_table.operasi_options.clone();
                        egui::ComboBox::from_id_salt("skenario_filter_operasi")
                            .selected_text(selected_op_text)
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.skenario_table.filter_operasi_id,
                                    None,
                                    "Semua Operasi",
                                );
                                for op in &op_options {
                                    ui.selectable_value(
                                        &mut state.skenario_table.filter_operasi_id,
                                        Some(op.id),
                                        format!("{} ({})", op.name, op.skenario_count),
                                    );
                                }
                            });

                        if state.skenario_table.filter_operasi_id != prev_filter_op {
                            let new_fid = state.skenario_table.filter_operasi_id;
                            state.selected_operasi_id = new_fid;
                            state.skenario_table.operasi = new_fid.and_then(|fid| {
                                state
                                    .skenario_table
                                    .operasi_options
                                    .iter()
                                    .find(|o| o.id == fid)
                                    .cloned()
                            });
                            state.skenario_table.page = 1;
                            trigger_load_skenarios(state, api);
                        }

                        ui.separator();

                        // Search input
                        ui.label(
                            egui::RichText::new("Cari:")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut state.skenario_table.search)
                                .hint_text("Cari nama skenario, operasi, mode, atau layer...")
                                .desired_width(270.0),
                        );
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Cari").clicked() || enter {
                            state.skenario_table.applied_search =
                                state.skenario_table.search.trim().to_string();
                            state.skenario_table.page = 1;
                            trigger_load_skenarios(state, api);
                        }
                        if (!state.skenario_table.applied_search.is_empty()
                            || state.skenario_table.filter_operasi_id.is_some())
                            && ui.button("Reset Filter").clicked()
                        {
                            state.skenario_table.search.clear();
                            state.skenario_table.applied_search.clear();
                            state.skenario_table.filter_operasi_id = None;
                            state.skenario_table.operasi = None;
                            state.selected_operasi_id = None;
                            state.skenario_table.page = 1;
                            trigger_load_skenarios(state, api);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.skenario_table.loading {
                                ui.colored_label(
                                    egui::Color32::from_rgb(56, 189, 248),
                                    "Memuat skenario dari server...",
                                );
                            } else if state.skenario_table.pending_open_plot_id.is_some() {
                                ui.colored_label(
                                    egui::Color32::from_rgb(52, 211, 153),
                                    "Membuka Editor Plot Skenario...",
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Total: {} skenario (server-side pagination)",
                                        state.skenario_table.total
                                    ))
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }
                        });
                    });
                });

            if let Some(err) = state.skenario_table.error.clone() {
                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(69, 10, 10))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(185, 28, 28)))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.colored_label(egui::Color32::from_rgb(252, 165, 165), err);
                    });
            }

            ui.add_space(10.0);

            // Full-width Table Container
            let pagination_height = 48.0;
            let table_height = (ui.available_height() - pagination_height - 10.0).max(180.0);
            let table_outer_w = ui.available_width();
            let content_w = table_content_width(table_outer_w);

            // 8 columns: No | Nama Skenario | Operasi | Mode & Layer | Koordinat & Zoom | Objek | Diperbarui | Aksi
            let col_no = 40.0_f32;
            let col_updated = 116.0_f32;
            let col_actions = 308.0_f32;
            let total_gaps = TABLE_COL_GAP * 7.0;
            let flex_w = (content_w - col_no - col_updated - col_actions - total_gaps).max(320.0);
            let col_name = (flex_w * 0.24).max(100.0);
            let col_operasi = (flex_w * 0.20).max(90.0);
            let col_mode = (flex_w * 0.17).max(82.0);
            let col_coord = (flex_w * 0.17).max(82.0);
            let col_entities = (flex_w - col_name - col_operasi - col_mode - col_coord).max(86.0);
            let col_widths = [
                col_no,
                col_name,
                col_operasi,
                col_mode,
                col_coord,
                col_entities,
                col_updated,
                col_actions,
            ];

            let mut plot_target: Option<Skenario> = None;
            let mut edit_target: Option<Skenario> = None;
            let mut copy_target: Option<Skenario> = None;
            let mut move_target: Option<Skenario> = None;
            let mut delete_target: Option<Skenario> = None;

            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 23, 42))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 49, 71)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.set_min_width(table_outer_w);
                    ui.set_min_height(table_height);
                    ui.set_max_height(table_height);
                    ui.spacing_mut().item_spacing.y = 0.0;

                    // Full-width Table Header (exact same column widths as rows)
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(24, 34, 53))
                        .inner_margin(egui::Margin {
                            left: TABLE_PAD_LEFT as i8,
                            right: (TABLE_PAD_RIGHT + TABLE_SCROLLBAR_RESERVE) as i8,
                            top: 10,
                            bottom: 10,
                        })
                        .show(ui, |ui| {
                            ui.set_min_width(content_w);
                            draw_aligned_row(
                                ui,
                                "skenario_header_row",
                                &col_widths,
                                20.0,
                                |ui, col, _| match col {
                                    0 => draw_header_label(ui, "No"),
                                    1 => draw_header_label(ui, "Nama Skenario"),
                                    2 => draw_header_label(ui, "Operasi"),
                                    3 => draw_header_label(ui, "Mode & Layer"),
                                    4 => draw_header_label(ui, "Koordinat & Zoom"),
                                    5 => draw_header_label(ui, "Objek Skenario"),
                                    6 => draw_header_label(ui, "Diperbarui"),
                                    7 => draw_header_label(ui, "Aksi"),
                                    _ => {}
                                },
                            );
                        });

                    let (sep_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 1.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        sep_rect,
                        0.0,
                        egui::Color32::from_rgb(45, 60, 86),
                    );

                    // Scrollable Table Body
                    egui::ScrollArea::vertical()
                        .id_salt("skenario_table_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;

                            if state.skenarios.is_empty() {
                                ui.add_space(48.0);
                                ui.vertical_centered(|ui| {
                                    let msg = if state.skenario_table.loading {
                                        "Sedang memuat daftar skenario..."
                                    } else if !state.skenario_table.applied_search.is_empty() {
                                        "Tidak ada skenario yang cocok dengan pencarian."
                                    } else {
                                        "Belum ada skenario. Klik '+ Tambah Skenario' untuk membuat baru."
                                    };
                                    ui.label(
                                        egui::RichText::new(msg)
                                            .size(13.5)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                });
                                return;
                            }

                            let base_index = (state.skenario_table.page.max(1) - 1)
                                * state.skenario_table.per_page.max(1);
                            let active_id = state.active_skenario.as_ref().map(|s| s.id);
                            let items = state.skenarios.clone();
                            let op_options = state.skenario_table.operasi_options.clone();

                            for (idx, sk) in items.iter().enumerate() {
                                let row_no = base_index + idx as i64 + 1;
                                let is_active = active_id == Some(sk.id);
                                let bg = if is_active {
                                    egui::Color32::from_rgb(17, 37, 56)
                                } else if idx % 2 == 0 {
                                    egui::Color32::from_rgb(15, 23, 42)
                                } else {
                                    egui::Color32::from_rgb(19, 29, 49)
                                };

                                let op_label = sk
                                    .operasi_name
                                    .clone()
                                    .or_else(|| {
                                        op_options
                                            .iter()
                                            .find(|o| o.id == sk.operasi_id)
                                            .map(|o| o.name.clone())
                                    })
                                    .unwrap_or_else(|| "—".to_string());

                                egui::Frame::new()
                                    .fill(bg)
                                    .inner_margin(egui::Margin {
                                        left: TABLE_PAD_LEFT as i8,
                                        right: TABLE_PAD_RIGHT as i8,
                                        top: 10,
                                        bottom: 10,
                                    })
                                    .show(ui, |ui| {
                                        ui.set_min_width(content_w);
                                        draw_aligned_row(
                                            ui,
                                            sk.id,
                                            &col_widths,
                                            26.0,
                                            |ui, col, _| match col {
                                                0 => {
                                                    ui.label(
                                                        egui::RichText::new(format!("{row_no}"))
                                                            .size(12.0)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                    );
                                                }
                                                1 => {
                                                    let name_resp = ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(&sk.name)
                                                                .size(13.0)
                                                                .strong()
                                                                .color(egui::Color32::WHITE),
                                                        )
                                                        .wrap()
                                                        .sense(egui::Sense::click()),
                                                    );
                                                    if name_resp
                                                        .on_hover_text(
                                                            "Klik untuk buka Plot Skenario",
                                                        )
                                                        .clicked()
                                                    {
                                                        plot_target = Some(sk.clone());
                                                    }
                                                }
                                                2 => {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(&op_label)
                                                                .size(12.0)
                                                                .color(egui::Color32::from_rgb(
                                                                    186, 230, 253,
                                                                )),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                3 => {
                                                    let (mode_txt, mode_bg, mode_fg) =
                                                        match sk.map_mode {
                                                            MapMode::Outdoor => (
                                                                "OUTDOOR",
                                                                egui::Color32::from_rgb(
                                                                    12, 74, 110,
                                                                ),
                                                                egui::Color32::from_rgb(
                                                                    125, 211, 252,
                                                                ),
                                                            ),
                                                            MapMode::Indoor => (
                                                                "INDOOR",
                                                                egui::Color32::from_rgb(
                                                                    76, 29, 149,
                                                                ),
                                                                egui::Color32::from_rgb(
                                                                    216, 180, 254,
                                                                ),
                                                            ),
                                                        };
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.spacing_mut().item_spacing =
                                                            egui::vec2(5.0, 3.0);
                                                        egui::Frame::new()
                                                            .fill(mode_bg)
                                                            .corner_radius(4.0)
                                                            .inner_margin(egui::Margin::symmetric(
                                                                6, 2,
                                                            ))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    egui::RichText::new(mode_txt)
                                                                        .size(10.0)
                                                                        .strong()
                                                                        .color(mode_fg),
                                                                );
                                                            });
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(
                                                                    sk.base_layer.label(),
                                                                )
                                                                .size(11.5)
                                                                .color(egui::Color32::from_rgb(
                                                                    203, 213, 225,
                                                                )),
                                                            )
                                                            .wrap(),
                                                        );
                                                    });
                                                }
                                                4 => {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(format!(
                                                                "{:.4}, {:.4}\nZoom {:.1}",
                                                                sk.center_lat,
                                                                sk.center_lng,
                                                                sk.zoom
                                                            ))
                                                            .size(11.5)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                5 => {
                                                    let has_content = sk.has_simulation_content();
                                                    let summary = sk.content_summary();
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(summary)
                                                                .size(11.0)
                                                                .color(if has_content {
                                                                    egui::Color32::from_rgb(
                                                                        186, 230, 253,
                                                                    )
                                                                } else {
                                                                    egui::Color32::from_rgb(
                                                                        100, 116, 139,
                                                                    )
                                                                }),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                6 => {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(format_timestamp(
                                                                sk.updated_at,
                                                            ))
                                                            .size(11.5)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                7 => {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.spacing_mut().item_spacing =
                                                            egui::vec2(6.0, 4.0);
                                                        let is_opening = state
                                                            .skenario_table
                                                            .pending_open_plot_id
                                                            == Some(sk.id);
                                                        let plot_btn = egui::Button::new(
                                                            egui::RichText::new(if is_opening {
                                                                "Membuka..."
                                                            } else {
                                                                "Plot Skenario"
                                                            })
                                                            .size(11.5)
                                                            .strong()
                                                            .color(egui::Color32::WHITE),
                                                        )
                                                        .fill(egui::Color32::from_rgb(5, 150, 105))
                                                        .min_size(egui::vec2(96.0, 24.0));
                                                        if ui
                                                            .add_enabled(!is_opening, plot_btn)
                                                            .on_hover_text(
                                                                "Buka Editor 3D Plot Skenario (Godot/Unity Style)",
                                                            )
                                                            .clicked()
                                                        {
                                                            plot_target = Some(sk.clone());
                                                        }

                                                        if can_edit_skenario
                                                            && !state.offline_preview
                                                        {
                                                            let edit_btn = egui::Button::new(
                                                                egui::RichText::new("Edit")
                                                                    .size(11.5)
                                                                    .color(egui::Color32::from_rgb(
                                                                        254, 240, 138,
                                                                    )),
                                                            )
                                                            .fill(egui::Color32::from_rgb(
                                                                113, 63, 18,
                                                            ))
                                                            .min_size(egui::vec2(46.0, 24.0));
                                                            if ui.add(edit_btn).clicked() {
                                                                edit_target = Some(sk.clone());
                                                            }
                                                        }

                                                        if can_write_admin && !state.offline_preview
                                                        {
                                                            let copy_btn = egui::Button::new(
                                                                egui::RichText::new("Salin")
                                                                    .size(11.5)
                                                                    .color(egui::Color32::from_rgb(
                                                                        186, 230, 253,
                                                                    )),
                                                            )
                                                            .fill(egui::Color32::from_rgb(
                                                                14, 116, 144,
                                                            ))
                                                            .min_size(egui::vec2(48.0, 24.0));
                                                            if ui
                                                                .add(copy_btn)
                                                                .on_hover_text(
                                                                    "Salin skenario beserta seluruh objek 3D, pengaturan peta & animasi ke operasi ini atau operasi lain",
                                                                )
                                                                .clicked()
                                                            {
                                                                copy_target = Some(sk.clone());
                                                            }

                                                            let move_btn = egui::Button::new(
                                                                egui::RichText::new("Pindah")
                                                                    .size(11.5)
                                                                    .color(egui::Color32::from_rgb(
                                                                        233, 213, 255,
                                                                    )),
                                                            )
                                                            .fill(egui::Color32::from_rgb(
                                                                88, 28, 135,
                                                            ))
                                                            .min_size(egui::vec2(54.0, 24.0));
                                                            if ui
                                                                .add(move_btn)
                                                                .on_hover_text(
                                                                    "Pindahkan skenario & seluruh isinya ke operasi lain",
                                                                )
                                                                .clicked()
                                                            {
                                                                move_target = Some(sk.clone());
                                                            }

                                                            let can_delete_sk =
                                                                !sk.has_simulation_content();
                                                            let del_btn = egui::Button::new(
                                                                egui::RichText::new("Hapus")
                                                                    .size(11.5)
                                                                    .color(if can_delete_sk {
                                                                        egui::Color32::from_rgb(
                                                                            254, 202, 202,
                                                                        )
                                                                    } else {
                                                                        egui::Color32::from_rgb(
                                                                            148, 163, 184,
                                                                        )
                                                                    }),
                                                            )
                                                            .fill(if can_delete_sk {
                                                                egui::Color32::from_rgb(
                                                                    127, 29, 29,
                                                                )
                                                            } else {
                                                                egui::Color32::from_rgb(30, 41, 59)
                                                            })
                                                            .min_size(egui::vec2(52.0, 24.0));
                                                            if ui
                                                                .add_enabled(can_delete_sk, del_btn)
                                                                .on_disabled_hover_text(format!(
                                                                    "Tidak dapat dihapus: skenario masih memiliki isi simulasi ({}). Kosongkan di Plot Skenario terlebih dahulu.",
                                                                    sk.content_summary()
                                                                ))
                                                                .clicked()
                                                            {
                                                                delete_target = Some(sk.clone());
                                                            }
                                                        }
                                                    });
                                                }
                                                _ => {}
                                            },
                                        );
                                    });

                                let (line_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 1.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    line_rect,
                                    0.0,
                                    egui::Color32::from_rgb(28, 39, 59),
                                );
                            }
                        });
                });

            if let Some(sk) = plot_target {
                open_plot_skenario(state, playback, api, &sk);
            }
            if let Some(sk) = edit_target {
                state.skenario_table.open_edit(&sk);
            }
            if let Some(sk) = copy_target {
                trigger_load_operasi_options(state, api);
                state.skenario_table.open_copy(&sk);
            }
            if let Some(sk) = move_target {
                trigger_load_operasi_options(state, api);
                state.skenario_table.open_move(&sk);
            }
            if let Some(sk) = delete_target {
                state.skenario_table.open_delete(&sk);
            }

            ui.add_space(10.0);

            // Server-side pagination footer
            let pag = draw_pagination_bar(
                ui,
                "skenario_pagination",
                state.skenario_table.page,
                state.skenario_table.per_page,
                state.skenario_table.total,
                state.skenarios.len(),
                state.skenario_table.loading,
                "skenario",
            );
            if let Some(new_per) = pag.new_per_page {
                state.skenario_table.per_page = new_per;
                state.skenario_table.page = 1;
                trigger_load_skenarios(state, api);
            } else if let Some(new_page) = pag.new_page {
                state.skenario_table.page = new_page;
                trigger_load_skenarios(state, api);
            }
        });

    draw_skenario_modals(ctx, state, api);
    if state.preset_manager.open {
        draw_location_preset_manager_modal(ctx, state, api, false);
    }
}

fn draw_location_preset_manager_modal(
    ctx: &egui::Context,
    state: &mut AppState,
    api: &ApiClientBridge,
    opened_from_skenario_form: bool,
) {
    draw_modal_backdrop(ctx, "location_preset_manager_backdrop");
    let mut close = false;
    let mut apply_preset_to_form: Option<LocationPreset> = None;
    let mut load_into_edit: Option<LocationPreset> = None;
    let mut delete_id: Option<Uuid> = None;
    let mut submit_form = false;

    egui::Area::new(egui::Id::new("location_preset_manager_modal"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 23, 42))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.set_min_width(860.0);
                    ui.set_max_width(920.0);

                    // Modal header
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Master Data — Preset Lokasi Cepat")
                                    .size(18.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Tambah, edit, atau hapus daftar preset lokasi untuk digunakan saat membuat skenario.",
                                )
                                .size(11.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("X Tutup").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.columns(2, |cols| {
                        // Left column: Master Preset List
                        cols[0].horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Daftar Preset ({})",
                                    state.location_presets.len()
                                ))
                                .size(13.0)
                                .strong()
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new("+ Preset Baru")
                                                .fill(egui::Color32::from_rgb(14, 116, 144)),
                                        )
                                        .clicked()
                                    {
                                        let next_sort = state
                                            .location_presets
                                            .iter()
                                            .map(|p| p.sort_order)
                                            .max()
                                            .unwrap_or(50)
                                            + 10;
                                        state.preset_manager.reset_form(next_sort);
                                        if opened_from_skenario_form {
                                            state.preset_manager.copy_from_skenario_form(
                                                &state.skenario_table,
                                            );
                                        }
                                    }
                                },
                            );
                        });
                        cols[0].add_space(6.0);

                        let presets_snapshot = state.location_presets.clone();
                        egui::ScrollArea::vertical()
                            .id_salt("master_presets_list_scroll")
                            .max_height(430.0)
                            .auto_shrink([false, false])
                            .show(&mut cols[0], |ui| {
                                if presets_snapshot.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Belum ada preset lokasi.")
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    return;
                                }
                                for p in &presets_snapshot {
                                    let is_editing = state.preset_manager.editing_id == Some(p.id);
                                    let bg = if is_editing {
                                        egui::Color32::from_rgb(17, 45, 72)
                                    } else {
                                        egui::Color32::from_rgb(22, 31, 48)
                                    };
                                    egui::Frame::new()
                                        .fill(bg)
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            if is_editing {
                                                egui::Color32::from_rgb(56, 189, 248)
                                            } else {
                                                egui::Color32::from_rgb(38, 52, 74)
                                            },
                                        ))
                                        .corner_radius(6.0)
                                        .inner_margin(egui::Margin::same(8))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&p.name)
                                                        .size(12.5)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "#{}",
                                                                p.sort_order
                                                            ))
                                                            .size(10.5)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                        );
                                                    },
                                                );
                                            });
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{:.4}, {:.4} • Z{:.1} ({:.0}-{:.0}) • {} / {}",
                                                    p.center_lat,
                                                    p.center_lng,
                                                    p.zoom,
                                                    p.min_zoom,
                                                    p.max_zoom,
                                                    p.map_mode.as_str(),
                                                    p.base_layer.label(),
                                                ))
                                                .size(10.5)
                                                .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                            ui.add_space(4.0);
                                            ui.horizontal_wrapped(|ui| {
                                                if opened_from_skenario_form
                                                    && ui
                                                        .add(
                                                            egui::Button::new("Pakai di Peta")
                                                                .small()
                                                                .fill(egui::Color32::from_rgb(
                                                                    5, 150, 105,
                                                                )),
                                                        )
                                                        .clicked()
                                                {
                                                    apply_preset_to_form = Some(p.clone());
                                                }
                                                if ui
                                                    .add(
                                                        egui::Button::new("Edit")
                                                            .small()
                                                            .fill(egui::Color32::from_rgb(
                                                                113, 63, 18,
                                                            )),
                                                    )
                                                    .clicked()
                                                {
                                                    load_into_edit = Some(p.clone());
                                                }
                                                if state.preset_manager.confirm_delete_id
                                                    == Some(p.id)
                                                {
                                                    if ui
                                                        .add_enabled(
                                                            !state.preset_manager.submitting,
                                                            egui::Button::new("Konfirmasi Hapus")
                                                                .small()
                                                                .fill(egui::Color32::from_rgb(
                                                                    185, 28, 28,
                                                                )),
                                                        )
                                                        .clicked()
                                                    {
                                                        delete_id = Some(p.id);
                                                    }
                                                    if ui.small_button("Batal").clicked() {
                                                        state.preset_manager.confirm_delete_id =
                                                            None;
                                                    }
                                                } else if ui
                                                    .add(
                                                        egui::Button::new("Hapus")
                                                            .small()
                                                            .fill(egui::Color32::from_rgb(
                                                                127, 29, 29,
                                                            )),
                                                    )
                                                    .clicked()
                                                {
                                                    state.preset_manager.confirm_delete_id =
                                                        Some(p.id);
                                                }
                                            });
                                        });
                                    ui.add_space(6.0);
                                }
                            });

                        // Right column: Create / Edit Master Preset Form
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(20, 29, 46))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
                            .corner_radius(7.0)
                            .inner_margin(egui::Margin::same(12))
                            .show(&mut cols[1], |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let form_title = if state.preset_manager.editing_id.is_some() {
                                        "Edit Master Preset"
                                    } else {
                                        "Tambah Master Preset Baru"
                                    };
                                    ui.label(
                                        egui::RichText::new(form_title)
                                            .size(13.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(56, 189, 248)),
                                    );
                                    if opened_from_skenario_form {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("Ambil dari Peta Aktif")
                                                    .clicked()
                                                {
                                                    state.preset_manager.copy_from_skenario_form(
                                                        &state.skenario_table,
                                                    );
                                                }
                                            },
                                        );
                                    }
                                });
                                ui.separator();
                                ui.add_space(6.0);

                                ui.label(
                                    egui::RichText::new("Nama Preset *")
                                        .size(11.5)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.preset_manager.name)
                                        .hint_text("Contoh: Bandara Ngurah Rai")
                                        .desired_width(f32::INFINITY),
                                );
                                ui.add_space(6.0);

                                ui.columns(2, |c| {
                                    c[0].label(egui::RichText::new("Latitude").size(11.0));
                                    numeric_spinner(
                                        &mut c[0],
                                        "mp_lat",
                                        &mut state.preset_manager.center_lat,
                                        SpinnerConfig {
                                            min: -85.0,
                                            max: 85.0,
                                            step: 0.001,
                                            drag_speed: 0.0005,
                                            decimals: 5,
                                            suffix: "",
                                            wrap_around: false,
                                        },
                                    );
                                    c[1].label(egui::RichText::new("Longitude").size(11.0));
                                    numeric_spinner(
                                        &mut c[1],
                                        "mp_lng",
                                        &mut state.preset_manager.center_lng,
                                        SpinnerConfig {
                                            min: -180.0,
                                            max: 180.0,
                                            step: 0.001,
                                            drag_speed: 0.0005,
                                            decimals: 5,
                                            suffix: "",
                                            wrap_around: false,
                                        },
                                    );
                                });
                                ui.add_space(6.0);

                                ui.columns(3, |c| {
                                    c[0].label(egui::RichText::new("Zoom").size(11.0));
                                    if numeric_spinner(
                                        &mut c[0],
                                        "mp_zoom",
                                        &mut state.preset_manager.zoom,
                                        SpinnerConfig {
                                            min: 1.0,
                                            max: 26.0,
                                            step: 0.5,
                                            drag_speed: 0.1,
                                            decimals: 1,
                                            suffix: "",
                                            wrap_around: false,
                                        },
                                    ) {
                                        state.preset_manager.clamp_zoom_bounds();
                                    }
                                    c[1].label(egui::RichText::new("Min Zoom").size(11.0));
                                    if numeric_spinner(
                                        &mut c[1],
                                        "mp_min_zoom",
                                        &mut state.preset_manager.min_zoom,
                                        SpinnerConfig {
                                            min: 1.0,
                                            max: 26.0,
                                            step: 0.5,
                                            drag_speed: 0.1,
                                            decimals: 1,
                                            suffix: "",
                                            wrap_around: false,
                                        },
                                    ) {
                                        if state.preset_manager.max_zoom
                                            < state.preset_manager.min_zoom
                                        {
                                            state.preset_manager.max_zoom =
                                                state.preset_manager.min_zoom;
                                        }
                                        state.preset_manager.clamp_zoom_bounds();
                                    }
                                    c[2].label(egui::RichText::new("Max Zoom").size(11.0));
                                    if numeric_spinner(
                                        &mut c[2],
                                        "mp_max_zoom",
                                        &mut state.preset_manager.max_zoom,
                                        SpinnerConfig {
                                            min: 1.0,
                                            max: 26.0,
                                            step: 0.5,
                                            drag_speed: 0.1,
                                            decimals: 1,
                                            suffix: "",
                                            wrap_around: false,
                                        },
                                    ) {
                                        if state.preset_manager.min_zoom
                                            > state.preset_manager.max_zoom
                                        {
                                            state.preset_manager.min_zoom =
                                                state.preset_manager.max_zoom;
                                        }
                                        state.preset_manager.clamp_zoom_bounds();
                                    }
                                });
                                ui.add_space(6.0);

                                ui.columns(3, |c| {
                                    c[0].label(egui::RichText::new("Pitch (°)").size(11.0));
                                    numeric_spinner(
                                        &mut c[0],
                                        "mp_pitch",
                                        &mut state.preset_manager.pitch,
                                        SpinnerConfig {
                                            min: 5.0,
                                            max: 82.0,
                                            step: 2.0,
                                            drag_speed: 0.5,
                                            decimals: 1,
                                            suffix: "°",
                                            wrap_around: false,
                                        },
                                    );
                                    c[1].label(egui::RichText::new("Bearing (°)").size(11.0));
                                    numeric_spinner(
                                        &mut c[1],
                                        "mp_bearing",
                                        &mut state.preset_manager.bearing,
                                        SpinnerConfig {
                                            min: 0.0,
                                            max: 360.0,
                                            step: 5.0,
                                            drag_speed: 1.0,
                                            decimals: 1,
                                            suffix: "°",
                                            wrap_around: true,
                                        },
                                    );
                                    c[2].label(egui::RichText::new("Urutan").size(11.0));
                                    numeric_spinner_i32(
                                        &mut c[2],
                                        "mp_sort_order",
                                        &mut state.preset_manager.sort_order,
                                        0,
                                        9999,
                                        10,
                                    );
                                });
                                ui.add_space(6.0);

                                let prev_mode = state.preset_manager.map_mode;
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Mode:").size(11.5));
                                    ui.selectable_value(
                                        &mut state.preset_manager.map_mode,
                                        MapMode::Outdoor,
                                        "Outdoor",
                                    );
                                    ui.selectable_value(
                                        &mut state.preset_manager.map_mode,
                                        MapMode::Indoor,
                                        "Indoor",
                                    );
                                });
                                if state.preset_manager.map_mode != prev_mode {
                                    state.preset_manager.base_layer =
                                        match state.preset_manager.map_mode {
                                            MapMode::Outdoor => BaseLayer::Osm,
                                            MapMode::Indoor => BaseLayer::GridDark,
                                        };
                                }

                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Layer:").size(11.5));
                                    egui::ComboBox::from_id_salt("mp_base_layer")
                                        .selected_text(state.preset_manager.base_layer.label())
                                        .width(190.0)
                                        .show_ui(ui, |ui| {
                                            let layers: &[BaseLayer] =
                                                match state.preset_manager.map_mode {
                                                    MapMode::Outdoor => {
                                                        &[BaseLayer::Osm, BaseLayer::Satellite]
                                                    }
                                                    MapMode::Indoor => &[
                                                        BaseLayer::GridDark,
                                                        BaseLayer::GridLight,
                                                        BaseLayer::BlankDark,
                                                        BaseLayer::BlankLight,
                                                        BaseLayer::FloorPlan,
                                                    ],
                                                };
                                            for layer in layers {
                                                ui.selectable_value(
                                                    &mut state.preset_manager.base_layer,
                                                    *layer,
                                                    layer.label(),
                                                );
                                            }
                                        });
                                });

                                if let Some(err) = &state.preset_manager.error {
                                    ui.add_space(6.0);
                                    ui.colored_label(
                                        egui::Color32::from_rgb(248, 113, 113),
                                        egui::RichText::new(err).size(11.5),
                                    );
                                }
                                if let Some(ok) = &state.preset_manager.success {
                                    ui.add_space(6.0);
                                    ui.colored_label(
                                        egui::Color32::from_rgb(74, 222, 128),
                                        egui::RichText::new(ok).size(11.5),
                                    );
                                }

                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    let btn_lbl = if state.preset_manager.submitting {
                                        "Menyimpan..."
                                    } else if state.preset_manager.editing_id.is_some() {
                                        "Update Preset"
                                    } else {
                                        "Simpan Preset Baru"
                                    };
                                    if ui
                                        .add_enabled(
                                            !state.preset_manager.submitting,
                                            egui::Button::new(
                                                egui::RichText::new(btn_lbl)
                                                    .strong()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(5, 150, 105))
                                            .min_size(egui::vec2(148.0, 28.0)),
                                        )
                                        .clicked()
                                    {
                                        submit_form = true;
                                    }
                                    if state.preset_manager.editing_id.is_some()
                                        && ui.button("Mode Tambah").clicked()
                                    {
                                        let next_sort = state
                                            .location_presets
                                            .iter()
                                            .map(|p| p.sort_order)
                                            .max()
                                            .unwrap_or(50)
                                            + 10;
                                        state.preset_manager.reset_form(next_sort);
                                    }
                                });
                            });
                    });
                });
        });

    if let Some(p) = apply_preset_to_form {
        state.skenario_table.apply_location_preset(&p);
        state.preset_manager.open = false;
        return;
    }
    if let Some(p) = load_into_edit {
        state.preset_manager.load_preset(&p);
    }
    if let Some(id) = delete_id
        && let Some(auth) = current_auth(state)
    {
        state.preset_manager.submitting = true;
        state.preset_manager.error = None;
        api.send(ApiCommand::DeleteLocationPreset { auth, id });
    }
    if submit_form {
        state.preset_manager.clamp_zoom_bounds();
        let name = state.preset_manager.name.trim().to_string();
        if name.is_empty() {
            state.preset_manager.error = Some("Nama preset wajib diisi.".into());
        } else if let Some(auth) = current_auth(state) {
            state.preset_manager.submitting = true;
            state.preset_manager.error = None;
            state.preset_manager.success = None;
            if let Some(id) = state.preset_manager.editing_id {
                api.send(ApiCommand::UpdateLocationPreset {
                    auth,
                    id,
                    req: UpdateLocationPresetRequest {
                        name,
                        center_lng: state.preset_manager.center_lng,
                        center_lat: state.preset_manager.center_lat,
                        zoom: state.preset_manager.zoom,
                        min_zoom: state.preset_manager.min_zoom,
                        max_zoom: state.preset_manager.max_zoom,
                        pitch: state.preset_manager.pitch,
                        bearing: state.preset_manager.bearing,
                        map_mode: state.preset_manager.map_mode,
                        base_layer: state.preset_manager.base_layer,
                        grid_size_m: state.preset_manager.grid_size_m,
                        sort_order: state.preset_manager.sort_order,
                    },
                });
            } else {
                api.send(ApiCommand::CreateLocationPreset {
                    auth,
                    req: CreateLocationPresetRequest {
                        name,
                        center_lng: state.preset_manager.center_lng,
                        center_lat: state.preset_manager.center_lat,
                        zoom: state.preset_manager.zoom,
                        min_zoom: state.preset_manager.min_zoom,
                        max_zoom: state.preset_manager.max_zoom,
                        pitch: state.preset_manager.pitch,
                        bearing: state.preset_manager.bearing,
                        map_mode: state.preset_manager.map_mode,
                        base_layer: state.preset_manager.base_layer,
                        grid_size_m: state.preset_manager.grid_size_m,
                        sort_order: state.preset_manager.sort_order,
                    },
                });
            }
        }
    }
    if close {
        state.preset_manager.open = false;
    }
}

fn draw_skenario_modals(ctx: &egui::Context, state: &mut AppState, api: &ApiClientBridge) {
    let mode = state.skenario_table.modal_mode.clone();
    match mode {
        SkenarioModalMode::None | SkenarioModalMode::Create | SkenarioModalMode::Edit(_) => {}
        SkenarioModalMode::Copy(sk) => {
            draw_modal_backdrop(ctx, "skenario_copy_backdrop");
            let mut close = false;
            let mut submit_copy = false;
            let op_options = state.skenario_table.operasi_options.clone();
            if state.skenario_table.form_operasi_id.is_none() {
                state.skenario_table.form_operasi_id = Some(sk.operasi_id);
            }

            egui::Area::new(egui::Id::new("skenario_copy_modal"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(56, 189, 248),
                        ))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::same(22))
                        .show(ui, |ui| {
                            ui.set_min_width(460.0);
                            ui.set_max_width(520.0);

                            ui.label(
                                egui::RichText::new("Salin Skenario & Seluruh Isinya")
                                    .size(17.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Sumber: \"{}\"",
                                    sk.name
                                ))
                                .size(12.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Akan menyalin pengaturan peta (Zoom {:.1}, Pitch {:.0}°, Bearing {:.0}°, {}, {}) serta seluruh isi simulasi: {}.",
                                    sk.zoom,
                                    sk.pitch,
                                    sk.bearing,
                                    sk.base_layer.label(),
                                    sk.skybox.short_label(),
                                    sk.content_summary()
                                ))
                                .size(11.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                            ui.add_space(12.0);

                            ui.label(
                                egui::RichText::new("Nama Skenario Hasil Salinan *")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(203, 213, 225)),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut state.skenario_table.form_name)
                                    .hint_text("Contoh: Pengamanan Rute VIP (Salinan)")
                                    .desired_width(f32::INFINITY),
                            );
                            ui.add_space(10.0);

                            ui.label(
                                egui::RichText::new("Operasi Tujuan *")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(203, 213, 225)),
                            );
                            let selected_op_label = state
                                .skenario_table
                                .form_operasi_id
                                .and_then(|oid| {
                                    op_options.iter().find(|o| o.id == oid).map(|o| {
                                        if o.id == sk.operasi_id {
                                            format!("{} (Operasi saat ini)", o.name)
                                        } else {
                                            o.name.clone()
                                        }
                                    })
                                })
                                .or_else(|| {
                                    sk.operasi_name
                                        .as_ref()
                                        .map(|n| format!("{n} (Operasi saat ini)"))
                                })
                                .unwrap_or_else(|| "Pilih Operasi Tujuan...".to_string());

                            egui::ComboBox::from_id_salt("skenario_copy_target_operasi")
                                .selected_text(selected_op_label)
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    for op in &op_options {
                                        let lbl = if op.id == sk.operasi_id {
                                            format!("{} (Operasi saat ini)", op.name)
                                        } else {
                                            op.name.clone()
                                        };
                                        ui.selectable_value(
                                            &mut state.skenario_table.form_operasi_id,
                                            Some(op.id),
                                            lbl,
                                        );
                                    }
                                });

                            if let Some(err) = &state.skenario_table.form_error {
                                ui.add_space(8.0);
                                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                            }

                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let copy_btn = egui::Button::new(
                                            egui::RichText::new(if state.skenario_table.submitting {
                                                "Menyalin..."
                                            } else {
                                                "Salin Skenario"
                                            })
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(14, 116, 144))
                                        .min_size(egui::vec2(126.0, 30.0));
                                        if ui
                                            .add_enabled(!state.skenario_table.submitting, copy_btn)
                                            .clicked()
                                        {
                                            submit_copy = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                !state.skenario_table.submitting,
                                                egui::Button::new("Batal")
                                                    .min_size(egui::vec2(76.0, 30.0)),
                                            )
                                            .clicked()
                                        {
                                            close = true;
                                        }
                                    },
                                );
                            });
                        });
                });

            if submit_copy {
                let new_name = state.skenario_table.form_name.trim().to_string();
                let target_op = state
                    .skenario_table
                    .form_operasi_id
                    .unwrap_or(sk.operasi_id);
                if new_name.is_empty() {
                    state.skenario_table.form_error =
                        Some("Nama skenario hasil salinan wajib diisi.".into());
                } else if let Some(auth) = current_auth(state) {
                    state.skenario_table.submitting = true;
                    state.skenario_table.form_error = None;
                    api.send(ApiCommand::CopySkenario {
                        auth,
                        id: sk.id,
                        req: CopySkenarioRequest {
                            target_operasi_id: target_op,
                            new_name: Some(new_name),
                        },
                    });
                }
            } else if close {
                state.skenario_table.close_modal();
            }
        }
        SkenarioModalMode::Move(sk) => {
            draw_modal_backdrop(ctx, "skenario_move_backdrop");
            let mut close = false;
            let mut submit_move = false;
            let other_ops: Vec<_> = state
                .skenario_table
                .operasi_options
                .iter()
                .filter(|o| o.id != sk.operasi_id)
                .cloned()
                .collect();
            if state
                .skenario_table
                .form_operasi_id
                .is_none_or(|oid| oid == sk.operasi_id || !other_ops.iter().any(|o| o.id == oid))
            {
                state.skenario_table.form_operasi_id = other_ops.first().map(|o| o.id);
            }

            egui::Area::new(egui::Id::new("skenario_move_modal"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(168, 85, 247),
                        ))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::same(22))
                        .show(ui, |ui| {
                            ui.set_min_width(450.0);
                            ui.set_max_width(510.0);

                            ui.label(
                                egui::RichText::new("Pindah Skenario ke Operasi Lain")
                                    .size(17.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(216, 180, 254)),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Skenario: \"{}\"",
                                    sk.name
                                ))
                                .size(12.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                            );
                            let cur_op_name = sk
                                .operasi_name
                                .as_deref()
                                .unwrap_or("Operasi saat ini");
                            ui.label(
                                egui::RichText::new(format!(
                                    "Dari operasi \"{cur_op_name}\" — seluruh isi ({}) ikut dipindahkan.",
                                    sk.content_summary()
                                ))
                                .size(11.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                            ui.add_space(12.0);

                            ui.label(
                                egui::RichText::new("Operasi Tujuan *")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(203, 213, 225)),
                            );
                            if other_ops.is_empty() {
                                ui.colored_label(
                                    egui::Color32::from_rgb(250, 204, 21),
                                    "Belum ada operasi tujuan lain yang tersedia. Buat operasi baru terlebih dahulu di halaman Operasi.",
                                );
                            } else {
                                let selected_label = state
                                    .skenario_table
                                    .form_operasi_id
                                    .and_then(|oid| {
                                        other_ops.iter().find(|o| o.id == oid).map(|o| o.name.clone())
                                    })
                                    .unwrap_or_else(|| "Pilih Operasi Tujuan...".to_string());
                                egui::ComboBox::from_id_salt("skenario_move_target_operasi")
                                    .selected_text(selected_label)
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for op in &other_ops {
                                            ui.selectable_value(
                                                &mut state.skenario_table.form_operasi_id,
                                                Some(op.id),
                                                &op.name,
                                            );
                                        }
                                    });
                            }

                            if let Some(err) = &state.skenario_table.form_error {
                                ui.add_space(8.0);
                                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                            }

                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let can_submit = !state.skenario_table.submitting
                                            && state.skenario_table.form_operasi_id.is_some()
                                            && !other_ops.is_empty();
                                        let move_btn = egui::Button::new(
                                            egui::RichText::new(if state.skenario_table.submitting {
                                                "Memindahkan..."
                                            } else {
                                                "Pindah Skenario"
                                            })
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(126, 34, 206))
                                        .min_size(egui::vec2(130.0, 30.0));
                                        if ui.add_enabled(can_submit, move_btn).clicked() {
                                            submit_move = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                !state.skenario_table.submitting,
                                                egui::Button::new("Batal")
                                                    .min_size(egui::vec2(76.0, 30.0)),
                                            )
                                            .clicked()
                                        {
                                            close = true;
                                        }
                                    },
                                );
                            });
                        });
                });

            if submit_move {
                if let Some(target_op) = state.skenario_table.form_operasi_id {
                    if let Some(auth) = current_auth(state) {
                        state.skenario_table.submitting = true;
                        state.skenario_table.form_error = None;
                        api.send(ApiCommand::MoveSkenario {
                            auth,
                            id: sk.id,
                            req: MoveSkenarioRequest {
                                target_operasi_id: target_op,
                            },
                        });
                    }
                } else {
                    state.skenario_table.form_error =
                        Some("Pilih operasi tujuan terlebih dahulu.".into());
                }
            } else if close {
                state.skenario_table.close_modal();
            }
        }
        SkenarioModalMode::DeleteConfirm(sk) => {
            draw_modal_backdrop(ctx, "skenario_delete_backdrop");
            let mut close = false;
            let mut confirm_delete = false;
            let has_content = sk.has_simulation_content();
            egui::Area::new(egui::Id::new("skenario_delete_modal"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(239, 68, 68)))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::same(22))
                        .show(ui, |ui| {
                            ui.set_min_width(420.0);
                            ui.set_max_width(460.0);

                            ui.label(
                                egui::RichText::new("Hapus Skenario?")
                                    .size(17.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(252, 165, 165)),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Anda yakin ingin menghapus skenario \"{}\"?",
                                    sk.name
                                ))
                                .color(egui::Color32::WHITE),
                            );
                            ui.add_space(4.0);
                            if has_content {
                                ui.colored_label(
                                    egui::Color32::from_rgb(250, 204, 21),
                                    format!(
                                        "Skenario ini tidak dapat dihapus karena masih memiliki isi simulasi: {}. Kosongkan seluruh isi di Plot Skenario terlebih dahulu.",
                                        sk.content_summary()
                                    ),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(
                                        "Skenario ini sudah kosong (tidak memiliki objek simulasi) dan akan dihapus permanen.",
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }

                            if let Some(err) = &state.skenario_table.form_error {
                                ui.add_space(8.0);
                                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                            }

                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let del_btn = egui::Button::new(
                                            egui::RichText::new(if state.skenario_table.submitting {
                                                "Menghapus..."
                                            } else {
                                                "Ya, Hapus Skenario"
                                            })
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(185, 28, 28))
                                        .min_size(egui::vec2(142.0, 30.0));
                                        if ui
                                            .add_enabled(
                                                !state.skenario_table.submitting && !has_content,
                                                del_btn,
                                            )
                                            .clicked()
                                        {
                                            confirm_delete = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                !state.skenario_table.submitting,
                                                egui::Button::new("Batal")
                                                    .min_size(egui::vec2(76.0, 30.0)),
                                            )
                                            .clicked()
                                        {
                                            close = true;
                                        }
                                    },
                                );
                            });
                        });
                });

            if confirm_delete && !has_content {
                if let Some(auth) = current_auth(state) {
                    state.skenario_table.submitting = true;
                    state.skenario_table.form_error = None;
                    api.send(ApiCommand::DeleteSkenario { auth, id: sk.id });
                }
            } else if close {
                state.skenario_table.close_modal();
            }
        }
    }
}
