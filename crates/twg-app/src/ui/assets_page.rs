use std::path::PathBuf;

use bevy_egui::egui;
use twg_shared::{
    api::{CreateModelCategoryRequest, UpdateModelCategoryRequest},
    models::{CategoryPlayBehavior, ModelAssetDto, ModelAssetType, ModelCategoryCount},
};

use crate::{
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::{ApiClientBridge, ApiCommand, ModelAssetUploadPayload},
        ws_client::WsClientBridge,
    },
    scene::{
        asset_preview::{AssetPreviewStage, resolve_preview_file_path},
        markers::{MAX_OBJECT_3D_SCALE, MIN_OBJECT_3D_SCALE, current_auth},
        media_preview::MediaPreviewPlayer,
    },
    state::{
        AppState, AssetFormMode, AssetPreviewLod, AssetPreviewVariant, AssetsSection,
        PlaybackState,
    },
    ui::{
        app_icon::draw_app_brand_icon,
        draw_search_input_with_reset,
        media_assets_ui::{
            draw_media_center_preview, draw_media_left_panel_body, draw_media_right_panel_body,
        },
        nav_bar::{draw_screen_nav_tabs, open_profile_screen},
        spinner::{SpinnerConfig, numeric_spinner, numeric_spinner_i32},
    },
};

pub struct AssetsLayoutInsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

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

fn pick_gltf_file(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("3D glTF / GLB Model", &["glb", "gltf"])
        .pick_file()
}

#[allow(clippy::too_many_arguments)]
pub fn draw_assets_screen(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    playback: &mut PlaybackState,
    stage: &mut AssetPreviewStage,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) -> AssetsLayoutInsets {
    let is_super_admin = state.is_super_admin();
    if !is_super_admin {
        state.assets_section = AssetsSection::Models3d;
        state.asset_form_mode = AssetFormMode::Inspect;
    }

    // Ensure selected asset is permitted for the active user (auto-select first allowed asset)
    let selection_valid = state
        .selected_asset()
        .is_some_and(|a| state.is_model_asset_allowed(a));
    if !selection_valid {
        let first_allowed = state
            .model_assets
            .iter()
            .find(|m| state.is_model_asset_allowed(m))
            .map(|m| m.id);
        if let Some(id) = first_allowed {
            state.select_asset(id);
        } else {
            state.selected_asset_id = None;
        }
    }

    let top = draw_assets_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);
    let bottom = draw_assets_bottom_bar(viewport_ui, state);
    let left = draw_assets_left_panel(viewport_ui, state, media_player);
    let right = draw_assets_right_panel(viewport_ui, state, stage, api);

    let screen_rect = ctx.viewport_rect();
    let center_rect = egui::Rect::from_min_max(
        egui::pos2(left, top),
        egui::pos2(
            (screen_rect.max.x - right).max(left + 64.0),
            (screen_rect.max.y - bottom).max(top + 64.0),
        ),
    );

    if let Some(media_type) = state.assets_section.media_type() {
        draw_media_center_preview(
            ctx,
            center_rect,
            state,
            media_player,
            sync_bridge,
            media_type,
        );
    } else if let Some(asset) = state.active_preview_asset() {
        let badge = if state.is_standard_user() {
            Some((
                "ASSETS SAYA (PREVIEW)",
                egui::Color32::from_rgb(12, 74, 110),
                egui::Color32::from_rgb(186, 230, 253),
            ))
        } else if !is_super_admin {
            Some((
                "MODE PREVIEW (READ-ONLY)",
                egui::Color32::from_rgb(51, 65, 85),
                egui::Color32::from_rgb(203, 213, 225),
            ))
        } else {
            None
        };
        draw_asset_preview_viewport_hud(ctx, center_rect, state, asset, badge);
    }

    AssetsLayoutInsets {
        left,
        right,
        top,
        bottom,
    }
}

fn draw_assets_top_bar(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) -> f32 {
    egui::Panel::top("assets_top_bar")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 23, 42))
                .inner_margin(egui::Margin::symmetric(12, 7)),
        )
        .show(viewport_ui, |ui| {
            let total_w = ui.available_width();
            let right_reserve = 148.0_f32.min((total_w * 0.32).max(96.0));
            let left_max_w = (total_w - right_reserve - 8.0).max(160.0);

            ui.horizontal_top(|ui| {
                ui.scope(|ui| {
                    ui.set_min_width(left_max_w);
                    ui.set_max_width(left_max_w);
                    ui.horizontal_wrapped(|ui| {
                        draw_app_brand_icon(ui, 18.0);
                        ui.label(
                            egui::RichText::new("TWG-RUST 3D")
                                .strong()
                                .color(egui::Color32::from_rgb(56, 189, 248)),
                        );
                        ui.separator();

                        draw_screen_nav_tabs(ui, state, playback, api, ws);

                        ui.separator();

                        if ui
                            .add_enabled(
                                !state.sync_status.is_syncing,
                                egui::Button::new("Sync Server"),
                            )
                            .clicked()
                        {
                            sync_bridge.send(AssetSyncCommand::SyncWithServer {
                                server_url: state.server_url.clone(),
                                local_root: state.assets_root.clone(),
                            });
                            if !state.offline_preview
                                && let Some(auth) = current_auth(state)
                            {
                                api.send(ApiCommand::RefreshCurrentUser { auth });
                            }
                        }

                        if state.sync_status.is_syncing {
                            let frac = if state.sync_status.total_files > 0 {
                                state.sync_status.completed_files as f32
                                    / state.sync_status.total_files as f32
                            } else {
                                0.0
                            };
                            ui.add(egui::ProgressBar::new(frac).desired_width(180.0).text(
                                if state.sync_status.checking {
                                    "Cek server...".to_string()
                                } else {
                                    format!(
                                        "Sync {}/{} ({:.1}/{:.1} MB)",
                                        state.sync_status.completed_files,
                                        state.sync_status.total_files,
                                        state.sync_status.downloaded_bytes as f64 / (1024.0 * 1024.0),
                                        state.sync_status.total_bytes as f64 / (1024.0 * 1024.0),
                                    )
                                },
                            ));
                        } else {
                            let col = if state.sync_status.offline_mode {
                                egui::Color32::from_rgb(250, 204, 21)
                            } else {
                                egui::Color32::from_rgb(74, 222, 128)
                            };
                            ui.colored_label(col, &state.sync_status.summary_text);
                        }
                    });
                });

                ui.allocate_ui_with_layout(
                    egui::vec2(right_reserve, 24.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match crate::ui::nav_bar::draw_account_menu_button(ui, state, "assets_nav")
                    {
                        crate::ui::nav_bar::AccountMenuAction::OpenProfile => {
                            open_profile_screen(state, playback, ws);
                        }
                        crate::ui::nav_bar::AccountMenuAction::Logout => {
                            crate::ui::nav_bar::perform_logout(state, playback, ws);
                        }
                        crate::ui::nav_bar::AccountMenuAction::None => {}
                    },
                );
            });
        })
        .response
        .rect
        .height()
}

