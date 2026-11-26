use bevy::prelude::*;
use bevy_egui::egui;

use crate::{
    map::camera::TacticalCamera,
    scene::{
        basic_mesh::BasicMeshShape, gizmo::compute_gizmo_world_radius,
        markers::MarkerWorldBounds, parse_hex_color, play_rts::PlayRtsState,
        playback::PlaybackOverrides, pointer_arrow::PointerArrowVisualPool,
    },
    state::{AnimRunStatus, AppState, PlaybackState, ToolMode},
};

#[allow(clippy::too_many_arguments)]
pub fn draw_projected_labels(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    overrides: &PlaybackOverrides,
    marker_bounds: &MarkerWorldBounds,
    pointer_pool: &PointerArrowVisualPool,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("twg_3d_labels"),
    ));
    let painter = painter.with_clip_rect(clip_rect);

    if state.show_text_markers {
        for tm in &state.text_markers {
            if state.hidden_entity_ids.contains(&tm.id) {
                continue;
            }
            let local = state
                .geo_anchor
                .lng_lat_alt_to_local(tm.lng, tm.lat, tm.altitude);
            let world_pos = Vec3::from_array(local);
            if let Ok(screen) = camera.world_to_viewport(cam_gtf, world_pos) {
                let pos = egui::pos2(screen.x, screen.y);
                if clip_rect.contains(pos) {
                    let c = parse_hex_color(&tm.color, 1.0).to_srgba();
                    let color = egui::Color32::from_rgb(
                        (c.red * 255.0) as u8,
                        (c.green * 255.0) as u8,
                        (c.blue * 255.0) as u8,
                    );
                    draw_badge(
                        &painter,
                        pos,
                        &tm.text,
                        color,
                        world_text_font_size_px(camera, cam_gtf, world_pos, tm.font_size as f32),
                        tm.show_background,
                    );
                }
            }
        }
    }

    // Hide all unit/object name labels during Play Skenario & Layar Besar (keep placed TextMarkers)
    if state.show_unit_labels && !playback.play_test_mode && !state.is_big_screen() {
        for m in &state.markers {
            if !overrides.is_marker_visible(m, playback.play_test_mode) {
                continue;
            }
            let world_pos = if let Some(b) = marker_bounds.bounds.get(&m.id) {
                let h_span = (b.world_max.y - b.world_min.y).max(1.0);
                Vec3::new(
                    b.footprint_center.x,
                    b.world_max.y + (h_span * 0.12).clamp(0.5, 4.0),
                    b.footprint_center.z,
                )
            } else {
                let (lng, lat, alt) = if let Some(ov) = overrides.markers.get(&m.id) {
                    (ov.lng, ov.lat, ov.altitude)
                } else {
                    (m.lng, m.lat, m.altitude)
                };
                Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt + 10.0))
            };
            if let Ok(screen) = camera.world_to_viewport(cam_gtf, world_pos) {
                let pos = egui::pos2(screen.x, screen.y);
                if clip_rect.contains(pos) {
                    draw_badge(
                        &painter,
                        pos,
                        &m.name,
                        egui::Color32::from_rgb(226, 232, 240),
                        11.5,
                        true,
                    );
                }
            }
        }
    }

    // Active 3D Pointer Arrow floating text labels (visible in Editor Preview & Play Skenario when Playing/Paused)
    for lbl in &pointer_pool.active_labels {
        if let Ok(screen) = camera.world_to_viewport(cam_gtf, lbl.world_pos) {
            let pos = egui::pos2(screen.x, screen.y);
            if clip_rect.contains(pos) {
                let tc = parse_hex_color(&lbl.text_color_hex, 1.0).to_srgba();
                let text_col = egui::Color32::from_rgb(
                    (tc.red * 255.0) as u8,
                    (tc.green * 255.0) as u8,
                    (tc.blue * 255.0) as u8,
                );
                let ac = parse_hex_color(&lbl.accent_color_hex, 1.0).to_srgba();
                let border_col = egui::Color32::from_rgba_unmultiplied(
                    (ac.red * 255.0) as u8,
                    (ac.green * 255.0) as u8,
                    (ac.blue * 255.0) as u8,
                    205,
                );
                draw_pointer_label(
                    &painter,
                    pos,
                    &lbl.text,
                    text_col,
                    border_col,
                    lbl.font_size,
                    lbl.show_bg,
                );
            }
        }
    }

    // Play Skenario Mode viewport border & floating HUD badge (omitted on clean fullscreen Layar Besar)
    if playback.play_test_mode && !state.is_big_screen() {
        let playing_cnt = playback
            .tracks
            .values()
            .filter(|t| t.status == AnimRunStatus::Playing)
            .count();
        let paused_cnt = playback
            .tracks
            .values()
            .filter(|t| t.status == AnimRunStatus::Paused)
            .count();
        let border_col = if playing_cnt > 0 {
            egui::Color32::from_rgba_unmultiplied(16, 185, 129, 215)
        } else if paused_cnt > 0 {
            egui::Color32::from_rgba_unmultiplied(245, 158, 11, 215)
        } else {
            egui::Color32::from_rgba_unmultiplied(56, 189, 248, 185)
        };
        painter.rect_stroke(
            clip_rect.shrink(1.5),
            0.0,
            egui::Stroke::new(3.0, border_col),
            egui::StrokeKind::Inside,
        );

        let hud_text = if playing_cnt > 0 {
            format!(
                "PLAY SKENARIO  •  {playing_cnt} Animasi Berjalan  ({:.2}x)",
                playback.speed_multiplier
            )
        } else if paused_cnt > 0 {
            format!(
                "PLAY SKENARIO  •  {paused_cnt} Animasi Pause  ({:.2}x)",
                playback.speed_multiplier
            )
        } else {
            format!(
                "PLAY SKENARIO  •  Siap ({} Animasi)  ({:.2}x)",
                state.anims.len(),
                playback.speed_multiplier
            )
        };
        let top_center = egui::pos2(clip_rect.center().x, clip_rect.top() + 20.0);
        let font = egui::FontId::proportional(12.5);
        let galley = painter.layout_no_wrap(hud_text, font, egui::Color32::WHITE);
        let pad = egui::vec2(12.0, 5.0);
        let pill_rect = egui::Rect::from_center_size(top_center, galley.size() + pad * 2.0);
        let pill_fill = if playing_cnt > 0 {
            egui::Color32::from_rgba_unmultiplied(6, 78, 59, 230)
        } else if paused_cnt > 0 {
            egui::Color32::from_rgba_unmultiplied(120, 53, 15, 230)
        } else {
            egui::Color32::from_rgba_unmultiplied(15, 23, 42, 230)
        };
        painter.rect_filled(pill_rect, 12.0, pill_fill);
        painter.rect_stroke(
            pill_rect,
            12.0,
            egui::Stroke::new(1.5, border_col),
            egui::StrokeKind::Inside,
        );
        painter.galley(pill_rect.min + pad, galley, egui::Color32::WHITE);
    } else if state.tool_mode != ToolMode::Select {
        // Editor active tool hint overlay in top-left of viewport
        let has_selected_marker = matches!(state.selected, crate::state::SelectedEntity::Marker(_));
        let hint = match state.tool_mode {
            ToolMode::MoveMarker => {
                if has_selected_marker {
                    "Mode Move (2): Drag panah X / Y (Alt) / Z atau lingkaran tengah untuk Free Move".to_string()
                } else {
                    "Mode Move (2): Klik objek 3D di peta untuk menampilkan Gizmo Move".to_string()
                }
            }
            ToolMode::RotateMarker => {
                if has_selected_marker {
                    "Mode Rotate (3): Drag cincin X / Y / Z atau bola tengah untuk Free Rotate"
                        .to_string()
                } else {
                    "Mode Rotate (3): Klik objek 3D di peta untuk menampilkan Gizmo Rotate"
                        .to_string()
                }
            }
            ToolMode::ScaleMarker => {
                if has_selected_marker {
                    "Mode Scale (4): Drag kubus X / Y / Z atau kubus tengah untuk Free Scale (X, Y, Z)".to_string()
                } else {
                    "Mode Scale (4): Klik objek 3D di peta untuk menampilkan Gizmo Scale"
                        .to_string()
                }
            }
            ToolMode::DrawLine => format!(
                "Mode Draw Line: {} titik — Klik peta untuk tambah, Enter selesai, Esc batal",
                state.draft_points_lnglat.len()
            ),
            ToolMode::DrawBuilding => format!(
                "Mode Draw Building: {} vertex — Min 3 titik, Enter selesai, Esc batal",
                state.draft_points_lnglat.len()
            ),
            ToolMode::DrawBasicPolygon => format!(
                "Mode Draw Polygon 3D (Tinggi {:.1}m): {} titik — Min 3 titik, Enter/Finish selesai, Esc batal",
                state.new_polygon_height,
                state.draft_points_lnglat.len()
            ),
            ToolMode::PlaceText => format!(
                "Mode Add Label: \"{}\" — Klik di peta (Esc batal)",
                state.new_text_label
            ),
            ToolMode::DrawRoute => format!(
                "Langkah 1/2 Gambar Garis Rute: {} titik — Min 2 titik, Enter selesai, Backspace undo, Esc batal",
                state.draft_points_lnglat.len()
            ),
            ToolMode::DrawTrafficLane => format!(
                "Gambar Jalur Traffic ({}): {} titik — Min 2 titik, Enter selesai, Backspace undo, Esc batal",
                if state.drawing_traffic_lane_for_anim_id.is_some() {
                    "Tambah Jalur"
                } else {
                    "Jalur 1"
                },
                state.draft_points_lnglat.len()
            ),
            ToolMode::PickRouteMarker
            | ToolMode::PickShootMarker
            | ToolMode::PickPointerMarker
            | ToolMode::PickSpawnHideMarker => String::new(),
            ToolMode::PlaceParticle => format!(
                "Mode Letakkan Partikel ({}): Klik lokasi di peta untuk menempatkan (Esc batal)",
                state.new_particle_kind.label()
            ),
            ToolMode::Measure => format!(
                "Mode Measure: {} titik — Klik untuk ukur jarak (Esc batal)",
                state.measure_points_lnglat.len()
            ),
            ToolMode::Select => String::new(),
        };
        if !hint.is_empty() {
            let font = egui::FontId::proportional(11.5);
            let galley = painter.layout_no_wrap(hint, font, egui::Color32::from_rgb(186, 230, 253));
            let pad = egui::vec2(10.0, 5.0);
            let origin = clip_rect.left_top() + egui::vec2(12.0, 12.0);
            let rect = egui::Rect::from_min_size(origin, galley.size() + pad * 2.0);
            painter.rect_filled(
                rect,
                6.0,
                egui::Color32::from_rgba_unmultiplied(15, 23, 42, 220),
            );
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(56, 189, 248, 160),
                ),
                egui::StrokeKind::Inside,
            );
            painter.galley(
                rect.min + pad,
                galley,
                egui::Color32::from_rgb(186, 230, 253),
            );
        }
    }
}

