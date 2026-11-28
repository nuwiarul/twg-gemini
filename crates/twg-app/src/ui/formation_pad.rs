use bevy_egui::egui;
use twg_shared::{
    models::Marker,
    route_formation::{MultiRouteConfig, PatwalUnitRole, RouteFormationKind},
};
use uuid::Uuid;

use crate::ui::spinner::{SpinnerConfig, numeric_spinner};

#[derive(Debug, Clone, Copy)]
struct FormationPadView {
    zoom: f32,
    pan_lat_m: f64,
    pan_lon_m: f64,
}

impl Default for FormationPadView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_lat_m: 0.0,
            pan_lon_m: 0.0,
        }
    }
}

fn paint_up_triangle(
    painter: &egui::Painter,
    center: egui::Pos2,
    half_w: f32,
    half_h: f32,
    color: egui::Color32,
) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(center.x, center.y - half_h),
            egui::pos2(center.x - half_w, center.y + half_h),
            egui::pos2(center.x + half_w, center.y + half_h),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

fn draw_legend_dot_badge(ui: &mut egui::Ui, color: egui::Color32, tag: &str, tooltip: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (dot_rect, dot_resp) =
            ui.allocate_exact_size(egui::vec2(10.0, 12.0), egui::Sense::hover());
        if ui.is_rect_visible(dot_rect) {
            let p = ui.painter();
            let c = dot_rect.center();
            p.circle_filled(c, 4.2, color);
            p.circle_stroke(
                c,
                4.2,
                egui::Stroke::new(0.9, egui::Color32::from_rgb(203, 213, 225)),
            );
        }
        let lbl_resp = ui.label(egui::RichText::new(tag).size(9.5).strong().color(color));
        dot_resp.on_hover_text(tooltip);
        lbl_resp.on_hover_text(tooltip);
    });
}

