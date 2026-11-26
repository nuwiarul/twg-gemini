use bevy_egui::egui;
use twg_shared::{
    models::Marker,
    route_formation::{MultiRouteConfig, PatwalUnitRole, RouteFormationKind},
};
use uuid::Uuid;

use crate::ui::spinner::{SpinnerConfig, numeric_spinner};

/// Interactive top-down 2D tactical Formation Pad inside the Inspector.
///
/// - Displays all bound units (`#1..#N`) relative to the route travel direction (`▲ ARAH RUTE`).
/// - In `RouteFormationKind::Custom`, supports direct click-and-drag of unit pins, optional grid
///   snapping, and precision `(Kanan/Kiri, Depan/Belakang)` numeric spinners.
/// - In preset formations (`Grid`, `Wedge`, `Staggered`, `VipEscort`), renders a live top-down
///   tactical diagram synchronized with the 3D map ghost preview.
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

    // Determine auto-scaled metric range so all units fit comfortably with breathing room
    let max_ext = offsets
        .iter()
        .map(|(_, lat, lon)| lat.abs().max(lon.abs()))
        .fold(5.0_f64, f64::max);
    let half_range_m = ((max_ext * 1.4).ceil()).clamp(8.0, 90.0);
    let grid_step_m = if half_range_m <= 10.0 {
        2.0
    } else if half_range_m <= 24.0 {
        4.0
    } else if half_range_m <= 50.0 {
        10.0
    } else {
        20.0
    };

    let pad_w = ui.available_width().max(180.0);
    let pad_h = 205.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(pad_w, pad_h), egui::Sense::click_and_drag());
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

    let center = egui::pos2(rect.center().x, rect.center().y + 6.0);
    let usable_half_px = ((rect.width().min(rect.height() - 28.0)) * 0.45).max(36.0);
    let px_per_m = usable_half_px / (half_range_m as f32);

    let to_screen = |lat_m: f64, lon_m: f64| -> egui::Pos2 {
        egui::pos2(
            center.x + (lat_m as f32) * px_per_m,
            center.y - (lon_m as f32) * px_per_m,
        )
    };
    let from_screen = |p: egui::Pos2| -> (f64, f64) {
        let lat = ((p.x - center.x) / px_per_m) as f64;
        let lon = (-(p.y - center.y) / px_per_m) as f64;
        (lat, lon)
    };

    // Draw tactical meter grid
    let grid_col = egui::Color32::from_rgba_unmultiplied(51, 65, 85, 85);
    let axis_col = egui::Color32::from_rgba_unmultiplied(56, 189, 248, 120);
    let max_steps = (half_range_m / grid_step_m).ceil() as i32;
    for k in -max_steps..=max_steps {
        let m = (k as f64) * grid_step_m;
        let x = center.x + (m as f32) * px_per_m;
        if x > rect.left() + 2.0 && x < rect.right() - 2.0 {
            let stroke = if k == 0 {
                egui::Stroke::new(1.1, axis_col)
            } else {
                egui::Stroke::new(0.8, grid_col)
            };
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + 18.0),
                    egui::pos2(x, rect.bottom() - 4.0),
                ],
                stroke,
            );
        }
        let y = center.y - (m as f32) * px_per_m;
        if y > rect.top() + 18.0 && y < rect.bottom() - 4.0 {
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

    // VIP escort dashed circle guide when VipEscort preset is active
    if cfg.formation == RouteFormationKind::VipEscort && cfg.marker_ids.len() > 1 {
        let r_px = (cfg.vip_radius_m as f32) * px_per_m;
        painter.circle_stroke(
            center,
            r_px,
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(251, 191, 36, 110),
            ),
        );
    }

    // Prominent top header arrow indicating forward route movement direction
    let top_mid = egui::pos2(rect.center().x, rect.top() + 10.0);
    painter.text(
        top_mid,
        egui::Align2::CENTER_CENTER,
        "▲  ARAH GERAK RUTE  ▲",
        egui::FontId::proportional(9.5),
        egui::Color32::from_rgb(52, 211, 153),
    );
    painter.text(
        egui::pos2(rect.left() + 6.0, center.y - 8.0),
        egui::Align2::LEFT_CENTER,
        "Kiri",
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgb(100, 116, 139),
    );
    painter.text(
        egui::pos2(rect.right() - 6.0, center.y - 8.0),
        egui::Align2::RIGHT_CENTER,
        "Kanan",
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgb(100, 116, 139),
    );
    painter.text(
        egui::pos2(rect.right() - 6.0, rect.bottom() - 8.0),
        egui::Align2::RIGHT_CENTER,
        format!("Grid {:.0}m", grid_step_m),
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgb(100, 116, 139),
    );

    // Center reference marker (Waypoint #1 centerline origin)
    painter.circle_stroke(
        center,
        4.0,
        egui::Stroke::new(1.2, egui::Color32::from_rgb(16, 185, 129)),
    );

    // Interactive Drag & Click handling on unit pins
    let drag_key = egui::Id::new(("formation_pad_drag_pin", anim_id));
    let mut active_drag_pin: Option<Uuid> = ui.ctx().data(|d| d.get_temp(drag_key));

    if (response.drag_started() || response.clicked())
        && let Some(ptr) = response.interact_pointer_pos()
    {
        let mut best: Option<(Uuid, f32)> = None;
        for &(mid, lat_m, lon_m) in &offsets {
            let pin_p = to_screen(lat_m, lon_m);
            let dist = pin_p.distance(ptr);
            if dist <= 18.0 && best.is_none_or(|(_, bd)| dist < bd) {
                best = Some((mid, dist));
            }
        }
        if let Some((hit_mid, _)) = best {
            *selected_marker = Some(hit_mid);
            if editable && is_custom && response.drag_started() {
                active_drag_pin = Some(hit_mid);
                ui.ctx().data_mut(|d| d.insert_temp(drag_key, hit_mid));
            }
        }
    }

    if response.dragged()
        && editable
        && is_custom
        && let Some(drag_mid) = active_drag_pin
        && let Some(ptr) = response.interact_pointer_pos()
    {
        let (mut lat_m, mut lon_m) = from_screen(ptr);
        let limit = (half_range_m * 1.12).min(240.0);
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

    if response.drag_stopped() && active_drag_pin.is_some() {
        active_drag_pin = None;
        ui.ctx().data_mut(|d| d.remove::<Uuid>(drag_key));
    }

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
            center,
            egui::Align2::CENTER_CENTER,
            "Belum ada unit dipilih\nKlik 'Pilih Unit di Peta' di atas",
            egui::FontId::proportional(10.5),
            egui::Color32::from_rgb(148, 163, 184),
        );
    } else {
        for (idx, &(mid, lat_m, lon_m)) in draw_offsets.iter().enumerate() {
            let pos = to_screen(lat_m, lon_m);
            let is_sel = *selected_marker == Some(mid);
            let is_dragged = active_drag_pin == Some(mid);
            let is_hov = hover_ptr.is_some_and(|hp| hp.distance(pos) <= 14.0);
            if is_hov {
                hovered_info = Some((idx, mid, lat_m, lon_m, pos));
            }

            // Faint leader line from origin to pin
            painter.line_segment(
                [center, pos],
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
    if active_drag_pin.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hovered_info.is_some() {
        ui.ctx().set_cursor_icon(if editable && is_custom {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::PointingHand
        });
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
            ui.spacing_mut().item_spacing.x = 6.0;
            for (tag, col) in [
                ("MTR-DPN", egui::Color32::from_rgb(34, 211, 238)),
                ("PTW-DPN", egui::Color32::from_rgb(59, 130, 246)),
                ("VIP", egui::Color32::from_rgb(251, 191, 36)),
                ("MTR-SMP", egui::Color32::from_rgb(45, 212, 191)),
                ("PTW-BLK", egui::Color32::from_rgb(129, 140, 248)),
            ] {
                ui.label(egui::RichText::new(format!("● {tag}")).size(9.0).color(col));
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
