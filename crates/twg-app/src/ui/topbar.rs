use bevy_egui::egui;
use twg_shared::{
    models::{BaseLayer, MapMode},
    ws::TacticalCommand,
};

use crate::{
    map::camera::TacticalCamera,
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::ApiClientBridge,
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::markers::{finish_active_draft, save_all_scenario_changes},
    state::{
        AppScreen, AppState, PlaySessionMode, PlaybackState, PlotExit, SkyboxPreset, ToolMode,
    },
    ui::{
        app_icon::draw_app_brand_icon,
        nav_bar::{
            enter_layar_besar, open_profile_screen, trigger_load_operasis, trigger_load_satkers,
            trigger_load_skenarios, trigger_load_tactical_operasis, trigger_load_users,
        },
    },
};

#[allow(clippy::too_many_arguments)]
pub fn draw_top_bar(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) -> f32 {
    let is_play_mode = playback.play_test_mode;
    let bar_fill = if is_play_mode {
        egui::Color32::from_rgb(10, 32, 28)
    } else {
        egui::Color32::from_rgb(13, 19, 31)
    };
    let bar_stroke = if is_play_mode {
        egui::Color32::from_rgb(16, 185, 129)
    } else {
        egui::Color32::from_rgb(32, 44, 66)
    };

    egui::Panel::top("plot_top_bar")
        .frame(
            egui::Frame::new()
                .fill(bar_fill)
                .stroke(egui::Stroke::new(1.0, bar_stroke))
                .inner_margin(egui::Margin::symmetric(10, 5)),
        )
        .show(viewport_ui, |ui| {
            // Row 1: Navigation Breadcrumb | Centered Godot/Unity Play Test Controls | Session Info
            ui.horizontal(|ui| {
                draw_app_brand_icon(ui, 18.0);
                ui.label(
                    egui::RichText::new("TWG-RUST 3D")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.separator();

                // Back to Skenario List
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("< Skenario")
                                .size(11.5)
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        )
                        .fill(egui::Color32::from_rgb(30, 41, 59)),
                    )
                    .clicked()
                {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Skenarios));
                }

                if ui.selectable_label(false, "Operasi").clicked() {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Operasis));
                }

                if ui
                    .selectable_label(
                        false,
                        egui::RichText::new("Tactical")
                            .color(egui::Color32::from_rgb(110, 231, 183)),
                    )
                    .clicked()
                {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Tactical));
                }

                if ui
                    .selectable_label(
                        false,
                        egui::RichText::new("Layar Besar")
                            .color(egui::Color32::from_rgb(253, 224, 71)),
                    )
                    .clicked()
                {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::LayarBesar));
                }

                if state.can_manage_satkers() && ui.selectable_label(false, "Satkers").clicked() {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Satkers));
                }

                if state.can_manage_users() && ui.selectable_label(false, "Users").clicked() {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Users));
                }

                if state.is_super_admin() && ui.selectable_label(false, "Assets 3D").clicked() {
                    state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Assets));
                }

                ui.separator();

                // Breadcrumb: Operasi / Skenario
                let op_name = state
                    .skenario_table
                    .operasi
                    .as_ref()
                    .map(|o| o.name.as_str())
                    .unwrap_or("Operasi");
                let sk_name = state
                    .active_skenario
                    .as_ref()
                    .map(|s| s.name.as_str())
                    .unwrap_or("Belum ada skenario");
                ui.label(
                    egui::RichText::new(format!("{op_name} /"))
                        .size(11.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.label(
                    egui::RichText::new(sk_name)
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(4.0);

                // Prominent Toolbar "Simpan" button (batch-saves all scenario edits, Ctrl+S)
                {
                    let dirty_cnt = state.unsaved_change_count();
                    let is_saving = state.saving_pending_count > 0;
                    let save_label = if is_saving {
                        "Menyimpan...".to_string()
                    } else if dirty_cnt > 0 {
                        format!("Simpan ({dirty_cnt})*")
                    } else {
                        "Simpan".to_string()
                    };
                    let save_fill = if is_saving {
                        egui::Color32::from_rgb(30, 58, 138)
                    } else if dirty_cnt > 0 {
                        egui::Color32::from_rgb(217, 119, 6)
                    } else {
                        egui::Color32::from_rgb(14, 116, 144)
                    };
                    let save_stroke = if dirty_cnt > 0 {
                        egui::Stroke::new(1.2, egui::Color32::from_rgb(253, 224, 71))
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248))
                    };
                    let save_btn = egui::Button::new(
                        egui::RichText::new(save_label)
                            .size(11.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(save_fill)
                    .stroke(save_stroke)
                    .min_size(egui::vec2(86.0, 24.0));
                    let tip = if dirty_cnt > 0 {
                        format!(
                            "Simpan {dirty_cnt} perubahan skenario (posisi, rotasi, skala, material, dll) ke server (Ctrl+S)"
                        )
                    } else {
                        "Simpan skenario (Ctrl+S) — semua perubahan sudah tersimpan".to_string()
                    };
                    if ui
                        .add_enabled(!is_play_mode && !is_saving, save_btn)
                        .on_hover_text(tip)
                        .clicked()
                    {
                        save_all_scenario_changes(state, api, ws);
                    }
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // Prominent Unity / Godot style PLAY / PAUSE / STOP / RESTART transport controls
                egui::Frame::new()
                    .fill(if is_play_mode {
                        egui::Color32::from_rgb(6, 78, 59)
                    } else {
                        egui::Color32::from_rgb(23, 32, 48)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if is_play_mode {
                            egui::Color32::from_rgb(16, 185, 129)
                        } else {
                            egui::Color32::from_rgb(51, 65, 85)
                        },
                    ))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if !is_play_mode {
                                let play_btn = egui::Button::new(
                                    egui::RichText::new("Play Skenario")
                                        .size(12.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(5, 150, 105))
                                .min_size(egui::vec2(106.0, 22.0));
                                if ui
                                    .add(play_btn)
                                    .on_hover_text(
                                        "Masuk Mode Play Skenario (sembunyikan panel editor & buka Panel Animasi)",
                                    )
                                    .clicked()
                                {
                                    state.draft_points_lnglat.clear();
                                    state.picking_marker_for_anim_id = None;
                                    state.tool_mode = ToolMode::Select;
                                    state.play_session_mode = PlaySessionMode::LocalPreview;
                                    playback.enter_play_mode();
                                    state.status_banner = Some(
                                        "Mode Play Skenario (Lokal Editor) aktif — tidak mempengaruhi app lain / Layar Besar."
                                            .into(),
                                    );
                                }
                            } else {
                                let active_cnt = playback
                                    .tracks
                                    .values()
                                    .filter(|t| t.status == crate::state::AnimRunStatus::Playing)
                                    .count();
                                ui.label(
                                    egui::RichText::new(if active_cnt > 0 {
                                        format!("PLAY MODE ({active_cnt} berjalan)")
                                    } else {
                                        "PLAY MODE (SIAP)".to_string()
                                    })
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(110, 231, 183)),
                                );

                                let stop_btn = egui::Button::new(
                                    egui::RichText::new("Keluar Play Mode")
                                        .size(11.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(185, 28, 28))
                                .min_size(egui::vec2(110.0, 22.0));
                                if ui
                                    .add(stop_btn)
                                    .on_hover_text(
                                        "Keluar dari Play Skenario, kembalikan semua unit ke posisi editor & tampilkan panel Editor",
                                    )
                                    .clicked()
                                {
                                    playback.stop_play_test();
                                    state.status_banner = Some("Kembali ke Mode Editor".into());
                                    broadcast_playback_cmd(state, playback, ws, "stop_anim");
                                }
                            }
                        });
                    });

                // Right side: WS status, user, logout
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Logout").clicked() {
                        state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Login));
                    }

                    if state.offline_preview {
                        ui.colored_label(
                            egui::Color32::from_rgb(250, 204, 21),
                            "OFFLINE PREVIEW",
                        );
                    } else {
                        let ws_color = if state.ws_connected {
                            egui::Color32::from_rgb(74, 222, 128)
                        } else {
                            egui::Color32::from_rgb(248, 113, 113)
                        };
                        let ws_text = if state.ws_connected {
                            format!("WS LIVE ({})", state.online_operators.len())
                        } else {
                            "WS OFFLINE".to_string()
                        };
                        ui.colored_label(ws_color, ws_text);
                    }

                    if let Some(user) = state.current_user.clone()
                        && ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!(
                                        "{} [{}]  •  Profil",
                                        user.username,
                                        user.role.label()
                                    ))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(203, 213, 225)),
                                )
                                .fill(egui::Color32::from_rgb(24, 34, 53)),
                            )
                            .on_hover_text("Buka halaman Profil & Ganti Password")
                            .clicked()
                    {
                        state.pending_plot_exit = Some(PlotExit::Page(AppScreen::Profile));
                    }
                });
            });

            ui.add_space(3.0);
            ui.separator();
            ui.add_space(3.0);

            // Row 2: Godot/Unity Transform Toolbar | Map Plot Tools | Base Layers | Display Options
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;

                // Dedicated Segmented Transform Toolbar: Select, Move, Rotate, Scale
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(10, 15, 25))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 62, 88)))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            for (tool, shortcut, tip) in [
                                (
                                    ToolMode::Select,
                                    "1",
                                    "Select (1): Pilih objek 3D di peta (drag area kosong untuk pan)",
                                ),
                                (
                                    ToolMode::MoveMarker,
                                    "2",
                                    "Move (2): Gizmo pindah X / Y (Alt) / Z & handle tengah untuk Free Move",
                                ),
                                (
                                    ToolMode::RotateMarker,
                                    "3",
                                    "Rotate (3): Gizmo putar cincin X / Y / Z & bola tengah untuk Free Rotate",
                                ),
                                (
                                    ToolMode::ScaleMarker,
                                    "4",
                                    "Scale (4): Gizmo skala X / Y / Z & kubus tengah untuk Free Scale (X, Y, Z)",
                                ),
                            ] {
                                let allowed = !is_play_mode || tool == ToolMode::Select;
                                let active = state.tool_mode == tool;
                                if draw_toolbar_tool_btn(ui, tool, shortcut, active, allowed, tip) {
                                    state.tool_mode = tool;
                                    state.draft_points_lnglat.clear();
                                    state.measure_points_lnglat.clear();
                                }
                            }
                        });
                    });

                ui.separator();

                // Secondary plotting tools (Place 3D removed — use 3D Catalog drag-drop or + Tambah)
                for tool in [
                    ToolMode::DrawLine,
                    ToolMode::PlaceText,
                    ToolMode::Measure,
                ] {
                    let allowed = !is_play_mode || tool == ToolMode::Measure;
                    let active = state.tool_mode == tool;
                    let tip = match tool {
                        ToolMode::DrawLine => "Draw Line: klik titik-titik di peta, lalu Enter untuk selesai",
                        ToolMode::PlaceText => "Add Label: letakkan satu label, lalu otomatis kembali ke Select",
                        ToolMode::Measure => "Measure: ukur jarak antar titik di peta",
                        _ => unreachable!(),
                    };
                    if draw_toolbar_tool_btn(ui, tool, "", active, allowed, tip) {
                        state.tool_mode = tool;
                        state.draft_points_lnglat.clear();
                        if tool != ToolMode::Measure {
                            state.measure_points_lnglat.clear();
                        }
                    }
                }

                if matches!(
                    state.tool_mode,
                    ToolMode::DrawLine
                        | ToolMode::DrawBuilding
                        | ToolMode::DrawBasicPolygon
                        | ToolMode::DrawGroundPolygon
                        | ToolMode::DrawRoute
                        | ToolMode::DrawTrafficLane
                        | ToolMode::DrawTrafficQueueLane
                        | ToolMode::DrawTrafficDiversionLane
                ) && !state.draft_points_lnglat.is_empty()
                {
                    let finish_btn = egui::Button::new(
                        egui::RichText::new("Finish (Enter)")
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105));
                    if ui.add(finish_btn).clicked() {
                        finish_active_draft(state, api);
                    }
                    if ui.button("Batal").clicked() {
                        state.draft_points_lnglat.clear();
                        state.drawing_traffic_lane_for_anim_id = None;
                        state.drawing_traffic_is_signal = false;
                        state.drawing_traffic_queue_target = None;
                        state.drawing_traffic_queue_is_gridlock = false;
                        state.drawing_traffic_queue_is_manual = false;
                        state.drawing_diversion_target = None;
                        state.tool_mode = ToolMode::Select;
                    }
                }

                if state.tool_mode == ToolMode::PlaceTrafficLight {
                    let done_btn = egui::Button::new(
                        egui::RichText::new("Selesai Pasang Lampu (Esc)")
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105));
                    if ui.add(done_btn).clicked() {
                        state.placing_traffic_light_for_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    }
                }

                if state.tool_mode == ToolMode::Measure
                    && !state.measure_points_lnglat.is_empty()
                    && ui.button("Clear Measure").clicked()
                {
                    state.measure_points_lnglat.clear();
                }

                ui.separator();

                // Map base layer switchers
                for (layer, mode, text) in [
                    (BaseLayer::Osm, MapMode::Outdoor, "OSM"),
                    (BaseLayer::Satellite, MapMode::Outdoor, "Satellite"),
                    (BaseLayer::GridDark, MapMode::Indoor, "Indoor Dark"),
                    (BaseLayer::GridLight, MapMode::Indoor, "Indoor Light"),
                ] {
                    let active = state.active_base_layer == layer && state.active_map_mode == mode;
                    if ui.selectable_label(active, text).clicked() && !active {
                        state.active_base_layer = layer;
                        state.active_map_mode = mode;
                        state.skenario_dirty = true;
                        if let Some(sk) = &mut state.active_skenario {
                            sk.base_layer = layer;
                            sk.map_mode = mode;
                        }
                    }
                }

                ui.separator();

                // Skybox selector (Tanpa Skybox / Siang Hari / Malam Hari) — persisted per-skenario
                let prev_skybox = state.active_skybox;
                ui.label(
                    egui::RichText::new("Skybox:")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                egui::ComboBox::from_id_salt("topbar_skybox_combo")
                    .selected_text(state.active_skybox.short_label())
                    .width(116.0)
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Tanpa Skybox")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.selectable_value(
                            &mut state.active_skybox,
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
                            ui.selectable_value(&mut state.active_skybox, preset, preset.label());
                        }
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Malam Hari")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(147, 197, 253)),
                        );
                        for preset in [SkyboxPreset::NightStarry, SkyboxPreset::NightTactical] {
                            ui.selectable_value(&mut state.active_skybox, preset, preset.label());
                        }
                    });
                if state.active_skybox != prev_skybox {
                    state.skenario_dirty = true;
                    if let Some(sk) = &mut state.active_skenario {
                        sk.skybox = state.active_skybox;
                    }
                }

                ui.separator();

                ui.checkbox(&mut state.show_unit_labels, "Labels");
                ui.checkbox(&mut state.show_text_markers, "Text");
                ui.checkbox(&mut state.show_gizmos, "Gizmos");
                ui.checkbox(&mut state.trackpad_mode, "Trackpad")
                    .on_hover_text(
                        "Mode Trackpad (MacBook / Laptop):\n\
                         • Scroll 2 jari = Pan peta 360°\n\
                         • Pinch 2 jari / Ctrl+Scroll = Zoom\n\
                         • Alt/Option + Drag Kiri (atau Scroll 2 jari) = Orbit / Pitch\n\
                         • Space + Drag Kiri = Pan alternatif\n\
                         • Putar 2 jari = Rotasi arah peta",
                    );

                if ui.button("Reset Cam (R)").clicked()
                    && let Some(sk) = &state.active_skenario
                {
                    tcam.apply_skenario_view(sk.zoom, sk.pitch, sk.bearing, state.geo_anchor.cos_lat);
                }

                if ui
                    .add_enabled(
                        !state.sync_status.is_syncing,
                        egui::Button::new(if state.sync_status.is_syncing {
                            format!(
                                "Sync {}/{}",
                                state.sync_status.completed_files, state.sync_status.total_files
                            )
                        } else {
                            "Sync 3D".to_string()
                        }),
                    )
                    .clicked()
                {
                    sync_bridge.send(AssetSyncCommand::SyncWithServer {
                        server_url: state.server_url.clone(),
                        local_root: state.assets_root.clone(),
                    });
                }
            });
        })
        .response
        .rect
        .height()
}

