use bevy_egui::egui;
use twg_shared::{
    api::{
        CreateMarkerRequest, UpdateAnimRequest, UpdateBuildingRequest, UpdateMarkerRequest,
        UpdatePlotLineRequest, UpdateSkenarioRequest, UpdateTextMarkerRequest,
    },
    geo::haversine_distance_m,
    group_anim::{
        GroupAnimMode, GroupStepItem, SequenceTriggerMode, anim_is_continuous_unbounded,
        is_group_anim, parse_group_anim_config, would_create_group_cycle, write_group_anim_config,
    },
    models::{BaseLayer, MapMode, MediaAssetType},
    particle_anim::{
        ParticleAnimConfig, ParticleAnimKind, is_particle_anim, parse_particle_anim_config,
        particle_anim_position, set_particle_anim_position, write_particle_anim_config,
    },
    pointer_anim::{is_pointer_arrow_anim, parse_pointer_arrow_config, write_pointer_arrow_config},
    route_formation::{
        FormationCornerMode, PatwalLeadMotoLayout, PatwalUnitRole, RouteFormationKind,
        anim_bound_marker_ids, anim_is_playable, is_multi_route_anim, parse_multi_route_config,
        write_multi_route_config,
    },
    shoot_anim::{
        BulletFireMode, ShootAnimKind, is_shoot_anim, parse_apar_spray_config,
        parse_bullet_shoot_config, parse_stone_throw_config, parse_tear_gas_config,
        parse_water_cannon_config, shoot_anim_kind, write_apar_spray_config,
        write_bullet_shoot_config, write_stone_throw_config, write_tear_gas_config,
        write_water_cannon_config,
    },
    spawn_hide_anim::{
        SpawnHideAction, SpawnHideTransition, is_spawn_hide_anim, parse_spawn_hide_config,
        spawn_hide_action, write_spawn_hide_config,
    },
    traffic_anim::{
        OvertakeSide, TrafficLightColor, TrafficSignalPhase, TrafficVehicleEntry,
        auto_place_intersection_traffic_lights, build_reverse_lane,
        default_traffic_light_transform, is_traffic_anim, is_traffic_flow_or_signal,
        is_traffic_signal_anim, parse_traffic_anim_config, sample_lane_local_frame,
        write_traffic_anim_config,
    },
    traffic_diversion::{
        DiversionFlowMode, is_traffic_diversion_anim, parse_traffic_diversion_config,
        write_traffic_diversion_config,
    },
    traffic_queue::{
        QueueClearanceMode, QueueManualAction, TrafficQueueCorridor, is_traffic_gridlock_anim,
        is_traffic_manual_anim, is_traffic_queue_anim, parse_traffic_queue_config,
        write_traffic_queue_config,
    },
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
        anim_primary_focus_lnglat,
        basic_mesh::{
            BasicMeshConfig, BasicMeshMaterialKind, BasicMeshShape, is_basic_mesh_marker,
            parse_basic_mesh_config, write_basic_mesh_config,
        },
        markers::{MAX_OBJECT_3D_SCALE, MIN_OBJECT_3D_SCALE, current_auth, finish_active_draft},
        media_preview::MediaPreviewPlayer,
        playback::{route_estimated_duration_sec, route_total_length_m},
        polygon_edit::{PolygonEditState, ensure_editable_polygon_points, insert_midpoint_after},
        route_edit::{RouteEditState, insert_route_midpoint_after},
        traffic::measured_vehicle_scaled_dims,
    },
    state::{
        AnimCategory, AnimRunStatus, AppState, PlaybackState, SelectedEntity, SkyboxPreset,
        ToolMode, TrafficPreviewLayout, TrafficScalePreviewState,
    },
    ui::{
        formation_pad::draw_formation_pad,
        spinner::{SpinnerConfig, numeric_spinner, numeric_spinner_u32_w, numeric_spinner_w},
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
    // Reset ephemeral hover highlight before right-panel widgets re-evaluate hover this frame
    state.group_hovered_child_anim_id = None;

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
                        SelectedEntity::Building(_) => {
                            ("BUILDING", egui::Color32::from_rgb(5, 150, 105))
                        }
                        SelectedEntity::Line(_) => {
                            ("PLOT LINE", egui::Color32::from_rgb(180, 83, 9))
                        }
                        SelectedEntity::TextMarker(_) => {
                            ("TEXT LABEL", egui::Color32::from_rgb(109, 40, 217))
                        }
                        SelectedEntity::Anim(id) => {
                            let anim_ref = state.anims.iter().find(|a| a.id == id);
                            if anim_ref.is_some_and(twg_shared::audio_anim::is_audio_anim) {
                                ("PLAY SUARA", egui::Color32::from_rgb(109, 40, 217))
                            } else if anim_ref.is_some_and(is_traffic_gridlock_anim) {
                                ("MACET TOTAL", egui::Color32::from_rgb(190, 18, 60))
                            } else if anim_ref.is_some_and(is_traffic_manual_anim) {
                                ("SIMPANG MANUAL", egui::Color32::from_rgb(79, 70, 229))
                            } else if anim_ref.is_some_and(is_traffic_queue_anim) {
                                ("URAI SIMPANG", egui::Color32::from_rgb(13, 148, 136))
                            } else if anim_ref.is_some_and(is_traffic_diversion_anim) {
                                ("ALIH ARUS", egui::Color32::from_rgb(2, 132, 199))
                            } else if anim_ref.is_some_and(is_traffic_anim) {
                                ("TRAFFIC ANIM", egui::Color32::from_rgb(217, 119, 6))
                            } else {
                                ("ROUTE ANIM", egui::Color32::from_rgb(16, 185, 129))
                            }
                        }
                        SelectedEntity::None => {
                            ("SKENARIO ROOT", egui::Color32::from_rgb(51, 65, 85))
                        }
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
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(16, 185, 129),
                    ))
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