/// Converts a label's world-space font size to screen pixels so it responds
/// to camera zoom and distance just like a 3D object.
fn world_text_font_size_px(
    camera: &Camera,
    cam_gtf: &GlobalTransform,
    world_pos: Vec3,
    font_size: f32,
) -> f32 {
    let meters_per_px = compute_gizmo_world_radius(camera, cam_gtf, world_pos, 1.0);
    (font_size.max(1.0) / meters_per_px).clamp(3.0, 512.0)
}

fn draw_badge(
    painter: &egui::Painter,
    center: egui::Pos2,
    text: &str,
    text_color: egui::Color32,
    font_size: f32,
    show_background: bool,
) {
    let font = egui::FontId::proportional(font_size.clamp(3.0, 512.0));
    let galley = painter.layout_no_wrap(text.to_string(), font, text_color);
    let pad = egui::vec2(6.0, 3.0);
    let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    if show_background {
        painter.rect_filled(
            rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(10, 15, 24, 195),
        );
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(56, 189, 248, 120),
            ),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, text_color);
    } else {
        let origin = center - galley.size() * 0.5;
        let shadow = painter.layout_no_wrap(
            text.to_string(),
            egui::FontId::proportional(font_size.clamp(3.0, 512.0)),
            egui::Color32::from_rgba_unmultiplied(5, 10, 18, 225),
        );
        painter.galley(
            origin + egui::vec2(1.5, 1.5),
            shadow,
            egui::Color32::from_rgba_unmultiplied(5, 10, 18, 225),
        );
        painter.galley(origin, galley, text_color);
    }
}