pub fn broadcast_tactical_anim_cmd(
    state: &AppState,
    ws: &WsClientBridge,
    command: &str,
    anim_id: Option<uuid::Uuid>,
    play_all: bool,
    speed_multiplier: f32,
) {
    // Strictly only broadcast when Play Skenario was opened from Tactical / Simulasi!
    // Play Skenario opened from Plot Editor (`LocalPreview`) is 100% isolated locally.
    if state.offline_preview || !state.is_tactical_live() {
        return;
    }
    let Some(sk) = &state.active_skenario else {
        return;
    };
    let marker_id = anim_id
        .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
        .and_then(|a| a.marker_id);
    ws.send(WsOutCommand::SendCommand(TacticalCommand {
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id,
        command: command.into(),
        payload: serde_json::json!({
            "anim_id": anim_id,
            "play_all": play_all,
            "speed_multiplier": speed_multiplier,
        }),
        save_history: false,
    }));
}

pub fn broadcast_tactical_queue_clear_cmd(
    state: &AppState,
    ws: &WsClientBridge,
    anim_id: uuid::Uuid,
    trigger_elapsed_sec: f64,
) {
    if state.offline_preview || !state.is_tactical_live() {
        return;
    }
    let Some(sk) = &state.active_skenario else {
        return;
    };
    ws.send(WsOutCommand::SendCommand(TacticalCommand {
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: None,
        command: "traffic_queue_clear".into(),
        payload: serde_json::json!({
            "anim_id": anim_id,
            "trigger_elapsed_sec": trigger_elapsed_sec,
        }),
        save_history: false,
    }));
}