fn draw_assets_left_panel(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    media_player: &mut MediaPreviewPlayer,
) -> f32 {
    egui::Panel::left("assets_left_panel")
        .resizable(true)
        .default_size(325.0)
        .min_size(260.0)
        .max_size(460.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 39))
                .inner_margin(egui::Margin::same(10)),
        )
        .show(viewport_ui, |ui| {
            let is_super_admin = state.is_super_admin();
            let is_standard_user = state.is_standard_user();

            if is_super_admin {
                // Top asset type switcher: 3D Models | Texture | Sound | Video
                let prev_section = state.assets_section;
                let tex_cnt = state
                    .media_assets
                    .iter()
                    .filter(|m| {
                        m.operasi_id.is_none()
                            && m.media_type == twg_shared::models::MediaAssetType::Texture
                    })
                    .count();
                let snd_cnt = state
                    .media_assets
                    .iter()
                    .filter(|m| {
                        m.operasi_id.is_none()
                            && m.media_type == twg_shared::models::MediaAssetType::Sound
                    })
                    .count();
                let vid_cnt = state
                    .media_assets
                    .iter()
                    .filter(|m| {
                        m.operasi_id.is_none()
                            && m.media_type == twg_shared::models::MediaAssetType::Video
                    })
                    .count();
                let master_3d_cnt = state
                    .model_assets
                    .iter()
                    .filter(|m| m.operasi_id.is_none())
                    .count();

                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut state.assets_section,
                        AssetsSection::Models3d,
                        format!("3D ({master_3d_cnt})"),
                    );
                    ui.selectable_value(
                        &mut state.assets_section,
                        AssetsSection::Texture,
                        format!("Texture ({tex_cnt})"),
                    );
                    ui.selectable_value(
                        &mut state.assets_section,
                        AssetsSection::Sound,
                        format!("Sound ({snd_cnt})"),
                    );
                    ui.selectable_value(
                        &mut state.assets_section,
                        AssetsSection::Video,
                        format!("Video ({vid_cnt})"),
                    );
                });
                ui.separator();

                if state.assets_section != prev_section {
                    media_player.stop_all_playback();
                    state.ensure_media_selection_for_section();
                }

                if let Some(media_type) = state.assets_section.media_type() {
                    draw_media_left_panel_body(ui, state, media_type, media_player);
                    return;
                }
            }

            ui.horizontal(|ui| {
                if is_super_admin {
                    ui.heading("3D Model Assets");
                } else if is_standard_user {
                    ui.heading("Assets 3D Saya");
                } else {
                    ui.heading("Katalog Assets 3D");
                }
                if is_super_admin {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let add_btn = egui::Button::new(
                            egui::RichText::new("+ Model")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144));
                        if ui.add(add_btn).clicked() {
                            let cat = if state.asset_category_filter == "all" {
                                state
                                    .model_categories
                                    .first()
                                    .map(|c| c.category.clone())
                                    .unwrap_or_else(|| "units".to_string())
                            } else {
                                state.asset_category_filter.clone()
                            };
                            let supports_anim = state.category_supports_idle_walk(&cat);
                            state.asset_form_mode = AssetFormMode::Create;
                            state.asset_form.reset_for_create(&cat, supports_anim);
                        }

                        let cat_active = state.asset_form_mode == AssetFormMode::Categories;
                        let cat_btn = egui::Button::new(
                            egui::RichText::new("+ Kategori").color(if cat_active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(125, 211, 252)
                            }),
                        )
                        .fill(if cat_active {
                            egui::Color32::from_rgb(30, 64, 175)
                        } else {
                            egui::Color32::from_rgb(30, 41, 59)
                        });
                        if ui.add(cat_btn).clicked() {
                            state.asset_form_mode = AssetFormMode::Categories;
                            state.category_form.error = None;
                            state.category_form.success = None;
                            let next_sort = state
                                .model_categories
                                .iter()
                                .map(|c| c.sort_order)
                                .max()
                                .unwrap_or(70)
                                + 10;
                            state.category_form.new_sort_order = next_sort;
                        }
                    });
                }
            });

            if !is_super_admin {
                ui.label(
                    egui::RichText::new(if is_standard_user {
                        "Daftar model 3D yang diberikan ke akun Anda. Klik item untuk melihat preview 3D."
                    } else {
                        "Mode Preview 3D (Read-Only). Klik item untuk melihat preview 3D."
                    })
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }

            ui.add_space(6.0);
            let extra_filter_active =
                state.asset_category_filter != "all" || state.asset_type_filter != "all";
            if draw_search_input_with_reset(
                ui,
                &mut state.asset_search,
                "Cari nama / slug model 3D...",
                extra_filter_active,
                "Reset pencarian, folder kategori, dan tipe model 3D",
            ) {
                state.asset_category_filter = "all".to_string();
                state.asset_type_filter = "all".to_string();
            }
            ui.add_space(6.0);

            let allowed_models: Vec<ModelAssetDto> = state
                .model_assets
                .iter()
                .filter(|m| state.is_model_asset_allowed(m))
                .cloned()
                .collect();

            // Dynamic category filter pills (loaded from server DB / offline manifest)
            ui.label(
                egui::RichText::new("Kategori Folder:")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.horizontal_wrapped(|ui| {
                let total_all = allowed_models.len();
                let active_all = state.asset_category_filter == "all";
                if ui
                    .selectable_label(active_all, format!("all ({total_all})"))
                    .clicked()
                {
                    state.asset_category_filter = "all".to_string();
                }

                let categories_snapshot: Vec<ModelCategoryCount> = state.model_categories.clone();
                for cat in &categories_snapshot {
                    let cnt = allowed_models
                        .iter()
                        .filter(|m| m.category == cat.category)
                        .count();
                    if is_standard_user && cnt == 0 && state.asset_category_filter != cat.category {
                        continue;
                    }
                    let active = state.asset_category_filter == cat.category;
                    if ui
                        .selectable_label(active, format!("{} ({cnt})", cat.category))
                        .on_hover_text(format!(
                            "{} • default: {} • scale: {:.2}",
                            cat.display_name(),
                            if cat.supports_idle_walk {
                                "Animasi (Idle+Walk)"
                            } else {
                                "Statis"
                            },
                            cat.default_scale
                        ))
                        .clicked()
                    {
                        state.asset_category_filter = cat.category.clone();
                    }
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Tipe:")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                for (val, lbl) in [
                    ("all", "Semua"),
                    ("static", "Statis"),
                    ("animated", "Animasi"),
                ] {
                    let active = state.asset_type_filter == val;
                    if ui.selectable_label(active, lbl).clicked() {
                        state.asset_type_filter = val.to_string();
                    }
                }
            });

            ui.separator();

            let search = state.asset_search.trim().to_lowercase();
            let cat_filter = state.asset_category_filter.clone();
            let type_filter = state.asset_type_filter.clone();

            let filtered: Vec<ModelAssetDto> = allowed_models
                .iter()
                .filter(|m| {
                    if cat_filter != "all" && m.category != cat_filter {
                        return false;
                    }
                    if type_filter != "all" && m.model_type.as_str() != type_filter {
                        return false;
                    }
                    if !search.is_empty()
                        && !m.name.to_lowercase().contains(&search)
                        && !m.slug.to_lowercase().contains(&search)
                    {
                        return false;
                    }
                    true
                })
                .cloned()
                .collect();

            ui.label(
                egui::RichText::new(format!("Menampilkan {} model", filtered.len()))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            if allowed_models.is_empty() && is_standard_user {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(24, 33, 50))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Belum ada Assets 3D")
                                .size(13.0)
                                .strong()
                                .color(egui::Color32::from_rgb(250, 204, 21)),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "Belum ada Assets 3D yang diberikan ke akun Anda. Hubungi Admin atau Admin Satker untuk mendapatkan akses model 3D.",
                            )
                            .size(11.5)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                        );
                    });
                return;
            }

            let mut clicked_id = None;
            egui::ScrollArea::vertical()
                .id_salt("admin_assets_list")
                .show(ui, |ui| {
                    for asset in &filtered {
                        let is_selected = state.selected_asset_id == Some(asset.id);
                        let primary_ready = state
                            .available_model_paths
                            .contains(&asset.model_file.rel_path)
                            || state.assets_root.join(&asset.model_file.rel_path).exists();
                        let type_tag = match asset.model_type {
                            ModelAssetType::Static => "STATIS",
                            ModelAssetType::Animated => "ANIM",
                        };
                        let sync_tag = if primary_ready { "OK" } else { "DL" };

                        let row_bg = if is_selected {
                            egui::Color32::from_rgb(14, 116, 144)
                        } else {
                            egui::Color32::from_rgb(24, 33, 50)
                        };

                        let resp = egui::Frame::new()
                            .fill(row_bg)
                            .corner_radius(5.0)
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let badge_col = match asset.model_type {
                                        ModelAssetType::Static => {
                                            egui::Color32::from_rgb(56, 189, 248)
                                        }
                                        ModelAssetType::Animated => {
                                            egui::Color32::from_rgb(192, 132, 252)
                                        }
                                    };
                                    ui.colored_label(
                                        badge_col,
                                        egui::RichText::new(format!("[{type_tag}]"))
                                            .size(10.0)
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(&asset.name)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let sync_col = if primary_ready {
                                                egui::Color32::from_rgb(74, 222, 128)
                                            } else {
                                                egui::Color32::from_rgb(250, 204, 21)
                                            };
                                            ui.colored_label(
                                                sync_col,
                                                egui::RichText::new(sync_tag).size(10.0).strong(),
                                            );
                                        },
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} • v{} • LOD0 {}",
                                            asset.category,
                                            asset.version,
                                            format_bytes(asset.model_file.size_bytes),
                                        ))
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    if let Some(l2) = &asset.lod2_file {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "| LOD2 {}",
                                                format_bytes(l2.size_bytes)
                                            ))
                                            .size(10.5)
                                            .color(egui::Color32::from_rgb(125, 211, 252)),
                                        );
                                    }
                                });
                            })
                            .response
                            .interact(egui::Sense::click());

                        if resp.clicked() {
                            clicked_id = Some(asset.id);
                        }
                        ui.add_space(4.0);
                    }
                });

            if let Some(id) = clicked_id {
                state.select_asset(id);
            }
        })
        .response
        .rect
        .width()
}

