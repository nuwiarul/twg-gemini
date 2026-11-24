use bevy_egui::egui;
use twg_shared::{
    api::{
        CreateMarkerRequest, UpdateAnimRequest, UpdateBuildingRequest, UpdateMarkerRequest,
        UpdatePlotLineRequest, UpdateSkenarioRequest, UpdateTextMarkerRequest,
    },
    models::{BaseLayer, MapMode, MediaAssetType},
    ws::TacticalCommand,
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    net::{
        client::{ApiClientBridge, ApiCommand},
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::{
        basic_mesh::{
            BasicMeshConfig, BasicMeshMaterialKind, BasicMeshShape, is_basic_mesh_marker,
            parse_basic_mesh_config, write_basic_mesh_config,
        },
        markers::{MAX_OBJECT_3D_SCALE, MIN_OBJECT_3D_SCALE, current_auth, finish_active_draft},
        media_preview::MediaPreviewPlayer,
        polygon_edit::{
            PolygonEditState, ensure_editable_polygon_points, insert_midpoint_after,
        },
    },
    state::{AppState, PlaybackState, SelectedEntity, SkyboxPreset, ToolMode},
    ui::spinner::{SpinnerConfig, numeric_spinner, numeric_spinner_i32, numeric_spinner_u32_w},
};

#[allow(clippy::too_many_arguments)]
pub fn draw_right_panel(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    poly_edit: &mut PolygonEditState,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) -> f32 {
    egui::Panel::right("editor_right_inspector")
        .resizable(true)
        .default_size(325.0)
        .min_size(270.0)
        .max_size(460.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 23, 35))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::same(10)),
        )
        .show(viewport_ui, |ui| {
            // Inspector Title Header (Godot / Unity Style)
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Inspector")
                        .size(14.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let badge = match state.selected {
                        SelectedEntity::Marker(id) => {
                            let is_bm = state
                                .markers
                                .iter()
                                .find(|m| m.id == id)
                                .is_some_and(is_basic_mesh_marker);
                            if is_bm {
                                ("BASIC MESH", egui::Color32::from_rgb(109, 40, 217))
                            } else {
                                ("3D MARKER", egui::Color32::from_rgb(14, 116, 144))
                            }
                        }
                        SelectedEntity::Building(_) => ("BUILDING", egui::Color32::from_rgb(5, 150, 105)),
                        SelectedEntity::Line(_) => ("PLOT LINE", egui::Color32::from_rgb(180, 83, 9)),
                        SelectedEntity::TextMarker(_) => ("TEXT LABEL", egui::Color32::from_rgb(109, 40, 217)),
                        SelectedEntity::Anim(_) => ("ROUTE ANIM", egui::Color32::from_rgb(16, 185, 129)),
                        SelectedEntity::None => ("SKENARIO ROOT", egui::Color32::from_rgb(51, 65, 85)),
                    };
                    egui::Frame::new()
                        .fill(badge.1)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(7, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(badge.0)
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                    if state.is_entity_locked(state.selected) {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(180, 83, 9))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("LOCKED")
                                        .size(9.5)
                                        .strong()
                                        .color(egui::Color32::from_rgb(254, 243, 199)),
                                );
                            });
                    }
                });
            });
            ui.separator();

            if playback.play_test_mode {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(6, 78, 59))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(16, 185, 129)))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Play Test Mode (Pratinjau)")
                                    .size(11.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(167, 243, 208)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Stop").clicked() {
                                        playback.stop_play_test();
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(6.0);
            }

            egui::ScrollArea::vertical()
                .id_salt("godot_inspector_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if !playback.play_test_mode {
                        draw_tool_settings(ui, state, api);
                    }

                    match state.selected {
                        SelectedEntity::Marker(_) => draw_marker_inspector(
                            ui,
                            ctx,
                            state,
                            tcam,
                            playback,
                            poly_edit,
                            media_player,
                            api,
                            ws,
                        ),
                        SelectedEntity::Building(_) => {
                            draw_building_inspector(ui, state, tcam, playback, api)
                        }
                        SelectedEntity::Line(_) => {
                            draw_line_inspector(ui, state, tcam, playback, api)
                        }
                        SelectedEntity::TextMarker(_) => {
                            draw_text_inspector(ui, state, tcam, playback, api)
                        }
                        SelectedEntity::Anim(_) => {
                            draw_anim_inspector(ui, state, playback, api)
                        }
                        SelectedEntity::None => {
                            draw_skenario_inspector(ui, state, tcam, playback, api)
                        }
                    }

                    ui.add_space(8.0);
                    draw_live_ops_section(ui, state, tcam, ws);
                });
        })
        .response
        .rect
        .width()
}

fn inspector_card(ui: &mut egui::Ui, title: &str, default_open: bool, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(22, 30, 45))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(
                egui::RichText::new(title)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(186, 230, 253)),
            )
            .default_open(default_open)
            .show(ui, |ui| {
                ui.add_space(3.0);
                body(ui);
            });
        });
    ui.add_space(6.0);
}

fn axis_badge(ui: &mut egui::Ui, letter: &str, bg: egui::Color32) {
    let badge_w = 34.0_f32;
    let badge_h = 22.0_f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(badge_w, badge_h), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, bg);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::proportional(10.5),
        egui::Color32::WHITE,
    );
}

fn hex_to_srgb(hex: &str) -> [u8; 3] {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() >= 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        )
    {
        return [r, g, b];
    }
    [56, 189, 248]
}

fn color_hex_picker(ui: &mut egui::Ui, label: &str, hex: &mut String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(11.5).color(egui::Color32::LIGHT_GRAY));
        let mut rgb = hex_to_srgb(hex);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            *hex = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        }
        ui.add(egui::TextEdit::singleline(hex).desired_width(90.0));
    });
}

fn draw_locked_banner(ui: &mut egui::Ui, can_unlock: bool) -> bool {
    let mut unlock_clicked = false;
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(66, 32, 6))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(245, 158, 11)))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Objek Terkunci (Locked)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(253, 230, 138)),
                );
                if can_unlock {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("Unlock")
                                .size(10.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(180, 83, 9));
                        if ui.add(btn).clicked() {
                            unlock_clicked = true;
                        }
                    });
                }
            });
        });
    ui.add_space(6.0);
    unlock_clicked
}