pub fn broadcast_tactical_queue_manual_cmd(
    state: &AppState,
    ws: &WsClientBridge,
    anim_id: uuid::Uuid,
    action: twg_shared::traffic_queue::QueueManualAction,
    trigger_elapsed_sec: f64,
) {
    if state.offline_preview || !state.is_tactical_live() {
        return;
    }
    let Some(sk) = &state.active_skenario else {
        return;
    };
    ws.send(WsOutCommand::SendCommand(TacticalCommand {
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: None,
        command: "traffic_queue_manual".into(),
        payload: serde_json::json!({
            "anim_id": anim_id,
            "manual_action": action,
            "trigger_elapsed_sec": trigger_elapsed_sec,
        }),
        save_history: false,
    }));
}

pub fn broadcast_tactical_diversion_toggle_cmd(
    state: &AppState,
    ws: &WsClientBridge,
    anim_id: uuid::Uuid,
    is_diverted: bool,
    trigger_elapsed_sec: f64,
) {
    if state.offline_preview || !state.is_tactical_live() {
        return;
    }
    let Some(sk) = &state.active_skenario else {
        return;
    };
    ws.send(WsOutCommand::SendCommand(TacticalCommand {
        operasi_id: sk.operasi_id,
        skenario_id: sk.id,
        marker_id: None,
        command: "traffic_diversion_toggle".into(),
        payload: serde_json::json!({
            "anim_id": anim_id,
            "is_diverted": is_diverted,
            "trigger_elapsed_sec": trigger_elapsed_sec,
        }),
        save_history: false,
    }));
}