fn draw_assets_right_panel(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    stage: &mut AssetPreviewStage,
    api: &ApiClientBridge,
) -> f32 {
    egui::Panel::right("assets_right_panel")
        .resizable(true)
        .default_size(365.0)
        .min_size(300.0)
        .max_size(500.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 39))
                .inner_margin(egui::Margin::same(12)),
        )
        .show(viewport_ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("admin_assets_right_scroll")
                .show(ui, |ui| {
                    if let Some(media_type) = state.assets_section.media_type() {
                        draw_media_right_panel_body(ui, state, api, media_type);
                        return;
                    }
                    match state.asset_form_mode {
                        AssetFormMode::Create => draw_create_form(ui, state, api),
                        AssetFormMode::Inspect => draw_inspect_and_edit(ui, state, stage, api),
                        AssetFormMode::Categories => draw_categories_manager(ui, state, api),
                    }
                });
        })
        .response
        .rect
        .width()
}

fn draw_create_form(ui: &mut egui::Ui, state: &mut AppState, api: &ApiClientBridge) {
    ui.horizontal(|ui| {
        ui.heading("Tambah Model 3D Baru");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Batal").clicked() {
                state.asset_form_mode = AssetFormMode::Inspect;
                if let Some(sel) = state.selected_asset().cloned() {
                    state.asset_form.load_from_asset(&sel);
                }
            }
        });
    });
    ui.separator();

    ui.label("Nama Model:");
    ui.add(
        egui::TextEdit::singleline(&mut state.asset_form.name)
            .hint_text("Contoh: Brimob Taktis Baru")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);

    ui.label("Kategori Folder:");
    let prev_cat = state.asset_form.category.clone();
    let cats: Vec<ModelCategoryCount> = state.model_categories.clone();
    egui::ComboBox::from_id_salt("create_asset_cat")
        .selected_text(&state.asset_form.category)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for cat in &cats {
                let lbl = if cat.display_name() == cat.category {
                    cat.category.clone()
                } else {
                    format!("{} ({})", cat.category, cat.display_name())
                };
                ui.selectable_value(&mut state.asset_form.category, cat.category.clone(), lbl);
            }
        });
    if state.asset_form.category != prev_cat {
        // Suggest appropriate default type for the selected folder category
        if state.category_supports_idle_walk(&state.asset_form.category) {
            state.asset_form.model_type = ModelAssetType::Animated;
        } else {
            state.asset_form.model_type = ModelAssetType::Static;
        }
    }
    ui.add_space(6.0);

    ui.label("Tipe Model 3D:");
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut state.asset_form.model_type,
            ModelAssetType::Static,
            "Statis (1 file)",
        );
        ui.radio_value(
            &mut state.asset_form.model_type,
            ModelAssetType::Animated,
            "Animasi (2 file: Idle + Walk)",
        );
    });
    ui.add_space(6.0);

    draw_transform_defaults_inputs(ui, state);
    ui.separator();

    match state.asset_form.model_type {
        ModelAssetType::Static => {
            ui.label(
                egui::RichText::new("File Model Statis (.glb / .gltf):")
                    .strong()
                    .color(egui::Color32::from_rgb(56, 189, 248)),
            );
            draw_file_picker_row(
                ui,
                "Pilih File Model (.glb/.gltf)",
                &mut state.asset_form.model_file_path,
                None,
            );
        }
        ModelAssetType::Animated => {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(30, 41, 59))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Khusus model Animasi wajib 2 file:\n\
                             1. *_idle.glb / *_idle.gltf (WAJIB tanpa animasi sama sekali)\n\
                             2. *_walk.glb / *_walk.gltf (WAJIB memiliki animasi jalan)",
                        )
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                });
            ui.add_space(6.0);

            ui.label(
                egui::RichText::new("1. File Idle (*_idle.glb / *_idle.gltf — Tanpa Animasi):")
                    .strong()
                    .color(egui::Color32::from_rgb(56, 189, 248)),
            );
            draw_file_picker_row(
                ui,
                "Pilih File Idle...",
                &mut state.asset_form.idle_file_path,
                Some("_idle"),
            );
            ui.add_space(6.0);

            ui.label(
                egui::RichText::new("2. File Walk (*_walk.glb / *_walk.gltf — Animasi Jalan):")
                    .strong()
                    .color(egui::Color32::from_rgb(192, 132, 252)),
            );
            draw_file_picker_row(
                ui,
                "Pilih File Walk...",
                &mut state.asset_form.walk_file_path,
                Some("_walk"),
            );
        }
    }

    ui.add_space(8.0);
    if let Some(err) = &state.asset_form.error {
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
        ui.add_space(4.0);
    }
    if let Some(ok) = &state.asset_form.success {
        ui.colored_label(egui::Color32::from_rgb(74, 222, 128), ok);
        ui.add_space(4.0);
    }

    let submit_text = if state.asset_form.submitting {
        "Mengunggah & Konversi LOD0 + LOD2..."
    } else {
        "Upload & Simpan ke Server"
    };
    let submit_btn = egui::Button::new(egui::RichText::new(submit_text).strong())
        .fill(egui::Color32::from_rgb(14, 116, 144));

    if ui
        .add_enabled(!state.asset_form.submitting, submit_btn)
        .clicked()
    {
        if state.asset_form.name.trim().is_empty() {
            state.asset_form.error = Some("Nama model wajib diisi.".into());
            return;
        }
        match state.asset_form.model_type {
            ModelAssetType::Static => {
                if state.asset_form.model_file_path.is_none() {
                    state.asset_form.error =
                        Some("Pilih file model 3D (.glb/.gltf) terlebih dahulu.".into());
                    return;
                }
            }
            ModelAssetType::Animated => {
                if state.asset_form.idle_file_path.is_none()
                    || state.asset_form.walk_file_path.is_none()
                {
                    state.asset_form.error = Some(
                        "Model Animasi wajib memilih KEDUA file: file Idle (*_idle) dan file Walk (*_walk)."
                            .into(),
                    );
                    return;
                }
            }
        }

        if let Some(auth) = current_auth(state) {
            state.asset_form.submitting = true;
            state.asset_form.error = None;
            state.asset_form.success = None;
            api.send(ApiCommand::CreateModelAsset {
                auth,
                payload: ModelAssetUploadPayload {
                    name: state.asset_form.name.trim().to_string(),
                    category: state.asset_form.category.clone(),
                    operasi_id: None,
                    model_type: state.asset_form.model_type,
                    default_height: state.asset_form.default_height,
                    default_rot_x: state.asset_form.default_rot_x,
                    default_rot_y: state.asset_form.default_rot_y,
                    default_rot_z: state.asset_form.default_rot_z,
                    model_file_path: if state.asset_form.model_type == ModelAssetType::Static {
                        state.asset_form.model_file_path.clone()
                    } else {
                        None
                    },
                    idle_file_path: if state.asset_form.model_type == ModelAssetType::Animated {
                        state.asset_form.idle_file_path.clone()
                    } else {
                        None
                    },
                    walk_file_path: if state.asset_form.model_type == ModelAssetType::Animated {
                        state.asset_form.walk_file_path.clone()
                    } else {
                        None
                    },
                },
            });
        } else {
            state.asset_form.error =
                Some("Harus login sebagai Admin untuk mengunggah model.".into());
        }
    }
}

