use bevy_egui::egui;
use chrono::{DateTime, Local, Utc};
use twg_shared::models::{Operasi, Skenario};

use crate::{
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::{ApiClientBridge, ApiCommand},
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::markers::current_auth,
    state::{AppScreen, AppState, PlaybackState},
};

pub fn format_timestamp(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Local).format("%d %b %Y %H:%M").to_string()
}

pub fn trigger_load_operasis(state: &mut AppState, api: &ApiClientBridge) {
    if state.offline_preview {
        state.operasi_table.loading = false;
        state.operasi_table.total = state.operasis.len() as i64;
        return;
    }
    let Some(auth) = current_auth(state) else {
        return;
    };
    state.operasi_table.loading = true;
    state.operasi_table.error = None;
    let search = {
        let q = state.operasi_table.applied_search.trim();
        if q.is_empty() {
            None
        } else {
            Some(q.to_string())
        }
    };
    api.send(ApiCommand::LoadOperasis {
        auth,
        page: state.operasi_table.page.max(1),
        per_page: state.operasi_table.per_page.clamp(1, 200),
        search,
    });
}

pub fn trigger_load_operasi_options(state: &AppState, api: &ApiClientBridge) {
    if state.offline_preview {
        return;
    }
    if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::LoadOperasiOptions { auth });
    }
}

pub fn trigger_load_skenarios(state: &mut AppState, api: &ApiClientBridge) {
    if state.offline_preview {
        state.skenario_table.loading = false;
        state.skenario_table.total = state.skenarios.len() as i64;
        return;
    }
    let Some(auth) = current_auth(state) else {
        return;
    };
    state.skenario_table.loading = true;
    state.skenario_table.error = None;
    let search = {
        let q = state.skenario_table.applied_search.trim();
        if q.is_empty() {
            None
        } else {
            Some(q.to_string())
        }
    };
    api.send(ApiCommand::LoadSkenarios {
        auth,
        operasi_id: state.skenario_table.filter_operasi_id,
        page: state.skenario_table.page.max(1),
        per_page: state.skenario_table.per_page.clamp(1, 200),
        search,
    });
}

pub fn navigate_to_operasi_skenarios(
    state: &mut AppState,
    api: &ApiClientBridge,
    op: Operasi,
) {
    let op_id = op.id;
    let changed_op = state.skenario_table.filter_operasi_id != Some(op_id);
    state.selected_operasi_id = Some(op_id);
    state.skenario_table.filter_operasi_id = Some(op_id);
    state.skenario_table.operasi = Some(op);
    state.skenario_table.page = 1;
    state.skenario_table.search.clear();
    state.skenario_table.applied_search.clear();
    state.skenario_table.close_modal();
    if changed_op && !state.offline_preview {
        state.skenarios.clear();
        state.skenario_table.total = 0;
    }
    state.screen = AppScreen::Skenarios;
    trigger_load_operasi_options(state, api);
    trigger_load_skenarios(state, api);
}

pub fn navigate_to_all_skenarios(state: &mut AppState, api: &ApiClientBridge) {
    state.skenario_table.close_modal();
    state.screen = AppScreen::Skenarios;
    trigger_load_operasi_options(state, api);
    trigger_load_skenarios(state, api);
}

pub fn open_plot_skenario(
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    sk: &Skenario,
) {
    playback.stop_play_test();
    if let Some(op) = state
        .skenario_table
        .operasi_options
        .iter()
        .find(|o| o.id == sk.operasi_id)
        .cloned()
    {
        state.skenario_table.operasi = Some(op);
    }
    if state.offline_preview {
        state.active_skenario = Some(sk.clone());
        state.screen = AppScreen::PlotSkenario;
        return;
    }
    if let Some(auth) = current_auth(state) {
        state.skenario_table.pending_open_plot_id = Some(sk.id);
        state.status_banner = Some(format!("Memuat skenario '{}'...", sk.name));
        api.send(ApiCommand::LoadBundle {
            auth,
            skenario_id: sk.id,
        });
    }
}

pub fn draw_screen_nav_tabs(
    ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
) {
    let on_op = state.screen == AppScreen::Operasis;
    if ui
        .selectable_label(on_op, egui::RichText::new("Operasi").strong())
        .clicked()
        && !on_op
    {
        playback.stop_play_test();
        state.skenario_table.close_modal();
        state.restore_active_skenario_map_state();
        state.screen = AppScreen::Operasis;
        trigger_load_operasis(state, api);
    }

    let on_sk = state.screen == AppScreen::Skenarios;
    if ui
        .selectable_label(on_sk, egui::RichText::new("Skenario").strong())
        .clicked()
        && (!on_sk || state.is_skenario_map_preview_active())
    {
        playback.stop_play_test();
        state.restore_active_skenario_map_state();
        navigate_to_all_skenarios(state, api);
    }

    if state.active_skenario.is_some() {
        let on_plot = state.screen == AppScreen::PlotSkenario;
        if ui
            .selectable_label(on_plot, egui::RichText::new("Plot Skenario").strong())
            .clicked()
            && !on_plot
        {
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            state.screen = AppScreen::PlotSkenario;
        }
    }

    if state.is_super_admin() {
        let on_assets = state.screen == AppScreen::Assets;
        if ui
            .selectable_label(on_assets, egui::RichText::new("Assets 3D (Admin)").strong())
            .clicked()
            && !on_assets
        {
            playback.stop_play_test();
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            state.screen = AppScreen::Assets;
        }
    }
}

pub fn draw_global_top_bar(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) -> f32 {
    egui::Panel::top("global_top_bar")
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 23, 42))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59)))
                .inner_margin(egui::Margin::symmetric(14, 8)),
        )
        .show(viewport_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("TWG-RUST 3D")
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.separator();

                draw_screen_nav_tabs(ui, state, playback, api);

                ui.separator();
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
                        })
                        .small(),
                    )
                    .clicked()
                {
                    sync_bridge.send(AssetSyncCommand::SyncWithServer {
                        server_url: state.server_url.clone(),
                        local_root: state.assets_root.clone(),
                    });
                }

                if let Some(banner) = &state.status_banner {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(banner)
                            .size(11.5)
                            .color(egui::Color32::from_rgb(250, 204, 21)),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Logout").clicked() {
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
                        state.skenario_table.filter_operasi_id = None;
                        state.skenario_table.operasi_options.clear();
                        state.ws_connected = false;
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

                    if let Some(user) = &state.current_user {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} [{}]",
                                user.username,
                                user.role.as_str()
                            ))
                            .size(11.5)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                        );
                    }
                });
            });
        })
        .response
        .rect
        .height()
}

pub fn draw_modal_backdrop(ctx: &egui::Context, id_salt: &str) {
    let screen_rect = ctx.viewport_rect();
    egui::Area::new(egui::Id::new(id_salt))
        .order(egui::Order::Middle)
        .fixed_pos(screen_rect.min)
        .show(ctx, |ui| {
            let (rect, _resp) = ui.allocate_exact_size(screen_rect.size(), egui::Sense::click());
            ui.painter().rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(4, 8, 16, 175),
            );
        });
}
