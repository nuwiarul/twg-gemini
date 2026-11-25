use bevy_egui::egui;
use twg_shared::{
    api::{CreateSatkerRequest, UpdateSatkerRequest},
    models::Satker,
};

use crate::{
    net::{
        asset_sync::AssetSyncBridge,
        client::{ApiClientBridge, ApiCommand},
        ws_client::WsClientBridge,
    },
    scene::markers::current_auth,
    state::{AppState, PlaybackState, SatkerModalMode},
    ui::{
        nav_bar::{draw_global_top_bar, draw_modal_backdrop, format_timestamp, trigger_load_satkers},
        table::{
            TABLE_COL_GAP, TABLE_PAD_LEFT, TABLE_PAD_RIGHT, TABLE_SCROLLBAR_RESERVE,
            draw_aligned_row, draw_header_label, table_content_width,
        },
    },
};

#[allow(clippy::too_many_arguments)]
pub fn draw_satker_screen(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);

    let can_manage = state.can_manage_satkers() && !state.offline_preview;

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
                        egui::RichText::new("Manajemen Satker")
                            .size(22.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Kelola daftar Satuan Kerja (Satker) untuk isolasi operasi, skenario, user, dan Layar Besar.",
                        )
                        .size(12.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if can_manage {
                        let add_btn = egui::Button::new(
                            egui::RichText::new("+ Tambah Satker")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .min_size(egui::vec2(138.0, 32.0));
                        if ui.add(add_btn).clicked() {
                            state.satker_table.open_create();
                        }
                    }

                    if ui
                        .add_enabled(
                            !state.satker_table.loading,
                            egui::Button::new("Refresh").min_size(egui::vec2(82.0, 32.0)),
                        )
                        .clicked()
                    {
                        trigger_load_satkers(state, api);
                    }
                });
            });

            ui.add_space(12.0);

            // Search bar
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
                            egui::TextEdit::singleline(&mut state.satker_table.search)
                                .hint_text("Cari kode atau nama satker...")
                                .desired_width(300.0),
                        );
                        if !state.satker_table.search.is_empty() && ui.button("Reset").clicked() {
                            state.satker_table.search.clear();
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.satker_table.loading {
                                ui.colored_label(
                                    egui::Color32::from_rgb(56, 189, 248),
                                    "Memuat daftar satker...",
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Total: {} satker",
                                        state.satker_table.satkers.len()
                                    ))
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }
                        });
                    });
                });

            if let Some(err) = state.satker_table.error.clone() {
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

            let query = state.satker_table.search.trim().to_lowercase();
            let filtered: Vec<Satker> = state
                .satker_table
                .satkers
                .iter()
                .filter(|s| {
                    query.is_empty()
                        || s.code.to_lowercase().contains(&query)
                        || s.name.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();

            let table_height = (ui.available_height() - 8.0).max(200.0);
            let table_outer_w = ui.available_width();
            let content_w = table_content_width(table_outer_w);

            // 6 columns: No | Kode | Nama Satker | Status | Diperbarui | Aksi
            let col_no = 42.0_f32;
            let col_code = 140.0_f32;
            let col_status = 100.0_f32;
            let col_updated = 136.0_f32;
            let col_actions = 152.0_f32;
            let total_gaps = TABLE_COL_GAP * 5.0;
            let col_name =
                (content_w - col_no - col_code - col_status - col_updated - col_actions - total_gaps)
                    .max(180.0);
            let col_widths = [
                col_no,
                col_code,
                col_name,
                col_status,
                col_updated,
                col_actions,
            ];

            let mut open_edit: Option<Satker> = None;
            let mut open_delete: Option<Satker> = None;

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
                                "satker_header_row",
                                &col_widths,
                                20.0,
                                |ui, col, _| match col {
                                    0 => draw_header_label(ui, "No"),
                                    1 => draw_header_label(ui, "Kode"),
                                    2 => draw_header_label(ui, "Nama Satker"),
                                    3 => draw_header_label(ui, "Status"),
                                    4 => draw_header_label(ui, "Diperbarui"),
                                    5 => draw_header_label(ui, "Aksi"),
                                    _ => {}
                                },
                            );
                        });

                    egui::ScrollArea::vertical()
                        .id_salt("satker_table_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if filtered.is_empty() {
                                ui.add_space(40.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new(if state.satker_table.loading {
                                            "Memuat data satker..."
                                        } else if !query.is_empty() {
                                            "Tidak ada satker yang cocok dengan pencarian."
                                        } else {
                                            "Belum ada data satker. Klik '+ Tambah Satker' untuk membuat baru."
                                        })
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                });
                                return;
                            }

                            for (idx, satker) in filtered.iter().enumerate() {
                                let row_bg = if idx % 2 == 0 {
                                    egui::Color32::from_rgb(15, 23, 42)
                                } else {
                                    egui::Color32::from_rgb(19, 28, 46)
                                };
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
                                            ("satker_row", satker.id),
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
                                                    ui.label(
                                                        egui::RichText::new(&satker.code)
                                                            .size(12.0)
                                                            .strong()
                                                            .monospace()
                                                            .color(egui::Color32::from_rgb(
                                                                125, 211, 252,
                                                            )),
                                                    );
                                                }
                                                2 => {
                                                    ui.label(
                                                        egui::RichText::new(&satker.name)
                                                            .size(12.5)
                                                            .strong()
                                                            .color(egui::Color32::WHITE),
                                                    );
                                                }
                                                3 => {
                                                    let (txt, bg, fg) = if satker.is_active {
                                                        (
                                                            "Aktif",
                                                            egui::Color32::from_rgb(6, 78, 59),
                                                            egui::Color32::from_rgb(110, 231, 183),
                                                        )
                                                    } else {
                                                        (
                                                            "Nonaktif",
                                                            egui::Color32::from_rgb(51, 65, 85),
                                                            egui::Color32::from_rgb(203, 213, 225),
                                                        )
                                                    };
                                                    egui::Frame::new()
                                                        .fill(bg)
                                                        .corner_radius(4.0)
                                                        .inner_margin(egui::Margin::symmetric(8, 2))
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                egui::RichText::new(txt)
                                                                    .size(10.5)
                                                                    .strong()
                                                                    .color(fg),
                                                            );
                                                        });
                                                }
                                                4 => {
                                                    ui.label(
                                                        egui::RichText::new(format_timestamp(
                                                            satker.updated_at,
                                                        ))
                                                        .size(11.5)
                                                        .color(egui::Color32::from_rgb(
                                                            148, 163, 184,
                                                        )),
                                                    );
                                                }
                                                5 if can_manage => {
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
                                                            open_edit = Some(satker.clone());
                                                        }
                                                        if ui
                                                            .add(
                                                                egui::Button::new("Hapus")
                                                                    .fill(egui::Color32::from_rgb(
                                                                        153, 27, 27,
                                                                    ))
                                                                    .min_size(egui::vec2(
                                                                        62.0, 24.0,
                                                                    )),
                                                            )
                                                            .clicked()
                                                        {
                                                            open_delete = Some(satker.clone());
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

            if let Some(s) = open_edit {
                state.satker_table.open_edit(&s);
            }
            if let Some(s) = open_delete {
                state.satker_table.open_delete(&s);
            }
        });

    draw_satker_modal(ctx, state, api);
}

