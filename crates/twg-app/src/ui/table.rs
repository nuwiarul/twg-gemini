use bevy_egui::egui;

pub const TABLE_COL_GAP: f32 = 12.0;
pub const TABLE_PAD_LEFT: f32 = 14.0;
pub const TABLE_PAD_RIGHT: f32 = 14.0;
pub const TABLE_SCROLLBAR_RESERVE: f32 = 14.0;

/// Computes the exact usable inner width shared by BOTH the table header and every row
/// inside the ScrollArea, ensuring 100% pixel-identical column alignment across full width.
pub fn table_content_width(table_outer_w: f32) -> f32 {
    (table_outer_w - TABLE_PAD_LEFT - TABLE_PAD_RIGHT - TABLE_SCROLLBAR_RESERVE).max(640.0)
}

/// Renders a single table row (header or data row) where every column `i` is placed at the
/// exact deterministic X coordinate `start.x + sum_{k < i}(widths[k] + gap)` and constrained
/// to `widths[i]` so wrapped text expands the row vertically without ever shifting sibling columns.
pub fn draw_aligned_row(
    ui: &mut egui::Ui,
    row_id: impl egui::AsIdSalt,
    widths: &[f32],
    min_height: f32,
    mut draw_col: impl FnMut(&mut egui::Ui, usize, f32),
) {
    let start = ui.cursor().min;
    let total_w: f32 =
        widths.iter().sum::<f32>() + TABLE_COL_GAP * widths.len().saturating_sub(1) as f32;
    let clip = ui.clip_rect();
    let mut max_h = min_height;
    let mut x = start.x;

    ui.push_id(row_id, |ui| {
        for (idx, &w) in widths.iter().enumerate() {
            let cell_max =
                egui::Rect::from_min_size(egui::pos2(x, start.y), egui::vec2(w, 10_000.0));
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(idx)
                    .max_rect(cell_max)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(clip.intersect(cell_max));
            child.set_min_width(w);
            child.set_max_width(w);

            draw_col(&mut child, idx, w);

            let used_h = child.min_rect().height();
            if used_h > max_h {
                max_h = used_h;
            }
            x += w + TABLE_COL_GAP;
        }
    });

    ui.advance_cursor_after_rect(egui::Rect::from_min_size(start, egui::vec2(total_w, max_h)));
}

pub fn draw_header_label(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(11.5)
                .strong()
                .color(egui::Color32::from_rgb(148, 163, 184)),
        )
        .wrap(),
    );
}