fn draw_pointer_label(
    painter: &egui::Painter,
    center: egui::Pos2,
    text: &str,
    text_color: egui::Color32,
    border_color: egui::Color32,
    font_size: f32,
    show_bg: bool,
) {
    let font = egui::FontId::proportional(font_size.clamp(8.0, 56.0));
    let galley = painter.layout_no_wrap(text.to_string(), font.clone(), text_color);
    let pad = egui::vec2(
        (font_size * 0.42).clamp(6.0, 16.0),
        (font_size * 0.20).clamp(3.0, 9.0),
    );
    let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    if show_bg {
        painter.rect_filled(
            rect,
            5.0,
            egui::Color32::from_rgba_unmultiplied(10, 15, 24, 212),
        );
        painter.rect_stroke(
            rect,
            5.0,
            egui::Stroke::new(1.3, border_color),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, text_color);
    } else {
        // High-contrast crisp drop shadow when background pill is disabled
        let shadow_galley = painter.layout_no_wrap(
            text.to_string(),
            font,
            egui::Color32::from_rgba_unmultiplied(5, 10, 18, 225),
        );
        let origin = rect.min + pad;
        painter.galley(
            origin + egui::vec2(1.5, 1.5),
            shadow_galley,
            egui::Color32::from_rgba_unmultiplied(5, 10, 18, 225),
        );
        painter.galley(origin, galley, text_color);
    }
}