pub(crate) fn draw_preview_controls_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    stage: &mut AssetPreviewStage,
    asset: &ModelAssetDto,
) {
    ui.heading("Preview 3D glTF");
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(24, 33, 50))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            // LOD Switcher
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Detail LOD:").strong());
                if ui
                    .selectable_label(state.preview_lod == AssetPreviewLod::Lod0, "LOD0 (Full)")
                    .clicked()
                {
                    state.preview_lod = AssetPreviewLod::Lod0;
                }
                let has_lod2 = asset.lod2_file.is_some();
                if ui
                    .add_enabled(
                        has_lod2,
                        egui::Button::selectable(
                            state.preview_lod == AssetPreviewLod::Lod2,
                            "LOD2 (Ringan)",
                        ),
                    )
                    .clicked()
                {
                    state.preview_lod = AssetPreviewLod::Lod2;
                }
            });

            // Idle vs Walk vs Extra Action variant switcher (Animated models)
            if asset.model_type == ModelAssetType::Animated {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Varian:").strong());
                    if ui
                        .selectable_label(
                            state.preview_variant == AssetPreviewVariant::Idle,
                            "Idle",
                        )
                        .clicked()
                    {
                        state.preview_variant = AssetPreviewVariant::Idle;
                    }
                    if ui
                        .selectable_label(
                            state.preview_variant == AssetPreviewVariant::Walk,
                            "Walk",
                        )
                        .clicked()
                    {
                        state.preview_variant = AssetPreviewVariant::Walk;
                    }
                    for act in &asset.extra_animations {
                        let is_sel = matches!(
                            &state.preview_variant,
                            AssetPreviewVariant::Action(s) if s == &act.slug
                        );
                        if ui.selectable_label(is_sel, &act.name).clicked() {
                            state.preview_variant = AssetPreviewVariant::Action(act.slug.clone());
                        }
                    }
                });

                if !matches!(state.preview_variant, AssetPreviewVariant::Idle) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let play_lbl = if state.preview_anim_playing {
                            "Pause Anim"
                        } else {
                            "Play Anim"
                        };
                        if ui.button(play_lbl).clicked() {
                            state.preview_anim_playing = !state.preview_anim_playing;
                        }
                        ui.label("Speed:");
                        ui.add(
                            egui::Slider::new(&mut state.preview_anim_speed, 0.25..=2.5)
                                .fixed_decimals(2)
                                .suffix("x"),
                        );
                    });
                }
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.preview_auto_rotate, "Auto-Rotate");
                if ui.button("Reset Kamera (R)").clicked() {
                    stage.reset_camera_requested = true;
                }
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Kontrol: Drag Kiri = Orbit • Scroll = Zoom • Drag Kanan = Naik/Turun",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
}

