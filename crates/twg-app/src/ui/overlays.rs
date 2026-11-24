use bevy::prelude::*;
use bevy_egui::egui;

use crate::{
    scene::{
        basic_mesh::BasicMeshShape,
        markers::MarkerWorldBounds,
        parse_hex_color,
        playback::PlaybackOverrides,
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
            if let Ok(screen) = camera.world_to_viewport(cam_gtf, Vec3::from_array(local)) {
                let pos = egui::pos2(screen.x, screen.y);
                if clip_rect.contains(pos) {
                    let c = parse_hex_color(&tm.color, 1.0).to_srgba();
                    let color = egui::Color32::from_rgb(
                        (c.red * 255.0) as u8,
                        (c.green * 255.0) as u8,
                        (c.blue * 255.0) as u8,
                    );
                    draw_badge(&painter, pos, &tm.text, color, tm.font_size as f32);
                }
            }
        }
    }

    // Hide all unit/object name labels during Play Skenario (keep only placed TextMarkers)
    if state.show_unit_labels && !playback.play_test_mode {
        for m in &state.markers {
            if !m.is_spawn {
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
                Vec3::from_array(
                    state
                        .geo_anchor
                        .lng_lat_alt_to_local(lng, lat, alt + 10.0),
                )
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
                    );
                }
            }
        }
    }

    // Play Skenario Mode viewport border & floating HUD badge
    if playback.play_test_mode {
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
            ToolMode::MoveMarker => if has_selected_marker {
                "Mode Move (2): Drag panah X / Y (Alt) / Z atau lingkaran tengah untuk Free Move".to_string()
            } else {
                "Mode Move (2): Klik objek 3D di peta untuk menampilkan Gizmo Move".to_string()
            },
            ToolMode::RotateMarker => if has_selected_marker {
                "Mode Rotate (3): Drag cincin X / Y / Z atau bola tengah untuk Free Rotate".to_string()
            } else {
                "Mode Rotate (3): Klik objek 3D di peta untuk menampilkan Gizmo Rotate".to_string()
            },
            ToolMode::ScaleMarker => if has_selected_marker {
                "Mode Scale (4): Drag kubus X / Y / Z atau kubus tengah untuk Free Scale (X, Y, Z)".to_string()
            } else {
                "Mode Scale (4): Klik objek 3D di peta untuk menampilkan Gizmo Scale".to_string()
            },
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
            ToolMode::PickRouteMarker | ToolMode::PickShootMarker => String::new(),
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
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(56, 189, 248, 160)),
                egui::StrokeKind::Inside,
            );
            painter.galley(rect.min + pad, galley, egui::Color32::from_rgb(186, 230, 253));
        }
    }
}

fn draw_badge(
    painter: &egui::Painter,
    center: egui::Pos2,
    text: &str,
    text_color: egui::Color32,
    font_size: f32,
) {
    let font = egui::FontId::proportional(font_size.clamp(10.0, 22.0));
    let galley = painter.layout_no_wrap(text.to_string(), font, text_color);
    let pad = egui::vec2(6.0, 3.0);
    let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    painter.rect_filled(
        rect,
        4.0,
        egui::Color32::from_rgba_unmultiplied(10, 15, 24, 195),
    );
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(56, 189, 248, 120)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, text_color);
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
    let rect = egui::Rect::from_min_size(pointer + egui::vec2(16.0, 14.0), egui::vec2(width, height));

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
    let rect = egui::Rect::from_min_size(pointer + egui::vec2(16.0, 14.0), egui::vec2(width, height));

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