pub(super) fn inspector_card(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
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
        ui.label(
            egui::RichText::new(label)
                .size(11.5)
                .color(egui::Color32::LIGHT_GRAY),
        );
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
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgb(245, 158, 11),
        ))
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
        ToolMode::DrawGroundPolygon => {
            inspector_card(ui, "Tool: Gambar Ground Polygon (Datar)", true, |ui| {
                let pt_count = state.draft_points_lnglat.len();
                ui.label(
                    egui::RichText::new(format!(
                        "Titik ground polygon: {pt_count} (min 3 — klik titik #1 atau Enter untuk Selesai)"
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
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
                        egui::RichText::new("Selesai (Buat Ground)")
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
        ToolMode::DrawTrafficLane => {
            let is_adding = state.drawing_traffic_lane_for_anim_id.is_some();
            let title = if is_adding {
                "Tambah Jalur Simulasi Traffic"
            } else {
                "Buat Simulasi Traffic (Jalur 1)"
            };
            inspector_card(ui, title, true, |ui| {
                if !is_adding {
                    ui.label("Nama Simulasi Traffic:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_traffic_name)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                }
                ui.label(
                    egui::RichText::new(format!(
                        "Titik jalur: {} (min 2 titik).\nKlik kiri di peta untuk tambah titik, Backspace undo, Enter / Finish selesai.",
                        state.draft_points_lnglat.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(253, 230, 138)),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.draft_points_lnglat.len() >= 2,
                            egui::Button::new(
                                egui::RichText::new("Finish (Enter)")
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105)),
                        )
                        .clicked()
                    {
                        finish_active_draft(state, api);
                    }
                    if ui.button("Batal (Esc)").clicked() {
                        state.draft_points_lnglat.clear();
                        state.drawing_traffic_lane_for_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::DrawTrafficQueueLane => {
            let is_gridlock_new = state.drawing_traffic_queue_target.is_none()
                && state.drawing_traffic_queue_is_gridlock;
            let is_manual_new = state.drawing_traffic_queue_target.is_none()
                && !is_gridlock_new
                && state.drawing_traffic_queue_is_manual;
            let (title, show_name_input, hint_extra) = match state.drawing_traffic_queue_target {
                None if is_gridlock_new => (
                    "Buat Macet Total & Urai (Jalur Antre A)",
                    true,
                    "Gambar jalur tempat mobil/motor tertahan macet total; default Pre-fill penuh & diam hingga tombol 'Lancar' diklik.",
                ),
                None if is_manual_new => (
                    "Buat Kontrol Simpang Manual (Jalur Antre A)",
                    true,
                    "Gambar Jalur Antre A; saat Play Anda bisa klik tombol Buka per jalur, Tahan Semua, atau Lancar secara live.",
                ),
                None => (
                    "Buat Antrean & Urai Simpang (Jalur Antre A)",
                    true,
                    "Gambar jalur tempat mobil/motor muncul dan berhenti antre di ujung jalur.",
                ),
                Some((_, None)) => (
                    "Tambah Jalur Antre Baru",
                    false,
                    "Gambar Jalur Antre baru; setelah selesai Anda akan langsung diarahkan menggambar Jalur Lancar wajibnya.",
                ),
                Some((_, Some(_))) => (
                    "Gambar Jalur Lancar (Wajib)",
                    false,
                    "Titik awal sudah menempel di ujung Jalur Antre. Klik lanjutan jalur ke arah keluar simpang lalu tekan Enter.",
                ),
            };
            inspector_card(ui, title, true, |ui| {
                if show_name_input {
                    ui.label("Nama Animasi:");
                    let name_buf = if is_gridlock_new {
                        &mut state.new_traffic_gridlock_name
                    } else if is_manual_new {
                        &mut state.new_traffic_manual_name
                    } else {
                        &mut state.new_traffic_queue_name
                    };
                    ui.add(egui::TextEdit::singleline(name_buf).desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                }
                ui.label(
                    egui::RichText::new(hint_extra)
                        .size(10.5)
                        .color(egui::Color32::from_rgb(153, 246, 228)),
                );
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Titik jalur: {} (min 2 titik).\nKlik kiri di peta untuk tambah titik, Backspace undo, Enter / Finish selesai.",
                        state.draft_points_lnglat.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(253, 230, 138)),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.draft_points_lnglat.len() >= 2,
                            egui::Button::new(
                                egui::RichText::new("Finish (Enter)")
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105)),
                        )
                        .clicked()
                    {
                        finish_active_draft(state, api);
                    }
                    if ui.button("Batal (Esc)").clicked() {
                        state.draft_points_lnglat.clear();
                        state.drawing_traffic_queue_target = None;
                        state.drawing_traffic_queue_is_gridlock = false;
                        state.drawing_traffic_queue_is_manual = false;
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::DrawTrafficDiversionLane => {
            let (title, show_name_input, hint_extra) = match state.drawing_diversion_target {
                None => (
                    "Buat Pengalihan Arus (Jalur Pendekat)",
                    true,
                    "Gambar jalur datang sampai ke TITIK SIMPANG tempat arus akan dialihkan.",
                ),
                Some((_, None)) => (
                    "Tambah Koridor Pengalihan (Jalur Pendekat)",
                    false,
                    "Gambar jalur datang sampai ke titik simpang; setelah Enter langsung lanjut gambar Jalur Utama & Alternatif.",
                ),
                Some((_, Some((_, 1)))) => (
                    "Gambar Jalur Utama / Normal",
                    false,
                    "Titik awal sudah menempel di titik simpang. Gambar lanjutan arah NORMAL lalu tekan Enter.",
                ),
                Some((_, Some((_, 2)))) => (
                    "Gambar Jalur Alternatif / Pengalihan",
                    false,
                    "Titik awal sudah menempel di titik simpang. Gambar arah belokan PENGALIHAN ARUS lalu tekan Enter.",
                ),
                _ => (
                    "Gambar Jalur Pengalihan Arus",
                    false,
                    "Klik titik-titik jalur di peta (min 2 titik) lalu tekan Enter.",
                ),
            };
            inspector_card(ui, title, true, |ui| {
                if show_name_input {
                    ui.label("Nama Animasi:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_traffic_diversion_name)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                }
                ui.label(
                    egui::RichText::new(hint_extra)
                        .size(10.5)
                        .color(egui::Color32::from_rgb(125, 211, 252)),
                );
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Titik jalur: {} (min 2 titik).\nKlik kiri di peta untuk tambah titik, Backspace undo, Enter / Finish selesai.",
                        state.draft_points_lnglat.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(253, 230, 138)),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.draft_points_lnglat.len() >= 2,
                            egui::Button::new(
                                egui::RichText::new("Finish (Enter)")
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105)),
                        )
                        .clicked()
                    {
                        finish_active_draft(state, api);
                    }
                    if ui.button("Batal (Esc)").clicked() {
                        state.draft_points_lnglat.clear();
                        state.drawing_diversion_target = None;
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::DrawTacticalArrow => {
            let redrawing_name = state
                .redrawing_tactical_arrow_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .map(|a| a.name.clone());
            let title = if redrawing_name.is_some() {
                "Gambar Ulang Jalur Panah Taktis"
            } else {
                "Gambar Panah Alur Taktis 3D"
            };
            inspector_card(ui, title, true, |ui| {
                if let Some(nm) = redrawing_name {
                    ui.label(
                        egui::RichText::new(format!("Animasi: {nm}"))
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(252, 165, 165)),
                    );
                } else {
                    ui.label("Nama Panah Taktis:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_tactical_arrow_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Titik jalur: {} (min 2 titik). Klik di peta untuk menambah titik.",
                        state.draft_points_lnglat.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_finish = state.draft_points_lnglat.len() >= 2;
                    if ui
                        .add_enabled(
                            can_finish,
                            egui::Button::new(
                                egui::RichText::new("Selesai (Enter)")
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105)),
                        )
                        .clicked()
                    {
                        finish_active_draft(state, api);
                    }
                    if ui
                        .add_enabled(
                            !state.draft_points_lnglat.is_empty(),
                            egui::Button::new("Undo"),
                        )
                        .clicked()
                    {
                        state.draft_points_lnglat.pop();
                    }
                    if ui.small_button("Batal (Esc)").clicked() {
                        state.draft_points_lnglat.clear();
                        state.redrawing_tactical_arrow_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    }
                });
            });
        }
        ToolMode::DrawRoute => {
            let is_patwal_mode =
                state.new_route_is_multi && state.new_route_formation == RouteFormationKind::Patwal;
            let card_title = if is_patwal_mode {
                "Langkah 1/2: Gambar Garis Rute Patwal"
            } else if state.new_route_is_multi {
                "Langkah 1/2: Gambar Garis Rute (Multiple)"
            } else {
                "Langkah 1/2: Gambar Garis Rute (Tunggal)"
            };
            inspector_card(ui, card_title, true, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(
                        egui::RichText::new("Tipe:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .selectable_label(!state.new_route_is_multi, "Tunggal")
                        .clicked()
                    {
                        state.new_route_is_multi = false;
                        state.new_route_formation = RouteFormationKind::Grid;
                    }
                    if ui
                        .selectable_label(state.new_route_is_multi && !is_patwal_mode, "Multiple")
                        .clicked()
                    {
                        state.new_route_is_multi = true;
                        state.new_route_formation = RouteFormationKind::Grid;
                    }
                    if ui.selectable_label(is_patwal_mode, "Patwal").clicked() {
                        state.new_route_is_multi = true;
                        state.new_route_formation = RouteFormationKind::Patwal;
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
            let rebinding_anim = state
                .picking_shoot_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .cloned();
            let active_kind = rebinding_anim
                .as_ref()
                .and_then(shoot_anim_kind)
                .unwrap_or(state.new_shoot_kind);
            let title = if rebinding_anim.is_some() {
                format!("Ganti Unit {} di Peta", active_kind.label())
            } else {
                format!("Buat {}: Pilih 1 Model 3D", active_kind.label())
            };
            inspector_card(ui, &title, true, |ui| {
                if let Some(ra) = rebinding_anim {
                    ui.label(
                        egui::RichText::new(format!("Animasi: {}", ra.name))
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(56, 189, 248)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Jenis Shoot:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for &k in ShootAnimKind::ALL {
                            let active = state.new_shoot_kind == k;
                            let btn = egui::Button::new(
                                egui::RichText::new(k.short_tag()).size(9.5).strong().color(
                                    if active {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(148, 163, 184)
                                    },
                                ),
                            )
                            .fill(if active {
                                egui::Color32::from_rgb(2, 132, 199)
                            } else {
                                egui::Color32::from_rgb(24, 34, 52)
                            })
                            .small();
                            if ui.add(btn).clicked() && state.new_shoot_kind != k {
                                let cnt = state
                                    .anims
                                    .iter()
                                    .filter(|a| shoot_anim_kind(a) == Some(k))
                                    .count()
                                    + 1;
                                state.new_shoot_kind = k;
                                state.new_shoot_name = k.default_name(cnt);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(format!("Nama Animasi {}:", state.new_shoot_kind.label()));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_shoot_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Klik 1 Model 3D / Unit di peta (1 animasi = 1 unit). Tinggi titik tembak otomatis menyesuaikan tinggi model dan live preview langsung aktif.",
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
        ToolMode::PickModelActionMarker => {
            let rebinding_anim = state
                .picking_model_action_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .cloned();
            let title = if rebinding_anim.is_some() {
                "Ganti Target Aksi Model 3D"
            } else {
                "Buat Aksi Model 3D: Pilih 1 Objek 3D"
            };
            inspector_card(ui, title, true, |ui| {
                if let Some(ra) = rebinding_anim {
                    ui.label(
                        egui::RichText::new(format!("Animasi: {}", ra.name))
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(251, 191, 36)),
                    );
                } else {
                    ui.label("Nama Animasi Aksi Model 3D:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_model_action_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Klik 1 Objek / Model 3D di peta untuk memilih target klip animasi aksi.",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(6.0);
                if ui.small_button("Batal (Esc)").clicked() {
                    state.picking_model_action_for_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                }
            });
        }
        ToolMode::PickDialogSpeaker => {
            let picking_target = state.picking_dialog_speaker.and_then(|(aid, line_idx)| {
                state
                    .anims
                    .iter()
                    .find(|a| a.id == aid)
                    .map(|a| (a.name.clone(), line_idx))
            });
            let title = if let Some((_, line_idx)) = &picking_target {
                format!("Pilih Pembicara Dialog (Baris #{})", line_idx + 1)
            } else {
                "Buat Dialog 3D: Pilih Pembicara Pertama".to_string()
            };
            inspector_card(ui, &title, true, |ui| {
                if let Some((anim_name, line_idx)) = picking_target {
                    ui.label(
                        egui::RichText::new(format!(
                            "Animasi: {anim_name}  •  Baris #{}",
                            line_idx + 1
                        ))
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::from_rgb(56, 189, 248)),
                    );
                } else {
                    ui.label("Nama Animasi Dialog:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_dialog_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Klik 1 Objek / Marker 3D di peta untuk menjadikannya pembicara pada speech bubble.",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(6.0);
                if ui.small_button("Batal (Esc)").clicked() {
                    state.picking_dialog_speaker = None;
                    state.tool_mode = ToolMode::Select;
                }
            });
        }
        ToolMode::PickCameraFollowMarker => {
            let rebinding_anim = state
                .picking_camera_follow_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .cloned();
            let title = if rebinding_anim.is_some() {
                "Ganti Target Camera Follow"
            } else {
                "Buat Camera Follow: Pilih 1 Objek 3D"
            };
            inspector_card(ui, title, true, |ui| {
                if let Some(ra) = rebinding_anim {
                    ui.label(
                        egui::RichText::new(format!("Animasi: {}", ra.name))
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(110, 231, 183)),
                    );
                } else {
                    ui.label("Nama Animasi Camera Follow:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_camera_follow_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Klik 1 Objek / Marker 3D di peta yang ingin diikuti kamera.",
                    )
                    .size(10.5)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(6.0);
                if ui.small_button("Batal (Esc)").clicked() {
                    state.picking_camera_follow_for_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                }
            });
        }
        ToolMode::PickPointerMarker => {
            let rebinding_anim = state
                .picking_pointer_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .cloned();
            let title = if rebinding_anim.is_some() {
                "Ganti Target Panah Penunjuk 3D"
            } else {
                "Buat Panah Penunjuk 3D: Pilih 1 Objek 3D"
            };
            inspector_card(ui, title, true, |ui| {
                if let Some(ra) = rebinding_anim {
                    ui.label(
                        egui::RichText::new(format!("Animasi: {}", ra.name))
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(250, 204, 21)),
                    );
                } else {
                    ui.label("Nama Animasi Panah Penunjuk:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_pointer_name)
                            .desired_width(f32::INFINITY),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Klik 1 Marker / Unit / Objek 3D di peta. Panah 3D akan muncul memantul naik-turun tepat di atas objek tersebut (teks, ukuran font & warna dapat diatur di Inspector).",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(6.0);
                if ui.small_button("Batal (Esc)").clicked() {
                    state.picking_pointer_for_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                }
            });
        }
        ToolMode::PickSpawnHideMarker => {
            let picked_anim = state
                .picking_spawn_hide_for_anim_id
                .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                .cloned();
            let act = picked_anim
                .as_ref()
                .and_then(spawn_hide_action)
                .unwrap_or(state.new_spawn_hide_action);
            let title = match act {
                SpawnHideAction::Spawn => "Pilih Objek 3D untuk Spawn (Multiple)",
                SpawnHideAction::Hide => "Pilih Objek 3D untuk Hide (Multiple)",
            };
            inspector_card(ui, title, true, |ui| {
                let anim_title = picked_anim
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| state.new_spawn_hide_name.clone());
                let accent = match act {
                    SpawnHideAction::Spawn => egui::Color32::from_rgb(52, 211, 153),
                    SpawnHideAction::Hide => egui::Color32::from_rgb(251, 113, 133),
                };
                ui.label(
                    egui::RichText::new(format!("[{}] {anim_title}", act.short_tag()))
                        .size(12.0)
                        .strong()
                        .color(accent),
                );
                ui.add_space(3.0);
                let bound_ids = picked_anim
                    .as_ref()
                    .map(anim_bound_marker_ids)
                    .unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!(
                        "Terpilih: {} objek. Klik Marker / Objek 3D di peta untuk menambah atau melepas dari daftar.",
                        bound_ids.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                if !bound_ids.is_empty() {
                    ui.add_space(4.0);
                    for (idx, mid) in bound_ids.iter().take(6).enumerate() {
                        let mname = state
                            .markers
                            .iter()
                            .find(|m| m.id == *mid)
                            .map(|m| m.name.as_str())
                            .unwrap_or("(Objek tidak ditemukan)");
                        ui.label(
                            egui::RichText::new(format!("  #{}  {mname}", idx + 1))
                                .size(10.5)
                                .color(accent),
                        );
                    }
                    if bound_ids.len() > 6 {
                        ui.label(
                            egui::RichText::new(format!(
                                "  ... +{} objek lainnya",
                                bound_ids.len() - 6
                            ))
                            .size(10.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                }
                ui.add_space(6.0);
                let done_btn = egui::Button::new(
                    egui::RichText::new("Selesai Pilih Objek (Enter)")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(5, 150, 105))
                .min_size(egui::vec2(ui.available_width(), 26.0));
                if ui.add(done_btn).clicked() {
                    state.picking_spawn_hide_for_anim_id = None;
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
                                        ParticleAnimKind::from_anim_type(&a.anim_type) == Some(kind)
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
    let editable = !playback.play_test_mode && !is_locked && state.can_edit_marker(&m);

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
            ui.label(
                egui::RichText::new("Name")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.checkbox(&mut m.is_spawn, "Active / Visible (Spawned)");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Description")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
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
            ui.label(
                egui::RichText::new("Position (Geo / Alt)")
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_rgb(203, 213, 225)),
            );
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
            ui.label(
                egui::RichText::new("Rotation (°)")
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_rgb(203, 213, 225)),
            );
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
                ui.label(
                    egui::RichText::new("Scale (min 0.01)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(203, 213, 225)),
                );
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
                    BasicMeshShape::Polygon | BasicMeshShape::GroundPolygon => {
                        if bm.shape == BasicMeshShape::Polygon {
                            f64_dim(ui, "Tinggi (Y):", "bm_h", &mut bm.height, dim_cfg, &mut bm_changed);
                            ui.add_space(4.0);
                        }
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
                                max: 200.0,
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
    // 2c. Basic Mesh RBAC Ownership & Multi-User Approval Card
    if is_basic_mesh_marker(&m) {
        draw_entity_permissions_card(
            ui,
            state,
            ("perm_basic_mesh", m.id),
            "Basic Mesh",
            false,
            editable,
            m.created_by,
            &mut m.allowed_user_ids,
        );
    }

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
        || m.metadata != orig.metadata
        || m.allowed_user_ids != orig.allowed_user_ids;

    if editable && marker_changed {
        if let Some(existing) = state.markers.iter_mut().find(|x| x.id == m.id) {
            *existing = m.clone();
        }
        state.marker_edit = Some(m.clone());
        state.dirty_marker_ids.insert(m.id);
    }

    // 3. Action Buttons
    if editable {
        let can_assign_users = state.can_write_admin();
        ui.horizontal(|ui| {
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
                        allowed_user_ids: can_assign_users.then_some(m.allowed_user_ids.clone()),
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
                    dup.created_by = state.current_user_id();
                    dup.allowed_user_ids.clear();
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
                            allowed_user_ids: Vec::new(),
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

#[allow(clippy::too_many_arguments)]
fn draw_entity_permissions_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    id_salt: (&str, Uuid),
    entity_label: &str,
    is_anim: bool,
    editable: bool,
    created_by: Option<Uuid>,
    allowed_user_ids: &mut Vec<Uuid>,
) {
    let is_admin_writer = state.can_write_admin();
    let card_title = if is_admin_writer {
        format!("Persetujuan User ({})", allowed_user_ids.len())
    } else {
        "Kepemilikan & Izin".to_string()
    };

    inspector_card(ui, &card_title, true, |ui| {
        // 1. Owner / Creator badge
        let my_id = state.current_user_id();
        let owner_text = match created_by {
            Some(cid) => {
                let disp = state.resolve_user_display(cid);
                if Some(cid) == my_id {
                    format!("Pemilik: {disp} (Anda)")
                } else {
                    format!("Pembuat: {disp}")
                }
            }
            None => "Pembuat: Admin / Sistem".to_string(),
        };
        let is_mine = created_by.is_some() && created_by == my_id;
        ui.horizontal_wrapped(|ui| {
            let col = if is_mine {
                egui::Color32::from_rgb(52, 211, 153)
            } else {
                egui::Color32::from_rgb(148, 163, 184)
            };
            ui.label(egui::RichText::new(owner_text).size(11.0).strong().color(col));
        });

        if state.is_standard_user() {
            ui.add_space(3.0);
            if is_mine {
                ui.label(
                    egui::RichText::new(format!(
                        "{entity_label} ini milik akun Anda. Admin, Admin Monitor, dan Admin Satker tetap dapat mengeditnya."
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            } else if my_id.is_some_and(|me| allowed_user_ids.contains(&me)) {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(6, 78, 59))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Disetujui oleh Admin untuk akun Anda")
                                .size(10.5)
                                .strong()
                                .color(egui::Color32::from_rgb(167, 243, 208)),
                        );
                    });
                ui.label(
                    egui::RichText::new(if is_anim {
                        "Anda dapat mengedit serta menekan Play/Pause/Stop di Play Skenario & Play Tactical."
                    } else {
                        "Anda diberi izin mengedit objek ini oleh Admin."
                    })
                    .size(10.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
            return;
        }

        if !is_admin_writer {
            return;
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(if is_anim {
                "User yang dicentang dapat mengedit animasi ini dan memutarnya di panel Animasi (Play Skenario / Play Tactical)."
            } else {
                "User yang dicentang dapat memilih dan mengedit objek ini di Plot Editor."
            })
            .size(10.0)
            .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        ui.add_space(4.0);

        let targets = state.assignable_target_users();

        if targets.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Belum ada akun User / User Satker pada ruang lingkup skenario ini.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(251, 191, 36)),
            );
            return;
        }

        ui.add_enabled_ui(editable, |ui| {
            // Selected user chips
            if !allowed_user_ids.is_empty() {
                let mut remove_uid: Option<Uuid> = None;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for uid in allowed_user_ids.iter() {
                        let disp = state.resolve_user_display(*uid);
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(14, 74, 110))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    ui.label(
                                        egui::RichText::new(disp)
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(224, 242, 254)),
                                    );
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("x")
                                                    .size(9.5)
                                                    .color(egui::Color32::from_rgb(252, 165, 165)),
                                            )
                                            .small()
                                            .frame(false),
                                        )
                                        .on_hover_text("Cabut persetujuan user ini")
                                        .clicked()
                                    {
                                        remove_uid = Some(*uid);
                                    }
                                });
                            });
                    }
                });
                if let Some(rm) = remove_uid {
                    allowed_user_ids.retain(|id| *id != rm);
                }
                ui.add_space(4.0);
            }

            // Search + Select All / Clear
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.perm_user_search)
                        .hint_text("Cari user...")
                        .desired_width((ui.available_width() - 108.0).max(70.0)),
                );
                if ui.small_button("Semua").clicked() {
                    let q = state.perm_user_search.trim().to_lowercase();
                    for u in &targets {
                        let hay = format!("{} {}", u.name, u.username).to_lowercase();
                        if q.is_empty() || hay.contains(&q) {
                            if !allowed_user_ids.contains(&u.id) {
                                allowed_user_ids.push(u.id);
                            }
                        }
                    }
                }
                if ui.small_button("Reset").clicked() {
                    allowed_user_ids.clear();
                }
            });

            ui.add_space(3.0);
            let q = state.perm_user_search.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt(id_salt)
                .max_height(130.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let mut shown = 0usize;
                    for u in &targets {
                        let hay = format!("{} {}", u.name, u.username).to_lowercase();
                        if !q.is_empty() && !hay.contains(&q) {
                            continue;
                        }
                        shown += 1;
                        let mut checked = allowed_user_ids.contains(&u.id);
                        let lbl = if u.name.trim().is_empty() {
                            format!("@{}", u.username)
                        } else {
                            format!("{} (@{})", u.name, u.username)
                        };
                        if ui.checkbox(&mut checked, lbl).changed() {
                            if checked {
                                if !allowed_user_ids.contains(&u.id) {
                                    allowed_user_ids.push(u.id);
                                }
                            } else {
                                allowed_user_ids.retain(|id| *id != u.id);
                            }
                        }
                    }
                    if shown == 0 {
                        ui.label(
                            egui::RichText::new("(Tidak ada user yg cocok pencarian)")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(100, 116, 139)),
                        );
                    }
                });
        });
    });
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
    let editable = !playback.play_test_mode && !is_locked && state.can_edit_building(&b);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&b.name)
                .size(13.5)
                .strong()
                .color(egui::Color32::WHITE),
        );
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

    draw_entity_permissions_card(
        ui,
        state,
        ("perm_building", b.id),
        "Building",
        false,
        editable,
        b.created_by,
        &mut b.allowed_user_ids,
    );

    if editable {
        let can_assign_users = state.can_write_admin();
        let b_changed = b.name != orig.name
            || b.color != orig.color
            || (b.height - orig.height).abs() > 1e-6
            || b.allowed_user_ids != orig.allowed_user_ids;
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
                        allowed_user_ids: can_assign_users.then_some(b.allowed_user_ids),
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
    let editable = !playback.play_test_mode && !is_locked && state.can_edit_line(&l);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&l.name)
                .size(13.5)
                .strong()
                .color(egui::Color32::WHITE),
        );
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
                cols[0].label(
                    egui::RichText::new("Width")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
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
                cols[1].label(
                    egui::RichText::new("Alpha")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
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
            ui.label(
                egui::RichText::new(
                    "Edit di peta: pilih titik untuk Gizmo 2D X/Z atau gerak bebas. Klik (+) untuk menyisipkan titik.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(125, 211, 252)),
            );
        });
    });

    draw_entity_permissions_card(
        ui,
        state,
        ("perm_line", l.id),
        "Plot Line",
        false,
        editable,
        l.created_by,
        &mut l.allowed_user_ids,
    );

    if editable {
        let can_assign_users = state.can_write_admin();
        let l_changed = l.name != orig.name
            || l.color != orig.color
            || (l.width - orig.width).abs() > 1e-6
            || (l.alpha - orig.alpha).abs() > 1e-6
            || l.allowed_user_ids != orig.allowed_user_ids;
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
                        allowed_user_ids: can_assign_users.then_some(l.allowed_user_ids),
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
    let editable = !playback.play_test_mode && !is_locked && state.can_edit_text_marker(&t);
    if editable {
        // Legacy labels may have been saved below ground before altitude was restricted.
        t.altitude = t.altitude.max(0.0);
    }

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&t.text)
                .size(13.5)
                .strong()
                .color(egui::Color32::WHITE),
        );
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
                        min: 1.0,
                        max: 512.0,
                        step: 1.0,
                        drag_speed: 0.5,
                        decimals: 1,
                        suffix: "",
                        wrap_around: false,
                    },
                );
            });
            ui.add_space(4.0);
            ui.checkbox(&mut t.show_background, "Tampilkan latar belakang teks");
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
                        min: 0.0,
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

    draw_entity_permissions_card(
        ui,
        state,
        ("perm_text", t.id),
        "Text Label",
        false,
        editable,
        t.created_by,
        &mut t.allowed_user_ids,
    );

    if editable {
        let can_assign_users = state.can_write_admin();
        let t_changed = t.text != orig.text
            || t.color != orig.color
            || (t.font_size - orig.font_size).abs() > 1e-6
            || t.show_background != orig.show_background
            || (t.lng - orig.lng).abs() > 1e-9
            || (t.lat - orig.lat).abs() > 1e-9
            || (t.altitude - orig.altitude).abs() > 1e-6
            || t.allowed_user_ids != orig.allowed_user_ids;
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
                        show_background: t.show_background,
                        lng: t.lng,
                        lat: t.lat,
                        altitude: t.altitude,
                        allowed_user_ids: can_assign_users.then_some(t.allowed_user_ids),
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

fn draw_editor_preview_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    playback: &mut PlaybackState,
    route_edit: &mut RouteEditState,
    a: &twg_shared::models::Anim,
) {
    if playback.play_test_mode {
        return;
    }

    let is_group = is_group_anim(a);
    let is_queue = is_traffic_queue_anim(a);
    let is_manual_queue = is_traffic_manual_anim(a);
    let is_diversion = is_traffic_diversion_anim(a);
    let can_preview = if is_group {
        !parse_group_anim_config(a).items.is_empty()
    } else {
        anim_is_playable(a, &state.markers)
    };
    let disabled_tip: String = if twg_shared::audio_anim::is_audio_anim(a) {
        "Pilih Sound dari Assets terlebih dahulu".into()
    } else if twg_shared::billboard_anim::is_billboard_anim(a) {
        "Pilih Video dari Assets terlebih dahulu".into()
    } else if twg_shared::camera_anim::is_camera_follow_anim(a) {
        "Pilih 1 Objek / Marker 3D target terlebih dahulu".into()
    } else if twg_shared::environment_anim::is_water_anim(a) {
        "Gambar area poligon air (minimal 3 titik) di peta terlebih dahulu".into()
    } else if twg_shared::environment_anim::is_rain_anim(a) {
        "Tentukan lokasi zona hujan lokal di peta terlebih dahulu".into()
    } else if twg_shared::model_action_anim::is_model_action_anim(a) {
        "Pilih 1 Objek 3D (model Animasi) & klip aksi tambahan di Inspector terlebih dahulu".into()
    } else if twg_shared::dialog_anim::is_dialog_anim(a) {
        "Tambahkan minimal 1 baris dialog dengan Pembicara dan teks terlebih dahulu".into()
    } else if twg_shared::tactical_arrow_anim::is_tactical_arrow_anim(a) {
        "Gambar jalur Panah Taktis minimal 2 titik di peta terlebih dahulu".into()
    } else if twg_shared::hud_banner_anim::is_hud_banner_anim(a) {
        "Isi Judul atau teks Running Text terlebih dahulu".into()
    } else if is_group {
        "Tambahkan minimal 1 animasi anggota ke dalam grup untuk menjalankan preview".into()
    } else if is_traffic_flow_or_signal(a) {
        "Tambahkan minimal 1 jalur aktif (>= 2 titik) dan 1 model kendaraan di Vehicle Pool".into()
    } else if is_queue {
        let qcfg = parse_traffic_queue_config(a);
        let inc = qcfg.incomplete_corridor_names();
        if !inc.is_empty() {
            format!(
                "Setiap jalur wajib punya Jalur Lancar: lengkapi {} terlebih dahulu",
                inc.join(", ")
            )
        } else if qcfg.vehicle_pool.is_empty() {
            "Tambahkan minimal 1 model kendaraan di Pool Model 3D".into()
        } else {
            "Tambahkan minimal 1 pasangan Jalur Antre + Jalur Lancar".into()
        }
    } else if is_diversion {
        let dcfg = parse_traffic_diversion_config(a);
        let inc = dcfg.incomplete_corridor_labels();
        if !inc.is_empty() {
            format!(
                "Lengkapi 3 jalur (Pendekat + Utama + Alternatif): {}",
                inc.join(", ")
            )
        } else if dcfg.vehicle_pool.is_empty() {
            "Tambahkan minimal 1 model kendaraan di Pool Model 3D".into()
        } else {
            "Tambahkan minimal 1 koridor Pengalihan Arus lengkap".into()
        }
    } else {
        "Pilih minimal 1 Unit / Model 3D dan 2 titik rute terlebih dahulu".into()
    };

    let is_previewing = playback.is_previewing_anim(a.id);
    let status = if is_previewing {
        playback.track_status(a.id)
    } else {
        AnimRunStatus::Stopped
    };

    let (badge_txt, badge_bg, badge_fg) = match status {
        AnimRunStatus::Playing => (
            "PREVIEW BERJALAN",
            egui::Color32::from_rgb(6, 78, 59),
            egui::Color32::from_rgb(110, 231, 183),
        ),
        AnimRunStatus::Paused => (
            "DIJEDA",
            egui::Color32::from_rgb(120, 53, 15),
            egui::Color32::from_rgb(253, 230, 138),
        ),
        AnimRunStatus::Finished => (
            "SELESAI",
            egui::Color32::from_rgb(12, 74, 110),
            egui::Color32::from_rgb(125, 211, 252),
        ),
        AnimRunStatus::Stopped => (
            "SIAP",
            egui::Color32::from_rgb(30, 41, 59),
            egui::Color32::from_rgb(148, 163, 184),
        ),
    };

    let border_col = match status {
        AnimRunStatus::Playing => egui::Color32::from_rgb(16, 185, 129),
        AnimRunStatus::Paused => egui::Color32::from_rgb(245, 158, 11),
        AnimRunStatus::Finished => egui::Color32::from_rgb(56, 189, 248),
        AnimRunStatus::Stopped => egui::Color32::from_rgb(44, 62, 92),
    };

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(14, 24, 38))
        .stroke(egui::Stroke::new(1.0, border_col))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Preview di Plot Editor")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(226, 232, 240)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(badge_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(badge_txt)
                                    .size(9.0)
                                    .strong()
                                    .color(badge_fg),
                            );
                        });
                });
            });
            ui.add_space(5.0);

            match status {
                AnimRunStatus::Stopped => {
                    let btn = egui::Button::new(
                        egui::RichText::new("Play Preview")
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .min_size(egui::vec2(ui.available_width(), 26.0));
                    if ui
                        .add_enabled(can_preview, btn)
                        .on_disabled_hover_text(disabled_tip)
                        .clicked()
                    {
                        route_edit.clear();
                        state.tool_mode = ToolMode::Select;
                        playback.start_editor_preview(a.id);
                    }
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(
                            if twg_shared::audio_anim::is_audio_anim(a) {
                                "Preview suara hanya diputar di perangkat ini. Stop mengembalikan pemutaran ke awal."
                            } else {
                                "Lihat animasi langsung di Plot Editor; posisi objek kembali normal saat Stop."
                            },
                        )
                        .size(9.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                }
                AnimRunStatus::Playing => {
                    if is_queue {
                        let is_clearing = playback
                            .tracks
                            .get(&a.id)
                            .is_some_and(|t| t.is_queue_clearing());
                        if is_manual_queue {
                            let qcfg = parse_traffic_queue_config(a);
                            let live = state.traffic_queue_live_status.get(&a.id);
                            let open_src = live.and_then(|s| s.active_corridor_source_idx);
                            let all_stopped = live.is_none_or(|s| s.is_all_stopped);
                            if !is_clearing {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                                    for (c_idx, cor) in qcfg.corridors.iter().enumerate() {
                                        if !cor.enabled || !cor.is_complete() {
                                            continue;
                                        }
                                        let is_open = open_src == Some(c_idx);
                                        let btn = egui::Button::new(
                                            egui::RichText::new(if is_open {
                                                format!("{} [BUKA]", cor.name)
                                            } else {
                                                format!("Buka {}", cor.name)
                                            })
                                            .size(10.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                        )
                                        .fill(if is_open {
                                            egui::Color32::from_rgb(37, 99, 235)
                                        } else {
                                            egui::Color32::from_rgb(51, 65, 85)
                                        });
                                        if ui.add(btn).clicked() {
                                            playback.push_queue_manual_action(
                                                a.id,
                                                QueueManualAction::OpenCorridor {
                                                    corridor_idx: c_idx,
                                                },
                                                None,
                                            );
                                        }
                                    }
                                    let hold_btn = egui::Button::new(
                                        egui::RichText::new("Tahan Semua")
                                            .size(10.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(if all_stopped {
                                        egui::Color32::from_rgb(190, 18, 60)
                                    } else {
                                        egui::Color32::from_rgb(71, 85, 105)
                                    });
                                    if ui.add(hold_btn).clicked() {
                                        playback.push_queue_manual_action(
                                            a.id,
                                            QueueManualAction::HoldAll,
                                            None,
                                        );
                                    }
                                });
                                ui.add_space(4.0);
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                                for (c_idx, cor) in qcfg.corridors.iter().enumerate() {
                                    if !cor.enabled || !cor.is_complete() {
                                        continue;
                                    }
                                    let (q_cnt, tot_cnt) = live
                                        .map(|s| s.corridor_counts(c_idx))
                                        .unwrap_or((0, 0));
                                    let is_draining = is_clearing && open_src == Some(c_idx);
                                    let can_click = !is_clearing || q_cnt > 0;
                                    let label = if is_clearing && tot_cnt == 0 {
                                        format!("{} [Habis]", cor.name)
                                    } else if is_clearing && q_cnt == 0 {
                                        format!("{} [Keluar {}]", cor.name, tot_cnt)
                                    } else if is_draining {
                                        format!("{} [LANCAR · {}]", cor.name, q_cnt)
                                    } else if q_cnt > 0 {
                                        format!("Lancar {} ({})", cor.name, q_cnt)
                                    } else {
                                        format!("Lancar {}", cor.name)
                                    };
                                    let fill = if !can_click {
                                        egui::Color32::from_rgb(30, 41, 59)
                                    } else if is_draining {
                                        egui::Color32::from_rgb(5, 150, 105)
                                    } else {
                                        egui::Color32::from_rgb(6, 95, 70)
                                    };
                                    let btn = egui::Button::new(
                                        egui::RichText::new(label)
                                            .size(10.0)
                                            .strong()
                                            .color(if can_click {
                                                egui::Color32::WHITE
                                            } else {
                                                egui::Color32::from_rgb(148, 163, 184)
                                            }),
                                    )
                                    .fill(fill);
                                    if ui
                                        .add_enabled(can_click, btn)
                                        .on_hover_text(
                                            "Aktifkan Mode Lancar (stop spawn) & urai khusus jalur ini secara manual",
                                        )
                                        .clicked()
                                    {
                                        playback.push_queue_manual_action(
                                            a.id,
                                            QueueManualAction::DrainCorridor {
                                                corridor_idx: c_idx,
                                            },
                                            None,
                                        );
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        } else {
                            let rem = state
                                .traffic_queue_live_status
                                .get(&a.id)
                                .map(|s| s.total_vehicles)
                                .unwrap_or(0);
                            let clear_btn = egui::Button::new(
                                egui::RichText::new(if is_clearing {
                                    format!("Mode Lancar Aktif (Sisa {rem} kendaraan...)")
                                } else {
                                    "LANCAR (Urai Sampai Habis)".to_string()
                                })
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                            )
                            .fill(if is_clearing {
                                egui::Color32::from_rgb(6, 78, 59)
                            } else {
                                egui::Color32::from_rgb(5, 150, 105)
                            })
                            .min_size(egui::vec2(ui.available_width(), 26.0));
                            if ui
                                .add_enabled(!is_clearing, clear_btn)
                                .on_hover_text(
                                    "Stop generate kendaraan baru & percepat seluruh kendaraan yang tersisa bergantian sampai habis (0 kendaraan)",
                                )
                                .clicked()
                            {
                                playback.trigger_queue_clearance(a.id, None);
                            }
                            ui.add_space(4.0);
                        }
                    } else if is_diversion {
                        let dcfg = parse_traffic_diversion_config(a);
                        let is_div = state
                            .traffic_diversion_live_status
                            .get(&a.id)
                            .map(|s| s.is_diverted)
                            .unwrap_or(dcfg.start_diverted);
                        let toggle_btn = egui::Button::new(
                            egui::RichText::new(if is_div {
                                "NORMALKAN ARUS (Kembali ke Utama)"
                            } else {
                                "ALIHKAN ARUS (Belok ke Alternatif)"
                            })
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                        )
                        .fill(if is_div {
                            egui::Color32::from_rgb(2, 132, 199)
                        } else {
                            egui::Color32::from_rgb(217, 119, 6)
                        })
                        .min_size(egui::vec2(ui.available_width(), 26.0));
                        if ui.add(toggle_btn).clicked() {
                            playback.push_diversion_toggle(a.id, !is_div, None);
                        }
                        ui.add_space(4.0);
                    }
                    ui.columns(2, |cols| {
                        let pause_btn = egui::Button::new(
                            egui::RichText::new("Pause")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(180, 83, 9))
                        .min_size(egui::vec2(cols[0].available_width(), 26.0));
                        if cols[0].add(pause_btn).clicked() {
                            playback.pause_anim_tree(a.id, &state.anims);
                        }

                        let stop_btn = egui::Button::new(
                            egui::RichText::new("Stop Preview")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(153, 27, 27))
                        .min_size(egui::vec2(cols[1].available_width(), 26.0));
                        if cols[1].add(stop_btn).clicked() {
                            playback.stop_editor_preview();
                        }
                    });
                }
                AnimRunStatus::Paused => {
                    ui.columns(2, |cols| {
                        let resume_btn = egui::Button::new(
                            egui::RichText::new("Lanjut")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(cols[0].available_width(), 26.0));
                        if cols[0].add(resume_btn).clicked() {
                            playback.resume_anim_tree(a.id, &state.anims);
                        }

                        let stop_btn = egui::Button::new(
                            egui::RichText::new("Stop Preview")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(153, 27, 27))
                        .min_size(egui::vec2(cols[1].available_width(), 26.0));
                        if cols[1].add(stop_btn).clicked() {
                            playback.stop_editor_preview();
                        }
                    });
                }
                AnimRunStatus::Finished => {
                    ui.columns(2, |cols| {
                        let replay_btn = egui::Button::new(
                            egui::RichText::new("Putar Ulang")
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .min_size(egui::vec2(cols[0].available_width(), 26.0));
                        if cols[0].add_enabled(can_preview, replay_btn).clicked() {
                            route_edit.clear();
                            state.tool_mode = ToolMode::Select;
                            playback.start_editor_preview(a.id);
                        }

                        let stop_btn = egui::Button::new(
                            egui::RichText::new(if twg_shared::audio_anim::is_audio_anim(a) { "Reset Suara" } else { "Reset Posisi" })
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(71, 85, 105))
                        .min_size(egui::vec2(cols[1].available_width(), 26.0));
                        if cols[1].add(stop_btn).clicked() {
                            playback.stop_editor_preview();
                        }
                    });
                }
            }

            if is_previewing && let Some(track) = playback.tracks.get(&a.id) {
                ui.add_space(5.0);
                if twg_shared::audio_anim::is_audio_anim(a) {
                    let elapsed = track.distance_m;
                    let total = track.total_length_m;
                    if total > 0.0 {
                        ui.add(egui::ProgressBar::new((elapsed / total).clamp(0.0, 1.0) as f32)
                            .desired_width(ui.available_width())
                            .text(format!("{elapsed:.1} / {total:.1} dtk")));
                    } else {
                        ui.label(format!("{elapsed:.1} dtk"));
                    }
                } else if is_group {
                    let gcfg = parse_group_anim_config(a);
                    let total_steps = gcfg.items.len().max(1);
                    let elapsed = track.distance_m;
                    if gcfg.mode == GroupAnimMode::Sequence {
                        let launched = track.group_launched_count.min(total_steps);
                        let frac = if status == AnimRunStatus::Finished {
                            1.0
                        } else {
                            (launched as f32) / (total_steps as f32)
                        };
                        let bar_w = ui.available_width().max(80.0);
                        ui.add(
                            egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                                .desired_width(bar_w)
                                .desired_height(15.0)
                                .text(format!(
                                    "Step {launched}/{total_steps} • t = {elapsed:.1} dtk"
                                )),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} animasi anggota berjalan bersamaan • t = {elapsed:.1} dtk",
                                track.group_owned_children.len()
                            ))
                            .size(10.0)
                            .color(egui::Color32::from_rgb(186, 230, 253)),
                        );
                    }
                } else if is_queue {
                    let elapsed = track.distance_m;
                    let status_line = if let Some(live) = state.traffic_queue_live_status.get(&a.id)
                    {
                        if live.clearance_active {
                            format!(
                                "MODE LANCAR  •  Sisa {} kendaraan ({} antre, {} keluar)  •  t = {elapsed:.1}s",
                                live.total_vehicles, live.queued_vehicles, live.clearing_vehicles
                            )
                        } else if live.is_all_stopped {
                            format!(
                                "MACET TOTAL  •  {} kendaraan tertahan diam — klik LANCAR untuk urai  •  t = {elapsed:.1}s",
                                live.total_vehicles
                            )
                        } else if live.is_switch_pause {
                            format!(
                                "Jeda pindah -> {} ({:.1}s)  •  {} kendaraan  •  t = {elapsed:.1}s",
                                live.active_corridor_name.as_deref().unwrap_or("-"),
                                live.remaining_turn_sec,
                                live.total_vehicles
                            )
                        } else {
                            format!(
                                "Giliran: {} ({:.0}s)  •  {} antre / {} lancar  •  t = {elapsed:.1}s",
                                live.active_corridor_name.as_deref().unwrap_or("-"),
                                live.remaining_turn_sec,
                                live.queued_vehicles,
                                live.clearing_vehicles
                            )
                        }
                    } else {
                        format!("Antrean & Urai Simpang berjalan  •  t = {elapsed:.1} dtk")
                    };
                    ui.label(
                        egui::RichText::new(status_line)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(153, 246, 228)),
                    );
                } else if is_diversion {
                    let elapsed = track.distance_m;
                    let status_line =
                        if let Some(live) = state.traffic_diversion_live_status.get(&a.id) {
                            if live.is_diverted {
                                format!(
                                    "DIALIHKAN -> Alternatif  •  {} pendekat / {} utama / {} alih  •  t = {elapsed:.1}s",
                                    live.on_approach, live.on_main, live.on_divert
                                )
                            } else {
                                format!(
                                    "NORMAL -> Utama  •  {} pendekat / {} utama / {} alih  •  t = {elapsed:.1}s",
                                    live.on_approach, live.on_main, live.on_divert
                                )
                            }
                        } else {
                            format!("Pengalihan Arus berjalan  •  t = {elapsed:.1} dtk")
                        };
                    ui.label(
                        egui::RichText::new(status_line)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                } else if twg_shared::billboard_anim::is_billboard_anim(a) {
                    let status_line = state
                        .billboard_status
                        .get(&a.id)
                        .cloned()
                        .unwrap_or_else(|| format!("Video berjalan · t = {:.1} dtk", track.distance_m));
                    ui.label(
                        egui::RichText::new(status_line)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                } else if twg_shared::camera_anim::is_camera_anim(a) {
                    let status_line = state
                        .camera_status
                        .get(&a.id)
                        .cloned()
                        .unwrap_or_else(|| format!("Kamera aktif · t = {:.1} dtk", track.distance_m));
                    ui.label(
                        egui::RichText::new(status_line)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                } else if twg_shared::environment_anim::is_environment_anim(a) {
                    let status_line = state
                        .environment_status
                        .get(&a.id)
                        .cloned()
                        .unwrap_or_else(|| format!("Simulasi aktif · t = {:.1} dtk", track.distance_m));
                    ui.label(
                        egui::RichText::new(status_line)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                } else if is_traffic_signal_anim(a) {
                    let elapsed = track.distance_m;
                    let (status_line, status_col) =
                        if let Some(live) = state.traffic_signal_live_status.get(&a.id) {
                            let (c_lbl, c_col) = if live.is_all_red {
                                ("ALL-RED", egui::Color32::from_rgb(251, 113, 133))
                            } else {
                                match live.active_color {
                                    TrafficLightColor::Green => (
                                        "HIJAU",
                                        egui::Color32::from_rgb(110, 231, 183),
                                    ),
                                    TrafficLightColor::Yellow => (
                                        "KUNING",
                                        egui::Color32::from_rgb(253, 224, 71),
                                    ),
                                    TrafficLightColor::Red => (
                                        "MERAH",
                                        egui::Color32::from_rgb(251, 113, 133),
                                    ),
                                }
                            };
                            (
                                format!(
                                    "{} [{c_lbl} {:.1}s]  •  {} kendaraan ({} berhenti)  •  t = {elapsed:.1}s",
                                    if live.active_phase_name.is_empty() {
                                        "APILL"
                                    } else {
                                        &live.active_phase_name
                                    },
                                    live.remaining_sec,
                                    live.total_vehicles,
                                    live.stopped_vehicles,
                                ),
                                c_col,
                            )
                        } else {
                            (
                                format!("Lalu Lintas Lampu Merah berjalan  •  t = {elapsed:.1} dtk"),
                                egui::Color32::from_rgb(253, 164, 175),
                            )
                        };
                    ui.label(
                        egui::RichText::new(status_line)
                            .size(10.0)
                            .color(status_col),
                    );
                } else if is_traffic_anim(a) {
                    ui.label(
                        egui::RichText::new(format!(
                            "Simulasi Traffic berjalan  •  t = {:.1} dtk",
                            track.distance_m
                        ))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(253, 230, 138)),
                    );
                } else {
                    let total_len = route_total_length_m(&a.geom).max(0.01);
                    let dist = track.distance_m.clamp(0.0, total_len);
                    let frac = (dist / total_len) as f32;
                    let speed = a.speed.max(0.1);
                    let elapsed = dist / speed;
                    let total_sec = route_estimated_duration_sec(&a.geom, speed);
                    let bar_w = ui.available_width().max(80.0);
                    ui.add(
                        egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                            .desired_width(bar_w)
                            .desired_height(15.0)
                            .text(format!(
                                "{dist:.1}/{total_len:.1} m • {elapsed:.1}/{total_sec:.1} dtk"
                            )),
                    );
                }
            }
        });
    ui.add_space(6.0);
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
    let editable = !playback.play_test_mode && !is_locked && state.can_edit_anim(&a);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&a.name)
                .size(13.5)
                .strong()
                .color(egui::Color32::WHITE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let focus_target = anim_primary_focus_lnglat(&a, &state.anims, &state.markers);
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

    let bound_ids_for_disp = anim_bound_marker_ids(&a);
    let markers_list: Vec<(Uuid, String)> = state
        .markers
        .iter()
        .filter(|m| state.can_edit_marker(m) || bound_ids_for_disp.contains(&m.id))
        .map(|m| (m.id, m.name.clone()))
        .collect();
    if twg_shared::audio_anim::is_audio_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::audio_anim::draw_inspector(ui, state, &mut a, editable);
    } else if twg_shared::billboard_anim::is_billboard_anim(&a) {
        if twg_shared::billboard_anim::parse_billboard_anim_config(&a).media_mode
            == twg_shared::billboard_anim::BillboardMediaMode::Video
        {
            draw_editor_preview_card(ui, state, playback, route_edit, &a);
        }
        super::billboard_anim::draw_inspector(ui, state, tcam, &mut a, editable);
    } else if twg_shared::camera_anim::is_camera_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::camera_anim::draw_inspector(ui, state, tcam, &mut a, editable);
    } else if twg_shared::environment_anim::is_environment_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::environment_anim::draw_inspector(ui, state, &mut a, editable);
    } else if twg_shared::model_action_anim::is_model_action_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::model_action_anim::draw_inspector(ui, state, &mut a, editable);
    } else if twg_shared::dialog_anim::is_dialog_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::dialog_anim::draw_inspector(ui, state, &mut a, editable);
    } else if twg_shared::tactical_arrow_anim::is_tactical_arrow_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::tactical_arrow_anim::draw_inspector(ui, state, &mut a, editable);
    } else if twg_shared::hud_banner_anim::is_hud_banner_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::hud_banner_anim::draw_inspector(ui, state, &mut a, editable);
    } else if is_particle_anim(&a) {
        draw_particle_inspector_cards(ui, state, &mut a, editable);
    } else if is_traffic_flow_or_signal(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        draw_traffic_inspector_cards(ui, state, tcam, playback, route_edit, &mut a, editable);
    } else if is_traffic_queue_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        draw_traffic_queue_inspector_cards(ui, state, tcam, playback, route_edit, &mut a, editable);
    } else if is_traffic_diversion_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        draw_traffic_diversion_inspector_cards(
            ui, state, tcam, playback, route_edit, &mut a, editable,
        );
    } else if is_group_anim(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        draw_group_anim_inspector_cards(ui, state, tcam, &mut a, editable);
    } else if is_spawn_hide_anim(&a) {
        draw_spawn_hide_inspector_cards(ui, state, route_edit, &mut a, &markers_list, editable);
    } else if is_pointer_arrow_anim(&a) {
        draw_pointer_arrow_inspector_cards(ui, state, route_edit, &mut a, &markers_list, editable);
    } else if let Some(shoot_kind) = shoot_anim_kind(&a) {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
        super::model_action_anim::draw_shoot_action_pose_card(
            ui,
            state,
            &mut a,
            shoot_kind,
            editable,
        );
        match shoot_kind {
            ShootAnimKind::WaterCannon => {
                draw_water_cannon_inspector_cards(
                    ui,
                    state,
                    route_edit,
                    &mut a,
                    &markers_list,
                    editable,
                );
            }
            ShootAnimKind::StoneThrow => {
                draw_stone_throw_inspector_cards(
                    ui,
                    state,
                    route_edit,
                    &mut a,
                    &markers_list,
                    editable,
                );
            }
            ShootAnimKind::TearGas => {
                draw_tear_gas_inspector_cards(
                    ui,
                    state,
                    route_edit,
                    &mut a,
                    &markers_list,
                    editable,
                );
            }
            ShootAnimKind::Bullet => {
                draw_bullet_shoot_inspector_cards(
                    ui,
                    state,
                    route_edit,
                    &mut a,
                    &markers_list,
                    editable,
                );
            }
            ShootAnimKind::Apar => {
                draw_apar_spray_inspector_cards(
                    ui,
                    state,
                    route_edit,
                    &mut a,
                    &markers_list,
                    editable,
                );
            }
        }
    } else {
        draw_editor_preview_card(ui, state, playback, route_edit, &a);
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
            ui.add_space(6.0);

            // Locomotion / Move Animation (Walk default, Auto Run/Lari, or Custom Action Clip)
            {
                use twg_shared::route_formation::{
                    RouteMoveAnimMode, parse_route_move_anim_config,
                    resolve_route_move_action_slug, write_route_move_anim_config,
                };
                let mut mcfg = parse_route_move_anim_config(&a);
                let orig_mcfg = mcfg.clone();

                ui.label(
                    egui::RichText::new("Gaya Gerak Saat Melaju (Walk / Lari):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                egui::ComboBox::from_id_salt(("route_move_anim_mode", a.id))
                    .selected_text(mcfg.mode.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for &mode in RouteMoveAnimMode::ALL {
                            ui.selectable_value(&mut mcfg.mode, mode, mode.label());
                        }
                    });

                // Collect bound markers and their available extra_animations from catalog
                let bound_ids = anim_bound_marker_ids(&a);
                let mut clip_options: Vec<(String, String, usize)> = Vec::new();
                for &mid in &bound_ids {
                    if let Some(m) = state.markers.iter().find(|x| x.id == mid)
                        && let Some(cat) = state.catalog.iter().find(|c| c.model_path == m.model_path)
                    {
                        for act in &cat.extra_animations {
                            if let Some(ex) = clip_options.iter_mut().find(|(s, _, _)| s == &act.slug) {
                                ex.2 += 1;
                            } else {
                                clip_options.push((act.slug.clone(), act.name.clone(), 1));
                            }
                        }
                    }
                }

                if mcfg.mode == RouteMoveAnimMode::ActionClip {
                    ui.add_space(3.0);
                    if clip_options.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Belum ada klip aksi tambahan pada unit rute ini. Upload di Assets -> 3D Models -> + Tambah Aksi.",
                            )
                            .size(10.0)
                            .color(egui::Color32::from_rgb(253, 230, 138)),
                        );
                    } else {
                        if mcfg.action_slug.is_empty() {
                            mcfg.action_slug = clip_options[0].0.clone();
                        }
                        let sel_lbl = clip_options
                            .iter()
                            .find(|(s, _, _)| s == &mcfg.action_slug)
                            .map(|(_, n, cnt)| {
                                if bound_ids.len() > 1 {
                                    format!("{n} ({cnt}/{} unit)", bound_ids.len())
                                } else {
                                    n.clone()
                                }
                            })
                            .unwrap_or_else(|| mcfg.action_slug.clone());
                        egui::ComboBox::from_id_salt(("route_move_anim_clip", a.id))
                            .selected_text(sel_lbl)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (slug, name, cnt) in &clip_options {
                                    let lbl = if bound_ids.len() > 1 {
                                        format!("{name} ({cnt}/{} unit)", bound_ids.len())
                                    } else {
                                        name.clone()
                                    };
                                    ui.selectable_value(&mut mcfg.action_slug, slug.clone(), lbl);
                                }
                            });
                    }
                }

                // Live coverage hint for RunAuto / ActionClip / Per-unit overrides
                if (mcfg.mode != RouteMoveAnimMode::Walk || !mcfg.per_marker_overrides.is_empty())
                    && !bound_ids.is_empty()
                {
                    let mut matched = 0usize;
                    for &mid in &bound_ids {
                        let eff = mcfg.effective_for_marker(mid);
                        if let Some(m) = state.markers.iter().find(|x| x.id == mid)
                            && let Some(cat) =
                                state.catalog.iter().find(|c| c.model_path == m.model_path)
                            && resolve_route_move_action_slug(&eff, &cat.extra_animations)
                                .is_some()
                        {
                            matched += 1;
                        }
                    }
                    let (hint_txt, hint_col) = if matched == 0 {
                        (
                            "Belum ada unit dengan klip Lari/Aksi cocok — fallback ke Walk (upload klip 'Lari' di Assets -> 3D Models).".to_string(),
                            egui::Color32::from_rgb(253, 230, 138),
                        )
                    } else if matched == bound_ids.len() {
                        (
                            format!(
                                "Siap: {matched}/{} unit memakai klip gerak ({}).",
                                bound_ids.len(),
                                mcfg.badge_label().unwrap_or_else(|| "Klip".into())
                            ),
                            egui::Color32::from_rgb(110, 231, 183),
                        )
                    } else {
                        (
                            format!(
                                "{matched}/{} unit memakai klip gerak, {} unit lainnya pakai Walk.",
                                bound_ids.len(),
                                bound_ids.len() - matched
                            ),
                            egui::Color32::from_rgb(125, 211, 252),
                        )
                    };
                    ui.label(egui::RichText::new(hint_txt).size(9.8).color(hint_col));
                }

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Kecepatan Klip:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner_w(
                        ui,
                        ("route_move_anim_speed", a.id),
                        &mut mcfg.anim_speed,
                        82.0,
                        SpinnerConfig {
                            min: 0.25,
                            max: 3.0,
                            step: 0.1,
                            drag_speed: 0.05,
                            decimals: 2,
                            suffix: "×",
                            wrap_around: false,
                        },
                    );
                });

                if mcfg != orig_mcfg {
                    write_route_move_anim_config(&mut a, &mcfg);
                }
            }
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
                        let added = cfg.toggle_marker(new_mid);
                        if added
                            && cfg.formation == RouteFormationKind::Patwal
                            && let Some(m) = state.markers.iter().find(|x| x.id == new_mid)
                        {
                            let role = cfg.infer_patwal_role_for_marker(m);
                            cfg.set_patwal_role(new_mid, role);
                        }
                        state.formation_pad_selected_marker = Some(new_mid);
                        cfg_changed = true;
                    }
                }

                let is_patwal_formation = cfg.formation == RouteFormationKind::Patwal;
                if is_patwal_formation && !cfg.marker_ids.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Peran Konvoi Patwal:")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Auto-Deteksi Peran")
                                            .size(9.5)
                                            .color(egui::Color32::from_rgb(186, 230, 253)),
                                    )
                                    .fill(egui::Color32::from_rgb(30, 58, 138))
                                    .small(),
                                )
                                .on_hover_text(
                                    "Deteksi otomatis peran (Motor Depan / Patwal / VIP / Penutup) dari nama model",
                                )
                                .clicked()
                            {
                                cfg.auto_assign_patwal_roles(&state.markers, true);
                                cfg_changed = true;
                            }
                        });
                    });
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
                    let mut role_update: Option<(Uuid, PatwalUnitRole)> = None;
                    let mut unit_move_override_update: Option<(
                        Uuid,
                        twg_shared::route_formation::RouteUnitMoveOverride,
                    )> = None;
                    let mut route_move_cfg =
                        twg_shared::route_formation::parse_route_move_anim_config(&a);
                    let unit_cnt = cfg.marker_ids.len();
                    let ids_snapshot = cfg.marker_ids.clone();
                    let has_any_anim_units = ids_snapshot.iter().any(|mid| {
                        state
                            .markers
                            .iter()
                            .find(|m| m.id == *mid)
                            .and_then(|m| {
                                state.catalog.iter().find(|c| c.model_path == m.model_path)
                            })
                            .is_some_and(|c| {
                                !c.extra_animations.is_empty()
                                    || c.model_type
                                        == twg_shared::models::ModelAssetType::Animated
                            })
                    });
                    egui::ScrollArea::vertical()
                        .id_salt(("multi_route_units_list", a.id))
                        .max_height(if is_patwal_formation || has_any_anim_units {
                            235.0
                        } else {
                            135.0
                        })
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
                                    let role_tag = if !is_patwal_formation && idx == 0 {
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
                                if is_patwal_formation {
                                    let cur_role = cfg.patwal_role_for(*mid);
                                    let role_col = match cur_role {
                                        PatwalUnitRole::LeadMoto => {
                                            egui::Color32::from_rgb(34, 211, 238)
                                        }
                                        PatwalUnitRole::FrontPolice => {
                                            egui::Color32::from_rgb(96, 165, 250)
                                        }
                                        PatwalUnitRole::Vip => {
                                            egui::Color32::from_rgb(251, 191, 36)
                                        }
                                        PatwalUnitRole::SideMoto => {
                                            egui::Color32::from_rgb(45, 212, 191)
                                        }
                                        PatwalUnitRole::RearPolice => {
                                            egui::Color32::from_rgb(129, 140, 248)
                                        }
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add_space(12.0);
                                        let mut chosen = cur_role;
                                        egui::ComboBox::from_id_salt((
                                            "patwal_unit_role",
                                            a.id,
                                            *mid,
                                        ))
                                        .selected_text(
                                            egui::RichText::new(format!(
                                                "[{}] {}",
                                                cur_role.short_tag(),
                                                cur_role.label()
                                            ))
                                            .size(9.5)
                                            .color(role_col),
                                        )
                                        .width((ui.available_width() - 18.0).max(120.0))
                                        .show_ui(ui, |ui| {
                                            for &r in PatwalUnitRole::ALL {
                                                ui.selectable_value(
                                                    &mut chosen,
                                                    r,
                                                    format!("[{}] {}", r.short_tag(), r.label()),
                                                );
                                            }
                                        });
                                        if chosen != cur_role {
                                            role_update = Some((*mid, chosen));
                                        }
                                    });
                                    ui.add_space(2.0);
                                }
                                // Per-unit locomotion override (shown for every Animated unit or any unit with extra_animations / override)
                                {
                                    use twg_shared::route_formation::{
                                        RouteUnitMoveOverride, resolve_route_move_action_slug,
                                    };
                                    let cat_opt = state
                                        .markers
                                        .iter()
                                        .find(|m| m.id == *mid)
                                        .and_then(|m| {
                                            state
                                                .catalog
                                                .iter()
                                                .find(|c| c.model_path == m.model_path)
                                        });
                                    let is_animated_unit = cat_opt.is_some_and(|c| {
                                        c.model_type
                                            == twg_shared::models::ModelAssetType::Animated
                                    });
                                    let unit_clips: Vec<(String, String)> = cat_opt
                                        .map(|cat| {
                                            cat.extra_animations
                                                .iter()
                                                .map(|act| (act.slug.clone(), act.name.clone()))
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    let cur_ov = route_move_cfg.marker_override(*mid);
                                    if is_animated_unit
                                        || !unit_clips.is_empty()
                                        || cur_ov != RouteUnitMoveOverride::Inherit
                                    {
                                        // Show what "Ikut Rute" actually resolves to for this unit
                                        let inherited_desc = cat_opt
                                            .and_then(|cat| {
                                                let base_only =
                                                    twg_shared::route_formation::RouteMoveAnimConfig {
                                                        mode: route_move_cfg.mode,
                                                        action_slug: route_move_cfg
                                                            .action_slug
                                                            .clone(),
                                                        anim_speed: route_move_cfg.anim_speed,
                                                        per_marker_overrides: Default::default(),
                                                    };
                                                let slug = resolve_route_move_action_slug(
                                                    &base_only,
                                                    &cat.extra_animations,
                                                )?;
                                                cat.extra_animations
                                                    .iter()
                                                    .find(|a| a.slug == slug)
                                                    .map(|a| a.name.clone())
                                            })
                                            .unwrap_or_else(|| "Walk".to_string());
                                        let inherit_lbl = format!("Ikut Rute ({inherited_desc})");

                                        let (sel_txt, sel_col) = match &cur_ov {
                                            RouteUnitMoveOverride::Inherit => (
                                                inherit_lbl.clone(),
                                                egui::Color32::from_rgb(148, 163, 184),
                                            ),
                                            RouteUnitMoveOverride::Walk => (
                                                "Walk (Paksa Jalan)".to_string(),
                                                egui::Color32::from_rgb(125, 211, 252),
                                            ),
                                            RouteUnitMoveOverride::ActionClip { slug } => {
                                                let cname = unit_clips
                                                    .iter()
                                                    .find(|(s, _)| s == slug)
                                                    .map(|(_, n)| n.clone())
                                                    .unwrap_or_else(|| slug.clone());
                                                (
                                                    format!("Klip: {cname}"),
                                                    egui::Color32::from_rgb(251, 191, 36),
                                                )
                                            }
                                        };

                                        ui.horizontal(|ui| {
                                            ui.add_space(12.0);
                                            ui.label(
                                                egui::RichText::new("Gerak:")
                                                    .size(9.5)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                            let mut chosen_ov = cur_ov.clone();
                                            egui::ComboBox::from_id_salt((
                                                "multi_unit_move_ov",
                                                a.id,
                                                *mid,
                                            ))
                                            .selected_text(
                                                egui::RichText::new(sel_txt)
                                                    .size(9.5)
                                                    .color(sel_col),
                                            )
                                            .width((ui.available_width() - 18.0).max(110.0))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut chosen_ov,
                                                    RouteUnitMoveOverride::Inherit,
                                                    &inherit_lbl,
                                                );
                                                ui.selectable_value(
                                                    &mut chosen_ov,
                                                    RouteUnitMoveOverride::Walk,
                                                    "Walk (Paksa Jalan)",
                                                );
                                                if unit_clips.is_empty() {
                                                    ui.separator();
                                                    ui.label(
                                                        egui::RichText::new(
                                                            "Belum ada klip tambahan (upload di Assets -> 3D Models -> + Tambah Aksi)",
                                                        )
                                                        .size(9.5)
                                                        .color(egui::Color32::from_rgb(
                                                            253, 230, 138,
                                                        )),
                                                    );
                                                } else {
                                                    for (slug, name) in &unit_clips {
                                                        ui.selectable_value(
                                                            &mut chosen_ov,
                                                            RouteUnitMoveOverride::ActionClip {
                                                                slug: slug.clone(),
                                                            },
                                                            format!("Klip: {name}"),
                                                        );
                                                    }
                                                }
                                            });
                                            if chosen_ov != cur_ov {
                                                unit_move_override_update =
                                                    Some((*mid, chosen_ov));
                                            }
                                        });
                                        ui.add_space(2.0);
                                    }
                                }
                            }
                        });
                    if let Some((mid, role)) = role_update {
                        cfg.set_patwal_role(mid, role);
                        cfg_changed = true;
                    }
                    if let Some((mid, ov)) = unit_move_override_update {
                        route_move_cfg.set_marker_override(mid, ov);
                        twg_shared::route_formation::write_route_move_anim_config(
                            &mut a,
                            &route_move_cfg,
                        );
                    }
                    if let Some((mid, up)) = move_action
                        && cfg.move_marker_order(mid, up)
                    {
                        cfg_changed = true;
                    }
                    if let Some(mid) = remove_mid {
                        cfg.toggle_marker(mid);
                        if route_move_cfg.per_marker_overrides.remove(&mid).is_some() {
                            twg_shared::route_formation::write_route_move_anim_config(
                                &mut a,
                                &route_move_cfg,
                            );
                        }
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
                    } else if cfg.formation == RouteFormationKind::Patwal {
                        cfg.auto_assign_patwal_roles(&state.markers, false);
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
                    RouteFormationKind::Patwal => {
                        let (mut n_lm, mut n_fp, mut n_vip, mut n_sm, mut n_rp) = (0, 0, 0, 0, 0);
                        for &mid in &cfg.marker_ids {
                            match cfg.patwal_role_for(mid) {
                                PatwalUnitRole::LeadMoto => n_lm += 1,
                                PatwalUnitRole::FrontPolice => n_fp += 1,
                                PatwalUnitRole::Vip => n_vip += 1,
                                PatwalUnitRole::SideMoto => n_sm += 1,
                                PatwalUnitRole::RearPolice => n_rp += 1,
                            }
                        }
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(15, 29, 52))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(37, 99, 235)))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(6, 4))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{n_lm} Mtr Dpn • {n_fp} Ptw Dpn • {n_vip} VIP • {n_sm} Mtr Smp • {n_rp} Ptw Blk"
                                    ))
                                    .size(9.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(191, 219, 254)),
                                );
                            });
                        ui.add_space(4.0);

                        let prev_layout = cfg.patwal_lead_layout;
                        ui.label(
                            egui::RichText::new("Susunan Motor Depan (Voorijder):")
                                .size(10.5)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        egui::ComboBox::from_id_salt(("patwal_lead_layout", a.id))
                            .selected_text(cfg.patwal_lead_layout.label())
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for &ly in PatwalLeadMotoLayout::ALL {
                                    ui.selectable_value(
                                        &mut cfg.patwal_lead_layout,
                                        ly,
                                        ly.label(),
                                    );
                                }
                            });
                        if cfg.patwal_lead_layout != prev_layout {
                            cfg_changed = true;
                        }
                        ui.add_space(3.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Antar Lapis:")
                                    .size(10.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            cfg_changed |= numeric_spinner(
                                ui,
                                ("patwal_gap", a.id),
                                &mut cfg.patwal_layer_gap_m,
                                SpinnerConfig {
                                    min: 2.0,
                                    max: 80.0,
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
                                egui::RichText::new("Samping Motor Dpn:")
                                    .size(10.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            cfg_changed |= numeric_spinner(
                                ui,
                                ("patwal_mx", a.id),
                                &mut cfg.patwal_moto_spacing_x_m,
                                SpinnerConfig {
                                    min: 0.8,
                                    max: 40.0,
                                    step: 0.2,
                                    drag_speed: 0.1,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Baris Motor Dpn:")
                                    .size(10.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            cfg_changed |= numeric_spinner(
                                ui,
                                ("patwal_mz", a.id),
                                &mut cfg.patwal_moto_spacing_z_m,
                                SpinnerConfig {
                                    min: 1.0,
                                    max: 40.0,
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
                                egui::RichText::new("Apit Motor Smp VIP:")
                                    .size(10.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            cfg_changed |= numeric_spinner(
                                ui,
                                ("patwal_sx", a.id),
                                &mut cfg.patwal_side_offset_x_m,
                                SpinnerConfig {
                                    min: 1.0,
                                    max: 40.0,
                                    step: 0.2,
                                    drag_speed: 0.1,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        });

                        ui.add_space(4.0);
                        let convert_btn = egui::Button::new(
                            egui::RichText::new("Ubah ke Bebas (Edit Manual di Pad)")
                                .size(10.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .min_size(egui::vec2(ui.available_width(), 24.0));
                        if ui
                            .add_enabled(!cfg.marker_ids.is_empty(), convert_btn)
                            .on_hover_text(
                                "Salin seluruh posisi Konvoi Patwal saat ini ke Formasi Bebas agar setiap pin dapat digeser bebas secara manual",
                            )
                            .clicked()
                        {
                            cfg.copy_preset_into_custom(RouteFormationKind::Patwal);
                            cfg.formation = RouteFormationKind::Custom;
                            cfg_changed = true;
                            state.status_banner = Some(
                                "Susunan Patwal disalin ke Formasi Bebas — silakan geser pin manual di Formation Pad!"
                                    .into(),
                            );
                        }
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

    draw_entity_permissions_card(
        ui,
        state,
        ("perm_anim", a.id),
        "Animasi",
        true,
        editable,
        a.created_by,
        &mut a.allowed_user_ids,
    );

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
            || a.data != orig.data
            || a.allowed_user_ids != orig.allowed_user_ids;
        if a_changed {
            if let Some(existing) = state.anims.iter_mut().find(|x| x.id == a.id) {
                *existing = a.clone();
            }
            state.anim_edit = Some(a.clone());
            state.dirty_anim_ids.insert(a.id);
        }
    }

    let can_play = state.can_play_anim(&a) && anim_is_playable(&a, &state.markers);
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
        let disabled_hint = if twg_shared::audio_anim::is_audio_anim(&a) {
            "Pilih Sound dari Assets terlebih dahulu"
        } else if twg_shared::billboard_anim::is_billboard_anim(&a) {
            "Mode Gambar tampil statis di peta; pilih Mode Video dan Video dari Assets untuk diputar"
        } else if twg_shared::camera_anim::is_camera_follow_anim(&a) {
            "Pilih 1 Objek / Marker 3D target terlebih dahulu"
        } else if twg_shared::environment_anim::is_water_anim(&a) {
            "Gambar area poligon air (minimal 3 titik) di peta terlebih dahulu"
        } else if twg_shared::environment_anim::is_rain_anim(&a) {
            "Tentukan lokasi zona hujan lokal di peta terlebih dahulu"
        } else if is_particle_anim(&a) {
            "Tentukan posisi partikel di peta terlebih dahulu"
        } else if is_traffic_flow_or_signal(&a) {
            "Tambahkan minimal 1 jalur aktif (>= 2 titik) dan 1 model kendaraan di Vehicle Pool"
        } else if is_traffic_queue_anim(&a) {
            "Setiap jalur aktif wajib punya Jalur Antre + Jalur Lancar (>= 2 titik) dan minimal 1 model kendaraan"
        } else if is_traffic_diversion_anim(&a) {
            "Setiap koridor wajib punya Jalur Pendekat + Jalur Utama + Jalur Alternatif (>= 2 titik) dan minimal 1 model kendaraan"
        } else if is_group_anim(&a) {
            "Tambahkan minimal 1 animasi anggota ke dalam grup terlebih dahulu"
        } else if is_spawn_hide_anim(&a) {
            "Pilih minimal 1 Marker / Objek 3D target terlebih dahulu"
        } else if is_pointer_arrow_anim(&a) {
            "Pilih 1 Marker / Objek 3D target panah terlebih dahulu"
        } else if twg_shared::model_action_anim::is_model_action_anim(&a) {
            "Pilih 1 Objek 3D (model Animasi) & klip aksi tambahan terlebih dahulu"
        } else if twg_shared::dialog_anim::is_dialog_anim(&a) {
            "Tambahkan minimal 1 baris dialog dengan Pembicara dan teks terlebih dahulu"
        } else if twg_shared::tactical_arrow_anim::is_tactical_arrow_anim(&a) {
            "Gambar jalur Panah Taktis minimal 2 titik di peta terlebih dahulu"
        } else if twg_shared::hud_banner_anim::is_hud_banner_anim(&a) {
            "Isi Judul atau teks Running Text terlebih dahulu"
        } else if is_shoot_anim(&a) {
            "Pilih 1 Model 3D sumber animasi Shoot terlebih dahulu"
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
            let can_assign_users = state.can_write_admin();
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
                            allowed_user_ids: can_assign_users
                                .then_some(a.allowed_user_ids.clone()),
                        },
                    });
                    state.dirty_anim_ids.remove(&a.id);
                } else {
                    state.dirty_anim_ids.remove(&a.id);
                    state.status_banner = Some(format!("Animasi '{}' disimpan (Offline).", a.name));
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
                        .add(egui::Slider::new(&mut cfg.spread, 0.0..=2.0).fixed_decimals(2))
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
                        let btn = egui::Button::new(egui::RichText::new(lbl).size(9.5).color(
                            if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(203, 213, 225)
                            },
                        ))
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
                    let btn =
                        egui::Button::new(egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }))
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

/// Shared Identity + Bound 3D Marker card used by Stone Throw, Tear Gas, Bullet & APAR inspectors.
#[allow(clippy::too_many_arguments)]
fn draw_shoot_identity_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
    kind: ShootAnimKind,
    preview_text: &str,
    accent_bg: egui::Color32,
    accent_stroke: egui::Color32,
    show_replay_preview_btn: bool,
) {
    let linked_name = a
        .marker_id
        .and_then(|mid| markers_list.iter().find(|(id, _)| *id == mid))
        .map(|(_, n)| n.clone());

    let card_title = format!("Shoot: {} — Unit & Identitas", kind.label());
    inspector_card(ui, &card_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            egui::Frame::new()
                .fill(accent_bg)
                .stroke(egui::Stroke::new(1.0, accent_stroke))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(preview_text)
                            .size(9.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                });
            if show_replay_preview_btn {
                ui.add_space(4.0);
                let replay_btn = egui::Button::new(
                    egui::RichText::new("Putar Ulang Preview (Tembak -> Ground -> Asap)")
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(51, 65, 85))
                .min_size(egui::vec2(ui.available_width(), 24.0));
                if ui.add(replay_btn).clicked() {
                    state.shoot_preview_reset_nonce =
                        state.shoot_preview_reset_nonce.wrapping_add(1);
                }
            }
            ui.add_space(5.0);

            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(6.0);

            ui.label(
                egui::RichText::new("Unit / Model 3D Sumber (1 Animasi = 1 Unit):")
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
                        "Klik 1 Model 3D di peta untuk memindahkan {} '{}' (Esc batal).",
                        kind.label(),
                        a.name
                    ));
                }
            }
            ui.add_space(3.0);
            egui::ComboBox::from_id_salt(("shoot_inspector_marker", a.id))
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
}

/// Shared 3-axis Launch Origin Offset card (Height / Forward / Right relative to bound marker).
#[allow(clippy::too_many_arguments)]
fn draw_shoot_origin_card(
    ui: &mut egui::Ui,
    card_title: &str,
    id_prefix: &str,
    anim_id: Uuid,
    origin_height_m: &mut f64,
    origin_forward_m: &mut f64,
    origin_right_m: &mut f64,
    editable: bool,
) -> bool {
    let mut changed = false;
    inspector_card(ui, card_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                axis_badge(ui, "Tgi", egui::Color32::from_rgb(21, 128, 61));
                ui.label(
                    egui::RichText::new("Tinggi:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                changed |= numeric_spinner(
                    ui,
                    (id_prefix, "h", anim_id),
                    origin_height_m,
                    SpinnerConfig {
                        min: 0.2,
                        max: 200.0,
                        step: 0.1,
                        drag_speed: 0.06,
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
                changed |= numeric_spinner(
                    ui,
                    (id_prefix, "fwd", anim_id),
                    origin_forward_m,
                    SpinnerConfig {
                        min: -50.0,
                        max: 50.0,
                        step: 0.1,
                        drag_speed: 0.06,
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
                changed |= numeric_spinner(
                    ui,
                    (id_prefix, "right", anim_id),
                    origin_right_m,
                    SpinnerConfig {
                        min: -50.0,
                        max: 50.0,
                        step: 0.1,
                        drag_speed: 0.06,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
        });
    });
    changed
}

fn draw_stone_throw_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_stone_throw_config(a);
    let mut cfg_changed = false;

    draw_shoot_identity_card(
        ui,
        state,
        route_edit,
        a,
        markers_list,
        editable,
        ShootAnimKind::StoneThrow,
        "LIVE PREVIEW AKTIF — Lemparan batu parabolik tampil real-time di peta",
        egui::Color32::from_rgb(66, 32, 6),
        egui::Color32::from_rgb(245, 158, 11),
        false,
    );

    cfg_changed |= draw_shoot_origin_card(
        ui,
        "Posisi Lempar (Tangan / Sumber)",
        "stone_org",
        a.id,
        &mut cfg.origin_height_m,
        &mut cfg.origin_forward_m,
        &mut cfg.origin_right_m,
        editable,
    );

    inspector_card(ui, "Lintasan & Karakter Lemparan Batu", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jarak Lemparan:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("stone_range", a.id),
                    &mut cfg.range_m,
                    SpinnerConfig {
                        min: 2.0,
                        max: 120.0,
                        step: 1.0,
                        drag_speed: 0.25,
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
                    ("stone_arc", a.id),
                    &mut cfg.arc_height_m,
                    SpinnerConfig {
                        min: 0.2,
                        max: 50.0,
                        step: 0.5,
                        drag_speed: 0.12,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kecepatan Terbang:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("stone_spd", a.id),
                    &mut cfg.throw_speed_mps,
                    SpinnerConfig {
                        min: 4.0,
                        max: 45.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Frekuensi Lempar:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("stone_rate", a.id),
                    &mut cfg.throw_rate_hz,
                    SpinnerConfig {
                        min: 0.2,
                        max: 6.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " /dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Batu per Lemparan:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(egui::Slider::new(&mut cfg.stones_per_throw, 1..=6).suffix(" batu"))
                    .changed()
                {
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Ukuran Batu:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("stone_size", a.id),
                    &mut cfg.stone_size_m,
                    SpinnerConfig {
                        min: 0.08,
                        max: 1.5,
                        step: 0.04,
                        drag_speed: 0.01,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Arah Lempar (Yaw):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("stone_yaw", a.id),
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
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Sebaran Sudut:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("stone_spread", a.id),
                    &mut cfg.spread_deg,
                    SpinnerConfig {
                        min: 0.0,
                        max: 60.0,
                        step: 2.0,
                        drag_speed: 0.3,
                        decimals: 0,
                        suffix: "°",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preset Warna Batu:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (lbl, hex) in [
                    ("Batu Abu", "#78716c"),
                    ("Batu Gelap", "#44403c"),
                    ("Bata Merah", "#b45309"),
                    ("Koral Terang", "#a8a29e"),
                ] {
                    let active = cfg.color_hex.eq_ignore_ascii_case(hex);
                    let btn =
                        egui::Button::new(egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }))
                        .fill(if active {
                            egui::Color32::from_rgb(180, 83, 9)
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
            ui.add_space(3.0);
            if ui
                .checkbox(
                    &mut cfg.dust_impact,
                    "Percikan Debu Saat Batu Jatuh ke Tanah",
                )
                .changed()
            {
                cfg_changed = true;
            }
        });
    });

    inspector_card(ui, "Mode & Durasi Lemparan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(
                egui::RichText::new("Mode Runtime Saat Play:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.columns(3, |cols| {
                let modes = [
                    (0u8, "Terus-Menerus", cfg.continuous && !cfg.single_shot),
                    (1u8, "1 Kali", cfg.single_shot),
                    (2u8, "Durasi", !cfg.continuous && !cfg.single_shot),
                ];
                for (idx, lbl, active) in modes {
                    let fill = if active {
                        if idx == 1 {
                            egui::Color32::from_rgb(5, 150, 105)
                        } else {
                            egui::Color32::from_rgb(180, 83, 9)
                        }
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    };
                    let btn = egui::Button::new(
                        egui::RichText::new(lbl).size(10.0).strong().color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(148, 163, 184)
                        }),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(cols[idx as usize].available_width(), 23.0));
                    if cols[idx as usize].add(btn).clicked() && !active {
                        match idx {
                            0 => {
                                cfg.continuous = true;
                                cfg.single_shot = false;
                            }
                            1 => {
                                cfg.continuous = false;
                                cfg.single_shot = true;
                                a.replay = false;
                            }
                            _ => {
                                cfg.continuous = false;
                                cfg.single_shot = false;
                            }
                        }
                        cfg_changed = true;
                        state.shoot_preview_reset_nonce =
                            state.shoot_preview_reset_nonce.wrapping_add(1);
                    }
                }
            });

            ui.add_space(4.0);
            if cfg.single_shot {
                ui.label(
                    egui::RichText::new(format!(
                        "Melempar 1 kali saja saat di-Play lalu otomatis selesai (±{:.2} dtk hingga mendarat & debu mereda).",
                        cfg.single_throw_duration_sec()
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(110, 231, 183)),
                );
                ui.checkbox(&mut a.replay, "Repeat / Ulangi Otomatis");
            } else if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Durasi Lempar:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner(
                        ui,
                        ("stone_duration", a.id),
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
            } else {
                ui.label(
                    egui::RichText::new("Melempar terus-menerus sampai tombol Stop ditekan.")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
        });
    });

    if cfg_changed {
        write_stone_throw_config(a, &cfg);
    }
}

fn draw_tear_gas_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_tear_gas_config(a);
    let mut cfg_changed = false;

    draw_shoot_identity_card(
        ui,
        state,
        route_edit,
        a,
        markers_list,
        editable,
        ShootAnimKind::TearGas,
        "LIVE PREVIEW AKTIF — Peluru melambung ke atas -> menyentuh Alt 0 -> asap putih menyebar & perlahan hilang",
        egui::Color32::from_rgb(30, 41, 59),
        egui::Color32::from_rgb(203, 213, 225),
        true,
    );

    cfg_changed |= draw_shoot_origin_card(
        ui,
        "Posisi Peluncur Gas Air Mata",
        "tg_org",
        a.id,
        &mut cfg.origin_height_m,
        &mut cfg.origin_forward_m,
        &mut cfg.origin_right_m,
        editable,
    );

    inspector_card(ui, "Lintasan Peluru ke Ground (Altitude 0)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jarak Jatuh:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_range", a.id),
                    &mut cfg.range_m,
                    SpinnerConfig {
                        min: 4.0,
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
                    egui::RichText::new("Tinggi Lontaran ke Atas:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_arc", a.id),
                    &mut cfg.arc_height_m,
                    SpinnerConfig {
                        min: 1.0,
                        max: 90.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Waktu Terbang:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_flight", a.id),
                    &mut cfg.flight_time_sec,
                    SpinnerConfig {
                        min: 0.5,
                        max: 10.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Arah Tembak (Yaw):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_yaw", a.id),
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
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Jumlah Peluru (Salvo):")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .add(egui::Slider::new(&mut cfg.salvo_count, 1..=5).suffix(" butir"))
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });
                if cfg.salvo_count > 1 {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Jeda Antar Peluru:")
                                .size(10.5)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        cfg_changed |= numeric_spinner(
                            ui,
                            ("tg_salvo_int", a.id),
                            &mut cfg.salvo_interval_sec,
                            SpinnerConfig {
                                min: 0.1,
                                max: 5.0,
                                step: 0.1,
                                drag_speed: 0.04,
                                decimals: 1,
                                suffix: " dtk",
                                wrap_around: false,
                            },
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Sebaran Sudut Salvo:")
                                .size(10.5)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        cfg_changed |= numeric_spinner(
                            ui,
                            ("tg_salvo_spr", a.id),
                            &mut cfg.salvo_spread_deg,
                            SpinnerConfig {
                                min: 0.0,
                                max: 60.0,
                                step: 2.0,
                                drag_speed: 0.3,
                                decimals: 0,
                                suffix: "°",
                                wrap_around: false,
                            },
                        );
                    });
                }
            }
        });
    });

    inspector_card(ui, "Asap Putih Saat Sampai Ground (Alt 0)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Radius Asap:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_smoke_r", a.id),
                    &mut cfg.smoke_radius_m,
                    SpinnerConfig {
                        min: 1.5,
                        max: 40.0,
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
                    egui::RichText::new("Tinggi Kepulan Asap:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_smoke_h", a.id),
                    &mut cfg.smoke_height_m,
                    SpinnerConfig {
                        min: 1.5,
                        max: 30.0,
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
                    egui::RichText::new("Durasi Asap Hilang:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_smoke_dur", a.id),
                    &mut cfg.smoke_duration_sec,
                    SpinnerConfig {
                        min: 2.0,
                        max: 60.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kepekatan Asap:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(
                        egui::Slider::new(&mut cfg.smoke_density, 0.3..=2.5)
                            .fixed_decimals(2)
                            .suffix("x"),
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Arah Angin:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("tg_wind_dir", a.id),
                    &mut cfg.wind_dir_deg,
                    SpinnerConfig {
                        min: 0.0,
                        max: 360.0,
                        step: 15.0,
                        drag_speed: 0.6,
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
                    ("tg_wind_spd", a.id),
                    &mut cfg.wind_speed_mps,
                    SpinnerConfig {
                        min: 0.0,
                        max: 15.0,
                        step: 0.5,
                        drag_speed: 0.08,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preset Warna Asap:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (lbl, hex) in [
                    ("Putih CS", "#f8fafc"),
                    ("Putih Abu", "#e2e8f0"),
                    ("Kuning Sinyal", "#fef08a"),
                    ("Oranye Sinyal", "#fdba74"),
                ] {
                    let active = cfg.smoke_color_hex.eq_ignore_ascii_case(hex);
                    let btn =
                        egui::Button::new(egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }))
                        .fill(if active {
                            egui::Color32::from_rgb(71, 85, 105)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .small();
                    if ui.add(btn).clicked() {
                        cfg.smoke_color_hex = hex.to_string();
                        cfg_changed = true;
                    }
                }
            });
            ui.add_space(2.0);
            let prev_hex = cfg.smoke_color_hex.clone();
            color_hex_picker(ui, "Warna Kustom:", &mut cfg.smoke_color_hex);
            if cfg.smoke_color_hex != prev_hex {
                cfg_changed = true;
            }
        });
    });

    inspector_card(ui, "Mode Tembakan & Durasi", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .checkbox(
                    &mut cfg.continuous,
                    "Tembak Berulang Terus-Menerus (sampai Stop)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Jeda Tembak Ulang:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("tg_refire", a.id),
                        &mut cfg.refire_interval_sec,
                        SpinnerConfig {
                            min: 1.5,
                            max: 30.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Sebaran Sudut Acak:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("tg_cont_spread", a.id),
                        &mut cfg.salvo_spread_deg,
                        SpinnerConfig {
                            min: 0.0,
                            max: 60.0,
                            step: 2.0,
                            drag_speed: 0.3,
                            decimals: 0,
                            suffix: "°",
                            wrap_around: false,
                        },
                    );
                });
            } else {
                let total_cycle = cfg.total_cycle_duration_sec();
                ui.label(
                    egui::RichText::new(format!(
                        "Durasi Total Siklus: {:.1} dtk (terbang {:.1} dtk + asap hilang {:.1} dtk)",
                        total_cycle, cfg.flight_time_sec, cfg.smoke_duration_sec
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(186, 230, 253)),
                );
                ui.add_space(2.0);
                ui.checkbox(&mut a.replay, "Repeat / Ulangi Otomatis");
            }
        });
    });

    if cfg_changed {
        write_tear_gas_config(a, &cfg);
    }
}