pub(crate) fn draw_asset_file_stats_card(ui: &mut egui::Ui, asset: &ModelAssetDto) {
    ui.label(egui::RichText::new("Informasi File & LOD2").strong());
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(20, 29, 44))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("Slug: {} | Versi: v{}", asset.slug, asset.version))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
            );
            let lod0_label = if asset.model_type == ModelAssetType::Animated {
                "Idle LOD0"
            } else {
                "Model LOD0"
            };
            ui.label(
                egui::RichText::new(format!(
                    "{lod0_label}: {} ({} tris) • sha256:{}",
                    format_bytes(asset.model_file.size_bytes),
                    asset.model_file.tris,
                    &asset.model_file.sha256[..asset.model_file.sha256.len().min(10)],
                ))
                .size(11.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            if let Some(l2) = &asset.lod2_file {
                let pct = if asset.model_file.size_bytes > 0 {
                    100 - ((l2.size_bytes * 100) / asset.model_file.size_bytes)
                } else {
                    0
                };
                ui.label(
                    egui::RichText::new(format!(
                        "LOD2: {} ({} tris, -{pct}% size)",
                        format_bytes(l2.size_bytes),
                        l2.tris,
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
                );
            }
            if let Some(w) = &asset.walk_file {
                ui.label(
                    egui::RichText::new(format!(
                        "Walk LOD0: {} ({} tris)",
                        format_bytes(w.size_bytes),
                        w.tris
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(192, 132, 252)),
                );
            }
            if let Some(wl2) = &asset.walk_lod2_file {
                ui.label(
                    egui::RichText::new(format!(
                        "Walk LOD2: {} ({} tris)",
                        format_bytes(wl2.size_bytes),
                        wl2.tris
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(216, 180, 254)),
                );
            }
        });
}

fn draw_inspect_and_edit(
    ui: &mut egui::Ui,
    state: &mut AppState,
    stage: &mut AssetPreviewStage,
    api: &ApiClientBridge,
) {
    let is_super_admin = state.is_super_admin();
    let is_standard_user = state.is_standard_user();

    let Some(asset) = state.active_preview_asset().cloned() else {
        ui.heading("Preview & Detail Model 3D");
        ui.separator();
        let empty_msg = if is_super_admin {
            "Pilih salah satu model 3D di daftar kiri untuk melihat preview 3D glTF, atau klik '+ Tambah Model'."
        } else if is_standard_user {
            "Belum ada model 3D yang dipilih atau diberikan ke akun Anda."
        } else {
            "Pilih salah satu model 3D di daftar kiri untuk melihat preview 3D glTF."
        };
        ui.label(empty_msg);
        return;
    };

    // Ensure edit form tracks selected asset
    if is_super_admin && state.asset_form.editing_id != Some(asset.id) {
        state.asset_form.load_from_asset(&asset);
    }

    // --- Section 1: 3D Preview Controls ---
    draw_preview_controls_card(ui, state, stage, &asset);

    ui.add_space(8.0);

    // --- Section 2: File & Optimization Stats ---
    draw_asset_file_stats_card(ui, &asset);

    // --- Section 2B: Extra Action Animation Clips (Animated Models Only) ---
    if asset.model_type == ModelAssetType::Animated
        && (is_super_admin || !asset.extra_animations.is_empty())
    {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "Animasi Aksi Tambahan ({})",
                    asset.extra_animations.len()
                ))
                .strong()
                .color(egui::Color32::from_rgb(251, 191, 36)),
            );
        });
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(22, 31, 48))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                if is_super_admin {
                    ui.label(
                        egui::RichText::new(
                            "Upload file .glb animasi tambahan (contoh: Melambai, Menembak, Pingsan, Hormat) agar dapat dipicu melalui Animasi -> Lainnya -> Aksi Model 3D.",
                        )
                        .size(10.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(6.0);
                }

                if asset.extra_animations.is_empty() {
                    ui.label(
                        egui::RichText::new("Belum ada aksi tambahan untuk model ini.")
                            .size(11.0)
                            .italics()
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                } else {
                    for act in &asset.extra_animations {
                        let is_previewing = matches!(
                            &state.preview_variant,
                            AssetPreviewVariant::Action(s) if s == &act.slug
                        );
                        egui::Frame::new()
                            .fill(if is_previewing {
                                egui::Color32::from_rgb(30, 50, 74)
                            } else {
                                egui::Color32::from_rgb(15, 23, 38)
                            })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if is_previewing {
                                    egui::Color32::from_rgb(56, 189, 248)
                                } else {
                                    egui::Color32::from_rgb(41, 53, 72)
                                },
                            ))
                            .corner_radius(5.0)
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&act.name)
                                                .strong()
                                                .size(12.0)
                                                .color(egui::Color32::WHITE),
                                        );
                                        let clip_info = if act.clip_names.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" • clip: {}", act.clip_names.join(", "))
                                        };
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} • {} ({} tris){clip_info}",
                                                act.slug,
                                                format_bytes(act.file.size_bytes),
                                                act.file.tris
                                            ))
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                        );
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let confirming = is_super_admin
                                                && state
                                                    .asset_form
                                                    .confirm_delete_action_slug
                                                    .as_deref()
                                                    == Some(act.slug.as_str());
                                            if confirming {
                                                if ui
                                                    .add_enabled(
                                                        !state.asset_form.submitting,
                                                        egui::Button::new(
                                                            egui::RichText::new("Ya")
                                                                .size(10.5)
                                                                .color(egui::Color32::WHITE),
                                                        )
                                                        .fill(egui::Color32::from_rgb(185, 28, 28)),
                                                    )
                                                    .clicked()
                                                    && let Some(auth) = current_auth(state)
                                                {
                                                    state.asset_form.submitting = true;
                                                    state.asset_form.error = None;
                                                    state.asset_form.confirm_delete_action_slug =
                                                        None;
                                                    if is_previewing {
                                                        state.preview_variant =
                                                            AssetPreviewVariant::Idle;
                                                    }
                                                    api.send(ApiCommand::DeleteModelAction {
                                                        auth,
                                                        model_id: asset.id,
                                                        action_slug: act.slug.clone(),
                                                    });
                                                }
                                                if ui
                                                    .small_button("Batal")
                                                    .clicked()
                                                {
                                                    state.asset_form.confirm_delete_action_slug =
                                                        None;
                                                }
                                            } else {
                                                if is_super_admin
                                                    && ui
                                                        .add_enabled(
                                                            !state.asset_form.submitting,
                                                            egui::Button::new(
                                                                egui::RichText::new("Hapus")
                                                                    .size(10.5)
                                                                    .color(
                                                                        egui::Color32::from_rgb(
                                                                            252, 165, 165,
                                                                        ),
                                                                    ),
                                                            )
                                                            .fill(egui::Color32::from_rgb(
                                                                69, 20, 26,
                                                            )),
                                                        )
                                                        .clicked()
                                                {
                                                    state.asset_form.confirm_delete_action_slug =
                                                        Some(act.slug.clone());
                                                }
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new(if is_previewing {
                                                                "Aktif"
                                                            } else {
                                                                "Preview"
                                                            })
                                                            .size(10.5),
                                                        )
                                                        .fill(if is_previewing {
                                                            egui::Color32::from_rgb(14, 116, 144)
                                                        } else {
                                                            egui::Color32::from_rgb(30, 41, 59)
                                                        }),
                                                    )
                                                    .clicked()
                                                {
                                                    state.preview_variant =
                                                        AssetPreviewVariant::Action(
                                                            act.slug.clone(),
                                                        );
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                }

                if is_super_admin {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("+ Tambah Aksi Baru (.glb / .gltf):")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.asset_form.new_action_name)
                            .hint_text("Nama aksi (mis. Melambai, Menembak, Pingsan)")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    draw_file_picker_row(
                        ui,
                        "Pilih File Aksi (.glb/.gltf)...",
                        &mut state.asset_form.new_action_file_path,
                        None,
                    );
                    ui.add_space(6.0);
                    let up_lbl = if state.asset_form.submitting {
                        "Mengunggah & Konversi Aksi..."
                    } else {
                        "+ Upload Aksi ke Model"
                    };
                    if ui
                        .add_enabled(
                            !state.asset_form.submitting,
                            egui::Button::new(
                                egui::RichText::new(up_lbl)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(21, 128, 61)),
                        )
                        .clicked()
                    {
                        let trimmed = state.asset_form.new_action_name.trim().to_string();
                        if trimmed.is_empty() {
                            state.asset_form.error =
                                Some("Nama aksi wajib diisi (contoh: Melambai).".into());
                        } else if let Some(path) = state.asset_form.new_action_file_path.clone() {
                            if let Some(auth) = current_auth(state) {
                                state.asset_form.submitting = true;
                                state.asset_form.error = None;
                                state.asset_form.success = None;
                                api.send(ApiCommand::UploadModelAction {
                                    auth,
                                    model_id: asset.id,
                                    name: trimmed,
                                    file_path: path,
                                });
                            } else {
                                state.asset_form.error = Some(
                                    "Harus login sebagai Admin untuk mengunggah aksi animasi."
                                        .into(),
                                );
                            }
                        } else {
                            state.asset_form.error =
                                Some("Pilih file .glb/.gltf animasi aksi terlebih dahulu.".into());
                        }
                    }
                }
            });
    }

    if !is_super_admin {
        ui.add_space(10.0);
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(15, 30, 48))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 65, 96)))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let note = if is_standard_user {
                    "Model 3D ini sudah diizinkan untuk akun Anda dan siap digunakan di Plot Skenario -> Tab Catalog."
                } else {
                    "Mode Preview (Read-Only). Untuk memberikan atau mengubah akses Assets 3D milik user, buka menu Users -> Assets."
                };
                ui.label(
                    egui::RichText::new(note)
                        .size(11.0)
                        .color(egui::Color32::from_rgb(186, 230, 253)),
                );
            });
        return;
    }

    ui.add_space(8.0);
    ui.separator();

    // --- Section 3: Admin Edit Form ---
    ui.heading("Edit Model Asset (Admin)");

    ui.label("Nama Model:");
    ui.add(egui::TextEdit::singleline(&mut state.asset_form.name).desired_width(f32::INFINITY));
    ui.add_space(4.0);

    ui.label("Kategori Folder:");
    let edit_cats: Vec<ModelCategoryCount> = state.model_categories.clone();
    egui::ComboBox::from_id_salt("edit_asset_cat")
        .selected_text(&state.asset_form.category)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for cat in &edit_cats {
                let lbl = if cat.display_name() == cat.category {
                    cat.category.clone()
                } else {
                    format!("{} ({})", cat.category, cat.display_name())
                };
                ui.selectable_value(&mut state.asset_form.category, cat.category.clone(), lbl);
            }
        });
    ui.add_space(4.0);

    ui.label("Tipe Model:");
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut state.asset_form.model_type,
            ModelAssetType::Static,
            "Statis",
        );
        ui.radio_value(
            &mut state.asset_form.model_type,
            ModelAssetType::Animated,
            "Animasi (Idle + Walk)",
        );
    });
    ui.add_space(4.0);

    draw_transform_defaults_inputs(ui, state);
    ui.add_space(6.0);

    ui.label(
        egui::RichText::new("Ganti File Model 3D (opsional saat edit):")
            .size(11.5)
            .strong()
            .color(egui::Color32::from_rgb(203, 213, 225)),
    );
    match state.asset_form.model_type {
        ModelAssetType::Static => {
            draw_file_picker_row(
                ui,
                "Ganti File Statis (.glb/.gltf)...",
                &mut state.asset_form.model_file_path,
                None,
            );
        }
        ModelAssetType::Animated => {
            if asset.model_type == ModelAssetType::Static {
                ui.colored_label(
                    egui::Color32::from_rgb(250, 204, 21),
                    "Mengubah Statis -> Animasi wajib pilih file Idle & Walk baru.",
                );
            }
            ui.label(egui::RichText::new("Idle (*_idle — tanpa animasi):").size(11.0));
            draw_file_picker_row(
                ui,
                "Ganti File Idle...",
                &mut state.asset_form.idle_file_path,
                Some("_idle"),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Walk (*_walk — animasi jalan):").size(11.0));
            draw_file_picker_row(
                ui,
                "Ganti File Walk...",
                &mut state.asset_form.walk_file_path,
                Some("_walk"),
            );
        }
    }

    ui.add_space(8.0);
    if let Some(err) = &state.asset_form.error {
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
        ui.add_space(4.0);
    }
    if let Some(ok) = &state.asset_form.success {
        ui.colored_label(egui::Color32::from_rgb(74, 222, 128), ok);
        ui.add_space(4.0);
    }

    ui.horizontal(|ui| {
        let save_lbl = if state.asset_form.submitting {
            "Menyimpan..."
        } else {
            "Simpan Perubahan"
        };
        let save_btn = egui::Button::new(egui::RichText::new(save_lbl).strong())
            .fill(egui::Color32::from_rgb(14, 116, 144));
        if ui
            .add_enabled(!state.asset_form.submitting, save_btn)
            .clicked()
        {
            if state.asset_form.name.trim().is_empty() {
                state.asset_form.error = Some("Nama model tidak boleh kosong.".into());
                return;
            }
            if asset.model_type == ModelAssetType::Static
                && state.asset_form.model_type == ModelAssetType::Animated
                && (state.asset_form.idle_file_path.is_none()
                    || state.asset_form.walk_file_path.is_none())
            {
                state.asset_form.error =
                    Some("Mengubah tipe ke Animasi wajib memilih file Idle dan Walk.".into());
                return;
            }
            if let Some(auth) = current_auth(state) {
                state.asset_form.submitting = true;
                state.asset_form.error = None;
                state.asset_form.success = None;
                api.send(ApiCommand::UpdateModelAsset {
                    auth,
                    id: asset.id,
                    payload: ModelAssetUploadPayload {
                        name: state.asset_form.name.trim().to_string(),
                        category: state.asset_form.category.clone(),
                        operasi_id: asset.operasi_id,
                        model_type: state.asset_form.model_type,
                        default_height: state.asset_form.default_height,
                        default_rot_x: state.asset_form.default_rot_x,
                        default_rot_y: state.asset_form.default_rot_y,
                        default_rot_z: state.asset_form.default_rot_z,
                        model_file_path: if state.asset_form.model_type == ModelAssetType::Static {
                            state.asset_form.model_file_path.clone()
                        } else {
                            None
                        },
                        idle_file_path: if state.asset_form.model_type == ModelAssetType::Animated {
                            state.asset_form.idle_file_path.clone()
                        } else {
                            None
                        },
                        walk_file_path: if state.asset_form.model_type == ModelAssetType::Animated {
                            state.asset_form.walk_file_path.clone()
                        } else {
                            None
                        },
                    },
                });
            }
        }
    });

    ui.add_space(12.0);
    ui.separator();

    // --- Section 4: Delete Model (2-step confirmation) ---
    if state.asset_form.confirm_delete_id == Some(asset.id) {
        ui.colored_label(
            egui::Color32::from_rgb(248, 113, 113),
            format!("Hapus permanen model '{}' dari server?", asset.name),
        );
        ui.horizontal(|ui| {
            let del_confirm = egui::Button::new(
                egui::RichText::new("Ya, Hapus Permanen")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(185, 28, 28));
            if ui
                .add_enabled(!state.asset_form.submitting, del_confirm)
                .clicked()
                && let Some(auth) = current_auth(state)
            {
                state.asset_form.submitting = true;
                state.asset_form.error = None;
                api.send(ApiCommand::DeleteModelAsset { auth, id: asset.id });
            }
            if ui.button("Batal").clicked() {
                state.asset_form.confirm_delete_id = None;
            }
        });
    } else {
        let del_btn = egui::Button::new(
            egui::RichText::new("Hapus Model Asset").color(egui::Color32::from_rgb(252, 165, 165)),
        )
        .fill(egui::Color32::from_rgb(69, 20, 26));
        if ui
            .add_enabled(!state.asset_form.submitting, del_btn)
            .clicked()
        {
            state.asset_form.confirm_delete_id = Some(asset.id);
        }
    }
}