pub fn broadcast_playback_cmd(
    state: &AppState,
    playback: &PlaybackState,
    ws: &WsClientBridge,
    command: &str,
) {
    broadcast_tactical_anim_cmd(
        state,
        ws,
        command,
        playback.active_anim_id,
        playback.play_all_anims,
        playback.speed_multiplier,
    );
}

fn draw_toolbar_tool_btn(
    ui: &mut egui::Ui,
    tool: ToolMode,
    shortcut: &str,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> bool {
    let label = tool.label();
    let font = egui::FontId::proportional(11.5);
    let key_font = egui::FontId::proportional(9.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, egui::Color32::WHITE);
    let key_galley = ui.painter().layout_no_wrap(
        shortcut.to_string(),
        key_font,
        egui::Color32::from_rgb(148, 163, 184),
    );
    let icon_w = 14.0;
    let width = 8.0 + icon_w + 5.0 + galley.size().x + 5.0 + key_galley.size().x + 8.0;
    let desired = egui::vec2(width, 24.0);

    let (rect, resp) = ui.allocate_exact_size(
        desired,
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let hovered = enabled && resp.hovered();

    let fill = if !enabled {
        egui::Color32::from_rgb(16, 22, 34)
    } else if active {
        egui::Color32::from_rgb(14, 116, 144)
    } else if hovered {
        egui::Color32::from_rgb(30, 44, 66)
    } else {
        egui::Color32::from_rgb(20, 29, 44)
    };
    let stroke_col = if active {
        egui::Color32::from_rgb(56, 189, 248)
    } else if hovered {
        egui::Color32::from_rgb(71, 85, 105)
    } else {
        egui::Color32::TRANSPARENT
    };
    let fg = if !enabled {
        egui::Color32::from_rgb(82, 96, 118)
    } else if active {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(203, 213, 225)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 4.5, fill);
    if stroke_col != egui::Color32::TRANSPARENT {
        painter.rect_stroke(
            rect,
            4.5,
            egui::Stroke::new(1.0, stroke_col),
            egui::StrokeKind::Inside,
        );
    }

    let icon_center = egui::pos2(rect.left() + 8.0 + icon_w * 0.5, rect.center().y);
    draw_toolbar_tool_icon(painter, icon_center, tool, fg, active);

    let text_pos = egui::pos2(
        rect.left() + 8.0 + icon_w + 5.0,
        rect.center().y - galley.size().y * 0.5,
    );
    painter.galley(text_pos, galley, fg);

    let key_pos = egui::pos2(
        rect.right() - 8.0 - key_galley.size().x,
        rect.center().y - key_galley.size().y * 0.5,
    );
    let key_col = if active {
        egui::Color32::from_rgb(186, 230, 253)
    } else {
        egui::Color32::from_rgb(100, 116, 139)
    };
    painter.galley(key_pos, key_galley, key_col);

    let clicked = enabled && resp.clicked();
    let _ = resp.on_hover_text(tooltip);
    clicked
}

/// Toolbar icons are drawn from local vector paths, not icon-font glyphs.
fn draw_toolbar_tool_icon(
    painter: &egui::Painter,
    c: egui::Pos2,
    tool: ToolMode,
    fg: egui::Color32,
    active: bool,
) {
    match tool {
        ToolMode::Select => {
            // Crisp pointer arrow icon
            let pts = vec![
                c + egui::vec2(-3.5, -5.5),
                c + egui::vec2(-3.5, 4.5),
                c + egui::vec2(-0.5, 2.0),
                c + egui::vec2(2.2, 5.5),
                c + egui::vec2(3.8, 4.2),
                c + egui::vec2(1.2, 0.8),
                c + egui::vec2(4.5, 0.5),
            ];
            painter.add(egui::Shape::line(
                [pts.as_slice(), &[pts[0]]].concat(),
                egui::Stroke::new(1.3, fg),
            ));
        }
        ToolMode::MoveMarker => {
            // 4-way gizmo cross arrows (subtle X/Y axis tint when inactive, crisp white when active)
            let x_col = if active {
                fg
            } else {
                egui::Color32::from_rgb(248, 113, 113)
            };
            let y_col = if active {
                fg
            } else {
                egui::Color32::from_rgb(74, 222, 128)
            };
            painter.line_segment(
                [c - egui::vec2(5.5, 0.0), c + egui::vec2(5.5, 0.0)],
                egui::Stroke::new(1.4, x_col),
            );
            painter.line_segment(
                [c - egui::vec2(0.0, 5.5), c + egui::vec2(0.0, 5.5)],
                egui::Stroke::new(1.4, y_col),
            );
            // Tiny arrowheads on 4 tips
            for (tip, d1, d2, col) in [
                (
                    egui::vec2(6.0, 0.0),
                    egui::vec2(3.2, -2.2),
                    egui::vec2(3.2, 2.2),
                    x_col,
                ),
                (
                    egui::vec2(-6.0, 0.0),
                    egui::vec2(-3.2, -2.2),
                    egui::vec2(-3.2, 2.2),
                    x_col,
                ),
                (
                    egui::vec2(0.0, -6.0),
                    egui::vec2(-2.2, -3.2),
                    egui::vec2(2.2, -3.2),
                    y_col,
                ),
                (
                    egui::vec2(0.0, 6.0),
                    egui::vec2(-2.2, 3.2),
                    egui::vec2(2.2, 3.2),
                    y_col,
                ),
            ] {
                painter.add(egui::Shape::convex_polygon(
                    vec![c + tip, c + d1, c + d2],
                    col,
                    egui::Stroke::NONE,
                ));
            }
        }
        ToolMode::RotateMarker => {
            // Circular orbit arc + arrowhead
            let arc_col = if active {
                fg
            } else {
                egui::Color32::from_rgb(96, 165, 250)
            };
            let r = 5.0;
            let mut pts = Vec::with_capacity(14);
            for i in 0..=12 {
                let a = -0.3 + (i as f32 / 12.0) * (std::f32::consts::TAU * 0.78);
                pts.push(c + egui::vec2(a.cos(), a.sin()) * r);
            }
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, arc_col)));
            // Arrowhead at start of arc
            let tip = c + egui::vec2(5.0, -1.6);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    tip + egui::vec2(0.6, -2.4),
                    tip + egui::vec2(-2.4, -0.4),
                    tip + egui::vec2(1.4, 1.2),
                ],
                arc_col,
                egui::Stroke::NONE,
            ));
        }
        ToolMode::ScaleMarker => {
            // Diagonal scale shaft with cube handles at tips
            let col = if active {
                fg
            } else {
                egui::Color32::from_rgb(250, 204, 21)
            };
            let a = c + egui::vec2(-4.2, 4.2);
            let b = c + egui::vec2(4.2, -4.2);
            painter.line_segment([a, b], egui::Stroke::new(1.4, col));
            painter.rect_filled(
                egui::Rect::from_center_size(a, egui::vec2(3.6, 3.6)),
                0.5,
                col,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(b, egui::vec2(4.4, 4.4)),
                0.5,
                col,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(3.0, 3.0)),
                0.5,
                egui::Stroke::new(1.0, fg),
                egui::StrokeKind::Inside,
            );
        }
        ToolMode::DrawLine => {
            let line_col = if active {
                fg
            } else {
                egui::Color32::from_rgb(56, 189, 248)
            };
            let a = c + egui::vec2(-5.5, 4.0);
            let b = c + egui::vec2(-1.2, -2.0);
            let d = c + egui::vec2(5.5, 2.5);
            painter.line_segment([a, b], egui::Stroke::new(1.8, line_col));
            painter.line_segment([b, d], egui::Stroke::new(1.8, line_col));
            for point in [a, b, d] {
                painter.circle_filled(point, 2.1, line_col);
                painter.circle_stroke(point, 2.1, egui::Stroke::new(0.8, fg));
            }
        }
        ToolMode::PlaceText => {
            let col = if active {
                fg
            } else {
                egui::Color32::from_rgb(196, 181, 253)
            };
            let body = egui::Rect::from_center_size(c + egui::vec2(0.0, -0.8), egui::vec2(12.0, 9.0));
            painter.rect_stroke(body, 2.0, egui::Stroke::new(1.4, col), egui::StrokeKind::Inside);
            painter.line_segment(
                [body.left_center() + egui::vec2(2.5, -1.5), body.right_center() + egui::vec2(-2.5, -1.5)],
                egui::Stroke::new(1.3, col),
            );
            painter.line_segment(
                [body.left_center() + egui::vec2(2.5, 1.8), body.center() + egui::vec2(1.5, 1.8)],
                egui::Stroke::new(1.3, col),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    body.left_bottom() + egui::vec2(3.0, 0.0),
                    body.left_bottom() + egui::vec2(6.0, 0.0),
                    body.left_bottom() + egui::vec2(4.2, 3.5),
                ],
                col,
                egui::Stroke::NONE,
            ));
        }
        ToolMode::Measure => {
            let col = if active {
                fg
            } else {
                egui::Color32::from_rgb(45, 212, 191)
            };
            let a = c + egui::vec2(-5.2, 4.8);
            let b = c + egui::vec2(5.2, -4.8);
            painter.line_segment([a, b], egui::Stroke::new(2.4, col));
            let dir = (b - a).normalized();
            let normal = egui::vec2(-dir.y, dir.x);
            for t in [0.12, 0.36, 0.60, 0.84] {
                let p = a + (b - a) * t;
                let tick_len = if (t - 0.60).abs() < f32::EPSILON { 3.6 } else { 2.4 };
                painter.line_segment([p - normal * tick_len, p + normal * tick_len], egui::Stroke::new(1.0, col));
            }
            painter.circle_filled(a, 1.6, fg);
            painter.circle_filled(b, 1.6, fg);
        }
        _ => {}
    }
}