fn draw_bullet_shoot_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_bullet_shoot_config(a);
    let mut cfg_changed = false;

    draw_shoot_identity_card(
        ui,
        state,
        route_edit,
        a,
        markers_list,
        editable,
        ShootAnimKind::Bullet,
        "LIVE PREVIEW AKTIF — Peluru melesat ke depan & hilang tepat saat mencapai jarak maksimal",
        egui::Color32::from_rgb(80, 20, 24),
        egui::Color32::from_rgb(248, 113, 113),
        false,
    );

    cfg_changed |= draw_shoot_origin_card(
        ui,
        "Posisi Moncong Senjata",
        "bullet_org",
        a.id,
        &mut cfg.origin_height_m,
        &mut cfg.origin_forward_m,
        &mut cfg.origin_right_m,
        editable,
    );

    inspector_card(ui, "Karakter Tembakan & Jarak Hilang Peluru", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(
                egui::RichText::new("Mode Tembakan:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for &mode in BulletFireMode::ALL {
                    let active = cfg.fire_mode == mode;
                    let btn = egui::Button::new(egui::RichText::new(mode.label()).size(9.5).color(
                        if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        },
                    ))
                    .fill(if active {
                        egui::Color32::from_rgb(185, 28, 28)
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    })
                    .small();
                    if ui.add(btn).clicked() && cfg.fire_mode != mode {
                        cfg.fire_mode = mode;
                        cfg_changed = true;
                    }
                }
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jarak Hilang Peluru:")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(253, 224, 71)),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_range", a.id),
                    &mut cfg.max_range_m,
                    SpinnerConfig {
                        min: 5.0,
                        max: 300.0,
                        step: 2.0,
                        drag_speed: 0.4,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kecepatan Peluru:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_spd", a.id),
                    &mut cfg.bullet_speed_mps,
                    SpinnerConfig {
                        min: 15.0,
                        max: 250.0,
                        step: 5.0,
                        drag_speed: 0.6,
                        decimals: 0,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Laju Tembak:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_rate", a.id),
                    &mut cfg.fire_rate_rps,
                    SpinnerConfig {
                        min: 0.5,
                        max: 18.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " /dtk",
                        wrap_around: false,
                    },
                );
            });
            if cfg.fire_mode == BulletFireMode::Burst {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Peluru per Burst:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .add(egui::Slider::new(&mut cfg.burst_count, 2..=6).suffix(" butir"))
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Jeda Antar Burst:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("bullet_bpause", a.id),
                        &mut cfg.burst_pause_sec,
                        SpinnerConfig {
                            min: 0.2,
                            max: 3.0,
                            step: 0.1,
                            drag_speed: 0.03,
                            decimals: 2,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Arah Tembak (Yaw):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_yaw", a.id),
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
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Elevasi (Pitch):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_pitch", a.id),
                    &mut cfg.pitch_deg,
                    SpinnerConfig {
                        min: -45.0,
                        max: 60.0,
                        step: 2.0,
                        drag_speed: 0.25,
                        decimals: 0,
                        suffix: "°",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Sebaran / Recoil:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_spread", a.id),
                    &mut cfg.spread_deg,
                    SpinnerConfig {
                        min: 0.0,
                        max: 20.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: "°",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Panjang Jejak Tracer:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_len", a.id),
                    &mut cfg.tracer_length_m,
                    SpinnerConfig {
                        min: 0.4,
                        max: 8.0,
                        step: 0.2,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Tebal Visual Peluru:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("bullet_thick", a.id),
                    &mut cfg.tracer_thickness_m,
                    SpinnerConfig {
                        min: 0.04,
                        max: 0.4,
                        step: 0.02,
                        drag_speed: 0.005,
                        decimals: 2,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preset Warna Peluru / Tracer:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (lbl, hex) in [
                    ("Kuning Emas", "#fde047"),
                    ("Merah Tracer", "#f87171"),
                    ("Hijau Taktis", "#4ade80"),
                    ("Putih Terang", "#ffffff"),
                ] {
                    let active = cfg.color_hex.eq_ignore_ascii_case(hex);
                    let btn =
                        egui::Button::new(egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }))
                        .fill(if active {
                            egui::Color32::from_rgb(185, 28, 28)
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
            ui.add_space(3.0);
            if ui
                .checkbox(&mut cfg.muzzle_flash, "Kilatan Moncong (Muzzle Flash)")
                .changed()
            {
                cfg_changed = true;
            }
        });
    });

    inspector_card(ui, "Mode & Durasi Tembakan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(
                egui::RichText::new("Mode Runtime Saat Play:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.columns(3, |cols| {
                let modes = [
                    (0u8, "Terus-Menerus", cfg.continuous && !cfg.single_shot),
                    (1u8, "1 Kali", cfg.single_shot),
                    (2u8, "Durasi", !cfg.continuous && !cfg.single_shot),
                ];
                for (idx, lbl, active) in modes {
                    let fill = if active {
                        if idx == 1 {
                            egui::Color32::from_rgb(5, 150, 105)
                        } else {
                            egui::Color32::from_rgb(185, 28, 28)
                        }
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    };
                    let btn = egui::Button::new(
                        egui::RichText::new(lbl).size(10.0).strong().color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(148, 163, 184)
                        }),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(cols[idx as usize].available_width(), 23.0));
                    if cols[idx as usize].add(btn).clicked() && !active {
                        match idx {
                            0 => {
                                cfg.continuous = true;
                                cfg.single_shot = false;
                            }
                            1 => {
                                cfg.continuous = false;
                                cfg.single_shot = true;
                                a.replay = false;
                            }
                            _ => {
                                cfg.continuous = false;
                                cfg.single_shot = false;
                            }
                        }
                        cfg_changed = true;
                        state.shoot_preview_reset_nonce =
                            state.shoot_preview_reset_nonce.wrapping_add(1);
                    }
                }
            });

            ui.add_space(4.0);
            if cfg.single_shot {
                ui.label(
                    egui::RichText::new(format!(
                        "Hanya menembakkan 1 peluru saat di-Play lalu otomatis selesai (±{:.2} dtk hingga batas jarak).",
                        cfg.single_shot_duration_sec()
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(110, 231, 183)),
                );
                ui.checkbox(&mut a.replay, "Repeat / Ulangi Otomatis");
            } else if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Durasi Tembak:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner(
                        ui,
                        ("bullet_duration", a.id),
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
            } else {
                ui.label(
                    egui::RichText::new("Menembak terus-menerus sampai tombol Stop ditekan.")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            }
        });
    });

    if cfg_changed {
        write_bullet_shoot_config(a, &cfg);
    }
}