fn draw_tool_settings(ui: &mut egui::Ui, state: &mut AppState, api: &ApiClientBridge) {
    match state.tool_mode {
        ToolMode::PlaceText => {
            inspector_card(ui, "Tool: Place Text Label", true, |ui| {
                ui.label("Teks Label Baru:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_text_label)
                        .desired_width(f32::INFINITY),
                );
                ui.label(
                    egui::RichText::new("Klik kiri di peta 3D untuk menempatkan label.")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            });
        }
        ToolMode::DrawBuilding => {
            inspector_card(ui, "Tool: Draw 3D Building", true, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Tinggi (m):");
                    numeric_spinner(
                        ui,
                        "tool_bldg_height",
                        &mut state.new_building_height,
                        SpinnerConfig {
                            min: 2.0,
                            max: 300.0,
                            step: 1.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });
                color_hex_picker(ui, "Warna:", &mut state.new_building_color);
                let pt_count = state.draft_points_lnglat.len();
                ui.label(
                    egui::RichText::new(format!(
                        "Titik poligon: {pt_count} (min 3 — klik titik #1 atau Enter)"
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(pt_count > 0, egui::Button::new("Undo").small())
                        .on_hover_text("Hapus titik terakhir (Backspace)")
                        .clicked()
                    {
                        state.draft_points_lnglat.pop();
                    }
                    let fin = egui::Button::new(
                        egui::RichText::new("Selesai")
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .small();
                    if ui.add_enabled(pt_count >= 3, fin).clicked() {
                        finish_active_draft(state, api);
                    }
                    if ui.small_button("Batal").clicked() {
                        state.draft_points_lnglat.clear();
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::DrawBasicPolygon => {
            inspector_card(ui, "Tool: Gambar Polygon Bebas (Height)", true, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Tinggi (m):");
                    numeric_spinner(
                        ui,
                        "tool_basic_poly_height",
                        &mut state.new_polygon_height,
                        SpinnerConfig {
                            min: 0.2,
                            max: 400.0,
                            step: 0.5,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });
                let pt_count = state.draft_points_lnglat.len();
                ui.label(
                    egui::RichText::new(format!(
                        "Titik polygon: {pt_count} (min 3 — klik titik #1 atau Enter untuk Extrude)"
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(196, 181, 253)),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(pt_count > 0, egui::Button::new("Undo Titik").small())
                        .on_hover_text("Batalkan titik terakhir (Backspace)")
                        .clicked()
                    {
                        state.draft_points_lnglat.pop();
                    }
                    let fin = egui::Button::new(
                        egui::RichText::new("Selesai (Extrude)")
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .small();
                    if ui.add_enabled(pt_count >= 3, fin).clicked() {
                        finish_active_draft(state, api);
                    }
                    if ui.small_button("Batal").clicked() {
                        state.draft_points_lnglat.clear();
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::DrawLine => {
            inspector_card(ui, "Tool: Draw Plot Line", true, |ui| {
                color_hex_picker(ui, "Warna Garis:", &mut state.new_line_color);
                ui.label(
                    egui::RichText::new(format!(
                        "Titik garis: {} (min 2, tekan Enter untuk selesai)",
                        state.draft_points_lnglat.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
                );
            });
        }
        ToolMode::DrawRoute => {
            inspector_card(ui, "Tool: Draw Route Animation", true, |ui| {
                ui.label("Nama Animasi Rute:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_route_name)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                let target_label = state
                    .route_target_marker_id
                    .and_then(|id| state.markers.iter().find(|m| m.id == id))
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| "(Pilih Unit 3D)".to_string());
                let markers_list: Vec<(Uuid, String)> = state
                    .markers
                    .iter()
                    .map(|m| (m.id, m.name.clone()))
                    .collect();
                ui.horizontal(|ui| {
                    ui.label("Unit:");
                    egui::ComboBox::from_id_salt("route_tool_target_marker")
                        .selected_text(target_label)
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            for (mid, mname) in &markers_list {
                                ui.selectable_value(
                                    &mut state.route_target_marker_id,
                                    Some(*mid),
                                    mname,
                                );
                            }
                        });
                });
                ui.add_space(3.0);
                ui.columns(2, |cols| {
                    cols[0].label(egui::RichText::new("Durasi (s)").size(10.5).color(egui::Color32::LIGHT_GRAY));
                    numeric_spinner(
                        &mut cols[0],
                        "tool_route_dur",
                        &mut state.new_route_duration,
                        SpinnerConfig {
                            min: 1.0,
                            max: 600.0,
                            step: 1.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " s",
                            wrap_around: false,
                        },
                    );
                    cols[1].label(egui::RichText::new("Speed").size(10.5).color(egui::Color32::LIGHT_GRAY));
                    numeric_spinner(
                        &mut cols[1],
                        "tool_route_speed",
                        &mut state.new_route_speed,
                        SpinnerConfig {
                            min: 1.0,
                            max: 200.0,
                            step: 1.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: "",
                            wrap_around: false,
                        },
                    );
                });
                ui.checkbox(&mut state.new_route_loop, "Loop / Ulangi");
                ui.label(
                    egui::RichText::new(format!(
                        "Waypoints: {} (min 2, tekan Enter / Finish di toolbar)",
                        state.draft_points_lnglat.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(110, 231, 183)),
                );
            });
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_marker_inspector(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &PlaybackState,
    poly_edit: &mut PolygonEditState,
    media_player: &mut MediaPreviewPlayer,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) {
    let Some(orig) = state.marker_edit.clone() else {
        return;
    };
    let mut m = orig.clone();
    let is_locked = state.is_marker_locked(&m);
    let editable = !playback.play_test_mode && !is_locked;

    // Quick Node Action Header
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&m.name)
                .size(13.5)
                .strong()
                .color(egui::Color32::WHITE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Focus")
                .on_hover_text("Arahkan kamera ke marker ini")
                .clicked()
            {
                let local = state.geo_anchor.lng_lat_alt_to_local(m.lng, m.lat, 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
            }
        });
    });
    ui.label(
        egui::RichText::new(format!("{}  •  {}", m.category, m.model_path))
            .size(10.5)
            .color(egui::Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(6.0);

    if is_locked && draw_locked_banner(ui, !playback.play_test_mode) {
        state.set_marker_locked(m.id, false);
        return;
    }

    // 1. Identity Component
    inspector_card(ui, "Node Identity", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(egui::RichText::new("Name").size(11.0).color(egui::Color32::LIGHT_GRAY));
            ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.checkbox(&mut m.is_spawn, "Active / Visible (Spawned)");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Description").size(11.0).color(egui::Color32::LIGHT_GRAY));
            let mut desc = m.description.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::multiline(&mut desc)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                m.description = if desc.trim().is_empty() {
                    None
                } else {
                    Some(desc)
                };
            }
        });
    });

    // 2. Transform Component (Unity / Godot signature X/Y/Z axis UI)
    inspector_card(ui, "Transform", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            // Position (Geo Coordinates)
            ui.label(egui::RichText::new("Position (Geo / Alt)").size(11.0).strong().color(egui::Color32::from_rgb(203, 213, 225)));
            ui.horizontal(|ui| {
                axis_badge(ui, "Lng", egui::Color32::from_rgb(185, 28, 28));
                numeric_spinner(
                    ui,
                    ("marker_lng", m.id),
                    &mut m.lng,
                    SpinnerConfig {
                        min: -180.0,
                        max: 180.0,
                        step: 0.0001,
                        drag_speed: 0.00002,
                        decimals: 6,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Lat", egui::Color32::from_rgb(29, 78, 216));
                numeric_spinner(
                    ui,
                    ("marker_lat", m.id),
                    &mut m.lat,
                    SpinnerConfig {
                        min: -85.0,
                        max: 85.0,
                        step: 0.0001,
                        drag_speed: 0.00002,
                        decimals: 6,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Alt", egui::Color32::from_rgb(21, 128, 61));
                numeric_spinner(
                    ui,
                    ("marker_alt", m.id),
                    &mut m.altitude,
                    SpinnerConfig {
                        min: -100.0,
                        max: 20_000.0,
                        step: 0.5,
                        drag_speed: 0.25,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(6.0);

            // Rotation (Euler degrees)
            ui.label(egui::RichText::new("Rotation (°)").size(11.0).strong().color(egui::Color32::from_rgb(203, 213, 225)));
            let rot_cfg = SpinnerConfig {
                min: -360.0,
                max: 360.0,
                step: 1.0,
                drag_speed: 1.0,
                decimals: 1,
                suffix: "°",
                wrap_around: false,
            };
            ui.horizontal(|ui| {
                axis_badge(ui, "X", egui::Color32::from_rgb(220, 38, 38));
                numeric_spinner(ui, ("marker_rot_x", m.id), &mut m.rot_x, rot_cfg);
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Y", egui::Color32::from_rgb(22, 163, 74));
                numeric_spinner(ui, ("marker_rot_y", m.id), &mut m.rot_y, rot_cfg);
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Z", egui::Color32::from_rgb(37, 99, 235));
                numeric_spinner(ui, ("marker_rot_z", m.id), &mut m.rot_z, rot_cfg);
            });

            ui.add_space(6.0);

            // Scale (default 0.1, min 0.01)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Scale (min 0.01)").size(11.0).strong().color(egui::Color32::from_rgb(203, 213, 225)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut state.uniform_scale_lock, "Uniform");
                });
            });
            let scale_cfg = SpinnerConfig {
                min: MIN_OBJECT_3D_SCALE,
                max: MAX_OBJECT_3D_SCALE,
                step: 0.01,
                drag_speed: 0.01,
                decimals: 2,
                suffix: "",
                wrap_around: false,
            };
            if state.uniform_scale_lock {
                let mut u_scale = m.scale_x.clamp(MIN_OBJECT_3D_SCALE, MAX_OBJECT_3D_SCALE);
                ui.horizontal(|ui| {
                    axis_badge(ui, "XYZ", egui::Color32::from_rgb(14, 116, 144));
                    if numeric_spinner(ui, ("marker_scale_xyz", m.id), &mut u_scale, scale_cfg) {
                        m.scale_x = u_scale;
                        m.scale_y = u_scale;
                        m.scale_z = u_scale;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    axis_badge(ui, "X", egui::Color32::from_rgb(220, 38, 38));
                    numeric_spinner(ui, ("marker_scale_x", m.id), &mut m.scale_x, scale_cfg);
                });
                ui.horizontal(|ui| {
                    axis_badge(ui, "Y", egui::Color32::from_rgb(22, 163, 74));
                    numeric_spinner(ui, ("marker_scale_y", m.id), &mut m.scale_y, scale_cfg);
                });
                ui.horizontal(|ui| {
                    axis_badge(ui, "Z", egui::Color32::from_rgb(37, 99, 235));
                    numeric_spinner(ui, ("marker_scale_z", m.id), &mut m.scale_z, scale_cfg);
                });
            }
        });
    });

    // 2b. Basic Mesh Shape, Dimensions & Material (Solid Color Picker / Assets Image Texture)
    if let Some(mut bm) = parse_basic_mesh_config(&m) {
        let mut bm_changed = false;

        inspector_card(ui, "Basic Mesh — Bentuk & Ukuran", true, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Bentuk:").size(11.0).color(egui::Color32::LIGHT_GRAY));
                    egui::ComboBox::from_id_salt(("bm_shape_combo", m.id))
                        .selected_text(bm.shape.label())
                        .width((ui.available_width() - 4.0).max(120.0))
                        .show_ui(ui, |ui| {
                            for &shape in BasicMeshShape::ALL {
                                if ui
                                    .selectable_value(&mut bm.shape, shape, shape.label())
                                    .changed()
                                {
                                    let defaults = BasicMeshConfig::default_for(shape);
                                    bm.width = defaults.width;
                                    bm.height = defaults.height;
                                    bm.depth = defaults.depth;
                                    bm.radius = defaults.radius;
                                    bm.tube_radius = defaults.tube_radius;
                                    bm.segments = defaults.segments;
                                    bm.polygon_points.clear();
                                    bm.double_sided = defaults.double_sided;
                                    if poly_edit.is_editing(m.id) {
                                        poly_edit.clear();
                                    }
                                    bm_changed = true;
                                }
                            }
                        });
                });

                ui.add_space(4.0);
                let dim_cfg = SpinnerConfig {
                    min: 0.1,
                    max: 500.0,
                    step: 0.5,
                    drag_speed: 0.1,
                    decimals: 2,
                    suffix: " m",
                    wrap_around: false,
                };
                let f64_dim = |ui: &mut egui::Ui,
                               label: &str,
                               id_key: &str,
                               field: &mut f32,
                               cfg: SpinnerConfig,
                               changed: &mut bool| {
                    let mut val = *field as f64;
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            egui::vec2(76.0, 20.0),
                            egui::Label::new(
                                egui::RichText::new(label)
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            ),
                        );
                        if numeric_spinner(ui, (id_key, m.id), &mut val, cfg) {
                            *field = val as f32;
                            *changed = true;
                        }
                    });
                };

                match bm.shape {
                    BasicMeshShape::Plane => {
                        f64_dim(ui, "Lebar (X):", "bm_w", &mut bm.width, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Panjang (Z):", "bm_d", &mut bm.depth, dim_cfg, &mut bm_changed);
                    }
                    BasicMeshShape::Cube | BasicMeshShape::Ramp => {
                        f64_dim(ui, "Lebar (X):", "bm_w", &mut bm.width, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Tinggi (Y):", "bm_h", &mut bm.height, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Panjang (Z):", "bm_d", &mut bm.depth, dim_cfg, &mut bm_changed);
                    }
                    BasicMeshShape::Wall => {
                        let thick_cfg = SpinnerConfig {
                            min: 0.05,
                            max: 50.0,
                            step: 0.05,
                            drag_speed: 0.02,
                            decimals: 2,
                            suffix: " m",
                            wrap_around: false,
                        };
                        f64_dim(ui, "Panjang:", "bm_w", &mut bm.width, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Tinggi:", "bm_h", &mut bm.height, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Tebal:", "bm_d", &mut bm.depth, thick_cfg, &mut bm_changed);
                    }
                    BasicMeshShape::Cylinder | BasicMeshShape::Cone | BasicMeshShape::Capsule => {
                        f64_dim(ui, "Radius:", "bm_r", &mut bm.radius, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Tinggi (Y):", "bm_h", &mut bm.height, dim_cfg, &mut bm_changed);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                egui::vec2(76.0, 20.0),
                                egui::Label::new(
                                    egui::RichText::new("Segmen:")
                                        .size(11.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                ),
                            );
                            let w = (ui.available_width() - 2.0).max(72.0);
                            if numeric_spinner_u32_w(
                                ui,
                                ("bm_seg", m.id),
                                &mut bm.segments,
                                w,
                                6,
                                64,
                                2,
                                0.5,
                            ) {
                                bm_changed = true;
                            }
                        });
                    }
                    BasicMeshShape::Sphere => {
                        f64_dim(ui, "Radius:", "bm_r", &mut bm.radius, dim_cfg, &mut bm_changed);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                egui::vec2(76.0, 20.0),
                                egui::Label::new(
                                    egui::RichText::new("Segmen:")
                                        .size(11.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                ),
                            );
                            let w = (ui.available_width() - 2.0).max(72.0);
                            if numeric_spinner_u32_w(
                                ui,
                                ("bm_seg", m.id),
                                &mut bm.segments,
                                w,
                                8,
                                48,
                                2,
                                0.5,
                            ) {
                                bm_changed = true;
                            }
                        });
                    }
                    BasicMeshShape::Polygon => {
                        f64_dim(ui, "Tinggi (Y):", "bm_h", &mut bm.height, dim_cfg, &mut bm_changed);

                        ui.add_space(4.0);
                        let is_editing_poly = poly_edit.is_editing(m.id);
                        let edit_poly_btn = egui::Button::new(
                            egui::RichText::new(if is_editing_poly {
                                "Selesai Edit Titik (Esc)"
                            } else {
                                "Edit Titik Polygon di Peta"
                            })
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(if is_editing_poly {
                            egui::Color32::from_rgb(5, 150, 105)
                        } else {
                            egui::Color32::from_rgb(14, 116, 144)
                        })
                        .min_size(egui::vec2(ui.available_width(), 26.0));
                        if ui
                            .add(edit_poly_btn)
                            .on_hover_text(if is_editing_poly {
                                "Keluar dari mode edit titik polygon"
                            } else {
                                "Tampilkan titik polygon di peta: klik titik untuk Move Gizmo, klik '+' pada sisi untuk menambah titik baru"
                            })
                            .clicked()
                        {
                            if is_editing_poly {
                                poly_edit.clear();
                            } else {
                                if bm.polygon_points.len() < 3 {
                                    ensure_editable_polygon_points(&mut bm);
                                    bm_changed = true;
                                }
                                poly_edit.editing_marker_id = Some(m.id);
                                poly_edit.selected_vertex = None;
                                poly_edit.add_point_on_map_mode = false;
                            }
                        }

                        if is_editing_poly {
                            ui.add_space(4.0);
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 23, 42))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(56, 189, 248),
                                ))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(6, 5))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new(
                                            "• Klik titik di peta -> muncul Move Gizmo (X/Z/Free)\n• Klik (+) hijau di tengah sisi -> sisipkan & tarik titik baru",
                                        )
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(186, 230, 253)),
                                    );
                                    ui.add_space(4.0);

                                    let add_mode = poly_edit.add_point_on_map_mode;
                                    let add_pt_btn = egui::Button::new(
                                        egui::RichText::new(if add_mode {
                                            "Mode Tambah Titik AKTIF (Klik Peta)"
                                        } else {
                                            "+ Tambah Titik (Klik Peta)"
                                        })
                                        .size(10.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if add_mode {
                                        egui::Color32::from_rgb(124, 58, 237)
                                    } else {
                                        egui::Color32::from_rgb(49, 46, 129)
                                    })
                                    .min_size(egui::vec2(ui.available_width(), 24.0));
                                    if ui
                                        .add(add_pt_btn)
                                        .on_hover_text("Klik sembarang lokasi di peta untuk menyisipkan titik baru pada sisi polygon terdekat")
                                        .clicked()
                                    {
                                        poly_edit.add_point_on_map_mode = !add_mode;
                                    }

                                    // Selected vertex precision editor
                                    if let Some(vi) = poly_edit.selected_vertex
                                        && vi < bm.polygon_points.len()
                                    {
                                        ui.add_space(5.0);
                                        ui.separator();
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "Titik Terpilih: #{}",
                                                    vi + 1
                                                ))
                                                .size(10.5)
                                                .strong()
                                                .color(egui::Color32::from_rgb(253, 224, 71)),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui.small_button("Lepas").clicked() {
                                                        poly_edit.selected_vertex = None;
                                                    }
                                                },
                                            );
                                        });
                                        let v_cfg = SpinnerConfig {
                                            min: -2000.0,
                                            max: 2000.0,
                                            step: 0.25,
                                            drag_speed: 0.1,
                                            decimals: 2,
                                            suffix: " m",
                                            wrap_around: false,
                                        };
                                        let mut vx = bm.polygon_points[vi][0] as f64;
                                        let mut vz = bm.polygon_points[vi][1] as f64;
                                        ui.horizontal(|ui| {
                                            axis_badge(
                                                ui,
                                                "X",
                                                egui::Color32::from_rgb(220, 38, 38),
                                            );
                                            if numeric_spinner(
                                                ui,
                                                ("poly_vx", m.id, vi),
                                                &mut vx,
                                                v_cfg,
                                            ) {
                                                bm.polygon_points[vi][0] = vx as f32;
                                                bm_changed = true;
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            axis_badge(
                                                ui,
                                                "Z",
                                                egui::Color32::from_rgb(37, 99, 235),
                                            );
                                            if numeric_spinner(
                                                ui,
                                                ("poly_vz", m.id, vi),
                                                &mut vz,
                                                v_cfg,
                                            ) {
                                                bm.polygon_points[vi][1] = vz as f32;
                                                bm_changed = true;
                                            }
                                        });
                                    }

                                    // Compact vertex list (click to select & show Move Gizmo, '+' insert, 'x' delete)
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Daftar Titik ({} titik):",
                                            bm.polygon_points.len()
                                        ))
                                        .size(10.5)
                                        .strong()
                                        .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    let mut insert_after_idx: Option<usize> = None;
                                    let mut delete_idx: Option<usize> = None;
                                    let can_delete = bm.polygon_points.len() > 3;

                                    egui::ScrollArea::vertical()
                                        .id_salt(("poly_verts_scroll", m.id))
                                        .max_height(135.0)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            for (i, pt) in bm.polygon_points.iter().enumerate() {
                                                let vsel = poly_edit.selected_vertex == Some(i);
                                                let vbg = if vsel {
                                                    egui::Color32::from_rgb(14, 116, 144)
                                                } else {
                                                    egui::Color32::from_rgb(23, 32, 48)
                                                };
                                                egui::Frame::new()
                                                    .fill(vbg)
                                                    .corner_radius(3.0)
                                                    .inner_margin(egui::Margin::symmetric(5, 2))
                                                    .show(ui, |ui| {
                                                        ui.horizontal(|ui| {
                                                            let lbl = format!(
                                                                "#{}  ({:.1}, {:.1})",
                                                                i + 1,
                                                                pt[0],
                                                                pt[1]
                                                            );
                                                            if ui
                                                                .add(
                                                                    egui::Label::new(
                                                                        egui::RichText::new(lbl)
                                                                            .size(10.5)
                                                                            .color(if vsel {
                                                                                egui::Color32::WHITE
                                                                            } else {
                                                                                egui::Color32::from_rgb(
                                                                                    203, 213, 225,
                                                                                )
                                                                            }),
                                                                    )
                                                                    .sense(egui::Sense::click()),
                                                                )
                                                                .on_hover_text(
                                                                    "Klik untuk menampilkan Move Gizmo pada titik ini",
                                                                )
                                                                .clicked()
                                                            {
                                                                poly_edit.selected_vertex = Some(i);
                                                                poly_edit.add_point_on_map_mode =
                                                                    false;
                                                            }
                                                            ui.with_layout(
                                                                egui::Layout::right_to_left(
                                                                    egui::Align::Center,
                                                                ),
                                                                |ui| {
                                                                    if ui
                                                                        .add_enabled(
                                                                            can_delete,
                                                                            egui::Button::new(
                                                                                egui::RichText::new("x")
                                                                                    .size(10.0)
                                                                                    .color(if can_delete {
                                                                                        egui::Color32::from_rgb(
                                                                                            248, 113, 113,
                                                                                        )
                                                                                    } else {
                                                                                        egui::Color32::from_rgb(
                                                                                            71, 85, 105,
                                                                                        )
                                                                                    }),
                                                                            )
                                                                            .small()
                                                                            .frame(false),
                                                                        )
                                                                        .on_hover_text(if can_delete {
                                                                            "Hapus titik ini"
                                                                        } else {
                                                                            "Minimal 3 titik untuk polygon"
                                                                        })
                                                                        .clicked()
                                                                    {
                                                                        delete_idx = Some(i);
                                                                    }
                                                                    if ui
                                                                        .add(
                                                                            egui::Button::new(
                                                                                egui::RichText::new("+")
                                                                                    .size(11.0)
                                                                                    .strong()
                                                                                    .color(
                                                                                        egui::Color32::from_rgb(
                                                                                            52, 211, 153,
                                                                                        ),
                                                                                    ),
                                                                            )
                                                                            .small()
                                                                            .frame(false),
                                                                        )
                                                                        .on_hover_text(
                                                                            "Sisipkan titik baru setelah titik ini",
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        insert_after_idx = Some(i);
                                                                    }
                                                                },
                                                            );
                                                        });
                                                    });
                                                ui.add_space(2.0);
                                            }
                                        });

                                    if let Some(edge_i) = insert_after_idx {
                                        let new_i =
                                            insert_midpoint_after(&mut bm.polygon_points, edge_i);
                                        poly_edit.selected_vertex = Some(new_i);
                                        bm_changed = true;
                                    } else if let Some(del_i) = delete_idx
                                        && bm.polygon_points.len() > 3
                                    {
                                        bm.polygon_points.remove(del_i);
                                        if let Some(sel) = poly_edit.selected_vertex {
                                            if sel == del_i {
                                                poly_edit.selected_vertex = None;
                                            } else if sel > del_i {
                                                poly_edit.selected_vertex = Some(sel - 1);
                                            }
                                        }
                                        bm_changed = true;
                                    }
                                });
                        } else if bm.polygon_points.len() >= 3 {
                            ui.add_space(3.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Custom Polygon ({} titik)",
                                        bm.polygon_points.len()
                                    ))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(196, 181, 253)),
                                );
                                if ui.small_button("Reset Segi-N").clicked() {
                                    bm.polygon_points.clear();
                                    poly_edit.clear();
                                    bm_changed = true;
                                }
                            });
                        } else {
                            f64_dim(ui, "Radius:", "bm_r", &mut bm.radius, dim_cfg, &mut bm_changed);
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(76.0, 20.0),
                                    egui::Label::new(
                                        egui::RichText::new("Jumlah Sisi:")
                                            .size(11.0)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    ),
                                );
                                let w = (ui.available_width() - 2.0).max(72.0);
                                if numeric_spinner_u32_w(
                                    ui,
                                    ("bm_sides", m.id),
                                    &mut bm.segments,
                                    w,
                                    3,
                                    32,
                                    1,
                                    0.3,
                                ) {
                                    bm_changed = true;
                                }
                            });
                        }
                    }
                    BasicMeshShape::Torus => {
                        let tube_cfg = SpinnerConfig {
                            min: 0.05,
                            max: 50.0,
                            step: 0.05,
                            drag_speed: 0.03,
                            decimals: 2,
                            suffix: " m",
                            wrap_around: false,
                        };
                        f64_dim(ui, "Radius Utama:", "bm_r", &mut bm.radius, dim_cfg, &mut bm_changed);
                        f64_dim(ui, "Radius Pipa:", "bm_tr", &mut bm.tube_radius, tube_cfg, &mut bm_changed);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                egui::vec2(76.0, 20.0),
                                egui::Label::new(
                                    egui::RichText::new("Segmen:")
                                        .size(11.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                ),
                            );
                            let w = (ui.available_width() - 2.0).max(72.0);
                            if numeric_spinner_u32_w(
                                ui,
                                ("bm_seg", m.id),
                                &mut bm.segments,
                                w,
                                12,
                                64,
                                2,
                                0.5,
                            ) {
                                bm_changed = true;
                            }
                        });
                    }
                }
            });
        });

        inspector_card(ui, "Material & Surface", true, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                // Material Kind Switcher: Solid Color vs Image Texture
                ui.columns(2, |cols| {
                    let is_solid = bm.material_kind == BasicMeshMaterialKind::SolidColor;
                    if cols[0]
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Solid Color")
                                    .size(11.0)
                                    .strong()
                                    .color(if is_solid {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(148, 163, 184)
                                    }),
                            )
                            .fill(if is_solid {
                                egui::Color32::from_rgb(14, 116, 144)
                            } else {
                                egui::Color32::from_rgb(15, 23, 42)
                            })
                            .min_size(egui::vec2(cols[0].available_width(), 24.0)),
                        )
                        .clicked()
                        && !is_solid
                    {
                        bm.material_kind = BasicMeshMaterialKind::SolidColor;
                        bm_changed = true;
                    }

                    let is_tex = bm.material_kind == BasicMeshMaterialKind::ImageTexture;
                    if cols[1]
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Image Texture")
                                    .size(11.0)
                                    .strong()
                                    .color(if is_tex {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(148, 163, 184)
                                    }),
                            )
                            .fill(if is_tex {
                                egui::Color32::from_rgb(109, 40, 217)
                            } else {
                                egui::Color32::from_rgb(15, 23, 42)
                            })
                            .min_size(egui::vec2(cols[1].available_width(), 24.0)),
                        )
                        .clicked()
                        && !is_tex
                    {
                        bm.material_kind = BasicMeshMaterialKind::ImageTexture;
                        if bm.texture_rel_path.is_none()
                            && let Some(first_tex) = state
                                .media_assets
                                .iter()
                                .find(|a| a.media_type == MediaAssetType::Texture)
                        {
                            bm.texture_asset_id = Some(first_tex.id);
                            bm.texture_rel_path = Some(first_tex.active_file.rel_path.clone());
                        }
                        bm_changed = true;
                    }
                });

                ui.add_space(6.0);

                match bm.material_kind {
                    BasicMeshMaterialKind::SolidColor => {
                        // Full interactive HSV 2D Color Picker + Alpha/Opacity bar
                        let [r, g, b, a] = bm.color_rgba;
                        let mut c32 = egui::Color32::from_rgba_unmultiplied(r, g, b, a.max(12));
                        if egui::color_picker::color_picker_color32(
                            ui,
                            &mut c32,
                            egui::color_picker::Alpha::OnlyBlend,
                        ) {
                            let [nr, ng, nb, na] = c32.to_srgba_unmultiplied();
                            bm.color_rgba = [nr, ng, nb, na.max(12)];
                            bm_changed = true;
                        }

                        ui.add_space(5.0);
                        ui.label(
                            egui::RichText::new("Preset Warna Cepat:")
                                .size(10.5)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            for (tip, rgba) in [
                                ("Aspal", [51, 65, 85, 255]),
                                ("Beton", [148, 163, 184, 255]),
                                ("Rumput", [34, 139, 76, 255]),
                                ("Pasir", [214, 188, 138, 255]),
                                ("Cyan Taktis", [56, 189, 248, 220]),
                                ("Amber Peringatan", [245, 158, 11, 230]),
                                ("Merah Zona", [239, 68, 68, 210]),
                                ("Kaca Transparan", [125, 211, 252, 110]),
                            ] {
                                let sw_col = egui::Color32::from_rgba_unmultiplied(
                                    rgba[0], rgba[1], rgba[2], rgba[3],
                                );
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(22.0, 18.0),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(rect, 3.0, sw_col);
                                ui.painter().rect_stroke(
                                    rect,
                                    3.0,
                                    egui::Stroke::new(
                                        1.0,
                                        if bm.color_rgba == rgba {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::from_rgb(71, 85, 105)
                                        },
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                if resp.on_hover_text(tip).clicked() {
                                    bm.color_rgba = rgba;
                                    bm_changed = true;
                                }
                            }
                        });
                    }
                    BasicMeshMaterialKind::ImageTexture => {
                        let textures: Vec<(Uuid, String, String)> = state
                            .media_assets
                            .iter()
                            .filter(|a| a.media_type == MediaAssetType::Texture)
                            .map(|a| (a.id, a.name.clone(), a.active_file.rel_path.clone()))
                            .collect();

                        if textures.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(250, 204, 21),
                                "Belum ada Image Texture. Tambahkan melalui menu Assets -> Texture.",
                            );
                        } else {
                            let selected_tex_name = bm
                                .texture_asset_id
                                .and_then(|tid| textures.iter().find(|(id, _, _)| *id == tid))
                                .or_else(|| {
                                    bm.texture_rel_path.as_deref().and_then(|rp| {
                                        textures.iter().find(|(_, _, rel)| rel == rp)
                                    })
                                })
                                .map(|(_, name, _)| name.clone())
                                .unwrap_or_else(|| "(Pilih Image Texture)".to_string());

                            ui.label(
                                egui::RichText::new("Assets Texture:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            egui::ComboBox::from_id_salt(("bm_texture_combo", m.id))
                                .selected_text(selected_tex_name)
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    for (tid, tname, trel) in &textures {
                                        let sel = bm.texture_asset_id == Some(*tid)
                                            || bm.texture_rel_path.as_deref() == Some(trel.as_str());
                                        if ui.selectable_label(sel, tname).clicked() {
                                            bm.texture_asset_id = Some(*tid);
                                            bm.texture_rel_path = Some(trel.clone());
                                            bm_changed = true;
                                        }
                                    }
                                });

                            // Live Thumbnail Preview of selected Asset Texture
                            if let Some(rel) = bm.texture_rel_path.clone()
                                && let Some(tex_id) = media_player
                                    .ensure_image_texture(
                                        ctx,
                                        &state.assets_root,
                                        &rel,
                                        state.media_reload_nonce,
                                    )
                                    .map(|h| h.id())
                            {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(10, 15, 25))
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            egui::Color32::from_rgb(51, 65, 85),
                                        ))
                                        .corner_radius(4.0)
                                        .inner_margin(egui::Margin::same(2))
                                        .show(ui, |ui| {
                                            ui.image(egui::load::SizedTexture::new(
                                                tex_id,
                                                egui::vec2(54.0, 54.0),
                                            ));
                                        });
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Preview Texture")
                                                .size(10.5)
                                                .strong()
                                                .color(egui::Color32::from_rgb(196, 181, 253)),
                                        );
                                        ui.label(
                                            egui::RichText::new(&rel)
                                                .size(9.5)
                                                .color(egui::Color32::from_rgb(148, 163, 184)),
                                        );
                                    });
                                });
                            }

                            ui.add_space(6.0);
                            // Texture UV Tiling / Scale controls
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Scale Texture (Tiling)")
                                        .size(11.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(203, 213, 225)),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.checkbox(&mut bm.uv_lock, "Uniform").changed() {
                                            bm_changed = true;
                                        }
                                    },
                                );
                            });
                            let uv_cfg = SpinnerConfig {
                                min: 0.05,
                                max: 50.0,
                                step: 0.1,
                                drag_speed: 0.05,
                                decimals: 2,
                                suffix: "x",
                                wrap_around: false,
                            };
                            if bm.uv_lock {
                                let mut uv_u = bm.uv_scale_x as f64;
                                ui.horizontal(|ui| {
                                    axis_badge(ui, "UV", egui::Color32::from_rgb(109, 40, 217));
                                    if numeric_spinner(ui, ("bm_uv_uni", m.id), &mut uv_u, uv_cfg) {
                                        bm.uv_scale_x = uv_u as f32;
                                        bm.uv_scale_y = uv_u as f32;
                                        bm_changed = true;
                                    }
                                });
                            } else {
                                let mut uv_x = bm.uv_scale_x as f64;
                                let mut uv_y = bm.uv_scale_y as f64;
                                ui.horizontal(|ui| {
                                    axis_badge(ui, "U", egui::Color32::from_rgb(220, 38, 38));
                                    if numeric_spinner(ui, ("bm_uv_x", m.id), &mut uv_x, uv_cfg) {
                                        bm.uv_scale_x = uv_x as f32;
                                        bm_changed = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    axis_badge(ui, "V", egui::Color32::from_rgb(22, 163, 74));
                                    if numeric_spinner(ui, ("bm_uv_y", m.id), &mut uv_y, uv_cfg) {
                                        bm.uv_scale_y = uv_y as f32;
                                        bm_changed = true;
                                    }
                                });
                            }

                            ui.add_space(4.0);
                            // Tint Color Picker & Opacity for Image Texture
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Tint / Warna:")
                                        .size(11.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                let [tr, tg, tb, ta] = bm.texture_tint;
                                let mut tc =
                                    egui::Color32::from_rgba_unmultiplied(tr, tg, tb, ta.max(12));
                                if egui::color_picker::color_edit_button_srgba(
                                    ui,
                                    &mut tc,
                                    egui::color_picker::Alpha::OnlyBlend,
                                )
                                .changed()
                                {
                                    let [nr, ng, nb, na] = tc.to_srgba_unmultiplied();
                                    bm.texture_tint = [nr, ng, nb, na.max(12)];
                                    bm_changed = true;
                                }
                                if ui.small_button("Reset Putih").clicked() {
                                    bm.texture_tint = [255, 255, 255, 255];
                                    bm_changed = true;
                                }
                            });
                        }
                    }
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                // Recommended PBR & Rendering Settings
                ui.label(
                    egui::RichText::new("Pengaturan Permukaan (PBR)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.columns(2, |cols| {
                    let mut rough = bm.roughness as f64;
                    cols[0].label(
                        egui::RichText::new("Roughness")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if numeric_spinner(
                        &mut cols[0],
                        ("bm_rough", m.id),
                        &mut rough,
                        SpinnerConfig {
                            min: 0.04,
                            max: 1.0,
                            step: 0.05,
                            drag_speed: 0.02,
                            decimals: 2,
                            suffix: "",
                            wrap_around: false,
                        },
                    ) {
                        bm.roughness = rough as f32;
                        bm_changed = true;
                    }

                    let mut metal = bm.metallic as f64;
                    cols[1].label(
                        egui::RichText::new("Metallic")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if numeric_spinner(
                        &mut cols[1],
                        ("bm_metal", m.id),
                        &mut metal,
                        SpinnerConfig {
                            min: 0.0,
                            max: 1.0,
                            step: 0.05,
                            drag_speed: 0.02,
                            decimals: 2,
                            suffix: "",
                            wrap_around: false,
                        },
                    ) {
                        bm.metallic = metal as f32;
                        bm_changed = true;
                    }
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut bm.unlit, "Unlit (Flat)").changed() {
                        bm_changed = true;
                    }
                    if ui.checkbox(&mut bm.double_sided, "Double Sided").changed() {
                        bm_changed = true;
                    }
                });
            });
        });

        if bm_changed && editable {
            write_basic_mesh_config(&mut m, &bm);
        }
    }

    // Apply live edits to local state immediately and track dirty flag for toolbar Simpan
    let marker_changed = m.name != orig.name
        || m.is_spawn != orig.is_spawn
        || m.description != orig.description
        || (m.lng - orig.lng).abs() > 1e-9
        || (m.lat - orig.lat).abs() > 1e-9
        || (m.altitude - orig.altitude).abs() > 1e-6
        || (m.rot_x - orig.rot_x).abs() > 1e-6
        || (m.rot_y - orig.rot_y).abs() > 1e-6
        || (m.rot_z - orig.rot_z).abs() > 1e-6
        || (m.scale_x - orig.scale_x).abs() > 1e-6
        || (m.scale_y - orig.scale_y).abs() > 1e-6
        || (m.scale_z - orig.scale_z).abs() > 1e-6
        || m.model_path != orig.model_path
        || m.metadata != orig.metadata;

    if editable && marker_changed {
        if let Some(existing) = state.markers.iter_mut().find(|x| x.id == m.id) {
            *existing = m.clone();
        }
        state.marker_edit = Some(m.clone());
        state.dirty_marker_ids.insert(m.id);
    }

    // 3. Action Buttons
    if editable {
        ui.horizontal(|ui| {
            let save_btn = egui::Button::new(
                egui::RichText::new("Simpan")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(save_btn).clicked() && !state.offline_preview
                && let Some(auth) = current_auth(state)
            {
                api.send(ApiCommand::UpdateMarker {
                    auth,
                    id: m.id,
                    req: UpdateMarkerRequest {
                        name: m.name.clone(),
                        lng: m.lng,
                        lat: m.lat,
                        altitude: m.altitude,
                        rot_x: m.rot_x,
                        rot_y: m.rot_y,
                        rot_z: m.rot_z,
                        scale_x: m.scale_x,
                        scale_y: m.scale_y,
                        scale_z: m.scale_z,
                        is_spawn: m.is_spawn,
                        description: m.description.clone(),
                        metadata: m.metadata.clone(),
                    },
                });
                ws.send(WsOutCommand::SendCommand(TacticalCommand {
                    operasi_id: m.operasi_id,
                    skenario_id: m.skenario_id,
                    marker_id: Some(m.id),
                    command: "move_marker".into(),
                    payload: serde_json::json!({
                        "lng": m.lng,
                        "lat": m.lat,
                        "altitude": m.altitude,
                        "rot_y": m.rot_y,
                    }),
                    save_history: true,
                }));
            }

            if ui.button("Duplikat").clicked() {
                let offset_lng = m.lng + 0.00012;
                let offset_lat = m.lat - 0.00012;
                if state.offline_preview {
                    let mut dup = m.clone();
                    dup.id = Uuid::new_v4();
                    dup.name = format!("{} (Copy)", m.name);
                    dup.lng = offset_lng;
                    dup.lat = offset_lat;
                    state.selected = SelectedEntity::Marker(dup.id);
                    state.marker_edit = Some(dup.clone());
                    state.markers.push(dup);
                } else if let Some(auth) = current_auth(state) {
                    api.send(ApiCommand::CreateMarker {
                        auth,
                        req: CreateMarkerRequest {
                            operasi_id: m.operasi_id,
                            skenario_id: m.skenario_id,
                            unit_id: m.unit_id.clone(),
                            name: format!("{} (Copy)", m.name),
                            category: m.category.clone(),
                            model_path: m.model_path.clone(),
                            quantity: m.quantity,
                            lng: offset_lng,
                            lat: offset_lat,
                            altitude: m.altitude,
                            rot_x: m.rot_x,
                            rot_y: m.rot_y,
                            rot_z: m.rot_z,
                            scale_x: m.scale_x,
                            scale_y: m.scale_y,
                            scale_z: m.scale_z,
                            is_spawn: m.is_spawn,
                            description: m.description.clone(),
                            metadata: m.metadata.clone(),
                        },
                    });
                }
            }

            let del_btn = egui::Button::new(
                egui::RichText::new("Hapus").color(egui::Color32::from_rgb(254, 202, 202)),
            )
            .fill(egui::Color32::from_rgb(127, 29, 29));
            if ui.add(del_btn).clicked() {
                state.request_delete_entity(SelectedEntity::Marker(m.id));
            }
        });
    }
}