pub fn draw_catalog_drag_ghost(
    ctx: &egui::Context,
    pointer: egui::Pos2,
    entry: &twg_shared::models::ModelCatalogEntry,
    drop_lnglat: Option<(f64, f64)>,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("twg_catalog_drag_ghost"),
    ));
    let title = format!("[3D] {}", entry.name);
    let subtitle = if let Some((lng, lat)) = drop_lnglat {
        format!("Lepas untuk letakkan di peta ({:.5}, {:.5})", lat, lng)
    } else {
        "Tarik ke area peta 3D untuk meletakkan... (Esc batal)".to_string()
    };

    let title_galley = painter.layout_no_wrap(
        title,
        egui::FontId::proportional(12.0),
        egui::Color32::WHITE,
    );
    let sub_col = if drop_lnglat.is_some() {
        egui::Color32::from_rgb(110, 231, 183)
    } else {
        egui::Color32::from_rgb(186, 230, 253)
    };
    let sub_galley = painter.layout_no_wrap(subtitle, egui::FontId::proportional(10.5), sub_col);

    let pad = egui::vec2(10.0, 7.0);
    let width = title_galley.size().x.max(sub_galley.size().x) + pad.x * 2.0;
    let height = title_galley.size().y + 3.0 + sub_galley.size().y + pad.y * 2.0;
    let rect =
        egui::Rect::from_min_size(pointer + egui::vec2(16.0, 14.0), egui::vec2(width, height));

    let border = if drop_lnglat.is_some() {
        egui::Color32::from_rgb(16, 185, 129)
    } else {
        egui::Color32::from_rgb(56, 189, 248)
    };
    painter.rect_filled(
        rect,
        6.0,
        egui::Color32::from_rgba_unmultiplied(12, 19, 32, 238),
    );
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.5, border),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, title_galley, egui::Color32::WHITE);
    painter.galley(
        rect.min + egui::vec2(pad.x, pad.y + 16.0),
        sub_galley,
        sub_col,
    );
}

