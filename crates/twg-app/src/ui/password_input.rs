use bevy_egui::egui;

/// Renders a single-line password `TextEdit` paired with a crisp vector Eye On / Eye Off toggle button.
/// Returns the `TextEdit` response (e.g. to detect Enter-key submission).
pub fn draw_password_input(
    ui: &mut egui::Ui,
    value: &mut String,
    show: &mut bool,
    hint: &str,
) -> egui::Response {
    let mut text_resp = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let toggle_w = 32.0_f32;
        let input_w = (ui.available_width() - toggle_w - 6.0).max(140.0);

        let resp = ui.add(
            egui::TextEdit::singleline(value)
                .password(!*show)
                .hint_text(hint)
                .desired_width(input_w),
        );
        let btn_h = resp.rect.height().max(22.0);
        text_resp = Some(resp);

        let (btn_rect, btn_resp) =
            ui.allocate_exact_size(egui::vec2(toggle_w, btn_h), egui::Sense::click());

        let hovered = btn_resp.hovered();
        let active = btn_resp.is_pointer_button_down_on();
        let bg_fill = if active {
            egui::Color32::from_rgb(51, 65, 85)
        } else if hovered {
            egui::Color32::from_rgb(39, 52, 73)
        } else if *show {
            egui::Color32::from_rgb(22, 46, 68)
        } else {
            egui::Color32::from_rgb(30, 41, 59)
        };
        let border_col = if hovered || *show {
            egui::Color32::from_rgb(56, 189, 248)
        } else {
            egui::Color32::from_rgb(51, 65, 85)
        };
        let icon_col = if *show {
            if hovered {
                egui::Color32::from_rgb(125, 211, 252)
            } else {
                egui::Color32::from_rgb(56, 189, 248)
            }
        } else if hovered {
            egui::Color32::from_rgb(226, 232, 240)
        } else {
            egui::Color32::from_rgb(148, 163, 184)
        };

        let painter = ui.painter();
        painter.rect(
            btn_rect,
            egui::CornerRadius::same(4),
            bg_fill,
            egui::Stroke::new(1.0, border_col),
            egui::StrokeKind::Inside,
        );

        paint_eye_icon(painter, btn_rect.center(), *show, icon_col, bg_fill);

        let btn_resp = btn_resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if *show {
                "Sembunyikan password"
            } else {
                "Tampilkan password"
            });
        if btn_resp.clicked() {
            *show = !*show;
        }
    });

    text_resp.expect("horizontal closure always renders password TextEdit")
}

/// Draws a Lucide-inspired vector Eye On (open eye + iris + pupil) or Eye Off (slashed eye).
fn paint_eye_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    eye_on: bool,
    fg: egui::Color32,
    bg: egui::Color32,
) {
    let rx = 7.4_f32;
    let ry = 4.6_f32;
    const STEPS: usize = 12;

    let mut top_arc = Vec::with_capacity(STEPS + 1);
    let mut bot_arc = Vec::with_capacity(STEPS + 1);
    for i in 0..=STEPS {
        let t = -1.0 + 2.0 * (i as f32 / STEPS as f32);
        let x = center.x + t * rx;
        let curve = 1.0 - t * t;
        top_arc.push(egui::pos2(x, center.y - ry * curve));
        bot_arc.push(egui::pos2(x, center.y + ry * curve));
    }

    let stroke = egui::Stroke::new(1.45, fg);
    painter.add(egui::Shape::line(top_arc, stroke));
    painter.add(egui::Shape::line(bot_arc, stroke));

    if eye_on {
        // Eye On: iris ring + solid pupil
        painter.circle_stroke(center, 2.7, egui::Stroke::new(1.35, fg));
        painter.circle_filled(center, 1.15, fg);
    } else {
        // Eye Off: muted iris ring + clean cut-out diagonal slash
        painter.circle_stroke(center, 2.35, egui::Stroke::new(1.2, fg));
        let p0 = egui::pos2(center.x - 6.6, center.y - 5.2);
        let p1 = egui::pos2(center.x + 6.6, center.y + 5.2);
        painter.line_segment([p0, p1], egui::Stroke::new(3.2, bg));
        painter.line_segment([p0, p1], egui::Stroke::new(1.55, fg));
    }
}
