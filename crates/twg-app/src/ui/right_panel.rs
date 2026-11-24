use bevy_egui::egui;
use twg_shared::{
    api::{
        CreateMarkerRequest, UpdateAnimRequest, UpdateBuildingRequest, UpdateMarkerRequest,
        UpdatePlotLineRequest, UpdateSkenarioRequest, UpdateTextMarkerRequest,
    },
    geo::haversine_distance_m,
    models::{BaseLayer, MapMode, MediaAssetType},
    particle_anim::{
        ParticleAnimConfig, ParticleAnimKind, is_particle_anim, parse_particle_anim_config,
        particle_anim_position, set_particle_anim_position, write_particle_anim_config,
    },
    route_formation::{
        FormationCornerMode, RouteFormationKind, anim_bound_marker_ids, anim_is_playable,
        is_multi_route_anim, parse_multi_route_config, write_multi_route_config,
    },
    shoot_anim::{is_water_cannon_anim, parse_water_cannon_config, write_water_cannon_config},
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
        playback::{route_estimated_duration_sec, route_total_length_m},
        polygon_edit::{
            PolygonEditState, ensure_editable_polygon_points, insert_midpoint_after,
        },
        route_edit::{RouteEditState, insert_route_midpoint_after},
    },
    state::{AppState, PlaybackState, SelectedEntity, SkyboxPreset, ToolMode},
    ui::{
        formation_pad::draw_formation_pad,
        spinner::{SpinnerConfig, numeric_spinner, numeric_spinner_u32_w},
    },
};

