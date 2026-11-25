use bevy_egui::egui;

use crate::{
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::{ApiClientBridge, ApiCommand},
    },
    settings::{
        ServerProtocol, reset_local_server_settings, save_local_server_settings,
    },
    state::AppState,
    ui::{app_icon::draw_app_brand_icon, password_input::draw_password_input},
};

pub fn draw_login_screen(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_rgb(10, 15, 24)))
        .show(viewport_ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("login_page_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        let top_pad = if state.show_server_settings {
                            (ui.available_height() * 0.08).max(20.0)
                        } else {
                            (ui.available_height() * 0.16).max(32.0)
                        };
                        ui.add_space(top_pad);

                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(17, 24, 39))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::same(28))
                            .show(ui, |ui| {
                                ui.set_max_width(420.0);
                                ui.set_min_width(360.0);

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    draw_app_brand_icon(ui, 44.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("TACTICAL WALL GAME 3D")
                                                .size(20.5)
                                                .strong()
                                                .color(egui::Color32::from_rgb(56, 189, 248)),
                                        );
                                        ui.label(
                                            egui::RichText::new(
                                                "Full-Rust Desktop Edition • OSM & ArcGIS • glTF 3D",
                                            )
                                            .size(11.5)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                        );
                                    });
                                });
                                ui.add_space(18.0);

                                ui.label(
                                    egui::RichText::new("Username")
                                        .size(12.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.username)
                                        .hint_text("Masukkan username")
                                        .desired_width(f32::INFINITY),
                                );
                                ui.add_space(10.0);

                                ui.label(
                                    egui::RichText::new("Password")
                                        .size(12.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                let pw_resp = draw_password_input(
                                    ui,
                                    &mut state.password,
                                    &mut state.login_show_password,
                                    "Masukkan password",
                                );
                                let submit_enter = pw_resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                                if let Some(err) = &state.login_error {
                                    ui.add_space(10.0);
                                    ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                                }

                                ui.add_space(16.0);
                                let login_btn = ui.add_sized(
                                    [ui.available_width(), 34.0],
                                    egui::Button::new(
                                        egui::RichText::new(if state.loading {
                                            "Signing in..."
                                        } else {
                                            "Sign In to Tactical Server"
                                        })
                                        .strong(),
                                    )
                                    .fill(egui::Color32::from_rgb(14, 116, 144)),
                                );

                                if (login_btn.clicked() || submit_enter) && !state.loading {
                                    state.server_settings.normalize_pasted_host();
                                    match state.server_settings.build_url() {
                                        Ok(url) => {
                                            state.server_url = url.clone();
                                            state.loading = true;
                                            state.login_error = None;
                                            api.send(ApiCommand::Login {
                                                server_url: url,
                                                username: state.username.clone(),
                                                password: state.password.clone(),
                                                device_id: state.device_id.clone(),
                                            });
                                        }
                                        Err(err) => {
                                            state.show_server_settings = true;
                                            state.login_error = Some(err);
                                        }
                                    }
                                }

                                ui.add_space(14.0);
                                egui::Frame::new()
                                    .fill(egui::Color32::from_rgb(24, 33, 50))
                                    .corner_radius(6.0)
                                    .inner_margin(egui::Margin::same(10))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(
                                                    "Sinkronisasi Offline Model 3D",
                                                )
                                                .size(11.5)
                                                .strong()
                                                .color(egui::Color32::from_rgb(56, 189, 248)),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .add_enabled(
                                                            !state.sync_status.is_syncing,
                                                            egui::Button::new("Sync Ulang").small(),
                                                        )
                                                        .clicked()
                                                    {
                                                        if let Ok(url) =
                                                            state.server_settings.build_url()
                                                        {
                                                            state.server_url = url;
                                                        }
                                                        sync_bridge.send(
                                                            AssetSyncCommand::SyncWithServer {
                                                                server_url: state
                                                                    .server_url
                                                                    .clone(),
                                                                local_root: state
                                                                    .assets_root
                                                                    .clone(),
                                                            },
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                        if state.sync_status.is_syncing {
                                            let frac = if state.sync_status.total_files > 0 {
                                                state.sync_status.completed_files as f32
                                                    / state.sync_status.total_files as f32
                                            } else {
                                                0.0
                                            };
                                            ui.add_space(4.0);
                                            let bar_w = ui.available_width().max(120.0);
                                            ui.add(
                                                egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                                                    .desired_width(bar_w)
                                                    .text(if state.sync_status.checking {
                                                        "Mengecek manifest server...".to_string()
                                                    } else {
                                                        format!(
                                                            "{}/{} file ({:.1}/{:.1} MB)",
                                                            state.sync_status.completed_files,
                                                            state.sync_status.total_files,
                                                            state.sync_status.downloaded_bytes
                                                                as f64
                                                                / (1024.0 * 1024.0),
                                                            state.sync_status.total_bytes as f64
                                                                / (1024.0 * 1024.0),
                                                        )
                                                    }),
                                            );
                                        } else {
                                            let col = if state.sync_status.offline_mode {
                                                egui::Color32::from_rgb(250, 204, 21)
                                            } else {
                                                egui::Color32::from_rgb(74, 222, 128)
                                            };
                                            ui.colored_label(
                                                col,
                                                egui::RichText::new(
                                                    &state.sync_status.summary_text,
                                                )
                                                .size(11.0),
                                            );
                                        }
                                    });

                                // Subtle, unobtrusive trigger to reveal local Server Host & Port settings (hidden by default)
                                ui.add_space(12.0);
                                draw_hidden_server_settings_section(ui, state, api, sync_bridge);
                            });

                        ui.add_space(24.0);
                    });
                });
        });
}