fn draw_apar_spray_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_apar_spray_config(a);
    let mut cfg_changed = false;

    draw_shoot_identity_card(
        ui,
        state,
        route_edit,
        a,
        markers_list,
        editable,
        ShootAnimKind::Apar,
        "LIVE PREVIEW AKTIF — Semprotan kabut APAR tampil real-time di peta",
        egui::Color32::from_rgb(13, 66, 62),
        egui::Color32::from_rgb(45, 212, 191),
        false,
    );

    cfg_changed |= draw_shoot_origin_card(
        ui,
        "Posisi Nozzle APAR",
        "apar_org",
        a.id,
        &mut cfg.origin_height_m,
        &mut cfg.origin_forward_m,
        &mut cfg.origin_right_m,
        editable,
    );

    inspector_card(ui, "Karakter Semprotan & Media APAR", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(
                egui::RichText::new("Preset Media Pemadam APAR:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (lbl, hex, spread, density, pressure) in [
                    ("Serbuk ABC", "#f8fafc", 0.58_f64, 1.25_f64, 1.2_f64),
                    ("Kabut CO2", "#e0f2fe", 0.44, 1.05, 1.55),
                    ("Busa AFFF", "#fefce8", 0.36, 1.45, 1.05),
                    ("Serbuk Kuning", "#fef08a", 0.62, 1.20, 1.2),
                ] {
                    let active = cfg.color_hex.eq_ignore_ascii_case(hex);
                    let btn =
                        egui::Button::new(egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }))
                        .fill(if active {
                            egui::Color32::from_rgb(15, 118, 110)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .small();
                    if ui.add(btn).clicked() {
                        cfg.color_hex = hex.to_string();
                        cfg.cone_spread = spread;
                        cfg.density = density;
                        cfg.pressure = pressure;
                        cfg_changed = true;
                    }
                }
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jarak Semprot:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("apar_range", a.id),
                    &mut cfg.range_m,
                    SpinnerConfig {
                        min: 1.5,
                        max: 30.0,
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
                    egui::RichText::new("Sudut Elevasi (Pitch):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("apar_pitch", a.id),
                    &mut cfg.pitch_deg,
                    SpinnerConfig {
                        min: -60.0,
                        max: 45.0,
                        step: 2.0,
                        drag_speed: 0.3,
                        decimals: 0,
                        suffix: "°",
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
                    ("apar_yaw", a.id),
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
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Tekanan Semprot:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(
                        egui::Slider::new(&mut cfg.pressure, 0.4..=2.5)
                            .fixed_decimals(2)
                            .suffix("x"),
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Lebar Kerucut Kabut:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(
                        egui::Slider::new(&mut cfg.cone_spread, 0.15..=1.5)
                            .fixed_decimals(2)
                            .suffix("x"),
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kepekatan Kabut:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(
                        egui::Slider::new(&mut cfg.density, 0.3..=2.2)
                            .fixed_decimals(2)
                            .suffix("x"),
                    )
                    .changed()
                {
                    cfg_changed = true;
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

    inspector_card(ui, "Sapuan PASS (Sweep) & Durasi", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .checkbox(
                    &mut cfg.sweep_enabled,
                    "Sapuan Kiri–Kanan Otomatis (Teknik PASS)",
                )
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
                        ("apar_sweep_ang", a.id),
                        &mut cfg.sweep_angle_deg,
                        SpinnerConfig {
                            min: 5.0,
                            max: 90.0,
                            step: 3.0,
                            drag_speed: 0.4,
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
                        ("apar_sweep_spd", a.id),
                        &mut cfg.sweep_speed,
                        SpinnerConfig {
                            min: 0.1,
                            max: 2.0,
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
                        ("apar_duration", a.id),
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
        write_apar_spray_config(a, &cfg);
    }
}

fn draw_pointer_arrow_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_pointer_arrow_config(a);
    let mut cfg_changed = false;

    let linked_name = a
        .marker_id
        .and_then(|mid| markers_list.iter().find(|(id, _)| *id == mid))
        .map(|(_, n)| n.clone());

    // Card 1: Identity, Target 3D Marker/Object & Live Preview status
    inspector_card(ui, "Panah Penunjuk 3D — Target & Identitas", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(66, 38, 8))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(250, 204, 21)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(
                            "LIVE PREVIEW AKTIF — Panah 3D memantul di atas target saat dipilih; di Play Skenario muncul saat Play dan hilang saat Stop",
                        )
                        .size(9.5)
                        .strong()
                        .color(egui::Color32::from_rgb(254, 240, 138)),
                    );
                });
            ui.add_space(5.0);

            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(6.0);

            ui.label(
                egui::RichText::new("Target Marker / Objek 3D (1 Animasi = 1 Objek):")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.add_space(2.0);
            let is_picking_this = state.tool_mode == ToolMode::PickPointerMarker
                && state.picking_pointer_for_anim_id == Some(a.id);
            let pick_btn = egui::Button::new(
                egui::RichText::new(if is_picking_this {
                    "Klik Objek 3D di Peta... (Batal)"
                } else if linked_name.is_some() {
                    "Ganti Target Objek 3D di Peta"
                } else {
                    "Pilih Target Objek 3D di Peta"
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
                    state.picking_pointer_for_anim_id = None;
                } else {
                    route_edit.clear();
                    state.picking_marker_for_anim_id = None;
                    state.picking_shoot_for_anim_id = None;
                    state.picking_pointer_for_anim_id = Some(a.id);
                    state.tool_mode = ToolMode::PickPointerMarker;
                    state.status_banner = Some(format!(
                        "Klik 1 Marker / Objek 3D di peta untuk target Panah '{}' (Esc batal).",
                        a.name
                    ));
                }
            }
            ui.add_space(3.0);
            egui::ComboBox::from_id_salt(("pointer_inspector_marker", a.id))
                .selected_text(
                    linked_name
                        .clone()
                        .unwrap_or_else(|| "(Belum pilih Marker / Objek 3D)".to_string()),
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

    // Card 2: 3D Arrow Size, Height, Bounce & Color
    inspector_card(ui, "Tampilan & Gerakan Panah 3D", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Skala Panah 3D:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("ptr_scale", a.id),
                    &mut cfg.arrow_scale,
                    SpinnerConfig {
                        min: 0.3,
                        max: 10.0,
                        step: 0.1,
                        drag_speed: 0.04,
                        decimals: 1,
                        suffix: "x",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Tinggi di Atas Objek:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("ptr_height", a.id),
                    &mut cfg.height_above_m,
                    SpinnerConfig {
                        min: 0.2,
                        max: 150.0,
                        step: 0.5,
                        drag_speed: 0.12,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jarak Naik–Turun:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("ptr_amp", a.id),
                    &mut cfg.bounce_amplitude_m,
                    SpinnerConfig {
                        min: 0.0,
                        max: 30.0,
                        step: 0.2,
                        drag_speed: 0.06,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kecepatan Naik–Turun:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("ptr_spd", a.id),
                    &mut cfg.bounce_speed_hz,
                    SpinnerConfig {
                        min: 0.1,
                        max: 5.0,
                        step: 0.1,
                        drag_speed: 0.03,
                        decimals: 1,
                        suffix: " Hz",
                        wrap_around: false,
                    },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Kecepatan Putar 3D:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("ptr_spin", a.id),
                    &mut cfg.spin_speed_deg,
                    SpinnerConfig {
                        min: 0.0,
                        max: 360.0,
                        step: 15.0,
                        drag_speed: 1.0,
                        decimals: 0,
                        suffix: "°/dtk",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preset Warna Panah:")
                    .size(10.5)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (lbl, hex) in [
                    ("Kuning", "#facc15"),
                    ("Merah", "#ef4444"),
                    ("Cyan", "#38bdf8"),
                    ("Hijau", "#22c55e"),
                    ("Putih", "#f8fafc"),
                ] {
                    let active = cfg.arrow_color_hex.eq_ignore_ascii_case(hex);
                    let btn =
                        egui::Button::new(egui::RichText::new(lbl).size(9.5).color(if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(203, 213, 225)
                        }))
                        .fill(if active {
                            egui::Color32::from_rgb(14, 116, 144)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .small();
                    if ui.add(btn).clicked() {
                        cfg.arrow_color_hex = hex.to_string();
                        cfg_changed = true;
                    }
                }
            });
            ui.add_space(2.0);
            let prev_arrow_hex = cfg.arrow_color_hex.clone();
            color_hex_picker(ui, "Warna Panah:", &mut cfg.arrow_color_hex);
            if cfg.arrow_color_hex != prev_arrow_hex {
                cfg_changed = true;
            }
            ui.add_space(3.0);
            if ui
                .checkbox(
                    &mut cfg.show_ground_ring,
                    "Tampilkan Cincin Sorotan di Dasar Objek",
                )
                .changed()
            {
                cfg_changed = true;
            }
        });
    });

    // Card 3: Optional Floating Text Label (Scalable Font + Color Picker + Empty-able)
    inspector_card(ui, "Tulisan / Label di Atas Panah (Opsional)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .checkbox(&mut cfg.show_label, "Tampilkan Tulisan di Atas Panah")
                .changed()
            {
                cfg_changed = true;
            }
            if cfg.show_label {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Teks Tulisan (kosongkan jika tanpa tulisan):")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut cfg.label_text)
                            .hint_text("Contoh: POSKO VIP / TARGET A (kosong = tanpa teks)")
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    if let Some(ref uname) = linked_name
                        && ui
                            .small_button("Isi Nama Objek")
                            .on_hover_text("Isi otomatis dengan nama Marker / Objek 3D target")
                            .clicked()
                    {
                        cfg.label_text = uname.clone();
                        cfg_changed = true;
                    }
                    if ui
                        .add_enabled(
                            !cfg.label_text.is_empty(),
                            egui::Button::new("Kosongkan (Tanpa Teks)").small(),
                        )
                        .clicked()
                    {
                        cfg.label_text.clear();
                        cfg_changed = true;
                    }
                });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Ukuran / Skala Font:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    if ui
                        .add(
                            egui::Slider::new(&mut cfg.font_size, 8.0..=56.0)
                                .fixed_decimals(0)
                                .suffix(" px"),
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Jarak Teks di Atas Panah:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("ptr_txt_off", a.id),
                        &mut cfg.text_offset_m,
                        SpinnerConfig {
                            min: 0.0,
                            max: 40.0,
                            step: 0.2,
                            drag_speed: 0.06,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Preset Warna Tulisan:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (lbl, hex) in [
                        ("Putih", "#ffffff"),
                        ("Kuning", "#facc15"),
                        ("Merah", "#f87171"),
                        ("Cyan", "#38bdf8"),
                        ("Hijau", "#4ade80"),
                    ] {
                        let active = cfg.text_color_hex.eq_ignore_ascii_case(hex);
                        let btn = egui::Button::new(egui::RichText::new(lbl).size(9.5).color(
                            if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(203, 213, 225)
                            },
                        ))
                        .fill(if active {
                            egui::Color32::from_rgb(14, 116, 144)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .small();
                        if ui.add(btn).clicked() {
                            cfg.text_color_hex = hex.to_string();
                            cfg_changed = true;
                        }
                    }
                });
                ui.add_space(2.0);
                let prev_txt_hex = cfg.text_color_hex.clone();
                color_hex_picker(ui, "Warna Tulisan:", &mut cfg.text_color_hex);
                if cfg.text_color_hex != prev_txt_hex {
                    cfg_changed = true;
                }
                ui.add_space(3.0);
                if ui
                    .checkbox(&mut cfg.show_text_bg, "Latar Belakang Teks (Badge Gelap)")
                    .changed()
                {
                    cfg_changed = true;
                }
            }
        });
    });

    // Card 4: Continuous vs Timed playback in Play Skenario
    inspector_card(ui, "Mode & Durasi di Play Skenario", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .checkbox(
                    &mut cfg.continuous,
                    "Tampil Terus-Menerus (hilang saat tombol Stop)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Durasi Tampil:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    numeric_spinner(
                        ui,
                        ("ptr_duration", a.id),
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
        write_pointer_arrow_config(a, &cfg);
    }
}

fn draw_spawn_hide_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    markers_list: &[(Uuid, String)],
    editable: bool,
) {
    let mut cfg = parse_spawn_hide_config(a);
    let mut cfg_changed = false;
    let is_spawn = cfg.action == SpawnHideAction::Spawn;

    // Card 1: Identity, Action Mode (Spawn/Hide) & Multi-Object Targets
    inspector_card(ui, "Spawn & Hide — Mode & Target Objek 3D", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            let (banner_bg, banner_stroke, banner_fg, banner_txt) = if is_spawn {
                (
                    egui::Color32::from_rgb(6, 56, 42),
                    egui::Color32::from_rgb(16, 185, 129),
                    egui::Color32::from_rgb(167, 243, 208),
                    "SPAWN: Di Plot Editor target tetap tampil agar mudah ditata. Saat Play Skenario mulai, target otomatis tersembunyi dan muncul saat di-Play.",
                )
            } else {
                (
                    egui::Color32::from_rgb(68, 14, 28),
                    egui::Color32::from_rgb(244, 63, 94),
                    egui::Color32::from_rgb(254, 205, 211),
                    "HIDE: Di Play Skenario target mulai tampil, menghilang saat animasi ini di-Play, dan muncul kembali saat Stop.",
                )
            };
            egui::Frame::new()
                .fill(banner_bg)
                .stroke(egui::Stroke::new(1.0, banner_stroke))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(banner_txt).size(9.5).color(banner_fg));
                });
            ui.add_space(5.0);

            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);

            ui.label(
                egui::RichText::new("Mode Aksi:")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.columns(2, |cols| {
                for (idx, &act) in SpawnHideAction::ALL.iter().enumerate() {
                    let active = cfg.action == act;
                    let fill = if active {
                        match act {
                            SpawnHideAction::Spawn => egui::Color32::from_rgb(5, 150, 105),
                            SpawnHideAction::Hide => egui::Color32::from_rgb(190, 18, 60),
                        }
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    };
                    let btn = egui::Button::new(
                        egui::RichText::new(act.label())
                            .size(10.5)
                            .strong()
                            .color(if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(148, 163, 184)
                            }),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(cols[idx].available_width(), 24.0));
                    if cols[idx].add(btn).clicked() && cfg.action != act {
                        cfg.action = act;
                        cfg_changed = true;
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Target Marker / Objek 3D ({} objek):",
                        cfg.marker_ids.len()
                    ))
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
                );
                if cfg.marker_ids.len() > 1 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Kosongkan")
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(248, 113, 113)),
                                )
                                .small()
                                .frame(false),
                            )
                            .clicked()
                        {
                            cfg.marker_ids.clear();
                            cfg_changed = true;
                        }
                    });
                }
            });
            ui.add_space(2.0);

            let is_picking_this = state.tool_mode == ToolMode::PickSpawnHideMarker
                && state.picking_spawn_hide_for_anim_id == Some(a.id);
            let pick_btn = egui::Button::new(
                egui::RichText::new(if is_picking_this {
                    "Selesai Pilih Objek di Peta (Enter)"
                } else {
                    "+ Pilih / Tambah Objek 3D di Peta"
                })
                .size(11.0)
                .strong()
                .color(egui::Color32::WHITE),
            )
            .fill(if is_picking_this {
                egui::Color32::from_rgb(5, 150, 105)
            } else {
                egui::Color32::from_rgb(109, 40, 217)
            })
            .min_size(egui::vec2(ui.available_width(), 26.0));
            if ui.add(pick_btn).clicked() {
                if is_picking_this {
                    state.tool_mode = ToolMode::Select;
                    state.picking_spawn_hide_for_anim_id = None;
                } else {
                    route_edit.clear();
                    state.picking_marker_for_anim_id = None;
                    state.picking_shoot_for_anim_id = None;
                    state.picking_pointer_for_anim_id = None;
                    state.picking_spawn_hide_for_anim_id = Some(a.id);
                    state.tool_mode = ToolMode::PickSpawnHideMarker;
                    state.status_banner = Some(format!(
                        "Klik 1 atau beberapa Marker / Objek 3D di peta untuk '{}' (Enter / Esc selesai).",
                        a.name
                    ));
                }
            }

            ui.add_space(4.0);
            // Dropdown fallback to add markers (including ones hidden in Layers)
            let mut add_marker_opt: Option<Uuid> = None;
            egui::ComboBox::from_id_salt(("spawn_hide_add_marker", a.id))
                .selected_text("+ Tambah dari daftar Objek 3D...")
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    let mut available_cnt = 0usize;
                    for (mid, mname) in markers_list {
                        if cfg.marker_ids.contains(mid) {
                            continue;
                        }
                        available_cnt += 1;
                        if ui.selectable_label(false, mname).clicked() {
                            add_marker_opt = Some(*mid);
                        }
                    }
                    if available_cnt == 0 {
                        ui.label(
                            egui::RichText::new("(Semua objek 3D sudah masuk daftar)")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                });
            if let Some(mid) = add_marker_opt {
                cfg.toggle_marker(mid);
                cfg_changed = true;
            }

            // Target list with Up/Down reorder (for stagger order) + Remove
            ui.add_space(4.0);
            if cfg.marker_ids.is_empty() {
                ui.label(
                    egui::RichText::new("Belum ada objek 3D terpilih. Klik tombol pilih di peta di atas.")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(248, 113, 113)),
                );
            } else {
                let mut move_swap: Option<(usize, usize)> = None;
                let mut remove_idx: Option<usize> = None;
                let len = cfg.marker_ids.len();
                for (idx, mid) in cfg.marker_ids.iter().enumerate() {
                    let mname = markers_list
                        .iter()
                        .find(|(id, _)| id == mid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("(Objek terhapus)");
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(15, 23, 42))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("#{}", idx + 1))
                                        .size(10.0)
                                        .strong()
                                        .color(if is_spawn {
                                            egui::Color32::from_rgb(52, 211, 153)
                                        } else {
                                            egui::Color32::from_rgb(251, 113, 133)
                                        }),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(mname)
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(226, 232, 240)),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("x")
                                                        .size(10.0)
                                                        .color(egui::Color32::from_rgb(248, 113, 113)),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Lepas objek dari daftar")
                                            .clicked()
                                        {
                                            remove_idx = Some(idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                idx + 1 < len,
                                                egui::Button::new(
                                                    egui::RichText::new("v").size(9.5),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Turunkan urutan stagger")
                                            .clicked()
                                        {
                                            move_swap = Some((idx, idx + 1));
                                        }
                                        if ui
                                            .add_enabled(
                                                idx > 0,
                                                egui::Button::new(
                                                    egui::RichText::new("^").size(9.5),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Naikkan urutan stagger")
                                            .clicked()
                                        {
                                            move_swap = Some((idx - 1, idx));
                                        }
                                    },
                                );
                            });
                        });
                }
                if let Some((i, j)) = move_swap {
                    cfg.marker_ids.swap(i, j);
                    cfg_changed = true;
                }
                if let Some(ri) = remove_idx {
                    cfg.marker_ids.remove(ri);
                    cfg_changed = true;
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
    });

    // Card 2: Transition, Stagger, Delay, FX Ring & Editor Preview
    inspector_card(ui, "Transisi, Urutan & Waktu", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(
                egui::RichText::new("Efek Transisi:")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.columns(3, |cols| {
                for (idx, &tr) in SpawnHideTransition::ALL.iter().enumerate() {
                    let active = cfg.transition == tr;
                    let btn = egui::Button::new(
                        egui::RichText::new(tr.short_label())
                            .size(10.0)
                            .strong()
                            .color(if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(148, 163, 184)
                            }),
                    )
                    .fill(if active {
                        egui::Color32::from_rgb(109, 40, 217)
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    })
                    .min_size(egui::vec2(cols[idx].available_width(), 24.0));
                    if cols[idx]
                        .add(btn)
                        .on_hover_text(tr.label())
                        .clicked()
                        && cfg.transition != tr
                    {
                        cfg.transition = tr;
                        cfg_changed = true;
                    }
                }
            });

            ui.add_space(5.0);
            if cfg.transition != SpawnHideTransition::Instant {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Durasi Transisi:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("sh_trans_sec", a.id),
                        &mut cfg.transition_sec,
                        SpinnerConfig {
                            min: 0.05,
                            max: 3.0,
                            step: 0.05,
                            drag_speed: 0.02,
                            decimals: 2,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                });
            }

            if cfg.transition == SpawnHideTransition::DropVertical {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Tinggi Turun / Naik:")
                            .size(11.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    cfg_changed |= numeric_spinner(
                        ui,
                        ("sh_drop_h", a.id),
                        &mut cfg.drop_height_m,
                        SpinnerConfig {
                            min: 1.0,
                            max: 40.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " m",
                            wrap_around: false,
                        },
                    );
                });
            }

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jeda Antar Objek (Stagger):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("sh_stagger_sec", a.id),
                    &mut cfg.stagger_sec,
                    SpinnerConfig {
                        min: 0.0,
                        max: 5.0,
                        step: 0.05,
                        drag_speed: 0.02,
                        decimals: 2,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.label(
                egui::RichText::new(
                    "0.00 dtk = semua objek serentak; > 0 = muncul/hilang berurutan #1, #2, #3...",
                )
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );

            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jeda Awal (Delay):")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("sh_delay_sec", a.id),
                    &mut cfg.delay_sec,
                    SpinnerConfig {
                        min: 0.0,
                        max: 60.0,
                        step: 0.5,
                        drag_speed: 0.05,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            if ui
                .checkbox(&mut cfg.show_fx_ring, "Tampilkan Cincin Efek di Tanah")
                .on_hover_text(
                    "Menampilkan cincin cahaya singkat (hijau untuk Spawn, merah untuk Hide) agar perubahan mudah terlihat",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if ui
                .checkbox(
                    &mut cfg.include_in_play_all,
                    "Ikut dijalankan saat klik 'Play Semua'",
                )
                .on_hover_text(
                    "Default mati agar Spawn/Hide dipicu manual di momen yang tepat; aktifkan jika ingin ikut Play Semua (disarankan pakai Delay).",
                )
                .changed()
            {
                cfg_changed = true;
            }

            ui.add_space(5.0);
            let total_d = cfg.total_duration_sec();
            ui.label(
                egui::RichText::new(format!("Estimasi durasi total: {total_d:.2} dtk"))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(196, 181, 253)),
            );
            ui.add_space(4.0);

            let is_previewing = state
                .spawn_hide_preview
                .is_some_and(|(pid, _)| pid == a.id);
            let prev_btn = egui::Button::new(
                egui::RichText::new(if is_previewing {
                    "Hentikan Preview Transisi"
                } else {
                    "Preview Transisi di Editor"
                })
                .size(11.0)
                .strong()
                .color(egui::Color32::WHITE),
            )
            .fill(if is_previewing {
                egui::Color32::from_rgb(180, 83, 9)
            } else {
                egui::Color32::from_rgb(14, 116, 144)
            })
            .min_size(egui::vec2(ui.available_width(), 25.0));
            if ui
                .add_enabled(!cfg.marker_ids.is_empty(), prev_btn)
                .on_hover_text("Putar 1x simulasi efek transisi langsung di peta Editor")
                .clicked()
            {
                if is_previewing {
                    state.spawn_hide_preview = None;
                } else {
                    state.spawn_hide_preview = Some((a.id, 0.0));
                }
            }
        });
    });

    if cfg_changed {
        write_spawn_hide_config(a, &cfg);
    }
}

fn short_anim_badge_for_picker(a: &twg_shared::models::Anim) -> &'static str {
    if twg_shared::audio_anim::is_audio_anim(a) {
        "PLAY SUARA"
    } else if is_group_anim(a) {
        match twg_shared::group_anim::group_anim_mode(a).unwrap_or_default() {
            GroupAnimMode::Combine => "COMBINE",
            GroupAnimMode::Sequence => "SEQUENCE",
        }
    } else if is_spawn_hide_anim(a) {
        match spawn_hide_action(a).unwrap_or_default() {
            SpawnHideAction::Spawn => "SPAWN",
            SpawnHideAction::Hide => "HIDE",
        }
    } else if is_particle_anim(a) {
        parse_particle_anim_config(a).kind.short_tag()
    } else if is_pointer_arrow_anim(a) {
        "PANAH 3D"
    } else if twg_shared::model_action_anim::is_model_action_anim(a) {
        "AKSI 3D"
    } else if twg_shared::dialog_anim::is_dialog_anim(a) {
        "DIALOG"
    } else if twg_shared::tactical_arrow_anim::is_tactical_arrow_anim(a) {
        "ALUR 3D"
    } else if twg_shared::hud_banner_anim::is_hud_banner_anim(a) {
        twg_shared::hud_banner_anim::parse_hud_banner_config(a)
            .style
            .short_tag()
    } else if let Some(sk) = shoot_anim_kind(a) {
        sk.short_tag()
    } else if twg_shared::billboard_anim::is_billboard_anim(a) {
        if twg_shared::billboard_anim::parse_billboard_anim_config(a).media_mode
            == twg_shared::billboard_anim::BillboardMediaMode::Video
        {
            "VIDEOTRON"
        } else {
            "BILLBOARD"
        }
    } else if twg_shared::camera_anim::is_camera_static_anim(a) {
        "STATIC CAM"
    } else if twg_shared::camera_anim::is_camera_follow_anim(a) {
        "FOLLOW CAM"
    } else if twg_shared::environment_anim::is_water_anim(a) {
        twg_shared::environment_anim::parse_water_anim_config(a)
            .kind
            .short_tag()
    } else if twg_shared::environment_anim::is_rain_anim(a) {
        twg_shared::environment_anim::parse_rain_anim_config(a)
            .preset
            .short_tag()
    } else if is_traffic_signal_anim(a) {
        "LAMPU MERAH"
    } else if is_traffic_anim(a) {
        "TRAFFIC"
    } else if is_traffic_gridlock_anim(a) {
        "MACET TOTAL"
    } else if is_traffic_manual_anim(a) {
        "SIMPANG MANUAL"
    } else if is_traffic_queue_anim(a) {
        "URAI SIMPANG"
    } else if is_traffic_diversion_anim(a) {
        "ALIH ARUS"
    } else if is_multi_route_anim(a) {
        if parse_multi_route_config(a).formation == RouteFormationKind::Patwal {
            "PATWAL"
        } else {
            "FORMASI"
        }
    } else {
        "RUTE"
    }
}