#[allow(clippy::too_many_arguments)]
pub fn draw_right_panel(
    viewport_ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    poly_edit: &mut PolygonEditState,
    route_edit: &mut RouteEditState,
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
                            draw_anim_inspector(ui, state, tcam, playback, route_edit, api)
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
            let card_title = if state.new_route_is_multi {
                "Langkah 1/2: Gambar Garis Rute (Multiple)"
            } else {
                "Langkah 1/2: Gambar Garis Rute (Tunggal)"
            };
            inspector_card(ui, card_title, true, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Tipe:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .selectable_label(!state.new_route_is_multi, "Unit Tunggal")
                        .clicked()
                    {
                        state.new_route_is_multi = false;
                    }
                    if ui
                        .selectable_label(state.new_route_is_multi, "Unit Multiple")
                        .clicked()
                    {
                        state.new_route_is_multi = true;
                    }
                });
                ui.add_space(4.0);
                ui.label("Nama Animasi Rute:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_route_name)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Kecepatan (m/s):")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner(
                        ui,
                        "tool_route_speed",
                        &mut state.new_route_speed,
                        SpinnerConfig {
                            min: 0.5,
                            max: 200.0,
                            step: 0.5,
                            drag_speed: 0.25,
                            decimals: 1,
                            suffix: " m/s",
                            wrap_around: false,
                        },
                    );
                });
                ui.checkbox(&mut state.new_route_loop, "Repeat / Ulangi dari Awal");
                ui.add_space(3.0);

                let pt_count = state.draft_points_lnglat.len();
                let draft_len_m: f64 = state
                    .draft_points_lnglat
                    .windows(2)
                    .map(|w| haversine_distance_m(w[0][0], w[0][1], w[1][0], w[1][1]))
                    .sum();
                ui.label(
                    egui::RichText::new(format!(
                        "Titik rute: {pt_count}  •  Panjang: {draft_len_m:.1} m"
                    ))
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 231, 183)),
                );
                ui.label(
                    egui::RichText::new(if state.new_route_is_multi {
                        "Klik titik-titik rute di peta (min 2). Setelah selesai, klik beberapa Model 3D di peta untuk dimasukkan ke formasi."
                    } else {
                        "Klik titik-titik rute di peta (min 2). Setelah selesai, klik Model 3D di peta untuk mengikat rute."
                    })
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(pt_count > 0, egui::Button::new("Undo").small())
                        .on_hover_text("Hapus titik terakhir (Backspace)")
                        .clicked()
                    {
                        state.draft_points_lnglat.pop();
                    }
                    let fin = egui::Button::new(
                        egui::RichText::new("Selesai & Pilih Unit ->")
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .small();
                    if ui.add_enabled(pt_count >= 2, fin).clicked() {
                        finish_active_draft(state, api);
                    }
                    if ui.small_button("Batal").clicked() {
                        state.draft_points_lnglat.clear();
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::PickRouteMarker => {
            let picked_anim = state
                .picking_marker_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .cloned();
            let is_multi = picked_anim.as_ref().is_some_and(is_multi_route_anim);
            let title = if is_multi {
                "Langkah 2/2: Pilih Beberapa Unit di Peta"
            } else {
                "Langkah 2/2: Pilih Model 3D di Peta"
            };
            inspector_card(ui, title, true, |ui| {
                let route_title = picked_anim
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Rute Baru".to_string());
                ui.label(
                    egui::RichText::new(format!("Rute: {route_title}"))
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                );
                ui.add_space(3.0);
                if is_multi {
                    let count = picked_anim
                        .as_ref()
                        .map(|a| anim_bound_marker_ids(a).len())
                        .unwrap_or(0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Terpilih: {count} unit. Klik Model 3D di peta untuk menambah/melepas unit dari formasi."
                        ))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add_space(6.0);
                    let done_btn = egui::Button::new(
                        egui::RichText::new("Selesai Pilih Unit (Enter)")
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .min_size(egui::vec2(ui.available_width(), 26.0));
                    if ui.add(done_btn).clicked() {
                        state.picking_marker_for_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    }
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Klik langsung pada Model 3D / Unit di peta untuk menjadikan objek tersebut target animasi rute ini.",
                        )
                        .size(11.0)
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add_space(6.0);
                    if ui.small_button("Batal / Lewati (Esc)").clicked() {
                        state.picking_marker_for_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    }
                }
            });
        }
        ToolMode::PickShootMarker => {
            let rebinding_name = state
                .picking_shoot_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .map(|a| a.name.clone());
            let title = if rebinding_name.is_some() {
                "Ganti Unit Water Cannon di Peta"
            } else {
                "Buat Water Cannon: Pilih 1 Model 3D"
            };
            inspector_card(ui, title, true, |ui| {
                if let Some(rname) = rebinding_name {
                    ui.label(
                        egui::RichText::new(format!("Animasi: {rname}"))
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(56, 189, 248)),
                    );
                } else {
                    ui.label("Nama Animasi Water Cannon:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_shoot_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Klik 1 kendaraan / Model 3D di peta (1 animasi = 1 unit). Tinggi nozzle otomatis menyesuaikan atap model dan preview semprotan langsung menyala.",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(6.0);
                if ui.small_button("Batal (Esc)").clicked() {
                    state.picking_shoot_for_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                }
            });
        }
        ToolMode::PlaceParticle => {
            inspector_card(
                ui,
                &format!("Letakkan Partikel: {}", state.new_particle_kind.label()),
                true,
                |ui| {
                    ui.label(
                        egui::RichText::new("Jenis Partikel:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.horizontal(|ui| {
                        for &kind in ParticleAnimKind::ALL {
                            let active = state.new_particle_kind == kind;
                            if ui.selectable_label(active, kind.short_tag()).clicked() {
                                let count = state
                                    .anims
                                    .iter()
                                    .filter(|a| {
                                        ParticleAnimKind::from_anim_type(&a.anim_type)
                                            == Some(kind)
                                    })
                                    .count()
                                    + 1;
                                state.new_particle_kind = kind;
                                state.new_particle_name = kind.default_name(count);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label("Nama Partikel Baru:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_particle_name)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Klik 1 titik di peta 3D untuk menempatkan partikel. Setelah dibuat, Inspector & Move Gizmo 3D (X / Alt / Z) langsung aktif.",
                        )
                        .size(11.0)
                        .color(egui::Color32::from_rgb(253, 186, 116)),
                    );
                    ui.add_space(6.0);
                    if ui.small_button("Batal (Esc)").clicked() {
                        state.tool_mode = ToolMode::Select;
                    }
                },
            );
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
                                                                    // Clear right margin so floating scrollbar never overlaps delete button
                                                                    ui.add_space(14.0);
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
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    route_edit: &mut RouteEditState,
    api: &ApiClientBridge,
) {
    let Some(orig) = state.anim_edit.clone() else {
        return;
    };
    let mut a = orig.clone();
    let is_locked = state.locked_entity_ids.contains(&a.id);
    let editable = !playback.play_test_mode && !is_locked;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&a.name)
                .size(13.5)
                .strong()
                .color(egui::Color32::WHITE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let focus_target = a
                .geom
                .first()
                .map(|p| (p[0], p[1]))
                .or_else(|| {
                    a.marker_id
                        .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                        .map(|m| (m.lng, m.lat))
                });
            if ui
                .add_enabled(focus_target.is_some(), egui::Button::new("Focus").small())
                .clicked()
                && let Some((lng, lat)) = focus_target
            {
                let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
            }
        });
    });
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
    if is_particle_anim(&a) {
        draw_particle_inspector_cards(ui, state, &mut a, editable);
    } else if is_water_cannon_anim(&a) {
        draw_water_cannon_inspector_cards(ui, state, route_edit, &mut a, &markers_list, editable);
    } else {
    let is_multi = is_multi_route_anim(&a);
    let linked_name = a
        .marker_id
        .and_then(|mid| markers_list.iter().find(|(id, _)| *id == mid))
        .map(|(_, n)| n.clone());

    let settings_title = if is_multi {
        "Rute Unit Multiple — Pengaturan"
    } else {
        "Rute Unit Tunggal — Pengaturan"
    };
    inspector_card(ui, settings_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(6.0);

            if !is_multi {
                // Single-unit target 3D Marker with direct map-pick button + dropdown fallback
                ui.label(
                    egui::RichText::new("Target Model 3D / Unit:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.add_space(2.0);
                let is_picking_this = state.tool_mode == ToolMode::PickRouteMarker
                    && state.picking_marker_for_anim_id == Some(a.id);
                let pick_btn = egui::Button::new(
                    egui::RichText::new(if is_picking_this {
                        "Klik Model 3D di Peta... (Batal)"
                    } else if linked_name.is_some() {
                        "Ganti / Pilih Unit di Peta"
                    } else {
                        "Pilih Unit / Model 3D di Peta"
                    })
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
                )
                .fill(if is_picking_this {
                    egui::Color32::from_rgb(180, 83, 9)
                } else {
                    egui::Color32::from_rgb(14, 116, 144)
                })
                .min_size(egui::vec2(ui.available_width(), 26.0));
                if ui.add(pick_btn).clicked() {
                    if is_picking_this {
                        state.tool_mode = ToolMode::Select;
                        state.picking_marker_for_anim_id = None;
                    } else {
                        route_edit.clear();
                        state.picking_marker_for_anim_id = Some(a.id);
                        state.tool_mode = ToolMode::PickRouteMarker;
                        state.status_banner = Some(format!(
                            "Klik Model 3D / Unit di peta untuk mengikat rute '{}' (Esc untuk batal).",
                            a.name
                        ));
                    }
                }
                ui.add_space(3.0);
                egui::ComboBox::from_id_salt("anim_inspector_marker")
                    .selected_text(
                        linked_name
                            .clone()
                            .unwrap_or_else(|| "(Belum pilih Model 3D)".to_string()),
                    )
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut a.marker_id, None, "(Belum terikat)");
                        for (mid, mname) in &markers_list {
                            ui.selectable_value(&mut a.marker_id, Some(*mid), mname);
                        }
                    });
                ui.add_space(6.0);
            }

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kecepatan (m/s):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if numeric_spinner(
                    ui,
                    ("anim_speed", a.id),
                    &mut a.speed,
                    SpinnerConfig {
                        min: 0.5,
                        max: 200.0,
                        step: 0.5,
                        drag_speed: 0.25,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                ) {
                    a.duration_sec = route_estimated_duration_sec(&a.geom, a.speed);
                }
            });
            ui.add_space(3.0);
            ui.checkbox(&mut a.replay, "Repeat / Ulangi dari Awal");
            ui.add_space(3.0);
            let order_pos = state
                .anims
                .iter()
                .position(|x| x.id == a.id)
                .map(|i| i + 1)
                .unwrap_or(a.sort_order.max(1) as usize);
            ui.label(
                egui::RichText::new(format!(
                    "Urutan Skenario: #{order_pos}  (atur cepat via Up / Down di tab Animasi)"
                ))
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );

            let len_m = route_total_length_m(&a.geom);
            let est_s = route_estimated_duration_sec(&a.geom, a.speed);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "Panjang: {len_m:.1} m  •  Estimasi: {est_s:.1} dtk  •  {} titik",
                    a.geom.len()
                ))
                .size(11.0)
                .color(egui::Color32::from_rgb(110, 231, 183)),
            );
        });
    });

    // Multi-Unit Formation Cards (Unit List + Formation Settings + 2D Interactive Formation Pad)
    if is_multi {
        let mut cfg = parse_multi_route_config(&a);
        let mut cfg_changed = false;

        let unit_card_title = format!("Daftar Unit Formasi ({} unit)", cfg.marker_ids.len());
        inspector_card(ui, &unit_card_title, true, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                let is_picking_this = state.tool_mode == ToolMode::PickRouteMarker
                    && state.picking_marker_for_anim_id == Some(a.id);
                let pick_btn = egui::Button::new(
                    egui::RichText::new(if is_picking_this {
                        "Selesai Pilih Unit di Peta (Enter)"
                    } else {
                        "+ Pilih / Ubah Unit di Peta"
                    })
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
                )
                .fill(if is_picking_this {
                    egui::Color32::from_rgb(5, 150, 105)
                } else {
                    egui::Color32::from_rgb(14, 116, 144)
                })
                .min_size(egui::vec2(ui.available_width(), 26.0));
                if ui.add(pick_btn).clicked() {
                    if is_picking_this {
                        state.tool_mode = ToolMode::Select;
                        state.picking_marker_for_anim_id = None;
                    } else {
                        route_edit.clear();
                        state.picking_marker_for_anim_id = Some(a.id);
                        state.tool_mode = ToolMode::PickRouteMarker;
                        state.status_banner = Some(format!(
                            "Klik beberapa Model 3D / Unit di peta untuk dimasukkan/dilepas dari '{}' (Enter / Esc selesai).",
                            a.name
                        ));
                    }
                }

                // Quick dropdown to add a marker without leaving Inspector
                let unadded: Vec<(Uuid, String)> = markers_list
                    .iter()
                    .filter(|(id, _)| !cfg.marker_ids.contains(id))
                    .cloned()
                    .collect();
                if !unadded.is_empty() {
                    ui.add_space(3.0);
                    let mut add_from_dropdown: Option<Uuid> = None;
                    egui::ComboBox::from_id_salt(("multi_route_add_unit", a.id))
                        .selected_text("+ Tambah Unit dari Daftar...")
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (mid, mname) in &unadded {
                                if ui.selectable_label(false, mname).clicked() {
                                    add_from_dropdown = Some(*mid);
                                }
                            }
                        });
                    if let Some(new_mid) = add_from_dropdown {
                        cfg.toggle_marker(new_mid);
                        state.formation_pad_selected_marker = Some(new_mid);
                        cfg_changed = true;
                    }
                }

                ui.add_space(5.0);
                if cfg.marker_ids.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Belum ada unit terpilih. Klik '+ Pilih / Ubah Unit di Peta' lalu klik beberapa Model 3D.",
                        )
                        .size(10.5)
                        .color(egui::Color32::from_rgb(251, 191, 36)),
                    );
                } else {
                    let mut move_action: Option<(Uuid, bool)> = None;
                    let mut remove_mid: Option<Uuid> = None;
                    let unit_cnt = cfg.marker_ids.len();
                    let ids_snapshot = cfg.marker_ids.clone();
                    egui::ScrollArea::vertical()
                        .id_salt(("multi_route_units_list", a.id))
                        .max_height(135.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for (idx, mid) in ids_snapshot.iter().enumerate() {
                                let uname = markers_list
                                    .iter()
                                    .find(|(id, _)| id == mid)
                                    .map(|(_, n)| n.as_str())
                                    .unwrap_or("(Unit terhapus)");
                                let is_sel_unit =
                                    state.formation_pad_selected_marker == Some(*mid);
                                ui.horizontal(|ui| {
                                    let role_tag = if idx == 0 {
                                        format!("#1 {uname} (Lead)")
                                    } else {
                                        format!("#{} {uname}", idx + 1)
                                    };
                                    if ui
                                        .selectable_label(
                                            is_sel_unit,
                                            egui::RichText::new(role_tag).size(10.5).color(
                                                if is_sel_unit {
                                                    egui::Color32::from_rgb(253, 224, 71)
                                                } else if idx == 0 {
                                                    egui::Color32::from_rgb(103, 232, 249)
                                                } else {
                                                    egui::Color32::from_rgb(226, 232, 240)
                                                },
                                            ),
                                        )
                                        .clicked()
                                    {
                                        state.formation_pad_selected_marker = Some(*mid);
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(14.0);
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("x")
                                                            .size(10.0)
                                                            .strong()
                                                            .color(egui::Color32::from_rgb(
                                                                254, 202, 202,
                                                            )),
                                                    )
                                                    .fill(egui::Color32::from_rgb(110, 28, 28))
                                                    .small(),
                                                )
                                                .on_hover_text("Lepas unit dari rute")
                                                .clicked()
                                            {
                                                remove_mid = Some(*mid);
                                            }
                                            if ui
                                                .add_enabled(
                                                    idx + 1 < unit_cnt,
                                                    egui::Button::new(
                                                        egui::RichText::new("v").size(9.5),
                                                    )
                                                    .small(),
                                                )
                                                .on_hover_text("Turunkan urutan unit")
                                                .clicked()
                                            {
                                                move_action = Some((*mid, false));
                                            }
                                            if ui
                                                .add_enabled(
                                                    idx > 0,
                                                    egui::Button::new(
                                                        egui::RichText::new("^").size(9.5),
                                                    )
                                                    .small(),
                                                )
                                                .on_hover_text("Naikkan urutan unit")
                                                .clicked()
                                            {
                                                move_action = Some((*mid, true));
                                            }
                                        },
                                    );
                                });
                            }
                        });
                    if let Some((mid, up)) = move_action
                        && cfg.move_marker_order(mid, up)
                    {
                        cfg_changed = true;
                    }
                    if let Some(mid) = remove_mid {
                        cfg.toggle_marker(mid);
                        cfg_changed = true;
                    }
                }
            });
        });

        inspector_card(ui, "Formasi & Posisi Unit", true, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                // Formation Preset Selector (5 Tactical Formations)
                let prev_formation = cfg.formation;
                ui.label(
                    egui::RichText::new("Tipe Formasi:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                egui::ComboBox::from_id_salt(("multi_route_formation_kind", a.id))
                    .selected_text(cfg.formation.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for &kind in RouteFormationKind::ALL {
                            ui.selectable_value(&mut cfg.formation, kind, kind.label());
                        }
                    });
                if cfg.formation != prev_formation {
                    if cfg.formation == RouteFormationKind::Custom {
                        cfg.ensure_custom_offsets_seeded(prev_formation);
                    }
                    cfg_changed = true;
                }

                ui.add_space(4.0);
                // Corner Behavior Mode Selector (Follow-Path vs Rigid)
                let prev_corner = cfg.corner_mode;
                ui.label(
                    egui::RichText::new("Gerak di Tikungan:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                egui::ComboBox::from_id_salt(("multi_route_corner_mode", a.id))
                    .selected_text(cfg.corner_mode.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for &cm in FormationCornerMode::ALL {
                            ui.selectable_value(&mut cfg.corner_mode, cm, cm.label());
                        }
                    });
                if cfg.corner_mode != prev_corner {
                    cfg_changed = true;
                }

                ui.add_space(5.0);
                // Dynamic parameters based on active formation
                match cfg.formation {
                    RouteFormationKind::Grid => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jumlah Kolom:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner_u32_w(
                                ui,
                                ("multi_cols", a.id),
                                &mut cfg.columns,
                                92.0,
                                1,
                                64,
                                1,
                                0.2,
                            ) {
                                cfg_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Samping:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                ui,
                                ("multi_sx", a.id),
                                &mut cfg.spacing_x_m,
                                SpinnerConfig {
                                    min: 0.5,
                                    max: 100.0,
                                    step: 0.5,
                                    drag_speed: 0.15,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            ) {
                                cfg_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Belakang:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                ui,
                                ("multi_sz", a.id),
                                &mut cfg.spacing_z_m,
                                SpinnerConfig {
                                    min: 0.5,
                                    max: 100.0,
                                    step: 0.5,
                                    drag_speed: 0.15,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            ) {
                                cfg_changed = true;
                            }
                        });
                    }
                    RouteFormationKind::Wedge | RouteFormationKind::Staggered => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Samping:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                ui,
                                ("multi_sx_ws", a.id),
                                &mut cfg.spacing_x_m,
                                SpinnerConfig {
                                    min: 0.5,
                                    max: 100.0,
                                    step: 0.5,
                                    drag_speed: 0.15,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            ) {
                                cfg_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Belakang:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                ui,
                                ("multi_sz_ws", a.id),
                                &mut cfg.spacing_z_m,
                                SpinnerConfig {
                                    min: 0.5,
                                    max: 100.0,
                                    step: 0.5,
                                    drag_speed: 0.15,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            ) {
                                cfg_changed = true;
                            }
                        });
                    }
                    RouteFormationKind::VipEscort => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Radius Kawal:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            if numeric_spinner(
                                ui,
                                ("multi_vip_r", a.id),
                                &mut cfg.vip_radius_m,
                                SpinnerConfig {
                                    min: 1.0,
                                    max: 100.0,
                                    step: 0.5,
                                    drag_speed: 0.15,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            ) {
                                cfg_changed = true;
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                "Unit #1 (Lead) berada di tengah, unit lain melingkar mengawal.",
                            )
                            .size(9.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                    RouteFormationKind::Custom => {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let cap_btn = egui::Button::new(
                                egui::RichText::new("Ambil dari Peta")
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105))
                            .small();
                            if ui
                                .add_enabled(!cfg.marker_ids.is_empty() && a.geom.len() >= 2, cap_btn)
                                .on_hover_text(
                                    "Rekam posisi relatif unit-unit di peta saat ini sebagai formasi kustom",
                                )
                                .clicked()
                            {
                                let n = cfg.capture_custom_from_map(&state.markers, &a.geom);
                                if n > 0 {
                                    cfg_changed = true;
                                    state.status_banner = Some(format!(
                                        "Posisi {n} unit dari peta berhasil direkam ke Formasi Bebas!"
                                    ));
                                }
                            }

                            let rnd_btn = egui::Button::new(
                                egui::RichText::new("Acak Rapi")
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(124, 58, 237))
                            .small();
                            if ui
                                .add_enabled(!cfg.marker_ids.is_empty(), rnd_btn)
                                .on_hover_text("Sebar acak posisi unit tanpa tumpang tindih")
                                .clicked()
                            {
                                state.formation_random_seed =
                                    state.formation_random_seed.wrapping_add(1);
                                let mix = state.formation_random_seed
                                    ^ (a.id.as_u128() as u64);
                                cfg.randomize_custom_offsets(mix);
                                cfg_changed = true;
                            }

                            if ui
                                .add_enabled(
                                    !cfg.marker_ids.is_empty(),
                                    egui::Button::new(
                                        egui::RichText::new("Reset Grid").size(10.0),
                                    )
                                    .small(),
                                )
                                .on_hover_text("Atur ulang posisi bebas ke susunan Grid rapi")
                                .clicked()
                            {
                                cfg.copy_preset_into_custom(RouteFormationKind::Grid);
                                cfg_changed = true;
                            }
                        });
                    }
                }

                ui.add_space(6.0);
                // Interactive 2D Formation Pad (Drag-and-Drop in Custom, Live Diagram in Presets)
                if draw_formation_pad(
                    ui,
                    a.id,
                    &mut cfg,
                    &state.markers,
                    &mut state.formation_pad_selected_marker,
                    &mut state.formation_pad_snap_m,
                    editable,
                ) {
                    cfg_changed = true;
                }
            });
        });

        if cfg_changed {
            write_multi_route_config(&mut a, &cfg);
        }
    }

    // Card 2: Interactive Route Polyline & Waypoint Editor (Auto-active on map + Inspector Spinners)
    inspector_card(ui, "Edit Garis Rute (Peta & Titik)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            let is_editing_map = route_edit.is_editing(a.id);
            if !is_editing_map {
                let edit_btn = egui::Button::new(
                    egui::RichText::new("Aktifkan Edit Titik di Peta")
                        .size(11.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(37, 99, 235))
                .min_size(egui::vec2(ui.available_width(), 27.0));
                if ui.add(edit_btn).clicked() {
                    route_edit.editing_anim_id = Some(a.id);
                    route_edit.selected_waypoint = None;
                    route_edit.add_point_on_map_mode = false;
                    state.tool_mode = ToolMode::Select;
                    state.picking_marker_for_anim_id = None;
                }
            } else {
                ui.label(
                    egui::RichText::new(
                        "• Tarik langsung titik (#1..#N) di peta, atau klik untuk Move Gizmo (X / Alt / Z)\n• Klik (+) hijau di garis untuk belokan baru, atau klik (x) merah / Del untuk hapus titik",
                    )
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(4.0);
                let add_active = route_edit.add_point_on_map_mode;
                let add_btn = egui::Button::new(
                    egui::RichText::new(if add_active {
                        "+ Klik Peta untuk Tambah Titik [AKTIF]"
                    } else {
                        "+ Tambah Titik di Ujung (Klik Peta)"
                    })
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::WHITE),
                )
                .fill(if add_active {
                    egui::Color32::from_rgb(16, 185, 129)
                } else {
                    egui::Color32::from_rgb(30, 58, 138)
                })
                .min_size(egui::vec2(ui.available_width(), 24.0));
                if ui.add(add_btn).clicked() {
                    route_edit.add_point_on_map_mode = !route_edit.add_point_on_map_mode;
                }
            }

            // Selected waypoint precision coordinate spinners + quick Delete button
            let mut remove_selected_idx: Option<usize> = None;
            if let Some(wi) = route_edit.selected_waypoint
                && wi < a.geom.len()
            {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 38))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Koordinat Titik #{}", wi + 1))
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let del_sel = egui::Button::new(
                                        egui::RichText::new("Hapus Titik")
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(254, 202, 202)),
                                    )
                                    .fill(egui::Color32::from_rgb(127, 29, 29))
                                    .small();
                                    if ui
                                        .add_enabled(a.geom.len() > 2, del_sel)
                                        .on_hover_text(
                                            "Hapus titik terpilih (bisa juga klik (x) merah di peta atau tekan Del)",
                                        )
                                        .clicked()
                                    {
                                        remove_selected_idx = Some(wi);
                                    }
                                },
                            );
                        });
                        ui.add_space(3.0);
                        let mut pt = a.geom[wi];
                        let mut pt_changed = false;
                        ui.horizontal(|ui| {
                            axis_badge(ui, "Lng", egui::Color32::from_rgb(185, 28, 28));
                            pt_changed |= numeric_spinner(
                                ui,
                                ("route_wp_lng", a.id, wi),
                                &mut pt[0],
                                SpinnerConfig {
                                    min: -180.0,
                                    max: 180.0,
                                    step: 0.00001,
                                    drag_speed: 0.000005,
                                    decimals: 6,
                                    suffix: "",
                                    wrap_around: false,
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            axis_badge(ui, "Lat", egui::Color32::from_rgb(29, 78, 216));
                            pt_changed |= numeric_spinner(
                                ui,
                                ("route_wp_lat", a.id, wi),
                                &mut pt[1],
                                SpinnerConfig {
                                    min: -85.0,
                                    max: 85.0,
                                    step: 0.00001,
                                    drag_speed: 0.000005,
                                    decimals: 6,
                                    suffix: "",
                                    wrap_around: false,
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            axis_badge(ui, "Alt", egui::Color32::from_rgb(21, 128, 61));
                            pt_changed |= numeric_spinner(
                                ui,
                                ("route_wp_alt", a.id, wi),
                                &mut pt[2],
                                SpinnerConfig {
                                    min: -50.0,
                                    max: 2000.0,
                                    step: 0.5,
                                    drag_speed: 0.2,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        });
                        if pt_changed {
                            a.geom[wi] = pt;
                            a.duration_sec = route_estimated_duration_sec(&a.geom, a.speed);
                        }
                    });
            }

            // Waypoint list with insert (+) and delete (x)
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Daftar Titik ({} waypoints):", a.geom.len()))
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(2.0);
            let mut insert_after: Option<usize> = None;
            let mut remove_idx: Option<usize> = remove_selected_idx;
            let wp_count = a.geom.len();
            egui::ScrollArea::vertical()
                .id_salt(("route_wp_list", a.id))
                .max_height(155.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (i, pt) in a.geom.iter().enumerate() {
                        let is_wp_sel =
                            route_edit.is_editing(a.id) && route_edit.selected_waypoint == Some(i);
                        ui.horizontal(|ui| {
                            let label = if pt[2].abs() > 0.05 {
                                format!("#{}  {:.5}, {:.5} ({:.0}m)", i + 1, pt[1], pt[0], pt[2])
                            } else {
                                format!("#{}  {:.5}, {:.5}", i + 1, pt[1], pt[0])
                            };
                            if ui
                                .selectable_label(
                                    is_wp_sel,
                                    egui::RichText::new(label).size(10.0).color(if is_wp_sel {
                                        egui::Color32::from_rgb(253, 224, 71)
                                    } else {
                                        egui::Color32::from_rgb(203, 213, 225)
                                    }),
                                )
                                .clicked()
                            {
                                route_edit.editing_anim_id = Some(a.id);
                                route_edit.selected_waypoint = Some(i);
                                state.tool_mode = ToolMode::Select;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Reserve right-side space so the vertical scrollbar never overlaps the 'x' button!
                                    ui.add_space(14.0);
                                    if ui
                                        .add_enabled(
                                            wp_count > 2,
                                            egui::Button::new(
                                                egui::RichText::new("x")
                                                    .size(10.0)
                                                    .strong()
                                                    .color(egui::Color32::from_rgb(254, 202, 202)),
                                            )
                                            .fill(if wp_count > 2 {
                                                egui::Color32::from_rgb(110, 28, 28)
                                            } else {
                                                egui::Color32::from_rgb(35, 44, 60)
                                            })
                                            .small(),
                                        )
                                        .on_hover_text("Hapus titik ini (min 2 titik)")
                                        .clicked()
                                    {
                                        remove_idx = Some(i);
                                    }
                                    if i + 1 < wp_count
                                        && ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("+")
                                                        .size(10.0)
                                                        .strong()
                                                        .color(egui::Color32::from_rgb(
                                                            110, 231, 183,
                                                        )),
                                                )
                                                .small(),
                                            )
                                            .on_hover_text(format!(
                                                "Sisipkan titik baru setelah #{}",
                                                i + 1
                                            ))
                                            .clicked()
                                    {
                                        insert_after = Some(i);
                                    }
                                },
                            );
                        });
                    }
                });

            if let Some(seg) = insert_after {
                let new_i = insert_route_midpoint_after(&mut a.geom, seg);
                route_edit.editing_anim_id = Some(a.id);
                route_edit.selected_waypoint = Some(new_i);
                a.duration_sec = route_estimated_duration_sec(&a.geom, a.speed);
            }
            if let Some(idx) = remove_idx
                && a.geom.len() > 2
                && idx < a.geom.len()
            {
                a.geom.remove(idx);
                if let Some(sel) = route_edit.selected_waypoint {
                    if sel == idx {
                        route_edit.selected_waypoint = Some(idx.min(a.geom.len() - 1));
                    } else if sel > idx {
                        route_edit.selected_waypoint = Some(sel - 1);
                    }
                }
                a.duration_sec = route_estimated_duration_sec(&a.geom, a.speed);
            }
        });
    });
    }

    if editable {
        let a_changed = a.name != orig.name
            || a.anim_type != orig.anim_type
            || a.marker_id != orig.marker_id
            || (a.duration_sec - orig.duration_sec).abs() > 1e-6
            || (a.speed - orig.speed).abs() > 1e-6
            || a.sort_order != orig.sort_order
            || a.replay != orig.replay
            || a.autoplay != orig.autoplay
            || a.geom != orig.geom
            || a.data != orig.data;
        if a_changed {
            if let Some(existing) = state.anims.iter_mut().find(|x| x.id == a.id) {
                *existing = a.clone();
            }
            state.anim_edit = Some(a.clone());
            state.dirty_anim_ids.insert(a.id);
        }
    }

    let can_play = anim_is_playable(&a, &state.markers);
    ui.horizontal(|ui| {
        let test_btn = egui::Button::new(
            egui::RichText::new("Play di Skenario")
                .strong()
                .color(egui::Color32::WHITE),
        )
        .fill(if can_play {
            egui::Color32::from_rgb(5, 150, 105)
        } else {
            egui::Color32::from_rgb(51, 65, 85)
        });
        let disabled_hint = if is_particle_anim(&a) {
            "Tentukan posisi partikel di peta terlebih dahulu"
        } else if is_water_cannon_anim(&a) {
            "Pilih 1 Model 3D sumber Water Cannon terlebih dahulu"
        } else {
            "Pilih Model 3D dan minimal 2 titik rute terlebih dahulu"
        };
        if ui
            .add_enabled(can_play, test_btn)
            .on_disabled_hover_text(disabled_hint)
            .clicked()
        {
            route_edit.clear();
            playback.play_single_immediately(a.id);
        }

        if editable {
            let save_btn = egui::Button::new(
                egui::RichText::new("Simpan")
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(14, 116, 144));
            if ui.add(save_btn).clicked() {
                if !state.offline_preview
                    && let Some(auth) = current_auth(state)
                {
                    api.send(ApiCommand::UpdateAnim {
                        auth,
                        id: a.id,
                        req: UpdateAnimRequest {
                            marker_id: a.marker_id,
                            name: a.name.clone(),
                            anim_type: a.anim_type.clone(),
                            sort_order: a.sort_order,
                            speed: a.speed,
                            duration_sec: a.duration_sec,
                            replay: a.replay,
                            autoplay: a.autoplay,
                            geom: a.geom.clone(),
                            data: a.data.clone(),
                        },
                    });
                    state.dirty_anim_ids.remove(&a.id);
                } else {
                    state.dirty_anim_ids.remove(&a.id);
                    state.status_banner =
                        Some(format!("Animasi '{}' disimpan (Offline).", a.name));
                }
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

fn draw_particle_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    a: &mut twg_shared::models::Anim,
    editable: bool,
) {
    let mut cfg = parse_particle_anim_config(a);
    let mut cfg_changed = false;

    // Card 1: Identity, Particle Type & Live Preview Control
    inspector_card(
        ui,
        &format!("Particle: {} — Identitas", cfg.kind.label()),
        true,
        |ui| {
            ui.add_enabled_ui(editable, |ui| {
                // Live preview indicator banner
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(67, 25, 10))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(251, 146, 60)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(
                                "LIVE PREVIEW & MOVE GIZMO AKTIF — Geser langsung panah X / Alt / Z di peta",
                            )
                            .size(9.5)
                            .strong()
                            .color(egui::Color32::from_rgb(253, 186, 116)),
                        );
                    });
                ui.add_space(5.0);

                ui.label("Nama Partikel:");
                ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
                ui.add_space(5.0);

                // Particle sub-type switcher
                let prev_kind = cfg.kind;
                ui.label(
                    egui::RichText::new("Tipe Efek Partikel:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                egui::ComboBox::from_id_salt(("particle_kind_combo", a.id))
                    .selected_text(cfg.kind.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for &k in ParticleAnimKind::ALL {
                            ui.selectable_value(&mut cfg.kind, k, k.label());
                        }
                    });
                if cfg.kind != prev_kind {
                    let keep_always = cfg.always_preview_in_editor;
                    let keep_wind_dir = cfg.wind_dir_deg;
                    let keep_wind_spd = cfg.wind_speed_mps;
                    cfg = ParticleAnimConfig::default_for(cfg.kind);
                    cfg.always_preview_in_editor = keep_always;
                    cfg.wind_dir_deg = keep_wind_dir;
                    cfg.wind_speed_mps = keep_wind_spd;
                    a.anim_type = cfg.kind.anim_type_str().to_string();
                    a.duration_sec = ParticleAnimConfig::default_duration_sec(cfg.kind);
                    state.particle_preview_reset_nonce =
                        state.particle_preview_reset_nonce.wrapping_add(1);
                    cfg_changed = true;
                }

                ui.add_space(4.0);
                if ui
                    .checkbox(
                        &mut cfg.always_preview_in_editor,
                        "Selalu Nyala di Editor (meski tidak dipilih)",
                    )
                    .changed()
                {
                    cfg_changed = true;
                }

                if cfg.kind == ParticleAnimKind::Explosion {
                    ui.add_space(4.0);
                    let replay_btn = egui::Button::new(
                        egui::RichText::new("Putar Ulang Preview Ledakan")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(185, 28, 28))
                    .min_size(egui::vec2(ui.available_width(), 25.0));
                    if ui.add(replay_btn).clicked() {
                        state.particle_preview_reset_nonce =
                            state.particle_preview_reset_nonce.wrapping_add(1);
                    }
                }

                ui.add_space(4.0);
                let order_pos = state
                    .anims
                    .iter()
                    .position(|x| x.id == a.id)
                    .map(|i| i + 1)
                    .unwrap_or(a.sort_order.max(1) as usize);
                ui.label(
                    egui::RichText::new(format!(
                        "Urutan Skenario: #{order_pos}  (atur cepat via Up / Down di tab Animasi)"
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            });
        },
    );

    // Card 2: Full 3D Position (Longitude, Latitude, Altitude) + Move Gizmo sync
    inspector_card(ui, "Posisi 3D (Lng / Lat / Altitude)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            let [mut lng, mut lat, mut alt] = particle_anim_position(a);
            let mut pos_changed = false;

            ui.horizontal(|ui| {
                axis_badge(ui, "Lng", egui::Color32::from_rgb(185, 28, 28));
                pos_changed |= numeric_spinner(
                    ui,
                    ("particle_lng", a.id),
                    &mut lng,
                    SpinnerConfig {
                        min: -180.0,
                        max: 180.0,
                        step: 0.00005,
                        drag_speed: 0.00001,
                        decimals: 6,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Lat", egui::Color32::from_rgb(29, 78, 216));
                pos_changed |= numeric_spinner(
                    ui,
                    ("particle_lat", a.id),
                    &mut lat,
                    SpinnerConfig {
                        min: -85.0,
                        max: 85.0,
                        step: 0.00005,
                        drag_speed: 0.00001,
                        decimals: 6,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Alt", egui::Color32::from_rgb(21, 128, 61));
                pos_changed |= numeric_spinner(
                    ui,
                    ("particle_alt", a.id),
                    &mut alt,
                    SpinnerConfig {
                        min: -50.0,
                        max: 2000.0,
                        step: 0.5,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });

            if pos_changed {
                set_particle_anim_position(a, lng, lat, alt);
            }

            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(
                    "Tips: Tarik panah Merah (X), Hijau (Alt/Y), Biru (Z), atau lingkaran Kuning (Free) di peta untuk memindahkan partikel.",
                )
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    // Card 3: Particle Effect Parameters, Color Presets & Wind
    inspector_card(
        ui,
        &format!("Pengaturan Efek ({})", cfg.kind.short_tag()),
        true,
        |ui| {
            ui.add_enabled_ui(editable, |ui| {
                let radius_lbl = match cfg.kind {
                    ParticleAnimKind::Explosion => "Radius Ledakan:",
                    _ => "Radius Sumber:",
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(radius_lbl)
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("particle_radius", a.id),
                        &mut cfg.radius_m,
                        SpinnerConfig {
                            min: 0.3,
                            max: 150.0,
                            step: 0.5,
                            drag_speed: 0.15,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });

                let height_lbl = match cfg.kind {
                    ParticleAnimKind::Fire => "Tinggi Lidah Api:",
                    ParticleAnimKind::Smoke => "Tinggi Kepulan Asap:",
                    ParticleAnimKind::Explosion => "Tinggi Bola / Jamur:",
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(height_lbl)
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("particle_height", a.id),
                        &mut cfg.height_m,
                        SpinnerConfig {
                            min: 0.5,
                            max: 250.0,
                            step: 0.5,
                            drag_speed: 0.2,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Intensitas / Kekuatan:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .add(
                            egui::Slider::new(&mut cfg.intensity, 0.2..=3.0)
                                .fixed_decimals(2)
                                .suffix("x"),
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });

                if matches!(cfg.kind, ParticleAnimKind::Fire | ParticleAnimKind::Smoke) {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Kecepatan Naik:")
                                .size(11.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        if ui
                            .add(
                                egui::Slider::new(&mut cfg.rise_speed, 0.2..=5.0)
                                    .fixed_decimals(2)
                                    .suffix("x"),
                            )
                            .changed()
                        {
                            cfg_changed = true;
                        }
                    });
                }

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Lebar Mekar (Spread):")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .add(
                            egui::Slider::new(&mut cfg.spread, 0.0..=2.0)
                                .fixed_decimals(2),
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });

                ui.add_space(4.0);
                // Kind-specific toggles
                match cfg.kind {
                    ParticleAnimKind::Fire => {
                        if ui
                            .checkbox(&mut cfg.with_smoke, "Asap Gelap di Atas Api")
                            .changed()
                        {
                            cfg_changed = true;
                        }
                        if ui
                            .checkbox(&mut cfg.with_embers, "Percikan Bara Terbang (Embers)")
                            .changed()
                        {
                            cfg_changed = true;
                        }
                    }
                    ParticleAnimKind::Smoke => {}
                    ParticleAnimKind::Explosion => {
                        if ui
                            .checkbox(
                                &mut cfg.shockwave_enabled,
                                "Cincin Gelombang Kejut (Shockwave)",
                            )
                            .changed()
                        {
                            cfg_changed = true;
                        }
                        if ui
                            .checkbox(&mut cfg.with_embers, "Puing & Percikan Api Terbang")
                            .changed()
                        {
                            cfg_changed = true;
                        }
                        if ui
                            .checkbox(&mut cfg.with_smoke, "Awan Asap Pasca-Ledakan")
                            .changed()
                        {
                            cfg_changed = true;
                        }
                    }
                }

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Preset Warna:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                let presets: &[(&str, &str)] = match cfg.kind {
                    ParticleAnimKind::Fire => &[
                        ("Api Normal", "#ff6a00"),
                        ("Merah Pekat", "#dc2626"),
                        ("Api Kimia Biru", "#38bdf8"),
                    ],
                    ParticleAnimKind::Smoke => &[
                        ("Kebakaran", "#475569"),
                        ("Gas Air Mata", "#e2e8f0"),
                        ("Sinyal Merah", "#ef4444"),
                        ("Sinyal Hijau", "#22c55e"),
                        ("Sinyal Kuning", "#eab308"),
                    ],
                    ParticleAnimKind::Explosion => &[
                        ("Bom / Granat", "#ff7b00"),
                        ("Ledakan BBM", "#ef4444"),
                        ("Flashbang", "#f8fafc"),
                    ],
                };
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for &(lbl, hex) in presets {
                        let active = cfg.color_hex.eq_ignore_ascii_case(hex);
                        let btn = egui::Button::new(
                            egui::RichText::new(lbl).size(9.5).color(if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(203, 213, 225)
                            }),
                        )
                        .fill(if active {
                            egui::Color32::from_rgb(234, 88, 12)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .small();
                        if ui.add(btn).clicked() {
                            cfg.color_hex = hex.to_string();
                            cfg_changed = true;
                        }
                    }
                });
                ui.add_space(2.0);
                let prev_hex = cfg.color_hex.clone();
                color_hex_picker(ui, "Warna Kustom:", &mut cfg.color_hex);
                if cfg.color_hex != prev_hex {
                    cfg_changed = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Pengaruh Angin (Wind Drift):")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Arah Angin (0°=U):")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("particle_wind_dir", a.id),
                        &mut cfg.wind_dir_deg,
                        SpinnerConfig {
                            min: 0.0,
                            max: 360.0,
                            step: 15.0,
                            drag_speed: 1.0,
                            decimals: 0,
                            suffix: "°",
                            wrap_around: true,
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Kecepatan Angin:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("particle_wind_spd", a.id),
                        &mut cfg.wind_speed_mps,
                        SpinnerConfig {
                            min: 0.0,
                            max: 25.0,
                            step: 0.5,
                            drag_speed: 0.15,
                            decimals: 1,
                            suffix: " m/s",
                            wrap_around: false,
                        },
                    );
                });
            });
        },
    );

    // Card 4: Duration & Continuous / Repeat Settings
    inspector_card(ui, "Durasi & Pengulangan (Play Skenario)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .checkbox(
                    &mut cfg.continuous,
                    "Nyala Terus-Menerus (sampai tombol Stop)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Durasi Efek:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner(
                        ui,
                        ("particle_duration", a.id),
                        &mut a.duration_sec,
                        SpinnerConfig {
                            min: 0.5,
                            max: 600.0,
                            step: 0.5,
                            drag_speed: 0.2,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
                ui.checkbox(&mut a.replay, "Repeat / Ulangi Otomatis");
            }
        });
    });

    if cfg_changed {
        write_particle_anim_config(a, &cfg);
    }
}

fn draw_water_cannon_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_water_cannon_config(a);
    let mut cfg_changed = false;

    let linked_name = a
        .marker_id
        .and_then(|mid| markers_list.iter().find(|(id, _)| *id == mid))
        .map(|(_, n)| n.clone());

    // Card 1: Identity, Bound 3D Marker (1-to-1) & Live Preview Status
    inspector_card(ui, "Shoot: Water Cannon — Unit & Identitas", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            // Live Preview Indicator Banner
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(12, 54, 78))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(
                            "LIVE PREVIEW AKTIF — Semprotan tampil real-time di peta saat animasi ini dipilih",
                        )
                        .size(9.5)
                        .strong()
                        .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                });
            ui.add_space(5.0);

            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(6.0);

            ui.label(
                egui::RichText::new("Kendaraan / Model 3D Sumber (1 Animasi = 1 Unit):")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.add_space(2.0);
            let is_picking_this = state.tool_mode == ToolMode::PickShootMarker
                && state.picking_shoot_for_anim_id == Some(a.id);
            let pick_btn = egui::Button::new(
                egui::RichText::new(if is_picking_this {
                    "Klik Model 3D di Peta... (Batal)"
                } else if linked_name.is_some() {
                    "Ganti Unit Sumber di Peta"
                } else {
                    "Pilih Unit Sumber di Peta"
                })
                .size(11.0)
                .strong()
                .color(egui::Color32::WHITE),
            )
            .fill(if is_picking_this {
                egui::Color32::from_rgb(180, 83, 9)
            } else {
                egui::Color32::from_rgb(2, 132, 199)
            })
            .min_size(egui::vec2(ui.available_width(), 26.0));
            if ui.add(pick_btn).clicked() {
                if is_picking_this {
                    state.tool_mode = ToolMode::Select;
                    state.picking_shoot_for_anim_id = None;
                } else {
                    route_edit.clear();
                    state.picking_marker_for_anim_id = None;
                    state.picking_shoot_for_anim_id = Some(a.id);
                    state.tool_mode = ToolMode::PickShootMarker;
                    state.status_banner = Some(format!(
                        "Klik 1 Model 3D di peta untuk memindahkan Water Cannon '{}' (Esc batal).",
                        a.name
                    ));
                }
            }
            ui.add_space(3.0);
            egui::ComboBox::from_id_salt(("wc_inspector_marker", a.id))
                .selected_text(
                    linked_name
                        .clone()
                        .unwrap_or_else(|| "(Belum pilih Model 3D)".to_string()),
                )
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut a.marker_id, None, "(Belum terikat)");
                    for (mid, mname) in markers_list {
                        ui.selectable_value(&mut a.marker_id, Some(*mid), mname);
                    }
                });

            ui.add_space(4.0);
            let order_pos = state
                .anims
                .iter()
                .position(|x| x.id == a.id)
                .map(|i| i + 1)
                .unwrap_or(a.sort_order.max(1) as usize);
            ui.label(
                egui::RichText::new(format!(
                    "Urutan Skenario: #{order_pos}  (atur cepat via Up / Down di tab Animasi)"
                ))
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
    });

    // Card 2: Nozzle Position (Height & Vehicle Roof Offsets)
    inspector_card(ui, "Posisi Nozzle (Sumber Air)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                axis_badge(ui, "Tgi", egui::Color32::from_rgb(21, 128, 61));
                ui.label(
                    egui::RichText::new("Tinggi:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_nozzle_h", a.id),
                    &mut cfg.nozzle_height_m,
                    SpinnerConfig {
                        min: 0.2,
                        max: 200.0,
                        step: 0.2,
                        drag_speed: 0.08,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Dpn", egui::Color32::from_rgb(29, 78, 216));
                ui.label(
                    egui::RichText::new("Depan:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_nozzle_fwd", a.id),
                    &mut cfg.nozzle_forward_m,
                    SpinnerConfig {
                        min: -50.0,
                        max: 50.0,
                        step: 0.2,
                        drag_speed: 0.08,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                axis_badge(ui, "Smp", egui::Color32::from_rgb(185, 28, 28));
                ui.label(
                    egui::RichText::new("Samping:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_nozzle_right", a.id),
                    &mut cfg.nozzle_right_m,
                    SpinnerConfig {
                        min: -50.0,
                        max: 50.0,
                        step: 0.2,
                        drag_speed: 0.08,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
        });
    });

    // Card 3: Spray Trajectory, Pressure, Dispersion & Liquid Color
    inspector_card(ui, "Lintasan, Tekanan & Karakter Air", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Panjang / Jangkauan:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_range", a.id),
                    &mut cfg.range_m,
                    SpinnerConfig {
                        min: 2.0,
                        max: 180.0,
                        step: 1.0,
                        drag_speed: 0.3,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Tinggi Lengkungan:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_arc", a.id),
                    &mut cfg.arc_height_m,
                    SpinnerConfig {
                        min: 0.0,
                        max: 80.0,
                        step: 0.5,
                        drag_speed: 0.15,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kuat / Tekanan:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_pressure", a.id),
                    &mut cfg.pressure_bar,
                    SpinnerConfig {
                        min: 1.0,
                        max: 10.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " bar",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Arah Semprot (Yaw):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("wc_yaw", a.id),
                    &mut cfg.yaw_offset_deg,
                    SpinnerConfig {
                        min: -180.0,
                        max: 180.0,
                        step: 5.0,
                        drag_speed: 0.5,
                        decimals: 0,
                        suffix: "°",
                        wrap_around: true,
                    },
                );
            });

            ui.add_space(4.0);
            let mut spread_pct = cfg.spread * 100.0;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Sebaran (Jet ↔ Spray):")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(
                        egui::Slider::new(&mut spread_pct, 0.0..=100.0)
                            .fixed_decimals(0)
                            .suffix("%"),
                    )
                    .changed()
                {
                    cfg.spread = (spread_pct / 100.0).clamp(0.0, 1.0);
                    cfg_changed = true;
                }
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preset Warna Cairan:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (lbl, hex) in [
                    ("Air Jernih", "#d8f3ff"),
                    ("Busa Damkar", "#f8fafc"),
                    ("Pewarna", "#fef08a"),
                    ("Biru Taktis", "#38bdf8"),
                ] {
                    let active = cfg.color_hex.eq_ignore_ascii_case(hex);
                    let btn = egui::Button::new(
                        egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }),
                    )
                    .fill(if active {
                        egui::Color32::from_rgb(2, 132, 199)
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    })
                    .small();
                    if ui.add(btn).clicked() {
                        cfg.color_hex = hex.to_string();
                        cfg_changed = true;
                    }
                }
            });
            ui.add_space(2.0);
            let prev_hex = cfg.color_hex.clone();
            color_hex_picker(ui, "Warna Kustom:", &mut cfg.color_hex);
            if cfg.color_hex != prev_hex {
                cfg_changed = true;
            }
        });
    });

    // Card 4: Automatic Turret Sweep & Duration / Continuous Mode
    inspector_card(ui, "Sapuan (Sweep) & Durasi", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .checkbox(&mut cfg.sweep_enabled, "Sapuan Otomatis Kiri–Kanan (Sweep)")
                .changed()
            {
                cfg_changed = true;
            }
            if cfg.sweep_enabled {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Sudut Sapuan (±):")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("wc_sweep_ang", a.id),
                        &mut cfg.sweep_angle_deg,
                        SpinnerConfig {
                            min: 5.0,
                            max: 120.0,
                            step: 5.0,
                            drag_speed: 0.5,
                            decimals: 0,
                            suffix: "°",
                            wrap_around: false,
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Kecepatan Sapuan:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("wc_sweep_spd", a.id),
                        &mut cfg.sweep_speed,
                        SpinnerConfig {
                            min: 0.05,
                            max: 2.5,
                            step: 0.05,
                            drag_speed: 0.02,
                            decimals: 2,
                            suffix: " Hz",
                            wrap_around: false,
                        },
                    );
                });
            }

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            if ui
                .checkbox(
                    &mut cfg.continuous,
                    "Semprot Terus-Menerus (sampai tombol Stop)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Durasi Semprot:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner(
                        ui,
                        ("wc_duration", a.id),
                        &mut a.duration_sec,
                        SpinnerConfig {
                            min: 1.0,
                            max: 600.0,
                            step: 1.0,
                            drag_speed: 0.25,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
                ui.checkbox(&mut a.replay, "Repeat / Ulangi Otomatis");
            }
        });
    });

    if cfg_changed {
        write_water_cannon_config(a, &cfg);
    }
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
