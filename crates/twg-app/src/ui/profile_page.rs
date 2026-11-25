use bevy_egui::egui;
use twg_shared::{
    api::{ChangePasswordRequest, ProfileUpdateRequest},
    models::UserRole,
};

use crate::{
    net::{
        asset_sync::AssetSyncBridge,
        client::{ApiClientBridge, ApiCommand},
        ws_client::WsClientBridge,
    },
    scene::markers::current_auth,
    state::{AppState, PlaybackState},
    ui::{nav_bar::draw_global_top_bar, password_input::draw_password_input},
};

fn profile_role_badge(role: UserRole) -> (egui::Color32, egui::Color32) {
    match role {
        UserRole::Admin => (
            egui::Color32::from_rgb(127, 29, 29),
            egui::Color32::from_rgb(254, 202, 202),
        ),
        UserRole::AdminMonitor => (
            egui::Color32::from_rgb(88, 28, 135),
            egui::Color32::from_rgb(233, 213, 255),
        ),
        UserRole::AdminSatker => (
            egui::Color32::from_rgb(120, 53, 15),
            egui::Color32::from_rgb(253, 230, 138),
        ),
        UserRole::User => (
            egui::Color32::from_rgb(12, 74, 110),
            egui::Color32::from_rgb(186, 230, 253),
        ),
        UserRole::UserSatker => (
            egui::Color32::from_rgb(6, 78, 59),
            egui::Color32::from_rgb(167, 243, 208),
        ),
        UserRole::LayarBesar => (
            egui::Color32::from_rgb(113, 63, 18),
            egui::Color32::from_rgb(254, 240, 138),
        ),
        UserRole::LayarBesarSatker => (
            egui::Color32::from_rgb(17, 94, 89),
            egui::Color32::from_rgb(153, 246, 228),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_profile_screen(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);

    let Some(user) = state.current_user.clone() else {
        return;
    };

    let mut submit_profile = false;
    let mut submit_password = false;

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(10, 15, 24))
                .inner_margin(egui::Margin::symmetric(28, 22)),
        )
        .show(viewport_ui, |ui| {
            ui.label(
                egui::RichText::new("Profil & Keamanan Akun")
                    .size(22.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
            ui.label(
                egui::RichText::new(
                    "Kelola informasi profil Anda serta perbarui password login secara mandiri.",
                )
                .size(12.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(16.0);

            egui::ScrollArea::vertical()
                .id_salt("profile_page_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let avail_w = ui.available_width();
                    let two_col = avail_w >= 860.0;
                    let gap = 18.0_f32;
                    let card_w = if two_col {
                        ((avail_w - gap) * 0.5).min(520.0)
                    } else {
                        avail_w.min(560.0)
                    };

                    let draw_account_card = |ui: &mut egui::Ui,
                                             state: &mut AppState,
                                             submit_profile: &mut bool| {
                        let inner_w = (card_w - 36.0).max(260.0);
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(15, 23, 42))
                            .stroke(egui::Stroke::new(
                                1.2,
                                egui::Color32::from_rgb(38, 55, 82),
                            ))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::same(18))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(inner_w);
                                    ui.label(
                                        egui::RichText::new("Informasi Akun")
                                            .size(16.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    );
                                    ui.add_space(10.0);

                                    // Account metadata summary box
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(20, 30, 48))
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            egui::Color32::from_rgb(44, 60, 86),
                                        ))
                                        .corner_radius(6.0)
                                        .inner_margin(egui::Margin::same(12))
                                        .show(ui, |ui| {
                                            ui.set_width(inner_w - 24.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Username:")
                                                        .size(12.0)
                                                        .color(egui::Color32::from_rgb(
                                                            148, 163, 184,
                                                        )),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "@{}",
                                                        user.username
                                                    ))
                                                    .size(12.5)
                                                    .strong()
                                                    .monospace()
                                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                                                );
                                            });
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Role:")
                                                        .size(12.0)
                                                        .color(egui::Color32::from_rgb(
                                                            148, 163, 184,
                                                        )),
                                                );
                                                let (bg, fg) = profile_role_badge(user.role);
                                                egui::Frame::new()
                                                    .fill(bg)
                                                    .corner_radius(4.0)
                                                    .inner_margin(egui::Margin::symmetric(8, 2))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            egui::RichText::new(user.role.label())
                                                                .size(10.5)
                                                                .strong()
                                                                .color(fg),
                                                        );
                                                    });
                                            });
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Satker:")
                                                        .size(12.0)
                                                        .color(egui::Color32::from_rgb(
                                                            148, 163, 184,
                                                        )),
                                                );
                                                ui.label(
                                                    egui::RichText::new(
                                                        user.satker_name
                                                            .as_deref()
                                                            .unwrap_or("Umum / Pusat"),
                                                    )
                                                    .size(12.0)
                                                    .color(egui::Color32::WHITE),
                                                );
                                            });
                                        });

                                    ui.add_space(12.0);
                                    ui.label(
                                        egui::RichText::new("Nama Lengkap *")
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut state.profile_form.name)
                                            .hint_text("Nama lengkap Anda")
                                            .desired_width(inner_w),
                                    );

                                    if let Some(err) = &state.profile_form.profile_error {
                                        ui.add_space(8.0);
                                        ui.colored_label(
                                            egui::Color32::from_rgb(248, 113, 113),
                                            err,
                                        );
                                    }
                                    if let Some(ok) = &state.profile_form.profile_success {
                                        ui.add_space(8.0);
                                        ui.colored_label(
                                            egui::Color32::from_rgb(74, 222, 128),
                                            ok,
                                        );
                                    }

                                    ui.add_space(14.0);
                                    let save_btn = egui::Button::new(
                                        egui::RichText::new(
                                            if state.profile_form.profile_submitting {
                                                "Menyimpan..."
                                            } else {
                                                "Simpan Perubahan Profil"
                                            },
                                        )
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(14, 116, 144))
                                    .min_size(egui::vec2(170.0, 32.0));
                                    if ui
                                        .add_enabled(
                                            !state.profile_form.profile_submitting,
                                            save_btn,
                                        )
                                        .clicked()
                                    {
                                        *submit_profile = true;
                                    }
                                });
                            });
                    };

                    let draw_password_card = |ui: &mut egui::Ui,
                                              state: &mut AppState,
                                              submit_password: &mut bool| {
                        let inner_w = (card_w - 36.0).max(260.0);
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(15, 23, 42))
                            .stroke(egui::Stroke::new(
                                1.2,
                                egui::Color32::from_rgb(38, 55, 82),
                            ))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::same(18))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(inner_w);
                                    ui.label(
                                        egui::RichText::new("Ganti Password")
                                            .size(16.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "Masukkan password lama Anda, lalu masukkan password baru minimal 6 karakter.",
                                        )
                                        .size(11.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    ui.add_space(12.0);

                                    ui.label(
                                        egui::RichText::new("Password Lama *")
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    draw_password_input(
                                        ui,
                                        &mut state.profile_form.old_password,
                                        &mut state.profile_form.show_old_password,
                                        "Masukkan password saat ini",
                                    );
                                    ui.add_space(10.0);

                                    ui.label(
                                        egui::RichText::new("Password Baru * (minimal 6 karakter)")
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    draw_password_input(
                                        ui,
                                        &mut state.profile_form.new_password,
                                        &mut state.profile_form.show_new_password,
                                        "Masukkan password baru",
                                    );
                                    ui.add_space(10.0);

                                    ui.label(
                                        egui::RichText::new("Konfirmasi Password Baru *")
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    draw_password_input(
                                        ui,
                                        &mut state.profile_form.confirm_password,
                                        &mut state.profile_form.show_confirm_password,
                                        "Ulangi password baru",
                                    );

                                    if let Some(err) = &state.profile_form.password_error {
                                        ui.add_space(8.0);
                                        ui.colored_label(
                                            egui::Color32::from_rgb(248, 113, 113),
                                            err,
                                        );
                                    }
                                    if let Some(ok) = &state.profile_form.password_success {
                                        ui.add_space(8.0);
                                        ui.colored_label(
                                            egui::Color32::from_rgb(74, 222, 128),
                                            ok,
                                        );
                                    }

                                    ui.add_space(14.0);
                                    let pw_btn = egui::Button::new(
                                        egui::RichText::new(
                                            if state.profile_form.password_submitting {
                                                "Memperbarui..."
                                            } else {
                                                "Ubah Password"
                                            },
                                        )
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(5, 150, 105))
                                    .min_size(egui::vec2(150.0, 32.0));
                                    if ui
                                        .add_enabled(
                                            !state.profile_form.password_submitting,
                                            pw_btn,
                                        )
                                        .clicked()
                                    {
                                        *submit_password = true;
                                    }
                                });
                            });
                    };

                    if two_col {
                        egui::Grid::new("profile_two_col_grid")
                            .num_columns(2)
                            .spacing([gap, gap])
                            .show(ui, |ui| {
                                draw_account_card(ui, state, &mut submit_profile);
                                draw_password_card(ui, state, &mut submit_password);
                                ui.end_row();
                            });
                    } else {
                        draw_account_card(ui, state, &mut submit_profile);
                        ui.add_space(gap);
                        draw_password_card(ui, state, &mut submit_password);
                    }
                });
        });

    if submit_profile {
        let name = state.profile_form.name.trim().to_string();
        state.profile_form.profile_success = None;
        if name.is_empty() {
            state.profile_form.profile_error = Some("Nama lengkap tidak boleh kosong.".into());
        } else if let Some(auth) = current_auth(state) {
            state.profile_form.profile_submitting = true;
            state.profile_form.profile_error = None;
            api.send(ApiCommand::UpdateProfile {
                auth,
                req: ProfileUpdateRequest {
                    name,
                    profile_img: None,
                },
            });
        }
    }

    if submit_password {
        let old_pw = state.profile_form.old_password.clone();
        let new_pw = state.profile_form.new_password.clone();
        let conf_pw = state.profile_form.confirm_password.clone();
        state.profile_form.password_success = None;

        if old_pw.is_empty() {
            state.profile_form.password_error = Some("Password lama wajib diisi.".into());
        } else if new_pw.len() < 6 {
            state.profile_form.password_error = Some("Password baru minimal 6 karakter.".into());
        } else if new_pw != conf_pw {
            state.profile_form.password_error =
                Some("Konfirmasi password baru tidak sama.".into());
        } else if let Some(auth) = current_auth(state) {
            state.profile_form.password_submitting = true;
            state.profile_form.password_error = None;
            api.send(ApiCommand::ChangePassword {
                auth,
                req: ChangePasswordRequest {
                    old_password: old_pw,
                    new_password: new_pw,
                },
            });
        }
    }
}