/// Interactive top-down 2D tactical Formation Pad inside the Inspector.
///
/// - Displays all bound units (`#1..#N`) relative to the route travel direction (`ARAH GERAK RUTE`).
/// - Supports Zoom In / Zoom Out / Auto-Fit / Reset and pan so dense formations (e.g. Patwal)
///   can be inspected and edited accurately.
/// - In `RouteFormationKind::Custom`, supports direct click-and-drag of unit pins, optional grid
///   snapping, and precision `(Kanan/Kiri, Depan/Belakang)` numeric spinners.
/// - In preset formations (`Grid`, `Wedge`, `Staggered`, `VipEscort`, `Patwal`), renders a live
///   top-down tactical diagram synchronized with the 3D map ghost preview.
///
/// Returns `true` if `cfg` was modified.
#[allow(clippy::too_many_arguments)]
pub fn draw_formation_pad(
    ui: &mut egui::Ui,
    anim_id: Uuid,
    cfg: &mut MultiRouteConfig,
    markers: &[Marker],
    selected_marker: &mut Option<Uuid>,
    snap_m: &mut f64,
    editable: bool,
) -> bool {
    let mut changed = false;
    let is_custom = cfg.formation == RouteFormationKind::Custom;
    let is_patwal = cfg.formation == RouteFormationKind::Patwal;
    let offsets = cfg.compute_all_offsets();

    // Ensure selected_marker still belongs to this route
    if let Some(sel) = *selected_marker
        && !cfg.marker_ids.contains(&sel)
    {
        *selected_marker = cfg.marker_ids.first().copied();
    }
    if selected_marker.is_none() {
        *selected_marker = cfg.marker_ids.first().copied();
    }

    // Per-animation Formation Pad zoom & pan view state
    let view_key = egui::Id::new(("formation_pad_view", anim_id));
    let mut view: FormationPadView = ui.ctx().data(|d| d.get_temp(view_key)).unwrap_or_default();

    // Base metric range from current formation extents
    let max_ext = offsets
        .iter()
        .map(|(_, lat, lon)| lat.abs().max(lon.abs()))
        .fold(5.0_f64, f64::max);
    let base_half_range_m = ((max_ext * 1.4).ceil()).clamp(8.0, 90.0);

    // Compact Zoom & Fit toolbar above the Formation Pad
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            egui::RichText::new("Zoom Pad:")
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        if ui
            .add(
                egui::Button::new(egui::RichText::new(" - ").size(10.5).strong())
                    .fill(egui::Color32::from_rgb(24, 34, 52))
                    .min_size(egui::vec2(22.0, 19.0)),
            )
            .on_hover_text("Zoom Out (perkecil tampilan pad)")
            .clicked()
        {
            view.zoom = (view.zoom / 1.35).clamp(0.25, 12.0);
        }
        ui.label(
            egui::RichText::new(format!("{:.0}%", view.zoom * 100.0))
                .size(10.0)
                .monospace()
                .color(egui::Color32::from_rgb(186, 230, 253)),
        );
        if ui
            .add(
                egui::Button::new(egui::RichText::new(" + ").size(10.5).strong())
                    .fill(egui::Color32::from_rgb(24, 34, 52))
                    .min_size(egui::vec2(22.0, 19.0)),
            )
            .on_hover_text("Zoom In (perbesar tampilan pad)")
            .clicked()
        {
            view.zoom = (view.zoom * 1.35).clamp(0.25, 12.0);
        }
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("Fit")
                        .size(9.5)
                        .strong()
                        .color(egui::Color32::from_rgb(167, 243, 208)),
                )
                .fill(egui::Color32::from_rgb(15, 42, 54))
                .min_size(egui::vec2(28.0, 19.0)),
            )
            .on_hover_text("Fokus & perbesar agar seluruh unit pas di tengah pad")
            .clicked()
            && !offsets.is_empty()
        {
            let (mut min_lat, mut max_lat) = (f64::INFINITY, f64::NEG_INFINITY);
            let (mut min_lon, mut max_lon) = (f64::INFINITY, f64::NEG_INFINITY);
            for &(_, lat, lon) in &offsets {
                min_lat = min_lat.min(lat);
                max_lat = max_lat.max(lat);
                min_lon = min_lon.min(lon);
                max_lon = max_lon.max(lon);
            }
            view.pan_lat_m = (min_lat + max_lat) * 0.5;
            view.pan_lon_m = (min_lon + max_lon) * 0.5;
            let span = (max_lat - min_lat).max(max_lon - min_lon).max(4.0);
            let target_half = (span * 0.68).max(3.0);
            view.zoom = ((base_half_range_m / target_half) as f32).clamp(0.25, 12.0);
        }
        let is_default_view = (view.zoom - 1.0).abs() < 0.01
            && view.pan_lat_m.abs() < 0.05
            && view.pan_lon_m.abs() < 0.05;
        if ui
            .add_enabled(
                !is_default_view,
                egui::Button::new(egui::RichText::new("Reset").size(9.5))
                    .fill(egui::Color32::from_rgb(24, 34, 52))
                    .min_size(egui::vec2(38.0, 19.0)),
            )
            .on_hover_text("Reset zoom 100% dan pusat ke titik awal (0, 0)")
            .clicked()
        {
            view = FormationPadView::default();
        }
    });
    ui.add_space(3.0);

    let pad_w = ui.available_width().max(180.0);
    let pad_h = 215.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(pad_w, pad_h), egui::Sense::click_and_drag());

    // Ctrl + Scroll wheel zoom when pointer hovers over the pad
    if response.hovered() {
        let (scroll_y, ctrl_held) =
            ui.ctx()
                .input(|i| (i.smooth_scroll_delta.y, i.modifiers.ctrl || i.modifiers.command));
        if ctrl_held && scroll_y.abs() > 0.1 {
            let factor = (1.0 + scroll_y * 0.0045).clamp(0.7, 1.45);
            view.zoom = (view.zoom * factor).clamp(0.25, 12.0);
        }
    }

    let half_range_m = (base_half_range_m / (view.zoom as f64)).clamp(1.5, 250.0);
    let grid_step_m = if half_range_m <= 4.0 {
        0.5
    } else if half_range_m <= 8.0 {
        1.0
    } else if half_range_m <= 16.0 {
        2.0
    } else if half_range_m <= 36.0 {
        5.0
    } else if half_range_m <= 75.0 {
        10.0
    } else if half_range_m <= 150.0 {
        20.0
    } else {
        50.0
    };

    let painter = ui.painter_at(rect);

    // Canvas background & border
    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(10, 17, 30));
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(
            1.1,
            if is_custom {
                egui::Color32::from_rgb(56, 189, 248)
            } else {
                egui::Color32::from_rgb(44, 62, 88)
            },
        ),
        egui::StrokeKind::Inside,
    );

    let view_center = egui::pos2(rect.center().x, rect.center().y + 6.0);
    let usable_half_px = ((rect.width().min(rect.height() - 28.0)) * 0.45).max(36.0);
    let px_per_m = usable_half_px / (half_range_m as f32);

    let to_screen = |v: FormationPadView, lat_m: f64, lon_m: f64| -> egui::Pos2 {
        egui::pos2(
            view_center.x + ((lat_m - v.pan_lat_m) as f32) * px_per_m,
            view_center.y - ((lon_m - v.pan_lon_m) as f32) * px_per_m,
        )
    };
    let from_screen = |v: FormationPadView, p: egui::Pos2| -> (f64, f64) {
        let lat = v.pan_lat_m + ((p.x - view_center.x) / px_per_m) as f64;
        let lon = v.pan_lon_m - ((p.y - view_center.y) / px_per_m) as f64;
        (lat, lon)
    };
    let origin_screen = to_screen(view, 0.0, 0.0);

    // Draw tactical meter grid across the entire visible panned viewport
    let grid_col = egui::Color32::from_rgba_unmultiplied(51, 65, 85, 85);
    let axis_col = egui::Color32::from_rgba_unmultiplied(56, 189, 248, 120);
    let vis_half_x_m = ((rect.width() * 0.5) / px_per_m) as f64;
    let vis_half_y_m = ((rect.height() * 0.5) / px_per_m) as f64;
    let min_kx = ((view.pan_lat_m - vis_half_x_m) / grid_step_m).floor() as i32;
    let max_kx = ((view.pan_lat_m + vis_half_x_m) / grid_step_m).ceil() as i32;
    let min_ky = ((view.pan_lon_m - vis_half_y_m) / grid_step_m).floor() as i32;
    let max_ky = ((view.pan_lon_m + vis_half_y_m) / grid_step_m).ceil() as i32;

    for k in min_kx..=max_kx {
        let m = (k as f64) * grid_step_m;
        let x = view_center.x + ((m - view.pan_lat_m) as f32) * px_per_m;
        if x > rect.left() + 2.0 && x < rect.right() - 2.0 {
            let stroke = if k == 0 {
                egui::Stroke::new(1.1, axis_col)
            } else {
                egui::Stroke::new(0.8, grid_col)
            };
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + 20.0),
                    egui::pos2(x, rect.bottom() - 4.0),
                ],
                stroke,
            );
        }
    }
    for k in min_ky..=max_ky {
        let m = (k as f64) * grid_step_m;
        let y = view_center.y - ((m - view.pan_lon_m) as f32) * px_per_m;
        if y > rect.top() + 20.0 && y < rect.bottom() - 4.0 {
            let stroke = if k == 0 {
                egui::Stroke::new(1.1, axis_col)
            } else {
                egui::Stroke::new(0.8, grid_col)
            };
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 4.0, y),
                    egui::pos2(rect.right() - 4.0, y),
                ],
                stroke,
            );
        }
    }

    // VIP escort guide circle when VipEscort preset is active
    if cfg.formation == RouteFormationKind::VipEscort && cfg.marker_ids.len() > 1 {
        let r_px = (cfg.vip_radius_m as f32) * px_per_m;
        painter.circle_stroke(
            origin_screen,
            r_px,
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(251, 191, 36, 110),
            ),
        );
    }

    // Prominent top header with crisp vector up-arrows (no tofu Unicode glyphs!)
    let header_col = egui::Color32::from_rgb(52, 211, 153);
    let top_mid = egui::pos2(rect.center().x, rect.top() + 11.0);
    let header_galley = painter.layout_no_wrap(
        "ARAH GERAK RUTE".to_string(),
        egui::FontId::proportional(9.5),
        header_col,
    );
    let hw = header_galley.size().x * 0.5;
    let hh = header_galley.size().y * 0.5;
    painter.galley(
        egui::pos2(top_mid.x - hw, top_mid.y - hh),
        header_galley,
        header_col,
    );
    paint_up_triangle(
        &painter,
        egui::pos2(top_mid.x - hw - 10.0, top_mid.y),
        4.0,
        3.5,
        header_col,
    );
    paint_up_triangle(
        &painter,
        egui::pos2(top_mid.x + hw + 10.0, top_mid.y),
        4.0,
        3.5,
        header_col,
    );

    let axis_label_y = origin_screen
        .y
        .clamp(rect.top() + 30.0, rect.bottom() - 22.0);
    painter.text(
        egui::pos2(rect.left() + 6.0, axis_label_y - 8.0),
        egui::Align2::LEFT_CENTER,
        "Kiri",
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgb(100, 116, 139),
    );
    painter.text(
        egui::pos2(rect.right() - 6.0, axis_label_y - 8.0),
        egui::Align2::RIGHT_CENTER,
        "Kanan",
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgb(100, 116, 139),
    );
    let grid_label = if grid_step_m < 1.0 {
        format!("Grid {:.1}m", grid_step_m)
    } else {
        format!("Grid {:.0}m", grid_step_m)
    };
    painter.text(
        egui::pos2(rect.right() - 6.0, rect.bottom() - 8.0),
        egui::Align2::RIGHT_CENTER,
        grid_label,
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgb(100, 116, 139),
    );
    painter.text(
        egui::pos2(rect.left() + 6.0, rect.bottom() - 8.0),
        egui::Align2::LEFT_CENTER,
        "Ctrl+Scroll: Zoom • Drag kosong: Geser",
        egui::FontId::proportional(8.0),
        egui::Color32::from_rgb(82, 98, 122),
    );

    // Center reference marker (Waypoint #1 centerline origin)
    painter.circle_stroke(
        origin_screen,
        4.0,
        egui::Stroke::new(1.2, egui::Color32::from_rgb(16, 185, 129)),
    );

    // Interactive Drag & Click handling on unit pins + Drag-to-pan on empty canvas
    let drag_key = egui::Id::new(("formation_pad_drag_pin", anim_id));
    let pan_key = egui::Id::new(("formation_pad_panning", anim_id));
    let mut active_drag_pin: Option<Uuid> = ui.ctx().data(|d| d.get_temp(drag_key));
    let mut is_panning: bool = ui.ctx().data(|d| d.get_temp(pan_key)).unwrap_or(false);

    if (response.drag_started() || response.clicked())
        && let Some(ptr) = response.interact_pointer_pos()
    {
        let right_or_mid = ui.ctx().input(|i| {
            i.pointer.button_down(egui::PointerButton::Secondary)
                || i.pointer.button_down(egui::PointerButton::Middle)
        });
        let mut best: Option<(Uuid, f32)> = None;
        if !right_or_mid {
            for &(mid, lat_m, lon_m) in &offsets {
                let pin_p = to_screen(view, lat_m, lon_m);
                let dist = pin_p.distance(ptr);
                if dist <= 18.0 && best.is_none_or(|(_, bd)| dist < bd) {
                    best = Some((mid, dist));
                }
            }
        }
        if let Some((hit_mid, _)) = best {
            *selected_marker = Some(hit_mid);
            if editable && is_custom && response.drag_started() {
                active_drag_pin = Some(hit_mid);
                ui.ctx().data_mut(|d| d.insert_temp(drag_key, hit_mid));
            }
        } else if response.drag_started() {
            is_panning = true;
            ui.ctx().data_mut(|d| d.insert_temp(pan_key, true));
        }
    }

    if response.dragged() {
        if is_panning {
            let delta = response.drag_delta();
            view.pan_lat_m = (view.pan_lat_m - (delta.x / px_per_m) as f64).clamp(-240.0, 240.0);
            view.pan_lon_m = (view.pan_lon_m + (delta.y / px_per_m) as f64).clamp(-240.0, 240.0);
        } else if editable
            && is_custom
            && let Some(drag_mid) = active_drag_pin
            && let Some(ptr) = response.interact_pointer_pos()
        {
            let (mut lat_m, mut lon_m) = from_screen(view, ptr);
            let limit = (base_half_range_m * 1.6).max(half_range_m * 1.12).min(240.0);
            lat_m = lat_m.clamp(-limit, limit);
            lon_m = lon_m.clamp(-limit, limit);
            if *snap_m > 0.01 {
                lat_m = (lat_m / *snap_m).round() * *snap_m;
                lon_m = (lon_m / *snap_m).round() * *snap_m;
            } else {
                lat_m = (lat_m * 20.0).round() / 20.0;
                lon_m = (lon_m * 20.0).round() / 20.0;
            }
            cfg.set_custom_offset(drag_mid, lat_m, lon_m);
            changed = true;
        }
    }

    if response.drag_stopped() {
        if active_drag_pin.is_some() {
            active_drag_pin = None;
            ui.ctx().data_mut(|d| d.remove::<Uuid>(drag_key));
        }
        if is_panning {
            is_panning = false;
            ui.ctx().data_mut(|d| d.remove::<bool>(pan_key));
        }
    }

    // Persist updated zoom/pan state
    ui.ctx().data_mut(|d| d.insert_temp(view_key, view));
    let origin_screen = to_screen(view, 0.0, 0.0);

    // Re-evaluate offsets after any drag mutation so pins paint at exact new location this frame
    let draw_offsets = if changed {
        cfg.compute_all_offsets()
    } else {
        offsets
    };

    // Draw unit pins (#1..#N) with hover detection
    let hover_ptr = response.hover_pos();
    let mut hovered_info: Option<(usize, Uuid, f64, f64, egui::Pos2)> = None;

    if draw_offsets.is_empty() {
        painter.text(
            view_center,
            egui::Align2::CENTER_CENTER,
            "Belum ada unit dipilih\nKlik 'Pilih Unit di Peta' di atas",
            egui::FontId::proportional(10.5),
            egui::Color32::from_rgb(148, 163, 184),
        );
    } else {
        for (idx, &(mid, lat_m, lon_m)) in draw_offsets.iter().enumerate() {
            let pos = to_screen(view, lat_m, lon_m);
            let is_sel = *selected_marker == Some(mid);
            let is_dragged = active_drag_pin == Some(mid);
            let is_hov = hover_ptr.is_some_and(|hp| hp.distance(pos) <= 14.0);
            if is_hov {
                hovered_info = Some((idx, mid, lat_m, lon_m, pos));
            }

            // Faint leader line from origin to pin
            painter.line_segment(
                [origin_screen, pos],
                egui::Stroke::new(
                    0.8,
                    if is_sel {
                        egui::Color32::from_rgba_unmultiplied(250, 204, 21, 90)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(56, 189, 248, 45)
                    },
                ),
            );

            let r = if is_dragged || is_sel {
                10.5
            } else if is_hov {
                9.5
            } else {
                8.5
            };
            let fill = if is_sel {
                egui::Color32::from_rgb(250, 204, 21)
            } else if is_patwal {
                match cfg.patwal_role_for(mid) {
                    PatwalUnitRole::LeadMoto => egui::Color32::from_rgb(34, 211, 238),
                    PatwalUnitRole::FrontPolice => egui::Color32::from_rgb(59, 130, 246),
                    PatwalUnitRole::Vip => egui::Color32::from_rgb(251, 191, 36),
                    PatwalUnitRole::SideMoto => egui::Color32::from_rgb(45, 212, 191),
                    PatwalUnitRole::RearPolice => egui::Color32::from_rgb(129, 140, 248),
                }
            } else if idx == 0 {
                egui::Color32::from_rgb(6, 182, 212)
            } else {
                egui::Color32::from_rgb(16, 185, 129)
            };
            let stroke = if is_sel || is_hov {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(203, 213, 225)
            };

            // Tiny forward-facing triangle chevron atop each pin
            let tip = pos - egui::vec2(0.0, r + 5.0);
            painter.add(egui::Shape::convex_polygon(
                vec![tip, tip + egui::vec2(-3.2, 4.0), tip + egui::vec2(3.2, 4.0)],
                fill,
                egui::Stroke::NONE,
            ));

            painter.circle_filled(
                pos,
                r + 1.5,
                egui::Color32::from_rgba_unmultiplied(5, 10, 20, 210),
            );
            painter.circle_filled(pos, r, fill);
            painter.circle_stroke(pos, r, egui::Stroke::new(1.5, stroke));
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                format!("{}", idx + 1),
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(15, 23, 42),
            );
        }
    }

    // Cursor icon feedback on canvas
    if active_drag_pin.is_some() || is_panning {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hovered_info.is_some() {
        ui.ctx().set_cursor_icon(if editable && is_custom {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::PointingHand
        });
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }

    // Compact live tooltip on hovered unit pin
    if let Some((idx, mid, lat_m, lon_m, pos)) = hovered_info {
        let uname = markers
            .iter()
            .find(|m| m.id == mid)
            .map(|m| m.name.as_str())
            .unwrap_or("Unit");
        let tip = if is_patwal {
            format!(
                "#{} {} [{}] ({:+.1}m, {:+.1}m)",
                idx + 1,
                uname,
                cfg.patwal_role_for(mid).short_tag(),
                lat_m,
                lon_m
            )
        } else {
            format!("#{} {} ({:+.1}m, {:+.1}m)", idx + 1, uname, lat_m, lon_m)
        };
        let font = egui::FontId::proportional(9.5);
        let galley = painter.layout_no_wrap(tip, font, egui::Color32::WHITE);
        let pad = egui::vec2(5.0, 2.5);
        let mut tpos = pos + egui::vec2(12.0, -18.0);
        if tpos.x + galley.size().x + pad.x * 2.0 > rect.right() - 2.0 {
            tpos.x = (rect.right() - galley.size().x - pad.x * 2.0 - 4.0).max(rect.left() + 2.0);
        }
        if tpos.y < rect.top() + 18.0 {
            tpos.y = pos.y + 14.0;
        }
        let trect = egui::Rect::from_min_size(tpos, galley.size() + pad * 2.0);
        painter.rect_filled(
            trect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(15, 23, 42, 235),
        );
        painter.rect_stroke(
            trect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)),
            egui::StrokeKind::Inside,
        );
        painter.galley(trect.min + pad, galley, egui::Color32::WHITE);
    }

    ui.add_space(4.0);

    // Custom formation controls: snap toggle + precision offset spinners for selected unit
    if is_custom {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Snap Grid:")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            for (lbl, val) in [("Bebas", 0.0_f64), ("0.5m", 0.5_f64), ("1.0m", 1.0_f64)] {
                let active = (*snap_m - val).abs() < 0.01;
                let btn = egui::Button::new(egui::RichText::new(lbl).size(9.5).strong().color(
                    if active {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(148, 163, 184)
                    },
                ))
                .fill(if active {
                    egui::Color32::from_rgb(14, 116, 144)
                } else {
                    egui::Color32::from_rgb(24, 34, 52)
                })
                .small();
                if ui.add_enabled(editable, btn).clicked() {
                    *snap_m = val;
                }
            }
        });

        if let Some(sel_id) = *selected_marker
            && let Some(sel_idx) = cfg.marker_ids.iter().position(|id| *id == sel_id)
        {
            let uname = markers
                .iter()
                .find(|m| m.id == sel_id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| format!("Unit #{}", sel_idx + 1));
            let (mut lat_m, mut lon_m) =
                cfg.offset_for_kind(RouteFormationKind::Custom, sel_idx, cfg.marker_ids.len());

            ui.add_space(3.0);
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 23, 38))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 64, 92)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(format!("Offset Unit #{}: {uname}", sel_idx + 1))
                            .size(10.5)
                            .strong()
                            .color(egui::Color32::from_rgb(253, 224, 71)),
                    );
                    ui.add_space(2.0);
                    let mut spinner_changed = false;
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Kanan/Kiri:")
                                .size(10.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        spinner_changed |= numeric_spinner(
                            ui,
                            ("form_lat", anim_id, sel_id),
                            &mut lat_m,
                            SpinnerConfig {
                                min: -200.0,
                                max: 200.0,
                                step: 0.5,
                                drag_speed: 0.1,
                                decimals: 1,
                                suffix: " m",
                                wrap_around: false,
                            },
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Depan/Blk:")
                                .size(10.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        spinner_changed |= numeric_spinner(
                            ui,
                            ("form_lon", anim_id, sel_id),
                            &mut lon_m,
                            SpinnerConfig {
                                min: -200.0,
                                max: 200.0,
                                step: 0.5,
                                drag_speed: 0.1,
                                decimals: 1,
                                suffix: " m",
                                wrap_around: false,
                            },
                        );
                    });
                    if spinner_changed && editable {
                        cfg.set_custom_offset(sel_id, lat_m, lon_m);
                        changed = true;
                    }
                });
        }
    } else if is_patwal {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (tag, col, desc) in [
                (
                    "MTR-DPN",
                    egui::Color32::from_rgb(34, 211, 238),
                    "Motor Pengawal Depan (Lead Moto)",
                ),
                (
                    "PTW-DPN",
                    egui::Color32::from_rgb(59, 130, 246),
                    "Mobil Patwal Depan (Front Police)",
                ),
                (
                    "VIP",
                    egui::Color32::from_rgb(251, 191, 36),
                    "Kendaraan VIP / Utama",
                ),
                (
                    "MTR-SMP",
                    egui::Color32::from_rgb(45, 212, 191),
                    "Motor Pengawal Samping (Side Moto)",
                ),
                (
                    "PTW-BLK",
                    egui::Color32::from_rgb(129, 140, 248),
                    "Mobil Patwal Belakang (Rear Police)",
                ),
            ] {
                draw_legend_dot_badge(ui, col, tag, desc);
            }
        });
        ui.label(
            egui::RichText::new(
                "Pratinjau Konvoi Patwal (arah gerak ke atas). Klik 'Ubah ke Bebas' di atas untuk drag pin manual.",
            )
            .size(9.5)
            .color(egui::Color32::from_rgb(148, 163, 184)),
        );
    } else {
        ui.label(
            egui::RichText::new(
                "Pratinjau formasi otomatis (arah gerak ke atas). Pilih 'Bebas / Kustom' untuk geser pin manual.",
            )
            .size(9.5)
            .color(egui::Color32::from_rgb(148, 163, 184)),
        );
    }

    changed
}