pub fn draw_basic_mesh_drag_ghost(
    ctx: &egui::Context,
    pointer: egui::Pos2,
    shape: BasicMeshShape,
    drop_lnglat: Option<(f64, f64)>,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("twg_basic_mesh_drag_ghost"),
    ));
    let title = format!("[{}] {}", shape.tag(), shape.label());
    let subtitle = if let Some((lng, lat)) = drop_lnglat {
        format!("Lepas untuk letakkan di peta ({:.5}, {:.5})", lat, lng)
    } else {
        "Tarik ke area peta 3D untuk meletakkan... (Esc batal)".to_string()
    };

    let title_galley = painter.layout_no_wrap(
        title,
        egui::FontId::proportional(12.0),
        egui::Color32::WHITE,
    );
    let sub_col = if drop_lnglat.is_some() {
        egui::Color32::from_rgb(110, 231, 183)
    } else {
        egui::Color32::from_rgb(196, 181, 253)
    };
    let sub_galley = painter.layout_no_wrap(subtitle, egui::FontId::proportional(10.5), sub_col);

    let pad = egui::vec2(10.0, 7.0);
    let width = title_galley.size().x.max(sub_galley.size().x) + pad.x * 2.0;
    let height = title_galley.size().y + 3.0 + sub_galley.size().y + pad.y * 2.0;
    let rect =
        egui::Rect::from_min_size(pointer + egui::vec2(16.0, 14.0), egui::vec2(width, height));

    let border = if drop_lnglat.is_some() {
        egui::Color32::from_rgb(16, 185, 129)
    } else {
        egui::Color32::from_rgb(139, 92, 246)
    };
    painter.rect_filled(
        rect,
        6.0,
        egui::Color32::from_rgba_unmultiplied(15, 20, 36, 238),
    );
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.5, border),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, title_galley, egui::Color32::WHITE);
    painter.galley(
        rect.min + egui::vec2(pad.x, pad.y + 16.0),
        sub_galley,
        sub_col,
    );
}

/// Draws Play Skenario RTS marquee selection box and a compact bottom-center tactical status
/// & Mouse/Trackpad control hint pill.
pub fn draw_play_rts_overlay(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    play_rts: &PlayRtsState,
) {
    if !playback.play_test_mode {
        return;
    }

    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("twg_play_rts_overlay"),
        ))
        .with_clip_rect(clip_rect);

    // 1. Marquee Box Selection rectangle
    if let Some((a, b)) = play_rts.marquee_drag {
        let rect = egui::Rect::from_two_pos(egui::pos2(a.x, a.y), egui::pos2(b.x, b.y))
            .intersect(clip_rect);
        if rect.width() >= 2.0 && rect.height() >= 2.0 {
            painter.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(16, 185, 129, 40),
            );
            painter.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(52, 211, 153)),
                egui::StrokeKind::Inside,
            );

            // Crisp tactical corner L-brackets
            let c_len = 8.0_f32.min(rect.width() * 0.35).min(rect.height() * 0.35);
            let corner_stroke = egui::Stroke::new(2.2, egui::Color32::from_rgb(167, 243, 208));
            let lt = rect.left_top();
            let rt = rect.right_top();
            let lb = rect.left_bottom();
            let rb = rect.right_bottom();
            painter.line_segment([lt, lt + egui::vec2(c_len, 0.0)], corner_stroke);
            painter.line_segment([lt, lt + egui::vec2(0.0, c_len)], corner_stroke);
            painter.line_segment([rt, rt + egui::vec2(-c_len, 0.0)], corner_stroke);
            painter.line_segment([rt, rt + egui::vec2(0.0, c_len)], corner_stroke);
            painter.line_segment([lb, lb + egui::vec2(c_len, 0.0)], corner_stroke);
            painter.line_segment([lb, lb + egui::vec2(0.0, -c_len)], corner_stroke);
            painter.line_segment([rb, rb + egui::vec2(-c_len, 0.0)], corner_stroke);
            painter.line_segment([rb, rb + egui::vec2(0.0, -c_len)], corner_stroke);
        }
    }

    // 2. Bottom-center RTS Selection / Controls Pill
    let (hint_text, border_col, text_col) = if let Some(gid) = play_rts.selected_gizmo_marker_id {
        let name = state
            .markers
            .iter()
            .find(|m| m.id == gid)
            .map(|m| m.name.as_str())
            .unwrap_or("Peralatan");
        (
            format!(
                "Peralatan: {name}  •  Drag panah X / Y (Alt) / Z atau lingkaran tengah (Free Move)  •  Esc batal"
            ),
            egui::Color32::from_rgba_unmultiplied(245, 158, 11, 210),
            egui::Color32::from_rgb(254, 240, 138),
        )
    } else if !play_rts.selected_unit_ids.is_empty() {
        let sel_cnt = play_rts.selected_unit_ids.len();
        let moving_cnt = play_rts
            .selected_unit_ids
            .iter()
            .filter(|uid| play_rts.unit_orders.get(uid).is_some_and(|o| o.moving))
            .count();
        let move_hint = if state.trackpad_mode {
            "Tap 2-Jari / Klik Kanan peta: Gerak Formasi"
        } else {
            "Klik Kanan peta: Gerak Formasi"
        };
        let detail = if moving_cnt > 0 {
            format!("{sel_cnt} Unit Terseleksi ({moving_cnt} bergerak)")
        } else {
            format!("{sel_cnt} Unit Terseleksi")
        };
        (
            format!("{detail}  •  {move_hint}  •  Shift+Klik: Tambah  •  Esc: Batal"),
            egui::Color32::from_rgba_unmultiplied(16, 185, 129, 215),
            egui::Color32::from_rgb(167, 243, 208),
        )
    } else if state.trackpad_mode {
        (
            "Trackpad: Drag Kiri = Box Select  •  Tap 2-Jari = Gerak  •  Scroll 2-Jari = Pan  •  Pinch = Zoom  •  Alt+Drag = Orbit"
                .to_string(),
            egui::Color32::from_rgba_unmultiplied(56, 189, 248, 165),
            egui::Color32::from_rgb(203, 213, 225),
        )
    } else {
        (
            "Drag Kiri = Box Select Unit  •  Klik Kanan = Gerak Formasi  •  Drag Tengah / Space+Kiri = Pan  •  Drag Kanan = Orbit"
                .to_string(),
            egui::Color32::from_rgba_unmultiplied(56, 189, 248, 165),
            egui::Color32::from_rgb(203, 213, 225),
        )
    };

    let font = egui::FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(hint_text, font, text_col);
    let pad = egui::vec2(12.0, 5.0);
    let center = egui::pos2(clip_rect.center().x, clip_rect.bottom() - 18.0);
    let pill_rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    painter.rect_filled(
        pill_rect,
        10.0,
        egui::Color32::from_rgba_unmultiplied(11, 18, 30, 222),
    );
    painter.rect_stroke(
        pill_rect,
        10.0,
        egui::Stroke::new(1.2, border_col),
        egui::StrokeKind::Inside,
    );
    painter.galley(pill_rect.min + pad, galley, text_col);
}

