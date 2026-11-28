use bevy_egui::egui;
use twg_shared::{geo::GeoAnchor, ws::WsClientMode};

use crate::{
    map::camera::TacticalCamera,
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::ApiClientBridge,
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::play_rts::PlayRtsState,
    state::{AppScreen, AppState, PlaySessionMode, PlaybackState},
    ui::{
        app_icon::draw_app_brand_icon,
        nav_bar::{open_profile_screen, trigger_load_operasis},
    },
};

/// Clean fullscreen read-only **Layar Besar** UI overlay.
/// All map camera & entity controls are completely locked here — it only responds to remote
/// WebSocket commands sent by authorized operators from Tactical -> Play Skenario.
#[allow(clippy::too_many_arguments)]
pub fn draw_layar_besar_overlay(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    play_rts: &mut PlayRtsState,
    recorder: &mut crate::scene::video_recorder::VideoRecorderState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    let is_dedicated_role = state.is_big_screen_only_role();
    let mut exit_big_screen = !is_dedicated_role && ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut do_refresh = ctx.input(|i| i.key_pressed(egui::Key::F5));
    let mut go_graphics = false;
    let mut go_profile = false;
    let mut do_logout = false;

    // Subtle top-left status & exit bar (non-intrusive for wall projection / big LED screen)
    egui::Area::new(egui::Id::new("layar_besar_topbar"))
        .order(egui::Order::Foreground)
        .fixed_pos(clip_rect.left_top() + egui::vec2(14.0, 14.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(10, 16, 28, 215))
                .stroke(egui::Stroke::new(
                    1.2,
                    if state.ws_connected {
                        egui::Color32::from_rgba_unmultiplied(16, 185, 129, 200)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(239, 68, 68, 200)
                    },
                ))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    super::audio_anim::draw_output_toggle(ui, state);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        draw_app_brand_icon(ui, 18.0);

                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(113, 63, 18))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("LAYAR BESAR")
                                        .size(10.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(254, 240, 138)),
                                );
                            });

                        if let Some(sk) = &state.active_skenario {
                            let op_prefix = sk
                                .operasi_name
                                .as_deref()
                                .map(|n| format!("{n} / "))
                                .unwrap_or_default();
                            ui.label(
                                egui::RichText::new(format!("{op_prefix}{}", sk.name))
                                    .size(12.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Menunggu Skenario...")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        }

                        let (ws_txt, ws_col) = if state.ws_connected {
                            ("LIVE", egui::Color32::from_rgb(74, 222, 128))
                        } else {
                            ("RECONNECTING...", egui::Color32::from_rgb(248, 113, 113))
                        };
                        ui.label(
                            egui::RichText::new(format!("• {ws_txt}"))
                                .size(10.5)
                                .strong()
                                .color(ws_col),
                        );

                        super::video_recorder_ui::draw_compact_recorder_button(ui, recorder);

                        let refresh_tip = if state.active_skenario.is_some() {
                            "Refresh Layar Besar ke kondisi skenario kosong & sambung ulang koneksi live (F5)"
                        } else {
                            "Refresh Layar Besar, sinkronkan aset 3D & tarik skenario aktif terbaru dari server (F5)"
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Refresh")
                                        .size(10.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(14, 116, 144)),
                            )
                            .on_hover_text(refresh_tip)
                            .clicked()
                        {
                            do_refresh = true;
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Grafis")
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(125, 211, 252)),
                                )
                                .fill(egui::Color32::from_rgb(20, 34, 54)),
                            )
                            .on_hover_text("Pengaturan Grafis & Performa 3D perangkat ini")
                            .clicked()
                        {
                            go_graphics = true;
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Profil")
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(186, 230, 253)),
                                )
                                .fill(egui::Color32::from_rgb(24, 34, 53)),
                            )
                            .on_hover_text("Buka halaman Profil & Ganti Password")
                            .clicked()
                        {
                            go_profile = true;
                        }

                        if is_dedicated_role {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Logout")
                                            .size(10.5)
                                            .color(egui::Color32::from_rgb(254, 202, 202)),
                                    )
                                    .fill(egui::Color32::from_rgb(127, 29, 29)),
                                )
                                .clicked()
                            {
                                do_logout = true;
                            }
                        } else if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Keluar (Esc)")
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(226, 232, 240)),
                                )
                                .fill(egui::Color32::from_rgb(30, 41, 59)),
                            )
                            .clicked()
                        {
                            exit_big_screen = true;
                        }
                    });
                });
        });

    // Centered standby banner when no scenario has been loaded onto Layar Besar yet
    if state.active_skenario.is_none() {
        egui::Area::new(egui::Id::new("layar_besar_standby_card"))
            .order(egui::Order::Middle)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(12, 19, 34, 232))
                    .stroke(egui::Stroke::new(
                        1.5,
                        egui::Color32::from_rgb(56, 189, 248),
                    ))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::same(22))
                    .show(ui, |ui| {
                        ui.set_max_width(480.0);
                        ui.vertical_centered(|ui| {
                            if let Some(org) = crate::branding::org_name() {
                                ui.label(
                                    egui::RichText::new(org)
                                        .size(13.5)
                                        .strong()
                                        .color(egui::Color32::from_rgb(56, 189, 248)),
                                );
                                if let Some(sub) = crate::branding::org_subtitle() {
                                    ui.label(
                                        egui::RichText::new(sub)
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                }
                                ui.add_space(6.0);
                            }
                            ui.label(
                                egui::RichText::new("LAYAR BESAR — MODE TAMPILAN TERKUNCI")
                                    .size(15.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(254, 240, 138)),
                            );
                            ui.add_space(8.0);
                            let scope_info = if let Some(u) = &state.current_user {
                                if u.role.is_satker_role() {
                                    format!(
                                        "Scope Satker: {} — menunggu Admin / Admin Satker memuat skenario dari halaman Tactical.",
                                        u.satker_name.as_deref().unwrap_or("Satker Anda")
                                    )
                                } else {
                                    "Scope Pusat / Global — menunggu Admin memuat skenario dari halaman Tactical.".to_string()
                                }
                            } else {
                                "Menunggu koneksi & skenario dari halaman Tactical.".to_string()
                            };
                            ui.label(
                                egui::RichText::new(scope_info)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(203, 213, 225)),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(
                                    "Peta pada layar ini tidak dapat digerakkan secara lokal (hanya menerima kendali realtime).",
                                )
                                .size(11.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                    });
            });
    }

    if do_logout {
        playback.stop_play_test();
        play_rts.reset_all();
        ws.send(WsOutCommand::Disconnect);
        state.play_session_mode = PlaySessionMode::LocalPreview;
        state.screen = AppScreen::Login;
        state.offline_preview = false;
        state.auth_token = None;
        state.refresh_token = None;
        state.current_user = None;
        state.active_skenario = None;
        state.ws_connected = false;
    } else if do_refresh {
        let had_active_skenario = state.active_skenario.is_some();
        playback.stop_play_test();
        play_rts.reset_all();
        state.play_session_mode = PlaySessionMode::BigScreen;
        state.active_skenario = None;
        state.markers.clear();
        state.buildings.clear();
        state.lines.clear();
        state.text_markers.clear();
        state.particles.clear();
        state.alurs.clear();
        state.anims.clear();
        state.geo_anchor = GeoAnchor::default();
        *tcam = TacticalCamera::default();
        state.big_screen.loaded_by_actor = None;
        state.big_screen.pending_skenario_id = None;
        state.big_screen.pending_camera_payload = None;
        state.big_screen.skip_next_cached_state = had_active_skenario;
        state.big_screen.last_event_text = Some(if had_active_skenario {
            "Layar Besar di-refresh ke skenario kosong — siap menerima skenario baru.".into()
        } else {
            "Menyegarkan koneksi Layar Besar & sinkronisasi aset 3D...".into()
        });
        state.ws_connected = false;

        sync_bridge.send(AssetSyncCommand::SyncWithServer {
            server_url: state.server_url.clone(),
            local_root: state.assets_root.clone(),
        });

        if let Some(token) = state.auth_token.clone() {
            ws.send(WsOutCommand::Connect {
                server_url: state.server_url.clone(),
                token,
                device_id: state.device_id.clone(),
                skenario_id: None,
                client_mode: WsClientMode::BigScreen,
            });
        } else {
            ws.send(WsOutCommand::Subscribe {
                skenario_id: None,
                client_mode: WsClientMode::BigScreen,
            });
        }
    } else if go_graphics {
        playback.stop_play_test();
        play_rts.reset_all();
        state.play_session_mode = PlaySessionMode::LocalPreview;
        state.screen = AppScreen::Graphics;
        ws.send(WsOutCommand::Subscribe {
            skenario_id: None,
            client_mode: WsClientMode::Idle,
        });
    } else if go_profile {
        play_rts.reset_all();
        open_profile_screen(state, playback, ws);
    } else if exit_big_screen {
        playback.stop_play_test();
        play_rts.reset_all();
        state.play_session_mode = PlaySessionMode::LocalPreview;
        state.screen = AppScreen::Operasis;
        ws.send(WsOutCommand::Subscribe {
            skenario_id: None,
            client_mode: WsClientMode::Idle,
        });
        trigger_load_operasis(state, api);
    }
}
