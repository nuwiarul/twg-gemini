use bevy_egui::egui;

use crate::{
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand},
        client::{ApiClientBridge, ApiCommand},
    },
    state::AppState,
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
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.16).max(32.0));
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(17, 24, 39))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(28))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.set_min_width(360.0);

                        ui.label(
                            egui::RichText::new("TACTICAL WALL GAME 3D")
                                .size(22.0)
                                .strong()
                                .color(egui::Color32::from_rgb(56, 189, 248)),
                        );
                        ui.label(
                            egui::RichText::new("Full-Rust Desktop Edition • OSM & ArcGIS • glTF 3D")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.add_space(18.0);

                        ui.label(egui::RichText::new("Server URL").size(12.0).color(egui::Color32::LIGHT_GRAY));
                        ui.add(
                            egui::TextEdit::singleline(&mut state.server_url)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.0);

                        ui.label(egui::RichText::new("Username").size(12.0).color(egui::Color32::LIGHT_GRAY));
                        ui.add(
                            egui::TextEdit::singleline(&mut state.username)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.0);

                        ui.label(egui::RichText::new("Password").size(12.0).color(egui::Color32::LIGHT_GRAY));
                        let pw_resp = ui.add(
                            egui::TextEdit::singleline(&mut state.password)
                                .password(true)
                                .hint_text("admin123")
                                .desired_width(f32::INFINITY),
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
                            state.loading = true;
                            state.login_error = None;
                            api.send(ApiCommand::Login {
                                server_url: state.server_url.clone(),
                                username: state.username.clone(),
                                password: state.password.clone(),
                                device_id: state.device_id.clone(),
                            });
                        }

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(8.0);

                        if ui
                            .add_sized(
                                [ui.available_width(), 30.0],
                                egui::Button::new("Offline 3D Map Preview (OSM / ArcGIS + glTF)"),
                            )
                            .clicked()
                        {
                            state.enter_offline_demo();
                        }

                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Seeded credentials: admin / admin123 or operator / operator123")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );

                        ui.add_space(12.0);
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(24, 33, 50))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Sinkronisasi Offline Model 3D")
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
                                                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                                                    server_url: state.server_url.clone(),
                                                    local_root: state.assets_root.clone(),
                                                });
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
                                    ui.add(
                                        egui::ProgressBar::new(frac)
                                            .desired_width(f32::INFINITY)
                                            .text(if state.sync_status.checking {
                                                "Mengecek manifest server...".to_string()
                                            } else {
                                                format!(
                                                    "{}/{} file ({:.1}/{:.1} MB)",
                                                    state.sync_status.completed_files,
                                                    state.sync_status.total_files,
                                                    state.sync_status.downloaded_bytes as f64
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
                                        egui::RichText::new(&state.sync_status.summary_text)
                                            .size(11.0),
                                    );
                                }
                            });
                    });
            });
        });
}