fn slugify_category_input(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_sep = false;
        } else if (ch == '_' || ch == '-' || ch == ' ') && !out.is_empty() && !prev_sep {
            out.push(if ch == '-' { '-' } else { '_' });
            prev_sep = true;
        }
    }
    out.trim_matches(|c| c == '_' || c == '-').to_string()
}

fn draw_categories_manager(ui: &mut egui::Ui, state: &mut AppState, api: &ApiClientBridge) {
    ui.horizontal(|ui| {
        ui.heading("Kelola Kategori Model 3D");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Tutup").clicked() {
                state.asset_form_mode = AssetFormMode::Inspect;
            }
        });
    });
    ui.label(
        egui::RichText::new(
            "Kategori baru tersimpan di database server & otomatis tersinkron ke semua PC klien tanpa rebuild.",
        )
        .size(11.0)
        .color(egui::Color32::from_rgb(148, 163, 184)),
    );
    ui.separator();

    // --- Create New Category Form ---
    ui.label(
        egui::RichText::new("Tambah Kategori Baru")
            .strong()
            .color(egui::Color32::from_rgb(56, 189, 248)),
    );
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(24, 33, 50))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.label("Nama Kategori:");
            let name_resp = ui.add(
                egui::TextEdit::singleline(&mut state.category_form.new_name)
                    .hint_text("Contoh: Drone & UAV")
                    .desired_width(f32::INFINITY),
            );
            if name_resp.changed() && !state.category_form.slug_manually_edited {
                state.category_form.new_slug =
                    slugify_category_input(&state.category_form.new_name);
            }
            ui.add_space(4.0);

            ui.label("Slug Folder (huruf kecil, angka, '_', '-'):");
            let slug_resp = ui.add(
                egui::TextEdit::singleline(&mut state.category_form.new_slug)
                    .hint_text("Contoh: drone_uav")
                    .desired_width(f32::INFINITY),
            );
            if slug_resp.changed() {
                state.category_form.slug_manually_edited =
                    !state.category_form.new_slug.trim().is_empty();
            }
            ui.add_space(4.0);

            ui.checkbox(
                &mut state.category_form.new_supports_idle_walk,
                "Default tipe Animasi (Idle + Walk)",
            );

            ui.columns(2, |cols| {
                cols[0].label(
                    egui::RichText::new("Default Scale (min 0.01)")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                numeric_spinner(
                    &mut cols[0],
                    "cat_new_default_scale",
                    &mut state.category_form.new_default_scale,
                    SpinnerConfig {
                        min: MIN_OBJECT_3D_SCALE,
                        max: MAX_OBJECT_3D_SCALE,
                        step: 0.01,
                        drag_speed: 0.01,
                        decimals: 2,
                        suffix: "",
                        wrap_around: false,
                    },
                );
                cols[1].label(
                    egui::RichText::new("Urutan")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                numeric_spinner_i32(
                    &mut cols[1],
                    "cat_new_sort_order",
                    &mut state.category_form.new_sort_order,
                    0,
                    9999,
                    1,
                );
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Perilaku di Play Skenario:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            egui::ComboBox::from_id_salt("cat_new_play_behavior")
                .selected_text(state.category_form.new_play_behavior.label())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for &beh in CategoryPlayBehavior::ALL {
                        ui.selectable_value(
                            &mut state.category_form.new_play_behavior,
                            beh,
                            beh.label(),
                        );
                    }
                });

            ui.add_space(6.0);
            let add_lbl = if state.category_form.submitting {
                "Menyimpan..."
            } else {
                "+ Buat Kategori"
            };
            let add_btn = egui::Button::new(egui::RichText::new(add_lbl).strong())
                .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui
                .add_enabled(!state.category_form.submitting, add_btn)
                .clicked()
            {
                let name = state.category_form.new_name.trim().to_string();
                let slug =
                    slugify_category_input(if state.category_form.new_slug.trim().is_empty() {
                        &name
                    } else {
                        &state.category_form.new_slug
                    });
                if name.is_empty() {
                    state.category_form.error = Some("Nama kategori wajib diisi.".into());
                } else if slug.is_empty() || slug == "all" {
                    state.category_form.error =
                        Some("Slug folder tidak valid (tidak boleh kosong atau 'all').".into());
                } else if let Some(auth) = current_auth(state) {
                    state.category_form.submitting = true;
                    state.category_form.error = None;
                    state.category_form.success = None;
                    api.send(ApiCommand::CreateModelCategory {
                        auth,
                        req: CreateModelCategoryRequest {
                            slug,
                            name,
                            supports_idle_walk: state.category_form.new_supports_idle_walk,
                            default_scale: state.category_form.new_default_scale,
                            sort_order: state.category_form.new_sort_order,
                            play_behavior: state.category_form.new_play_behavior,
                        },
                    });
                }
            }
        });

    if let Some(err) = &state.category_form.error {
        ui.add_space(6.0);
        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
    }
    if let Some(ok) = &state.category_form.success {
        ui.add_space(6.0);
        ui.colored_label(egui::Color32::from_rgb(74, 222, 128), ok);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.label(
        egui::RichText::new(format!(
            "Daftar Kategori Aktif ({})",
            state.model_categories.len()
        ))
        .strong(),
    );
    ui.add_space(4.0);

    let cats: Vec<ModelCategoryCount> = state.model_categories.clone();
    for cat in &cats {
        let model_count = state
            .model_assets
            .iter()
            .filter(|m| m.category == cat.category)
            .count();
        let is_editing = state.category_form.editing_slug.as_deref() == Some(cat.category.as_str());

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(20, 29, 44))
            .corner_radius(5.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&cat.category)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new(format!("({model_count} model)"))
                            .size(11.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                    if cat.supports_idle_walk {
                        ui.colored_label(
                            egui::Color32::from_rgb(192, 132, 252),
                            egui::RichText::new("[Idle+Walk]").size(10.0).strong(),
                        );
                    }
                    if cat.is_system {
                        ui.colored_label(
                            egui::Color32::from_rgb(148, 163, 184),
                            egui::RichText::new("[Bawaan]").size(9.5).strong(),
                        );
                    }
                    let beh_col = match cat.play_behavior {
                        CategoryPlayBehavior::StaticObstacle => {
                            egui::Color32::from_rgb(248, 113, 113)
                        }
                        CategoryPlayBehavior::GizmoMove => egui::Color32::from_rgb(251, 191, 36),
                        CategoryPlayBehavior::RtsUnit => egui::Color32::from_rgb(52, 211, 153),
                    };
                    ui.colored_label(
                        beh_col,
                        egui::RichText::new(format!("[{}]", cat.play_behavior.short_label()))
                            .size(9.5)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !is_editing {
                            if !cat.is_system
                                && ui
                                    .add_enabled(
                                        !state.category_form.submitting && model_count == 0,
                                        egui::Button::new("Hapus").small(),
                                    )
                                    .on_disabled_hover_text(
                                        "Kategori yang masih berisi model tidak bisa dihapus.",
                                    )
                                    .clicked()
                            {
                                state.category_form.confirm_delete_slug =
                                    Some(cat.category.clone());
                            }
                            if ui.small_button("Edit").clicked() {
                                state.category_form.editing_slug = Some(cat.category.clone());
                                state.category_form.edit_name = cat.display_name().to_string();
                                state.category_form.edit_supports_idle_walk =
                                    cat.supports_idle_walk;
                                state.category_form.edit_default_scale = cat.default_scale;
                                state.category_form.edit_sort_order = cat.sort_order;
                                state.category_form.edit_play_behavior = cat.play_behavior;
                                state.category_form.confirm_delete_slug = None;
                            }
                        }
                    });
                });

                ui.label(
                    egui::RichText::new(format!(
                        "Nama: {} • Scale: {:.2} • Urutan: {} • Play: {}",
                        cat.display_name(),
                        cat.default_scale,
                        cat.sort_order,
                        cat.play_behavior.short_label()
                    ))
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );

                if is_editing {
                    ui.separator();
                    ui.label("Nama Tampilan:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.category_form.edit_name)
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(
                        &mut state.category_form.edit_supports_idle_walk,
                        "Default Animasi (Idle + Walk)",
                    );
                    ui.label(
                        egui::RichText::new("Perilaku di Play Skenario:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    egui::ComboBox::from_id_salt(("cat_edit_play_behavior", &cat.category))
                        .selected_text(state.category_form.edit_play_behavior.label())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for &beh in CategoryPlayBehavior::ALL {
                                ui.selectable_value(
                                    &mut state.category_form.edit_play_behavior,
                                    beh,
                                    beh.label(),
                                );
                            }
                        });
                    ui.columns(2, |cols| {
                        cols[0].label(
                            egui::RichText::new("Scale (min 0.01)")
                                .size(10.5)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        numeric_spinner(
                            &mut cols[0],
                            ("cat_edit_scale", &cat.category),
                            &mut state.category_form.edit_default_scale,
                            SpinnerConfig {
                                min: MIN_OBJECT_3D_SCALE,
                                max: MAX_OBJECT_3D_SCALE,
                                step: 0.01,
                                drag_speed: 0.01,
                                decimals: 2,
                                suffix: "",
                                wrap_around: false,
                            },
                        );
                        cols[1].label(
                            egui::RichText::new("Urutan")
                                .size(10.5)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        numeric_spinner_i32(
                            &mut cols[1],
                            ("cat_edit_sort", &cat.category),
                            &mut state.category_form.edit_sort_order,
                            0,
                            9999,
                            1,
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !state.category_form.submitting,
                                egui::Button::new("Simpan").small(),
                            )
                            .clicked()
                            && let Some(auth) = current_auth(state)
                        {
                            state.category_form.submitting = true;
                            state.category_form.error = None;
                            api.send(ApiCommand::UpdateModelCategory {
                                auth,
                                slug: cat.category.clone(),
                                req: UpdateModelCategoryRequest {
                                    name: state.category_form.edit_name.trim().to_string(),
                                    supports_idle_walk: state.category_form.edit_supports_idle_walk,
                                    default_scale: state.category_form.edit_default_scale,
                                    sort_order: state.category_form.edit_sort_order,
                                    play_behavior: state.category_form.edit_play_behavior,
                                },
                            });
                        }
                        if ui.small_button("Batal").clicked() {
                            state.category_form.editing_slug = None;
                        }
                    });
                }

                if state.category_form.confirm_delete_slug.as_deref() == Some(cat.category.as_str())
                {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(248, 113, 113),
                        format!("Yakin hapus kategori '{}'?", cat.category),
                    );
                    ui.horizontal(|ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("Ya, Hapus").color(egui::Color32::WHITE),
                        )
                        .small()
                        .fill(egui::Color32::from_rgb(185, 28, 28));
                        if ui
                            .add_enabled(!state.category_form.submitting, btn)
                            .clicked()
                            && let Some(auth) = current_auth(state)
                        {
                            state.category_form.submitting = true;
                            state.category_form.error = None;
                            api.send(ApiCommand::DeleteModelCategory {
                                auth,
                                slug: cat.category.clone(),
                            });
                        }
                        if ui.small_button("Batal").clicked() {
                            state.category_form.confirm_delete_slug = None;
                        }
                    });
                }
            });
        ui.add_space(4.0);
    }
}