fn draw_traffic_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    editable: bool,
) {
    let is_signal = is_traffic_signal_anim(a);
    let mut cfg = parse_traffic_anim_config(a);
    let mut cfg_changed = false;

    // 1. General Info & Playback Mode
    let card_title = if is_signal {
        "Lalu Lintas Lampu Merah (APILL)"
    } else {
        "Simulasi Lalu Lintas (Traffic Pool)"
    };
    inspector_card(ui, card_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut cfg.continuous, "Terus-menerus (Loop)")
                    .changed()
                {
                    cfg_changed = true;
                }
                ui.checkbox(&mut a.autoplay, "Autoplay");
            });
            if !cfg.continuous {
                ui.horizontal(|ui| {
                    ui.label("Durasi (dtk):");
                    numeric_spinner(
                        ui,
                        ("tr_dur", a.id),
                        &mut a.duration_sec,
                        SpinnerConfig {
                            min: 10.0,
                            max: 3600.0,
                            step: 5.0,
                            drag_speed: 1.0,
                            decimals: 1,
                            suffix: " s",
                            wrap_around: false,
                        },
                    );
                });
                ui.checkbox(&mut a.replay, "Ulangi (Repeat)");
            }
        });
    });

    // 2. Multi-Lane Manager (Edit waypoints on map, Add Lane, Reverse 2-Way with custom meter offset)
    inspector_card(
        ui,
        &format!("Jalur / Rute Traffic ({})", cfg.lanes.len()),
        true,
        |ui| {
            ui.label(
                egui::RichText::new(
                    "Klik 'Edit Jalur' (atau klik garis di peta) untuk menggeser titik, tambah titik (+) di tengah, atau hapus titik.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            if editable {
                let add_lane_btn = egui::Button::new(
                    egui::RichText::new("+ Gambar Jalur Baru di Peta")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(217, 119, 6))
                .min_size(egui::vec2(ui.available_width(), 24.0));
                if ui.add(add_lane_btn).clicked() {
                    state.traffic_scale_preview = None;
                    state.selected_traffic_light = None;
                    route_edit.clear();
                    state.tool_mode = ToolMode::DrawTrafficLane;
                    state.drawing_traffic_lane_for_anim_id = Some(a.id);
                    state.drawing_traffic_is_signal = is_signal;
                    state.draft_points_lnglat.clear();
                    state.status_banner = Some(
                        "Klik titik-titik Jalur Traffic baru di peta (min 2 titik), lalu tekan Enter / Finish."
                            .into(),
                    );
                }
                ui.add_space(5.0);

                // Configurable distance & side for 'Balik 2 Arah' reverse-lane duplication
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 42))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 62, 92)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Balik:")
                                    .size(10.5)
                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            let w = (ui.available_width() - 2.0).max(72.0);
                            numeric_spinner_w(
                                ui,
                                ("tr_rev_dist", a.id),
                                &mut state.traffic_reverse_offset_m,
                                w,
                                SpinnerConfig {
                                    min: 0.5,
                                    max: 80.0,
                                    step: 0.5,
                                    drag_speed: 0.25,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Sisi Duplikat:")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                            if ui
                                .selectable_label(
                                    state.traffic_reverse_side == OvertakeSide::Right,
                                    "Kanan",
                                )
                                .clicked()
                            {
                                state.traffic_reverse_side = OvertakeSide::Right;
                            }
                            if ui
                                .selectable_label(
                                    state.traffic_reverse_side == OvertakeSide::Left,
                                    "Kiri",
                                )
                                .clicked()
                            {
                                state.traffic_reverse_side = OvertakeSide::Left;
                            }
                        });
                    });
                ui.add_space(5.0);
            }

            let mut delete_lane_idx: Option<usize> = None;
            let mut reverse_dup_idx: Option<usize> = None;
            let mut flip_dir_idx: Option<usize> = None;

            for idx in 0..cfg.lanes.len() {
                let lane_len = cfg.lanes[idx].total_length_m();
                let pts_cnt = cfg.lanes[idx].geom.len();
                let is_editing_this_lane = route_edit.editing_anim_id == Some(a.id)
                    && route_edit.traffic_lane_idx == Some(idx);

                egui::Frame::new()
                    .fill(if is_editing_this_lane {
                        egui::Color32::from_rgb(10, 42, 38)
                    } else {
                        egui::Color32::from_rgb(15, 23, 42)
                    })
                    .stroke(egui::Stroke::new(
                        if is_editing_this_lane { 1.4 } else { 1.0 },
                        if is_editing_this_lane {
                            egui::Color32::from_rgb(16, 185, 129)
                        } else if cfg.lanes[idx].enabled {
                            egui::Color32::from_rgb(44, 62, 92)
                        } else {
                            egui::Color32::from_rgb(51, 51, 60)
                        },
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if editable {
                                if ui.checkbox(&mut cfg.lanes[idx].enabled, "").changed() {
                                    cfg_changed = true;
                                }
                                let name_w = (ui.available_width() - 88.0).clamp(58.0, 125.0);
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut cfg.lanes[idx].name)
                                            .desired_width(name_w),
                                    )
                                    .changed()
                                {
                                    cfg_changed = true;
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new(&cfg.lanes[idx].name)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!("{lane_len:.0}m ({pts_cnt} pt)"))
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                        if editable {
                            ui.add_space(3.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let edit_btn = egui::Button::new(
                                    egui::RichText::new(if is_editing_this_lane {
                                        "Selesai Edit"
                                    } else {
                                        "Edit Jalur"
                                    })
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                                )
                                .fill(if is_editing_this_lane {
                                    egui::Color32::from_rgb(5, 150, 105)
                                } else {
                                    egui::Color32::from_rgb(30, 64, 175)
                                })
                                .small();
                                if ui
                                    .add(edit_btn)
                                    .on_hover_text(
                                        "Aktifkan edit titik jalur di peta (geser titik, sisipkan + di tengah, atau hapus titik)",
                                    )
                                    .clicked()
                                {
                                    if is_editing_this_lane {
                                        route_edit.clear();
                                    } else {
                                        state.traffic_scale_preview = None;
                                        playback.stop_editor_preview();
                                        state.tool_mode = ToolMode::Select;
                                        route_edit.editing_anim_id = Some(a.id);
                                        route_edit.traffic_lane_idx = Some(idx);
                                        route_edit.selected_waypoint = None;
                                        route_edit.add_point_on_map_mode = false;
                                        route_edit.active_drag = None;
                                        state.status_banner = Some(format!(
                                            "Edit '{}': tarik titik untuk geser, klik (+) hijau untuk sisipkan titik, atau klik (x) merah untuk hapus.",
                                            cfg.lanes[idx].name
                                        ));
                                    }
                                }
                                if ui
                                    .small_button(format!(
                                        "Balik ({:.1}m)",
                                        state.traffic_reverse_offset_m
                                    ))
                                    .on_hover_text(
                                        "Duplikat jalur ini sebagai jalur lawan arah sejajar sejauh Jarak Balik di atas",
                                    )
                                    .clicked()
                                {
                                    reverse_dup_idx = Some(idx);
                                }
                                if ui
                                    .small_button("Putar")
                                    .on_hover_text("Balik arah titik awal dan akhir jalur ini")
                                    .clicked()
                                {
                                    flip_dir_idx = Some(idx);
                                }
                                if cfg.lanes.len() > 1
                                    && ui
                                        .small_button(
                                            egui::RichText::new("Hapus")
                                                .color(egui::Color32::from_rgb(248, 113, 113)),
                                        )
                                        .clicked()
                                {
                                    delete_lane_idx = Some(idx);
                                }
                            });
                            if is_editing_this_lane {
                                ui.add_space(3.0);
                                ui.horizontal(|ui| {
                                    let add_mode = route_edit.add_point_on_map_mode;
                                    let add_pt_btn = egui::Button::new(
                                        egui::RichText::new(if add_mode {
                                            "Klik Peta: Tambah Titik (Aktif)"
                                        } else {
                                            "+ Tambah Titik di Ujung"
                                        })
                                        .size(10.0)
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if add_mode {
                                        egui::Color32::from_rgb(180, 83, 9)
                                    } else {
                                        egui::Color32::from_rgb(14, 116, 144)
                                    })
                                    .small();
                                    if ui.add(add_pt_btn).clicked() {
                                        route_edit.add_point_on_map_mode = !add_mode;
                                    }
                                });
                            }
                        }
                    });
                ui.add_space(3.0);
            }

            if let Some(idx) = reverse_dup_idx {
                let new_name = format!("{} (Balik)", cfg.lanes[idx].name);
                let dist_m = state.traffic_reverse_offset_m.clamp(0.5, 80.0);
                if let Some(rev) =
                    build_reverse_lane(&cfg.lanes[idx], dist_m, state.traffic_reverse_side, new_name)
                {
                    cfg.lanes.push(rev);
                    cfg_changed = true;
                    state.status_banner = Some(format!(
                        "Jalur lawan arah (jarak {dist_m:.1} m) berhasil diduplikasi!"
                    ));
                }
            }
            if let Some(idx) = flip_dir_idx {
                cfg.lanes[idx].geom.reverse();
                cfg_changed = true;
            }
            if let Some(idx) = delete_lane_idx {
                let removed_lane_id = cfg.lanes[idx].id;
                cfg.lanes.remove(idx);
                cfg.traffic_lights.retain(|tl| tl.lane_id != removed_lane_id);
                if state
                    .selected_traffic_light
                    .is_some_and(|(aid, lid)| {
                        aid == a.id && !cfg.traffic_lights.iter().any(|tl| tl.id == lid)
                    })
                {
                    state.selected_traffic_light = None;
                }
                if route_edit.editing_anim_id == Some(a.id) {
                    route_edit.clear();
                }
                cfg_changed = true;
            }
        },
    );

    // 2b. Dedicated Traffic Light (APILL) & Signal Phase Manager (only for `traffic_signal` animations)
    if is_signal {
        if cfg.signal_phases.is_empty() {
            cfg.signal_phases.push(TrafficSignalPhase::new("Fase 1"));
            cfg.signal_phases.push(TrafficSignalPhase::new("Fase 2"));
            cfg_changed = true;
        }
        let lights_cnt = cfg.traffic_lights.len();
        let phases_cnt = cfg.signal_phases.len();
        inspector_card(
            ui,
            &format!("Lampu Merah 3D (APILL) ({lights_cnt} Lampu • {phases_cnt} Fase)"),
            true,
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Pasang lampu merah menempel ke jalur. Klik tiang lampu di peta (atau pilih di daftar) untuk Move / Rotate / Scale 3D Gizmo.",
                    )
                    .size(10.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(4.0);

                if editable {
                    let is_placing = state.tool_mode == ToolMode::PlaceTrafficLight
                        && state.placing_traffic_light_for_anim_id == Some(a.id);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Fase Pasang:")
                                .size(10.5)
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                        );
                        let p_idx = state
                            .placing_traffic_light_phase_idx
                            .min(cfg.signal_phases.len().saturating_sub(1));
                        state.placing_traffic_light_phase_idx = p_idx;
                        let cur_phase_lbl = cfg
                            .signal_phases
                            .get(p_idx)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| format!("Fase {}", p_idx + 1));
                        egui::ComboBox::from_id_salt(("tl_place_phase", a.id))
                            .selected_text(cur_phase_lbl)
                            .width(105.0)
                            .show_ui(ui, |ui| {
                                for (pi, ph) in cfg.signal_phases.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut state.placing_traffic_light_phase_idx,
                                        pi,
                                        &ph.name,
                                    );
                                }
                            });
                    });
                    ui.add_space(4.0);

                    let place_btn = egui::Button::new(
                        egui::RichText::new(if is_placing {
                            "Klik di Garis Jalur (Aktif • Esc Selesai)"
                        } else {
                            "+ Pasang Lampu Merah di Jalur"
                        })
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                    )
                    .fill(if is_placing {
                        egui::Color32::from_rgb(5, 150, 105)
                    } else {
                        egui::Color32::from_rgb(225, 29, 72)
                    })
                    .min_size(egui::vec2(ui.available_width(), 24.0));
                    if ui
                        .add_enabled(!cfg.lanes.is_empty(), place_btn)
                        .on_hover_text(
                            "Klik di atas garis jalur pada peta untuk menempelkan Garis Henti & Lampu Merah 3D otomatis menghadap kendaraan",
                        )
                        .clicked()
                    {
                        if is_placing {
                            state.placing_traffic_light_for_anim_id = None;
                            state.tool_mode = ToolMode::Select;
                        } else {
                            state.traffic_scale_preview = None;
                            route_edit.clear();
                            state.placing_traffic_light_for_anim_id = Some(a.id);
                            state.tool_mode = ToolMode::PlaceTrafficLight;
                            state.status_banner = Some(format!(
                                "Mode Pasang Lampu Merah (Fase {}): Klik di atas garis jalur pada peta (Esc selesai).",
                                state.placing_traffic_light_phase_idx + 1
                            ));
                        }
                    }

                    ui.add_space(4.0);
                    let auto_btn = egui::Button::new(
                        egui::RichText::new("Auto-Pasang di Simpang")
                            .size(10.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(14, 116, 144))
                    .min_size(egui::vec2(ui.available_width(), 22.0));
                    if ui
                        .add_enabled(!cfg.lanes.is_empty(), auto_btn)
                        .on_hover_text(
                            "Deteksi perpotongan antar jalur & pasang Lampu Merah otomatis 10m sebelum simpang (jalur sejajar berbagi fase, jalur silang beda fase)",
                        )
                        .clicked()
                    {
                        let added =
                            auto_place_intersection_traffic_lights(&mut cfg, state.geo_anchor);
                        if added > 0 {
                            cfg_changed = true;
                            if let Some(last_tl) = cfg.traffic_lights.last() {
                                state.selected_traffic_light = Some((a.id, last_tl.id));
                            }
                            state.status_banner = Some(format!(
                                "{added} Lampu Merah 3D otomatis dipasang! Klik lampu di peta atau daftar untuk Move / Rotate / Scale."
                            ));
                        } else {
                            state.status_banner = Some(
                                "Semua jalur aktif sudah memiliki Lampu Merah. Klik '+ Pasang Lampu Merah' untuk tambah lagi."
                                    .into(),
                            );
                        }
                    }
                    ui.add_space(6.0);
                }

                // --- Signal Phases Editor ---
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 42))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 62, 92)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let total_cycle: f64 =
                            cfg.signal_phases.iter().map(|p| p.span_sec()).sum();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Fase Giliran (Siklus {total_cycle:.1}s)"
                                ))
                                .size(10.5)
                                .strong()
                                .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            if editable {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("+ Fase").clicked()
                                            && cfg.signal_phases.len() < 8
                                        {
                                            let n = cfg.signal_phases.len() + 1;
                                            cfg.signal_phases
                                                .push(TrafficSignalPhase::new(format!("Fase {n}")));
                                            cfg_changed = true;
                                        }
                                    },
                                );
                            }
                        });
                        ui.add_space(4.0);

                        let mut remove_phase_idx: Option<usize> = None;
                        let phase_len = cfg.signal_phases.len();
                        for pi in 0..phase_len {
                            let assigned_cnt = cfg
                                .traffic_lights
                                .iter()
                                .filter(|tl| tl.phase_idx == pi)
                                .count();
                            ui.horizontal(|ui| {
                                if editable {
                                    let w = (ui.available_width() - 78.0).clamp(54.0, 110.0);
                                    if ui
                                        .add(
                                            egui::TextEdit::singleline(
                                                &mut cfg.signal_phases[pi].name,
                                            )
                                            .desired_width(w),
                                        )
                                        .changed()
                                    {
                                        cfg_changed = true;
                                    }
                                } else {
                                    ui.label(
                                        egui::RichText::new(&cfg.signal_phases[pi].name)
                                            .size(10.5)
                                            .strong(),
                                    );
                                }
                                ui.label(
                                    egui::RichText::new(format!("{assigned_cnt} lampu"))
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                                if editable && phase_len > 1 {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button(
                                                    egui::RichText::new("x")
                                                        .color(egui::Color32::from_rgb(248, 113, 113)),
                                                )
                                                .clicked()
                                            {
                                                remove_phase_idx = Some(pi);
                                            }
                                        },
                                    );
                                }
                            });
                            ui.add_enabled_ui(editable, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("H:")
                                            .size(9.5)
                                            .color(egui::Color32::from_rgb(74, 222, 128)),
                                    );
                                    let prev_g = cfg.signal_phases[pi].green_sec;
                                    numeric_spinner_w(
                                        ui,
                                        ("ts_ph_g", a.id, pi),
                                        &mut cfg.signal_phases[pi].green_sec,
                                        56.0,
                                        SpinnerConfig {
                                            min: 2.0,
                                            max: 180.0,
                                            step: 1.0,
                                            drag_speed: 0.25,
                                            decimals: 0,
                                            suffix: "s",
                                            wrap_around: false,
                                        },
                                    );
                                    if (cfg.signal_phases[pi].green_sec - prev_g).abs() > 1e-6 {
                                        cfg_changed = true;
                                    }
                                    ui.label(
                                        egui::RichText::new("K:")
                                            .size(9.5)
                                            .color(egui::Color32::from_rgb(250, 204, 21)),
                                    );
                                    let prev_y = cfg.signal_phases[pi].yellow_sec;
                                    numeric_spinner_w(
                                        ui,
                                        ("ts_ph_y", a.id, pi),
                                        &mut cfg.signal_phases[pi].yellow_sec,
                                        50.0,
                                        SpinnerConfig {
                                            min: 1.0,
                                            max: 15.0,
                                            step: 0.5,
                                            drag_speed: 0.1,
                                            decimals: 1,
                                            suffix: "s",
                                            wrap_around: false,
                                        },
                                    );
                                    if (cfg.signal_phases[pi].yellow_sec - prev_y).abs() > 1e-6 {
                                        cfg_changed = true;
                                    }
                                    ui.label(
                                        egui::RichText::new("AR:")
                                            .size(9.5)
                                            .color(egui::Color32::from_rgb(248, 113, 113)),
                                    );
                                    let prev_ar = cfg.signal_phases[pi].all_red_sec;
                                    numeric_spinner_w(
                                        ui,
                                        ("ts_ph_ar", a.id, pi),
                                        &mut cfg.signal_phases[pi].all_red_sec,
                                        50.0,
                                        SpinnerConfig {
                                            min: 0.0,
                                            max: 10.0,
                                            step: 0.5,
                                            drag_speed: 0.1,
                                            decimals: 1,
                                            suffix: "s",
                                            wrap_around: false,
                                        },
                                    );
                                    if (cfg.signal_phases[pi].all_red_sec - prev_ar).abs() > 1e-6 {
                                        cfg_changed = true;
                                    }
                                });
                            });
                            if pi + 1 < phase_len {
                                ui.add_space(3.0);
                            }
                        }

                        if let Some(rm_idx) = remove_phase_idx
                            && cfg.signal_phases.len() > 1
                        {
                            cfg.signal_phases.remove(rm_idx);
                            let max_p = cfg.signal_phases.len() - 1;
                            for tl in &mut cfg.traffic_lights {
                                if tl.phase_idx > rm_idx {
                                    tl.phase_idx -= 1;
                                } else if tl.phase_idx > max_p {
                                    tl.phase_idx = max_p;
                                }
                            }
                            cfg_changed = true;
                        }

                        ui.add_space(4.0);
                        ui.add_enabled_ui(editable, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Offset Siklus:")
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                                let prev_off = cfg.signal_cycle_offset_sec;
                                numeric_spinner_w(
                                    ui,
                                    ("ts_cyc_off", a.id),
                                    &mut cfg.signal_cycle_offset_sec,
                                    82.0,
                                    SpinnerConfig {
                                        min: 0.0,
                                        max: 600.0,
                                        step: 1.0,
                                        drag_speed: 0.25,
                                        decimals: 1,
                                        suffix: " s",
                                        wrap_around: false,
                                    },
                                );
                                if (cfg.signal_cycle_offset_sec - prev_off).abs() > 1e-6 {
                                    cfg_changed = true;
                                }
                            });
                            ui.add_space(3.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Zona Larang Salip:")
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(252, 165, 165)),
                                )
                                .on_hover_text(
                                    "Jarak sebelum garis henti di mana kendaraan dilarang menyalip (100% saat Merah/Kuning, ~55% saat Hijau; 0 m = bebas menyalip)",
                                );
                                let prev_z = cfg.signal_no_overtake_zone_m;
                                numeric_spinner_w(
                                    ui,
                                    ("ts_no_ot_zone", a.id),
                                    &mut cfg.signal_no_overtake_zone_m,
                                    86.0,
                                    SpinnerConfig {
                                        min: 0.0,
                                        max: 1000.0,
                                        step: 5.0,
                                        drag_speed: 1.0,
                                        decimals: 0,
                                        suffix: " m",
                                        wrap_around: false,
                                    },
                                );
                                if (cfg.signal_no_overtake_zone_m - prev_z).abs() > 1e-6 {
                                    cfg_changed = true;
                                }
                            });
                        });
                    });

                ui.add_space(6.0);

                // --- Placed 3D Traffic Lights List + Full Move / Rotate / Scale Transform Controls ---
                if cfg.traffic_lights.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Belum ada Lampu Merah terpasang. Klik '+ Pasang Lampu Merah di Jalur' atau 'Auto-Pasang di Simpang' di atas.",
                        )
                        .size(10.5)
                        .color(egui::Color32::from_rgb(251, 191, 36)),
                    );
                } else {
                    let mut delete_light_idx: Option<usize> = None;
                    let live_colors: std::collections::HashMap<Uuid, TrafficLightColor> = state
                        .traffic_signal_live_status
                        .get(&a.id)
                        .map(|s| {
                            s.light_states
                                .iter()
                                .map(|lp| (lp.light_id, lp.color))
                                .collect()
                        })
                        .unwrap_or_default();

                    let lane_choices: Vec<(Uuid, String, f64)> = cfg
                        .lanes
                        .iter()
                        .map(|l| (l.id, l.name.clone(), l.total_length_m()))
                        .collect();
                    let phase_names: Vec<String> =
                        cfg.signal_phases.iter().map(|p| p.name.clone()).collect();
                    let lane_w_m = cfg.lane_width_m;
                    let ot_side = cfg.overtake_side;

                    for tl_idx in 0..cfg.traffic_lights.len() {
                        let tl_id = cfg.traffic_lights[tl_idx].id;
                        let is_tl_sel = state.selected_traffic_light == Some((a.id, tl_id));
                        let dot_col = match live_colors.get(&tl_id).copied() {
                            Some(TrafficLightColor::Green) => egui::Color32::from_rgb(34, 197, 94),
                            Some(TrafficLightColor::Yellow) => egui::Color32::from_rgb(234, 179, 8),
                            Some(TrafficLightColor::Red) => egui::Color32::from_rgb(239, 68, 68),
                            None => {
                                if cfg.traffic_lights[tl_idx].phase_idx == 0 {
                                    egui::Color32::from_rgb(34, 197, 94)
                                } else {
                                    egui::Color32::from_rgb(239, 68, 68)
                                }
                            }
                        };

                        egui::Frame::new()
                            .fill(if is_tl_sel {
                                egui::Color32::from_rgb(34, 24, 48)
                            } else {
                                egui::Color32::from_rgb(15, 23, 42)
                            })
                            .stroke(egui::Stroke::new(
                                if is_tl_sel { 1.4 } else { 1.0 },
                                if is_tl_sel {
                                    egui::Color32::from_rgb(250, 204, 21)
                                } else {
                                    egui::Color32::from_rgb(44, 62, 92)
                                },
                            ))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(9.0, 9.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(rect.center(), 4.2, dot_col);

                                    if editable {
                                        if ui
                                            .checkbox(&mut cfg.traffic_lights[tl_idx].enabled, "")
                                            .changed()
                                        {
                                            cfg_changed = true;
                                        }
                                        let name_w =
                                            (ui.available_width() - 96.0).clamp(60.0, 125.0);
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(
                                                    &mut cfg.traffic_lights[tl_idx].name,
                                                )
                                                .desired_width(name_w),
                                            )
                                            .changed()
                                        {
                                            cfg_changed = true;
                                        }
                                    } else {
                                        ui.label(
                                            egui::RichText::new(&cfg.traffic_lights[tl_idx].name)
                                                .size(10.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        );
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if editable
                                                && ui
                                                    .small_button(
                                                        egui::RichText::new("Hapus").color(
                                                            egui::Color32::from_rgb(248, 113, 113),
                                                        ),
                                                    )
                                                    .clicked()
                                            {
                                                delete_light_idx = Some(tl_idx);
                                            }
                                            let giz_btn = egui::Button::new(
                                                egui::RichText::new(if is_tl_sel {
                                                    "Aktif"
                                                } else {
                                                    "Pilih"
                                                })
                                                .size(9.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                            )
                                            .fill(if is_tl_sel {
                                                egui::Color32::from_rgb(217, 119, 6)
                                            } else {
                                                egui::Color32::from_rgb(30, 58, 138)
                                            })
                                            .small();
                                            if ui.add(giz_btn).clicked() {
                                                if is_tl_sel {
                                                    state.selected_traffic_light = None;
                                                } else {
                                                    state.traffic_scale_preview = None;
                                                    route_edit.clear();
                                                    state.selected_traffic_light =
                                                        Some((a.id, tl_id));
                                                    if !state.tool_mode.is_transform_tool() {
                                                        state.tool_mode = ToolMode::MoveMarker;
                                                    }
                                                }
                                            }
                                        },
                                    );
                                });

                                // Quick lane & phase binding row
                                ui.add_space(3.0);
                                ui.add_enabled_ui(editable, |ui| {
                                    ui.horizontal(|ui| {
                                        let cur_lane_name = lane_choices
                                            .iter()
                                            .find(|(id, _, _)| {
                                                *id == cfg.traffic_lights[tl_idx].lane_id
                                            })
                                            .map(|(_, n, _)| n.clone())
                                            .unwrap_or_else(|| "(Pilih Jalur)".into());
                                        egui::ComboBox::from_id_salt(("tl_lane", a.id, tl_id))
                                            .selected_text(cur_lane_name)
                                            .width(105.0)
                                            .show_ui(ui, |ui| {
                                                for (lid, lname, _) in &lane_choices {
                                                    if ui
                                                        .selectable_value(
                                                            &mut cfg.traffic_lights[tl_idx].lane_id,
                                                            *lid,
                                                            lname,
                                                        )
                                                        .changed()
                                                    {
                                                        cfg_changed = true;
                                                    }
                                                }
                                            });
                                        let cur_phase_name = phase_names
                                            .get(cfg.traffic_lights[tl_idx].phase_idx)
                                            .cloned()
                                            .unwrap_or_else(|| {
                                                format!(
                                                    "Fase {}",
                                                    cfg.traffic_lights[tl_idx].phase_idx + 1
                                                )
                                            });
                                        egui::ComboBox::from_id_salt(("tl_phase", a.id, tl_id))
                                            .selected_text(cur_phase_name)
                                            .width(82.0)
                                            .show_ui(ui, |ui| {
                                                for (pi, pname) in phase_names.iter().enumerate() {
                                                    if ui
                                                        .selectable_value(
                                                            &mut cfg.traffic_lights[tl_idx]
                                                                .phase_idx,
                                                            pi,
                                                            pname,
                                                        )
                                                        .changed()
                                                    {
                                                        cfg_changed = true;
                                                    }
                                                }
                                            });
                                    });
                                });

                                // Expanded 3D Move / Rotate / Scale & Stop-Line controls for the selected light
                                if is_tl_sel {
                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(3.0);

                                    if editable {
                                        // 3D Gizmo quick mode buttons: Move / Rotate / Scale + Focus
                                        ui.horizontal(|ui| {
                                            let is_mv = matches!(
                                                state.tool_mode,
                                                ToolMode::Select | ToolMode::MoveMarker
                                            );
                                            let is_rot = state.tool_mode == ToolMode::RotateMarker;
                                            let is_scl = state.tool_mode == ToolMode::ScaleMarker;
                                            if ui
                                                .selectable_label(is_mv, "Move (2)")
                                                .clicked()
                                            {
                                                state.tool_mode = ToolMode::MoveMarker;
                                            }
                                            if ui
                                                .selectable_label(is_rot, "Rotate (3)")
                                                .clicked()
                                            {
                                                state.tool_mode = ToolMode::RotateMarker;
                                            }
                                            if ui
                                                .selectable_label(is_scl, "Scale (4)")
                                                .clicked()
                                            {
                                                state.tool_mode = ToolMode::ScaleMarker;
                                            }
                                            if ui.small_button("Focus").clicked() {
                                                let p = cfg.traffic_lights[tl_idx].pos_lnglat;
                                                let l = state
                                                    .geo_anchor
                                                    .lng_lat_alt_to_local(p[0], p[1], p[2]);
                                                tcam.target_local =
                                                    bevy::math::Vec2::new(l[0], l[2]);
                                            }
                                        });
                                        ui.add_space(2.0);
                                        ui.checkbox(
                                            &mut state.traffic_light_move_sync_stop_line,
                                            "Sinkron Garis Henti saat Move Gizmo",
                                        );
                                        ui.horizontal(|ui| {
                                            if ui
                                                .small_button("Snap Tiang ke Garis Henti")
                                                .clicked()
                                                && let Some(lane) = cfg.lanes.iter().find(|l| {
                                                    l.id == cfg.traffic_lights[tl_idx].lane_id
                                                })
                                                && let Some((pos, rot)) =
                                                    default_traffic_light_transform(
                                                        lane,
                                                        state.geo_anchor,
                                                        cfg.traffic_lights[tl_idx].stop_s_m,
                                                        lane_w_m,
                                                        ot_side,
                                                    )
                                            {
                                                cfg.traffic_lights[tl_idx].pos_lnglat = pos;
                                                cfg.traffic_lights[tl_idx].rot_deg = rot;
                                                cfg_changed = true;
                                            }
                                            if ui
                                                .small_button("Hadap ke Jalur")
                                                .clicked()
                                                && let Some(lane) = cfg.lanes.iter().find(|l| {
                                                    l.id == cfg.traffic_lights[tl_idx].lane_id
                                                })
                                                && let Some((_, fwd, _)) = sample_lane_local_frame(
                                                    lane,
                                                    state.geo_anchor,
                                                    cfg.traffic_lights[tl_idx].stop_s_m,
                                                )
                                            {
                                                let yaw = (-fwd.0).atan2(-fwd.1).to_degrees();
                                                cfg.traffic_lights[tl_idx].rot_deg =
                                                    [0.0, (yaw * 10.0).round() / 10.0, 0.0];
                                                cfg_changed = true;
                                            }
                                        });
                                    }

                                    ui.add_space(4.0);
                                    ui.add_enabled_ui(editable, |ui| {
                                        let max_s = lane_choices
                                            .iter()
                                            .find(|(id, _, _)| {
                                                *id == cfg.traffic_lights[tl_idx].lane_id
                                            })
                                            .map(|(_, _, len)| (*len - 2.0).max(4.0))
                                            .unwrap_or(200.0);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Garis Henti:")
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                                            );
                                            let prev_s = cfg.traffic_lights[tl_idx].stop_s_m;
                                            numeric_spinner_w(
                                                ui,
                                                ("tl_stop_s", a.id, tl_id),
                                                &mut cfg.traffic_lights[tl_idx].stop_s_m,
                                                92.0,
                                                SpinnerConfig {
                                                    min: 2.0,
                                                    max: max_s,
                                                    step: 1.0,
                                                    drag_speed: 0.35,
                                                    decimals: 1,
                                                    suffix: " m",
                                                    wrap_around: false,
                                                },
                                            );
                                            if (cfg.traffic_lights[tl_idx].stop_s_m - prev_s).abs()
                                                > 1e-6
                                            {
                                                cfg_changed = true;
                                            }
                                        });

                                        // Altitude & Position readout
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Alt (Y):")
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                            let prev_alt =
                                                cfg.traffic_lights[tl_idx].pos_lnglat[2];
                                            numeric_spinner_w(
                                                ui,
                                                ("tl_alt", a.id, tl_id),
                                                &mut cfg.traffic_lights[tl_idx].pos_lnglat[2],
                                                82.0,
                                                SpinnerConfig {
                                                    min: -50.0,
                                                    max: 2000.0,
                                                    step: 0.5,
                                                    drag_speed: 0.1,
                                                    decimals: 2,
                                                    suffix: " m",
                                                    wrap_around: false,
                                                },
                                            );
                                            if (cfg.traffic_lights[tl_idx].pos_lnglat[2] - prev_alt)
                                                .abs()
                                                > 1e-6
                                            {
                                                cfg_changed = true;
                                            }
                                        });

                                        // Rotation (X, Y, Z degrees)
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Rotasi:")
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                            for (axis_i, lbl) in [(0, "X"), (1, "Y"), (2, "Z")] {
                                                ui.label(
                                                    egui::RichText::new(lbl)
                                                        .size(9.5)
                                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                                );
                                                let prev_r =
                                                    cfg.traffic_lights[tl_idx].rot_deg[axis_i];
                                                numeric_spinner_w(
                                                    ui,
                                                    ("tl_rot", a.id, tl_id, axis_i),
                                                    &mut cfg.traffic_lights[tl_idx].rot_deg[axis_i],
                                                    54.0,
                                                    SpinnerConfig {
                                                        min: -360.0,
                                                        max: 360.0,
                                                        step: 5.0,
                                                        drag_speed: 0.8,
                                                        decimals: 1,
                                                        suffix: "°",
                                                        wrap_around: true,
                                                    },
                                                );
                                                if (cfg.traffic_lights[tl_idx].rot_deg[axis_i]
                                                    - prev_r)
                                                    .abs()
                                                    > 1e-6
                                                {
                                                    cfg_changed = true;
                                                }
                                            }
                                        });

                                        // Uniform & Per-Axis Scale
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Skala:")
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                            let mut uni = cfg.traffic_lights[tl_idx].scale[0];
                                            let prev_u = uni;
                                            numeric_spinner_w(
                                                ui,
                                                ("tl_scl_uni", a.id, tl_id),
                                                &mut uni,
                                                72.0,
                                                SpinnerConfig {
                                                    min: 0.15,
                                                    max: 15.0,
                                                    step: 0.1,
                                                    drag_speed: 0.03,
                                                    decimals: 2,
                                                    suffix: "x",
                                                    wrap_around: false,
                                                },
                                            );
                                            if (uni - prev_u).abs() > 1e-6 {
                                                cfg.traffic_lights[tl_idx].scale = [uni, uni, uni];
                                                cfg_changed = true;
                                            }
                                            if ui.small_button("Reset 1x").clicked() {
                                                cfg.traffic_lights[tl_idx].scale = [1.0, 1.0, 1.0];
                                                cfg_changed = true;
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            for (axis_i, lbl) in [(0, "SX"), (1, "SY"), (2, "SZ")] {
                                                ui.label(
                                                    egui::RichText::new(lbl)
                                                        .size(9.5)
                                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                                );
                                                let prev_sc =
                                                    cfg.traffic_lights[tl_idx].scale[axis_i];
                                                numeric_spinner_w(
                                                    ui,
                                                    ("tl_scl_ax", a.id, tl_id, axis_i),
                                                    &mut cfg.traffic_lights[tl_idx].scale[axis_i],
                                                    54.0,
                                                    SpinnerConfig {
                                                        min: 0.15,
                                                        max: 15.0,
                                                        step: 0.1,
                                                        drag_speed: 0.03,
                                                        decimals: 2,
                                                        suffix: "",
                                                        wrap_around: false,
                                                    },
                                                );
                                                if (cfg.traffic_lights[tl_idx].scale[axis_i]
                                                    - prev_sc)
                                                    .abs()
                                                    > 1e-6
                                                {
                                                    cfg_changed = true;
                                                }
                                            }
                                        });
                                    });
                                }
                            });
                        ui.add_space(3.0);
                    }

                    if let Some(di) = delete_light_idx {
                        let rm_id = cfg.traffic_lights[di].id;
                        cfg.traffic_lights.remove(di);
                        if state.selected_traffic_light == Some((a.id, rm_id)) {
                            state.selected_traffic_light = None;
                        }
                        cfg_changed = true;
                    }
                }
            },
        );
    }

    // 3. Shared Vehicle 3D Pool Picker (ALL 3D Catalog + Category Filter + 2-Model Front-to-Back Scale Preview!)
    inspector_card(
        ui,
        &format!("Pool Model 3D Kendaraan ({})", cfg.vehicle_pool.len()),
        true,
        |ui| {
            ui.label(
                egui::RichText::new(
                    "Bisa preview maks. 2 model sekaligus (berjejer depan-belakang di jalur) untuk membandingkan skala antar kendaraan langsung di peta.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            // Show active preview counter bar (0..=2 models) with quick Close All button
            let active_preview_count = state
                .traffic_scale_preview
                .as_ref()
                .filter(|p| p.anim_id == a.id)
                .map(|p| p.pool_indices.len())
                .unwrap_or(0);
            if active_preview_count > 0 {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(12, 46, 62))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Preview aktif: {active_preview_count}/2 model"
                                ))
                                .size(10.0)
                                .strong()
                                .color(egui::Color32::from_rgb(125, 211, 252)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button(
                                            egui::RichText::new("Tutup Semua")
                                                .size(9.5)
                                                .color(egui::Color32::from_rgb(253, 230, 138)),
                                        )
                                        .clicked()
                                    {
                                        state.traffic_scale_preview = None;
                                    }
                                },
                            );
                        });
                        ui.add_space(3.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("Posisi:")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            if ui
                                .selectable_label(
                                    state.traffic_preview_layout == TrafficPreviewLayout::FrontBack,
                                    "Depan-Belakang",
                                )
                                .clicked()
                            {
                                state.traffic_preview_layout = TrafficPreviewLayout::FrontBack;
                            }
                            if ui
                                .selectable_label(
                                    state.traffic_preview_layout == TrafficPreviewLayout::SideBySide,
                                    "Berdampingan",
                                )
                                .clicked()
                            {
                                state.traffic_preview_layout = TrafficPreviewLayout::SideBySide;
                            }
                        });
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Preview:")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            let w = (ui.available_width() - 2.0).max(66.0);
                            numeric_spinner_w(
                                ui,
                                ("tr_prev_gap", a.id),
                                &mut state.traffic_preview_gap_m,
                                w,
                                SpinnerConfig {
                                    min: 0.0,
                                    max: 50.0,
                                    step: 0.5,
                                    drag_speed: 0.2,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }

            if cfg.vehicle_pool.is_empty() {
                ui.label(
                    egui::RichText::new("Belum ada model kendaraan! Pilih minimal 1 model di bawah.")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(248, 113, 113)),
                );
                ui.add_space(4.0);
            }

            let mut remove_pool_idx: Option<usize> = None;
            let mut toggle_preview_idx: Option<usize> = None;
            let mut focus_gizmo_idx: Option<usize> = None;
            let has_any_lane = cfg.lanes.iter().any(|l| l.geom.len() >= 2);

            for idx in 0..cfg.vehicle_pool.len() {
                let preview_slot = state
                    .traffic_scale_preview
                    .as_ref()
                    .filter(|p| p.anim_id == a.id)
                    .and_then(|p| p.pool_indices.iter().position(|&pi| pi == idx));
                let is_in_preview = preview_slot.is_some();
                let is_active_gizmo = state
                    .traffic_scale_preview
                    .as_ref()
                    .is_some_and(|p| p.anim_id == a.id && p.active_gizmo_pool_idx == idx && is_in_preview);

                egui::Frame::new()
                    .fill(if is_active_gizmo {
                        egui::Color32::from_rgb(12, 46, 62)
                    } else if is_in_preview {
                        egui::Color32::from_rgb(42, 34, 16)
                    } else {
                        egui::Color32::from_rgb(15, 23, 42)
                    })
                    .stroke(egui::Stroke::new(
                        if is_in_preview { 1.3 } else { 1.0 },
                        if is_active_gizmo {
                            egui::Color32::from_rgb(56, 189, 248)
                        } else if is_in_preview {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            egui::Color32::from_rgb(44, 62, 92)
                        },
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // Header line 1: Model name + compact Hapus button (never overflows)
                        ui.horizontal(|ui| {
                            if editable {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(
                                                egui::RichText::new("Hapus")
                                                    .size(9.5)
                                                    .color(egui::Color32::from_rgb(248, 113, 113)),
                                            )
                                            .clicked()
                                        {
                                            remove_pool_idx = Some(idx);
                                        }
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let slot_tag = match (preview_slot, state.traffic_preview_layout) {
                                                    (Some(0), TrafficPreviewLayout::FrontBack) => "[Depan] ",
                                                    (Some(1), TrafficPreviewLayout::FrontBack) => "[Belakang] ",
                                                    (Some(0), TrafficPreviewLayout::SideBySide) => "[Sisi 1] ",
                                                    (Some(1), TrafficPreviewLayout::SideBySide) => "[Sisi 2] ",
                                                    _ => "",
                                                };
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(format!(
                                                            "{slot_tag}{}",
                                                            cfg.vehicle_pool[idx].name
                                                        ))
                                                        .size(11.0)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                    )
                                                    .truncate(),
                                                );
                                            },
                                        );
                                    },
                                );
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&cfg.vehicle_pool[idx].name)
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                            }
                        });

                        // Header line 2: Preview & Scale Gizmo controls (wrapped cleanly within card width)
                        if editable {
                            ui.add_space(2.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                if is_in_preview {
                                    if is_active_gizmo {
                                        egui::Frame::new()
                                            .fill(egui::Color32::from_rgb(8, 145, 178))
                                            .corner_radius(3.0)
                                            .inner_margin(egui::Margin::symmetric(6, 2))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new("Scale Gizmo Aktif")
                                                        .size(9.5)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                );
                                            });
                                    } else {
                                        let pick_gizmo = egui::Button::new(
                                            egui::RichText::new("Pindah Scale Gizmo")
                                                .size(9.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(37, 99, 235))
                                        .small();
                                        if ui.add(pick_gizmo).clicked() {
                                            focus_gizmo_idx = Some(idx);
                                        }
                                    }
                                    let close_btn = egui::Button::new(
                                        egui::RichText::new("Tutup Preview")
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(180, 83, 9))
                                    .small();
                                    if ui.add(close_btn).clicked() {
                                        toggle_preview_idx = Some(idx);
                                    }
                                } else {
                                    let lbl = if active_preview_count == 1 {
                                        "Preview Bandingkan (2/2)"
                                    } else {
                                        "Preview & Scale di Peta"
                                    };
                                    let prev_btn = egui::Button::new(
                                        egui::RichText::new(lbl)
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(14, 116, 144))
                                    .small();
                                    if ui
                                        .add_enabled(has_any_lane, prev_btn)
                                        .on_hover_text(
                                            "Tampilkan model 3D ini di jalur peta (maks. 2 model depan-belakang) untuk scaling langsung",
                                        )
                                        .clicked()
                                    {
                                        toggle_preview_idx = Some(idx);
                                    }
                                }
                            });
                        }

                        ui.add_space(3.0);
                        ui.add_enabled_ui(editable, |ui| {
                            let lbl_w = 50.0_f32;
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Skala:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 2.0).max(66.0);
                                let prev_s = cfg.vehicle_pool[idx].scale;
                                numeric_spinner_w(
                                    ui,
                                    ("tr_v_scale", a.id, idx),
                                    &mut cfg.vehicle_pool[idx].scale,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 0.01,
                                        max: 20.0,
                                        step: 0.01,
                                        drag_speed: 0.01,
                                        decimals: 2,
                                        suffix: "x",
                                        wrap_around: false,
                                    },
                                );
                                if (cfg.vehicle_pool[idx].scale - prev_s).abs() > 1e-6 {
                                    cfg_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Panjang:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 2.0).max(66.0);
                                let prev_l = cfg.vehicle_pool[idx].length_m;
                                numeric_spinner_w(
                                    ui,
                                    ("tr_v_len", a.id, idx),
                                    &mut cfg.vehicle_pool[idx].length_m,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 1.5,
                                        max: 24.0,
                                        step: 0.5,
                                        drag_speed: 0.2,
                                        decimals: 1,
                                        suffix: " m",
                                        wrap_around: false,
                                    },
                                );
                                if (cfg.vehicle_pool[idx].length_m - prev_l).abs() > 1e-6 {
                                    cfg_changed = true;
                                }
                            });
                            if let Some([mw, mh, ml]) = measured_vehicle_scaled_dims(
                                state,
                                &cfg.vehicle_pool[idx].model_path,
                                cfg.vehicle_pool[idx].scale as f32,
                                cfg.vehicle_pool[idx].rot_y_offset_deg,
                            ) {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "3D asli: P {ml:.1}m x L {mw:.1}m x T {mh:.1}m"
                                        ))
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(125, 211, 252)),
                                    );
                                    if (ml - cfg.vehicle_pool[idx].length_m as f32).abs() > 0.35
                                        && ui
                                            .small_button(format!("Sync P ({ml:.1}m)"))
                                            .on_hover_text(
                                                "Samakan Panjang simulasi dengan panjang asli mesh 3D agar anti-tabrak akurat",
                                            )
                                            .clicked()
                                    {
                                        cfg.vehicle_pool[idx].length_m =
                                            ((ml as f64).clamp(1.2, 24.0) * 10.0).round() / 10.0;
                                        cfg_changed = true;
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Rot Y:").size(10.0)),
                                );
                                // Reserve 44px for the '+90°' button so this row NEVER overflows panel width!
                                let spin_w = (ui.available_width() - 44.0).max(66.0);
                                let prev_r = cfg.vehicle_pool[idx].rot_y_offset_deg;
                                numeric_spinner_w(
                                    ui,
                                    ("tr_v_roty", a.id, idx),
                                    &mut cfg.vehicle_pool[idx].rot_y_offset_deg,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 0.0,
                                        max: 360.0,
                                        step: 90.0,
                                        drag_speed: 1.0,
                                        decimals: 0,
                                        suffix: "°",
                                        wrap_around: true,
                                    },
                                );
                                if (cfg.vehicle_pool[idx].rot_y_offset_deg - prev_r).abs() > 1e-6 {
                                    cfg_changed = true;
                                }
                                if ui.small_button("+90°").clicked() {
                                    cfg.vehicle_pool[idx].rot_y_offset_deg =
                                        (cfg.vehicle_pool[idx].rot_y_offset_deg + 90.0) % 360.0;
                                    cfg_changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Bobot:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 2.0).max(66.0);
                                let mut w_f64 = cfg.vehicle_pool[idx].weight as f64;
                                numeric_spinner_w(
                                    ui,
                                    ("tr_v_w", a.id, idx),
                                    &mut w_f64,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 0.1,
                                        max: 20.0,
                                        step: 0.5,
                                        drag_speed: 0.1,
                                        decimals: 1,
                                        suffix: "",
                                        wrap_around: false,
                                    },
                                );
                                if (w_f64 as f32 - cfg.vehicle_pool[idx].weight).abs() > 1e-4 {
                                    cfg.vehicle_pool[idx].weight = w_f64 as f32;
                                    cfg_changed = true;
                                }
                            });
                        });
                    });
                ui.add_space(3.0);
            }

            if let Some(idx) = focus_gizmo_idx
                && let Some(ref mut preview) = state.traffic_scale_preview
                && preview.anim_id == a.id
            {
                preview.active_gizmo_pool_idx = idx;
                let vname = cfg
                    .vehicle_pool
                    .get(idx)
                    .map(|v| v.name.as_str())
                    .unwrap_or("Model 3D");
                state.status_banner = Some(format!(
                    "Scale Gizmo dipindahkan ke '{vname}' — tarik sumbu/titik tengah di peta untuk mengubah skala."
                ));
            }

            if let Some(idx) = toggle_preview_idx {
                let already_present = state
                    .traffic_scale_preview
                    .as_ref()
                    .filter(|p| p.anim_id == a.id)
                    .is_some_and(|p| p.pool_indices.contains(&idx));
                if already_present {
                    if let Some(ref mut preview) = state.traffic_scale_preview {
                        preview.pool_indices.retain(|&i| i != idx);
                        if preview.pool_indices.is_empty() {
                            state.traffic_scale_preview = None;
                        } else if preview.active_gizmo_pool_idx == idx {
                            preview.active_gizmo_pool_idx = preview.pool_indices[0];
                        }
                    }
                } else {
                    route_edit.clear();
                    playback.stop_editor_preview();
                    state.tool_mode = ToolMode::Select;
                    let mut indices = state
                        .traffic_scale_preview
                        .as_ref()
                        .filter(|p| p.anim_id == a.id)
                        .map(|p| p.pool_indices.clone())
                        .unwrap_or_default();
                    if indices.len() >= 2 {
                        // Max 2 preview models simultaneously: keep the most recent one and replace the oldest
                        indices.remove(0);
                    }
                    indices.push(idx);
                    let total_shown = indices.len();
                    state.traffic_scale_preview = Some(TrafficScalePreviewState {
                        anim_id: a.id,
                        pool_indices: indices,
                        active_gizmo_pool_idx: idx,
                    });
                    // Center camera on the preview pair along the lane start
                    if let Some(lane) = cfg
                        .lanes
                        .iter()
                        .find(|l| l.enabled && l.geom.len() >= 2)
                        .or_else(|| cfg.lanes.iter().find(|l| l.geom.len() >= 2))
                    {
                        let p0 = state.geo_anchor.lng_lat_alt_to_local(
                            lane.geom[0][0],
                            lane.geom[0][1],
                            0.0,
                        );
                        let p1 = state.geo_anchor.lng_lat_alt_to_local(
                            lane.geom[1][0],
                            lane.geom[1][1],
                            0.0,
                        );
                        let v0 = bevy::math::Vec2::new(p0[0], p0[2]);
                        let v1 = bevy::math::Vec2::new(p1[0], p1[2]);
                        let seg = v1 - v0;
                        let seg_len = seg.length().max(0.01);
                        let t = (10.0_f32.min(seg_len * 0.45) / seg_len).clamp(0.0, 1.0);
                        tcam.target_local = v0.lerp(v1, t);
                    }
                    let vname = cfg
                        .vehicle_pool
                        .get(idx)
                        .map(|v| v.name.as_str())
                        .unwrap_or("Model 3D");
                    state.status_banner = Some(if total_shown == 2 {
                        format!(
                            "Preview 2/2 aktif (depan-belakang): Scale Gizmo pada '{vname}' — bandingkan ukurannya langsung di peta!"
                        )
                    } else {
                        format!(
                            "Preview '{vname}' aktif (1/2) — klik 'Preview Bandingkan' pada model lain untuk melihat 2 mobil berjejer!"
                        )
                    });
                }
            }

            if let Some(idx) = remove_pool_idx {
                cfg.vehicle_pool.remove(idx);
                if let Some(ref mut preview) = state.traffic_scale_preview
                    && preview.anim_id == a.id
                {
                    preview.pool_indices.retain(|&i| i != idx);
                    for pi in &mut preview.pool_indices {
                        if *pi > idx {
                            *pi -= 1;
                        }
                    }
                    if preview.pool_indices.is_empty() {
                        state.traffic_scale_preview = None;
                    } else {
                        if preview.active_gizmo_pool_idx == idx {
                            preview.active_gizmo_pool_idx = preview.pool_indices[0];
                        } else if preview.active_gizmo_pool_idx > idx {
                            preview.active_gizmo_pool_idx -= 1;
                        }
                    }
                }
                cfg_changed = true;
            }

            if editable {
                ui.add_space(5.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Tambah dari Semua Katalog 3D ({} total):",
                        state.catalog.len()
                    ))
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(186, 230, 253)),
                );
                ui.add_space(3.0);

                // Collect all distinct categories dynamically from the full 3D catalog
                let mut cat_counts: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for m in &state.catalog {
                    *cat_counts.entry(m.category.clone()).or_insert(0) += 1;
                }

                let selected_cat_label = match &state.traffic_pool_category_filter {
                    None => format!("Semua Kategori ({})", state.catalog.len()),
                    Some(c) => {
                        let cnt = cat_counts.get(c).copied().unwrap_or(0);
                        format!("{c} ({cnt})")
                    }
                };
                egui::ComboBox::from_id_salt(("tr_cat_filter", a.id))
                    .width((ui.available_width() - 4.0).max(120.0))
                    .selected_text(selected_cat_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.traffic_pool_category_filter,
                            None,
                            format!("Semua Kategori ({})", state.catalog.len()),
                        );
                        for (cat, cnt) in &cat_counts {
                            ui.selectable_value(
                                &mut state.traffic_pool_category_filter,
                                Some(cat.clone()),
                                format!("{cat} ({cnt})"),
                            );
                        }
                    });
                ui.add_space(3.0);

                ui.horizontal(|ui| {
                    let has_q = !state.traffic_pool_search.is_empty();
                    let search_w =
                        (ui.available_width() - if has_q { 52.0 } else { 4.0 }).max(80.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.traffic_pool_search)
                            .hint_text("Cari nama / kategori model 3D...")
                            .desired_width(search_w),
                    );
                    if has_q && ui.small_button("Reset").clicked() {
                        state.traffic_pool_search.clear();
                    }
                });
                ui.add_space(3.0);

                let q = state.traffic_pool_search.trim().to_lowercase();
                let cat_filter = state.traffic_pool_category_filter.clone();
                let candidates: Vec<_> = state
                    .catalog
                    .iter()
                    .filter(|m| {
                        let already = cfg
                            .vehicle_pool
                            .iter()
                            .any(|v| v.model_path == m.model_path);
                        if already {
                            return false;
                        }
                        if let Some(ref cf) = cat_filter
                            && m.category != *cf
                        {
                            return false;
                        }
                        if q.is_empty() {
                            true
                        } else {
                            m.name.to_lowercase().contains(&q)
                                || m.category.to_lowercase().contains(&q)
                                || m.model_path.to_lowercase().contains(&q)
                        }
                    })
                    .cloned()
                    .collect();

                if cat_filter.is_some() && !candidates.is_empty() && candidates.len() <= 25 {
                    let add_all_btn = egui::Button::new(
                        egui::RichText::new(format!(
                            "+ Tambah Semua ({} model)",
                            candidates.len()
                        ))
                        .size(10.0)
                        .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .small();
                    if ui.add(add_all_btn).clicked() {
                        for m in &candidates {
                            cfg.vehicle_pool.push(TrafficVehicleEntry {
                                model_path: m.model_path.clone(),
                                name: m.name.clone(),
                                scale: 1.0,
                                length_m: TrafficVehicleEntry::estimate_length_m(
                                    &m.name,
                                    &m.model_path,
                                ),
                                weight: 1.0,
                                rot_y_offset_deg: m.default_rotation_deg[1],
                            });
                        }
                        cfg_changed = true;
                    }
                    ui.add_space(3.0);
                }

                egui::ScrollArea::vertical()
                    .id_salt(("tr_cat_scroll", a.id))
                    .max_height(160.0)
                    .show(ui, |ui| {
                        if candidates.is_empty() {
                            ui.label(
                                egui::RichText::new("Tidak ada model tambahan yang cocok.")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        } else {
                            for m in &candidates {
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button(
                                            egui::RichText::new("+")
                                                .strong()
                                                .color(egui::Color32::from_rgb(110, 231, 183)),
                                        )
                                        .clicked()
                                    {
                                        cfg.vehicle_pool.push(TrafficVehicleEntry {
                                            model_path: m.model_path.clone(),
                                            name: m.name.clone(),
                                            scale: 1.0,
                                            length_m: TrafficVehicleEntry::estimate_length_m(
                                                &m.name,
                                                &m.model_path,
                                            ),
                                            weight: 1.0,
                                            rot_y_offset_deg: m.default_rotation_deg[1],
                                        });
                                        cfg_changed = true;
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!("{} ({})", m.name, m.category))
                                                .size(10.5)
                                                .color(egui::Color32::LIGHT_GRAY),
                                        )
                                        .truncate(),
                                    );
                                });
                            }
                        }
                    });
            }
        },
    );

    // 4. Speed, Spawn Density, Overtaking & Safety Controls
    inspector_card(ui, "Kecepatan, Kepadatan & Menyalip", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Max / Jalur:");
                let mut v = cfg.max_vehicles_per_lane as f64;
                numeric_spinner(
                    ui,
                    ("tr_max_lane", a.id),
                    &mut v,
                    SpinnerConfig {
                        min: 1.0,
                        max: 25.0,
                        step: 1.0,
                        drag_speed: 0.2,
                        decimals: 0,
                        suffix: " unit",
                        wrap_around: false,
                    },
                );
                if v.round() as u32 != cfg.max_vehicles_per_lane {
                    cfg.max_vehicles_per_lane = v.round() as u32;
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Max Total Pool:");
                let mut v = cfg.max_total_vehicles as f64;
                numeric_spinner(
                    ui,
                    ("tr_max_tot", a.id),
                    &mut v,
                    SpinnerConfig {
                        min: 1.0,
                        max: 100.0,
                        step: 2.0,
                        drag_speed: 0.5,
                        decimals: 0,
                        suffix: " unit",
                        wrap_around: false,
                    },
                );
                if v.round() as u32 != cfg.max_total_vehicles {
                    cfg.max_total_vehicles = v.round() as u32;
                    cfg_changed = true;
                }
            });
            ui.add_space(3.0);

            ui.horizontal(|ui| {
                ui.label("Speed Min:");
                let prev = cfg.min_speed_mps;
                numeric_spinner(
                    ui,
                    ("tr_spd_min", a.id),
                    &mut cfg.min_speed_mps,
                    SpinnerConfig {
                        min: 1.0,
                        max: 45.0,
                        step: 0.5,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
                if (cfg.min_speed_mps - prev).abs() > 1e-6 {
                    if cfg.max_speed_mps < cfg.min_speed_mps {
                        cfg.max_speed_mps = cfg.min_speed_mps;
                    }
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Speed Max:");
                let prev = cfg.max_speed_mps;
                numeric_spinner(
                    ui,
                    ("tr_spd_max", a.id),
                    &mut cfg.max_speed_mps,
                    SpinnerConfig {
                        min: 1.0,
                        max: 55.0,
                        step: 0.5,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
                if (cfg.max_speed_mps - prev).abs() > 1e-6 {
                    if cfg.min_speed_mps > cfg.max_speed_mps {
                        cfg.min_speed_mps = cfg.max_speed_mps;
                    }
                    cfg_changed = true;
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "Rentang acak: {:.0} – {:.0} km/jam",
                    cfg.min_speed_mps * 3.6,
                    cfg.max_speed_mps * 3.6
                ))
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(3.0);

            ui.horizontal(|ui| {
                ui.label("Jeda Spawn Min:");
                let prev = cfg.spawn_interval_min_sec;
                numeric_spinner(
                    ui,
                    ("tr_spw_min", a.id),
                    &mut cfg.spawn_interval_min_sec,
                    SpinnerConfig {
                        min: 0.4,
                        max: 30.0,
                        step: 0.2,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " s",
                        wrap_around: false,
                    },
                );
                if (cfg.spawn_interval_min_sec - prev).abs() > 1e-6 {
                    if cfg.spawn_interval_max_sec < cfg.spawn_interval_min_sec {
                        cfg.spawn_interval_max_sec = cfg.spawn_interval_min_sec;
                    }
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Jeda Spawn Max:");
                let prev = cfg.spawn_interval_max_sec;
                numeric_spinner(
                    ui,
                    ("tr_spw_max", a.id),
                    &mut cfg.spawn_interval_max_sec,
                    SpinnerConfig {
                        min: 0.5,
                        max: 60.0,
                        step: 0.2,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " s",
                        wrap_around: false,
                    },
                );
                if (cfg.spawn_interval_max_sec - prev).abs() > 1e-6 {
                    if cfg.spawn_interval_min_sec > cfg.spawn_interval_max_sec {
                        cfg.spawn_interval_min_sec = cfg.spawn_interval_max_sec;
                    }
                    cfg_changed = true;
                }
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Lebar Lajur:");
                let prev = cfg.lane_width_m;
                numeric_spinner(
                    ui,
                    ("tr_lane_w", a.id),
                    &mut cfg.lane_width_m,
                    SpinnerConfig {
                        min: 0.5,
                        max: 100_000.0,
                        step: 0.5,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
                if (cfg.lane_width_m - prev).abs() > 1e-6 {
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Jarak Aman:");
                let prev = cfg.min_safe_gap_m;
                numeric_spinner(
                    ui,
                    ("tr_gap", a.id),
                    &mut cfg.min_safe_gap_m,
                    SpinnerConfig {
                        min: 2.0,
                        max: 20.0,
                        step: 0.5,
                        drag_speed: 0.2,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
                if (cfg.min_safe_gap_m - prev).abs() > 1e-6 {
                    cfg_changed = true;
                }
            });
            ui.add_space(4.0);

            if ui
                .checkbox(
                    &mut cfg.allow_overtaking,
                    "Aktifkan Menyalip (cek arus berlawanan)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if cfg.allow_overtaking {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sisi Salip:");
                    for &side in OvertakeSide::ALL {
                        if ui
                            .selectable_label(cfg.overtake_side == side, side.label())
                            .clicked()
                            && cfg.overtake_side != side
                        {
                            cfg.overtake_side = side;
                            cfg_changed = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Rem Depan Disalip:");
                    let prev = cfg.front_yield_slowdown_pct;
                    numeric_spinner(
                        ui,
                        ("tr_yield_pct", a.id),
                        &mut cfg.front_yield_slowdown_pct,
                        SpinnerConfig {
                            min: 0.0,
                            max: 50.0,
                            step: 5.0,
                            drag_speed: 1.0,
                            decimals: 0,
                            suffix: " %",
                            wrap_around: false,
                        },
                    );
                    if (cfg.front_yield_slowdown_pct - prev).abs() > 1e-6 {
                        cfg_changed = true;
                    }
                });
            }
            if ui
                .checkbox(
                    &mut cfg.yield_at_intersections,
                    "Beri jalan di persimpangan antar jalur",
                )
                .changed()
            {
                cfg_changed = true;
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Seed Sync: #{}", cfg.seed % 100_000))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if ui
                    .small_button("Acak Pola")
                    .on_hover_text(
                        "Ubah seed acak deterministik (tetap sinkron identik dengan Layar Besar)",
                    )
                    .clicked()
                {
                    cfg.seed = cfg.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    cfg_changed = true;
                }
            });
        });
    });

    if cfg_changed {
        write_traffic_anim_config(a, &cfg);
    }
}

fn draw_traffic_queue_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    editable: bool,
) {
    let mut cfg = parse_traffic_queue_config(a);
    let mut cfg_changed = false;
    let is_gridlock = is_traffic_gridlock_anim(a);
    let is_manual = is_traffic_manual_anim(a);

    // 1. General Info & Validation Summary
    let card1_title = if is_gridlock {
        "Macet Total & Urai"
    } else if is_manual {
        "Kontrol Simpang Manual"
    } else {
        "Antrean & Urai Simpang"
    };
    inspector_card(ui, card1_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
        });
        ui.add_space(4.0);
        let incomplete = cfg.incomplete_corridor_names();
        if !incomplete.is_empty() {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(76, 29, 29))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(248, 113, 113)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(format!(
                            "Wajib Jalur Lancar: {} belum punya Jalur Lancar! Klik '+ Gambar Jalur Lancar' di bawah.",
                            incomplete.join(", ")
                        ))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(254, 202, 202)),
                    );
                });
        } else if cfg.has_playable_setup() {
            let ready_msg = if is_gridlock {
                "Siap dijalankan! Semua jalur tertahan macet total (diam) sampai tombol 'Lancar' diklik untuk mengurai seluruh kendaraan hingga habis."
            } else if is_manual {
                "Siap dijalankan! Saat Play/Preview Anda bisa kendalikan Buka Jalur A/B/C & Tahan Semua, lalu atur penguraian manual lewat tombol Lancar Jalur A/B/C."
            } else {
                "Siap dijalankan! Kendaraan bergerak konvoi bergantian per durasi; klik 'Lancar' saat Play/Preview untuk mengurai seluruh kendaraan sampai habis."
            };
            ui.label(
                egui::RichText::new(ready_msg)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(153, 246, 228)),
            );
        }
    });

    // 2. Paired Corridors (Jalur Antre + Mandatory Jalur Lancar)
    let complete_cnt = cfg.corridors.iter().filter(|c| c.is_complete()).count();
    inspector_card(
        ui,
        &format!(
            "Pasangan Jalur Antre & Lancar ({complete_cnt}/{})",
            cfg.corridors.len()
        ),
        true,
        |ui| {
            ui.label(
                egui::RichText::new(
                    "Setiap Jalur Antre (oranye) wajib memiliki 1 Jalur Lancar (hijau). Garis merah di ujung Jalur Antre adalah batas berhenti saat giliran jalur lain.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            if editable {
                let add_btn = egui::Button::new(
                    egui::RichText::new("+ Tambah Jalur Antre Baru di Peta")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(217, 119, 6))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(add_btn).clicked() {
                    let next_label =
                        TrafficQueueCorridor::default_label_for_index(cfg.corridors.len());
                    route_edit.clear();
                    state.traffic_scale_preview = None;
                    state.tool_mode = ToolMode::DrawTrafficQueueLane;
                    state.drawing_traffic_queue_target = Some((a.id, None));
                    state.draft_points_lnglat.clear();
                    state.status_banner = Some(format!(
                        "Klik titik-titik Jalur Antre '{next_label}' di peta (min 2 titik), tekan Enter / Finish."
                    ));
                }
                ui.add_space(5.0);
            }

            let mut delete_cor_idx: Option<usize> = None;
            let mut reverse_queue_idx: Option<usize> = None;
            let mut reverse_clear_idx: Option<usize> = None;
            let mut draw_clear_for_idx: Option<usize> = None;
            let mut toggle_edit_encoded: Option<usize> = None;

            for idx in 0..cfg.corridors.len() {
                let is_complete = cfg.corridors[idx].is_complete();
                let q_len_m = route_total_length_m(&cfg.corridors[idx].queue_geom);
                let q_pts = cfg.corridors[idx].queue_geom.len();
                let cl_len_m = route_total_length_m(&cfg.corridors[idx].clear_geom);
                let cl_pts = cfg.corridors[idx].clear_geom.len();
                let q_encoded = idx * 2;
                let cl_encoded = idx * 2 + 1;
                let editing_q = route_edit.is_editing(a.id)
                    && route_edit.traffic_lane_idx == Some(q_encoded);
                let editing_cl = route_edit.is_editing(a.id)
                    && route_edit.traffic_lane_idx == Some(cl_encoded);

                egui::Frame::new()
                    .fill(if editing_q || editing_cl {
                        egui::Color32::from_rgb(10, 44, 48)
                    } else {
                        egui::Color32::from_rgb(15, 23, 42)
                    })
                    .stroke(egui::Stroke::new(
                        if editing_q || editing_cl || !is_complete {
                            1.25
                        } else {
                            1.0
                        },
                        if editing_q || editing_cl {
                            egui::Color32::from_rgb(34, 211, 238)
                        } else if !is_complete {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            egui::Color32::from_rgb(44, 62, 92)
                        },
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // Header row: checkbox + name + status badge
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut cfg.corridors[idx].enabled, "").changed() {
                                cfg_changed = true;
                            }
                            let name_w = (ui.available_width() - 96.0).clamp(64.0, 150.0);
                            if ui
                                .add_enabled(
                                    editable,
                                    egui::TextEdit::singleline(&mut cfg.corridors[idx].name)
                                        .desired_width(name_w),
                                )
                                .changed()
                            {
                                cfg_changed = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let (tag, bg, fg) = if is_complete {
                                        (
                                            "Lengkap",
                                            egui::Color32::from_rgb(6, 78, 59),
                                            egui::Color32::from_rgb(110, 231, 183),
                                        )
                                    } else {
                                        (
                                            "Belum Lancar",
                                            egui::Color32::from_rgb(120, 53, 15),
                                            egui::Color32::from_rgb(253, 230, 138),
                                        )
                                    };
                                    egui::Frame::new()
                                        .fill(bg)
                                        .corner_radius(3.0)
                                        .inner_margin(egui::Margin::symmetric(5, 1))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(tag)
                                                    .size(9.0)
                                                    .strong()
                                                    .color(fg),
                                            );
                                        });
                                },
                            );
                        });

                        ui.add_space(3.0);
                        // Sub-row 1: Jalur Antre
                        ui.label(
                            egui::RichText::new(format!(
                                "Jalur Antre (Oranye): {q_len_m:.0}m ({q_pts} pt)"
                            ))
                            .size(10.0)
                            .color(egui::Color32::from_rgb(253, 186, 116)),
                        );
                        if editable {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let edit_q_btn = egui::Button::new(
                                    egui::RichText::new(if editing_q {
                                        "Selesai Edit"
                                    } else {
                                        "Edit Antre"
                                    })
                                    .size(9.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                                )
                                .fill(if editing_q {
                                    egui::Color32::from_rgb(5, 150, 105)
                                } else {
                                    egui::Color32::from_rgb(37, 99, 235)
                                })
                                .small();
                                if ui.add(edit_q_btn).clicked() {
                                    toggle_edit_encoded = Some(q_encoded);
                                }
                                if ui
                                    .small_button("Putar")
                                    .on_hover_text("Balik arah Jalur Antre")
                                    .clicked()
                                {
                                    reverse_queue_idx = Some(idx);
                                }
                                if cfg.corridors.len() > 1
                                    && ui
                                        .small_button(
                                            egui::RichText::new("Hapus")
                                                .color(egui::Color32::from_rgb(248, 113, 113)),
                                        )
                                        .clicked()
                                {
                                    delete_cor_idx = Some(idx);
                                }
                            });
                        }

                        ui.add_space(4.0);
                        // Sub-row 2: Jalur Lancar (Mandatory!)
                        if cl_pts >= 2 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Jalur Lancar (Hijau): {cl_len_m:.0}m ({cl_pts} pt)"
                                ))
                                .size(10.0)
                                .color(egui::Color32::from_rgb(110, 231, 183)),
                            );
                            if editable {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    let edit_cl_btn = egui::Button::new(
                                        egui::RichText::new(if editing_cl {
                                            "Selesai Edit"
                                        } else {
                                            "Edit Lancar"
                                        })
                                        .size(9.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if editing_cl {
                                        egui::Color32::from_rgb(5, 150, 105)
                                    } else {
                                        egui::Color32::from_rgb(13, 148, 136)
                                    })
                                    .small();
                                    if ui.add(edit_cl_btn).clicked() {
                                        toggle_edit_encoded = Some(cl_encoded);
                                    }
                                    if ui
                                        .small_button("Gambar Ulang")
                                        .on_hover_text(
                                            "Gambar ulang Jalur Lancar dari ujung Jalur Antre",
                                        )
                                        .clicked()
                                    {
                                        draw_clear_for_idx = Some(idx);
                                    }
                                    if ui
                                        .small_button("Putar")
                                        .on_hover_text("Balik arah Jalur Lancar")
                                        .clicked()
                                    {
                                        reverse_clear_idx = Some(idx);
                                    }
                                });
                            }
                        } else if editable {
                            let draw_cl_btn = egui::Button::new(
                                egui::RichText::new("+ Gambar Jalur Lancar (Wajib)")
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105))
                            .min_size(egui::vec2(ui.available_width(), 24.0));
                            if ui.add(draw_cl_btn).clicked() {
                                draw_clear_for_idx = Some(idx);
                            }
                        }
                    });
                ui.add_space(3.0);
            }

            if let Some(enc) = toggle_edit_encoded {
                if route_edit.is_editing(a.id) && route_edit.traffic_lane_idx == Some(enc) {
                    route_edit.clear();
                    state.status_banner = Some("Mode Edit Jalur selesai.".into());
                } else {
                    state.traffic_scale_preview = None;
                    playback.stop_editor_preview();
                    state.tool_mode = ToolMode::Select;
                    route_edit.editing_anim_id = Some(a.id);
                    route_edit.traffic_lane_idx = Some(enc);
                    route_edit.selected_waypoint = None;
                    route_edit.add_point_on_map_mode = false;
                    route_edit.active_drag = None;
                    let c_idx = enc / 2;
                    let kind = if enc % 2 == 1 {
                        "Jalur Lancar"
                    } else {
                        "Jalur Antre"
                    };
                    let cname = cfg
                        .corridors
                        .get(c_idx)
                        .map(|c| c.name.as_str())
                        .unwrap_or("Jalur");
                    state.status_banner = Some(format!(
                        "Edit {kind} '{cname}' aktif — geser titik di peta, klik (+) untuk tambah titik, Delete untuk hapus titik."
                    ));
                }
            }
            if let Some(c_idx) = draw_clear_for_idx
                && let Some(cor) = cfg.corridors.get(c_idx)
            {
                let cname = cor.name.clone();
                route_edit.clear();
                state.traffic_scale_preview = None;
                playback.stop_editor_preview();
                state.tool_mode = ToolMode::DrawTrafficQueueLane;
                state.drawing_traffic_queue_target = Some((a.id, Some(c_idx)));
                state.draft_points_lnglat.clear();
                if let Some(&end_pt) = cor.queue_geom.last() {
                    state.draft_points_lnglat.push([end_pt[0], end_pt[1]]);
                }
                state.status_banner = Some(format!(
                    "Gambar Jalur Lancar untuk '{cname}': titik awal sudah menempel di ujung Jalur Antre — klik lanjutan arah keluar di peta lalu Enter / Finish."
                ));
            }
            if let Some(idx) = reverse_queue_idx {
                cfg.corridors[idx].queue_geom.reverse();
                cfg_changed = true;
            }
            if let Some(idx) = reverse_clear_idx {
                cfg.corridors[idx].clear_geom.reverse();
                cfg_changed = true;
            }
            if let Some(idx) = delete_cor_idx {
                cfg.corridors.remove(idx);
                if route_edit.editing_anim_id == Some(a.id) {
                    route_edit.clear();
                }
                cfg_changed = true;
            }
        },
    );

    // 3. Shared Vehicle 3D Pool Picker (All 3D Catalog + Category Filter + 2-Model Preview & Scale Gizmo!)
    let preview_lane_geom: Option<Vec<[f64; 3]>> = cfg
        .corridors
        .iter()
        .find(|c| c.enabled && c.queue_geom.len() >= 2)
        .or_else(|| cfg.corridors.iter().find(|c| c.queue_geom.len() >= 2))
        .map(|c| c.queue_geom.clone());
    if draw_traffic_queue_vehicle_pool_card(
        ui,
        state,
        tcam,
        playback,
        route_edit,
        a.id,
        &mut cfg.vehicle_pool,
        preview_lane_geom.as_deref(),
        editable,
    ) {
        cfg_changed = true;
    }

    // 4. Convoy Speed, Turn Duration & Live "Lancar" Speed Settings
    let card4_title = if is_gridlock {
        "Pengaturan Macet Total & Mode Lancar"
    } else if is_manual {
        "Pengaturan Kontrol Simpang Manual"
    } else {
        "Pengaturan Konvoi, Giliran & Mode Lancar"
    };
    inspector_card(ui, card4_title, true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if is_gridlock || is_manual {
                ui.label(
                    egui::RichText::new("Kondisi Awal Saat Play:")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(253, 164, 175)),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui
                        .selectable_value(
                            &mut cfg.prefill_initial_queue,
                            true,
                            egui::RichText::new("Pre-fill (Penuh & Diam)").size(10.0),
                        )
                        .on_hover_text(
                            "Semua jalur langsung penuh kendaraan dari garis akhir sampai garis awal dalam posisi tertahan hingga Anda membuka jalur / klik 'Lancar'",
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut cfg.prefill_initial_queue,
                            false,
                            egui::RichText::new(if is_manual {
                                "Mengalir & Antre"
                            } else {
                                "Mengalir Lalu Macet"
                            })
                            .size(10.0),
                        )
                        .on_hover_text(
                            "Kendaraan bergerak masuk dari pangkal tiap jalur hingga garis akhir lalu berhenti antre",
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });
                ui.add_space(4.0);
            }

            if !is_gridlock || !cfg.prefill_initial_queue {
                ui.horizontal(|ui| {
                    ui.label(if is_gridlock {
                        "Kecepatan Masuk:"
                    } else {
                        "Kecepatan Konvoi:"
                    });
                    let prev = cfg.convoy_speed_mps;
                    numeric_spinner(
                        ui,
                        ("tq_convoy_spd", a.id),
                        &mut cfg.convoy_speed_mps,
                        SpinnerConfig {
                            min: 1.5,
                            max: 40.0,
                            step: 0.5,
                            drag_speed: 0.2,
                            decimals: 1,
                            suffix: " m/s",
                            wrap_around: false,
                        },
                    );
                    if (cfg.convoy_speed_mps - prev).abs() > 1e-6 {
                        if cfg.clear_speed_mps < cfg.convoy_speed_mps {
                            cfg.clear_speed_mps = cfg.convoy_speed_mps;
                        }
                        cfg_changed = true;
                    }
                });
                ui.label(
                    egui::RichText::new(format!(
                        "Konstan tanpa menyalip (~{:.0} km/jam)",
                        cfg.convoy_speed_mps * 3.6
                    ))
                    .size(9.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(3.0);
            }

            ui.horizontal(|ui| {
                ui.label("Kecepatan Lancar:");
                let prev = cfg.clear_speed_mps;
                numeric_spinner(
                    ui,
                    ("tq_clear_spd", a.id),
                    &mut cfg.clear_speed_mps,
                    SpinnerConfig {
                        min: 2.0,
                        max: 60.0,
                        step: 0.5,
                        drag_speed: 0.25,
                        decimals: 1,
                        suffix: " m/s",
                        wrap_around: false,
                    },
                );
                if (cfg.clear_speed_mps - prev).abs() > 1e-6 {
                    cfg_changed = true;
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "Kecepatan saat tombol '{}' diklik (~{:.0} km/jam)",
                    if is_manual { "Lancar Jalur" } else { "Lancar" },
                    cfg.clear_speed_mps * 3.6
                ))
                .size(9.5)
                .color(egui::Color32::from_rgb(110, 231, 183)),
            );
            ui.add_space(4.0);

            if !is_manual {
                ui.label(
                    egui::RichText::new("Mode Urai (Saat Tombol Lancar Diklik):")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(186, 230, 253)),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui
                        .selectable_value(
                            &mut cfg.clearance_mode,
                            QueueClearanceMode::DrainCorridor,
                            egui::RichText::new(QueueClearanceMode::DrainCorridor.label())
                                .size(10.0),
                        )
                        .on_hover_text(
                            "Kuras habis seluruh sisa antrean jalur aktif dulu sampai kosong, baru pindah ke jalur berikutnya",
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut cfg.clearance_mode,
                            QueueClearanceMode::AlternatingTimed,
                            egui::RichText::new(QueueClearanceMode::AlternatingTimed.label())
                                .size(10.0),
                        )
                        .on_hover_text(
                            "Tetap bergantian antar jalur sesuai Durasi Giliran dengan kecepatan Lancar (langsung pindah jika jalur sudah habis) sampai semua jalur 0 kendaraan",
                        )
                        .changed()
                    {
                        cfg_changed = true;
                    }
                });
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Durasi Giliran:");
                    let prev = cfg.turn_duration_sec;
                    numeric_spinner(
                        ui,
                        ("tq_turn_dur", a.id),
                        &mut cfg.turn_duration_sec,
                        SpinnerConfig {
                            min: 2.0,
                            max: 180.0,
                            step: 1.0,
                            drag_speed: 0.5,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                    if (cfg.turn_duration_sec - prev).abs() > 1e-6 {
                        cfg_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Jeda Pindah:");
                    let prev = cfg.switch_pause_sec;
                    numeric_spinner(
                        ui,
                        ("tq_sw_pause", a.id),
                        &mut cfg.switch_pause_sec,
                        SpinnerConfig {
                            min: 0.0,
                            max: 15.0,
                            step: 0.5,
                            drag_speed: 0.1,
                            decimals: 1,
                            suffix: " dtk",
                            wrap_around: false,
                        },
                    );
                    if (cfg.switch_pause_sec - prev).abs() > 1e-6 {
                        cfg_changed = true;
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Max Antre/Jalur:");
                let mut v = cfg.max_vehicles_per_corridor as f64;
                numeric_spinner(
                    ui,
                    ("tq_max_v", a.id),
                    &mut v,
                    SpinnerConfig {
                        min: 1.0,
                        max: 80.0,
                        step: 1.0,
                        drag_speed: 0.25,
                        decimals: 0,
                        suffix: " unit",
                        wrap_around: false,
                    },
                );
                let new_v = v.round().clamp(1.0, 80.0) as u32;
                if new_v != cfg.max_vehicles_per_corridor {
                    cfg.max_vehicles_per_corridor = new_v;
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Jarak Aman:");
                let prev = cfg.min_safe_gap_m;
                numeric_spinner(
                    ui,
                    ("tq_gap", a.id),
                    &mut cfg.min_safe_gap_m,
                    SpinnerConfig {
                        min: 0.8,
                        max: 25.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
                if (cfg.min_safe_gap_m - prev).abs() > 1e-6 {
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Jeda Spawn:");
                let prev = cfg.spawn_interval_sec;
                numeric_spinner(
                    ui,
                    ("tq_spw", a.id),
                    &mut cfg.spawn_interval_sec,
                    SpinnerConfig {
                        min: 0.3,
                        max: 20.0,
                        step: 0.2,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
                if (cfg.spawn_interval_sec - prev).abs() > 1e-6 {
                    cfg_changed = true;
                }
            });

            if !is_gridlock && !is_manual {
                ui.add_space(4.0);
                if ui
                    .checkbox(
                        &mut cfg.prefill_initial_queue,
                        "Mulai penuh antrean (Pre-fill)",
                    )
                    .on_hover_text(
                        "Jika aktif, seluruh Jalur Antre langsung terisi penuh dari garis akhir sampai garis awal saat Play ditekan, dan terus diisi penuh selama fase normal",
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Seed Sync: #{}", cfg.seed % 100_000))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if ui
                    .small_button("Acak Pola")
                    .on_hover_text(
                        "Ubah seed acak deterministik (tetap sinkron identik dengan Layar Besar)",
                    )
                    .clicked()
                {
                    cfg.seed = cfg
                        .seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    cfg_changed = true;
                }
            });
        });
    });

    if cfg_changed {
        write_traffic_queue_config(a, &cfg);
    }
}

fn draw_traffic_diversion_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    route_edit: &mut RouteEditState,
    a: &mut twg_shared::models::Anim,
    editable: bool,
) {
    let mut cfg = parse_traffic_diversion_config(a);
    let mut cfg_changed = false;

    // 1. General Info & Validation Summary
    inspector_card(ui, "Pengalihan Arus (Traffic Diversion)", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
        });
        ui.add_space(4.0);
        let incomplete = cfg.incomplete_corridor_labels();
        if !incomplete.is_empty() {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(76, 29, 29))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(248, 113, 113)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(format!(
                            "Belum lengkap: {}",
                            incomplete.join("; ")
                        ))
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(254, 202, 202)),
                    );
                });
        } else if cfg.has_playable_setup() {
            ui.label(
                egui::RichText::new(
                    "Siap dijalankan! Klik 'ALIHKAN ARUS' saat Play/Preview untuk membelokkan arus ke Jalur Alternatif, lalu 'NORMALKAN ARUS' untuk kembali ke Jalur Utama.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(125, 211, 252)),
            );
        }
    });

    // 2. Y-Junction Diversion Corridors (Pendekat + Utama + Alternatif)
    let complete_cnt = cfg.corridors.iter().filter(|c| c.is_complete()).count();
    inspector_card(
        ui,
        &format!(
            "Koridor Pengalihan Y-Junction ({complete_cnt}/{})",
            cfg.corridors.len()
        ),
        true,
        |ui| {
            ui.label(
                egui::RichText::new(
                    "Setiap koridor terdiri dari 3 jalur: Pendekat (biru) -> titik simpang -> Utama/Normal (hijau) & Alternatif/Alih (oranye).",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            if editable {
                let add_btn = egui::Button::new(
                    egui::RichText::new("+ Tambah Koridor Pengalihan Baru")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(2, 132, 199))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(add_btn).clicked() {
                    let next_num = cfg.corridors.len() + 1;
                    route_edit.clear();
                    state.traffic_scale_preview = None;
                    state.tool_mode = ToolMode::DrawTrafficDiversionLane;
                    state.drawing_diversion_target = Some((a.id, None));
                    state.draft_points_lnglat.clear();
                    state.status_banner = Some(format!(
                        "Klik titik Jalur Pendekat Koridor {next_num} hingga ke titik simpang (min 2 titik), tekan Enter / Finish."
                    ));
                }
                ui.add_space(5.0);
            }

            let mut delete_cor_idx: Option<usize> = None;
            let mut reverse_part: Option<(usize, u8)> = None;
            let mut draw_branch: Option<(usize, u8)> = None;
            let mut toggle_edit_encoded: Option<usize> = None;

            for idx in 0..cfg.corridors.len() {
                let is_complete = cfg.corridors[idx].is_complete();
                let app_len = route_total_length_m(&cfg.corridors[idx].approach_geom);
                let app_pts = cfg.corridors[idx].approach_geom.len();
                let main_len = route_total_length_m(&cfg.corridors[idx].main_geom);
                let main_pts = cfg.corridors[idx].main_geom.len();
                let div_len = route_total_length_m(&cfg.corridors[idx].divert_geom);
                let div_pts = cfg.corridors[idx].divert_geom.len();

                let enc_app = idx * 3;
                let enc_main = idx * 3 + 1;
                let enc_div = idx * 3 + 2;
                let ed_app = route_edit.is_editing(a.id) && route_edit.traffic_lane_idx == Some(enc_app);
                let ed_main = route_edit.is_editing(a.id) && route_edit.traffic_lane_idx == Some(enc_main);
                let ed_div = route_edit.is_editing(a.id) && route_edit.traffic_lane_idx == Some(enc_div);
                let any_ed = ed_app || ed_main || ed_div;

                egui::Frame::new()
                    .fill(if any_ed {
                        egui::Color32::from_rgb(10, 40, 56)
                    } else {
                        egui::Color32::from_rgb(15, 23, 42)
                    })
                    .stroke(egui::Stroke::new(
                        if any_ed || !is_complete { 1.25 } else { 1.0 },
                        if any_ed {
                            egui::Color32::from_rgb(34, 211, 238)
                        } else if !is_complete {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            egui::Color32::from_rgb(44, 62, 92)
                        },
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut cfg.corridors[idx].enabled, "").changed() {
                                cfg_changed = true;
                            }
                            let name_w = (ui.available_width() - 96.0).clamp(64.0, 150.0);
                            if ui
                                .add_enabled(
                                    editable,
                                    egui::TextEdit::singleline(&mut cfg.corridors[idx].name)
                                        .desired_width(name_w),
                                )
                                .changed()
                            {
                                cfg_changed = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let (tag, bg, fg) = if is_complete {
                                        (
                                            "Lengkap",
                                            egui::Color32::from_rgb(6, 78, 59),
                                            egui::Color32::from_rgb(110, 231, 183),
                                        )
                                    } else {
                                        (
                                            "Belum Lengkap",
                                            egui::Color32::from_rgb(120, 53, 15),
                                            egui::Color32::from_rgb(253, 230, 138),
                                        )
                                    };
                                    egui::Frame::new()
                                        .fill(bg)
                                        .corner_radius(3.0)
                                        .inner_margin(egui::Margin::symmetric(5, 1))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(tag)
                                                    .size(9.0)
                                                    .strong()
                                                    .color(fg),
                                            );
                                        });
                                },
                            );
                        });

                        ui.add_space(3.0);
                        // 1. Jalur Pendekat
                        ui.label(
                            egui::RichText::new(format!(
                                "1. Pendekat (Biru): {app_len:.0}m ({app_pts} pt)"
                            ))
                            .size(10.0)
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                        );
                        if editable {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let btn = egui::Button::new(
                                    egui::RichText::new(if ed_app {
                                        "Selesai Edit"
                                    } else {
                                        "Edit Pendekat"
                                    })
                                    .size(9.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                                )
                                .fill(if ed_app {
                                    egui::Color32::from_rgb(5, 150, 105)
                                } else {
                                    egui::Color32::from_rgb(37, 99, 235)
                                })
                                .small();
                                if ui.add(btn).clicked() {
                                    toggle_edit_encoded = Some(enc_app);
                                }
                                if ui.small_button("Putar").clicked() {
                                    reverse_part = Some((idx, 0));
                                }
                                if cfg.corridors.len() > 1
                                    && ui
                                        .small_button(
                                            egui::RichText::new("Hapus")
                                                .color(egui::Color32::from_rgb(248, 113, 113)),
                                        )
                                        .clicked()
                                {
                                    delete_cor_idx = Some(idx);
                                }
                            });
                        }

                        ui.add_space(3.0);
                        // 2. Jalur Utama (Normal)
                        if main_pts >= 2 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "2. Utama / Normal (Hijau): {main_len:.0}m ({main_pts} pt)"
                                ))
                                .size(10.0)
                                .color(egui::Color32::from_rgb(110, 231, 183)),
                            );
                            if editable {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    let btn = egui::Button::new(
                                        egui::RichText::new(if ed_main {
                                            "Selesai Edit"
                                        } else {
                                            "Edit Utama"
                                        })
                                        .size(9.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if ed_main {
                                        egui::Color32::from_rgb(5, 150, 105)
                                    } else {
                                        egui::Color32::from_rgb(13, 148, 136)
                                    })
                                    .small();
                                    if ui.add(btn).clicked() {
                                        toggle_edit_encoded = Some(enc_main);
                                    }
                                    if ui.small_button("Gambar Ulang").clicked() {
                                        draw_branch = Some((idx, 1));
                                    }
                                    if ui.small_button("Putar").clicked() {
                                        reverse_part = Some((idx, 1));
                                    }
                                });
                            }
                        } else if editable {
                            let btn = egui::Button::new(
                                egui::RichText::new("+ Gambar Jalur Utama / Normal (Wajib)")
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(5, 150, 105))
                            .min_size(egui::vec2(ui.available_width(), 24.0));
                            if ui.add(btn).clicked() {
                                draw_branch = Some((idx, 1));
                            }
                        }

                        ui.add_space(3.0);
                        // 3. Jalur Alternatif (Pengalihan)
                        if div_pts >= 2 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "3. Alternatif / Alih (Oranye): {div_len:.0}m ({div_pts} pt)"
                                ))
                                .size(10.0)
                                .color(egui::Color32::from_rgb(253, 186, 116)),
                            );
                            if editable {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    let btn = egui::Button::new(
                                        egui::RichText::new(if ed_div {
                                            "Selesai Edit"
                                        } else {
                                            "Edit Alternatif"
                                        })
                                        .size(9.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(if ed_div {
                                        egui::Color32::from_rgb(5, 150, 105)
                                    } else {
                                        egui::Color32::from_rgb(217, 119, 6)
                                    })
                                    .small();
                                    if ui.add(btn).clicked() {
                                        toggle_edit_encoded = Some(enc_div);
                                    }
                                    if ui.small_button("Gambar Ulang").clicked() {
                                        draw_branch = Some((idx, 2));
                                    }
                                    if ui.small_button("Putar").clicked() {
                                        reverse_part = Some((idx, 2));
                                    }
                                });
                            }
                        } else if editable {
                            let btn = egui::Button::new(
                                egui::RichText::new("+ Gambar Jalur Alternatif / Alih (Wajib)")
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(217, 119, 6))
                            .min_size(egui::vec2(ui.available_width(), 24.0));
                            if ui.add(btn).clicked() {
                                draw_branch = Some((idx, 2));
                            }
                        }
                    });
                ui.add_space(3.0);
            }

            if let Some(enc) = toggle_edit_encoded {
                if route_edit.is_editing(a.id) && route_edit.traffic_lane_idx == Some(enc) {
                    route_edit.clear();
                    state.status_banner = Some("Mode Edit Jalur selesai.".into());
                } else {
                    state.traffic_scale_preview = None;
                    playback.stop_editor_preview();
                    state.tool_mode = ToolMode::Select;
                    route_edit.editing_anim_id = Some(a.id);
                    route_edit.traffic_lane_idx = Some(enc);
                    route_edit.selected_waypoint = None;
                    route_edit.add_point_on_map_mode = false;
                    route_edit.active_drag = None;
                    state.status_banner = Some(
                        "Edit Jalur Pengalihan aktif — geser titik di peta, klik (+) tambah titik, Delete hapus titik."
                            .into(),
                    );
                }
            }
            if let Some((c_idx, branch_code)) = draw_branch
                && let Some(cor) = cfg.corridors.get(c_idx)
            {
                let cname = cor.name.clone();
                route_edit.clear();
                state.traffic_scale_preview = None;
                playback.stop_editor_preview();
                state.tool_mode = ToolMode::DrawTrafficDiversionLane;
                state.drawing_diversion_target = Some((a.id, Some((c_idx, branch_code))));
                state.draft_points_lnglat.clear();
                if let Some(&split_pt) = cor.approach_geom.last() {
                    state.draft_points_lnglat.push([split_pt[0], split_pt[1]]);
                }
                let lbl = if branch_code == 1 {
                    "Jalur Utama / Normal"
                } else {
                    "Jalur Alternatif / Pengalihan"
                };
                state.status_banner = Some(format!(
                    "Gambar {lbl} untuk '{cname}': titik awal menempel di titik simpang — klik arah lanjutan lalu Enter / Finish."
                ));
            }
            if let Some((idx, part)) = reverse_part {
                match part {
                    0 => cfg.corridors[idx].approach_geom.reverse(),
                    1 => cfg.corridors[idx].main_geom.reverse(),
                    _ => cfg.corridors[idx].divert_geom.reverse(),
                }
                cfg_changed = true;
            }
            if let Some(idx) = delete_cor_idx {
                cfg.corridors.remove(idx);
                if route_edit.editing_anim_id == Some(a.id) {
                    route_edit.clear();
                }
                cfg_changed = true;
            }
        },
    );

    // 3. Shared Vehicle 3D Pool Picker (All 3D Catalog + 2-Model Preview & Scale Gizmo!)
    let preview_lane_geom: Option<Vec<[f64; 3]>> = cfg
        .corridors
        .iter()
        .find(|c| c.enabled && c.approach_geom.len() >= 2)
        .or_else(|| cfg.corridors.iter().find(|c| c.approach_geom.len() >= 2))
        .map(|c| c.approach_geom.clone());
    if draw_traffic_queue_vehicle_pool_card(
        ui,
        state,
        tcam,
        playback,
        route_edit,
        a.id,
        &mut cfg.vehicle_pool,
        preview_lane_geom.as_deref(),
        editable,
    ) {
        cfg_changed = true;
    }

    // 4. Diversion Flow Style, Speed, Overtaking & Initial Branch Settings
    inspector_card(ui, "Pengaturan Gaya Arus & Pengalihan", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            ui.label(
                egui::RichText::new("Gaya Arus Lalu Lintas:")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui
                    .selectable_value(
                        &mut cfg.flow_mode,
                        DiversionFlowMode::Convoy,
                        egui::RichText::new(DiversionFlowMode::Convoy.label()).size(10.0),
                    )
                    .on_hover_text(
                        "Kecepatan seragam berbaris rapi tanpa menyalip (cocok untuk jalur pengalihan sempit / 1 lajur)",
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
                if ui
                    .selectable_value(
                        &mut cfg.flow_mode,
                        DiversionFlowMode::FreeFlow,
                        egui::RichText::new(DiversionFlowMode::FreeFlow.label()).size(10.0),
                    )
                    .on_hover_text(
                        "Seperti Simulasi Traffic: kecepatan acak per kendaraan, jeda spawn acak, dan bisa menyalip mobil lebih lambat",
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
            });
            ui.add_space(4.0);

            match cfg.flow_mode {
                DiversionFlowMode::Convoy => {
                    ui.horizontal(|ui| {
                        ui.label("Kecepatan Normal:");
                        let prev = cfg.normal_speed_mps;
                        numeric_spinner(
                            ui,
                            ("td_norm_spd", a.id),
                            &mut cfg.normal_speed_mps,
                            SpinnerConfig {
                                min: 1.5,
                                max: 45.0,
                                step: 0.5,
                                drag_speed: 0.2,
                                decimals: 1,
                                suffix: " m/s",
                                wrap_around: false,
                            },
                        );
                        if (cfg.normal_speed_mps - prev).abs() > 1e-6 {
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Kecepatan Alih:");
                        let prev = cfg.divert_speed_mps;
                        numeric_spinner(
                            ui,
                            ("td_div_spd", a.id),
                            &mut cfg.divert_speed_mps,
                            SpinnerConfig {
                                min: 1.5,
                                max: 45.0,
                                step: 0.5,
                                drag_speed: 0.2,
                                decimals: 1,
                                suffix: " m/s",
                                wrap_around: false,
                            },
                        );
                        if (cfg.divert_speed_mps - prev).abs() > 1e-6 {
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Jeda Spawn:");
                        let prev = cfg.spawn_interval_sec;
                        numeric_spinner(
                            ui,
                            ("td_spw", a.id),
                            &mut cfg.spawn_interval_sec,
                            SpinnerConfig {
                                min: 0.3,
                                max: 20.0,
                                step: 0.2,
                                drag_speed: 0.1,
                                decimals: 1,
                                suffix: " dtk",
                                wrap_around: false,
                            },
                        );
                        if (cfg.spawn_interval_sec - prev).abs() > 1e-6 {
                            cfg_changed = true;
                        }
                    });
                }
                DiversionFlowMode::FreeFlow => {
                    ui.horizontal(|ui| {
                        ui.label("Kecepatan Min:");
                        let prev = cfg.min_speed_mps;
                        numeric_spinner(
                            ui,
                            ("td_min_spd", a.id),
                            &mut cfg.min_speed_mps,
                            SpinnerConfig {
                                min: 1.5,
                                max: 45.0,
                                step: 0.5,
                                drag_speed: 0.2,
                                decimals: 1,
                                suffix: " m/s",
                                wrap_around: false,
                            },
                        );
                        if (cfg.min_speed_mps - prev).abs() > 1e-6 {
                            if cfg.max_speed_mps < cfg.min_speed_mps {
                                cfg.max_speed_mps = cfg.min_speed_mps;
                            }
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Kecepatan Max:");
                        let prev = cfg.max_speed_mps;
                        numeric_spinner(
                            ui,
                            ("td_max_spd", a.id),
                            &mut cfg.max_speed_mps,
                            SpinnerConfig {
                                min: 1.5,
                                max: 55.0,
                                step: 0.5,
                                drag_speed: 0.2,
                                decimals: 1,
                                suffix: " m/s",
                                wrap_around: false,
                            },
                        );
                        if (cfg.max_speed_mps - prev).abs() > 1e-6 {
                            if cfg.min_speed_mps > cfg.max_speed_mps {
                                cfg.min_speed_mps = cfg.max_speed_mps;
                            }
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Skala Speed Alih:");
                        let prev = cfg.divert_speed_pct;
                        numeric_spinner(
                            ui,
                            ("td_div_pct", a.id),
                            &mut cfg.divert_speed_pct,
                            SpinnerConfig {
                                min: 30.0,
                                max: 120.0,
                                step: 5.0,
                                drag_speed: 1.0,
                                decimals: 0,
                                suffix: " %",
                                wrap_around: false,
                            },
                        );
                        if (cfg.divert_speed_pct - prev).abs() > 1e-6 {
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Jeda Spawn Min:");
                        let prev = cfg.spawn_interval_min_sec;
                        numeric_spinner(
                            ui,
                            ("td_spw_min", a.id),
                            &mut cfg.spawn_interval_min_sec,
                            SpinnerConfig {
                                min: 0.3,
                                max: 30.0,
                                step: 0.2,
                                drag_speed: 0.1,
                                decimals: 1,
                                suffix: " dtk",
                                wrap_around: false,
                            },
                        );
                        if (cfg.spawn_interval_min_sec - prev).abs() > 1e-6 {
                            if cfg.spawn_interval_max_sec < cfg.spawn_interval_min_sec {
                                cfg.spawn_interval_max_sec = cfg.spawn_interval_min_sec;
                            }
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Jeda Spawn Max:");
                        let prev = cfg.spawn_interval_max_sec;
                        numeric_spinner(
                            ui,
                            ("td_spw_max", a.id),
                            &mut cfg.spawn_interval_max_sec,
                            SpinnerConfig {
                                min: 0.3,
                                max: 60.0,
                                step: 0.2,
                                drag_speed: 0.1,
                                decimals: 1,
                                suffix: " dtk",
                                wrap_around: false,
                            },
                        );
                        if (cfg.spawn_interval_max_sec - prev).abs() > 1e-6 {
                            if cfg.spawn_interval_min_sec > cfg.spawn_interval_max_sec {
                                cfg.spawn_interval_min_sec = cfg.spawn_interval_max_sec;
                            }
                            cfg_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Lebar Lajur:");
                        let prev = cfg.lane_width_m;
                        numeric_spinner(
                            ui,
                            ("td_lane_w", a.id),
                            &mut cfg.lane_width_m,
                            SpinnerConfig {
                                min: 0.5,
                                max: 100_000.0,
                                step: 0.5,
                                drag_speed: 0.2,
                                decimals: 1,
                                suffix: " m",
                                wrap_around: false,
                            },
                        );
                        if (cfg.lane_width_m - prev).abs() > 1e-6 {
                            cfg_changed = true;
                        }
                    });
                    if ui
                        .checkbox(&mut cfg.allow_overtaking, "Izinkan Menyalip (Overtaking)")
                        .changed()
                    {
                        cfg_changed = true;
                    }
                    if cfg.allow_overtaking {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Sisi Salip:");
                            for side in OvertakeSide::ALL {
                                if ui
                                    .selectable_value(
                                        &mut cfg.overtake_side,
                                        *side,
                                        side.label(),
                                    )
                                    .changed()
                                {
                                    cfg_changed = true;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Perlambatan Depan:");
                            let prev = cfg.front_yield_slowdown_pct;
                            numeric_spinner(
                                ui,
                                ("td_yield_pct", a.id),
                                &mut cfg.front_yield_slowdown_pct,
                                SpinnerConfig {
                                    min: 0.0,
                                    max: 60.0,
                                    step: 5.0,
                                    drag_speed: 0.5,
                                    decimals: 0,
                                    suffix: " %",
                                    wrap_around: false,
                                },
                            );
                            if (cfg.front_yield_slowdown_pct - prev).abs() > 1e-6 {
                                cfg_changed = true;
                            }
                        });
                    }
                }
            }

            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label("Max / Koridor:");
                let mut v = cfg.max_vehicles_per_corridor as f64;
                numeric_spinner(
                    ui,
                    ("td_max_v", a.id),
                    &mut v,
                    SpinnerConfig {
                        min: 1.0,
                        max: 60.0,
                        step: 1.0,
                        drag_speed: 0.25,
                        decimals: 0,
                        suffix: " unit",
                        wrap_around: false,
                    },
                );
                let new_v = v.round().clamp(1.0, 60.0) as u32;
                if new_v != cfg.max_vehicles_per_corridor {
                    cfg.max_vehicles_per_corridor = new_v;
                    cfg_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Jarak Aman:");
                let prev = cfg.min_safe_gap_m;
                numeric_spinner(
                    ui,
                    ("td_gap", a.id),
                    &mut cfg.min_safe_gap_m,
                    SpinnerConfig {
                        min: 0.8,
                        max: 25.0,
                        step: 0.5,
                        drag_speed: 0.1,
                        decimals: 1,
                        suffix: " m",
                        wrap_around: false,
                    },
                );
                if (cfg.min_safe_gap_m - prev).abs() > 1e-6 {
                    cfg_changed = true;
                }
            });
            ui.add_space(4.0);
            if ui
                .checkbox(
                    &mut cfg.start_diverted,
                    "Mulai langsung dialihkan (Jalur Alternatif)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            if ui
                .checkbox(
                    &mut cfg.prefill_initial,
                    "Mulai sudah terisi kendaraan (Pre-fill)",
                )
                .changed()
            {
                cfg_changed = true;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Seed Sync: #{}", cfg.seed % 100_000))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if ui.small_button("Acak Pola").clicked() {
                    cfg.seed = cfg
                        .seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    cfg_changed = true;
                }
            });
        });
    });

    if cfg_changed {
        write_traffic_diversion_config(a, &cfg);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_traffic_queue_vehicle_pool_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    route_edit: &mut RouteEditState,
    anim_id: Uuid,
    vehicle_pool: &mut Vec<TrafficVehicleEntry>,
    preview_lane_geom: Option<&[[f64; 3]]>,
    editable: bool,
) -> bool {
    let mut changed = false;
    inspector_card(
        ui,
        &format!("Pool Model 3D Kendaraan ({})", vehicle_pool.len()),
        true,
        |ui| {
            ui.label(
                egui::RichText::new(
                    "Bisa preview maks. 2 model sekaligus di jalur untuk membandingkan skala antar mobil/motor langsung di peta.",
                )
                .size(10.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);

            let active_preview_count = state
                .traffic_scale_preview
                .as_ref()
                .filter(|p| p.anim_id == anim_id)
                .map(|p| p.pool_indices.len())
                .unwrap_or(0);
            if active_preview_count > 0 {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(12, 46, 62))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Preview aktif: {active_preview_count}/2 model"
                                ))
                                .size(10.0)
                                .strong()
                                .color(egui::Color32::from_rgb(125, 211, 252)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button(
                                            egui::RichText::new("Tutup Semua")
                                                .size(9.5)
                                                .color(egui::Color32::from_rgb(253, 230, 138)),
                                        )
                                        .clicked()
                                    {
                                        state.traffic_scale_preview = None;
                                    }
                                },
                            );
                        });
                        ui.add_space(3.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(
                                egui::RichText::new("Posisi:")
                                    .size(9.5)
                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            ui.selectable_value(
                                &mut state.traffic_preview_layout,
                                TrafficPreviewLayout::FrontBack,
                                egui::RichText::new("Depan-Belakang").size(9.5),
                            );
                            ui.selectable_value(
                                &mut state.traffic_preview_layout,
                                TrafficPreviewLayout::SideBySide,
                                egui::RichText::new("Berdampingan").size(9.5),
                            );
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jarak Preview:")
                                    .size(9.5)
                                    .color(egui::Color32::from_rgb(186, 230, 253)),
                            );
                            let spin_w = (ui.available_width() - 2.0).max(70.0);
                            numeric_spinner_w(
                                ui,
                                ("tq_prev_gap", anim_id),
                                &mut state.traffic_preview_gap_m,
                                spin_w,
                                SpinnerConfig {
                                    min: 0.0,
                                    max: 50.0,
                                    step: 0.5,
                                    drag_speed: 0.2,
                                    decimals: 1,
                                    suffix: " m",
                                    wrap_around: false,
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }

            let mut remove_pool_idx: Option<usize> = None;
            let mut toggle_preview_idx: Option<usize> = None;
            let mut focus_gizmo_idx: Option<usize> = None;
            let has_any_lane = preview_lane_geom.is_some_and(|g| g.len() >= 2);

            for idx in 0..vehicle_pool.len() {
                let preview_slot = state
                    .traffic_scale_preview
                    .as_ref()
                    .filter(|p| p.anim_id == anim_id)
                    .and_then(|p| p.pool_indices.iter().position(|&pi| pi == idx));
                let is_in_preview = preview_slot.is_some();
                let is_active_gizmo = state.traffic_scale_preview.as_ref().is_some_and(|p| {
                    p.anim_id == anim_id && p.active_gizmo_pool_idx == idx && is_in_preview
                });

                egui::Frame::new()
                    .fill(if is_active_gizmo {
                        egui::Color32::from_rgb(12, 46, 62)
                    } else if is_in_preview {
                        egui::Color32::from_rgb(42, 34, 16)
                    } else {
                        egui::Color32::from_rgb(15, 23, 42)
                    })
                    .stroke(egui::Stroke::new(
                        if is_in_preview { 1.3 } else { 1.0 },
                        if is_active_gizmo {
                            egui::Color32::from_rgb(56, 189, 248)
                        } else if is_in_preview {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            egui::Color32::from_rgb(44, 62, 92)
                        },
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if editable {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(
                                                egui::RichText::new("Hapus")
                                                    .size(9.5)
                                                    .color(egui::Color32::from_rgb(248, 113, 113)),
                                            )
                                            .clicked()
                                        {
                                            remove_pool_idx = Some(idx);
                                        }
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let slot_tag =
                                                    match (preview_slot, state.traffic_preview_layout)
                                                    {
                                                        (
                                                            Some(0),
                                                            TrafficPreviewLayout::FrontBack,
                                                        ) => "[Depan] ",
                                                        (
                                                            Some(1),
                                                            TrafficPreviewLayout::FrontBack,
                                                        ) => "[Belakang] ",
                                                        (
                                                            Some(0),
                                                            TrafficPreviewLayout::SideBySide,
                                                        ) => "[Sisi 1] ",
                                                        (
                                                            Some(1),
                                                            TrafficPreviewLayout::SideBySide,
                                                        ) => "[Sisi 2] ",
                                                        _ => "",
                                                    };
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(format!(
                                                            "{slot_tag}{}",
                                                            vehicle_pool[idx].name
                                                        ))
                                                        .size(11.0)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                    )
                                                    .truncate(),
                                                );
                                            },
                                        );
                                    },
                                );
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&vehicle_pool[idx].name)
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                            }
                        });

                        if editable {
                            ui.add_space(2.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                if is_in_preview {
                                    if is_active_gizmo {
                                        egui::Frame::new()
                                            .fill(egui::Color32::from_rgb(8, 145, 178))
                                            .corner_radius(3.0)
                                            .inner_margin(egui::Margin::symmetric(6, 2))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new("Scale Gizmo Aktif")
                                                        .size(9.5)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                );
                                            });
                                    } else {
                                        let pick_gizmo = egui::Button::new(
                                            egui::RichText::new("Pindah Scale Gizmo")
                                                .size(9.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(37, 99, 235))
                                        .small();
                                        if ui.add(pick_gizmo).clicked() {
                                            focus_gizmo_idx = Some(idx);
                                        }
                                    }
                                    let close_btn = egui::Button::new(
                                        egui::RichText::new("Tutup Preview")
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(180, 83, 9))
                                    .small();
                                    if ui.add(close_btn).clicked() {
                                        toggle_preview_idx = Some(idx);
                                    }
                                } else {
                                    let lbl = if active_preview_count == 1 {
                                        "Preview Bandingkan (2/2)"
                                    } else {
                                        "Preview & Scale di Peta"
                                    };
                                    let prev_btn = egui::Button::new(
                                        egui::RichText::new(lbl)
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(14, 116, 144))
                                    .small();
                                    if ui.add_enabled(has_any_lane, prev_btn).clicked() {
                                        toggle_preview_idx = Some(idx);
                                    }
                                }
                            });
                        }

                        ui.add_space(3.0);
                        ui.add_enabled_ui(editable, |ui| {
                            let lbl_w = 50.0_f32;
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Skala:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 2.0).max(66.0);
                                let prev_s = vehicle_pool[idx].scale;
                                numeric_spinner_w(
                                    ui,
                                    ("tq_v_scale", anim_id, idx),
                                    &mut vehicle_pool[idx].scale,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 0.01,
                                        max: 20.0,
                                        step: 0.01,
                                        drag_speed: 0.01,
                                        decimals: 2,
                                        suffix: "x",
                                        wrap_around: false,
                                    },
                                );
                                if (vehicle_pool[idx].scale - prev_s).abs() > 1e-6 {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Panjang:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 2.0).max(66.0);
                                let prev_l = vehicle_pool[idx].length_m;
                                numeric_spinner_w(
                                    ui,
                                    ("tq_v_len", anim_id, idx),
                                    &mut vehicle_pool[idx].length_m,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 1.5,
                                        max: 24.0,
                                        step: 0.5,
                                        drag_speed: 0.2,
                                        decimals: 1,
                                        suffix: " m",
                                        wrap_around: false,
                                    },
                                );
                                if (vehicle_pool[idx].length_m - prev_l).abs() > 1e-6 {
                                    changed = true;
                                }
                            });
                            if let Some([mw, mh, ml]) = measured_vehicle_scaled_dims(
                                state,
                                &vehicle_pool[idx].model_path,
                                vehicle_pool[idx].scale as f32,
                                vehicle_pool[idx].rot_y_offset_deg,
                            ) {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "3D asli: P {ml:.1}m x L {mw:.1}m x T {mh:.1}m"
                                        ))
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(125, 211, 252)),
                                    );
                                    if (ml - vehicle_pool[idx].length_m as f32).abs() > 0.35
                                        && ui
                                            .small_button(format!("Sync P ({ml:.1}m)"))
                                            .clicked()
                                    {
                                        vehicle_pool[idx].length_m =
                                            ((ml as f64).clamp(1.2, 24.0) * 10.0).round() / 10.0;
                                        changed = true;
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Rot Y:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 44.0).max(66.0);
                                let prev_r = vehicle_pool[idx].rot_y_offset_deg;
                                numeric_spinner_w(
                                    ui,
                                    ("tq_v_roty", anim_id, idx),
                                    &mut vehicle_pool[idx].rot_y_offset_deg,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 0.0,
                                        max: 360.0,
                                        step: 90.0,
                                        drag_speed: 1.0,
                                        decimals: 0,
                                        suffix: "°",
                                        wrap_around: true,
                                    },
                                );
                                if (vehicle_pool[idx].rot_y_offset_deg - prev_r).abs() > 1e-6 {
                                    changed = true;
                                }
                                if ui.small_button("+90°").clicked() {
                                    vehicle_pool[idx].rot_y_offset_deg =
                                        (vehicle_pool[idx].rot_y_offset_deg + 90.0) % 360.0;
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(lbl_w, 20.0),
                                    egui::Label::new(egui::RichText::new("Bobot:").size(10.0)),
                                );
                                let spin_w = (ui.available_width() - 2.0).max(66.0);
                                let mut w_f64 = vehicle_pool[idx].weight as f64;
                                numeric_spinner_w(
                                    ui,
                                    ("tq_v_w", anim_id, idx),
                                    &mut w_f64,
                                    spin_w,
                                    SpinnerConfig {
                                        min: 0.1,
                                        max: 20.0,
                                        step: 0.5,
                                        drag_speed: 0.1,
                                        decimals: 1,
                                        suffix: "",
                                        wrap_around: false,
                                    },
                                );
                                if (w_f64 as f32 - vehicle_pool[idx].weight).abs() > 1e-4 {
                                    vehicle_pool[idx].weight = w_f64 as f32;
                                    changed = true;
                                }
                            });
                        });
                    });
                ui.add_space(3.0);
            }

            if let Some(idx) = focus_gizmo_idx
                && let Some(ref mut preview) = state.traffic_scale_preview
                && preview.anim_id == anim_id
            {
                preview.active_gizmo_pool_idx = idx;
            }

            if let Some(idx) = toggle_preview_idx {
                let already_present = state
                    .traffic_scale_preview
                    .as_ref()
                    .filter(|p| p.anim_id == anim_id)
                    .is_some_and(|p| p.pool_indices.contains(&idx));
                if already_present {
                    if let Some(ref mut preview) = state.traffic_scale_preview {
                        preview.pool_indices.retain(|&i| i != idx);
                        if preview.pool_indices.is_empty() {
                            state.traffic_scale_preview = None;
                        } else if preview.active_gizmo_pool_idx == idx {
                            preview.active_gizmo_pool_idx = preview.pool_indices[0];
                        }
                    }
                } else {
                    route_edit.clear();
                    playback.stop_editor_preview();
                    state.tool_mode = ToolMode::Select;
                    let mut indices = state
                        .traffic_scale_preview
                        .as_ref()
                        .filter(|p| p.anim_id == anim_id)
                        .map(|p| p.pool_indices.clone())
                        .unwrap_or_default();
                    if indices.len() >= 2 {
                        indices.remove(0);
                    }
                    indices.push(idx);
                    state.traffic_scale_preview = Some(TrafficScalePreviewState {
                        anim_id,
                        pool_indices: indices,
                        active_gizmo_pool_idx: idx,
                    });
                    if let Some(geom) = preview_lane_geom
                        && geom.len() >= 2
                    {
                        let p0 = state
                            .geo_anchor
                            .lng_lat_alt_to_local(geom[0][0], geom[0][1], 0.0);
                        let p1 = state
                            .geo_anchor
                            .lng_lat_alt_to_local(geom[1][0], geom[1][1], 0.0);
                        let v0 = bevy::math::Vec2::new(p0[0], p0[2]);
                        let v1 = bevy::math::Vec2::new(p1[0], p1[2]);
                        let seg = v1 - v0;
                        let seg_len = seg.length().max(0.01);
                        let t = (10.0_f32.min(seg_len * 0.45) / seg_len).clamp(0.0, 1.0);
                        tcam.target_local = v0.lerp(v1, t);
                    }
                }
            }

            if let Some(idx) = remove_pool_idx {
                vehicle_pool.remove(idx);
                if let Some(ref mut preview) = state.traffic_scale_preview
                    && preview.anim_id == anim_id
                {
                    preview.pool_indices.retain(|&i| i != idx);
                    for pi in &mut preview.pool_indices {
                        if *pi > idx {
                            *pi -= 1;
                        }
                    }
                    if preview.pool_indices.is_empty() {
                        state.traffic_scale_preview = None;
                    } else if preview.active_gizmo_pool_idx == idx {
                        preview.active_gizmo_pool_idx = preview.pool_indices[0];
                    } else if preview.active_gizmo_pool_idx > idx {
                        preview.active_gizmo_pool_idx -= 1;
                    }
                }
                changed = true;
            }

            if editable {
                ui.add_space(5.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Tambah dari Semua Katalog 3D ({} total):",
                        state.catalog.len()
                    ))
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(186, 230, 253)),
                );
                ui.add_space(3.0);

                let mut cat_counts: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for m in &state.catalog {
                    *cat_counts.entry(m.category.clone()).or_insert(0) += 1;
                }
                let selected_cat_label = match &state.traffic_pool_category_filter {
                    None => format!("Semua Kategori ({})", state.catalog.len()),
                    Some(c) => {
                        let cnt = cat_counts.get(c).copied().unwrap_or(0);
                        format!("{c} ({cnt})")
                    }
                };
                egui::ComboBox::from_id_salt(("tq_cat_filter", anim_id))
                    .width((ui.available_width() - 4.0).max(120.0))
                    .selected_text(selected_cat_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.traffic_pool_category_filter,
                            None,
                            format!("Semua Kategori ({})", state.catalog.len()),
                        );
                        for (cat, cnt) in &cat_counts {
                            ui.selectable_value(
                                &mut state.traffic_pool_category_filter,
                                Some(cat.clone()),
                                format!("{cat} ({cnt})"),
                            );
                        }
                    });
                ui.add_space(3.0);

                ui.horizontal(|ui| {
                    let has_q = !state.traffic_pool_search.is_empty();
                    let search_w =
                        (ui.available_width() - if has_q { 52.0 } else { 4.0 }).max(80.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.traffic_pool_search)
                            .hint_text("Cari nama / kategori model 3D...")
                            .desired_width(search_w),
                    );
                    if has_q && ui.small_button("Reset").clicked() {
                        state.traffic_pool_search.clear();
                    }
                });
                ui.add_space(3.0);

                let q = state.traffic_pool_search.trim().to_lowercase();
                let cat_filter = state.traffic_pool_category_filter.clone();
                let candidates: Vec<_> = state
                    .catalog
                    .iter()
                    .filter(|m| {
                        if !state.is_model_path_allowed(&m.model_path) {
                            return false;
                        }
                        if vehicle_pool.iter().any(|v| v.model_path == m.model_path) {
                            return false;
                        }
                        if let Some(ref cf) = cat_filter
                            && m.category != *cf
                        {
                            return false;
                        }
                        if q.is_empty() {
                            true
                        } else {
                            m.name.to_lowercase().contains(&q)
                                || m.category.to_lowercase().contains(&q)
                                || m.model_path.to_lowercase().contains(&q)
                        }
                    })
                    .cloned()
                    .collect();

                if cat_filter.is_some() && !candidates.is_empty() && candidates.len() <= 25 {
                    let add_all_btn = egui::Button::new(
                        egui::RichText::new(format!(
                            "+ Tambah Semua ({} model)",
                            candidates.len()
                        ))
                        .size(10.0)
                        .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(5, 150, 105))
                    .small();
                    if ui.add(add_all_btn).clicked() {
                        for m in &candidates {
                            vehicle_pool.push(TrafficVehicleEntry {
                                model_path: m.model_path.clone(),
                                name: m.name.clone(),
                                scale: 1.0,
                                length_m: TrafficVehicleEntry::estimate_length_m(
                                    &m.name,
                                    &m.model_path,
                                ),
                                weight: 1.0,
                                rot_y_offset_deg: m.default_rotation_deg[1],
                            });
                        }
                        changed = true;
                    }
                    ui.add_space(3.0);
                }

                egui::ScrollArea::vertical()
                    .id_salt(("tq_cat_scroll", anim_id))
                    .max_height(160.0)
                    .show(ui, |ui| {
                        if candidates.is_empty() {
                            ui.label(
                                egui::RichText::new("Tidak ada model tambahan yang cocok.")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        } else {
                            for m in &candidates {
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button(
                                            egui::RichText::new("+")
                                                .strong()
                                                .color(egui::Color32::from_rgb(110, 231, 183)),
                                        )
                                        .clicked()
                                    {
                                        vehicle_pool.push(TrafficVehicleEntry {
                                            model_path: m.model_path.clone(),
                                            name: m.name.clone(),
                                            scale: 1.0,
                                            length_m: TrafficVehicleEntry::estimate_length_m(
                                                &m.name,
                                                &m.model_path,
                                            ),
                                            weight: 1.0,
                                            rot_y_offset_deg: m.default_rotation_deg[1],
                                        });
                                        changed = true;
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!(
                                                "{} ({})",
                                                m.name, m.category
                                            ))
                                            .size(10.5)
                                            .color(egui::Color32::LIGHT_GRAY),
                                        )
                                        .truncate(),
                                    );
                                });
                            }
                        }
                    });
            }
        },
    );
    changed
}

