use bevy_egui::egui;
use chrono::{DateTime, Local, Utc};
use twg_shared::{
    models::{Operasi, Skenario},
    ws::WsClientMode,
};

use crate::{
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::{ApiClientBridge, ApiCommand},
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::markers::current_auth,
    state::{AppScreen, AppState, PlayPanelTab, PlaySessionMode, PlaybackState},
    ui::app_icon::draw_app_brand_icon,
};

pub fn format_timestamp(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Local)
        .format("%d %b %Y %H:%M")
        .to_string()
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

pub fn navigate_to_operasi_skenarios(state: &mut AppState, api: &ApiClientBridge, op: Operasi) {
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

pub fn trigger_load_satkers(state: &mut AppState, api: &ApiClientBridge) {
    if state.offline_preview {
        state.satker_table.loading = false;
        return;
    }
    let Some(auth) = current_auth(state) else {
        return;
    };
    state.satker_table.loading = true;
    state.satker_table.error = None;
    api.send(ApiCommand::LoadSatkers { auth });
}

pub fn trigger_load_users(state: &mut AppState, api: &ApiClientBridge) {
    if state.offline_preview {
        state.user_table.loading = false;
        return;
    }
    let Some(auth) = current_auth(state) else {
        return;
    };
    state.user_table.loading = true;
    state.user_table.error = None;
    api.send(ApiCommand::LoadUsers { auth });
}

pub fn trigger_load_tactical_operasis(state: &mut AppState, api: &ApiClientBridge) {
    if state.offline_preview {
        state.tactical_page.loading = false;
        return;
    }
    let Some(auth) = current_auth(state) else {
        return;
    };
    state.tactical_page.loading = true;
    state.tactical_page.error = None;
    let search = {
        let q = state.tactical_page.applied_search.trim();
        if q.is_empty() {
            None
        } else {
            Some(q.to_string())
        }
    };
    api.send(ApiCommand::LoadTacticalOperasis {
        auth,
        page: state.tactical_page.page.max(1),
        per_page: state.tactical_page.per_page.clamp(1, 50),
        search,
    });
}

/// Opens a scenario directly into **Play Skenario Tactical** (realtime live sync mode).
/// When `broadcast_cmd` is `Some("load_skenario" | "reload_skenario")` and the current user
/// is authorized (`admin` or satker-matching `admin_satker`), Layar Besar is instructed to
/// load/reload this scenario as well once the bundle finishes loading.
pub fn open_tactical_skenario(
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    sk: &Skenario,
    operasi_name: String,
    sibling_skenarios: Vec<Skenario>,
    broadcast_cmd: Option<&'static str>,
) {
    playback.stop_play_test();
    state.play_session_mode = PlaySessionMode::TacticalLive;
    state.tactical_page.active_operasi_name = operasi_name;
    state.tactical_page.active_operasi_skenarios = sibling_skenarios;
    state.tactical_page.play_panel_tab = PlayPanelTab::Animasi;
    state.tactical_page.pending_open_tactical_id = Some(sk.id);
    state.tactical_page.pending_broadcast_big_screen = if state.can_control_skenario_big_screen(sk)
    {
        broadcast_cmd
    } else {
        None
    };
    state.skenario_table.pending_open_plot_id = None;

    if state.offline_preview {
        state.active_skenario = Some(sk.clone());
        state.screen = AppScreen::PlotSkenario;
        playback.enter_play_mode();
        return;
    }

    if let Some(auth) = current_auth(state) {
        state.status_banner = Some(format!("Memuat Simulasi Tactical '{}'...", sk.name));
        api.send(ApiCommand::LoadBundle {
            auth: auth.clone(),
            skenario_id: sk.id,
        });
        if state.tactical_page.active_operasi_skenarios.is_empty() {
            api.send(ApiCommand::LoadOperasiSkenariosForTactical {
                auth,
                operasi_id: sk.operasi_id,
            });
        }
    }
}

pub fn enter_layar_besar(state: &mut AppState, playback: &mut PlaybackState, ws: &WsClientBridge) {
    playback.stop_play_test();
    state.skenario_table.close_modal();
    state.play_session_mode = PlaySessionMode::BigScreen;
    state.active_skenario = None;
    state.markers.clear();
    state.buildings.clear();
    state.lines.clear();
    state.text_markers.clear();
    state.particles.clear();
    state.alurs.clear();
    state.anims.clear();
    state.big_screen.loaded_by_actor = None;
    state.big_screen.pending_skenario_id = None;
    state.big_screen.pending_camera_payload = None;
    state.big_screen.skip_next_cached_state = false;
    state.big_screen.last_event_text = Some(
        "Menunggu skenario di-load dari Play Skenario Tactical (Admin / Admin Satker)...".into(),
    );
    state.screen = AppScreen::LayarBesar;
    ws.send(WsOutCommand::Subscribe {
        skenario_id: None,
        client_mode: WsClientMode::BigScreen,
    });
}

pub fn open_plot_skenario(
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    sk: &Skenario,
) {
    playback.stop_play_test();
    state.play_session_mode = PlaySessionMode::LocalPreview;
    state.tactical_page.pending_open_tactical_id = None;
    state.tactical_page.pending_broadcast_big_screen = None;
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

pub fn open_profile_screen(
    state: &mut AppState,
    playback: &mut PlaybackState,
    ws: &WsClientBridge,
) {
    playback.stop_play_test();
    state.play_session_mode = PlaySessionMode::LocalPreview;
    state.skenario_table.close_modal();
    state.restore_active_skenario_map_state();
    if let Some(u) = state.current_user.clone() {
        state.profile_form.load_from_user(&u);
    }
    state.screen = AppScreen::Profile;
    ws.send(WsOutCommand::Subscribe {
        skenario_id: None,
        client_mode: WsClientMode::Idle,
    });
}

pub fn draw_screen_nav_tabs(
    ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) {
    // Dedicated Layar Besar accounts (`layar_besar` & `layar_besar_satker`) ONLY see Layar Besar (+ Profil).
    if state.is_big_screen_only_role() {
        let on_big = state.screen == AppScreen::LayarBesar;
        if ui
            .selectable_label(
                on_big,
                egui::RichText::new("Layar Besar")
                    .strong()
                    .color(if on_big {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(253, 224, 71)
                    }),
            )
            .clicked()
            && !on_big
        {
            enter_layar_besar(state, playback, ws);
        }

        let on_profile = state.screen == AppScreen::Profile;
        if ui
            .selectable_label(on_profile, egui::RichText::new("Profil").strong())
            .clicked()
            && !on_profile
        {
            open_profile_screen(state, playback, ws);
        }
        return;
    }

    let on_op = state.screen == AppScreen::Operasis;
    if ui
        .selectable_label(on_op, egui::RichText::new("Operasi").strong())
        .clicked()
        && !on_op
    {
        playback.stop_play_test();
        state.play_session_mode = PlaySessionMode::LocalPreview;
        state.skenario_table.close_modal();
        state.restore_active_skenario_map_state();
        state.screen = AppScreen::Operasis;
        ws.send(WsOutCommand::Subscribe {
            skenario_id: None,
            client_mode: WsClientMode::Idle,
        });
        trigger_load_operasis(state, api);
    }

    let on_sk = state.screen == AppScreen::Skenarios;
    if ui
        .selectable_label(on_sk, egui::RichText::new("Skenario").strong())
        .clicked()
        && (!on_sk || state.is_skenario_map_preview_active())
    {
        playback.stop_play_test();
        state.play_session_mode = PlaySessionMode::LocalPreview;
        state.restore_active_skenario_map_state();
        ws.send(WsOutCommand::Subscribe {
            skenario_id: None,
            client_mode: WsClientMode::Idle,
        });
        navigate_to_all_skenarios(state, api);
    }

    if state.active_skenario.is_some() {
        let on_plot = state.screen == AppScreen::PlotSkenario
            && state.play_session_mode == PlaySessionMode::LocalPreview;
        if ui
            .selectable_label(on_plot, egui::RichText::new("Plot Skenario").strong())
            .clicked()
            && !on_plot
        {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            if state.reload_plot_on_return
                && let Some(sk) = state.active_skenario.clone()
            {
                open_plot_skenario(state, playback, api, &sk);
            } else {
                state.screen = AppScreen::PlotSkenario;
            }
            ws.send(WsOutCommand::Subscribe {
                skenario_id: None,
                client_mode: WsClientMode::Idle,
            });
        }
    }

    // Tactical / Simulasi (all roles)
    let on_tactical = state.screen == AppScreen::Tactical;
    if ui
        .selectable_label(
            on_tactical,
            egui::RichText::new("Tactical")
                .strong()
                .color(if on_tactical {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(110, 231, 183)
                }),
        )
        .clicked()
        && !on_tactical
    {
        playback.stop_play_test();
        state.play_session_mode = PlaySessionMode::LocalPreview;
        state.skenario_table.close_modal();
        state.restore_active_skenario_map_state();
        state.screen = AppScreen::Tactical;
        ws.send(WsOutCommand::Subscribe {
            skenario_id: None,
            client_mode: WsClientMode::Idle,
        });
        trigger_load_tactical_operasis(state, api);
    }

    // Layar Besar (all roles — read-only display synced via WS)
    let on_big = state.screen == AppScreen::LayarBesar;
    if ui
        .selectable_label(
            on_big,
            egui::RichText::new("Layar Besar")
                .strong()
                .color(if on_big {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(253, 224, 71)
                }),
        )
        .clicked()
        && !on_big
    {
        enter_layar_besar(state, playback, ws);
    }

    // Satkers CRUD (admin & admin_monitor)
    if state.can_manage_satkers() {
        let on_satker = state.screen == AppScreen::Satkers;
        if ui
            .selectable_label(on_satker, egui::RichText::new("Satkers").strong())
            .clicked()
            && !on_satker
        {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            state.screen = AppScreen::Satkers;
            ws.send(WsOutCommand::Subscribe {
                skenario_id: None,
                client_mode: WsClientMode::Idle,
            });
            trigger_load_satkers(state, api);
        }
    }

    // Users CRUD (admin, admin_monitor, admin_satker)
    if state.can_manage_users() {
        let on_users = state.screen == AppScreen::Users;
        if ui
            .selectable_label(on_users, egui::RichText::new("Users").strong())
            .clicked()
            && !on_users
        {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            state.screen = AppScreen::Users;
            ws.send(WsOutCommand::Subscribe {
                skenario_id: None,
                client_mode: WsClientMode::Idle,
            });
            trigger_load_users(state, api);
            trigger_load_satkers(state, api);
        }
    }

    if state.is_super_admin() {
        let on_assets = state.screen == AppScreen::Assets;
        if ui
            .selectable_label(on_assets, egui::RichText::new("Assets 3D").strong())
            .clicked()
            && !on_assets
        {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            state.screen = AppScreen::Assets;
            ws.send(WsOutCommand::Subscribe {
                skenario_id: None,
                client_mode: WsClientMode::Idle,
            });
        }
    }

    // Local Graphics & Performance Settings (saved per-device, available to all roles right beside Assets 3D)
    {
        let on_graphics = state.screen == AppScreen::Graphics;
        if ui
            .selectable_label(
                on_graphics,
                egui::RichText::new("Grafis")
                    .strong()
                    .color(if on_graphics {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(125, 211, 252)
                    }),
            )
            .on_hover_text("Pengaturan Grafis & Performa 3D lokal (LOD0/LOD2, Skinning, Culling)")
            .clicked()
            && !on_graphics
        {
            playback.stop_play_test();
            state.play_session_mode = PlaySessionMode::LocalPreview;
            state.skenario_table.close_modal();
            state.restore_active_skenario_map_state();
            state.screen = AppScreen::Graphics;
            ws.send(WsOutCommand::Subscribe {
                skenario_id: None,
                client_mode: WsClientMode::Idle,
            });
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
                draw_app_brand_icon(ui, 20.0);
                ui.label(
                    egui::RichText::new("TWG-RUST 3D")
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.separator();

                draw_screen_nav_tabs(ui, state, playback, api, ws);

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
                        ui.colored_label(egui::Color32::from_rgb(250, 204, 21), "OFFLINE PREVIEW");
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

                    if let Some(user) = state.current_user.clone() {
                        let on_prof = state.screen == AppScreen::Profile;
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!(
                                        "{} [{}]  •  Profil",
                                        user.username,
                                        user.role.label()
                                    ))
                                    .size(11.5)
                                    .color(if on_prof {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(203, 213, 225)
                                    }),
                                )
                                .fill(if on_prof {
                                    egui::Color32::from_rgb(14, 116, 144)
                                } else {
                                    egui::Color32::from_rgb(24, 34, 53)
                                }),
                            )
                            .on_hover_text("Buka halaman Profil & Ganti Password")
                            .clicked()
                            && !on_prof
                        {
                            open_profile_screen(state, playback, ws);
                        }
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