fn draw_transform_defaults_inputs(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.label("Default Height:");
        numeric_spinner(
            ui,
            "asset_default_height",
            &mut state.asset_form.default_height,
            SpinnerConfig {
                min: 0.5,
                max: 200.0,
                step: 0.5,
                drag_speed: 0.5,
                decimals: 1,
                suffix: " m",
                wrap_around: false,
            },
        );
    });
    ui.add_space(4.0);
    let rot_cfg = SpinnerConfig {
        min: -360.0,
        max: 360.0,
        step: 1.0,
        drag_speed: 1.0,
        decimals: 1,
        suffix: "°",
        wrap_around: false,
    };
    ui.columns(3, |cols| {
        cols[0].label(
            egui::RichText::new("Rot° X")
                .size(10.5)
                .color(egui::Color32::from_rgb(248, 113, 113)),
        );
        numeric_spinner(
            &mut cols[0],
            "asset_rot_x",
            &mut state.asset_form.default_rot_x,
            rot_cfg,
        );
        cols[1].label(
            egui::RichText::new("Rot° Y")
                .size(10.5)
                .color(egui::Color32::from_rgb(74, 222, 128)),
        );
        numeric_spinner(
            &mut cols[1],
            "asset_rot_y",
            &mut state.asset_form.default_rot_y,
            rot_cfg,
        );
        cols[2].label(
            egui::RichText::new("Rot° Z")
                .size(10.5)
                .color(egui::Color32::from_rgb(96, 165, 250)),
        );
        numeric_spinner(
            &mut cols[2],
            "asset_rot_z",
            &mut state.asset_form.default_rot_z,
            rot_cfg,
        );
    });
}

