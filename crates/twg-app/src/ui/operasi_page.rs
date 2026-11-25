use bevy_egui::egui;
use twg_shared::{
    api::{CreateOperasiRequest, UpdateOperasiRequest},
    models::Operasi,
};
use uuid::Uuid;

use crate::{
    net::{
        asset_sync::AssetSyncBridge,
        client::{ApiClientBridge, ApiCommand},
        ws_client::WsClientBridge,
    },
    scene::markers::current_auth,
    state::{AppState, OperasiModalMode, PlaybackState},
    ui::{
        nav_bar::{
            draw_global_top_bar, draw_modal_backdrop, format_timestamp,
            navigate_to_operasi_skenarios, trigger_load_operasis,
        },
        pagination::draw_pagination_bar,
        table::{
            TABLE_COL_GAP, TABLE_PAD_LEFT, TABLE_PAD_RIGHT, TABLE_SCROLLBAR_RESERVE,
            draw_aligned_row, draw_header_label, table_content_width,
        },
    },
};

#[allow(clippy::too_many_arguments)]
pub fn draw_operasi_screen(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);

    let can_write = state
        .current_user
        .as_ref()
        .is_some_and(|u| u.role.can_write_admin());

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(10, 15, 24))
                .inner_margin(egui::Margin::symmetric(24, 18)),
        )
        .show(viewport_ui, |ui| {
            // Header row
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Manajemen Operasi")
                            .size(22.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Pilih operasi untuk melihat detail atau masuk ke halaman skenario taktis.",
                        )
                        .size(12.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if can_write && !state.offline_preview {
                        let add_btn = egui::Button::new(
                            egui::RichText::new("+ Tambah Operasi")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .min_size(egui::vec2(142.0, 32.0));
                        if ui.add(add_btn).clicked() {
                            let default_satker =
                                state.current_user.as_ref().and_then(|u| u.satker_id);
                            state.operasi_table.open_create(default_satker);
                        }
                    }

                    if ui
                        .add_enabled(
                            !state.operasi_table.loading,
                            egui::Button::new("Refresh").min_size(egui::vec2(82.0, 32.0)),
                        )
                        .clicked()
                    {
                        trigger_load_operasis(state, api);
                    }
                });
            });

            ui.add_space(12.0);

            // Search & filter toolbar
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
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut state.operasi_table.search)
                                .hint_text("Cari nama operasi, deskripsi, atau satker...")
                                .desired_width(320.0),
                        );
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Cari").clicked() || enter {
                            state.operasi_table.applied_search =
                                state.operasi_table.search.trim().to_string();
                            state.operasi_table.page = 1;
                            trigger_load_operasis(state, api);
                        }
                        if !state.operasi_table.applied_search.is_empty()
                            && ui.button("Reset").clicked()
                        {
                            state.operasi_table.search.clear();
                            state.operasi_table.applied_search.clear();
                            state.operasi_table.page = 1;
                            trigger_load_operasis(state, api);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.operasi_table.loading {
                                ui.colored_label(
                                    egui::Color32::from_rgb(56, 189, 248),
                                    "Memuat data dari server...",
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Total: {} operasi (server-side pagination)",
                                        state.operasi_table.total
                                    ))
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }
                        });
                    });
                });

            if let Some(err) = state.operasi_table.error.clone() {
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

            // Full-width Table Container
            let pagination_height = 48.0;
            let table_height = (ui.available_height() - pagination_height - 10.0).max(180.0);
            let table_outer_w = ui.available_width();
            let content_w = table_content_width(table_outer_w);

            // 7 columns: No | Nama Operasi | Deskripsi | Satker | Skenario | Diperbarui | Aksi
            let col_no = 42.0_f32;
            let col_satker = 132.0_f32;
            let col_count = 96.0_f32;
            let col_updated = 128.0_f32;
            let col_actions = if can_write && !state.offline_preview {
                276.0_f32
            } else {
                164.0_f32
            };
            let total_gaps = TABLE_COL_GAP * 6.0;
            let flex_w = (content_w
                - col_no
                - col_satker
                - col_count
                - col_updated
                - col_actions
                - total_gaps)
                .max(180.0);
            let col_name = (flex_w * 0.42).max(100.0);
            let col_desc = (flex_w - col_name).max(80.0);
            let col_widths = [
                col_no,
                col_name,
                col_desc,
                col_satker,
                col_count,
                col_updated,
                col_actions,
            ];

            let mut open_skenarios_for: Option<Operasi> = None;
            let mut open_detail_for: Option<Operasi> = None;
            let mut open_edit_for: Option<Operasi> = None;
            let mut open_delete_for: Option<Operasi> = None;

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

                    // Full-width Table Header (exact same column widths as rows)
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
                                "operasi_header_row",
                                &col_widths,
                                20.0,
                                |ui, col, _| match col {
                                    0 => draw_header_label(ui, "No"),
                                    1 => draw_header_label(ui, "Nama Operasi"),
                                    2 => draw_header_label(ui, "Deskripsi"),
                                    3 => draw_header_label(ui, "Satker"),
                                    4 => draw_header_label(ui, "Skenario"),
                                    5 => draw_header_label(ui, "Diperbarui"),
                                    6 => draw_header_label(ui, "Aksi"),
                                    _ => {}
                                },
                            );
                        });

                    // Crisp divider line below header
                    let (sep_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 1.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        sep_rect,
                        0.0,
                        egui::Color32::from_rgb(45, 60, 86),
                    );

                    // Scrollable Table Body
                    egui::ScrollArea::vertical()
                        .id_salt("operasi_table_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;

                            if state.operasis.is_empty() {
                                ui.add_space(48.0);
                                ui.vertical_centered(|ui| {
                                    let msg = if state.operasi_table.loading {
                                        "Sedang memuat daftar operasi..."
                                    } else if !state.operasi_table.applied_search.is_empty() {
                                        "Tidak ada operasi yang cocok dengan kata kunci pencarian."
                                    } else {
                                        "Belum ada data operasi. Klik '+ Tambah Operasi' untuk membuat baru."
                                    };
                                    ui.label(
                                        egui::RichText::new(msg)
                                            .size(13.5)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                });
                                return;
                            }

                            let base_index = (state.operasi_table.page.max(1) - 1)
                                * state.operasi_table.per_page.max(1);
                            let items = state.operasis.clone();

                            for (idx, op) in items.iter().enumerate() {
                                let row_no = base_index + idx as i64 + 1;
                                let bg = if idx % 2 == 0 {
                                    egui::Color32::from_rgb(15, 23, 42)
                                } else {
                                    egui::Color32::from_rgb(19, 29, 49)
                                };

                                egui::Frame::new()
                                    .fill(bg)
                                    .inner_margin(egui::Margin {
                                        left: TABLE_PAD_LEFT as i8,
                                        right: TABLE_PAD_RIGHT as i8,
                                        top: 10,
                                        bottom: 10,
                                    })
                                    .show(ui, |ui| {
                                        ui.set_min_width(content_w);
                                        draw_aligned_row(
                                            ui,
                                            op.id,
                                            &col_widths,
                                            26.0,
                                            |ui, col, _| match col {
                                                0 => {
                                                    ui.label(
                                                        egui::RichText::new(format!("{row_no}"))
                                                            .size(12.0)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                    );
                                                }
                                                1 => {
                                                    let name_resp = ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(&op.name)
                                                                .size(13.0)
                                                                .strong()
                                                                .color(egui::Color32::WHITE),
                                                        )
                                                        .wrap()
                                                        .sense(egui::Sense::click()),
                                                    );
                                                    if name_resp
                                                        .on_hover_text(
                                                            "Klik untuk menuju halaman skenario",
                                                        )
                                                        .clicked()
                                                    {
                                                        open_skenarios_for = Some(op.clone());
                                                    }
                                                }
                                                2 => {
                                                    let desc_text = op
                                                        .description
                                                        .as_deref()
                                                        .map(str::trim)
                                                        .filter(|s| !s.is_empty())
                                                        .unwrap_or("—");
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(desc_text)
                                                                .size(12.0)
                                                                .color(egui::Color32::from_rgb(
                                                                    148, 163, 184,
                                                                )),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                3 => {
                                                    let satker_label = op
                                                        .satker_name
                                                        .as_deref()
                                                        .unwrap_or("Global / Umum");
                                                    egui::Frame::new()
                                                        .fill(egui::Color32::from_rgb(30, 41, 59))
                                                        .corner_radius(4.0)
                                                        .inner_margin(egui::Margin::symmetric(7, 3))
                                                        .show(ui, |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    egui::RichText::new(satker_label)
                                                                        .size(11.0)
                                                                        .color(egui::Color32::from_rgb(
                                                                            186, 230, 253,
                                                                        )),
                                                                )
                                                                .wrap(),
                                                            );
                                                        });
                                                }
                                                4 => {
                                                    egui::Frame::new()
                                                        .fill(egui::Color32::from_rgb(12, 74, 110))
                                                        .corner_radius(10.0)
                                                        .inner_margin(egui::Margin::symmetric(8, 3))
                                                        .show(ui, |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    egui::RichText::new(format!(
                                                                        "{} skenario",
                                                                        op.skenario_count
                                                                    ))
                                                                    .size(11.0)
                                                                    .strong()
                                                                    .color(egui::Color32::from_rgb(
                                                                        125, 211, 252,
                                                                    )),
                                                                )
                                                                .wrap(),
                                                            );
                                                        });
                                                }
                                                5 => {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(format_timestamp(
                                                                op.updated_at,
                                                            ))
                                                            .size(11.5)
                                                            .color(egui::Color32::from_rgb(
                                                                148, 163, 184,
                                                            )),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                                6 => {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.spacing_mut().item_spacing =
                                                            egui::vec2(6.0, 4.0);
                                                        let sk_btn = egui::Button::new(
                                                            egui::RichText::new("Skenario >")
                                                                .size(11.5)
                                                                .strong()
                                                                .color(egui::Color32::WHITE),
                                                        )
                                                        .fill(egui::Color32::from_rgb(14, 116, 144))
                                                        .min_size(egui::vec2(78.0, 24.0));
                                                        if ui.add(sk_btn).clicked() {
                                                            open_skenarios_for = Some(op.clone());
                                                        }

                                                        let det_btn = egui::Button::new(
                                                            egui::RichText::new("Detail")
                                                                .size(11.5)
                                                                .color(egui::Color32::from_rgb(
                                                                    226, 232, 240,
                                                                )),
                                                        )
                                                        .fill(egui::Color32::from_rgb(51, 65, 85))
                                                        .min_size(egui::vec2(50.0, 24.0));
                                                        if ui.add(det_btn).clicked() {
                                                            open_detail_for = Some(op.clone());
                                                        }

                                                        if can_write && !state.offline_preview {
                                                            let edit_btn = egui::Button::new(
                                                                egui::RichText::new("Edit")
                                                                    .size(11.5)
                                                                    .color(egui::Color32::from_rgb(
                                                                        254, 240, 138,
                                                                    )),
                                                            )
                                                            .fill(egui::Color32::from_rgb(113, 63, 18))
                                                            .min_size(egui::vec2(44.0, 24.0));
                                                            if ui.add(edit_btn).clicked() {
                                                                open_edit_for = Some(op.clone());
                                                            }

                                                            let can_delete_op =
                                                                op.skenario_count == 0;
                                                            let del_btn = egui::Button::new(
                                                                egui::RichText::new("Hapus")
                                                                    .size(11.5)
                                                                    .color(if can_delete_op {
                                                                        egui::Color32::from_rgb(
                                                                            254, 202, 202,
                                                                        )
                                                                    } else {
                                                                        egui::Color32::from_rgb(
                                                                            148, 163, 184,
                                                                        )
                                                                    }),
                                                            )
                                                            .fill(if can_delete_op {
                                                                egui::Color32::from_rgb(
                                                                    127, 29, 29,
                                                                )
                                                            } else {
                                                                egui::Color32::from_rgb(30, 41, 59)
                                                            })
                                                            .min_size(egui::vec2(50.0, 24.0));
                                                            if ui
                                                                .add_enabled(can_delete_op, del_btn)
                                                                .on_disabled_hover_text(format!(
                                                                    "Tidak dapat dihapus: masih ada {} skenario di dalam operasi ini. Hapus atau pindahkan semua skenario terlebih dahulu.",
                                                                    op.skenario_count
                                                                ))
                                                                .clicked()
                                                            {
                                                                open_delete_for = Some(op.clone());
                                                            }
                                                        }
                                                    });
                                                }
                                                _ => {}
                                            },
                                        );
                                    });

                                let (line_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 1.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    line_rect,
                                    0.0,
                                    egui::Color32::from_rgb(28, 39, 59),
                                );
                            }
                        });
                });

            if let Some(op) = open_skenarios_for {
                navigate_to_operasi_skenarios(state, api, op);
            }
            if let Some(op) = open_detail_for {
                state.operasi_table.open_detail(&op);
            }
            if let Some(op) = open_edit_for {
                state.operasi_table.open_edit(&op);
            }
            if let Some(op) = open_delete_for {
                state.operasi_table.open_delete(&op);
            }

            ui.add_space(10.0);

            // Server-side pagination footer
            let pag = draw_pagination_bar(
                ui,
                "operasi_pagination",
                state.operasi_table.page,
                state.operasi_table.per_page,
                state.operasi_table.total,
                state.operasis.len(),
                state.operasi_table.loading,
                "operasi",
            );
            if let Some(new_per) = pag.new_per_page {
                state.operasi_table.per_page = new_per;
                state.operasi_table.page = 1;
                trigger_load_operasis(state, api);
            } else if let Some(new_page) = pag.new_page {
                state.operasi_table.page = new_page;
                trigger_load_operasis(state, api);
            }
        });

    draw_operasi_modals(ctx, state, api);
}

