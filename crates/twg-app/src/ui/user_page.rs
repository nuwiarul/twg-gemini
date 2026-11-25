use bevy_egui::egui;
use twg_shared::{
    api::{CreateUserRequest, UpdateUserRequest},
    models::{UserDto, UserRole},
};

use crate::{
    net::{
        asset_sync::AssetSyncBridge,
        client::{ApiClientBridge, ApiCommand},
        ws_client::WsClientBridge,
    },
    scene::markers::current_auth,
    state::{AppState, PlaybackState, UserModalMode},
    ui::{
        nav_bar::{
            draw_global_top_bar, draw_modal_backdrop, format_timestamp, trigger_load_satkers,
            trigger_load_users,
        },
        password_input::draw_password_input,
        table::{
            TABLE_COL_GAP, TABLE_PAD_LEFT, TABLE_PAD_RIGHT, TABLE_SCROLLBAR_RESERVE,
            draw_aligned_row, draw_header_label, table_content_width,
        },
    },
};

fn role_badge_colors(role: UserRole) -> (egui::Color32, egui::Color32) {
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
pub fn draw_user_screen(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);

    let can_manage = state.can_manage_users() && !state.offline_preview;
    let is_admin_satker = state
        .current_user
        .as_ref()
        .is_some_and(|u| u.role.is_admin_satker());
    let current_user_id = state.current_user.as_ref().map(|u| u.id);
    let my_satker_id = state.current_user.as_ref().and_then(|u| u.satker_id);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(10, 15, 24))
                .inner_margin(egui::Margin::symmetric(24, 18)),
        )
        .show(viewport_ui, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Manajemen Users")
                            .size(22.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    let subtitle = if is_admin_satker {
                        "Kelola akun Admin Satker, User Satker & Layar Besar Satker khusus untuk Satker Anda."
                    } else {
                        "Kelola akun pengguna, role (Admin, Admin Monitor, Admin Satker, User, User Satker, Layar Besar, Layar Besar Satker), dan Satker."
                    };
                    ui.label(
                        egui::RichText::new(subtitle)
                            .size(12.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if can_manage {
                        let add_btn = egui::Button::new(
                            egui::RichText::new("+ Tambah User")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .min_size(egui::vec2(132.0, 32.0));
                        if ui.add(add_btn).clicked() {
                            if state.satker_table.satkers.is_empty() {
                                trigger_load_satkers(state, api);
                            }
                            let (def_role, def_satker) = if is_admin_satker {
                                (UserRole::UserSatker, my_satker_id)
                            } else {
                                (UserRole::User, None)
                            };
                            state.user_table.open_create(def_role, def_satker);
                        }
                    }

                    if ui
                        .add_enabled(
                            !state.user_table.loading,
                            egui::Button::new("Refresh").min_size(egui::vec2(82.0, 32.0)),
                        )
                        .clicked()
                    {
                        trigger_load_users(state, api);
                        trigger_load_satkers(state, api);
                    }
                });
            });

            ui.add_space(12.0);

            // Search & Role Filter toolbar
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 39))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 49, 71)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Cari:")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut state.user_table.search)
                                .hint_text("Cari nama, username, atau satker...")
                                .desired_width(260.0),
                        );

                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Role:")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        let filter_txt = state
                            .user_table
                            .filter_role
                            .map(|r| r.label())
                            .unwrap_or("Semua Role");
                        let allowed_filters: &[UserRole] = if is_admin_satker {
                            UserRole::SATKER_ROLES
                        } else {
                            UserRole::ALL
                        };
                        egui::ComboBox::from_id_salt("user_role_filter")
                            .selected_text(filter_txt)
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.user_table.filter_role,
                                    None,
                                    "Semua Role",
                                );
                                for &r in allowed_filters {
                                    ui.selectable_value(
                                        &mut state.user_table.filter_role,
                                        Some(r),
                                        r.label(),
                                    );
                                }
                            });

                        if (!state.user_table.search.is_empty()
                            || state.user_table.filter_role.is_some())
                            && ui.button("Reset").clicked()
                        {
                            state.user_table.search.clear();
                            state.user_table.filter_role = None;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.user_table.loading {
                                ui.colored_label(
                                    egui::Color32::from_rgb(56, 189, 248),
                                    "Memuat daftar users...",
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Total: {} user",
                                        state.user_table.users.len()
                                    ))
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }
                        });
                    });
                });

            if let Some(err) = state.user_table.error.clone() {
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

            let query = state.user_table.search.trim().to_lowercase();
            let role_filter = state.user_table.filter_role;
            let filtered: Vec<UserDto> = state
                .user_table
                .users
                .iter()
                .filter(|u| {
                    if let Some(rf) = role_filter
                        && u.role != rf
                    {
                        return false;
                    }
                    if query.is_empty() {
                        return true;
                    }
                    u.name.to_lowercase().contains(&query)
                        || u.username.to_lowercase().contains(&query)
                        || u.satker_name
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                })
                .cloned()
                .collect();

            let table_height = (ui.available_height() - 8.0).max(200.0);
            let table_outer_w = ui.available_width();
            let content_w = table_content_width(table_outer_w);

            // 7 columns: No | Nama | Username | Role | Satker | Diperbarui | Aksi
            let col_no = 40.0_f32;
            let col_username = 148.0_f32;
            let col_role = 132.0_f32;
            let col_satker = 160.0_f32;
            let col_updated = 130.0_f32;
            let col_actions = 152.0_f32;
            let total_gaps = TABLE_COL_GAP * 6.0;
            let col_name = (content_w
                - col_no
                - col_username
                - col_role
                - col_satker
                - col_updated
                - col_actions
                - total_gaps)
                .max(140.0);
            let col_widths = [
                col_no,
                col_name,
                col_username,
                col_role,
                col_satker,
                col_updated,
                col_actions,
            ];

            let mut open_edit: Option<UserDto> = None;
            let mut open_delete: Option<UserDto> = None;

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
                                "user_header_row",
                                &col_widths,
                                20.0,
                                |ui, col, _| match col {
                                    0 => draw_header_label(ui, "No"),
                                    1 => draw_header_label(ui, "Nama Lengkap"),
                                    2 => draw_header_label(ui, "Username"),
                                    3 => draw_header_label(ui, "Role"),
                                    4 => draw_header_label(ui, "Satker"),
                                    5 => draw_header_label(ui, "Diperbarui"),
                                    6 => draw_header_label(ui, "Aksi"),
                                    _ => {}
                                },
                            );
                        });

                    egui::ScrollArea::vertical()
                        .id_salt("user_table_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if filtered.is_empty() {
                                ui.add_space(40.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new(if state.user_table.loading {
                                            "Memuat data pengguna..."
                                        } else {
                                            "Tidak ada user yang ditemukan."
                                        })
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                });
                                return;
                            }

                            for (idx, user) in filtered.iter().enumerate() {
                                let row_bg = if idx % 2 == 0 {
                                    egui::Color32::from_rgb(15, 23, 42)
                                } else {
                                    egui::Color32::from_rgb(19, 28, 46)
                                };
                                let is_self = current_user_id == Some(user.id);

                                egui::Frame::new()
                                    .fill(row_bg)
                                    .stroke(egui::Stroke::new(
                                        0.5,
                                        egui::Color32::from_rgb(30, 41, 59),
                                    ))
                                    .inner_margin(egui::Margin {
                                        left: TABLE_PAD_LEFT as i8,
                                        right: TABLE_PAD_RIGHT as i8,
                                        top: 8,
                                        bottom: 8,
                                    })
                                    .show(ui, |ui| {
                                        ui.set_min_width(content_w);
                                        draw_aligned_row(
                                            ui,
                                            ("user_row", user.id),
                                            &col_widths,
                                            26.0,
                                            |ui, col, _| match col {
                                                0 => {
                                                    ui.label(
                                                        egui::RichText::new(format!("{}", idx + 1))
                                                            .size(12.0)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                    );
                                                }
                                                1 => {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            egui::RichText::new(&user.name)
                                                                .size(12.5)
                                                                .strong()
                                                                .color(egui::Color32::WHITE),
                                                        );
                                                        if is_self {
                                                            ui.label(
                                                                egui::RichText::new("(Anda)")
                                                                    .size(10.5)
                                                                    .color(egui::Color32::from_rgb(
                                                                        56, 189, 248,
                                                                    )),
                                                            );
                                                        }
                                                    });
                                                }
                                                2 => {
                                                    ui.label(
                                                        egui::RichText::new(&user.username)
                                                            .size(12.0)
                                                            .monospace()
                                                            .color(egui::Color32::from_rgb(
                                                                186, 230, 253,
                                                            )),
                                                    );
                                                }
                                                3 => {
                                                    let (bg, fg) = role_badge_colors(user.role);
                                                    egui::Frame::new()
                                                        .fill(bg)
                                                        .corner_radius(4.0)
                                                        .inner_margin(egui::Margin::symmetric(8, 2))
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    user.role.label(),
                                                                )
                                                                .size(10.5)
                                                                .strong()
                                                                .color(fg),
                                                            );
                                                        });
                                                }
                                                4 => {
                                                    let s_txt = user
                                                        .satker_name
                                                        .as_deref()
                                                        .unwrap_or("Umum / Pusat");
                                                    ui.label(
                                                        egui::RichText::new(s_txt).size(11.5).color(
                                                            if user.satker_id.is_some() {
                                                                egui::Color32::from_rgb(
                                                                    226, 232, 240,
                                                                )
                                                            } else {
                                                                egui::Color32::from_rgb(
                                                                    100, 116, 139,
                                                                )
                                                            },
                                                        ),
                                                    );
                                                }
                                                5 => {
                                                    ui.label(
                                                        egui::RichText::new(format_timestamp(
                                                            user.updated_at,
                                                        ))
                                                        .size(11.5)
                                                        .color(egui::Color32::from_rgb(
                                                            148, 163, 184,
                                                        )),
                                                    );
                                                }
                                                6 if can_manage => {
                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 6.0;
                                                        if ui
                                                            .add(
                                                                egui::Button::new("Edit")
                                                                    .fill(egui::Color32::from_rgb(
                                                                        30, 58, 138,
                                                                    ))
                                                                    .min_size(egui::vec2(
                                                                        58.0, 24.0,
                                                                    )),
                                                            )
                                                            .clicked()
                                                        {
                                                            open_edit = Some(user.clone());
                                                        }
                                                        if ui
                                                            .add_enabled(
                                                                !is_self,
                                                                egui::Button::new("Hapus")
                                                                    .fill(egui::Color32::from_rgb(
                                                                        153, 27, 27,
                                                                    ))
                                                                    .min_size(egui::vec2(
                                                                        62.0, 24.0,
                                                                    )),
                                                            )
                                                            .on_disabled_hover_text(
                                                                "Tidak bisa menghapus akun sendiri yang sedang aktif.",
                                                            )
                                                            .clicked()
                                                        {
                                                            open_delete = Some(user.clone());
                                                        }
                                                    });
                                                }
                                                _ => {}
                                            },
                                        );
                                    });
                            }
                        });
                });

            if let Some(u) = open_edit {
                if state.satker_table.satkers.is_empty() {
                    trigger_load_satkers(state, api);
                }
                state.user_table.open_edit(&u);
            }
            if let Some(u) = open_delete {
                state.user_table.open_delete(&u);
            }
        });

    draw_user_modal(ctx, state, api);
}