/// Draws the rotating **North Compass (Utara)** and realtime camera/performance telemetry
/// (**FPS**, **Zoom**, **Pitch**, **Bearing**) on **Layar Besar** and **Play Skenario Tactical**.
/// When `interactive` is true (Play Skenario Tactical), clicking the compass snaps Bearing to 0° (North).
pub fn draw_compass_and_telemetry_hud(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    tcam: &mut TacticalCamera,
    camera: &Camera,
    interactive: bool,
) {
    let viewport_h = camera
        .logical_viewport_rect()
        .map(|r| r.height())
        .unwrap_or(clip_rect.height())
        .max(200.0);
    let zoom = tcam.current_zoom(viewport_h, state.geo_anchor.cos_lat);
    let pitch = tcam.smooth_pitch;
    let bearing = tcam.smooth_bearing.rem_euclid(360.0);
    let fps = state.smoothed_fps;

    let hud_pos = egui::pos2(clip_rect.right() - 188.0, clip_rect.top() + 14.0);

    egui::Area::new(egui::Id::new("twg_compass_telemetry_hud"))
        .order(egui::Order::Foreground)
        .interactable(interactive)
        .fixed_pos(hud_pos)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(10, 16, 28, 224))
                .stroke(egui::Stroke::new(
                    1.2,
                    egui::Color32::from_rgba_unmultiplied(56, 189, 248, 175),
                ))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;

                        // 1. Rotating North Compass Dial
                        let dial_size = 62.0_f32;
                        let sense = if interactive {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        };
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(dial_size, dial_size), sense);
                        let p = ui.painter();
                        let c = rect.center();
                        let r = dial_size * 0.46;

                        p.circle_filled(
                            c,
                            r + 1.0,
                            egui::Color32::from_rgba_unmultiplied(15, 23, 42, 240),
                        );
                        p.circle_stroke(
                            c,
                            r,
                            egui::Stroke::new(
                                1.5,
                                if interactive && resp.hovered() {
                                    egui::Color32::from_rgb(253, 224, 71)
                                } else {
                                    egui::Color32::from_rgb(71, 85, 105)
                                },
                            ),
                        );

                        // Rotating cardinal ticks (angle = -bearing so Bearing=0° puts North at top)
                        let north_rad = -bearing.to_radians();
                        let rot_vec = |ang: f32, len: f32| -> egui::Vec2 {
                            egui::vec2(ang.sin() * len, -ang.cos() * len)
                        };

                        for k in 0..8 {
                            let a = north_rad + (k as f32) * std::f32::consts::FRAC_PI_4;
                            let is_cardinal = k % 2 == 0;
                            let inner_r = if is_cardinal { r * 0.76 } else { r * 0.85 };
                            let col = if k == 0 {
                                egui::Color32::from_rgb(248, 113, 113)
                            } else if is_cardinal {
                                egui::Color32::from_rgb(148, 163, 184)
                            } else {
                                egui::Color32::from_rgb(71, 85, 105)
                            };
                            p.line_segment(
                                [c + rot_vec(a, inner_r), c + rot_vec(a, r - 1.5)],
                                egui::Stroke::new(if is_cardinal { 1.4 } else { 1.0 }, col),
                            );
                        }

                        // North (Red) & South (Slate) diamond needle
                        let n_tip = c + rot_vec(north_rad, r * 0.72);
                        let s_tip = c - rot_vec(north_rad, r * 0.62);
                        let perp = egui::vec2(north_rad.cos(), north_rad.sin()) * 4.6;
                        p.add(egui::Shape::convex_polygon(
                            vec![n_tip, c + perp, c - perp],
                            egui::Color32::from_rgb(239, 68, 68),
                            egui::Stroke::NONE,
                        ));
                        p.add(egui::Shape::convex_polygon(
                            vec![s_tip, c + perp, c - perp],
                            egui::Color32::from_rgb(148, 163, 184),
                            egui::Stroke::NONE,
                        ));
                        p.circle_filled(c, 2.8, egui::Color32::WHITE);

                        // Crisp "U" (Utara) marker along the North direction
                        let u_pos = c + rot_vec(north_rad, r * 0.90);
                        p.circle_filled(
                            u_pos,
                            6.5,
                            egui::Color32::from_rgba_unmultiplied(15, 23, 42, 235),
                        );
                        p.text(
                            u_pos,
                            egui::Align2::CENTER_CENTER,
                            "U",
                            egui::FontId::proportional(9.5),
                            egui::Color32::from_rgb(248, 113, 113),
                        );

                        if interactive {
                            let resp = resp.on_hover_text(
                                "Kompas Utara — klik untuk mengembalikan arah kamera ke Utara (0°)",
                            );
                            if resp.clicked() {
                                tcam.bearing_deg = 0.0;
                            }
                        }

                        // 2. Telemetry Numeric Readouts: FPS, Zoom, Pitch, Bearing
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let fps_col = if fps >= 48.0 {
                                egui::Color32::from_rgb(74, 222, 128)
                            } else if fps >= 28.0 {
                                egui::Color32::from_rgb(250, 204, 21)
                            } else {
                                egui::Color32::from_rgb(248, 113, 113)
                            };
                            draw_telemetry_row(ui, "FPS", &format!("{:.0}", fps), fps_col);
                            draw_telemetry_row(
                                ui,
                                "Zoom",
                                &format!("{:.2}", zoom),
                                egui::Color32::from_rgb(125, 211, 252),
                            );
                            draw_telemetry_row(
                                ui,
                                "Pitch",
                                &format!("{:.1}°", pitch),
                                egui::Color32::from_rgb(226, 232, 240),
                            );
                            draw_telemetry_row(
                                ui,
                                "Bearing",
                                &format!("{:.1}°", bearing),
                                egui::Color32::from_rgb(253, 224, 71),
                            );
                        });
                    });
                });
        });
}

fn draw_telemetry_row(ui: &mut egui::Ui, label: &str, value: &str, value_col: egui::Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_sized(
            [44.0, 14.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            ),
        );
        ui.label(
            egui::RichText::new(value)
                .size(11.0)
                .strong()
                .monospace()
                .color(value_col),
        );
    });
}