fn draw_operasi_modals(ctx: &egui::Context, state: &mut AppState, api: &ApiClientBridge) {
    let mode = state.operasi_table.modal_mode.clone();
    match mode {
        OperasiModalMode::None => {}
        OperasiModalMode::Create => {
            draw_operasi_form_modal(ctx, state, api, None);
        }
        OperasiModalMode::Edit(id) => {
            draw_operasi_form_modal(ctx, state, api, Some(id));
        }
        OperasiModalMode::Detail(op) => {
            draw_modal_backdrop(ctx, "operasi_detail_backdrop");
            let mut close = false;
            let mut go_skenario = false;
            egui::Area::new(egui::Id::new("operasi_detail_modal"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::same(22))
                        .show(ui, |ui| {
                            ui.set_min_width(480.0);
                            ui.set_max_width(540.0);

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Detail Operasi")
                                        .size(18.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("X").clicked() {
                                            close = true;
                                        }
                                    },
                                );
                            });
                            ui.separator();
                            ui.add_space(6.0);

                            detail_row(ui, "ID Operasi", &op.id.to_string());
                            detail_row(ui, "Nama Operasi", &op.name);
                            detail_row(
                                ui,
                                "Satker",
                                op.satker_name.as_deref().unwrap_or("Global / Umum"),
                            );
                            detail_row(
                                ui,
                                "Jumlah Skenario",
                                &format!("{} skenario", op.skenario_count),
                            );
                            detail_row(
                                ui,
                                "Deskripsi",
                                op.description
                                    .as_deref()
                                    .filter(|s| !s.trim().is_empty())
                                    .unwrap_or("—"),
                            );
                            detail_row(ui, "Dibuat", &format_timestamp(op.created_at));
                            detail_row(ui, "Diperbarui", &format_timestamp(op.updated_at));

                            ui.add_space(14.0);
                            ui.separator();
                            ui.add_space(8.0);

                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let go_btn = egui::Button::new(
                                            egui::RichText::new("Menuju Halaman Skenario >")
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(14, 116, 144))
                                        .min_size(egui::vec2(190.0, 30.0));
                                        if ui.add(go_btn).clicked() {
                                            go_skenario = true;
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new("Tutup")
                                                    .min_size(egui::vec2(76.0, 30.0)),
                                            )
                                            .clicked()
                                        {
                                            close = true;
                                        }
                                    },
                                );
                            });
                        });
                });

            if go_skenario {
                state.operasi_table.close_modal();
                navigate_to_operasi_skenarios(state, api, op);
            } else if close {
                state.operasi_table.close_modal();
            }
        }
        OperasiModalMode::DeleteConfirm(op) => {
            draw_modal_backdrop(ctx, "operasi_delete_backdrop");
            let mut close = false;
            let mut confirm_delete = false;
            egui::Area::new(egui::Id::new("operasi_delete_modal"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 24, 39))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(239, 68, 68)))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::same(22))
                        .show(ui, |ui| {
                            ui.set_min_width(420.0);
                            ui.set_max_width(460.0);

                            ui.label(
                                egui::RichText::new("Hapus Operasi?")
                                    .size(17.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(252, 165, 165)),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Anda yakin ingin menghapus operasi \"{}\"?",
                                    op.name
                                ))
                                .color(egui::Color32::WHITE),
                            );
                            ui.add_space(4.0);
                            let has_skenarios = op.skenario_count > 0;
                            if has_skenarios {
                                ui.colored_label(
                                    egui::Color32::from_rgb(250, 204, 21),
                                    format!(
                                        "Operasi ini masih memiliki {} skenario dan tidak dapat dihapus. Hapus atau pindahkan semua skenario terlebih dahulu.",
                                        op.skenario_count
                                    ),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(
                                        "Operasi ini sudah kosong (0 skenario) dan akan dihapus permanen.",
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }

                            if let Some(err) = &state.operasi_table.form_error {
                                ui.add_space(8.0);
                                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                            }

                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let del_btn = egui::Button::new(
                                            egui::RichText::new(if state.operasi_table.submitting {
                                                "Menghapus..."
                                            } else {
                                                "Ya, Hapus Operasi"
                                            })
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(185, 28, 28))
                                        .min_size(egui::vec2(138.0, 30.0));
                                        if ui
                                            .add_enabled(
                                                !state.operasi_table.submitting && !has_skenarios,
                                                del_btn,
                                            )
                                            .clicked()
                                        {
                                            confirm_delete = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                !state.operasi_table.submitting,
                                                egui::Button::new("Batal")
                                                    .min_size(egui::vec2(76.0, 30.0)),
                                            )
                                            .clicked()
                                        {
                                            close = true;
                                        }
                                    },
                                );
                            });
                        });
                });

            if confirm_delete {
                if let Some(auth) = current_auth(state) {
                    state.operasi_table.submitting = true;
                    state.operasi_table.form_error = None;
                    api.send(ApiCommand::DeleteOperasi { auth, id: op.id });
                }
            } else if close {
                state.operasi_table.close_modal();
            }
        }
    }
}

