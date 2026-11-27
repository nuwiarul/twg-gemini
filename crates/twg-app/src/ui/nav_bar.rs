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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountMenuAction {
    None,
    OpenProfile,
    Logout,
}

pub fn perform_logout(state: &mut AppState, playback: &mut PlaybackState, ws: &WsClientBridge) {
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

fn paint_user_avatar_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    fg: egui::Color32,
    status_dot: Option<egui::Color32>,
    bg_ring: egui::Color32,
) {
    // Subtle circular avatar badge background
    painter.circle_filled(center, radius, egui::Color32::from_rgb(30, 41, 59));
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(71, 85, 105)),
    );
    // Head
    let head_r = radius * 0.30;
    let head_c = egui::pos2(center.x, center.y - radius * 0.22);
    painter.circle_filled(head_c, head_r, fg);
    // Shoulders (rounded capsule clipped nicely inside circle)
    let sh_w = radius * 1.05;
    let sh_h = radius * 0.50;
    let sh_rect = egui::Rect::from_center_size(
        egui::pos2(center.x, center.y + radius * 0.38),
        egui::vec2(sh_w, sh_h),
    );
    painter.rect_filled(sh_rect, radius * 0.30, fg);

    // Optional live WS status dot at bottom-right of avatar
    if let Some(dot_col) = status_dot {
        let dot_c = egui::pos2(center.x + radius * 0.68, center.y + radius * 0.65);
        painter.circle_filled(dot_c, radius * 0.36, bg_ring);
        painter.circle_filled(dot_c, radius * 0.24, dot_col);
    }
}

fn paint_chevron_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    half_w: f32,
    half_h: f32,
    up: bool,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.5, color);
    let (y_edge, y_tip) = if up {
        (center.y + half_h, center.y - half_h)
    } else {
        (center.y - half_h, center.y + half_h)
    };
    let left = egui::pos2(center.x - half_w, y_edge);
    let tip = egui::pos2(center.x, y_tip);
    let right = egui::pos2(center.x + half_w, y_edge);
    painter.line_segment([left, tip], stroke);
    painter.line_segment([tip, right], stroke);
}

fn paint_logout_icon(painter: &egui::Painter, center: egui::Pos2, r: f32, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.4, color);
    // Door bracket on the left: [
    let x_left = center.x - r * 0.75;
    let x_mid = center.x - r * 0.05;
    let y_top = center.y - r * 0.75;
    let y_bot = center.y + r * 0.75;
    painter.line_segment([egui::pos2(x_mid, y_top), egui::pos2(x_left, y_top)], stroke);
    painter.line_segment([egui::pos2(x_left, y_top), egui::pos2(x_left, y_bot)], stroke);
    painter.line_segment([egui::pos2(x_left, y_bot), egui::pos2(x_mid, y_bot)], stroke);
    // Arrow pointing right ->
    let x_a0 = center.x - r * 0.20;
    let x_a1 = center.x + r * 0.80;
    painter.line_segment(
        [egui::pos2(x_a0, center.y), egui::pos2(x_a1, center.y)],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(x_a1 - r * 0.38, center.y - r * 0.38),
            egui::pos2(x_a1, center.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(x_a1 - r * 0.38, center.y + r * 0.38),
            egui::pos2(x_a1, center.y),
        ],
        stroke,
    );
}