pub(super) fn leave_plot(
    page: AppScreen,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) {
    match page {
        AppScreen::Skenarios => {
            playback.stop_play_test();
            state.draft_points_lnglat.clear();
            state.measure_points_lnglat.clear();
            state.tool_mode = ToolMode::Select;
            state.screen = AppScreen::Skenarios;
            trigger_load_skenarios(state, api);
        }
        AppScreen::Operasis => {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.screen = AppScreen::Operasis;
            trigger_load_operasis(state, api);
        }
        AppScreen::Tactical => {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.screen = AppScreen::Tactical;
            trigger_load_tactical_operasis(state, api);
        }
        AppScreen::LayarBesar => {
            enter_layar_besar(state, playback, ws);
        }
        AppScreen::Satkers => {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.screen = AppScreen::Satkers;
            trigger_load_satkers(state, api);
        }
        AppScreen::Users => {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.screen = AppScreen::Users;
            trigger_load_users(state, api);
            trigger_load_satkers(state, api);
        }
        AppScreen::Assets => {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.screen = AppScreen::Assets;
        }
        AppScreen::Login => {
            playback.stop_play_test();
            ws.send(WsOutCommand::Disconnect);
            state.screen = AppScreen::Login;
            state.offline_preview = false;
            state.auth_token = None;
            state.refresh_token = None;
            state.current_user = None;
            state.active_skenario = None;
            state.selected_operasi_id = None;
            state.skenario_table.operasi = None;
            state.ws_connected = false;
        }
        AppScreen::Profile => {
            open_profile_screen(state, playback, ws);
        }
        AppScreen::PlotSkenario => {}
    }
}