fn draw_operasi_form_modal(
    ctx: &egui::Context,
    state: &mut AppState,
    api: &ApiClientBridge,
    editing_id: Option<Uuid>,
) {
    draw_modal_backdrop(ctx, "operasi_form_backdrop");
    let is_super_admin = state.is_super_admin();
    let mut close = false;
    let mut submit = false;

    egui::Area::new(egui::Id::new("operasi_form_modal"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 39))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(22))
                .show(ui, |ui| {
                    ui.set_min_width(460.0);
                    ui.set_max_width(520.0);

                    let title = if editing_id.is_some() {
                        "Edit Operasi"
                    } else {
                        "Tambah Operasi Baru"
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .size(18.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!state.operasi_table.submitting, egui::Button::new("X"))
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label(
                        egui::RichText::new("Nama Operasi *")
                            .size(12.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.operasi_table.form_name)
                            .hint_text("Contoh: Operasi Pengamanan KTT 2026")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(10.0);

                    ui.label(
                        egui::RichText::new("Deskripsi")
                            .size(12.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut state.operasi_table.form_description)
                            .desired_rows(3)
                            .hint_text("Deskripsi singkat operasi (opsional)...")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(10.0);

                    if is_super_admin {
                        ui.label(
                            egui::RichText::new("Satker")
                                .size(12.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        let selected_name = state
                            .operasi_table
                            .form_satker_id
                            .and_then(|sid| {
                                state
                                    .operasi_table
                                    .satkers
                                    .iter()
                                    .find(|s| s.id == sid)
                                    .map(|s| format!("{} ({})", s.name, s.code))
                            })
                            .unwrap_or_else(|| "Global / Tanpa Satker".to_string());

                        let satkers_snapshot = state.operasi_table.satkers.clone();
                        egui::ComboBox::from_id_salt("operasi_form_satker")
                            .selected_text(selected_name)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.operasi_table.form_satker_id,
                                    None,
                                    "Global / Tanpa Satker",
                                );
                                for s in &satkers_snapshot {
                                    ui.selectable_value(
                                        &mut state.operasi_table.form_satker_id,
                                        Some(s.id),
                                        format!("{} ({})", s.name, s.code),
                                    );
                                }
                            });
                    }

                    if let Some(err) = &state.operasi_table.form_error {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                    }

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let save_btn = egui::Button::new(
                                egui::RichText::new(if state.operasi_table.submitting {
                                    "Menyimpan..."
                                } else if editing_id.is_some() {
                                    "Simpan Perubahan"
                                } else {
                                    "Buat Operasi"
                                })
                                .strong()
                                .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(14, 116, 144))
                            .min_size(egui::vec2(140.0, 30.0));
                            if ui
                                .add_enabled(!state.operasi_table.submitting, save_btn)
                                .clicked()
                            {
                                submit = true;
                            }
                            if ui
                                .add_enabled(
                                    !state.operasi_table.submitting,
                                    egui::Button::new("Batal").min_size(egui::vec2(76.0, 30.0)),
                                )
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                });
        });

    if submit {
        let name = state.operasi_table.form_name.trim().to_string();
        if name.is_empty() {
            state.operasi_table.form_error = Some("Nama operasi wajib diisi.".into());
        } else if let Some(auth) = current_auth(state) {
            let desc = {
                let d = state.operasi_table.form_description.trim();
                if d.is_empty() {
                    None
                } else {
                    Some(d.to_string())
                }
            };
            let satker_id = if is_super_admin {
                state.operasi_table.form_satker_id
            } else {
                state.current_user.as_ref().and_then(|u| u.satker_id)
            };

            state.operasi_table.submitting = true;
            state.operasi_table.form_error = None;
            if let Some(id) = editing_id {
                api.send(ApiCommand::UpdateOperasi {
                    auth,
                    id,
                    req: UpdateOperasiRequest {
                        name,
                        description: desc,
                        satker_id,
                    },
                });
            } else {
                api.send(ApiCommand::CreateOperasi {
                    auth,
                    req: CreateOperasiRequest {
                        name,
                        description: desc,
                        satker_id,
                    },
                });
            }
        }
    } else if close {
        state.operasi_table.close_modal();
    }
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(130.0, 20.0),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.set_min_width(130.0);
                ui.label(
                    egui::RichText::new(label)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            },
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(value)
                    .size(12.5)
                    .color(egui::Color32::WHITE),
            )
            .wrap(),
        );
    });
    ui.add_space(4.0);
}