/// Compact account dropdown button with custom vector avatar, WS status dot, username, and chevron.
/// Keeps the right side of every top bar compact (~115px) with zero font-glyph tofu boxes.
pub fn draw_account_menu_button(
    ui: &mut egui::Ui,
    state: &AppState,
    id_salt: &str,
) -> AccountMenuAction {
    let mut action = AccountMenuAction::None;
    let popup_id = ui.make_persistent_id(format!("{id_salt}_acct_popup"));
    let open_key = ui.make_persistent_id(format!("{id_salt}_acct_open"));
    let mut is_open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(open_key))
        .unwrap_or(false);

    let (dot_col, ws_status_text) = if state.offline_preview {
        (
            egui::Color32::from_rgb(250, 204, 21),
            "OFFLINE PREVIEW".to_string(),
        )
    } else if state.ws_connected {
        (
            egui::Color32::from_rgb(74, 222, 128),
            format!("WS LIVE ({} online)", state.online_operators.len()),
        )
    } else {
        (
            egui::Color32::from_rgb(248, 113, 113),
            "WS OFFLINE".to_string(),
        )
    };

    let (uname, role_lbl) = state
        .current_user
        .as_ref()
        .map(|u| (u.username.as_str(), u.role.label()))
        .unwrap_or(("Akun", "Offline"));
    let short_name: String = if uname.chars().count() > 14 {
        format!("{}...", uname.chars().take(12).collect::<String>())
    } else {
        uname.to_string()
    };

    let on_prof = state.screen == AppScreen::Profile;
    let font_id = egui::FontId::proportional(11.5);
    let text_galley = ui.painter().layout_no_wrap(
        short_name.clone(),
        font_id,
        egui::Color32::from_rgb(226, 232, 240),
    );
    let btn_h = 24.0_f32;
    let avatar_d = 16.0_f32;
    let pad_l = 6.0_f32;
    let gap_l = 6.0_f32;
    let gap_r = 7.0_f32;
    let chev_w = 8.0_f32;
    let pad_r = 8.0_f32;
    let btn_w = (pad_l + avatar_d + gap_l + text_galley.size().x + gap_r + chev_w + pad_r).ceil();

    let (rect, btn_resp) = ui.allocate_exact_size(egui::vec2(btn_w, btn_h), egui::Sense::click());
    let hovered = btn_resp.hovered();
    let btn_fill = if is_open || on_prof {
        egui::Color32::from_rgb(14, 116, 144)
    } else if hovered {
        egui::Color32::from_rgb(32, 46, 70)
    } else {
        egui::Color32::from_rgb(24, 34, 53)
    };
    let btn_stroke = egui::Stroke::new(
        1.0,
        if is_open || hovered {
            egui::Color32::from_rgb(56, 189, 248)
        } else {
            egui::Color32::from_rgb(51, 65, 85)
        },
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            egui::CornerRadius::same(5),
            btn_fill,
            btn_stroke,
            egui::StrokeKind::Inside,
        );
        // Custom vector user avatar + WS status dot
        let av_center = egui::pos2(rect.left() + pad_l + avatar_d * 0.5, rect.center().y);
        paint_user_avatar_icon(
            painter,
            av_center,
            avatar_d * 0.5,
            egui::Color32::from_rgb(203, 213, 225),
            Some(dot_col),
            btn_fill,
        );
        // Username text
        let text_pos = egui::pos2(
            rect.left() + pad_l + avatar_d + gap_l,
            rect.center().y - text_galley.size().y * 0.5,
        );
        painter.galley(text_pos, text_galley, egui::Color32::WHITE);
        // Custom vector chevron
        let chev_center = egui::pos2(rect.right() - pad_r - chev_w * 0.5, rect.center().y);
        paint_chevron_icon(
            painter,
            chev_center,
            3.4,
            2.0,
            is_open,
            egui::Color32::from_rgb(186, 230, 253),
        );
    }

    let btn_resp = btn_resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{uname} [{role_lbl}] • {ws_status_text}"));

    if btn_resp.clicked() {
        is_open = !is_open;
    }

    if is_open {
        let popup_w = 220.0_f32;
        let vp = ui.ctx().viewport_rect();
        let x = (btn_resp.rect.right() - popup_w)
            .max(vp.left() + 8.0)
            .min(vp.right() - popup_w - 8.0);
        let y = btn_resp.rect.bottom() + 4.0;

        let area_resp = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 42))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 74, 102)))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(10))
                    .shadow(egui::Shadow {
                        offset: [0, 4],
                        blur: 12,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(120),
                    })
                    .show(ui, |ui| {
                        ui.set_width(popup_w - 20.0);
                        ui.horizontal(|ui| {
                            let (av_rect, _) =
                                ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                            paint_user_avatar_icon(
                                ui.painter(),
                                av_rect.center(),
                                13.0,
                                egui::Color32::from_rgb(186, 230, 253),
                                Some(dot_col),
                                egui::Color32::from_rgb(15, 23, 42),
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.label(
                                    egui::RichText::new(uname)
                                        .size(13.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new(role_lbl)
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            });
                        });

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let (dot_rect, _) =
                                ui.allocate_exact_size(egui::vec2(10.0, 12.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 3.6, dot_col);
                            ui.label(egui::RichText::new(&ws_status_text).size(10.5).color(dot_col));
                        });

                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(5.0);

                        if state.current_user.is_some() {
                            let w = ui.available_width();
                            let (prect, presp) =
                                ui.allocate_exact_size(egui::vec2(w, 28.0), egui::Sense::click());
                            let pfill = if on_prof {
                                egui::Color32::from_rgb(14, 116, 144)
                            } else if presp.hovered() {
                                egui::Color32::from_rgb(38, 52, 74)
                            } else {
                                egui::Color32::from_rgb(26, 36, 54)
                            };
                            ui.painter().rect_filled(prect, 5.0, pfill);
                            paint_user_avatar_icon(
                                ui.painter(),
                                egui::pos2(prect.left() + 14.0, prect.center().y),
                                7.0,
                                egui::Color32::from_rgb(186, 230, 253),
                                None,
                                pfill,
                            );
                            let pgalley = ui.painter().layout_no_wrap(
                                "Profil & Ganti Password".into(),
                                egui::FontId::proportional(11.5),
                                egui::Color32::WHITE,
                            );
                            ui.painter().galley(
                                egui::pos2(
                                    prect.left() + 28.0,
                                    prect.center().y - pgalley.size().y * 0.5,
                                ),
                                pgalley,
                                egui::Color32::WHITE,
                            );
                            if presp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                action = AccountMenuAction::OpenProfile;
                                is_open = false;
                            }
                            ui.add_space(4.0);
                        }

                        {
                            let w = ui.available_width();
                            let (lrect, lresp) =
                                ui.allocate_exact_size(egui::vec2(w, 28.0), egui::Sense::click());
                            let lfill = if lresp.hovered() {
                                egui::Color32::from_rgb(153, 27, 27)
                            } else {
                                egui::Color32::from_rgb(127, 29, 29)
                            };
                            ui.painter().rect_filled(lrect, 5.0, lfill);
                            paint_logout_icon(
                                ui.painter(),
                                egui::pos2(lrect.left() + 14.0, lrect.center().y),
                                6.5,
                                egui::Color32::from_rgb(254, 202, 202),
                            );
                            let lgalley = ui.painter().layout_no_wrap(
                                "Logout".into(),
                                egui::FontId::proportional(11.5),
                                egui::Color32::from_rgb(254, 226, 226),
                            );
                            ui.painter().galley(
                                egui::pos2(
                                    lrect.left() + 28.0,
                                    lrect.center().y - lgalley.size().y * 0.5,
                                ),
                                lgalley,
                                egui::Color32::from_rgb(254, 226, 226),
                            );
                            if lresp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                action = AccountMenuAction::Logout;
                                is_open = false;
                            }
                        }
                    });
            });

        // Close popup when pressing Escape or clicking outside both the button and popup card
        let esc_pressed = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        let clicked_outside = ui.ctx().input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| {
                    !btn_resp.rect.contains(p) && !area_resp.response.rect.contains(p)
                })
        });
        if esc_pressed || clicked_outside {
            is_open = false;
        }
    }

    ui.ctx().data_mut(|d| d.insert_temp(open_key, is_open));
    action
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
            let total_w = ui.available_width();
            let right_reserve = 148.0_f32.min((total_w * 0.32).max(96.0));
            let left_max_w = (total_w - right_reserve - 8.0).max(160.0);

            ui.horizontal_top(|ui| {
                ui.scope(|ui| {
                    ui.set_min_width(left_max_w);
                    ui.set_max_width(left_max_w);
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
                                        state.sync_status.completed_files,
                                        state.sync_status.total_files
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
                    });
                });

                ui.allocate_ui_with_layout(
                    egui::vec2(right_reserve, 24.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match draw_account_menu_button(ui, state, "global_nav") {
                        AccountMenuAction::OpenProfile => {
                            if state.screen != AppScreen::Profile {
                                open_profile_screen(state, playback, ws);
                            }
                        }
                        AccountMenuAction::Logout => {
                            perform_logout(state, playback, ws);
                        }
                        AccountMenuAction::None => {}
                    },
                );
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