fn draw_group_anim_inspector_cards(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    a: &mut twg_shared::models::Anim,
    editable: bool,
) {
    let mut cfg = parse_group_anim_config(a);
    let mut cfg_changed = false;
    let is_combine = cfg.mode == GroupAnimMode::Combine;

    // Card 1: Identity, Group Mode & Member Animations
    inspector_card(ui, "Grup Animasi — Mode & Anggota", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            let (banner_bg, banner_stroke, banner_fg, banner_txt) = if is_combine {
                (
                    egui::Color32::from_rgb(23, 37, 84),
                    egui::Color32::from_rgb(59, 130, 246),
                    egui::Color32::from_rgb(191, 219, 254),
                    "COMBINE (BARENG): Semua animasi anggota akan dijalankan bersamaan ketika grup ini di-Play. Pause / Resume / Stop ikut mengendalikan seluruh anggota.",
                )
            } else {
                (
                    egui::Color32::from_rgb(46, 16, 101),
                    egui::Color32::from_rgb(139, 92, 246),
                    egui::Color32::from_rgb(221, 214, 254),
                    "SEQUENCE (BERURUTAN): Animasi berjalan satu per satu sesuai urutan #1, #2, #3... Mendukung mode Tunggu Selesai (anak Repeat jalan 1 siklus lalu lanjut) atau Jeda Waktu Detik.",
                )
            };
            egui::Frame::new()
                .fill(banner_bg)
                .stroke(egui::Stroke::new(1.0, banner_stroke))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(banner_txt).size(9.5).color(banner_fg));
                });
            ui.add_space(5.0);

            ui.label("Nama Animasi:");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            ui.add_space(5.0);

            ui.label(
                egui::RichText::new("Mode Grup:")
                    .size(11.0)
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.columns(2, |cols| {
                for (idx, &mode) in GroupAnimMode::ALL.iter().enumerate() {
                    let active = cfg.mode == mode;
                    let fill = if active {
                        match mode {
                            GroupAnimMode::Combine => egui::Color32::from_rgb(37, 99, 235),
                            GroupAnimMode::Sequence => egui::Color32::from_rgb(124, 58, 237),
                        }
                    } else {
                        egui::Color32::from_rgb(24, 34, 52)
                    };
                    let btn = egui::Button::new(
                        egui::RichText::new(mode.label())
                            .size(10.5)
                            .strong()
                            .color(if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(148, 163, 184)
                            }),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(cols[idx].available_width(), 24.0));
                    if cols[idx].add(btn).clicked() && cfg.mode != mode {
                        cfg.mode = mode;
                        cfg_changed = true;
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Animasi Anggota ({} item):", cfg.items.len()))
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                if cfg.items.len() > 1 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Kosongkan")
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(248, 113, 113)),
                                )
                                .small()
                                .frame(false),
                            )
                            .clicked()
                        {
                            cfg.items.clear();
                            cfg_changed = true;
                        }
                    });
                }
            });

            ui.add_space(3.0);
            // Quick filter + dropdown to add child animations (cycle-safe!)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add(
                    egui::TextEdit::singleline(&mut state.group_add_search)
                        .hint_text("Filter animasi yg ingin ditambahkan...")
                        .desired_width((ui.available_width() - 28.0).max(80.0)),
                );
                if ui
                    .add_enabled(
                        !state.group_add_search.is_empty(),
                        egui::Button::new("X").small(),
                    )
                    .on_hover_text("Reset filter")
                    .clicked()
                {
                    state.group_add_search.clear();
                }
            });
            ui.add_space(3.0);

            let q_lower = state.group_add_search.trim().to_lowercase();
            let mut add_child_opt: Option<Uuid> = None;
            let mut hovered_child_opt: Option<Uuid> = None;
            let mut fly_to_child: Option<(f64, f64)> = None;
            egui::ComboBox::from_id_salt(("group_add_child_anim", a.id))
                .selected_text("+ Tambah Animasi ke Grup...")
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    let mut avail_cnt = 0usize;
                    for cand in &state.anims {
                        if !state.can_play_anim(cand) {
                            continue;
                        }
                        if would_create_group_cycle(a.id, cand.id, &state.anims) {
                            continue;
                        }
                        if is_combine && cfg.items.iter().any(|it| it.anim_id == cand.id) {
                            continue;
                        }
                        let tag = short_anim_badge_for_picker(cand);
                        let label_full = format!("[{tag}] {}", cand.name);
                        if !q_lower.is_empty() && !label_full.to_lowercase().contains(&q_lower) {
                            continue;
                        }
                        avail_cnt += 1;
                        let resp = ui.selectable_label(false, label_full);
                        if resp.hovered() {
                            hovered_child_opt = Some(cand.id);
                        }
                        if resp.clicked() {
                            add_child_opt = Some(cand.id);
                            hovered_child_opt = Some(cand.id);
                        }
                    }
                    if avail_cnt == 0 {
                        ui.label(
                            egui::RichText::new("(Tidak ada animasi lain yg tersedia)")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                });
            if let Some(cid) = add_child_opt {
                cfg.items.push(GroupStepItem::new(cid));
                cfg_changed = true;
            }
            ui.label(
                egui::RichText::new(
                    "Tip: Arahkan kursor ke daftar animasi (dropdown / anggota) untuk melihat highlight lokasinya di peta 3D.",
                )
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );

            // Ordered member list
            ui.add_space(5.0);
            if cfg.items.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "Belum ada animasi anggota. Pilih minimal 2 animasi dari daftar di atas.",
                    )
                    .size(10.5)
                    .color(egui::Color32::from_rgb(248, 113, 113)),
                );
            } else {
                let mut move_swap: Option<(usize, usize)> = None;
                let mut remove_idx: Option<usize> = None;
                let len = cfg.items.len();
                let default_trig = cfg.sequence_trigger;
                let default_delay = cfg.sequence_delay_sec;
                let default_gap = cfg.sequence_gap_sec;

                for idx in 0..len {
                    let cid = cfg.items[idx].anim_id;
                    let child_ref = state.anims.iter().find(|x| x.id == cid);
                    let (tag_str, cname, is_rep, is_cont) = match child_ref {
                        Some(c) => (
                            short_anim_badge_for_picker(c),
                            c.name.as_str(),
                            c.replay,
                            anim_is_continuous_unbounded(c),
                        ),
                        None => ("HILANG", "(Animasi terhapus)", false, false),
                    };

                    let child_focus_pt = child_ref
                        .and_then(|c| anim_primary_focus_lnglat(c, &state.anims, &state.markers));
                    let mut row_hovered = false;
                    let frame_resp = egui::Frame::new()
                        .fill(egui::Color32::from_rgb(15, 23, 42))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74)))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(6, 4))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.label(
                                    egui::RichText::new(format!("#{}", idx + 1))
                                        .size(10.0)
                                        .strong()
                                        .color(if is_combine {
                                            egui::Color32::from_rgb(96, 165, 250)
                                        } else {
                                            egui::Color32::from_rgb(192, 132, 252)
                                        }),
                                );
                                ui.label(
                                    egui::RichText::new(format!("[{tag_str}]"))
                                        .size(8.5)
                                        .strong()
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                                let lbl_resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(cname)
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(226, 232, 240)),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::hover()),
                                );
                                if lbl_resp.hovered() {
                                    row_hovered = true;
                                }
                                if is_rep {
                                    ui.label(
                                        egui::RichText::new("REP")
                                            .size(8.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(251, 191, 36)),
                                    )
                                    .on_hover_text(
                                        "Animasi ini berstatus Repeat — di Sequence tetap lanjut mengulang setelah 1 siklus selesai",
                                    );
                                }
                                if is_cont {
                                    ui.label(
                                        egui::RichText::new("TERUS")
                                            .size(8.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(56, 189, 248)),
                                    )
                                    .on_hover_text(
                                        if child_ref.is_some_and(twg_shared::audio_anim::is_audio_anim) {
                                            "Suara tanpa batas: Tunggu Selesai menunggu sampai Stop. Pilih Jeda Waktu untuk lanjut otomatis."
                                        } else {
                                            "Animasi terus-menerus — di Sequence mode Tunggu Selesai akan lanjut setelah 'Batas Tunggu Terus-menerus'"
                                        },
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("x")
                                                        .size(10.0)
                                                        .color(egui::Color32::from_rgb(
                                                            248, 113, 113,
                                                        )),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Hapus dari grup")
                                            .clicked()
                                        {
                                            remove_idx = Some(idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                idx + 1 < len,
                                                egui::Button::new(
                                                    egui::RichText::new("v").size(9.5),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Turunkan urutan")
                                            .clicked()
                                        {
                                            move_swap = Some((idx, idx + 1));
                                        }
                                        if ui
                                            .add_enabled(
                                                idx > 0,
                                                egui::Button::new(
                                                    egui::RichText::new("^").size(9.5),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Naikkan urutan")
                                            .clicked()
                                        {
                                            move_swap = Some((idx - 1, idx));
                                        }
                                        if let Some(pt) = child_focus_pt {
                                            let go_resp = ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("Go")
                                                            .size(9.5)
                                                            .color(egui::Color32::from_rgb(
                                                                125, 211, 252,
                                                            )),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text("Fokus kamera ke lokasi animasi ini");
                                            if go_resp.hovered() {
                                                row_hovered = true;
                                            }
                                            if go_resp.clicked() {
                                                fly_to_child = Some(pt);
                                            }
                                        }
                                    },
                                );
                            });

                            // Per-step optional trigger override for Sequence steps #2, #3, ...
                            if !is_combine && idx > 0 {
                                let item = &mut cfg.items[idx];
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.label(
                                        egui::RichText::new("Pemicu:")
                                            .size(9.5)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    let cur_lbl = match item.trigger_override {
                                        None => format!("Default ({})", default_trig.short_label()),
                                        Some(SequenceTriggerMode::WaitComplete) => {
                                            "Tunggu Selesai".to_string()
                                        }
                                        Some(SequenceTriggerMode::WaitDelay) => {
                                            "Jeda Detik".to_string()
                                        }
                                    };
                                    egui::ComboBox::from_id_salt(("seq_step_trig", a.id, idx))
                                        .selected_text(egui::RichText::new(cur_lbl).size(9.5))
                                        .width(118.0)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(
                                                    item.trigger_override.is_none(),
                                                    format!("Default ({})", default_trig.short_label()),
                                                )
                                                .clicked()
                                            {
                                                item.trigger_override = None;
                                                item.delay_override_sec = None;
                                                cfg_changed = true;
                                            }
                                            if ui
                                                .selectable_label(
                                                    item.trigger_override
                                                        == Some(SequenceTriggerMode::WaitComplete),
                                                    "Tunggu Selesai",
                                                )
                                                .clicked()
                                            {
                                                item.trigger_override =
                                                    Some(SequenceTriggerMode::WaitComplete);
                                                if item.delay_override_sec.is_none() {
                                                    item.delay_override_sec = Some(default_gap);
                                                }
                                                cfg_changed = true;
                                            }
                                            if ui
                                                .selectable_label(
                                                    item.trigger_override
                                                        == Some(SequenceTriggerMode::WaitDelay),
                                                    "Jeda Detik",
                                                )
                                                .clicked()
                                            {
                                                item.trigger_override =
                                                    Some(SequenceTriggerMode::WaitDelay);
                                                if item.delay_override_sec.is_none() {
                                                    item.delay_override_sec = Some(default_delay);
                                                }
                                                cfg_changed = true;
                                            }
                                        });

                                    if let Some(ov_mode) = item.trigger_override {
                                        let fallback_d = match ov_mode {
                                            SequenceTriggerMode::WaitComplete => default_gap,
                                            SequenceTriggerMode::WaitDelay => default_delay,
                                        };
                                        let mut val =
                                            item.delay_override_sec.unwrap_or(fallback_d);
                                        if numeric_spinner(
                                            ui,
                                            ("seq_step_delay", a.id, idx),
                                            &mut val,
                                            SpinnerConfig {
                                                min: 0.0,
                                                max: 120.0,
                                                step: 0.05,
                                                drag_speed: 0.02,
                                                decimals: 2,
                                                suffix: "s",
                                                wrap_around: false,
                                            },
                                        ) {
                                            item.delay_override_sec = Some(val);
                                            cfg_changed = true;
                                        }
                                    }
                                });
                            }
                        });
                    if frame_resp.response.hovered() || row_hovered {
                        hovered_child_opt = Some(cid);
                    }
                }
                if let Some((i, j)) = move_swap {
                    cfg.items.swap(i, j);
                    cfg_changed = true;
                }
                if let Some(ri) = remove_idx {
                    cfg.items.remove(ri);
                    cfg_changed = true;
                }
            }
            if let Some(hid) = hovered_child_opt {
                state.group_hovered_child_anim_id = Some(hid);
            }
            if let Some((lng, lat)) = fly_to_child {
                let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
                tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
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
    });

    // Card 2: Timing & Orchestration Settings
    inspector_card(ui, "Pengaturan Waktu & Eksekusi", true, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            if !is_combine {
                ui.label(
                    egui::RichText::new("Mode Pemicu Default Sequence:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.columns(2, |cols| {
                    for (idx, &tr) in SequenceTriggerMode::ALL.iter().enumerate() {
                        let active = cfg.sequence_trigger == tr;
                        let btn = egui::Button::new(
                            egui::RichText::new(tr.label())
                                .size(10.0)
                                .strong()
                                .color(if active {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_rgb(148, 163, 184)
                                }),
                        )
                        .fill(if active {
                            egui::Color32::from_rgb(124, 58, 237)
                        } else {
                            egui::Color32::from_rgb(24, 34, 52)
                        })
                        .min_size(egui::vec2(cols[idx].available_width(), 24.0));
                        if cols[idx].add(btn).clicked() && cfg.sequence_trigger != tr {
                            cfg.sequence_trigger = tr;
                            cfg_changed = true;
                        }
                    }
                });
                ui.add_space(5.0);

                match cfg.sequence_trigger {
                    SequenceTriggerMode::WaitDelay => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jeda Waktu Antar Langkah:")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            cfg_changed |= numeric_spinner(
                                ui,
                                ("grp_seq_delay", a.id),
                                &mut cfg.sequence_delay_sec,
                                SpinnerConfig {
                                    min: 0.0,
                                    max: 120.0,
                                    step: 0.05,
                                    drag_speed: 0.02,
                                    decimals: 2,
                                    suffix: " dtk",
                                    wrap_around: false,
                                },
                            );
                        });
                        ui.label(
                            egui::RichText::new(
                                "Mis. 0.25 dtk: animasi #1 jalan -> tunggu 0.25 dtk -> animasi #2 jalan tanpa menunggu #1 selesai.",
                            )
                            .size(9.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                    SequenceTriggerMode::WaitComplete => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Jeda Setelah Selesai (Gap):")
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            cfg_changed |= numeric_spinner(
                                ui,
                                ("grp_seq_gap", a.id),
                                &mut cfg.sequence_gap_sec,
                                SpinnerConfig {
                                    min: 0.0,
                                    max: 60.0,
                                    step: 0.1,
                                    drag_speed: 0.05,
                                    decimals: 2,
                                    suffix: " dtk",
                                    wrap_around: false,
                                },
                            );
                        });
                        ui.label(
                            egui::RichText::new(
                                "Menunggu animasi sebelumnya selesai 1 siklus penuh (jika anak berstatus Repeat, langkah berikut tetap jalan sementara anak terus mengulang).",
                            )
                            .size(9.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                }
                ui.add_space(4.0);
            }

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Batas Tunggu Anak Terus-menerus:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("grp_cont_wait", a.id),
                    &mut cfg.continuous_wait_sec,
                    SpinnerConfig {
                        min: 0.0,
                        max: 60.0,
                        step: 0.25,
                        drag_speed: 0.05,
                        decimals: 2,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });
            ui.label(
                egui::RichText::new(
                    "Untuk Water Cannon / Api / Panah 3D, Sequence lanjut setelah batas ini. Play Suara tanpa batas tetap menunggu Stop; gunakan Jeda Waktu untuk lanjut otomatis.",
                )
                .size(9.5)
                .color(egui::Color32::from_rgb(148, 163, 184)),
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Jeda Awal Sebelum Mulai:")
                        .size(11.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                cfg_changed |= numeric_spinner(
                    ui,
                    ("grp_init_delay", a.id),
                    &mut cfg.initial_delay_sec,
                    SpinnerConfig {
                        min: 0.0,
                        max: 60.0,
                        step: 0.25,
                        drag_speed: 0.05,
                        decimals: 2,
                        suffix: " dtk",
                        wrap_around: false,
                    },
                );
            });

            ui.add_space(4.0);
            if !is_combine {
                ui.checkbox(&mut a.replay, "Repeat / Ulangi Sequence Otomatis");
            }
            if ui
                .checkbox(
                    &mut cfg.include_in_play_all,
                    "Ikut dijalankan saat klik 'Play Semua'",
                )
                .on_hover_text(
                    "Default mati agar animasi anggota tidak terpicu ganda saat klik Play Semua.",
                )
                .changed()
            {
                cfg_changed = true;
            }
        });
    });

    if cfg_changed {
        write_group_anim_config(a, &cfg);
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
                state.active_map_mode.as_str(),
                state.active_base_layer.label()
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
            // Keep Base Layer valid for the active Map Mode
            match state.active_map_mode {
                MapMode::Outdoor => {
                    if !matches!(state.active_base_layer, BaseLayer::Osm | BaseLayer::Satellite) {
                        state.active_base_layer = BaseLayer::Osm;
                    }
                }
                MapMode::Indoor => {
                    if !matches!(
                        state.active_base_layer,
                        BaseLayer::GridDark
                            | BaseLayer::GridLight
                            | BaseLayer::BlankDark
                            | BaseLayer::BlankLight
                            | BaseLayer::FloorPlan
                    ) {
                        state.active_base_layer = BaseLayer::GridDark;
                    }
                }
            }
            let allowed_layers: &[BaseLayer] = match state.active_map_mode {
                MapMode::Outdoor => &[BaseLayer::Osm, BaseLayer::Satellite],
                MapMode::Indoor => &[
                    BaseLayer::GridDark,
                    BaseLayer::GridLight,
                    BaseLayer::BlankDark,
                    BaseLayer::BlankLight,
                    BaseLayer::FloorPlan,
                ],
            };
            ui.horizontal(|ui| {
                ui.label("Base Layer:");
                egui::ComboBox::from_id_salt("inspector_sk_layer")
                    .selected_text(state.active_base_layer.label())
                    .width(155.0)
                    .show_ui(ui, |ui| {
                        for &layer in allowed_layers {
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

                if state.active_base_layer == BaseLayer::FloorPlan {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Pengaturan Floor Plan")
                            .size(11.5)
                            .strong()
                            .color(egui::Color32::from_rgb(125, 211, 252)),
                    );
                    ui.add_space(3.0);

                    let current_url = sk
                        .indoor_config
                        .floor_plan_url
                        .clone()
                        .filter(|s| !s.trim().is_empty());
                    if let Some(url) = &current_url {
                        let short_name = url.rsplit('/').next().unwrap_or(url);
                        ui.label(
                            egui::RichText::new(format!("Gambar: {short_name}"))
                                .size(10.5)
                                .color(egui::Color32::from_rgb(134, 239, 172)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("Belum ada gambar Floor Plan.")
                                .size(10.5)
                                .color(egui::Color32::from_rgb(251, 191, 36)),
                        );
                    }

                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let btn_text = if state.floor_plan_uploading {
                            "Mengunggah..."
                        } else if current_url.is_some() {
                            "Ganti Gambar..."
                        } else {
                            "Upload Gambar..."
                        };
                        let upload_btn = egui::Button::new(
                            egui::RichText::new(btn_text)
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(14, 116, 144));
                        if ui
                            .add_enabled(!state.floor_plan_uploading, upload_btn)
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .set_title("Pilih Gambar Floor Plan (PNG / JPG / WebP)")
                                .add_filter("Gambar Floor Plan", &["png", "jpg", "jpeg", "webp"])
                                .pick_file()
                        {
                            if let Some(auth) = current_auth(state)
                                && !state.offline_preview
                            {
                                state.floor_plan_uploading = true;
                                state.floor_plan_error = None;
                                api.send(ApiCommand::UploadFloorPlan {
                                    auth,
                                    file_path: path,
                                    target: crate::net::client::FloorPlanUploadTarget::ActiveSkenario,
                                });
                            } else {
                                // Offline fallback: use local file path directly
                                let p_str = path.to_string_lossy().to_string();
                                let dims = image::image_dimensions(&path).ok();
                                sk.indoor_config.floor_plan_url = Some(p_str.clone());
                                if let Some((w_px, h_px)) = dims
                                    && w_px > 0
                                    && h_px > 0
                                {
                                    let w_m = sk
                                        .indoor_config
                                        .floor_plan_width_m
                                        .unwrap_or(100.0)
                                        .clamp(5.0, 4000.0);
                                    let h_m = (w_m * (h_px as f64 / w_px as f64)).clamp(5.0, 4000.0);
                                    sk.indoor_config.floor_plan_width_m = Some(w_m);
                                    sk.indoor_config.floor_plan_height_m = Some(h_m);
                                }
                                state.skenario_dirty = true;
                                if let Some(active) = &mut state.active_skenario {
                                    active.indoor_config = sk.indoor_config.clone();
                                }
                            }
                        }

                        if current_url.is_some()
                            && ui
                                .add_enabled(
                                    !state.floor_plan_uploading,
                                    egui::Button::new(
                                        egui::RichText::new("Hapus")
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(252, 165, 165)),
                                    ),
                                )
                                .clicked()
                        {
                            sk.indoor_config.floor_plan_url = None;
                            state.skenario_dirty = true;
                            if let Some(active) = &mut state.active_skenario {
                                active.indoor_config.floor_plan_url = None;
                            }
                        }
                    });

                    if let Some(err) = &state.floor_plan_error {
                        ui.colored_label(
                            egui::Color32::from_rgb(252, 165, 165),
                            egui::RichText::new(err).size(10.5),
                        );
                    }

                    ui.add_space(4.0);
                    let mut fp_w = sk
                        .indoor_config
                        .floor_plan_width_m
                        .unwrap_or(100.0)
                        .clamp(5.0, 4000.0);
                    let mut fp_h = sk
                        .indoor_config
                        .floor_plan_height_m
                        .unwrap_or(100.0)
                        .clamp(5.0, 4000.0);
                    let mut fp_rot = sk
                        .indoor_config
                        .floor_plan_rotation_deg
                        .unwrap_or(0.0)
                        .rem_euclid(360.0);
                    let mut fp_op = sk
                        .indoor_config
                        .floor_plan_opacity
                        .unwrap_or(1.0)
                        .clamp(0.1, 1.0);
                    let mut fp_changed = false;

                    ui.horizontal(|ui| {
                        ui.label("Lebar (X):");
                        if numeric_spinner(
                            ui,
                            "inspector_fp_width",
                            &mut fp_w,
                            SpinnerConfig {
                                min: 5.0,
                                max: 4000.0,
                                step: 1.0,
                                drag_speed: 0.5,
                                decimals: 1,
                                suffix: " m",
                                wrap_around: false,
                            },
                        ) {
                            fp_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Panjang (Z):");
                        if numeric_spinner(
                            ui,
                            "inspector_fp_height",
                            &mut fp_h,
                            SpinnerConfig {
                                min: 5.0,
                                max: 4000.0,
                                step: 1.0,
                                drag_speed: 0.5,
                                decimals: 1,
                                suffix: " m",
                                wrap_around: false,
                            },
                        ) {
                            fp_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Rotasi:");
                        if numeric_spinner(
                            ui,
                            "inspector_fp_rot",
                            &mut fp_rot,
                            SpinnerConfig {
                                min: 0.0,
                                max: 360.0,
                                step: 5.0,
                                drag_speed: 0.5,
                                decimals: 1,
                                suffix: "°",
                                wrap_around: true,
                            },
                        ) {
                            fp_rot = fp_rot.rem_euclid(360.0);
                            fp_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (lbl, delta) in [("-90°", -90.0), ("0°", 0.0), ("+90°", 90.0)] {
                            if ui.small_button(lbl).clicked() {
                                fp_rot = if delta == 0.0 {
                                    0.0
                                } else {
                                    (fp_rot + delta).rem_euclid(360.0)
                                };
                                fp_changed = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Opasitas:");
                        if ui
                            .add(
                                egui::Slider::new(&mut fp_op, 0.1..=1.0)
                                    .fixed_decimals(2),
                            )
                            .changed()
                        {
                            fp_changed = true;
                        }
                    });

                    if fp_changed {
                        sk.indoor_config.floor_plan_width_m = Some(fp_w);
                        sk.indoor_config.floor_plan_height_m = Some(fp_h);
                        sk.indoor_config.floor_plan_rotation_deg = Some(fp_rot);
                        sk.indoor_config.floor_plan_opacity = Some(fp_op);
                        state.skenario_dirty = true;
                        if let Some(active) = &mut state.active_skenario {
                            active.indoor_config.floor_plan_width_m = Some(fp_w);
                            active.indoor_config.floor_plan_height_m = Some(fp_h);
                            active.indoor_config.floor_plan_rotation_deg = Some(fp_rot);
                            active.indoor_config.floor_plan_opacity = Some(fp_op);
                        }
                    }
                }
            }
        });
    });

    inspector_card(ui, "Scene Statistics", true, |ui| {
        // Mirror exact grouping & counts shown in Left Panel -> Layers
        let unit_count = state
            .markers
            .iter()
            .filter(|m| !is_basic_mesh_marker(m))
            .count();
        let mesh_count = state.markers.len().saturating_sub(unit_count);
        let bldg_mesh_count = mesh_count + state.buildings.len();
        let line_count = state.lines.len();
        let text_count = state.text_markers.len();

        let mut route_count = 0usize;
        let mut shoot_count = 0usize;
        let mut particle_count = 0usize;
        let mut other_count = 0usize;
        let mut spawn_hide_count = 0usize;
        let mut media_count = 0usize;
        let mut camera_count = 0usize;
        let mut env_count = 0usize;
        let mut group_count = 0usize;
        for a in &state.anims {
            match AnimCategory::from_anim_type(&a.anim_type) {
                AnimCategory::Route => route_count += 1,
                AnimCategory::Shoot => shoot_count += 1,
                AnimCategory::Particle => particle_count += 1,
                AnimCategory::Other => other_count += 1,
                AnimCategory::SpawnHide => spawn_hide_count += 1,
                AnimCategory::Audio => media_count += 1,
                AnimCategory::Camera => camera_count += 1,
                AnimCategory::Environment => env_count += 1,
                AnimCategory::Group => group_count += 1,
            }
        }

        let total_objects = unit_count + bldg_mesh_count + line_count + text_count;
        let total_anims = state.anims.len();

        ui.label(
            egui::RichText::new(format!("Objek Peta ({total_objects})"))
                .size(10.5)
                .strong()
                .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        ui.columns(2, |cols| {
            cols[0].label(format!("3D Units: {unit_count}"));
            cols[1].label(format!("Buildings/Mesh: {bldg_mesh_count}"));
            cols[0].label(format!("Plot Lines: {line_count}"));
            cols[1].label(format!("Text Labels: {text_count}"));
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);

        ui.label(
            egui::RichText::new(format!("Animasi ({total_anims})"))
                .size(10.5)
                .strong()
                .color(egui::Color32::from_rgb(148, 163, 184)),
        );
        ui.columns(2, |cols| {
            cols[0].label(format!("Route: {route_count}"));
            cols[1].label(format!("Shoot: {shoot_count}"));
            cols[0].label(format!("Particle: {particle_count}"));
            cols[1].label(format!("Penanda/Lainnya: {other_count}"));
            cols[0].label(format!("Spawn & Hide: {spawn_hide_count}"));
            cols[1].label(format!("Media: {media_count}"));
            cols[0].label(format!("Kamera: {camera_count}"));
            cols[1].label(format!("Lingkungan: {env_count}"));
            cols[0].label(format!("Grup Animasi: {group_count}"));
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
                let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                    tcam.target_local.x,
                    0.0,
                    tcam.target_local.y,
                ]);
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
        &format!(
            "Live Ops WebSocket ({} online)",
            state.online_operators.len()
        ),
        false,
        |ui| {
            if state.can_control_active_big_screen()
                && let Some(sk) = &state.active_skenario
            {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Sync Camera").clicked() {
                        let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([
                            tcam.target_local.x,
                            0.0,
                            tcam.target_local.y,
                        ]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_preview_card_renders_progress_bar_without_nan_panic() {
        let now = chrono::Utc::now();
        let route_id = Uuid::new_v4();
        let marker_id = Uuid::new_v4();
        let route_anim = twg_shared::models::Anim {
            id: route_id,
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: Some(marker_id),
            name: "Rute Uji".into(),
            anim_type: "route".into(),
            sort_order: 0,
            speed: 10.0,
            duration_sec: 10.0,
            replay: false,
            autoplay: false,
            geom: vec![[115.216, -8.670, 0.0], [115.217, -8.671, 0.0]],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };

        let mut state = AppState::new(std::env::temp_dir());
        let mut playback = PlaybackState::default();
        playback.start_editor_preview(route_id);
        let mut route_edit = RouteEditState::default();

        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::Panel::right("test_right_panel")
                .default_size(320.0)
                .show(ui, |ui| {
                    draw_editor_preview_card(
                        ui,
                        &mut state,
                        &mut playback,
                        &mut route_edit,
                        &route_anim,
                    );
                });
        });
        out.textures_delta.clear();
    }
}
