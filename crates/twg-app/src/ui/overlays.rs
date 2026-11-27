use bevy::prelude::*;
use bevy_egui::egui;

use crate::{
    map::camera::TacticalCamera,
    scene::{
        basic_mesh::BasicMeshShape, gizmo::compute_gizmo_world_radius,
        markers::MarkerWorldBounds, parse_hex_color, play_rts::PlayRtsState,
        playback::PlaybackOverrides, pointer_arrow::PointerArrowVisualPool,
        tactical_arrow::TacticalArrowVisualPool,
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
    tactical_arrow_pool: &TacticalArrowVisualPool,
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

    // Hide all unit/object name labels during Play Skenario & Layar Besar (keep placed TextMarkers).
    // Performance cap: when a scenario contains hundreds/thousands of units, only project the nearest
    // 100 unit badges around the camera so egui doesn't lay out 10,000 overlapping text galleys/frame.
    if state.show_unit_labels && !playback.play_test_mode && !state.is_big_screen() {
        const MAX_UNIT_LABELS: usize = 100;
        let cam_pos = cam_gtf.translation();
        let mut drawn_labels = 0usize;
        let high_count = state.markers.len() > MAX_UNIT_LABELS;
        let max_dist_sq = 180.0_f32 * 180.0_f32;
        for m in &state.markers {
            if drawn_labels >= MAX_UNIT_LABELS {
                break;
            }
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
            if high_count && world_pos.distance_squared(cam_pos) > max_dist_sq {
                continue;
            }
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
                    drawn_labels += 1;
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

    // Active 3D Tactical Flow Arrow floating midpoint labels
    for lbl in &tactical_arrow_pool.active_labels {
        if let Ok(screen) = camera.world_to_viewport(cam_gtf, lbl.world_pos) {
            let pos = egui::pos2(screen.x, screen.y);
            if clip_rect.contains(pos) {
                let tc = parse_hex_color(&lbl.accent_hex, 1.0).to_srgba();
                let alpha_u8 = (lbl.alpha.clamp(0.0, 1.0) * 255.0) as u8;
                let text_col = egui::Color32::from_rgba_unmultiplied(
                    (tc.red * 255.0) as u8,
                    (tc.green * 255.0) as u8,
                    (tc.blue * 255.0) as u8,
                    alpha_u8,
                );
                let bc = parse_hex_color(&lbl.color_hex, 1.0).to_srgba();
                let border_col = egui::Color32::from_rgba_unmultiplied(
                    (bc.red * 255.0) as u8,
                    (bc.green * 255.0) as u8,
                    (bc.blue * 255.0) as u8,
                    (lbl.alpha.clamp(0.0, 1.0) * 230.0) as u8,
                );
                draw_pointer_label(&painter, pos, &lbl.text, text_col, border_col, 13.0, true);
            }
        }
    }

    // Active 3D Speech Bubble Dialogs (Play Skenario / Preview AND Plot Editor live line preview)
    {
        use twg_shared::dialog_anim::{is_dialog_anim, parse_dialog_anim_config};
        let playback_active = playback.is_playback_active();
        let mut bubbles_to_draw: Vec<(
            twg_shared::dialog_anim::DialogAnimConfig,
            twg_shared::dialog_anim::ActiveDialogSample,
        )> = Vec::new();

        if playback_active {
            for anim in &state.anims {
                if !is_dialog_anim(anim) || state.hidden_entity_ids.contains(&anim.id) {
                    continue;
                }
                let status = playback.track_status(anim.id);
                if status != AnimRunStatus::Playing && status != AnimRunStatus::Paused {
                    continue;
                }
                let elapsed = playback
                    .tracks
                    .get(&anim.id)
                    .map(|t| t.distance_m)
                    .unwrap_or(0.0);
                let cfg = parse_dialog_anim_config(anim);
                if let Some(sample) = cfg.evaluate_at(elapsed) {
                    bubbles_to_draw.push((cfg, sample));
                }
            }
        } else if let Some(edit_anim) = state.anim_edit.as_ref()
            && state.selected == crate::SelectedEntity::Anim(edit_anim.id)
            && is_dialog_anim(edit_anim)
            && !state.hidden_entity_ids.contains(&edit_anim.id)
        {
            let cfg = parse_dialog_anim_config(edit_anim);
            let idx = state
                .dialog_preview_line_idx
                .filter(|(aid, _)| *aid == edit_anim.id)
                .map(|(_, i)| i)
                .unwrap_or(0);
            if let Some(sample) = cfg
                .preview_sample_for_line(idx)
                .or_else(|| cfg.preview_sample_for_line(0))
            {
                bubbles_to_draw.push((cfg, sample));
            }
        }

        for (cfg, sample) in bubbles_to_draw {
            let Some(marker) = state.markers.iter().find(|m| m.id == sample.marker_id) else {
                continue;
            };
            if !overrides.is_marker_visible(marker, playback_active) {
                continue;
            }
            let head_world = if let Some(b) = marker_bounds.bounds.get(&marker.id) {
                Vec3::new(
                    b.footprint_center.x,
                    b.world_max.y + cfg.height_offset_m as f32,
                    b.footprint_center.z,
                )
            } else {
                let (lng, lat, alt) = if let Some(ov) = overrides.markers.get(&marker.id) {
                    (ov.lng, ov.lat, ov.altitude)
                } else {
                    (marker.lng, marker.lat, marker.altitude)
                };
                Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(
                    lng,
                    lat,
                    alt + 2.2 + cfg.height_offset_m,
                ))
            };

            if let Ok(screen) = camera.world_to_viewport(cam_gtf, head_world) {
                let anchor_pos = egui::pos2(screen.x, screen.y);
                if clip_rect.expand(120.0).contains(anchor_pos) {
                    let speaker_name = if cfg.show_speaker_name {
                        let nm = if !sample.speaker_override.is_empty() {
                            sample.speaker_override.clone()
                        } else {
                            marker.name.clone()
                        };
                        Some(nm)
                    } else {
                        None
                    };
                    draw_speech_bubble(
                        &painter,
                        clip_rect,
                        anchor_pos,
                        speaker_name.as_deref(),
                        &sample.visible_text,
                        &sample.text_color,
                        &sample.bg_color,
                        cfg.bg_opacity,
                        &sample.border_color,
                        cfg.font_size,
                        cfg.max_width_px,
                        sample.alpha,
                    );
                }
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
            ToolMode::DrawGroundPolygon => format!(
                "Mode Draw Ground Polygon (Datar): {} titik — Min 3 titik, klik titik #1 / Enter selesai, Esc batal",
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
                "Gambar Jalur {} ({}): {} titik — Min 2 titik, Enter selesai, Backspace undo, Esc batal",
                if state.drawing_traffic_is_signal {
                    "Lampu Merah"
                } else {
                    "Traffic"
                },
                if state.drawing_traffic_lane_for_anim_id.is_some() {
                    "Tambah Jalur"
                } else {
                    "Jalur 1"
                },
                state.draft_points_lnglat.len()
            ),
            ToolMode::PlaceTrafficLight => format!(
                "Mode Pasang Lampu Merah 3D (Fase {}): Klik di atas garis jalur untuk menempelkan lampu — Esc selesai",
                state.placing_traffic_light_phase_idx + 1
            ),
            ToolMode::PlaceBillboard => format!(
                "Mode Pasang Billboard / Videotron 3D ({}): Klik lokasi di peta untuk menempatkan — Esc batal",
                state.new_billboard_name
            ),
            ToolMode::DrawWaterPolygon => format!(
                "Mode Gambar Area Air ({}): Klik minimal 3 titik ({} titik) — Klik titik #1 atau Enter selesai, Esc batal",
                state.new_water_name,
                state.draft_points_lnglat.len()
            ),
            ToolMode::DrawTacticalArrow => format!(
                "Mode Gambar Panah Taktis ({}): Klik minimal 2 titik ({} titik) — Enter selesai, Backspace undo, Esc batal",
                state.new_tactical_arrow_name,
                state.draft_points_lnglat.len()
            ),
            ToolMode::PlaceRainZone => format!(
                "Mode Zona Hujan Lokal ({}): Klik lokasi pusat hujan di peta — Esc batal",
                state.new_rain_name
            ),
            ToolMode::DrawTrafficQueueLane => format!(
                "Gambar {} ({} titik) — Min 2 titik, Enter selesai, Backspace undo, Esc batal",
                match state.drawing_traffic_queue_target {
                    Some((_, Some(_))) => "Jalur Lancar (Wajib)",
                    Some((_, None)) => "Jalur Antre Baru",
                    None if state.drawing_traffic_queue_is_gridlock => "Jalur Antre A (Macet Total)",
                    None if state.drawing_traffic_queue_is_manual => "Jalur Antre A (Simpang Manual)",
                    None => "Jalur Antre A (Urai Simpang)",
                },
                state.draft_points_lnglat.len()
            ),
            ToolMode::DrawTrafficDiversionLane => format!(
                "Gambar {} ({} titik) — Min 2 titik, Enter selesai, Backspace undo, Esc batal",
                match state.drawing_diversion_target {
                    Some((_, Some((_, 1)))) => "Jalur Utama / Normal (Pengalihan Arus)",
                    Some((_, Some((_, 2)))) => "Jalur Alternatif / Pengalihan",
                    Some((_, None)) => "Jalur Pendekat Baru (Pengalihan Arus)",
                    _ => "Jalur Pendekat (Pengalihan Arus)",
                },
                state.draft_points_lnglat.len()
            ),
            ToolMode::PickRouteMarker
            | ToolMode::PickShootMarker
            | ToolMode::PickPointerMarker
            | ToolMode::PickSpawnHideMarker
            | ToolMode::PickCameraFollowMarker
            | ToolMode::PickModelActionMarker
            | ToolMode::PickDialogSpeaker => String::new(),
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

/// Draws a crisp 3D-projected speech bubble above a speaking marker with drop shadow,
/// rounded background, colored border, optional speaker header, wrapped dialog text,
/// and a downward-pointing speech tail pointing straight toward `anchor_pos`.
#[allow(clippy::too_many_arguments)]
fn draw_speech_bubble(
    painter: &egui::Painter,
    clip_rect: egui::Rect,
    anchor_pos: egui::Pos2,
    speaker_name: Option<&str>,
    text: &str,
    text_color_hex: &str,
    bg_color_hex: &str,
    bg_opacity: f32,
    border_color_hex: &str,
    font_size: f32,
    max_width_px: f32,
    alpha: f32,
) {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }

    let parse_rgba = |hex: &str, op: f32| {
        let c = parse_hex_color(hex, 1.0).to_srgba();
        egui::Color32::from_rgba_unmultiplied(
            (c.red * 255.0) as u8,
            (c.green * 255.0) as u8,
            (c.blue * 255.0) as u8,
            ((op.clamp(0.0, 1.0) * a) * 255.0).round() as u8,
        )
    };

    let text_col = parse_rgba(text_color_hex, 1.0);
    let bg_col = parse_rgba(bg_color_hex, bg_opacity);
    let border_col = parse_rgba(border_color_hex, 0.95);
    let shadow_col = egui::Color32::from_rgba_unmultiplied(0, 0, 0, (110.0 * a) as u8);

    let wrap_w = max_width_px.clamp(140.0, 600.0);
    let body_font = egui::FontId::proportional(font_size.clamp(10.0, 36.0));
    let body_galley = painter.layout(text.to_string(), body_font, text_col, wrap_w);

    let header_galley = speaker_name.map(|name| {
        let hdr_font = egui::FontId::proportional((font_size * 0.84).clamp(9.5, 24.0));
        painter.layout(name.to_string(), hdr_font, border_col, wrap_w)
    });

    let pad_x = 12.0_f32;
    let pad_y = 8.0_f32;
    let header_gap = if header_galley.is_some() { 3.0 } else { 0.0 };
    let header_h = header_galley.as_ref().map(|g| g.size().y).unwrap_or(0.0);
    let header_w = header_galley.as_ref().map(|g| g.size().x).unwrap_or(0.0);

    let content_w = body_galley.size().x.max(header_w).max(40.0);
    let content_h = header_h + header_gap + body_galley.size().y;
    let box_w = content_w + pad_x * 2.0;
    let box_h = content_h + pad_y * 2.0;

    let tail_h = 11.0_f32;
    let tail_half_w = 8.0_f32;
    let tip_y = anchor_pos.y - 4.0;
    let box_bottom = tip_y - tail_h;
    let box_top = box_bottom - box_h;

    // Keep bubble inside viewport horizontally while keeping tail tip pointing at speaker
    let desired_left = anchor_pos.x - box_w * 0.5;
    let min_left = clip_rect.left() + 8.0;
    let max_left = (clip_rect.right() - 8.0 - box_w).max(min_left);
    let box_left = desired_left.clamp(min_left, max_left);
    let bubble_rect = egui::Rect::from_min_size(
        egui::pos2(box_left, box_top),
        egui::vec2(box_w, box_h),
    );

    let corner_r = 9.0_f32;
    let tail_base_x = anchor_pos
        .x
        .clamp(bubble_rect.left() + corner_r + tail_half_w, bubble_rect.right() - corner_r - tail_half_w);
    let tail_tip = egui::pos2(
        anchor_pos.x.clamp(clip_rect.left() + 6.0, clip_rect.right() - 6.0),
        tip_y,
    );
    let tail_left = egui::pos2(tail_base_x - tail_half_w, bubble_rect.bottom() - 1.0);
    let tail_right = egui::pos2(tail_base_x + tail_half_w, bubble_rect.bottom() - 1.0);

    // 1. Drop shadow
    painter.rect_filled(
        bubble_rect.translate(egui::vec2(0.0, 3.0)),
        corner_r,
        shadow_col,
    );

    // 2. Main bubble box + border
    painter.rect_filled(bubble_rect, corner_r, bg_col);
    painter.rect_stroke(
        bubble_rect,
        corner_r,
        egui::Stroke::new(1.6, border_col),
        egui::StrokeKind::Outside,
    );

    // 3. Seamless downward speech tail pointing at speaker's head
    painter.add(egui::Shape::convex_polygon(
        vec![tail_left, tail_right, tail_tip],
        bg_col,
        egui::Stroke::NONE,
    ));
    painter.line_segment([tail_left, tail_tip], egui::Stroke::new(1.6, border_col));
    painter.line_segment([tail_right, tail_tip], egui::Stroke::new(1.6, border_col));

    // 4. Speaker name header + wrapped dialog text
    let mut text_y = bubble_rect.top() + pad_y;
    if let Some(hg) = header_galley {
        painter.galley(egui::pos2(bubble_rect.left() + pad_x, text_y), hg, border_col);
        text_y += header_h + header_gap;
    }
    painter.galley(
        egui::pos2(bubble_rect.left() + pad_x, text_y),
        body_galley,
        text_col,
    );
}

/// Renders active broadcast-style **HUD Banners** (Title Card, Lower-Third, and Running Text / Ticker)
/// cleanly over the 3D viewport in Play Skenario, Layar Besar, and live Plot Editor preview.
pub fn draw_hud_banners(
    ctx: &egui::Context,
    clip_rect: egui::Rect,
    state: &AppState,
    playback: &PlaybackState,
    wall_time_sec: f64,
) {
    use twg_shared::hud_banner_anim::{
        HudBannerConfig, HudBannerEval, HudBannerPosition, HudBannerStyle, is_hud_banner_anim,
        parse_hud_banner_config,
    };

    let playback_active = playback.is_playback_active();
    let mut active_banners: Vec<(HudBannerConfig, HudBannerEval)> = Vec::new();

    if playback_active {
        for anim in &state.anims {
            if !is_hud_banner_anim(anim) || state.hidden_entity_ids.contains(&anim.id) {
                continue;
            }
            let status = playback.track_status(anim.id);
            if status != AnimRunStatus::Playing && status != AnimRunStatus::Paused {
                continue;
            }
            let elapsed = playback
                .tracks
                .get(&anim.id)
                .map(|t| t.distance_m)
                .unwrap_or(0.0);
            let cfg = parse_hud_banner_config(anim);
            let eval = cfg.evaluate_at(elapsed);
            if eval.visible {
                active_banners.push((cfg, eval));
            }
        }
    } else if let Some(edit_anim) = state.anim_edit.as_ref()
        && state.selected == crate::SelectedEntity::Anim(edit_anim.id)
        && is_hud_banner_anim(edit_anim)
        && !state.hidden_entity_ids.contains(&edit_anim.id)
    {
        let cfg = parse_hud_banner_config(edit_anim);
        let eval = cfg.preview_eval(wall_time_sec);
        if eval.visible {
            active_banners.push((cfg, eval));
        }
    }

    if active_banners.is_empty() {
        return;
    }

    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("twg_hud_banners"),
        ))
        .with_clip_rect(clip_rect);

    let has_running_text = active_banners
        .iter()
        .any(|(c, _)| c.style == HudBannerStyle::RunningText);
    let bottom_ticker_reserve = if has_running_text { 46.0 } else { 0.0 };

    let mut top_stack_y = clip_rect.top() + 26.0;
    let mut bottom_left_y = clip_rect.bottom() - 24.0 - bottom_ticker_reserve;
    let mut bottom_center_y = clip_rect.bottom() - 24.0 - bottom_ticker_reserve;
    let mut center_stack_offset = 0.0_f32;

    for (cfg, eval) in active_banners {
        let alpha = eval.alpha.clamp(0.0, 1.0);
        let slide = (1.0 - eval.enter_progress.clamp(0.0, 1.0)) * 18.0;

        let acc_s = parse_hex_color(&cfg.accent_color, 1.0).to_srgba();
        let bg_s = parse_hex_color(&cfg.bg_color, 1.0).to_srgba();
        let txt_s = parse_hex_color(&cfg.text_color, 1.0).to_srgba();

        let accent_col = egui::Color32::from_rgba_unmultiplied(
            (acc_s.red * 255.0) as u8,
            (acc_s.green * 255.0) as u8,
            (acc_s.blue * 255.0) as u8,
            (alpha * 255.0) as u8,
        );
        let bg_col = egui::Color32::from_rgba_unmultiplied(
            (bg_s.red * 255.0) as u8,
            (bg_s.green * 255.0) as u8,
            (bg_s.blue * 255.0) as u8,
            (alpha * cfg.bg_opacity * 255.0) as u8,
        );
        let text_col = egui::Color32::from_rgba_unmultiplied(
            (txt_s.red * 255.0) as u8,
            (txt_s.green * 255.0) as u8,
            (txt_s.blue * 255.0) as u8,
            (alpha * 255.0) as u8,
        );
        let sub_col = egui::Color32::from_rgba_unmultiplied(
            (txt_s.red * 215.0) as u8,
            (txt_s.green * 222.0) as u8,
            (txt_s.blue * 232.0) as u8,
            (alpha * 220.0) as u8,
        );
        let shadow_col = egui::Color32::from_rgba_unmultiplied(0, 0, 0, (alpha * 115.0) as u8);

        match cfg.style {
            HudBannerStyle::RunningText => {
                let bar_h = (36.0 * cfg.font_scale).clamp(28.0, 56.0);
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(clip_rect.left(), clip_rect.bottom() - bar_h + slide),
                    egui::pos2(clip_rect.right(), clip_rect.bottom() + slide),
                );
                painter.rect_filled(bar_rect, 0.0, bg_col);
                painter.line_segment(
                    [bar_rect.left_top(), bar_rect.right_top()],
                    egui::Stroke::new(2.0, accent_col),
                );

                // Left label badge ("SITUASI LIVE")
                let badge_txt = if cfg.badge_text.trim().is_empty() {
                    "INFO".to_string()
                } else {
                    cfg.badge_text.trim().to_uppercase()
                };
                let badge_font = egui::FontId::proportional((12.5 * cfg.font_scale).clamp(10.0, 22.0));
                let badge_galley = painter.layout_no_wrap(
                    badge_txt,
                    badge_font,
                    egui::Color32::from_rgba_unmultiplied(10, 15, 25, (alpha * 255.0) as u8),
                );
                let badge_w = badge_galley.size().x + 22.0;
                let badge_rect = egui::Rect::from_min_size(
                    bar_rect.left_top(),
                    egui::vec2(badge_w, bar_h),
                );
                painter.rect_filled(badge_rect, 0.0, accent_col);
                painter.galley(
                    egui::pos2(
                        badge_rect.left() + 11.0,
                        badge_rect.center().y - badge_galley.size().y * 0.5,
                    ),
                    badge_galley,
                    egui::Color32::from_rgba_unmultiplied(10, 15, 25, (alpha * 255.0) as u8),
                );

                // Seamless horizontal scrolling text in the remainder of the bar
                let track_rect = egui::Rect::from_min_max(
                    egui::pos2(badge_rect.right() + 6.0, bar_rect.top()),
                    egui::pos2(bar_rect.right() - 8.0, bar_rect.bottom()),
                );
                if track_rect.width() > 40.0 {
                    let ticker_painter = painter.with_clip_rect(track_rect);
                    let font = egui::FontId::proportional((14.0 * cfg.font_scale).clamp(11.0, 24.0));
                    let item = format!("{}     •     ", cfg.ticker_text.trim());
                    let g = ticker_painter.layout_no_wrap(item.clone(), font.clone(), text_col);
                    let seg_w = g.size().x.max(120.0);
                    let offset = ((eval.scroll_sec * cfg.ticker_speed_px as f64) % (seg_w as f64)) as f32;
                    let text_y = track_rect.center().y - g.size().y * 0.5;
                    let mut x = track_rect.left() - offset;
                    while x < track_rect.right() {
                        let g_copy = ticker_painter.layout_no_wrap(item.clone(), font.clone(), text_col);
                        ticker_painter.galley(egui::pos2(x, text_y), g_copy, text_col);
                        x += seg_w;
                    }
                }
            }
            HudBannerStyle::TitleCard => {
                let max_w = (clip_rect.width() * 0.72).clamp(280.0, 720.0);
                let pad_x = 22.0;
                let pad_y = 14.0;
                let badge_font = egui::FontId::proportional((11.5 * cfg.font_scale).clamp(9.5, 20.0));
                let title_font = egui::FontId::proportional((21.0 * cfg.font_scale).clamp(14.0, 36.0));
                let sub_font = egui::FontId::proportional((13.0 * cfg.font_scale).clamp(10.5, 22.0));

                let badge_g = if !cfg.badge_text.trim().is_empty() {
                    Some(painter.layout_no_wrap(
                        cfg.badge_text.trim().to_uppercase(),
                        badge_font,
                        accent_col,
                    ))
                } else {
                    None
                };
                let title_g = if !cfg.title_text.trim().is_empty() {
                    Some(painter.layout(
                        cfg.title_text.trim().to_string(),
                        title_font,
                        text_col,
                        max_w - pad_x * 2.0,
                    ))
                } else {
                    None
                };
                let sub_g = if !cfg.subtitle_text.trim().is_empty() {
                    Some(painter.layout(
                        cfg.subtitle_text.trim().to_string(),
                        sub_font,
                        sub_col,
                        max_w - pad_x * 2.0,
                    ))
                } else {
                    None
                };

                let content_w = [
                    badge_g.as_ref().map(|g| g.size().x).unwrap_or(0.0),
                    title_g.as_ref().map(|g| g.size().x).unwrap_or(0.0),
                    sub_g.as_ref().map(|g| g.size().x).unwrap_or(0.0),
                ]
                .into_iter()
                .fold(220.0_f32, f32::max);
                let mut content_h = 0.0_f32;
                if let Some(g) = &badge_g {
                    content_h += g.size().y + 5.0;
                }
                if let Some(g) = &title_g {
                    content_h += g.size().y;
                }
                if let Some(g) = &sub_g {
                    content_h += 5.0 + g.size().y;
                }

                let box_w = (content_w + pad_x * 2.0).min(max_w);
                let box_h = content_h + pad_y * 2.0;

                let rect = match cfg.position {
                    HudBannerPosition::TopCenter => {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(clip_rect.center().x, top_stack_y + box_h * 0.5 - slide),
                            egui::vec2(box_w, box_h),
                        );
                        top_stack_y += box_h + 10.0;
                        r
                    }
                    HudBannerPosition::Center => {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(
                                clip_rect.center().x,
                                clip_rect.center().y + center_stack_offset - slide,
                            ),
                            egui::vec2(box_w, box_h),
                        );
                        center_stack_offset += box_h + 10.0;
                        r
                    }
                    HudBannerPosition::BottomLeft => {
                        let r = egui::Rect::from_min_size(
                            egui::pos2(clip_rect.left() + 24.0 - slide, bottom_left_y - box_h),
                            egui::vec2(box_w, box_h),
                        );
                        bottom_left_y -= box_h + 10.0;
                        r
                    }
                    HudBannerPosition::BottomCenter => {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(
                                clip_rect.center().x,
                                bottom_center_y - box_h * 0.5 + slide,
                            ),
                            egui::vec2(box_w, box_h),
                        );
                        bottom_center_y -= box_h + 10.0;
                        r
                    }
                };

                painter.rect_filled(rect.translate(egui::vec2(0.0, 4.0)), 10.0, shadow_col);
                painter.rect_filled(rect, 10.0, bg_col);
                painter.rect_stroke(
                    rect,
                    10.0,
                    egui::Stroke::new(1.5, accent_col),
                    egui::StrokeKind::Inside,
                );
                // Top accent bar
                let accent_bar = egui::Rect::from_min_size(
                    rect.left_top() + egui::vec2(14.0, 0.0),
                    egui::vec2((rect.width() - 28.0).max(20.0), 3.5),
                );
                painter.rect_filled(accent_bar, 2.0, accent_col);

                let mut y = rect.top() + pad_y;
                if let Some(g) = badge_g {
                    let x = rect.center().x - g.size().x * 0.5;
                    let gh = g.size().y;
                    painter.galley(egui::pos2(x, y), g, accent_col);
                    y += gh + 5.0;
                }
                if let Some(g) = title_g {
                    let x = rect.center().x - g.size().x * 0.5;
                    let gh = g.size().y;
                    painter.galley(egui::pos2(x, y), g, text_col);
                    y += gh + 5.0;
                }
                if let Some(g) = sub_g {
                    let x = rect.center().x - g.size().x * 0.5;
                    painter.galley(egui::pos2(x, y), g, sub_col);
                }
            }
            HudBannerStyle::LowerThird => {
                let max_w = (clip_rect.width() * 0.56).clamp(250.0, 560.0);
                let pad_left = 18.0;
                let pad_right = 18.0;
                let pad_y = 10.0;
                let badge_font = egui::FontId::proportional((10.5 * cfg.font_scale).clamp(9.0, 18.0));
                let title_font = egui::FontId::proportional((16.5 * cfg.font_scale).clamp(12.5, 28.0));
                let sub_font = egui::FontId::proportional((12.0 * cfg.font_scale).clamp(10.0, 20.0));

                let badge_g = if !cfg.badge_text.trim().is_empty() {
                    Some(painter.layout_no_wrap(
                        cfg.badge_text.trim().to_uppercase(),
                        badge_font,
                        accent_col,
                    ))
                } else {
                    None
                };
                let title_g = if !cfg.title_text.trim().is_empty() {
                    Some(painter.layout(
                        cfg.title_text.trim().to_string(),
                        title_font,
                        text_col,
                        max_w - pad_left - pad_right,
                    ))
                } else {
                    None
                };
                let sub_g = if !cfg.subtitle_text.trim().is_empty() {
                    Some(painter.layout(
                        cfg.subtitle_text.trim().to_string(),
                        sub_font,
                        sub_col,
                        max_w - pad_left - pad_right,
                    ))
                } else {
                    None
                };

                let content_w = [
                    badge_g.as_ref().map(|g| g.size().x).unwrap_or(0.0),
                    title_g.as_ref().map(|g| g.size().x).unwrap_or(0.0),
                    sub_g.as_ref().map(|g| g.size().x).unwrap_or(0.0),
                ]
                .into_iter()
                .fold(180.0_f32, f32::max);
                let mut content_h = 0.0_f32;
                if let Some(g) = &badge_g {
                    content_h += g.size().y + 3.0;
                }
                if let Some(g) = &title_g {
                    content_h += g.size().y;
                }
                if let Some(g) = &sub_g {
                    content_h += 3.0 + g.size().y;
                }

                let box_w = (content_w + pad_left + pad_right).min(max_w);
                let box_h = content_h + pad_y * 2.0;

                let rect = match cfg.position {
                    HudBannerPosition::TopCenter => {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(clip_rect.center().x, top_stack_y + box_h * 0.5 - slide),
                            egui::vec2(box_w, box_h),
                        );
                        top_stack_y += box_h + 10.0;
                        r
                    }
                    HudBannerPosition::Center => {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(
                                clip_rect.center().x,
                                clip_rect.center().y + center_stack_offset - slide,
                            ),
                            egui::vec2(box_w, box_h),
                        );
                        center_stack_offset += box_h + 10.0;
                        r
                    }
                    HudBannerPosition::BottomCenter => {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(
                                clip_rect.center().x,
                                bottom_center_y - box_h * 0.5 + slide,
                            ),
                            egui::vec2(box_w, box_h),
                        );
                        bottom_center_y -= box_h + 10.0;
                        r
                    }
                    HudBannerPosition::BottomLeft => {
                        let r = egui::Rect::from_min_size(
                            egui::pos2(clip_rect.left() + 24.0 - slide, bottom_left_y - box_h),
                            egui::vec2(box_w, box_h),
                        );
                        bottom_left_y -= box_h + 10.0;
                        r
                    }
                };

                painter.rect_filled(rect.translate(egui::vec2(0.0, 3.0)), 6.0, shadow_col);
                painter.rect_filled(rect, 6.0, bg_col);
                // Thick vertical broadcast accent bar on the left edge
                let left_bar = egui::Rect::from_min_size(
                    rect.left_top(),
                    egui::vec2(5.5, rect.height()),
                );
                painter.rect_filled(left_bar, 3.0, accent_col);

                let mut y = rect.top() + pad_y;
                let x = rect.left() + pad_left;
                if let Some(g) = badge_g {
                    let gh = g.size().y;
                    painter.galley(egui::pos2(x, y), g, accent_col);
                    y += gh + 3.0;
                }
                if let Some(g) = title_g {
                    let gh = g.size().y;
                    painter.galley(egui::pos2(x, y), g, text_col);
                    y += gh + 3.0;
                }
                if let Some(g) = sub_g {
                    painter.galley(egui::pos2(x, y), g, sub_col);
                }
            }
        }
    }
}
