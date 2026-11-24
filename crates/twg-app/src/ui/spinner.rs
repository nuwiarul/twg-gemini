use bevy_egui::egui;

#[derive(Debug, Clone, Copy)]
pub struct SpinnerConfig {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub drag_speed: f64,
    pub decimals: usize,
    pub suffix: &'static str,
    pub wrap_around: bool,
}

impl Default for SpinnerConfig {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            drag_speed: 0.1,
            decimals: 1,
            suffix: "",
            wrap_around: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct HoldRepeatState {
    press_start: f64,
    last_tick: f64,
}

/// Custom egui numeric spinner filling available width, combining:
/// 1) Direct keyboard input + horizontal drag (left/right) via `DragValue`
/// 2) Stacked vector Up (▲) and Down (▼) stepper buttons with click + press-and-hold auto-repeat
pub fn numeric_spinner(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut f64,
    cfg: SpinnerConfig,
) -> bool {
    let total_w = (ui.available_width() - 2.0).max(72.0);
    numeric_spinner_w(ui, id_salt, value, total_w, cfg)
}

/// Custom egui numeric spinner with an explicit total width (ideal for compact horizontal rows).
pub fn numeric_spinner_w(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut f64,
    total_width: f32,
    cfg: SpinnerConfig,
) -> bool {
    let mut changed = false;

    ui.push_id(id_salt, |ui| {
        let total_w = total_width.max(66.0);
        let btn_col_w = 19.0_f32;
        let inner_w = (total_w - 8.0).max(58.0);
        let field_w = (inner_w - btn_col_w - 3.0).max(36.0);
        let row_h = 22.0_f32;

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(13, 19, 31))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(3, 1))
            .show(ui, |ui| {
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;

                    // Left: DragValue (supports typing + left/right drag)
                    let mut dv = egui::DragValue::new(value)
                        .speed(cfg.drag_speed)
                        .range(cfg.min..=cfg.max)
                        .fixed_decimals(cfg.decimals);
                    if !cfg.suffix.is_empty() {
                        dv = dv.suffix(cfg.suffix);
                    }
                    let dv_resp = ui.add_sized(egui::vec2(field_w, row_h - 2.0), dv);
                    if dv_resp.changed() {
                        *value = apply_bounds(*value, cfg);
                        changed = true;
                    }

                    // Right: Stacked Up & Down vector arrow buttons
                    let (btn_area, _) =
                        ui.allocate_exact_size(egui::vec2(btn_col_w, row_h), egui::Sense::hover());
                    let half_h = btn_area.height() * 0.5;
                    let up_rect = egui::Rect::from_min_size(
                        btn_area.min,
                        egui::vec2(btn_area.width(), half_h - 0.5),
                    );
                    let down_rect = egui::Rect::from_min_size(
                        egui::pos2(btn_area.min.x, btn_area.min.y + half_h + 0.5),
                        egui::vec2(btn_area.width(), half_h - 0.5),
                    );

                    if draw_stepper_button(ui, "up", up_rect, true) {
                        *value = apply_bounds(*value + cfg.step, cfg);
                        changed = true;
                    }
                    if draw_stepper_button(ui, "down", down_rect, false) {
                        *value = apply_bounds(*value - cfg.step, cfg);
                        changed = true;
                    }
                });
            });
    });

    changed
}

pub fn numeric_spinner_f32_w(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut f32,
    total_width: f32,
    cfg: SpinnerConfig,
) -> bool {
    let mut fval = *value as f64;
    let changed = numeric_spinner_w(ui, id_salt, &mut fval, total_width, cfg);
    if changed {
        *value = fval as f32;
    }
    changed
}

pub fn numeric_spinner_i32(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut i32,
    min: i32,
    max: i32,
    step: i32,
) -> bool {
    let total_w = (ui.available_width() - 2.0).max(72.0);
    numeric_spinner_i32_w(ui, id_salt, value, total_w, min, max, step)
}