fn draw_satker_modal(ctx: &egui::Context, state: &mut AppState, api: &ApiClientBridge) {
    let mode = state.satker_table.modal_mode.clone();
    if matches!(mode, SatkerModalMode::None) {
        return;
    }

    draw_modal_backdrop(ctx, "satker_modal_backdrop");

    match mode {
        SatkerModalMode::None => {}
        SatkerModalMode::Create | SatkerModalMode::Edit(_) => {
            let edit_id = match mode {
                SatkerModalMode::Edit(id) => Some(id),
                _ => None,
            };
            let title = if edit_id.is_some() {
                "Edit Satker"
            } else {
                "Tambah Satker Baru"
            };
            let mut close = false;
            let mut submit = false;

            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .fixed_size(egui::vec2(440.0, 0.0))
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(56, 189, 248)))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Kode Satker *")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.satker_table.form_code)
                            .hint_text("Contoh: POLDA-BALI, POLRES-DPS")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(10.0);

                    ui.label(
                        egui::RichText::new("Nama Satker *")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.satker_table.form_name)
                            .hint_text("Contoh: Polda Bali / Polresta Denpasar")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(10.0);

                    ui.checkbox(&mut state.satker_table.form_is_active, "Satker Aktif");

                    if let Some(err) = &state.satker_table.form_error {
                        ui.add_space(8.0);
                        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                    }

                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save_btn = egui::Button::new(
                            egui::RichText::new(if state.satker_table.submitting {
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
                            .add_enabled(!state.satker_table.submitting, save_btn)
                            .clicked()
                        {
                            submit = true;
                        }
                        if ui
                            .add_enabled(
                                !state.satker_table.submitting,
                                egui::Button::new("Batal").min_size(egui::vec2(80.0, 30.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });

            if close {
                state.satker_table.close_modal();
            } else if submit {
                let code = state.satker_table.form_code.trim().to_string();
                let name = state.satker_table.form_name.trim().to_string();
                if code.is_empty() || name.is_empty() {
                    state.satker_table.form_error =
                        Some("Kode dan nama satker wajib diisi.".into());
                } else if let Some(auth) = current_auth(state) {
                    state.satker_table.submitting = true;
                    state.satker_table.form_error = None;
                    let is_active = state.satker_table.form_is_active;
                    if let Some(id) = edit_id {
                        api.send(ApiCommand::UpdateSatker {
                            auth,
                            id,
                            req: UpdateSatkerRequest {
                                code,
                                name,
                                is_active,
                            },
                        });
                    } else {
                        api.send(ApiCommand::CreateSatker {
                            auth,
                            req: CreateSatkerRequest {
                                code,
                                name,
                                is_active,
                            },
                        });
                    }
                }
            }
        }
        SatkerModalMode::DeleteConfirm(satker) => {
            let mut close = false;
            let mut confirm = false;

            egui::Window::new("Hapus Satker")
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
                            "Yakin ingin menghapus satker '{}' ({})?",
                            satker.name, satker.code
                        ))
                        .size(13.0)
                        .color(egui::Color32::WHITE),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "Pastikan tidak ada user atau operasi yang masih terikat pada satker ini.",
                        )
                        .size(11.5)
                        .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                    if let Some(err) = &state.satker_table.form_error {
                        ui.add_space(8.0);
                        ui.colored_label(egui::Color32::from_rgb(252, 165, 165), err);
                    }
                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let del_btn = egui::Button::new(
                            egui::RichText::new(if state.satker_table.submitting {
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
                            .add_enabled(!state.satker_table.submitting, del_btn)
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui
                            .add_enabled(
                                !state.satker_table.submitting,
                                egui::Button::new("Batal").min_size(egui::vec2(80.0, 30.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });

            if close {
                state.satker_table.close_modal();
            } else if confirm
                && let Some(auth) = current_auth(state)
            {
                state.satker_table.submitting = true;
                state.satker_table.form_error = None;
                api.send(ApiCommand::DeleteSatker {
                    auth,
                    id: satker.id,
                });
            }
        }
    }
}
