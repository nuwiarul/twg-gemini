use bevy_egui::egui;

pub struct PaginationAction {
    pub new_page: Option<i64>,
    pub new_per_page: Option<i64>,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_pagination_bar(
    ui: &mut egui::Ui,
    id_salt: &str,
    page: i64,
    per_page: i64,
    total: i64,
    current_count: usize,
    loading: bool,
    item_label: &str,
) -> PaginationAction {
    let mut action = PaginationAction {
        new_page: None,
        new_per_page: None,
    };

    let safe_per_page = per_page.max(1);
    let total_pages = ((total + safe_per_page - 1) / safe_per_page).max(1);
    let safe_page = page.clamp(1, total_pages);

    let start_idx = if total == 0 || current_count == 0 {
        0
    } else {
        (safe_page - 1) * safe_per_page + 1
    };
    let end_idx = if total == 0 || current_count == 0 {
        0
    } else {
        (start_idx + current_count as i64 - 1).min(total)
    };

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(18, 26, 42))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 49, 71)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Left: record summary
                ui.label(
                    egui::RichText::new(format!(
                        "Menampilkan {start_idx}–{end_idx} dari {total} {item_label}  •  Halaman {safe_page} / {total_pages}"
                    ))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Right-most: per-page selector
                    let mut selected_per = safe_per_page;
                    egui::ComboBox::from_id_salt(format!("{id_salt}_per_page"))
                        .selected_text(format!("{selected_per} / hal"))
                        .width(86.0)
                        .show_ui(ui, |ui| {
                            for opt in [5_i64, 10, 20, 50] {
                                ui.selectable_value(&mut selected_per, opt, format!("{opt} / halaman"));
                            }
                        });
                    if selected_per != safe_per_page {
                        action.new_per_page = Some(selected_per);
                    }

                    ui.separator();

                    // Page navigation buttons (drawn right-to-left, so Last -> Next -> numbers -> Prev -> First)
                    let can_next = !loading && safe_page < total_pages;
                    let can_prev = !loading && safe_page > 1;

                    if ui
                        .add_enabled(can_next, egui::Button::new("Akhir »").min_size(egui::vec2(56.0, 24.0)))
                        .clicked()
                    {
                        action.new_page = Some(total_pages);
                    }
                    if ui
                        .add_enabled(can_next, egui::Button::new("Next ›").min_size(egui::vec2(54.0, 24.0)))
                        .clicked()
                    {
                        action.new_page = Some(safe_page + 1);
                    }

                    // Windowed page numbers (reversed order because inside right_to_left layout)
                    let pages = build_page_window(safe_page, total_pages);
                    for entry in pages.into_iter().rev() {
                        match entry {
                            PageToken::Ellipsis => {
                                ui.label(
                                    egui::RichText::new("…")
                                        .color(egui::Color32::from_rgb(100, 116, 139)),
                                );
                            }
                            PageToken::Number(p) => {
                                let is_cur = p == safe_page;
                                let btn = egui::Button::new(
                                    egui::RichText::new(p.to_string())
                                        .strong()
                                        .color(if is_cur {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::from_rgb(203, 213, 225)
                                        }),
                                )
                                .fill(if is_cur {
                                    egui::Color32::from_rgb(14, 116, 144)
                                } else {
                                    egui::Color32::from_rgb(30, 41, 59)
                                })
                                .min_size(egui::vec2(28.0, 24.0));
                                if ui.add_enabled(!loading, btn).clicked() && !is_cur {
                                    action.new_page = Some(p);
                                }
                            }
                        }
                    }

                    if ui
                        .add_enabled(can_prev, egui::Button::new("‹ Prev").min_size(egui::vec2(54.0, 24.0)))
                        .clicked()
                    {
                        action.new_page = Some(safe_page - 1);
                    }
                    if ui
                        .add_enabled(can_prev, egui::Button::new("« Awal").min_size(egui::vec2(56.0, 24.0)))
                        .clicked()
                    {
                        action.new_page = Some(1);
                    }
                });
            });
        });

    action
}

enum PageToken {
    Number(i64),
    Ellipsis,
}

fn build_page_window(current: i64, total: i64) -> Vec<PageToken> {
    if total <= 7 {
        return (1..=total).map(PageToken::Number).collect();
    }
    let mut out = Vec::new();
    out.push(PageToken::Number(1));

    let left = (current - 1).max(2);
    let right = (current + 1).min(total - 1);

    if left > 2 {
        out.push(PageToken::Ellipsis);
    }
    for p in left..=right {
        out.push(PageToken::Number(p));
    }
    if right < total - 1 {
        out.push(PageToken::Ellipsis);
    }
    out.push(PageToken::Number(total));
    out
}