pub fn numeric_spinner_i32_w(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut i32,
    total_width: f32,
    min: i32,
    max: i32,
    step: i32,
) -> bool {
    let mut fval = *value as f64;
    let changed = numeric_spinner_w(
        ui,
        id_salt,
        &mut fval,
        total_width,
        SpinnerConfig {
            min: min as f64,
            max: max as f64,
            step: step.max(1) as f64,
            drag_speed: 0.5,
            decimals: 0,
            suffix: "",
            wrap_around: false,
        },
    );
    if changed {
        *value = (fval.round() as i32).clamp(min, max);
    }
    changed
}

#[allow(clippy::too_many_arguments)]
pub fn numeric_spinner_u32_w(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut u32,
    total_width: f32,
    min: u32,
    max: u32,
    step: u32,
    drag_speed: f64,
) -> bool {
    let mut fval = *value as f64;
    let changed = numeric_spinner_w(
        ui,
        id_salt,
        &mut fval,
        total_width,
        SpinnerConfig {
            min: min as f64,
            max: max as f64,
            step: step.max(1) as f64,
            drag_speed,
            decimals: 0,
            suffix: "",
            wrap_around: false,
        },
    );
    if changed {
        *value = (fval.round().max(0.0) as u32).clamp(min, max);
    }
    changed
}

fn apply_bounds(v: f64, cfg: SpinnerConfig) -> f64 {
    let span = cfg.max - cfg.min;
    let bounded = if cfg.wrap_around && span > 0.0 {
        cfg.min + (v - cfg.min).rem_euclid(span)
    } else {
        v.clamp(cfg.min, cfg.max)
    };
    let factor = 10_f64.powi(cfg.decimals.min(6) as i32);
    let rounded = (bounded * factor).round() / factor;
    if rounded.abs() < 1e-12 { 0.0 } else { rounded }
}

fn draw_stepper_button(ui: &mut egui::Ui, salt: &str, rect: egui::Rect, is_up: bool) -> bool {
    let enabled = ui.is_enabled();
    let id = ui.make_persistent_id(salt);
    let sense = if enabled {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::hover()
    };
    let resp = ui.interact(rect, id, sense);

    let fill = if !enabled {
        egui::Color32::from_rgb(17, 23, 35)
    } else if resp.is_pointer_button_down_on() {
        egui::Color32::from_rgb(14, 116, 144)
    } else if resp.hovered() {
        egui::Color32::from_rgb(38, 52, 74)
    } else {
        egui::Color32::from_rgb(24, 34, 53)
    };
    let arrow_col = if !enabled {
        egui::Color32::from_rgb(71, 85, 105)
    } else if resp.is_pointer_button_down_on() || resp.hovered() {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(186, 230, 253)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, fill);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(0.75, egui::Color32::from_rgb(51, 65, 85)),
        egui::StrokeKind::Inside,
    );

    // Crisp vector triangle (zero font glyph dependency!)
    let c = rect.center();
    let dx = 3.4_f32;
    let dy = 2.2_f32;
    let pts = if is_up {
        vec![
            egui::pos2(c.x, c.y - dy),
            egui::pos2(c.x + dx, c.y + dy),
            egui::pos2(c.x - dx, c.y + dy),
        ]
    } else {
        vec![
            egui::pos2(c.x - dx, c.y - dy),
            egui::pos2(c.x + dx, c.y - dy),
            egui::pos2(c.x, c.y + dy),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        pts,
        arrow_col,
        egui::Stroke::NONE,
    ));

    if !enabled {
        return false;
    }

    // Fire immediately on mouse-down, then repeat smoothly while held down
    let now = ui.input(|i| i.time);
    let hold_id = id.with("hold_repeat");
    let mut triggered = false;
    if resp.is_pointer_button_down_on() {
        ui.memory_mut(|mem| {
            let st = mem.data.get_temp_mut_or_default::<HoldRepeatState>(hold_id);
            if st.press_start <= 0.0 {
                st.press_start = now;
                st.last_tick = now;
                triggered = true;
            } else if now - st.press_start >= 0.34 && now - st.last_tick >= 0.075 {
                st.last_tick = now;
                triggered = true;
            }
        });
        ui.ctx().request_repaint();
    } else {
        ui.memory_mut(|mem| {
            let st = mem.data.get_temp_mut_or_default::<HoldRepeatState>(hold_id);
            st.press_start = 0.0;
            st.last_tick = 0.0;
        });
    }

    triggered
}