fn draw_user_modal(ctx: &egui::Context, state: &mut AppState, api: &ApiClientBridge) {
    let mode = state.user_table.modal_mode.clone();
    if matches!(mode, UserModalMode::None) {
        return;
    }

    draw_modal_backdrop(ctx, "user_modal_backdrop");

    let is_admin_satker = state
        .current_user
        .as_ref()
        .is_some_and(|u| u.role.is_admin_satker());
    let my_satker_id = state.current_user.as_ref().and_then(|u| u.satker_id);
    let my_satker_name = state
        .current_user
        .as_ref()
        .and_then(|u| u.satker_name.clone())
        .unwrap_or_else(|| "Satker Anda".to_string());

    match mode {
        UserModalMode::None => {}
        UserModalMode::Create | UserModalMode::Edit(_) => {
            let edit_id = match mode {
                UserModalMode::Edit(id) => Some(id),
                _ => None,
            };
            let title = if edit_id.is_some() {
                "Edit User"
            } else {
                "Tambah User Baru"
            };
            let mut close = false;
            let mut submit = false;

            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .fixed_size(egui::vec2(470.0, 0.0))
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(56, 189, 248)))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Nama Lengkap *")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.user_table.form_name)
                            .hint_text("Contoh: Kompol Budi Santoso")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(9.0);

                    ui.label(
                        egui::RichText::new(if edit_id.is_some() {
                            "Username (tidak dapat diubah)"
                        } else {
                            "Username *"
                        })
                        .size(12.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add_enabled(
                        edit_id.is_none(),
                        egui::TextEdit::singleline(&mut state.user_table.form_username)
                            .hint_text("Contoh: budi_polres")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(9.0);

                    ui.label(
                        egui::RichText::new(if edit_id.is_some() {
                            "Password Baru (kosongkan jika tidak ingin mengubah)"
                        } else {
                            "Password * (minimal 6 karakter)"
                        })
                        .size(12.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    draw_password_input(
                        ui,
                        &mut state.user_table.form_password,
                        &mut state.user_table.form_show_password,
                        if edit_id.is_some() {
                            "Biarkan kosong untuk tetap pakai password lama"
                        } else {
                            "Minimal 6 karakter"
                        },
                    );
                    ui.add_space(9.0);

                    ui.label(
                        egui::RichText::new("Role *")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    let allowed_roles: &[UserRole] = if is_admin_satker {
                        UserRole::SATKER_ROLES
                    } else {
                        UserRole::ALL
                    };
                    if is_admin_satker && !state.user_table.form_role.is_satker_role() {
                        state.user_table.form_role = UserRole::UserSatker;
                    }
                    egui::ComboBox::from_id_salt("user_modal_role_combo")
                        .selected_text(state.user_table.form_role.label())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for &r in allowed_roles {
                                ui.selectable_value(
                                    &mut state.user_table.form_role,
                                    r,
                                    r.label(),
                                );
                            }
                        });

                    // Concise role capability summary
                    let role_desc = match state.user_table.form_role {
                        UserRole::Admin => {
                            "Akses penuh: semua CRUD, upload assets/kategori, serta kendali penuh Layar Besar."
                        }
                        UserRole::AdminMonitor => {
                            "Akses penuh seperti Admin, tetapi kamera & load skenario TIDAK mempengaruhi Layar Besar."
                        }
                        UserRole::AdminSatker => {
                            "Kelola Operasi, Skenario, User & Layar Besar khusus untuk Satkernya (tanpa CRUD Satker/Assets)."
                        }
                        UserRole::User => {
                            "Hanya edit Plot Skenario & ikut Play Tactical pada operasi umum (tanpa CRUD Operasi/Skenario/User/Assets)."
                        }
                        UserRole::UserSatker => {
                            "Seperti User, namun khusus untuk operasi & skenario milik Satkernya."
                        }
                        UserRole::LayarBesar => {
                            "Akun khusus Layar Besar (Pusat/Global): hanya menampilkan peta Layar Besar (read-only) yang dikendalikan Admin."
                        }
                        UserRole::LayarBesarSatker => {
                            "Akun khusus Layar Besar Satker: hanya menampilkan peta Layar Besar (read-only) khusus untuk Satkernya."
                        }
                    };
                    ui.label(
                        egui::RichText::new(role_desc)
                            .size(10.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );

                    ui.add_space(9.0);
                    if state.user_table.form_role.is_satker_role() {
                        ui.label(
                            egui::RichText::new("Satker *")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(203, 213, 225)),
                        );
                        if is_admin_satker {
                            state.user_table.form_satker_id = my_satker_id;
                            ui.label(
                                egui::RichText::new(format!(
                                    "{my_satker_name} (terkunci ke satker Anda)"
                                ))
                                .size(12.0)
                                .strong()
                                .color(egui::Color32::from_rgb(125, 211, 252)),
                            );
                        } else {
                            let satkers = state.satker_table.satkers.clone();
                            let selected_label = state
                                .user_table
                                .form_satker_id
                                .and_then(|sid| satkers.iter().find(|s| s.id == sid))
                                .map(|s| format!("{} ({})", s.name, s.code))
                                .unwrap_or_else(|| "-- Pilih Satker --".to_string());
                            egui::ComboBox::from_id_salt("user_modal_satker_combo")
                                .selected_text(selected_label)
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    for s in &satkers {
                                        ui.selectable_value(
                                            &mut state.user_table.form_satker_id,
                                            Some(s.id),
                                            format!("{} ({})", s.name, s.code),
                                        );
                                    }
                                });
                        }
                    } else {
                        state.user_table.form_satker_id = None;
                    }

                    if let Some(err) = &state.user_table.form_error {
                        ui.add_space(8.0);
                        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                    }

                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save_btn = egui::Button::new(
                            egui::RichText::new(if state.user_table.submitting {
                                "Menyimpan..."
                            } else {
                                "Simpan"
                            })
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(104.0, 30.0));
                        if ui
                            .add_enabled(!state.user_table.submitting, save_btn)
                            .clicked()
                        {
                            submit = true;
                        }
                        if ui
                            .add_enabled(
                                !state.user_table.submitting,
                                egui::Button::new("Batal").min_size(egui::vec2(80.0, 30.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });

            if close {
                state.user_table.close_modal();
            } else if submit {
                let name = state.user_table.form_name.trim().to_string();
                let username = state.user_table.form_username.trim().to_string();
                let password = state.user_table.form_password.clone();
                let role = state.user_table.form_role;
                let satker_id = if role.is_satker_role() {
                    if is_admin_satker {
                        my_satker_id
                    } else {
                        state.user_table.form_satker_id
                    }
                } else {
                    None
                };

                if name.is_empty() {
                    state.user_table.form_error = Some("Nama lengkap wajib diisi.".into());
                } else if edit_id.is_none() && username.is_empty() {
                    state.user_table.form_error = Some("Username wajib diisi.".into());
                } else if edit_id.is_none() && password.len() < 6 {
                    state.user_table.form_error = Some("Password minimal 6 karakter.".into());
                } else if edit_id.is_some()
                    && !password.trim().is_empty()
                    && password.trim().len() < 6
                {
                    state.user_table.form_error =
                        Some("Password baru minimal 6 karakter.".into());
                } else if role.is_satker_role() && satker_id.is_none() {
                    state.user_table.form_error =
                        Some("Satker wajib dipilih untuk role Admin Satker / User Satker.".into());
                } else if let Some(auth) = current_auth(state) {
                    state.user_table.submitting = true;
                    state.user_table.form_error = None;
                    if let Some(id) = edit_id {
                        let pw_opt = if password.trim().is_empty() {
                            None
                        } else {
                            Some(password.trim().to_string())
                        };
                        api.send(ApiCommand::UpdateUser {
                            auth,
                            id,
                            req: UpdateUserRequest {
                                name,
                                role,
                                satker_id,
                                password: pw_opt,
                                profile_img: None,
                                units: serde_json::Value::Null,
                            },
                        });
                    } else {
                        api.send(ApiCommand::CreateUser {
                            auth,
                            req: CreateUserRequest {
                                name,
                                username,
                                password,
                                role,
                                satker_id,
                                profile_img: None,
                                units: serde_json::json!([]),
                            },
                        });
                    }
                }
            }
        }
        UserModalMode::DeleteConfirm(user) => {
            let mut close = false;
            let mut confirm = false;

            egui::Window::new("Hapus User")
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .fixed_size(egui::vec2(400.0, 0.0))
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(185, 28, 28)))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Yakin ingin menghapus akun '{}' (@{}) [{}]?",
                            user.name,
                            user.username,
                            user.role.label()
                        ))
                        .size(13.0)
                        .color(egui::Color32::WHITE),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Tindakan ini tidak dapat dibatalkan.")
                            .size(11.5)
                            .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                    if let Some(err) = &state.user_table.form_error {
                        ui.add_space(8.0);
                        ui.colored_label(egui::Color32::from_rgb(252, 165, 165), err);
                    }
                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let del_btn = egui::Button::new(
                            egui::RichText::new(if state.user_table.submitting {
                                "Menghapus..."
                            } else {
                                "Ya, Hapus"
                            })
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(185, 28, 28))
                        .min_size(egui::vec2(102.0, 30.0));
                        if ui
                            .add_enabled(!state.user_table.submitting, del_btn)
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui
                            .add_enabled(
                                !state.user_table.submitting,
                                egui::Button::new("Batal").min_size(egui::vec2(80.0, 30.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });

            if close {
                state.user_table.close_modal();
            } else if confirm
                && let Some(auth) = current_auth(state)
            {
                state.user_table.submitting = true;
                state.user_table.form_error = None;
                api.send(ApiCommand::DeleteUser { auth, id: user.id });
            }
        }
    }
}