fn draw_building_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &PlaybackState,
    api: &ApiClientBridge,
) {
    let Some(orig) = state.building_edit.clone() else {
        return;
    };
    let mut b = orig.clone();
    let is_locked = state.locked_entity_ids.contains(&b.id);
    let editable = !playback.play_test_mode && !is_locked;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&b.name).size(13.5).strong().color(egui::Color32::WHITE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Focus").clicked()
                && let Some(pt) = b.geom.first()
            {
                let local = state.geo_anchor.lng_lat_alt_to_local(pt[0], pt[1], 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
            }
        });
    });
    ui.add_space(6.0);

    if is_locked && draw_locked_banner(ui, !playback.play_test_mode) {
        state.locked_entity_ids.remove(&b.id);
        return;
    }

    inspector_card(ui, "Building Extrusion & Material", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Height (m):");
                numeric_spinner(
                    ui,
                    ("bldg_height", b.id),
                    &mut b.height,
                    SpinnerConfig {
                        min: 1.0,
                        max: 400.0,
                        step: 1.0,
                        drag_speed: 0.5,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            color_hex_picker(ui, "Color:", &mut b.color);
            ui.label(
                egui::RichText::new(format!("Polygon vertices: {}", b.geom.len()))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    if editable {
        let b_changed = b.name != orig.name
            || b.color != orig.color
            || (b.height - orig.height).abs() > 1e-6;
        if b_changed {
            if let Some(existing) = state.buildings.iter_mut().find(|x| x.id == b.id) {
                *existing = b.clone();
            }
            state.building_edit = Some(b.clone());
            state.dirty_building_ids.insert(b.id);
        }

        ui.horizontal(|ui| {
            let save_btn = egui::Button::new(
                egui::RichText::new("Simpan Building")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(save_btn).clicked()
                && !state.offline_preview
                && let Some(auth) = current_auth(state)
            {
                api.send(ApiCommand::UpdateBuilding {
                    auth,
                    id: b.id,
                    req: UpdateBuildingRequest {
                        name: b.name,
                        description: b.description,
                        color: b.color,
                        height: b.height,
                        geom: b.geom,
                    },
                });
            }
            let del_btn = egui::Button::new(
                egui::RichText::new("Hapus").color(egui::Color32::from_rgb(254, 202, 202)),
            )
            .fill(egui::Color32::from_rgb(127, 29, 29));
            if ui.add(del_btn).clicked() {
                state.request_delete_entity(SelectedEntity::Building(b.id));
            }
        });
    }
}

fn draw_line_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &PlaybackState,
    api: &ApiClientBridge,
) {
    let Some(orig) = state.line_edit.clone() else {
        return;
    };
    let mut l = orig.clone();
    let is_locked = state.locked_entity_ids.contains(&l.id);
    let editable = !playback.play_test_mode && !is_locked;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&l.name).size(13.5).strong().color(egui::Color32::WHITE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Focus").clicked()
                && let Some(pt) = l.geom.first()
            {
                let local = state.geo_anchor.lng_lat_alt_to_local(pt[0], pt[1], 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
            }
        });
    });
    ui.add_space(6.0);

    if is_locked && draw_locked_banner(ui, !playback.play_test_mode) {
        state.locked_entity_ids.remove(&l.id);
        return;
    }

    inspector_card(ui, "Plot Line Properties", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            color_hex_picker(ui, "Color:", &mut l.color);
            ui.columns(2, |cols| {
                cols[0].label(egui::RichText::new("Width").size(10.5).color(egui::Color32::LIGHT_GRAY));
                numeric_spinner(
                    &mut cols[0],
                    ("line_width", l.id),
                    &mut l.width,
                    SpinnerConfig {
                        min: 1.0,
                        max: 20.0,
                        step: 0.5,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: "",
                        wrap_around: false,
                    },
                );
                cols[1].label(egui::RichText::new("Alpha").size(10.5).color(egui::Color32::LIGHT_GRAY));
                numeric_spinner(
                    &mut cols[1],
                    ("line_alpha", l.id),
                    &mut l.alpha,
                    SpinnerConfig {
                        min: 0.1,
                        max: 1.0,
                        step: 0.05,
                        drag_speed: 0.02,
                        decimals: 2,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.label(
                egui::RichText::new(format!("Polyline points: {}", l.geom.len()))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    if editable {
        let l_changed = l.name != orig.name
            || l.color != orig.color
            || (l.width - orig.width).abs() > 1e-6
            || (l.alpha - orig.alpha).abs() > 1e-6;
        if l_changed {
            if let Some(existing) = state.lines.iter_mut().find(|x| x.id == l.id) {
                *existing = l.clone();
            }
            state.line_edit = Some(l.clone());
            state.dirty_line_ids.insert(l.id);
        }

        ui.horizontal(|ui| {
            let save_btn = egui::Button::new(
                egui::RichText::new("Simpan Garis")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(save_btn).clicked()
                && !state.offline_preview
                && let Some(auth) = current_auth(state)
            {
                api.send(ApiCommand::UpdateLine {
                    auth,
                    id: l.id,
                    req: UpdatePlotLineRequest {
                        name: l.name,
                        color: l.color,
                        width: l.width,
                        alpha: l.alpha,
                        geom: l.geom,
                    },
                });
            }
            let del_btn = egui::Button::new(
                egui::RichText::new("Hapus").color(egui::Color32::from_rgb(254, 202, 202)),
            )
            .fill(egui::Color32::from_rgb(127, 29, 29));
            if ui.add(del_btn).clicked() {
                state.request_delete_entity(SelectedEntity::Line(l.id));
            }
        });
    }
}

fn draw_text_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &PlaybackState,
    api: &ApiClientBridge,
) {
    let Some(orig) = state.text_edit.clone() else {
        return;
    };
    let mut t = orig.clone();
    let is_locked = state.locked_entity_ids.contains(&t.id);
    let editable = !playback.play_test_mode && !is_locked;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&t.text).size(13.5).strong().color(egui::Color32::WHITE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Focus").clicked() {
                let local = state.geo_anchor.lng_lat_alt_to_local(t.lng, t.lat, 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
            }
        });
    });
    ui.add_space(6.0);

    if is_locked && draw_locked_banner(ui, !playback.play_test_mode) {
        state.locked_entity_ids.remove(&t.id);
        return;
    }

    inspector_card(ui, "Text Label & Transform", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Text:");
            ui.add(egui::TextEdit::singleline(&mut t.text).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            color_hex_picker(ui, "Color:", &mut t.color);
            ui.horizontal(|ui| {
                ui.label("Font Size:");
                numeric_spinner(
                    ui,
                    ("tm_font_size", t.id),
                    &mut t.font_size,
                    SpinnerConfig {
                        min: 10.0,
                        max: 28.0,
                        step: 1.0,
                        drag_speed: 0.5,
                        decimals: 1,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                axis_badge(ui, "Lng", egui::Color32::from_rgb(185, 28, 28));
                numeric_spinner(
                    ui,
                    ("tm_lng", t.id),
                    &mut t.lng,
                    SpinnerConfig {
                        min: -180.0,
                        max: 180.0,
                        step: 0.0001,
                        drag_speed: 0.00002,
                        decimals: 6,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Lat", egui::Color32::from_rgb(29, 78, 216));
                numeric_spinner(
                    ui,
                    ("tm_lat", t.id),
                    &mut t.lat,
                    SpinnerConfig {
                        min: -85.0,
                        max: 85.0,
                        step: 0.0001,
                        drag_speed: 0.00002,
                        decimals: 6,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Alt", egui::Color32::from_rgb(21, 128, 61));
                numeric_spinner(
                    ui,
                    ("tm_alt", t.id),
                    &mut t.altitude,
                    SpinnerConfig {
                        min: -100.0,
                        max: 20_000.0,
                        step: 0.5,
                        drag_speed: 0.5,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
        });
    });

    if editable {
        let t_changed = t.text != orig.text
            || t.color != orig.color
            || (t.font_size - orig.font_size).abs() > 1e-6
            || (t.lng - orig.lng).abs() > 1e-9
            || (t.lat - orig.lat).abs() > 1e-9
            || (t.altitude - orig.altitude).abs() > 1e-6;
        if t_changed {
            if let Some(existing) = state.text_markers.iter_mut().find(|x| x.id == t.id) {
                *existing = t.clone();
            }
            state.text_edit = Some(t.clone());
            state.dirty_text_ids.insert(t.id);
        }

        ui.horizontal(|ui| {
            let save_btn = egui::Button::new(
                egui::RichText::new("Simpan Label")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(save_btn).clicked()
                && !state.offline_preview
                && let Some(auth) = current_auth(state)
            {
                api.send(ApiCommand::UpdateTextMarker {
                    auth,
                    id: t.id,
                    req: UpdateTextMarkerRequest {
                        text: t.text,
                        color: t.color,
                        font_size: t.font_size,
                        lng: t.lng,
                        lat: t.lat,
                        altitude: t.altitude,
                    },
                });
            }
            let del_btn = egui::Button::new(
                egui::RichText::new("Hapus").color(egui::Color32::from_rgb(254, 202, 202)),
            )
            .fill(egui::Color32::from_rgb(127, 29, 29));
            if ui.add(del_btn).clicked() {
                state.request_delete_entity(SelectedEntity::TextMarker(t.id));
            }
        });
    }
}

fn draw_anim_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
) {
    let Some(orig) = state.anim_edit.clone() else {
        return;
    };
    let mut a = orig.clone();
    let is_locked = state.locked_entity_ids.contains(&a.id);
    let editable = !playback.play_test_mode && !is_locked;

    ui.label(egui::RichText::new(&a.name).size(13.5).strong().color(egui::Color32::WHITE));
    ui.add_space(6.0);

    if is_locked && draw_locked_banner(ui, !playback.play_test_mode) {
        state.locked_entity_ids.remove(&a.id);
        return;
    }

    let markers_list: Vec<(Uuid, String)> = state
        .markers
        .iter()
        .map(|m| (m.id, m.name.clone()))
        .collect();
    let linked_name = a
        .marker_id
        .and_then(|mid| markers_list.iter().find(|(id, _)| *id == mid))
        .map(|(_, n)| n.clone())
        .unwrap_or_else(|| "(Tidak terikat unit)".to_string());

    inspector_card(ui, "Route Animation Track", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Target Unit:");
                egui::ComboBox::from_id_salt("anim_inspector_marker")
                    .selected_text(linked_name)
                    .width(175.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut a.marker_id, None, "(Tidak terikat)");
                        for (mid, mname) in &markers_list {
                            ui.selectable_value(&mut a.marker_id, Some(*mid), mname);
                        }
                    });
            });
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                cols[0].label(egui::RichText::new("Durasi (s)").size(10.5).color(egui::Color32::LIGHT_GRAY));
                numeric_spinner(
                    &mut cols[0],
                    ("anim_dur", a.id),
                    &mut a.duration_sec,
                    SpinnerConfig {
                        min: 1.0,
                        max: 600.0,
                        step: 1.0,
                        drag_speed: 0.5,
                        decimals: 1,
                        suffix: " s",
                        wrap_around: false,
                    },
                );
                cols[1].label(egui::RichText::new("Speed").size(10.5).color(egui::Color32::LIGHT_GRAY));
                numeric_spinner(
                    &mut cols[1],
                    ("anim_speed", a.id),
                    &mut a.speed,
                    SpinnerConfig {
                        min: 0.5,
                        max: 200.0,
                        step: 1.0,
                        drag_speed: 0.5,
                        decimals: 1,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label("Sort Order:");
                numeric_spinner_i32(ui, ("anim_sort", a.id), &mut a.sort_order, 0, 9999, 1);
            });
            ui.checkbox(&mut a.replay, "Loop / Replay");
            ui.checkbox(&mut a.autoplay, "Autoplay");
            ui.label(
                egui::RichText::new(format!("Waypoints: {}", a.geom.len()))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(110, 231, 183)),
            );
        });
    });

    if editable {
        let a_changed = a.name != orig.name
            || a.marker_id != orig.marker_id
            || (a.duration_sec - orig.duration_sec).abs() > 1e-6
            || (a.speed - orig.speed).abs() > 1e-6
            || a.sort_order != orig.sort_order
            || a.replay != orig.replay
            || a.autoplay != orig.autoplay;
        if a_changed {
            if let Some(existing) = state.anims.iter_mut().find(|x| x.id == a.id) {
                *existing = a.clone();
            }
            state.anim_edit = Some(a.clone());
            state.dirty_anim_ids.insert(a.id);
        }
    }

    ui.horizontal(|ui| {
        let test_btn = egui::Button::new(
            egui::RichText::new("Test Rute")
                .strong()
                .color(egui::Color32::WHITE),
        )
        .fill(egui::Color32::from_rgb(5, 150, 105));
        if ui.add(test_btn).clicked() {
            playback.play_all_anims = false;
            playback.active_anim_id = Some(a.id);
            playback.restart_play_test();
        }

        if editable {
            let save_btn = egui::Button::new(
                egui::RichText::new("Simpan")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(save_btn).clicked()
                && !state.offline_preview
                && let Some(auth) = current_auth(state)
            {
                api.send(ApiCommand::UpdateAnim {
                    auth,
                    id: a.id,
                    req: UpdateAnimRequest {
                        marker_id: a.marker_id,
                        name: a.name,
                        anim_type: a.anim_type,
                        sort_order: a.sort_order,
                        speed: a.speed,
                        duration_sec: a.duration_sec,
                        replay: a.replay,
                        autoplay: a.autoplay,
                        geom: a.geom,
                        data: a.data,
                    },
                });
            }

            let del_btn = egui::Button::new(
                egui::RichText::new("Hapus").color(egui::Color32::from_rgb(254, 202, 202)),
            )
            .fill(egui::Color32::from_rgb(127, 29, 29));
            if ui.add(del_btn).clicked() {
                state.request_delete_entity(SelectedEntity::Anim(a.id));
            }
        }
    });
}

fn draw_skenario_inspector(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    playback: &PlaybackState,
    api: &ApiClientBridge,
) {
    let Some(mut sk) = state.active_skenario.clone() else {
        ui.label(
            egui::RichText::new("Belum ada skenario aktif.")
                .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        return;
    };
    let editable = !playback.play_test_mode;

    inspector_card(ui, "Scene Root: Skenario", true, |ui| {
        ui.label(
            egui::RichText::new(&sk.name)
                .size(13.5)
                .strong()
                .color(egui::Color32::from_rgb(56, 189, 248)),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "Center: {:.5}, {:.5}  •  Zoom {:.1}",
                sk.center_lat, sk.center_lng, sk.zoom
            ))
            .size(11.5)
            .color(egui::Color32::from_rgb(203, 213, 225)),
        );
        ui.label(
            egui::RichText::new(format!(
                "Mode: {} • Layer: {}",
                sk.map_mode.as_str(),
                sk.base_layer.label()
            ))
            .size(11.5)
            .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        ui.add_space(6.0);

        ui.add_enabled_ui(editable, |ui| {
            let prev_mode = state.active_map_mode;
            let prev_layer = state.active_base_layer;
            let prev_skybox = state.active_skybox;
            ui.horizontal(|ui| {
                ui.label("Map Mode:");
                ui.selectable_value(&mut state.active_map_mode, MapMode::Outdoor, "Outdoor");
                ui.selectable_value(&mut state.active_map_mode, MapMode::Indoor, "Indoor");
            });
            ui.horizontal(|ui| {
                ui.label("Base Layer:");
                egui::ComboBox::from_id_salt("inspector_sk_layer")
                    .selected_text(state.active_base_layer.label())
                    .width(155.0)
                    .show_ui(ui, |ui| {
                        for layer in [
                            BaseLayer::Osm,
                            BaseLayer::Satellite,
                            BaseLayer::GridDark,
                            BaseLayer::GridLight,
                            BaseLayer::BlankDark,
                            BaseLayer::BlankLight,
                        ] {
                            ui.selectable_value(&mut state.active_base_layer, layer, layer.label());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Skybox:");
                egui::ComboBox::from_id_salt("inspector_sk_skybox")
                    .selected_text(state.active_skybox.label())
                    .width(165.0)
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Tanpa Skybox")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.selectable_value(
                            &mut state.active_skybox,
                            SkyboxPreset::None,
                            SkyboxPreset::None.label(),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Siang Hari")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(253, 224, 71)),
                        );
                        for preset in [
                            SkyboxPreset::DayClear,
                            SkyboxPreset::DayCloudy,
                            SkyboxPreset::DaySunset,
                        ] {
                            ui.selectable_value(&mut state.active_skybox, preset, preset.label());
                        }
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Malam Hari")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(147, 197, 253)),
                        );
                        for preset in [SkyboxPreset::NightStarry, SkyboxPreset::NightTactical] {
                            ui.selectable_value(&mut state.active_skybox, preset, preset.label());
                        }
                    });
            });
            if state.active_map_mode != prev_mode
                || state.active_base_layer != prev_layer
                || state.active_skybox != prev_skybox
            {
                state.skenario_dirty = true;
                if let Some(active) = &mut state.active_skenario {
                    active.map_mode = state.active_map_mode;
                    active.base_layer = state.active_base_layer;
                    active.skybox = state.active_skybox;
                }
            }
            if state.active_map_mode == MapMode::Indoor {
                ui.horizontal(|ui| {
                    ui.label("Grid Size (m):");
                    let mut grid = sk.indoor_config.grid_size_m.max(1.0);
                    if numeric_spinner(
                        ui,
                        "inspector_indoor_grid",
                        &mut grid,
                        SpinnerConfig {
                            min: 1.0,
                            max: 100.0,
                            step: 1.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    ) {
                        sk.indoor_config.grid_size_m = grid;
                        state.skenario_dirty = true;
                        if let Some(active) = &mut state.active_skenario {
                            active.indoor_config.grid_size_m = grid;
                        }
                    }
                });
            }
        });
    });

    inspector_card(ui, "Scene Statistics", true, |ui| {
        ui.columns(2, |cols| {
            cols[0].label(format!("3D Units: {}", state.markers.len()));
            cols[1].label(format!("Buildings: {}", state.buildings.len()));
            cols[0].label(format!("Plot Lines: {}", state.lines.len()));
            cols[1].label(format!("Text Labels: {}", state.text_markers.len()));
            cols[0].label(format!("Route Anims: {}", state.anims.len()));
            cols[1].label(format!("Particles: {}", state.particles.len()));
        });
    });

    if editable && !state.offline_preview {
        inspector_card(ui, "Default Camera View", true, |ui| {
            let zoom = tcam.current_zoom(820.0, state.geo_anchor.cos_lat);
            ui.label(
                egui::RichText::new(format!(
                    "Current Cam: Z{:.1} • Pitch {:.0}° • Bearing {:.0}°",
                    zoom, tcam.pitch_deg, tcam.bearing_deg
                ))
                .size(11.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            if ui
                .add(
                    egui::Button::new("Simpan Kamera Sebagai Default View")
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .min_size(egui::vec2(ui.available_width(), 26.0)),
                )
                .clicked()
            {
                let (lng, lat, _) = state
                    .geo_anchor
                    .local_to_lng_lat_alt([tcam.target_local.x, 0.0, tcam.target_local.y]);
                sk.center_lng = lng;
                sk.center_lat = lat;
                sk.zoom = zoom;
                sk.pitch = tcam.pitch_deg as f64;
                sk.bearing = tcam.bearing_deg as f64;
                sk.base_layer = state.active_base_layer;
                sk.map_mode = state.active_map_mode;
                sk.skybox = state.active_skybox;

                if let Some(auth) = current_auth(state) {
                    api.send(ApiCommand::UpdateSkenario {
                        auth,
                        id: sk.id,
                        req: UpdateSkenarioRequest {
                            name: sk.name.clone(),
                            center_lng: sk.center_lng,
                            center_lat: sk.center_lat,
                            zoom: sk.zoom,
                            min_zoom: sk.min_zoom,
                            max_zoom: sk.max_zoom,
                            pitch: sk.pitch,
                            bearing: sk.bearing,
                            map_mode: sk.map_mode,
                            base_layer: sk.base_layer,
                            skybox: sk.skybox,
                            indoor_config: sk.indoor_config.clone(),
                        },
                    });
                }
            }
        });
    }
}

fn draw_live_ops_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    ws: &WsClientBridge,
) {
    inspector_card(
        ui,
        &format!("Live Ops WebSocket ({} online)", state.online_operators.len()),
        false,
        |ui| {
            if let Some(sk) = &state.active_skenario {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Sync Camera").clicked() {
                        let (lng, lat, _) = state
                            .geo_anchor
                            .local_to_lng_lat_alt([tcam.target_local.x, 0.0, tcam.target_local.y]);
                        let zoom = tcam.current_zoom(820.0, state.geo_anchor.cos_lat);
                        ws.send(WsOutCommand::SendCommand(TacticalCommand {
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: None,
                            command: "camera_sync".into(),
                            payload: serde_json::json!({
                                "lng": lng,
                                "lat": lat,
                                "zoom": zoom,
                                "pitch": tcam.pitch_deg,
                                "bearing": tcam.bearing_deg,
                            }),
                            save_history: false,
                        }));
                    }
                    if ui.button("Broadcast Reload").clicked() {
                        ws.send(WsOutCommand::SendCommand(TacticalCommand {
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: None,
                            command: "reload".into(),
                            payload: serde_json::json!({}),
                            save_history: false,
                        }));
                    }
                });
            }

            if !state.online_operators.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Online Operators:").size(11.0).strong());
                for op in &state.online_operators {
                    ui.label(
                        egui::RichText::new(format!("• {} ({})", op.name, op.role.as_str()))
                            .size(10.5)
                            .color(egui::Color32::from_rgb(74, 222, 128)),
                    );
                }
            }

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Command Log:").size(11.0).strong());
            for entry in state.command_log.iter().take(10) {
                ui.label(
                    egui::RichText::new(format!(
                        "[{}] {} > {} {}",
                        entry.timestamp, entry.actor, entry.command, entry.detail
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
        },
    );
}