fn draw_hidden_server_settings_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    let toggle_text = if state.show_server_settings {
        "Sembunyikan pengaturan server"
    } else {
        "Pengaturan server"
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (icon_rect, icon_resp) =
            ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::click());
        let text_resp = ui.add(
            egui::Label::new(
                egui::RichText::new(toggle_text)
                    .size(10.5)
                    .color(egui::Color32::from_rgb(100, 116, 139)),
            )
            .sense(egui::Sense::click()),
        );
        let hovered = icon_resp.hovered() || text_resp.hovered();
        let chevron_col = if hovered {
            egui::Color32::from_rgb(148, 163, 184)
        } else {
            egui::Color32::from_rgb(100, 116, 139)
        };
        let c = icon_rect.center();
        let stroke = egui::Stroke::new(1.25, chevron_col);
        if state.show_server_settings {
            // Downward chevron (expanded)
            ui.painter().line_segment(
                [egui::pos2(c.x - 3.0, c.y - 1.5), egui::pos2(c.x, c.y + 1.8)],
                stroke,
            );
            ui.painter().line_segment(
                [egui::pos2(c.x, c.y + 1.8), egui::pos2(c.x + 3.0, c.y - 1.5)],
                stroke,
            );
        } else {
            // Rightward chevron (collapsed)
            ui.painter().line_segment(
                [egui::pos2(c.x - 1.5, c.y - 3.0), egui::pos2(c.x + 1.8, c.y)],
                stroke,
            );
            ui.painter().line_segment(
                [egui::pos2(c.x + 1.8, c.y), egui::pos2(c.x - 1.5, c.y + 3.0)],
                stroke,
            );
        }

        let hover_tip = format!(
            "Server aktif: {} — klik untuk mengatur protokol, host & port",
            state.server_url
        );
        let clicked = icon_resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(&hover_tip)
            .clicked()
            || text_resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(&hover_tip)
                .clicked();
        if clicked {
            state.show_server_settings = !state.show_server_settings;
        }
    });

    if !state.show_server_settings {
        return;
    }

    ui.add_space(8.0);
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(13, 20, 34))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Protocol selector (HTTP / HTTPS)
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Protokol")
                        .size(11.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(6.0);
                for (proto, label) in [
                    (ServerProtocol::Http, "HTTP"),
                    (ServerProtocol::Https, "HTTPS"),
                ] {
                    let selected = state.server_settings.protocol == proto;
                    let fill = if selected {
                        if proto == ServerProtocol::Https {
                            egui::Color32::from_rgb(6, 95, 70)
                        } else {
                            egui::Color32::from_rgb(14, 116, 144)
                        }
                    } else {
                        egui::Color32::from_rgb(30, 41, 59)
                    };
                    let fg = if selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(148, 163, 184)
                    };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(label).size(11.0).strong().color(fg))
                                .fill(fill)
                                .min_size(egui::vec2(58.0, 22.0)),
                        )
                        .clicked()
                    {
                        state.server_settings.protocol = proto;
                        state.server_settings_feedback = None;
                        if let Ok(url) = state.server_settings.build_url() {
                            state.server_url = url;
                        }
                    }
                }
            });

            ui.add_space(8.0);

            // Host & Port side-by-side
            let total_w = ui.available_width();
            let gap = 8.0_f32;
            let port_w = 88.0_f32;
            let host_w = (total_w - port_w - gap).max(140.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;

                ui.vertical(|ui| {
                    ui.set_width(host_w);
                    ui.label(
                        egui::RichText::new("Host / IP / Domain")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut state.server_settings.host)
                            .hint_text("127.0.0.1 / twg.domain.com")
                            .desired_width(host_w),
                    );
                    if resp.changed() {
                        state.server_settings.normalize_pasted_host();
                        state.server_settings_feedback = None;
                        if let Ok(url) = state.server_settings.build_url() {
                            state.server_url = url;
                        }
                    }
                });

                ui.vertical(|ui| {
                    ui.set_width(port_w);
                    ui.label(
                        egui::RichText::new("Port (opsional)")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut state.server_settings.port)
                            .hint_text(
                                state.server_settings.protocol.default_port().to_string(),
                            )
                            .desired_width(port_w),
                    );
                    if resp.changed() {
                        // Keep digits only for port field
                        state.server_settings.port.retain(|c| c.is_ascii_digit());
                        state.server_settings_feedback = None;
                        if let Ok(url) = state.server_settings.build_url() {
                            state.server_url = url;
                        }
                    }
                });
            });

            ui.add_space(6.0);
            match state.server_settings.build_url() {
                Ok(preview) => {
                    ui.label(
                        egui::RichText::new(format!("Alamat: {preview}"))
                            .size(10.5)
                            .monospace()
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                }
                Err(err) => {
                    ui.label(
                        egui::RichText::new(err)
                            .size(10.5)
                            .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                }
            }

            if state.testing_server_connection {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Menguji koneksi ke server...")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
            } else if let Some((ok, msg)) = &state.server_settings_feedback {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(msg).size(10.5).color(if *ok {
                        egui::Color32::from_rgb(74, 222, 128)
                    } else {
                        egui::Color32::from_rgb(248, 113, 113)
                    }),
                );
            }

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.spacing_mut().item_spacing.y = 6.0;
                if ui
                    .add_enabled(
                        !state.testing_server_connection,
                        egui::Button::new(
                            egui::RichText::new(if state.testing_server_connection {
                                "Menguji..."
                            } else {
                                "Test Koneksi"
                            })
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144)),
                    )
                    .clicked()
                {
                    state.server_settings.normalize_pasted_host();
                    match state.server_settings.build_url() {
                        Ok(url) => {
                            state.server_url = url.clone();
                            state.testing_server_connection = true;
                            state.server_settings_feedback = None;
                            api.send(ApiCommand::TestServerConnection { server_url: url });
                        }
                        Err(err) => {
                            state.server_settings_feedback = Some((false, err));
                        }
                    }
                }

                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Simpan Lokal")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105)),
                    )
                    .clicked()
                {
                    state.server_settings.normalize_pasted_host();
                    match state.server_settings.build_url() {
                        Ok(url) => match save_local_server_settings(&state.server_settings) {
                            Ok(_path) => {
                                state.server_url = url.clone();
                                state.server_settings_saved_locally = true;
                                state.login_error = None;
                                state.server_settings_feedback = Some((
                                    true,
                                    "Pengaturan server tersimpan di komputer ini.".into(),
                                ));
                                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                                    server_url: url,
                                    local_root: state.assets_root.clone(),
                                });
                            }
                            Err(err) => {
                                state.server_settings_feedback = Some((false, err));
                            }
                        },
                        Err(err) => {
                            state.server_settings_feedback = Some((false, err));
                        }
                    }
                }

                if state.server_settings_saved_locally
                    && ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Reset")
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(203, 213, 225)),
                            )
                            .fill(egui::Color32::from_rgb(30, 41, 59)),
                        )
                        .clicked()
                {
                    match reset_local_server_settings() {
                        Ok(def) => {
                            let url = def
                                .build_url()
                                .unwrap_or_else(|_| "http://127.0.0.1:8010".to_string());
                            state.server_settings = def;
                            state.server_url = url.clone();
                            state.server_settings_saved_locally = false;
                            state.server_settings_feedback = Some((
                                true,
                                "Pengaturan dikembalikan ke default.".into(),
                            ));
                            sync_bridge.send(AssetSyncCommand::SyncWithServer {
                                server_url: url,
                                local_root: state.assets_root.clone(),
                            });
                        }
                        Err(err) => {
                            state.server_settings_feedback = Some((false, err));
                        }
                    }
                }
            });
        });
}