fn draw_file_picker_row(
    ui: &mut egui::Ui,
    button_label: &str,
    slot: &mut Option<PathBuf>,
    recommended_suffix: Option<&str>,
) {
    ui.horizontal(|ui| {
        if ui.button(button_label).clicked()
            && let Some(picked) = pick_gltf_file(button_label)
        {
            *slot = Some(picked);
        }
        if slot.is_some() && ui.small_button("Clear").clicked() {
            *slot = None;
        }
    });
    if let Some(p) = slot {
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("selected");
        ui.label(
            egui::RichText::new(format!("Dipilih: {fname}"))
                .size(11.0)
                .color(egui::Color32::from_rgb(74, 222, 128)),
        );
        if let Some(suffix) = recommended_suffix {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !stem.ends_with(suffix) {
                ui.label(
                    egui::RichText::new(format!(
                        "Info: nama file tidak berakhiran '{suffix}', server tetap akan memvalidasi animasi & menyimpan sebagai *{suffix}.glb."
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(250, 204, 21)),
                );
            }
        }
    }
}

pub(crate) fn draw_asset_preview_viewport_hud(
    ctx: &egui::Context,
    center_rect: egui::Rect,
    state: &AppState,
    asset: &ModelAssetDto,
    status_badge: Option<(&str, egui::Color32, egui::Color32)>,
) {
    let target_rel = resolve_preview_file_path(asset, &state.preview_variant, state.preview_lod);
    let ready = state.available_model_paths.contains(&target_rel)
        || state.assets_root.join(&target_rel).exists();

    let lod_tag = match state.preview_lod {
        AssetPreviewLod::Lod0 => "LOD0",
        AssetPreviewLod::Lod2 => "LOD2",
    };
    let var_tag: String = if asset.model_type == ModelAssetType::Animated {
        match &state.preview_variant {
            AssetPreviewVariant::Idle => " • IDLE (No Anim)".to_string(),
            AssetPreviewVariant::Walk => " • WALK (Animated)".to_string(),
            AssetPreviewVariant::Action(slug) => {
                let lbl = asset
                    .extra_animations
                    .iter()
                    .find(|a| a.slug == *slug)
                    .map(|a| a.name.as_str())
                    .unwrap_or(slug.as_str());
                format!(" • AKSI: {lbl}")
            }
        }
    } else {
        " • STATIS".to_string()
    };

    egui::Area::new(egui::Id::new("asset_preview_hud"))
        .fixed_pos(center_rect.left_top() + egui::vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 23, 42, 215))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&asset.name)
                                .size(15.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        if let Some((badge_txt, badge_bg, badge_fg)) = status_badge {
                            egui::Frame::new()
                                .fill(badge_bg)
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(7, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(badge_txt)
                                            .size(10.0)
                                            .strong()
                                            .color(badge_fg),
                                    );
                                });
                        }
                    });
                    ui.label(
                        egui::RichText::new(format!("{} • {lod_tag}{var_tag}", asset.category))
                            .size(11.5)
                            .color(egui::Color32::from_rgb(56, 189, 248)),
                    );
                    ui.label(
                        egui::RichText::new(&target_rel)
                            .size(10.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                    if !ready {
                        ui.add_space(4.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(250, 204, 21),
                            "Mengunduh file model dari server untuk preview...",
                        );
                    }
                });
        });
}

fn draw_assets_bottom_bar(viewport_ui: &mut egui::Ui, state: &AppState) -> f32 {
    egui::Panel::bottom("assets_bottom_bar")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 23, 42))
                .inner_margin(egui::Margin::symmetric(12, 6)),
        )
        .show(viewport_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Offline Storage: {}",
                        state.assets_root.display()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.separator();
                let summary = if state.is_standard_user() {
                    let allowed_cnt = state
                        .model_assets
                        .iter()
                        .filter(|m| state.is_model_asset_allowed(m))
                        .count();
                    format!(
                        "Assets Saya: {allowed_cnt} model 3D • {} file lokal",
                        state.available_model_paths.len(),
                    )
                } else {
                    let master_models = state
                        .model_assets
                        .iter()
                        .filter(|m| m.operasi_id.is_none())
                        .count();
                    let master_media = state
                        .media_assets
                        .iter()
                        .filter(|m| m.operasi_id.is_none())
                        .count();
                    format!(
                        "Total: {master_models} model 3D • {master_media} media • {} file lokal",
                        state.available_model_paths.len(),
                    )
                };
                ui.label(
                    egui::RichText::new(summary)
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                if let Some(banner) = &state.status_banner {
                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(250, 204, 21), banner);
                }
            });
        })
        .response
        .rect
        .height()
}
