use bevy_egui::egui;
use twg_shared::models::{MapMode, Skenario, TacticalOperasiCard};

use crate::{
    net::{
        asset_sync::AssetSyncBridge,
        client::ApiClientBridge,
        ws_client::WsClientBridge,
    },
    state::{AppState, PlaybackState},
    ui::{
        nav_bar::{
            draw_global_top_bar, open_tactical_skenario, trigger_load_tactical_operasis,
        },
        pagination::draw_pagination_bar,
    },
};

#[allow(clippy::too_many_arguments)]
pub fn draw_tactical_screen(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
    sync_bridge: &AssetSyncBridge,
) {
    draw_global_top_bar(viewport_ui, state, playback, api, ws, sync_bridge);

    let mut open_action: Option<(Skenario, String, Vec<Skenario>, Option<&'static str>)> = None;

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
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Tactical / Simulasi Realtime")
                                .size(22.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(6, 78, 59))
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(16, 185, 129),
                            ))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("LIVE SYNC")
                                        .size(10.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(167, 243, 208)),
                                );
                            });
                    });
                    ui.label(
                        egui::RichText::new(
                            "Pilih skenario di dalam kartu operasi untuk memulai simulasi realtime bersama operator lain & Layar Besar.",
                        )
                        .size(12.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !state.tactical_page.loading,
                            egui::Button::new("Refresh").min_size(egui::vec2(84.0, 32.0)),
                        )
                        .clicked()
                    {
                        trigger_load_tactical_operasis(state, api);
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
                            egui::RichText::new("Cari Operasi:")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut state.tactical_page.search)
                                .hint_text("Cari nama operasi, deskripsi, atau satker...")
                                .desired_width(300.0),
                        );
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Cari").clicked() || enter {
                            state.tactical_page.applied_search =
                                state.tactical_page.search.trim().to_string();
                            state.tactical_page.page = 1;
                            trigger_load_tactical_operasis(state, api);
                        }
                        if !state.tactical_page.applied_search.is_empty()
                            && ui.button("Reset").clicked()
                        {
                            state.tactical_page.search.clear();
                            state.tactical_page.applied_search.clear();
                            state.tactical_page.page = 1;
                            trigger_load_tactical_operasis(state, api);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.tactical_page.loading {
                                ui.colored_label(
                                    egui::Color32::from_rgb(56, 189, 248),
                                    "Memuat kartu operasi...",
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Total: {} operasi",
                                        state.tactical_page.total
                                    ))
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }
                        });
                    });
                });

            if let Some(err) = state.tactical_page.error.clone() {
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

            ui.add_space(14.0);

            let pagination_h = 48.0;
            let scroll_h = (ui.available_height() - pagination_h - 10.0).max(200.0);
            let cards = state.tactical_page.cards.clone();

            egui::ScrollArea::vertical()
                .id_salt("tactical_operasi_cards_scroll")
                .max_height(scroll_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if cards.is_empty() {
                        ui.add_space(50.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(if state.tactical_page.loading {
                                    "Memuat daftar operasi & skenario..."
                                } else {
                                    "Belum ada operasi yang tersedia untuk simulasi tactical."
                                })
                                .size(13.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                        return;
                    }

                    // Responsive Grid: Large -> 3 cols, Medium -> 2 cols, Small -> 1 col
                    let avail_w = (ui.available_width() - 12.0).max(280.0);
                    let cols = if avail_w >= 1120.0 {
                        3
                    } else if avail_w >= 720.0 {
                        2
                    } else {
                        1
                    };
                    let gap = 16.0_f32;
                    let card_w = ((avail_w - gap * (cols as f32 - 1.0)) / cols as f32).max(260.0);

                    egui::Grid::new("tactical_operasi_cards_grid")
                        .num_columns(cols)
                        .spacing([gap, gap])
                        .show(ui, |ui| {
                            for (idx, card) in cards.iter().enumerate() {
                                draw_operasi_tactical_card(
                                    ui,
                                    card_w,
                                    state,
                                    card,
                                    &mut open_action,
                                );
                                if (idx + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                            if !cards.len().is_multiple_of(cols) {
                                ui.end_row();
                            }
                        });
                });

            ui.add_space(8.0);
            let pag_action = draw_pagination_bar(
                ui,
                "tactical_operasis_pagination",
                state.tactical_page.page,
                state.tactical_page.per_page,
                state.tactical_page.total,
                cards.len(),
                state.tactical_page.loading,
                "operasi",
            );
            let mut reload_pag = false;
            if let Some(new_per) = pag_action.new_per_page {
                state.tactical_page.per_page = new_per;
                state.tactical_page.page = 1;
                reload_pag = true;
            }
            if let Some(new_page) = pag_action.new_page {
                state.tactical_page.page = new_page;
                reload_pag = true;
            }
            if reload_pag {
                trigger_load_tactical_operasis(state, api);
            }
        });

    if let Some((sk, op_name, siblings, bcast)) = open_action {
        open_tactical_skenario(state, playback, api, &sk, op_name, siblings, bcast);
    }
}

fn draw_operasi_tactical_card(
    ui: &mut egui::Ui,
    card_w: f32,
    state: &AppState,
    card: &TacticalOperasiCard,
    open_action: &mut Option<(Skenario, String, Vec<Skenario>, Option<&'static str>)>,
) {
    let op = &card.operasi;
    let pad = 16.0_f32;
    let inner_w = (card_w - pad * 2.0).max(220.0);

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 23, 42))
        .stroke(egui::Stroke::new(1.2, egui::Color32::from_rgb(38, 55, 82)))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(pad as i8))
        .show(ui, |ui| {
            // Explicit vertical layout container so all card sections stack cleanly top-to-bottom
            ui.vertical(|ui| {
                ui.set_width(inner_w);

                // 1. Judul Operasi
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&op.name)
                            .size(15.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .wrap(),
                );

                // 2. Deskripsi Operasi (tepat di bawah judul operasi)
                if let Some(desc) = &op.description
                    && !desc.trim().is_empty()
                {
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(desc.trim())
                                .size(11.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        )
                        .wrap(),
                    );
                }

                // 3. Badge Satker + Jumlah Skenario
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let satker_txt = op.satker_name.as_deref().unwrap_or("Umum / Pusat");
                    egui::Frame::new()
                        .fill(if op.satker_id.is_some() {
                            egui::Color32::from_rgb(30, 58, 138)
                        } else {
                            egui::Color32::from_rgb(30, 41, 59)
                        })
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(7, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(satker_txt)
                                    .size(10.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(191, 219, 254)),
                            );
                        });
                    ui.label(
                        egui::RichText::new(format!("{} skenario", card.skenarios.len()))
                            .size(11.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                // 4. Clean HTML-like <ul><li> list of Skenarios (NO nested cards, fixed-width Play button)
                if card.skenarios.is_empty() {
                    ui.label(
                        egui::RichText::new("Belum ada skenario di operasi ini.")
                            .size(11.5)
                            .italics()
                            .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                } else {
                    let play_btn_w = 96.0_f32;
                    let bullet_w = 14.0_f32;
                    let row_gap = 10.0_f32;
                    let title_w = (inner_w - play_btn_w - bullet_w - row_gap * 2.0).max(100.0);

                    egui::ScrollArea::vertical()
                        .id_salt(("tactical_card_sk_list", op.id))
                        .max_height(260.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_width(inner_w);
                            for (idx, sk) in card.skenarios.iter().enumerate() {
                                if idx > 0 {
                                    ui.add_space(5.0);
                                    let (sep_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(inner_w, 1.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().line_segment(
                                        [sep_rect.left_center(), sep_rect.right_center()],
                                        egui::Stroke::new(
                                            1.0,
                                            egui::Color32::from_rgb(30, 42, 62),
                                        ),
                                    );
                                    ui.add_space(5.0);
                                }

                                let can_big = state.can_control_skenario_big_screen(sk);
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = row_gap;

                                    // Bullet point (<li> style)
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(bullet_w, 20.0),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(2.0);
                                            ui.label(
                                                egui::RichText::new("•")
                                                    .size(15.0)
                                                    .strong()
                                                    .color(egui::Color32::from_rgb(52, 211, 153)),
                                            );
                                        },
                                    );

                                    // Responsive wrapping Skenario title + compact meta
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(title_w, 0.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.set_width(title_w);
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&sk.name)
                                                        .size(12.5)
                                                        .strong()
                                                        .color(egui::Color32::from_rgb(
                                                            241, 245, 249,
                                                        )),
                                                )
                                                .wrap(),
                                            );
                                            let mode_tag = match sk.map_mode {
                                                MapMode::Outdoor => "Outdoor",
                                                MapMode::Indoor => "Indoor",
                                            };
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{mode_tag} • {} unit • {} anim",
                                                    sk.marker_count, sk.anim_count,
                                                ))
                                                .size(10.0)
                                                .color(egui::Color32::from_rgb(100, 116, 139)),
                                            );
                                        },
                                    );

                                    // Fixed-width Play Tactical button (never shrinks or wraps)
                                    let play_btn = egui::Button::new(
                                        egui::RichText::new("Play Tactical")
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(5, 150, 105))
                                    .min_size(egui::vec2(play_btn_w, 26.0));
                                    if ui
                                        .add_sized([play_btn_w, 26.0], play_btn)
                                        .on_hover_text(if can_big {
                                            "Buka Play Skenario Tactical (realtime sync + otomatis tampil ke Layar Besar)"
                                        } else {
                                            "Buka Play Skenario Tactical (sinkronisasi animasi & unit realtime)"
                                        })
                                        .clicked()
                                    {
                                        *open_action = Some((
                                            sk.clone(),
                                            op.name.clone(),
                                            card.skenarios.clone(),
                                            if can_big {
                                                Some("load_skenario")
                                            } else {
                                                None
                                            },
                                        ));
                                    }
                                });
                            }
                        });
                }
            });
        });
}
