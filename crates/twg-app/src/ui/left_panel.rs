use bevy_egui::egui;
use twg_shared::{
    api::{CreateAnimRequest, UpdateMarkerRequest},
    group_anim::{
        GroupAnimConfig, GroupAnimMode, group_anim_mode, is_group_anim, parse_group_anim_config,
    },
    models::Anim,
    particle_anim::{
        ParticleAnimKind, is_particle_anim, parse_particle_anim_config, particle_anim_position,
    },
    pointer_anim::{is_pointer_arrow_anim, parse_pointer_arrow_config},
    route_formation::{
        RouteFormationKind, anim_bound_marker_ids, anim_is_playable, is_multi_route_anim,
        parse_multi_route_config,
    },
    shoot_anim::{
        ShootAnimKind, parse_apar_spray_config, parse_bullet_shoot_config,
        parse_stone_throw_config, parse_tear_gas_config, parse_water_cannon_config,
        shoot_anim_kind, shoot_anim_range_m,
    },
    spawn_hide_anim::{
        SpawnHideAction, SpawnHideConfig, is_spawn_hide_anim, parse_spawn_hide_config,
        spawn_hide_action,
    },
    traffic_anim::{is_traffic_anim, is_traffic_signal_anim, parse_traffic_anim_config},
    traffic_diversion::{is_traffic_diversion_anim, parse_traffic_diversion_config},
    traffic_queue::{
        is_traffic_gridlock_anim, is_traffic_manual_anim, is_traffic_queue_anim,
        parse_traffic_queue_config,
    },
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    net::client::{ApiClientBridge, ApiCommand},
    scene::{
        basic_mesh::{BasicMeshShape, is_basic_mesh_marker, parse_basic_mesh_config},
        markers::{current_auth, spawn_basic_mesh_at_map_center, spawn_marker_at_map_center},
        playback::route_total_length_m,
    },
    state::{
        AnimCategory, AppState, LayerLockFilter, LayerVisibilityFilter, LeftTab, PlaybackState,
        SelectedEntity, ToolMode,
    },
    ui::draw_search_input_with_reset,
};

pub fn draw_left_panel(
    viewport_ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
) -> f32 {
    egui::Panel::left("editor_left_dock")
        .resizable(true)
        .default_size(310.0)
        .min_size(255.0)
        .max_size(440.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 23, 35))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::same(8)),
        )
        .show(viewport_ui, |ui| {
            // Godot / Unity Dock Tab Bar: Layers | Catalog | Mesh | Animasi
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(12, 17, 27))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::same(3))
                .show(ui, |ui| {
                    ui.columns(4, |cols| {
                        for (idx, (tab, title, active_fill)) in [
                            (
                                LeftTab::Layers,
                                "Layers",
                                egui::Color32::from_rgb(14, 116, 144),
                            ),
                            (
                                LeftTab::Catalog,
                                "Catalog",
                                egui::Color32::from_rgb(14, 116, 144),
                            ),
                            (
                                LeftTab::BasicMesh,
                                "Mesh",
                                egui::Color32::from_rgb(109, 40, 217),
                            ),
                            (
                                LeftTab::Animations,
                                "Animasi",
                                egui::Color32::from_rgb(5, 150, 105),
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let is_active = state.left_tab == tab;
                            let btn = egui::Button::new(
                                egui::RichText::new(title).size(10.5).strong().color(
                                    if is_active {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(148, 163, 184)
                                    },
                                ),
                            )
                            .fill(if is_active {
                                active_fill
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .min_size(egui::vec2(cols[idx].available_width(), 25.0));
                            if cols[idx].add(btn).clicked() {
                                state.left_tab = tab;
                            }
                        }
                    });
                });

            ui.add_space(6.0);

            match state.left_tab {
                LeftTab::Layers => draw_layers_tab(ui, state, tcam, playback, api),
                LeftTab::Catalog => draw_catalog_tab(ui, state, tcam, playback, api),
                LeftTab::BasicMesh => draw_basic_mesh_tab(ui, state, tcam, playback, api),
                LeftTab::Animations => draw_animations_tab(ui, state, tcam, playback, api),
            }
        })
        .response
        .rect
        .width()
}

fn draw_eye_toggle_btn(ui: &mut egui::Ui, visible: bool, enabled: bool) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if enabled && resp.hovered() {
            p.rect_filled(
                rect,
                3.0,
                egui::Color32::from_rgba_unmultiplied(148, 163, 184, 34),
            );
        }
        let c = rect.center();
        let col = if !enabled {
            egui::Color32::from_rgb(71, 85, 105)
        } else if visible {
            egui::Color32::from_rgb(74, 222, 128)
        } else {
            egui::Color32::from_rgb(100, 116, 139)
        };
        let stroke = egui::Stroke::new(1.3, col);
        let left = egui::pos2(c.x - 6.0, c.y);
        let right = egui::pos2(c.x + 6.0, c.y);
        let top_pts = vec![
            left,
            egui::pos2(c.x - 3.0, c.y - 3.2),
            egui::pos2(c.x, c.y - 4.0),
            egui::pos2(c.x + 3.0, c.y - 3.2),
            right,
        ];
        let bot_pts = vec![
            left,
            egui::pos2(c.x - 3.0, c.y + 3.2),
            egui::pos2(c.x, c.y + 4.0),
            egui::pos2(c.x + 3.0, c.y + 3.2),
            right,
        ];
        p.add(egui::Shape::line(top_pts, stroke));
        p.add(egui::Shape::line(bot_pts, stroke));
        if visible {
            p.circle_filled(c, 2.0, col);
        } else {
            p.circle_stroke(c, 1.7, egui::Stroke::new(1.0, col));
            let slash_col = if enabled {
                egui::Color32::from_rgb(248, 113, 113)
            } else {
                egui::Color32::from_rgb(100, 116, 139)
            };
            p.line_segment(
                [
                    egui::pos2(c.x - 5.2, c.y - 5.2),
                    egui::pos2(c.x + 5.2, c.y + 5.2),
                ],
                egui::Stroke::new(1.5, slash_col),
            );
        }
    }
    resp.on_hover_text(if visible {
        "Tampil (klik untuk sembunyikan / Hide)"
    } else {
        "Tersembunyi (klik untuk tampilkan / Show)"
    })
}

fn draw_lock_toggle_btn(ui: &mut egui::Ui, locked: bool, enabled: bool) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if enabled && resp.hovered() {
            p.rect_filled(
                rect,
                3.0,
                egui::Color32::from_rgba_unmultiplied(148, 163, 184, 34),
            );
        }
        let c = rect.center();
        let col = if !enabled {
            egui::Color32::from_rgb(71, 85, 105)
        } else if locked {
            egui::Color32::from_rgb(251, 191, 36)
        } else {
            egui::Color32::from_rgb(100, 116, 139)
        };
        let body = egui::Rect::from_min_max(
            egui::pos2(c.x - 4.5, c.y - 0.5),
            egui::pos2(c.x + 4.5, c.y + 5.8),
        );
        if locked {
            p.rect_filled(body, 1.5, col);
            // Closed shackle
            let shackle = vec![
                egui::pos2(c.x - 2.8, c.y - 0.5),
                egui::pos2(c.x - 2.8, c.y - 3.6),
                egui::pos2(c.x - 1.4, c.y - 5.0),
                egui::pos2(c.x + 1.4, c.y - 5.0),
                egui::pos2(c.x + 2.8, c.y - 3.6),
                egui::pos2(c.x + 2.8, c.y - 0.5),
            ];
            p.add(egui::Shape::line(shackle, egui::Stroke::new(1.5, col)));
            // Keyhole dot
            p.circle_filled(
                egui::pos2(c.x, c.y + 2.5),
                1.2,
                egui::Color32::from_rgb(17, 23, 35),
            );
        } else {
            p.rect_stroke(
                body,
                1.5,
                egui::Stroke::new(1.2, col),
                egui::StrokeKind::Middle,
            );
            // Open shackle (lifted slightly on the right)
            let shackle = vec![
                egui::pos2(c.x - 2.8, c.y - 0.5),
                egui::pos2(c.x - 2.8, c.y - 4.2),
                egui::pos2(c.x - 1.2, c.y - 5.5),
                egui::pos2(c.x + 1.6, c.y - 5.5),
                egui::pos2(c.x + 3.0, c.y - 4.0),
            ];
            p.add(egui::Shape::line(shackle, egui::Stroke::new(1.3, col)));
        }
    }
    resp.on_hover_text(if locked {
        "Terkunci (klik untuk Unlock)"
    } else {
        "Terbuka (klik untuk Lock)"
    })
}

fn draw_filter_pill(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    active_fill: egui::Color32,
) -> bool {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(10.0)
            .strong()
            .color(if active {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(148, 163, 184)
            }),
    )
    .fill(if active {
        active_fill
    } else {
        egui::Color32::from_rgb(24, 33, 49)
    })
    .stroke(egui::Stroke::new(
        1.0,
        if active {
            active_fill
        } else {
            egui::Color32::from_rgb(44, 58, 82)
        },
    ))
    .corner_radius(3.0)
    .min_size(egui::vec2(0.0, 19.0));
    ui.add(btn).clicked()
}

fn draw_layers_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
) {
    let is_play_mode = playback.play_test_mode;

    // Scene Hierarchy search + Lock/Hide filter bar
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.layer_search)
                .hint_text("Filter scene nodes / layers...")
                .desired_width(ui.available_width() - 28.0),
        );
        if ui
            .add_enabled(
                !state.layer_search.is_empty(),
                egui::Button::new("X").small(),
            )
            .on_hover_text("Hapus kata kunci")
            .clicked()
        {
            state.layer_search.clear();
        }
    });
    ui.add_space(4.0);

    let has_active_filter = !state.layer_search.is_empty()
        || state.layer_vis_filter != LayerVisibilityFilter::All
        || state.layer_lock_filter != LayerLockFilter::All;

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(13, 19, 30))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59)))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(
                    egui::RichText::new("Tampil:")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if draw_filter_pill(
                    ui,
                    "Semua",
                    state.layer_vis_filter == LayerVisibilityFilter::All,
                    egui::Color32::from_rgb(14, 116, 144),
                ) {
                    state.layer_vis_filter = LayerVisibilityFilter::All;
                }
                if draw_filter_pill(
                    ui,
                    "Show",
                    state.layer_vis_filter == LayerVisibilityFilter::VisibleOnly,
                    egui::Color32::from_rgb(5, 150, 105),
                ) {
                    state.layer_vis_filter = LayerVisibilityFilter::VisibleOnly;
                }
                if draw_filter_pill(
                    ui,
                    "Hide",
                    state.layer_vis_filter == LayerVisibilityFilter::HiddenOnly,
                    egui::Color32::from_rgb(185, 28, 28),
                ) {
                    state.layer_vis_filter = LayerVisibilityFilter::HiddenOnly;
                }
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(
                    egui::RichText::new("Kunci: ")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                if draw_filter_pill(
                    ui,
                    "Semua",
                    state.layer_lock_filter == LayerLockFilter::All,
                    egui::Color32::from_rgb(14, 116, 144),
                ) {
                    state.layer_lock_filter = LayerLockFilter::All;
                }
                if draw_filter_pill(
                    ui,
                    "Buka",
                    state.layer_lock_filter == LayerLockFilter::UnlockedOnly,
                    egui::Color32::from_rgb(37, 99, 235),
                ) {
                    state.layer_lock_filter = LayerLockFilter::UnlockedOnly;
                }
                if draw_filter_pill(
                    ui,
                    "Lock",
                    state.layer_lock_filter == LayerLockFilter::LockedOnly,
                    egui::Color32::from_rgb(180, 83, 9),
                ) {
                    state.layer_lock_filter = LayerLockFilter::LockedOnly;
                }
                if has_active_filter {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Reset")
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(248, 113, 113)),
                                )
                                .frame(false)
                                .small(),
                            )
                            .on_hover_text("Reset semua filter layer")
                            .clicked()
                        {
                            state.layer_search.clear();
                            state.layer_vis_filter = LayerVisibilityFilter::All;
                            state.layer_lock_filter = LayerLockFilter::All;
                        }
                    });
                }
            });
        });
    ui.add_space(6.0);

    // Root Scene Node (Godot/Unity Root)
    let root_selected = state.selected == SelectedEntity::None;
    let sk_title = state
        .active_skenario
        .as_ref()
        .map(|s| s.name.as_str())
        .unwrap_or("Scene Root");
    let root_bg = if root_selected {
        egui::Color32::from_rgb(14, 78, 108)
    } else {
        egui::Color32::from_rgb(23, 32, 48)
    };
    egui::Frame::new()
        .fill(root_bg)
        .stroke(egui::Stroke::new(
            1.0,
            if root_selected {
                egui::Color32::from_rgb(56, 189, 248)
            } else {
                egui::Color32::from_rgb(38, 52, 74)
            },
        ))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new(format!("[Scene] {sk_title}"))
                                .size(12.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .truncate()
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    state.selected = SelectedEntity::None;
                    state.marker_edit = None;
                    state.building_edit = None;
                    state.line_edit = None;
                    state.text_edit = None;
                    state.anim_edit = None;
                }
            });
        });

    ui.add_space(6.0);

    let filter = state.layer_search.trim().to_lowercase();
    let mut fly_to: Option<(f64, f64)> = None;
    let mut toggle_marker_vis: Option<uuid::Uuid> = None;
    let mut toggle_marker_lock: Option<(uuid::Uuid, bool)> = None;
    let mut toggle_hidden_id: Option<uuid::Uuid> = None;
    let mut toggle_locked_id: Option<uuid::Uuid> = None;
    let mut delete_marker_id: Option<uuid::Uuid> = None;
    let mut delete_bldg_id: Option<uuid::Uuid> = None;
    let mut delete_line_id: Option<uuid::Uuid> = None;
    let mut delete_tm_id: Option<uuid::Uuid> = None;
    let mut delete_anim_id: Option<uuid::Uuid> = None;
    let mut quick_play_anim_id: Option<uuid::Uuid> = None;

    let unit_markers: Vec<twg_shared::models::Marker> = state
        .markers
        .iter()
        .filter(|m| !is_basic_mesh_marker(m))
        .cloned()
        .collect();
    let mesh_markers: Vec<twg_shared::models::Marker> = state
        .markers
        .iter()
        .filter(|m| is_basic_mesh_marker(m))
        .cloned()
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("godot_layers_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // 1. 3D Units / Markers (excluding Basic Mesh)
            let unit_total = unit_markers.len();
            let unit_filtered_count = unit_markers
                .iter()
                .filter(|m| {
                    let locked = state.is_marker_locked(m);
                    state.matches_layer_filters(m.is_spawn, locked)
                        && (filter.is_empty()
                            || m.name.to_lowercase().contains(&filter)
                            || m.category.to_lowercase().contains(&filter))
                })
                .count();
            let unit_header = if has_active_filter {
                format!("3D Units / Markers ({unit_filtered_count}/{unit_total})")
            } else {
                format!("3D Units / Markers ({unit_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(unit_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(186, 230, 253)),
            )
            .id_salt("layer_group_markers")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for m in &unit_markers {
                    let locked = state.is_marker_locked(m);
                    if !state.matches_layer_filters(m.is_spawn, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !m.name.to_lowercase().contains(&filter)
                        && !m.category.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Marker(id) if id == m.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, m.is_spawn, !is_play_mode).clicked() {
                                    toggle_marker_vis = Some(m.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_marker_lock = Some((m.id, !locked));
                                }

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !m.is_spawn {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let name_resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&m.name).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if name_resp.clicked() {
                                    state.selected = SelectedEntity::Marker(m.id);
                                    state.marker_edit = Some(m.clone());
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    state.anim_edit = None;
                                }
                                if name_resp.double_clicked() {
                                    fly_to = Some((m.lng, m.lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus unit 3D"
                                                })
                                                .clicked()
                                            {
                                                delete_marker_id = Some(m.id);
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Go").size(10.5).color(
                                                        egui::Color32::from_rgb(125, 211, 252),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Fokus kamera ke unit")
                                            .clicked()
                                        {
                                            fly_to = Some((m.lng, m.lat));
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (kosong)"
                        })
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 2. Buildings / Mesh (Basic Mesh + Extruded Buildings)
            let bldgs = state.buildings.clone();
            let bm_total = mesh_markers.len() + bldgs.len();
            let bm_filtered_count = mesh_markers
                .iter()
                .filter(|m| {
                    let locked = state.is_marker_locked(m);
                    state.matches_layer_filters(m.is_spawn, locked)
                        && (filter.is_empty()
                            || m.name.to_lowercase().contains(&filter)
                            || "mesh".contains(&filter))
                })
                .count()
                + bldgs
                    .iter()
                    .filter(|b| {
                        let vis = !state.hidden_entity_ids.contains(&b.id);
                        let locked = state.locked_entity_ids.contains(&b.id);
                        state.matches_layer_filters(vis, locked)
                            && (filter.is_empty()
                                || b.name.to_lowercase().contains(&filter)
                                || "building".contains(&filter))
                    })
                    .count();
            let bm_header = if has_active_filter {
                format!("Buildings / Mesh ({bm_filtered_count}/{bm_total})")
            } else {
                format!("Buildings / Mesh ({bm_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(bm_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(167, 243, 208)),
            )
            .id_salt("layer_group_buildings")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;

                // 2a. Basic Mesh markers
                for m in &mesh_markers {
                    let locked = state.is_marker_locked(m);
                    if !state.matches_layer_filters(m.is_spawn, locked) {
                        continue;
                    }
                    let shape_tag = parse_basic_mesh_config(m)
                        .map(|c| c.shape.tag())
                        .unwrap_or("MSH");
                    if !filter.is_empty()
                        && !m.name.to_lowercase().contains(&filter)
                        && !shape_tag.to_lowercase().contains(&filter)
                        && !"mesh".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Marker(id) if id == m.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, m.is_spawn, !is_play_mode).clicked() {
                                    toggle_marker_vis = Some(m.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_marker_lock = Some((m.id, !locked));
                                }

                                ui.label(
                                    egui::RichText::new(format!("[{shape_tag}]"))
                                        .size(9.5)
                                        .strong()
                                        .color(if sel {
                                            egui::Color32::from_rgb(233, 213, 255)
                                        } else if !m.is_spawn {
                                            egui::Color32::from_rgb(100, 116, 139)
                                        } else {
                                            egui::Color32::from_rgb(196, 181, 253)
                                        }),
                                );

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !m.is_spawn {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&m.name).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Marker(m.id);
                                    state.marker_edit = Some(m.clone());
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    state.anim_edit = None;
                                }
                                if resp.double_clicked() {
                                    fly_to = Some((m.lng, m.lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus basic mesh"
                                                })
                                                .clicked()
                                            {
                                                delete_marker_id = Some(m.id);
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Go").size(10.5).color(
                                                        egui::Color32::from_rgb(125, 211, 252),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Fokus kamera ke mesh")
                                            .clicked()
                                        {
                                            fly_to = Some((m.lng, m.lat));
                                        }
                                    },
                                );
                            });
                        });
                }

                // 2b. Extruded Buildings
                for b in &bldgs {
                    let vis = !state.hidden_entity_ids.contains(&b.id);
                    let locked = state.locked_entity_ids.contains(&b.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !b.name.to_lowercase().contains(&filter)
                        && !"building".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Building(id) if id == b.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(b.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(b.id);
                                }

                                ui.label(egui::RichText::new("[BLD]").size(9.5).strong().color(
                                    if sel {
                                        egui::Color32::from_rgb(167, 243, 208)
                                    } else if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        egui::Color32::from_rgb(52, 211, 153)
                                    },
                                ));

                                let label_txt = format!("{} ({:.0}m)", b.name, b.height);
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(label_txt).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Building(b.id);
                                    state.building_edit = Some(b.clone());
                                    state.marker_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    state.anim_edit = None;
                                }
                                if resp.double_clicked()
                                    && let Some(pt) = b.geom.first()
                                {
                                    fly_to = Some((pt[0], pt[1]));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus bangunan"
                                                })
                                                .clicked()
                                            {
                                                delete_bldg_id = Some(b.id);
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Go").size(10.5).color(
                                                        egui::Color32::from_rgb(125, 211, 252),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Fokus kamera ke bangunan")
                                            .clicked()
                                            && let Some(pt) = b.geom.first()
                                        {
                                            fly_to = Some((pt[0], pt[1]));
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (kosong)"
                        })
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 3. Plot Lines
            let lines = state.lines.clone();
            let line_total = lines.len();
            let line_filtered = lines
                .iter()
                .filter(|l| {
                    let vis = !state.hidden_entity_ids.contains(&l.id);
                    let locked = state.locked_entity_ids.contains(&l.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty() || l.name.to_lowercase().contains(&filter))
                })
                .count();
            let line_header = if has_active_filter {
                format!("Plot Lines ({line_filtered}/{line_total})")
            } else {
                format!("Plot Lines ({line_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(line_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(253, 230, 138)),
            )
            .id_salt("layer_group_lines")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for l in &lines {
                    let vis = !state.hidden_entity_ids.contains(&l.id);
                    let locked = state.locked_entity_ids.contains(&l.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty() && !l.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Line(id) if id == l.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(l.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(l.id);
                                }

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "{} ({} pt)",
                                            l.name,
                                            l.geom.len()
                                        ))
                                        .size(11.5)
                                        .color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Line(l.id);
                                    state.line_edit = Some(l.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.text_edit = None;
                                    state.anim_edit = None;
                                }
                                if resp.double_clicked()
                                    && let Some(pt) = l.geom.first()
                                {
                                    fly_to = Some((pt[0], pt[1]));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus garis"
                                                })
                                                .clicked()
                                            {
                                                delete_line_id = Some(l.id);
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Go").size(10.5).color(
                                                        egui::Color32::from_rgb(125, 211, 252),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .clicked()
                                            && let Some(pt) = l.geom.first()
                                        {
                                            fly_to = Some((pt[0], pt[1]));
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (kosong)"
                        })
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 4. Text Labels
            let tms = state.text_markers.clone();
            let tm_total = tms.len();
            let tm_filtered = tms
                .iter()
                .filter(|t| {
                    let vis = !state.hidden_entity_ids.contains(&t.id);
                    let locked = state.locked_entity_ids.contains(&t.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty() || t.text.to_lowercase().contains(&filter))
                })
                .count();
            let tm_header = if has_active_filter {
                format!("Text Labels ({tm_filtered}/{tm_total})")
            } else {
                format!("Text Labels ({tm_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(tm_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(221, 214, 254)),
            )
            .id_salt("layer_group_text")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for t in &tms {
                    let vis = !state.hidden_entity_ids.contains(&t.id);
                    let locked = state.locked_entity_ids.contains(&t.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty() && !t.text.to_lowercase().contains(&filter) {
                        continue;
                    }
                    shown += 1;
                    let sel =
                        matches!(state.selected, SelectedEntity::TextMarker(id) if id == t.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(t.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(t.id);
                                }

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&t.text).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::TextMarker(t.id);
                                    state.text_edit = Some(t.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.anim_edit = None;
                                }
                                if resp.double_clicked() {
                                    fly_to = Some((t.lng, t.lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus label teks"
                                                })
                                                .clicked()
                                            {
                                                delete_tm_id = Some(t.id);
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Go").size(10.5).color(
                                                        egui::Color32::from_rgb(125, 211, 252),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            fly_to = Some((t.lng, t.lat));
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (kosong)"
                        })
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 5. Route Animations (Category: Route)
            let route_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Route)
                .cloned()
                .collect();
            let anim_total = route_anims.len();
            let anim_filtered = route_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty() || a.name.to_lowercase().contains(&filter))
                })
                .count();
            let anim_header = if has_active_filter {
                format!("Route Animations ({anim_filtered}/{anim_total})")
            } else {
                format!("Route Animations ({anim_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(anim_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 231, 183)),
            )
            .id_salt("layer_group_anims")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &route_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty() && !a.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "{} ({:.0}s)",
                                            a.name, a.duration_sec
                                        ))
                                        .size(11.5)
                                        .color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                }
                                if resp.double_clicked()
                                    && let Some(pt) = a.geom.first()
                                {
                                    fly_to = Some((pt[0], pt[1]));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus rute animasi"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Play").size(10.5).color(
                                                        egui::Color32::from_rgb(52, 211, 153),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Test animasi ini sekarang")
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada rute — buat di tab Animasi -> Rute)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 6. Shoot Animations (Water Cannon, Lempar Batu, Gas Air Mata, Peluru, APAR)
            let shoot_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Shoot)
                .cloned()
                .collect();
            let shoot_total = shoot_anims.len();
            let shoot_filtered = shoot_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    let sk = shoot_anim_kind(a).unwrap_or_default();
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || sk.label().to_lowercase().contains(&filter)
                            || "shoot".contains(&filter))
                })
                .count();
            let shoot_header = if has_active_filter {
                format!("Shoot Animations ({shoot_filtered}/{shoot_total})")
            } else {
                format!("Shoot Animations ({shoot_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(shoot_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(125, 211, 252)),
            )
            .id_salt("layer_group_shoot_anims")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &shoot_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    let sk = shoot_anim_kind(a).unwrap_or_default();
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !sk.label().to_lowercase().contains(&filter)
                        && !"shoot".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let bound_coords = a
                        .marker_id
                        .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                        .map(|m| (m.lng, m.lat));
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }

                                let tag_col = match sk {
                                    ShootAnimKind::WaterCannon => {
                                        egui::Color32::from_rgb(56, 189, 248)
                                    }
                                    ShootAnimKind::StoneThrow => {
                                        egui::Color32::from_rgb(251, 191, 36)
                                    }
                                    ShootAnimKind::TearGas => {
                                        egui::Color32::from_rgb(203, 213, 225)
                                    }
                                    ShootAnimKind::Bullet => egui::Color32::from_rgb(248, 113, 113),
                                    ShootAnimKind::Apar => egui::Color32::from_rgb(52, 211, 153),
                                };
                                ui.label(
                                    egui::RichText::new(format!("[{}]", sk.short_tag()))
                                        .size(8.5)
                                        .strong()
                                        .color(if !vis {
                                            egui::Color32::from_rgb(100, 116, 139)
                                        } else {
                                            tag_col
                                        }),
                                );

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let range_m = shoot_anim_range_m(a);
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "{} ({:.0}m)",
                                            a.name, range_m
                                        ))
                                        .size(11.5)
                                        .color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    state.shoot_preview_reset_nonce =
                                        state.shoot_preview_reset_nonce.wrapping_add(1);
                                }
                                if resp.double_clicked()
                                    && let Some((lng, lat)) = bound_coords
                                {
                                    fly_to = Some((lng, lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus animasi shoot"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play").size(10.5).color(
                                                        egui::Color32::from_rgb(56, 189, 248),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Play animasi shoot di Play Skenario")
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada shoot — buat di tab Animasi -> Shoot)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 7. Particle Animations (Api, Asap, Ledakan)
            let particle_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Particle)
                .cloned()
                .collect();
            let particle_total = particle_anims.len();
            let particle_filtered = particle_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    let cfg = parse_particle_anim_config(a);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || cfg.kind.label().to_lowercase().contains(&filter)
                            || "particle".contains(&filter))
                })
                .count();
            let particle_header = if has_active_filter {
                format!("Particle Animations ({particle_filtered}/{particle_total})")
            } else {
                format!("Particle Animations ({particle_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(particle_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(253, 186, 116)),
            )
            .id_salt("layer_group_particle_anims")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &particle_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    let cfg = parse_particle_anim_config(a);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !cfg.kind.label().to_lowercase().contains(&filter)
                        && !"particle".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let (tag, tag_col) = match cfg.kind {
                        ParticleAnimKind::Fire => ("API", egui::Color32::from_rgb(251, 146, 60)),
                        ParticleAnimKind::Smoke => ("ASAP", egui::Color32::from_rgb(203, 213, 225)),
                        ParticleAnimKind::Explosion => {
                            ("LED", egui::Color32::from_rgb(248, 113, 113))
                        }
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }

                                ui.label(
                                    egui::RichText::new(format!("[{tag}]"))
                                        .size(9.0)
                                        .strong()
                                        .color(if !vis {
                                            egui::Color32::from_rgb(100, 116, 139)
                                        } else {
                                            tag_col
                                        }),
                                );

                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    state.particle_preview_reset_nonce =
                                        state.particle_preview_reset_nonce.wrapping_add(1);
                                }
                                if resp.double_clicked() && !a.geom.is_empty() {
                                    let [lng, lat, _] = particle_anim_position(a);
                                    fly_to = Some((lng, lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus animasi partikel"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play").size(10.5).color(
                                                        egui::Color32::from_rgb(251, 146, 60),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Play animasi partikel di Play Skenario")
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada partikel — buat di tab Animasi -> Particle)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 8. Other / Pointer Animations (Panah Penunjuk 3D, dll.)
            let other_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Other)
                .cloned()
                .collect();
            let other_total = other_anims.len();
            let other_filtered = other_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || "panah".contains(&filter)
                            || "pointer".contains(&filter))
                })
                .count();
            let other_header = if has_active_filter {
                format!("Penanda / Lainnya ({other_filtered}/{other_total})")
            } else {
                format!("Penanda / Lainnya ({other_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(other_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(254, 240, 138)),
            )
            .id_salt("layer_group_other_anims")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &other_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !"panah".contains(&filter)
                        && !"pointer".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let bound_coords = a
                        .marker_id
                        .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                        .map(|m| (m.lng, m.lat));
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }
                                ui.label(egui::RichText::new("[PANAH]").size(8.5).strong().color(
                                    if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        egui::Color32::from_rgb(250, 204, 21)
                                    },
                                ));
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                }
                                if resp.double_clicked()
                                    && let Some((lng, lat)) = bound_coords
                                {
                                    fly_to = Some((lng, lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus animasi panah penunjuk"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play").size(10.5).color(
                                                        egui::Color32::from_rgb(250, 204, 21),
                                                    ),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text(
                                                "Play panah penunjuk 3D di Play Skenario",
                                            )
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada penanda — buat di tab Animasi -> Lainnya)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 9. Spawn & Hide Animations
            let sh_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::SpawnHide)
                .cloned()
                .collect();
            let sh_total = sh_anims.len();
            let sh_filtered = sh_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || "spawn".contains(&filter)
                            || "hide".contains(&filter))
                })
                .count();
            let sh_header = if has_active_filter {
                format!("Spawn & Hide ({sh_filtered}/{sh_total})")
            } else {
                format!("Spawn & Hide ({sh_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(sh_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(196, 181, 253)),
            )
            .id_salt("layer_group_spawn_hide_anims")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &sh_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !"spawn".contains(&filter)
                        && !"hide".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let act = spawn_hide_action(a).unwrap_or_default();
                    let bound_coords = anim_bound_marker_ids(a)
                        .first()
                        .and_then(|mid| state.markers.iter().find(|m| m.id == *mid))
                        .map(|m| (m.lng, m.lat));
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }
                                let (tag_txt, tag_col) = match act {
                                    SpawnHideAction::Spawn => {
                                        ("[SPAWN]", egui::Color32::from_rgb(52, 211, 153))
                                    }
                                    SpawnHideAction::Hide => {
                                        ("[HIDE]", egui::Color32::from_rgb(251, 113, 133))
                                    }
                                };
                                ui.label(egui::RichText::new(tag_txt).size(8.5).strong().color(
                                    if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        tag_col
                                    },
                                ));
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                }
                                if resp.double_clicked()
                                    && let Some((lng, lat)) = bound_coords
                                {
                                    fly_to = Some((lng, lat));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus animasi Spawn / Hide"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play")
                                                        .size(10.5)
                                                        .color(tag_col),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text(
                                                "Play animasi Spawn / Hide di Play Skenario",
                                            )
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada animasi — buat di tab Animasi -> Spawn & Hide)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 7. Media (Play Suara & Billboard / Videotron 3D)
            let media_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Audio)
                .cloned()
                .collect();
            let media_total = media_anims.len();
            let media_filtered = media_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || "media".contains(&filter)
                            || "audio play suara".contains(&filter)
                            || "billboard videotron video gambar".contains(&filter))
                })
                .count();
            let media_header = if has_active_filter {
                format!("Media ({media_filtered}/{media_total})")
            } else {
                format!("Media ({media_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(media_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(192, 132, 252)),
            )
            .id_salt("media_layers")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &media_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !"media".contains(&filter)
                        && !"audio play suara".contains(&filter)
                        && !"billboard videotron video gambar".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let (tag_txt, tag_col) = if twg_shared::billboard_anim::is_billboard_anim(a) {
                        if twg_shared::billboard_anim::parse_billboard_anim_config(a).media_mode
                            == twg_shared::billboard_anim::BillboardMediaMode::Video
                        {
                            ("[VIDEO]", egui::Color32::from_rgb(56, 189, 248))
                        } else {
                            ("[IMG]", egui::Color32::from_rgb(45, 212, 191))
                        }
                    } else {
                        ("[SUARA]", egui::Color32::from_rgb(192, 132, 252))
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }
                                ui.label(egui::RichText::new(tag_txt).size(8.5).strong().color(
                                    if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        tag_col
                                    },
                                ));
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.0).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    if let Some(pt) =
                                        crate::scene::anim_primary_focus_lnglat(a, &state.anims, &state.markers)
                                    {
                                        fly_to = Some(pt);
                                    }
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").size(10.5).color(
                                                            if locked {
                                                                egui::Color32::from_rgb(71, 85, 105)
                                                            } else {
                                                                egui::Color32::from_rgb(
                                                                    248, 113, 113,
                                                                )
                                                            },
                                                        ),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus animasi Media"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if anim_is_playable(a, &state.markers)
                                            && ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("Play")
                                                            .size(10.5)
                                                            .color(tag_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text("Play animasi Media di Play Skenario")
                                                .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada media — buat di tab Animasi -> Media)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 8. Kamera (Static Camera & Camera Follow)
            let cam_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Camera)
                .cloned()
                .collect();
            let cam_total = cam_anims.len();
            let cam_filtered = cam_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || "kamera camera static follow orbit".contains(&filter))
                })
                .count();
            let cam_header = if has_active_filter {
                format!("Kamera ({cam_filtered}/{cam_total})")
            } else {
                format!("Kamera ({cam_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(cam_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(56, 189, 248)),
            )
            .id_salt("camera_layers")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &cam_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !"kamera camera static follow orbit".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let (tag_txt, tag_col) = if twg_shared::camera_anim::is_camera_static_anim(a) {
                        ("[STATIC]", egui::Color32::from_rgb(56, 189, 248))
                    } else {
                        ("[FOLLOW]", egui::Color32::from_rgb(52, 211, 153))
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }
                                ui.label(egui::RichText::new(tag_txt).size(8.5).strong().color(
                                    if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        tag_col
                                    },
                                ));
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.0).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    if let Some(pt) =
                                        crate::scene::anim_primary_focus_lnglat(a, &state.anims, &state.markers)
                                    {
                                        fly_to = Some(pt);
                                    }
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").size(10.5).color(
                                                            if locked {
                                                                egui::Color32::from_rgb(71, 85, 105)
                                                            } else {
                                                                egui::Color32::from_rgb(
                                                                    248, 113, 113,
                                                                )
                                                            },
                                                        ),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus animasi Kamera"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play")
                                                        .size(10.5)
                                                        .color(tag_col),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Play animasi Kamera di Play Skenario")
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada kamera — buat di tab Animasi -> Kamera)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });

            ui.add_space(4.0);

            // 9. Lingkungan (Laut / Sungai / Banjir & Hujan / Badai)
            let env_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Environment)
                .cloned()
                .collect();
            let env_total = env_anims.len();
            let env_filtered = env_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || "lingkungan air water laut sungai banjir hujan rain badai"
                                .contains(&filter))
                })
                .count();
            let env_header = if has_active_filter {
                format!("Lingkungan ({env_filtered}/{env_total})")
            } else {
                format!("Lingkungan ({env_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(env_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(45, 212, 191)),
            )
            .id_salt("environment_layers")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &env_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !"lingkungan air water laut sungai banjir hujan rain badai"
                            .contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let (tag_txt, tag_col) = if twg_shared::environment_anim::is_water_anim(a) {
                        match twg_shared::environment_anim::parse_water_anim_config(a).kind {
                            twg_shared::environment_anim::WaterSimKind::Ocean => {
                                ("[LAUT]", egui::Color32::from_rgb(56, 189, 248))
                            }
                            twg_shared::environment_anim::WaterSimKind::River => {
                                ("[SUNGAI]", egui::Color32::from_rgb(45, 212, 191))
                            }
                            twg_shared::environment_anim::WaterSimKind::Flood => {
                                ("[BANJIR]", egui::Color32::from_rgb(251, 191, 36))
                            }
                        }
                    } else {
                        match twg_shared::environment_anim::parse_rain_anim_config(a).preset {
                            twg_shared::environment_anim::RainPreset::Storm => {
                                ("[BADAI]", egui::Color32::from_rgb(192, 132, 252))
                            }
                            _ => ("[HUJAN]", egui::Color32::from_rgb(129, 140, 248)),
                        }
                    };
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }
                                ui.label(egui::RichText::new(tag_txt).size(8.5).strong().color(
                                    if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        tag_col
                                    },
                                ));
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.0).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                    if let Some(pt) =
                                        crate::scene::anim_primary_focus_lnglat(a, &state.anims, &state.markers)
                                    {
                                        fly_to = Some(pt);
                                    }
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").size(10.5).color(
                                                            if locked {
                                                                egui::Color32::from_rgb(71, 85, 105)
                                                            } else {
                                                                egui::Color32::from_rgb(
                                                                    248, 113, 113,
                                                                )
                                                            },
                                                        ),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus simulasi Lingkungan"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play")
                                                        .size(10.5)
                                                        .color(tag_col),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Play simulasi Lingkungan di Play Skenario")
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada simulasi — buat di tab Animasi -> Lingkungan)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });
            ui.add_space(4.0);

            // 10. Grup Animasi (Combine & Sequence)
            let grp_anims: Vec<twg_shared::models::Anim> = state
                .anims
                .iter()
                .filter(|a| AnimCategory::from_anim_type(&a.anim_type) == AnimCategory::Group)
                .cloned()
                .collect();
            let grp_total = grp_anims.len();
            let grp_filtered = grp_anims
                .iter()
                .filter(|a| {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    state.matches_layer_filters(vis, locked)
                        && (filter.is_empty()
                            || a.name.to_lowercase().contains(&filter)
                            || "combine".contains(&filter)
                            || "sequence".contains(&filter)
                            || "grup".contains(&filter))
                })
                .count();
            let grp_header = if has_active_filter {
                format!("Grup Animasi ({grp_filtered}/{grp_total})")
            } else {
                format!("Grup Animasi ({grp_total})")
            };
            egui::CollapsingHeader::new(
                egui::RichText::new(grp_header)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(165, 180, 252)),
            )
            .id_salt("layer_group_orchestration_anims")
            .default_open(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for a in &grp_anims {
                    let vis = !state.hidden_entity_ids.contains(&a.id);
                    let locked = state.locked_entity_ids.contains(&a.id);
                    if !state.matches_layer_filters(vis, locked) {
                        continue;
                    }
                    if !filter.is_empty()
                        && !a.name.to_lowercase().contains(&filter)
                        && !"combine".contains(&filter)
                        && !"sequence".contains(&filter)
                        && !"grup".contains(&filter)
                    {
                        continue;
                    }
                    shown += 1;
                    let sel = matches!(state.selected, SelectedEntity::Anim(id) if id == a.id);
                    let row_bg = if sel {
                        egui::Color32::from_rgb(14, 116, 144)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let mode = group_anim_mode(a).unwrap_or_default();
                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if draw_eye_toggle_btn(ui, vis, !is_play_mode).clicked() {
                                    toggle_hidden_id = Some(a.id);
                                }
                                if draw_lock_toggle_btn(ui, locked, !is_play_mode).clicked() {
                                    toggle_locked_id = Some(a.id);
                                }
                                let (tag_txt, tag_col) = match mode {
                                    GroupAnimMode::Combine => {
                                        ("[COMBINE]", egui::Color32::from_rgb(96, 165, 250))
                                    }
                                    GroupAnimMode::Sequence => {
                                        ("[SEQ]", egui::Color32::from_rgb(192, 132, 252))
                                    }
                                };
                                ui.label(egui::RichText::new(tag_txt).size(8.5).strong().color(
                                    if !vis {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    } else {
                                        tag_col
                                    },
                                ));
                                let name_col = if sel {
                                    egui::Color32::WHITE
                                } else if !vis {
                                    egui::Color32::from_rgb(100, 116, 139)
                                } else if locked {
                                    egui::Color32::from_rgb(253, 230, 138)
                                } else {
                                    egui::Color32::from_rgb(226, 232, 240)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&a.name).size(11.5).color(name_col),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    state.selected = SelectedEntity::Anim(a.id);
                                    state.anim_edit = Some(a.clone());
                                    state.marker_edit = None;
                                    state.building_edit = None;
                                    state.line_edit = None;
                                    state.text_edit = None;
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !is_play_mode {
                                            let del_col = if locked {
                                                egui::Color32::from_rgb(71, 85, 105)
                                            } else {
                                                egui::Color32::from_rgb(248, 113, 113)
                                            };
                                            if ui
                                                .add_enabled(
                                                    !locked,
                                                    egui::Button::new(
                                                        egui::RichText::new("x").color(del_col),
                                                    )
                                                    .small()
                                                    .frame(false),
                                                )
                                                .on_hover_text(if locked {
                                                    "Objek terkunci — buka kunci untuk menghapus"
                                                } else {
                                                    "Hapus grup animasi"
                                                })
                                                .clicked()
                                            {
                                                delete_anim_id = Some(a.id);
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                anim_is_playable(a, &state.markers),
                                                egui::Button::new(
                                                    egui::RichText::new("Play")
                                                        .size(10.5)
                                                        .color(tag_col),
                                                )
                                                .small()
                                                .frame(false),
                                            )
                                            .on_hover_text("Play grup animasi di Play Skenario")
                                            .clicked()
                                        {
                                            quick_play_anim_id = Some(a.id);
                                        }
                                    },
                                );
                            });
                        });
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new(if has_active_filter {
                            "  (tidak ada yg cocok filter)"
                        } else {
                            "  (belum ada grup — buat di tab Animasi -> Grup)"
                        })
                        .size(10.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                }
            });
        });

    // Deferred state mutations
    if let Some(id) = toggle_marker_vis
        && let Some(m) = state.markers.iter_mut().find(|x| x.id == id)
    {
        m.is_spawn = !m.is_spawn;
        let updated = m.clone();
        if state.marker_edit.as_ref().is_some_and(|e| e.id == id) {
            state.marker_edit = Some(updated.clone());
        }
        if !state.offline_preview
            && let Some(auth) = current_auth(state)
        {
            api.send(ApiCommand::UpdateMarker {
                auth,
                id,
                req: UpdateMarkerRequest {
                    name: updated.name,
                    lng: updated.lng,
                    lat: updated.lat,
                    altitude: updated.altitude,
                    rot_x: updated.rot_x,
                    rot_y: updated.rot_y,
                    rot_z: updated.rot_z,
                    scale_x: updated.scale_x,
                    scale_y: updated.scale_y,
                    scale_z: updated.scale_z,
                    is_spawn: updated.is_spawn,
                    description: updated.description,
                    metadata: updated.metadata,
                },
            });
        }
    }

    if let Some((id, locked)) = toggle_marker_lock {
        state.set_marker_locked(id, locked);
    }

    if let Some(id) = toggle_hidden_id
        && !state.hidden_entity_ids.remove(&id)
    {
        state.hidden_entity_ids.insert(id);
    }

    if let Some(id) = toggle_locked_id
        && !state.locked_entity_ids.remove(&id)
    {
        state.locked_entity_ids.insert(id);
    }

    if let Some(id) = delete_marker_id {
        state.request_delete_entity(SelectedEntity::Marker(id));
    }
    if let Some(id) = delete_bldg_id {
        state.request_delete_entity(SelectedEntity::Building(id));
    }
    if let Some(id) = delete_line_id {
        state.request_delete_entity(SelectedEntity::Line(id));
    }
    if let Some(id) = delete_tm_id {
        state.request_delete_entity(SelectedEntity::TextMarker(id));
    }
    if let Some(id) = delete_anim_id {
        state.request_delete_entity(SelectedEntity::Anim(id));
    }
    if let Some(aid) = quick_play_anim_id {
        playback.play_single_immediately(aid);
    }

    if let Some((lng, lat)) = fly_to {
        let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
        tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
    }
}

fn draw_catalog_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    playback: &PlaybackState,
    api: &ApiClientBridge,
) {
    let editable = !playback.play_test_mode;
    ui.label(
        egui::RichText::new("3D Asset Catalog")
            .size(13.0)
            .strong()
            .color(egui::Color32::WHITE),
    );
    ui.label(
        egui::RichText::new("Tarik (drag) model ke peta 3D atau klik '+ Tambah'")
            .size(10.5)
            .color(egui::Color32::from_rgb(125, 211, 252)),
    );
    ui.add_space(4.0);
    draw_search_input_with_reset(
        ui,
        &mut state.catalog_search,
        "Cari unit, kendaraan, bangunan...",
        false,
        "Hapus / reset pencarian katalog",
    );
    ui.add_space(3.0);
    ui.checkbox(&mut state.catalog_ready_only, "Hanya .glb lokal siap pakai");

    ui.horizontal_wrapped(|ui| {
        let active_all = state.catalog_category == "all";
        if ui.selectable_label(active_all, "all").clicked() {
            state.catalog_category = "all".to_string();
        }
        let cats: Vec<String> = state
            .model_categories
            .iter()
            .map(|c| c.category.clone())
            .collect();
        for cat in cats {
            let active = state.catalog_category == cat;
            if ui.selectable_label(active, &cat).clicked() {
                state.catalog_category = cat;
            }
        }
    });
    ui.separator();

    let search = state.catalog_search.to_lowercase();
    let cat_filter = state.catalog_category.clone();
    let ready_only = state.catalog_ready_only;

    let mut add_to_map: Option<twg_shared::models::ModelCatalogEntry> = None;
    let mut start_drag: Option<twg_shared::models::ModelCatalogEntry> = None;
    let mut clicked_id: Option<String> = None;

    egui::ScrollArea::vertical()
        .id_salt("godot_catalog_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let dragging_id = state.dragging_catalog_entry.as_ref().map(|e| e.id.clone());

            for entry in &state.catalog {
                if ready_only && !entry.ready {
                    continue;
                }
                if cat_filter != "all" && entry.category != cat_filter {
                    continue;
                }
                if !search.is_empty()
                    && !entry.name.to_lowercase().contains(&search)
                    && !entry.category.to_lowercase().contains(&search)
                {
                    continue;
                }

                let is_dragging = dragging_id.as_deref() == Some(entry.id.as_str());
                let selected = state.selected_catalog_id.as_deref() == Some(&entry.id);
                let card_bg = if is_dragging {
                    egui::Color32::from_rgb(12, 74, 110)
                } else if selected {
                    egui::Color32::from_rgb(20, 56, 78)
                } else {
                    egui::Color32::from_rgb(24, 33, 49)
                };
                let card_stroke = if is_dragging {
                    egui::Color32::from_rgb(34, 211, 238)
                } else if selected {
                    egui::Color32::from_rgb(56, 189, 248)
                } else {
                    egui::Color32::from_rgb(38, 52, 74)
                };

                egui::Frame::new()
                    .fill(card_bg)
                    .stroke(egui::Stroke::new(1.0, card_stroke))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(7, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let total_w = ui.available_width();
                        let btn_w = 68.0_f32;
                        let info_w = (total_w - btn_w - 6.0).max(80.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            // Left area: draggable model info (does NOT overlap the + Tambah button)
                            let info_resp = ui
                                .allocate_ui_with_layout(
                                    egui::vec2(info_w, 34.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            let tag = if entry.ready { "GLB" } else { "3D*" };
                                            let tag_col = if entry.ready {
                                                egui::Color32::from_rgb(74, 222, 128)
                                            } else {
                                                egui::Color32::from_rgb(250, 204, 21)
                                            };
                                            ui.label(
                                                egui::RichText::new(format!("[{tag}]"))
                                                    .size(9.5)
                                                    .strong()
                                                    .color(tag_col),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&entry.name)
                                                        .size(11.5)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            ui.label(
                                                egui::RichText::new(&entry.category)
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                            let anim_txt = match entry.model_type {
                                                twg_shared::models::ModelAssetType::Animated => {
                                                    "• ANIM"
                                                }
                                                twg_shared::models::ModelAssetType::Static => {
                                                    "• STATIS"
                                                }
                                            };
                                            ui.label(
                                                egui::RichText::new(anim_txt)
                                                    .size(9.5)
                                                    .color(egui::Color32::from_rgb(125, 211, 252)),
                                            );
                                        });
                                    },
                                )
                                .response;

                            let drag_resp = ui.interact(
                                info_resp.rect,
                                ui.make_persistent_id(("catalog_drag_item", &entry.id)),
                                egui::Sense::click_and_drag(),
                            );
                            if editable && drag_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if drag_resp.clicked() {
                                clicked_id = Some(entry.id.clone());
                            }
                            if drag_resp.drag_started() && editable {
                                start_drag = Some(entry.clone());
                            }
                            let _ = drag_resp.on_hover_text(if editable {
                                "Tarik (drag) ke peta 3D untuk menempatkan model ini"
                            } else {
                                "Hentikan Play Mode (Stop) untuk menambah model"
                            });

                            // Right area: prominent + Tambah button
                            let add_btn = egui::Button::new(
                                egui::RichText::new("+ Tambah").size(10.5).strong().color(
                                    if editable {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    },
                                ),
                            )
                            .fill(if editable {
                                egui::Color32::from_rgb(5, 150, 105)
                            } else {
                                egui::Color32::from_rgb(30, 41, 59)
                            })
                            .min_size(egui::vec2(btn_w, 28.0));
                            if ui
                                .add_enabled(editable, add_btn)
                                .on_hover_text("Tambahkan model 3D ini ke tengah peta")
                                .clicked()
                            {
                                add_to_map = Some(entry.clone());
                            }
                        });
                    });
                ui.add_space(4.0);
            }
        });

    if let Some(id) = clicked_id {
        state.selected_catalog_id = Some(id);
    }

    if let Some(entry) = start_drag {
        state.selected_catalog_id = Some(entry.id.clone());
        state.dragging_catalog_entry = Some(entry);
    }

    if let Some(entry) = add_to_map {
        if editable {
            if spawn_marker_at_map_center(state, tcam, api, &entry) {
                state.status_banner = Some(format!(
                    "Model '{}' ditambahkan ke peta — gunakan Move/Rotate/Scale untuk atur.",
                    entry.name
                ));
            }
        } else {
            state.status_banner =
                Some("Hentikan Play Mode (Stop) untuk menambahkan objek baru.".into());
        }
    }
}

fn draw_basic_mesh_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &TacticalCamera,
    playback: &PlaybackState,
    api: &ApiClientBridge,
) {
    let editable = !playback.play_test_mode;
    ui.label(
        egui::RichText::new("Basic Mesh 3D")
            .size(13.0)
            .strong()
            .color(egui::Color32::WHITE),
    );
    ui.label(
        egui::RichText::new("Drag ke peta 3D atau klik '+ Tambah' — dukung Color Picker, Texture & Move/Rotate/Scale")
            .size(10.5)
            .color(egui::Color32::from_rgb(196, 181, 253)),
    );
    ui.add_space(6.0);

    // Quick custom freeform polygon drawing tool on map
    let draw_poly_active = state.tool_mode == ToolMode::DrawBasicPolygon;
    let draw_poly_btn = egui::Button::new(
        egui::RichText::new(if draw_poly_active {
            "Sedang Menggambar Polygon... (Klik titik 1 / Enter)"
        } else {
            "+ Gambar Polygon Bebas (Height) di Peta"
        })
        .size(11.0)
        .strong()
        .color(egui::Color32::WHITE),
    )
    .fill(if draw_poly_active {
        egui::Color32::from_rgb(124, 58, 237)
    } else {
        egui::Color32::from_rgb(49, 46, 129)
    })
    .min_size(egui::vec2(ui.available_width(), 28.0));
    if ui
        .add_enabled(editable, draw_poly_btn)
        .on_hover_text("Klik bebas min 3 titik di peta 3D, lalu klik titik #1 atau tekan Enter untuk membentuk Polygon 3D dengan tinggi & material")
        .clicked()
    {
        if draw_poly_active {
            state.tool_mode = ToolMode::Select;
            state.draft_points_lnglat.clear();
        } else {
            state.tool_mode = ToolMode::DrawBasicPolygon;
            state.draft_points_lnglat.clear();
            state.status_banner = Some(
                "Mode Gambar Polygon Bebas: Klik min 3 titik di peta, klik titik #1 atau tekan Enter untuk selesai (Backspace = Undo)."
                    .into(),
            );
        }
    }

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    let mut add_shape: Option<BasicMeshShape> = None;
    let mut start_drag_shape: Option<BasicMeshShape> = None;
    let mut toggle_draw_poly_from_card = false;

    egui::ScrollArea::vertical()
        .id_salt("godot_basic_mesh_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for &shape in BasicMeshShape::ALL {
                let is_poly_card = shape == BasicMeshShape::Polygon;
                let is_dragging = state.dragging_basic_mesh == Some(shape);
                let is_active_poly_draw = is_poly_card && draw_poly_active;
                let card_bg = if is_dragging || is_active_poly_draw {
                    egui::Color32::from_rgb(59, 34, 108)
                } else {
                    egui::Color32::from_rgb(24, 31, 49)
                };
                let card_stroke = if is_dragging || is_active_poly_draw {
                    egui::Color32::from_rgb(167, 139, 250)
                } else {
                    egui::Color32::from_rgb(44, 56, 82)
                };

                egui::Frame::new()
                    .fill(card_bg)
                    .stroke(egui::Stroke::new(1.0, card_stroke))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(7, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let total_w = ui.available_width();
                        let btn_w = 68.0_f32;
                        let info_w = (total_w - btn_w - 6.0).max(80.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            let info_resp = ui
                                .allocate_ui_with_layout(
                                    egui::vec2(info_w, 34.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            let tag_col = if shape.is_recommended_extra() {
                                                egui::Color32::from_rgb(250, 204, 21)
                                            } else {
                                                egui::Color32::from_rgb(167, 139, 250)
                                            };
                                            ui.label(
                                                egui::RichText::new(format!("[{}]", shape.tag()))
                                                    .size(9.5)
                                                    .strong()
                                                    .color(tag_col),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(shape.label())
                                                        .size(11.5)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        let sub = if is_poly_card {
                                            "Gambar bebas di peta + tinggi 3D & edit titik"
                                        } else {
                                            shape.subtitle()
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(sub)
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                            )
                                            .truncate(),
                                        );
                                    },
                                )
                                .response;

                            let drag_resp = ui.interact(
                                info_resp.rect,
                                ui.make_persistent_id(("basic_mesh_drag", shape.slug())),
                                egui::Sense::click_and_drag(),
                            );
                            if editable && drag_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if drag_resp.drag_started() && editable {
                                start_drag_shape = Some(shape);
                            }
                            let _ = drag_resp.on_hover_text(if !editable {
                                "Hentikan Play Mode (Stop) untuk menambah Basic Mesh"
                            } else if is_poly_card {
                                "Tarik ke peta untuk mulai menggambar titik #1 Polygon, atau klik 'Gambar'"
                            } else {
                                "Tarik (drag) ke peta 3D untuk meletakkan Basic Mesh ini"
                            });

                            let (btn_text, btn_fill, btn_hover) = if is_poly_card {
                                if draw_poly_active {
                                    (
                                        "Batal",
                                        egui::Color32::from_rgb(185, 28, 28),
                                        "Batalkan gambar polygon bebas",
                                    )
                                } else {
                                    (
                                        "Gambar",
                                        egui::Color32::from_rgb(14, 116, 144),
                                        "Gambar polygon bebas langsung di peta 3D lalu extrude tinggi",
                                    )
                                }
                            } else {
                                (
                                    "+ Tambah",
                                    egui::Color32::from_rgb(109, 40, 217),
                                    "Tambahkan Basic Mesh ini ke tengah peta",
                                )
                            };
                            let add_btn = egui::Button::new(
                                egui::RichText::new(btn_text)
                                    .size(10.5)
                                    .strong()
                                    .color(if editable {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    }),
                            )
                            .fill(if editable {
                                btn_fill
                            } else {
                                egui::Color32::from_rgb(30, 41, 59)
                            })
                            .min_size(egui::vec2(btn_w, 28.0));
                            if ui
                                .add_enabled(editable, add_btn)
                                .on_hover_text(btn_hover)
                                .clicked()
                            {
                                if is_poly_card {
                                    toggle_draw_poly_from_card = true;
                                } else {
                                    add_shape = Some(shape);
                                }
                            }
                        });
                    });
                ui.add_space(4.0);
            }
        });

    if toggle_draw_poly_from_card && editable {
        if state.tool_mode == ToolMode::DrawBasicPolygon {
            state.tool_mode = ToolMode::Select;
            state.draft_points_lnglat.clear();
        } else {
            state.tool_mode = ToolMode::DrawBasicPolygon;
            state.draft_points_lnglat.clear();
            state.status_banner = Some(
                "Mode Gambar Polygon Bebas: Klik min 3 titik di peta, klik titik #1 atau tekan Enter untuk selesai (Backspace = Undo)."
                    .into(),
            );
        }
    }

    if let Some(shape) = start_drag_shape {
        state.dragging_basic_mesh = Some(shape);
        state.dragging_catalog_entry = None;
    }

    if let Some(shape) = add_shape {
        if editable {
            if spawn_basic_mesh_at_map_center(state, tcam, api, shape) {
                state.status_banner = Some(format!(
                    "Basic Mesh '{}' ditambahkan — atur ukuran, warna/texture & transform di Inspector.",
                    shape.label()
                ));
            }
        } else {
            state.status_banner =
                Some("Hentikan Play Mode (Stop) untuk menambahkan objek baru.".into());
        }
    }
}

fn draw_reorder_arrow_btn(ui: &mut egui::Ui, up: bool, enabled: bool) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 18.0), sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if !enabled {
            egui::Color32::from_rgb(18, 25, 38)
        } else if resp.hovered() {
            egui::Color32::from_rgb(30, 64, 110)
        } else {
            egui::Color32::from_rgb(26, 36, 54)
        };
        let border = if !enabled {
            egui::Color32::from_rgb(35, 46, 66)
        } else if resp.hovered() {
            egui::Color32::from_rgb(56, 189, 248)
        } else {
            egui::Color32::from_rgb(51, 65, 85)
        };
        p.rect_filled(rect, 3.0, bg);
        p.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        let col = if !enabled {
            egui::Color32::from_rgb(71, 85, 105)
        } else if resp.hovered() {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(186, 230, 253)
        };
        let c = rect.center();
        let tri = if up {
            vec![
                egui::pos2(c.x, c.y - 3.6),
                egui::pos2(c.x - 4.2, c.y + 3.0),
                egui::pos2(c.x + 4.2, c.y + 3.0),
            ]
        } else {
            vec![
                egui::pos2(c.x, c.y + 3.6),
                egui::pos2(c.x - 4.2, c.y - 3.0),
                egui::pos2(c.x + 4.2, c.y - 3.0),
            ]
        };
        p.add(egui::Shape::convex_polygon(tri, col, egui::Stroke::NONE));
    }
    resp
}

fn draw_animations_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
) {
    let editable = !playback.play_test_mode;
    ui.label(
        egui::RichText::new("Animasi Skenario")
            .size(13.0)
            .strong()
            .color(egui::Color32::WHITE),
    );
    ui.label(
        egui::RichText::new(
            "Buat animasi berdasarkan kategori, lalu atur urutannya di daftar bawah.",
        )
        .size(10.5)
        .color(egui::Color32::from_rgb(110, 231, 183)),
    );
    ui.add_space(5.0);

    // Section 1: Create new animation by Category (5 categories in 2 compact rows + creation card)
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(12, 17, 27))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            let draw_cat_btn = |col_ui: &mut egui::Ui, state: &mut AppState, cat: AnimCategory| {
                let active = state.anim_category_filter == cat;
                let btn =
                    egui::Button::new(egui::RichText::new(cat.label()).size(10.5).strong().color(
                        if active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(148, 163, 184)
                        },
                    ))
                    .fill(if active {
                        match cat {
                            AnimCategory::Route => egui::Color32::from_rgb(5, 150, 105),
                            AnimCategory::Shoot => egui::Color32::from_rgb(185, 28, 28),
                            AnimCategory::Particle => egui::Color32::from_rgb(234, 88, 12),
                            AnimCategory::SpawnHide => egui::Color32::from_rgb(109, 40, 217),
                            AnimCategory::Group => egui::Color32::from_rgb(79, 70, 229),
                            AnimCategory::Other => egui::Color32::from_rgb(14, 116, 144),
                            AnimCategory::Audio => egui::Color32::from_rgb(109, 40, 217),
                            AnimCategory::Camera => egui::Color32::from_rgb(2, 132, 199),
                            AnimCategory::Environment => egui::Color32::from_rgb(13, 148, 136),
                        }
                    } else {
                        egui::Color32::from_rgb(22, 30, 45)
                    })
                    .min_size(egui::vec2(col_ui.available_width(), 22.0));
                if col_ui.add(btn).clicked() {
                    state.anim_category_filter = cat;
                }
            };

            ui.columns(3, |cols| {
                draw_cat_btn(&mut cols[0], state, AnimCategory::Route);
                draw_cat_btn(&mut cols[1], state, AnimCategory::Shoot);
                draw_cat_btn(&mut cols[2], state, AnimCategory::Particle);
            });
            ui.add_space(3.0);
            ui.columns(3, |cols| {
                draw_cat_btn(&mut cols[0], state, AnimCategory::SpawnHide);
                draw_cat_btn(&mut cols[1], state, AnimCategory::Group);
                draw_cat_btn(&mut cols[2], state, AnimCategory::Other);
            });
            ui.add_space(3.0);
            ui.columns(3, |cols| {
                draw_cat_btn(&mut cols[0], state, AnimCategory::Audio);
                draw_cat_btn(&mut cols[1], state, AnimCategory::Camera);
                draw_cat_btn(&mut cols[2], state, AnimCategory::Environment);
            });
        });

    ui.add_space(6.0);

    match state.anim_category_filter {
        AnimCategory::Route => draw_route_create_card(ui, state, editable),
        AnimCategory::Shoot => draw_shoot_create_card(ui, state, editable),
        AnimCategory::Particle => draw_particle_create_card(ui, state, editable),
        AnimCategory::SpawnHide => draw_spawn_hide_create_card(ui, state, api, editable),
        AnimCategory::Group => draw_group_create_card(ui, state, api, editable),
        AnimCategory::Other => draw_other_create_card(ui, state, editable),
        AnimCategory::Audio => {
            super::audio_anim::draw_create(ui, state, api, editable);
            super::billboard_anim::draw_create(ui, state, editable);
        }
        AnimCategory::Camera => {
            super::camera_anim::draw_create(ui, state, tcam, api, editable);
        }
        AnimCategory::Environment => {
            super::environment_anim::draw_create(ui, state, api, editable);
        }
    }

    ui.add_space(7.0);
    ui.separator();
    ui.add_space(5.0);

    // Section 2: Master Ordered Animation List (Global 'Semua' default + Category Filters + Up/Down Sort!)
    draw_ordered_animation_list(ui, state, tcam, playback, editable);
}

fn draw_group_create_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    editable: bool,
) {
    let is_combine = state.new_group_mode == GroupAnimMode::Combine;

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(22, 30, 45))
        .stroke(egui::Stroke::new(
            1.2,
            if is_combine {
                egui::Color32::from_rgb(59, 130, 246)
            } else {
                egui::Color32::from_rgb(139, 92, 246)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Buat Grup Animasi")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(79, 70, 229))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("COMBINE / SEQ")
                                    .size(8.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            });
            ui.add_space(4.0);

            ui.columns(2, |cols| {
                for (idx, &mode) in GroupAnimMode::ALL.iter().enumerate() {
                    let active = state.new_group_mode == mode;
                    let fill = if active {
                        match mode {
                            GroupAnimMode::Combine => egui::Color32::from_rgb(37, 99, 235),
                            GroupAnimMode::Sequence => egui::Color32::from_rgb(124, 58, 237),
                        }
                    } else {
                        egui::Color32::from_rgb(15, 23, 42)
                    };
                    let btn = egui::Button::new(
                        egui::RichText::new(mode.label())
                            .size(10.0)
                            .strong()
                            .color(if active {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(148, 163, 184)
                            }),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(cols[idx].available_width(), 24.0));
                    if cols[idx].add(btn).clicked() && state.new_group_mode != mode {
                        let cnt = state
                            .anims
                            .iter()
                            .filter(|a| group_anim_mode(a) == Some(mode))
                            .count()
                            + 1;
                        state.new_group_mode = mode;
                        state.new_group_name = mode.default_name(cnt);
                    }
                }
            });

            ui.add_space(4.0);
            let hint = match state.new_group_mode {
                GroupAnimMode::Combine => {
                    "Menjalankan 2 atau lebih animasi secara bersamaan (paralel) dalam 1 klik Play."
                }
                GroupAnimMode::Sequence => {
                    "Menjalankan animasi secara bergantian: tunggu 1 siklus selesai (anak Repeat tetap mengulang) atau jeda waktu detik (mis. 0.25 dtk)."
                }
            };
            ui.label(
                egui::RichText::new(hint)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(199, 210, 254)),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Nama:")
                        .size(10.5)
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_group_name)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.add_space(5.0);

            let (btn_txt, btn_fill) = match state.new_group_mode {
                GroupAnimMode::Combine => (
                    "+ Buat Animasi Combine",
                    egui::Color32::from_rgb(37, 99, 235),
                ),
                GroupAnimMode::Sequence => (
                    "+ Buat Animasi Sequence",
                    egui::Color32::from_rgb(124, 58, 237),
                ),
            };
            let create_btn = egui::Button::new(
                egui::RichText::new(btn_txt)
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(btn_fill)
            .min_size(egui::vec2(ui.available_width(), 27.0));
            if ui
                .add_enabled(editable, create_btn)
                .on_hover_text("Buat grup baru lalu pilih animasi anggota di panel Inspector kanan")
                .clicked()
                && let Some(sk) = state.active_skenario.clone()
            {
                let mode = state.new_group_mode;
                let cnt = state
                    .anims
                    .iter()
                    .filter(|a| group_anim_mode(a) == Some(mode))
                    .count()
                    + 1;
                let name = if state.new_group_name.trim().is_empty() {
                    mode.default_name(cnt)
                } else {
                    state.new_group_name.trim().to_string()
                };
                let cfg = GroupAnimConfig::default_for(mode);
                let data = serde_json::json!({ "group": cfg });
                let sort_order = (state.anims.len() + 1) as i32;
                let anim_type = mode.anim_type_str().to_string();

                state.picking_marker_for_anim_id = None;
                state.picking_shoot_for_anim_id = None;
                state.picking_pointer_for_anim_id = None;
                state.picking_spawn_hide_for_anim_id = None;
                state.draft_points_lnglat.clear();
                state.tool_mode = ToolMode::Select;
                state.group_add_search.clear();

                if state.offline_preview {
                    let now = chrono::Utc::now();
                    let a = Anim {
                        id: Uuid::new_v4(),
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: None,
                        name: name.clone(),
                        anim_type,
                        sort_order,
                        speed: 1.0,
                        duration_sec: 0.0,
                        replay: false,
                        autoplay: false,
                        geom: Vec::new(),
                        data,
                        created_at: now,
                        updated_at: now,
                    };
                    let aid = a.id;
                    state.selected = SelectedEntity::Anim(aid);
                    state.anim_edit = Some(a.clone());
                    state.marker_edit = None;
                    state.building_edit = None;
                    state.line_edit = None;
                    state.text_edit = None;
                    state.anims.push(a);
                    state.sort_anims_by_order();
                    state.status_banner = Some(format!(
                        "Grup {} '{name}' dibuat — tambahkan animasi anggota di Inspector kanan!",
                        mode.short_tag()
                    ));
                } else if let Some(auth) = current_auth(state) {
                    api.send(ApiCommand::CreateAnim {
                        auth,
                        req: CreateAnimRequest {
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: None,
                            name: name.clone(),
                            anim_type,
                            sort_order,
                            speed: 1.0,
                            duration_sec: 0.0,
                            replay: false,
                            autoplay: false,
                            geom: Vec::new(),
                            data,
                        },
                    });
                }
                state.new_group_name = mode.default_name(cnt + 1);
            }
        });
}

fn draw_spawn_hide_create_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    api: &ApiClientBridge,
    editable: bool,
) {
    let is_picking_sh = state.tool_mode == ToolMode::PickSpawnHideMarker;
    let active_action = state
        .picking_spawn_hide_for_anim_id
        .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
        .and_then(spawn_hide_action)
        .unwrap_or(state.new_spawn_hide_action);
    let is_spawn = active_action == SpawnHideAction::Spawn;

    egui::Frame::new()
        .fill(if is_picking_sh {
            if is_spawn {
                egui::Color32::from_rgb(6, 56, 42)
            } else {
                egui::Color32::from_rgb(68, 14, 28)
            }
        } else {
            egui::Color32::from_rgb(22, 30, 45)
        })
        .stroke(egui::Stroke::new(
            1.2,
            if is_picking_sh {
                if is_spawn {
                    egui::Color32::from_rgb(16, 185, 129)
                } else {
                    egui::Color32::from_rgb(244, 63, 94)
                }
            } else {
                egui::Color32::from_rgb(88, 56, 148)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Buat Animasi Spawn & Hide")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(109, 40, 217))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("MULTI-OBJEK")
                                    .size(8.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            });
            ui.add_space(4.0);

            if is_picking_sh {
                let picked_anim = state
                    .picking_spawn_hide_for_anim_id
                    .and_then(|aid| state.anims.iter().find(|a| a.id == aid));
                let picked_cnt = picked_anim
                    .map(|a| anim_bound_marker_ids(a).len())
                    .unwrap_or(0);
                let anim_label = picked_anim
                    .map(|a| a.name.as_str())
                    .unwrap_or(state.new_spawn_hide_name.as_str());
                ui.label(
                    egui::RichText::new(format!(
                        "Aktif [{}]: '{}' ({picked_cnt} objek terpilih)",
                        active_action.short_tag(),
                        anim_label
                    ))
                    .size(10.5)
                    .strong()
                    .color(if is_spawn {
                        egui::Color32::from_rgb(110, 231, 183)
                    } else {
                        egui::Color32::from_rgb(253, 164, 175)
                    }),
                );
                ui.label(
                    egui::RichText::new(
                        "Klik 1 atau beberapa Marker / Objek 3D di peta (klik ulang untuk lepas).",
                    )
                    .size(10.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
                );
                ui.add_space(4.0);
                let done_btn = egui::Button::new(
                    egui::RichText::new("Selesai Pilih Objek (Enter)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(5, 150, 105))
                .min_size(egui::vec2(ui.available_width(), 26.0));
                if ui.add(done_btn).clicked() {
                    state.picking_spawn_hide_for_anim_id = None;
                    state.tool_mode = ToolMode::Select;
                    state.status_banner = Some(format!(
                        "Selesai memilih {picked_cnt} objek untuk '{anim_label}'."
                    ));
                }
            } else {
                // Mode selector: Spawn (Munculkan) vs Hide (Sembunyikan)
                ui.columns(2, |cols| {
                    for (idx, &act) in SpawnHideAction::ALL.iter().enumerate() {
                        let active = state.new_spawn_hide_action == act;
                        let fill = if active {
                            match act {
                                SpawnHideAction::Spawn => egui::Color32::from_rgb(5, 150, 105),
                                SpawnHideAction::Hide => egui::Color32::from_rgb(190, 18, 60),
                            }
                        } else {
                            egui::Color32::from_rgb(15, 23, 42)
                        };
                        let btn = egui::Button::new(
                            egui::RichText::new(act.label())
                                .size(10.0)
                                .strong()
                                .color(if active {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_rgb(148, 163, 184)
                                }),
                        )
                        .fill(fill)
                        .min_size(egui::vec2(cols[idx].available_width(), 24.0));
                        if cols[idx].add(btn).clicked() && state.new_spawn_hide_action != act {
                            let cnt = state
                                .anims
                                .iter()
                                .filter(|a| spawn_hide_action(a) == Some(act))
                                .count()
                                + 1;
                            state.new_spawn_hide_action = act;
                            state.new_spawn_hide_name = act.default_name(cnt);
                        }
                    }
                });

                ui.add_space(4.0);
                let hint = match state.new_spawn_hide_action {
                    SpawnHideAction::Spawn => {
                        "Di Editor objek tetap tampil agar mudah ditata. Saat Play Skenario mulai, target otomatis tersembunyi dan muncul ketika animasi ini di-Play."
                    }
                    SpawnHideAction::Hide => {
                        "Pilih 1 atau beberapa Marker / Objek 3D di peta. Saat Play Skenario, target akan hilang ketika animasi ini di-Play dan kembali saat Stop."
                    }
                };
                ui.label(
                    egui::RichText::new(hint)
                        .size(10.0)
                        .color(egui::Color32::from_rgb(196, 181, 253)),
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Nama:")
                            .size(10.5)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_spawn_hide_name)
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.add_space(5.0);

                let (btn_txt, btn_fill) = match state.new_spawn_hide_action {
                    SpawnHideAction::Spawn => (
                        "+ Buat Spawn & Pilih Objek di Peta",
                        egui::Color32::from_rgb(5, 150, 105),
                    ),
                    SpawnHideAction::Hide => (
                        "+ Buat Hide & Pilih Objek di Peta",
                        egui::Color32::from_rgb(190, 18, 60),
                    ),
                };
                let create_btn = egui::Button::new(
                    egui::RichText::new(btn_txt)
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(btn_fill)
                .min_size(egui::vec2(ui.available_width(), 27.0));
                if ui
                    .add_enabled(editable, create_btn)
                    .on_hover_text(
                        "Buat animasi baru lalu klik 1 atau beberapa Marker/Objek 3D di peta",
                    )
                    .clicked()
                    && let Some(sk) = state.active_skenario.clone()
                {
                    let action = state.new_spawn_hide_action;
                    let cnt = state
                        .anims
                        .iter()
                        .filter(|a| spawn_hide_action(a) == Some(action))
                        .count()
                        + 1;
                    let name = if state.new_spawn_hide_name.trim().is_empty() {
                        action.default_name(cnt)
                    } else {
                        state.new_spawn_hide_name.trim().to_string()
                    };
                    let cfg = SpawnHideConfig::default_for(action);
                    let dur = cfg.total_duration_sec();
                    let data = serde_json::json!({ "spawn_hide": cfg });
                    let sort_order = (state.anims.len() + 1) as i32;
                    let anim_type = action.anim_type_str().to_string();

                    state.picking_marker_for_anim_id = None;
                    state.picking_shoot_for_anim_id = None;
                    state.picking_pointer_for_anim_id = None;
                    state.draft_points_lnglat.clear();

                    if state.offline_preview {
                        let now = chrono::Utc::now();
                        let a = Anim {
                            id: Uuid::new_v4(),
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: None,
                            name: name.clone(),
                            anim_type,
                            sort_order,
                            speed: 1.0,
                            duration_sec: dur,
                            replay: false,
                            autoplay: false,
                            geom: Vec::new(),
                            data,
                            created_at: now,
                            updated_at: now,
                        };
                        let aid = a.id;
                        state.selected = SelectedEntity::Anim(aid);
                        state.anim_edit = Some(a.clone());
                        state.marker_edit = None;
                        state.building_edit = None;
                        state.line_edit = None;
                        state.text_edit = None;
                        state.anims.push(a);
                        state.sort_anims_by_order();
                        state.picking_spawn_hide_for_anim_id = Some(aid);
                        state.tool_mode = ToolMode::PickSpawnHideMarker;
                        state.status_banner = Some(format!(
                            "Animasi {} '{name}' dibuat! Klik 1 atau beberapa Marker / Objek 3D di peta (Enter / Esc selesai).",
                            action.short_tag()
                        ));
                    } else if let Some(auth) = current_auth(state) {
                        api.send(ApiCommand::CreateAnim {
                            auth,
                            req: CreateAnimRequest {
                                operasi_id: sk.operasi_id,
                                skenario_id: sk.id,
                                marker_id: None,
                                name: name.clone(),
                                anim_type,
                                sort_order,
                                speed: 1.0,
                                duration_sec: dur,
                                replay: false,
                                autoplay: false,
                                geom: Vec::new(),
                                data,
                            },
                        });
                    }
                    state.new_spawn_hide_name = action.default_name(cnt + 1);
                }
            }
        });
}

fn draw_other_create_card(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    let is_picking_ptr = state.tool_mode == ToolMode::PickPointerMarker;

    egui::Frame::new()
        .fill(if is_picking_ptr {
            egui::Color32::from_rgb(66, 38, 8)
        } else {
            egui::Color32::from_rgb(22, 30, 45)
        })
        .stroke(egui::Stroke::new(
            1.2,
            if is_picking_ptr {
                egui::Color32::from_rgb(250, 204, 21)
            } else {
                egui::Color32::from_rgb(42, 60, 86)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Buat Animasi Penanda / Lainnya")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(14, 116, 144))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("LAINNYA")
                                    .size(9.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Klik '+ Buat Panah Penunjuk 3D', lalu klik 1 Marker / Objek 3D di peta. Panah 3D akan memantul naik-turun di atas target & mendukung teks opsional.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(254, 240, 138)),
            );
            ui.add_space(5.0);

            if is_picking_ptr {
                ui.label(
                    egui::RichText::new("Aktif: Panah Penunjuk 3D — klik 1 Objek 3D di peta")
                        .size(10.5)
                        .strong()
                        .color(egui::Color32::from_rgb(250, 204, 21)),
                );
                ui.add_space(3.0);
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("Batal Pilih Target di Peta (Esc)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(180, 83, 9))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(cancel_btn).clicked() {
                    state.tool_mode = ToolMode::Select;
                    state.picking_pointer_for_anim_id = None;
                }
            } else {
                let ptr_btn = egui::Button::new(
                    egui::RichText::new("+ Buat Panah Penunjuk 3D")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::from_rgb(15, 23, 42)),
                )
                .fill(egui::Color32::from_rgb(250, 204, 21))
                .min_size(egui::vec2(ui.available_width(), 26.0));
                if ui
                    .add_enabled(editable, ptr_btn)
                    .on_hover_text(
                        "Panah 3D memantul naik-turun di atas 1 Marker/Objek 3D dengan tulisan, skala font & warna kustom opsional",
                    )
                    .clicked()
                {
                    let ptr_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_pointer_arrow_anim(a))
                        .count()
                        + 1;
                    state.new_pointer_name = format!("Panah Penunjuk {ptr_idx}");
                    state.picking_pointer_for_anim_id = None;
                    state.picking_shoot_for_anim_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.draft_points_lnglat.clear();
                    state.tool_mode = ToolMode::PickPointerMarker;
                    state.status_banner = Some(
                        "Klik 1 Marker / Objek 3D di peta untuk memasang Panah Penunjuk 3D (Esc batal)."
                            .into(),
                    );
                }
            }
        });
}

fn draw_shoot_create_card(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    let is_picking_shoot = state.tool_mode == ToolMode::PickShootMarker;

    egui::Frame::new()
        .fill(if is_picking_shoot {
            egui::Color32::from_rgb(12, 46, 72)
        } else {
            egui::Color32::from_rgb(22, 30, 45)
        })
        .stroke(egui::Stroke::new(
            1.2,
            if is_picking_shoot {
                egui::Color32::from_rgb(56, 189, 248)
            } else {
                egui::Color32::from_rgb(42, 60, 86)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Buat Animasi Shoot")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(185, 28, 28))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("SHOOT")
                                    .size(9.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Pilih jenis aksi shoot di bawah, lalu klik 1 Model 3D / Unit di peta. Preview langsung tampil saat animasi dipilih.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(186, 230, 253)),
            );
            ui.add_space(5.0);

            if is_picking_shoot {
                let active_lbl = state
                    .picking_shoot_for_anim_id
                    .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                    .and_then(shoot_anim_kind)
                    .unwrap_or(state.new_shoot_kind)
                    .label();
                ui.label(
                    egui::RichText::new(format!(
                        "Aktif: {active_lbl} — klik 1 Model 3D di peta"
                    ))
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(125, 211, 252)),
                );
                ui.add_space(3.0);
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("Batal Pilih Unit di Peta (Esc)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(180, 83, 9))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(cancel_btn).clicked() {
                    state.tool_mode = ToolMode::Select;
                    state.picking_shoot_for_anim_id = None;
                }
            } else {
                let start_pick = |state: &mut AppState, kind: ShootAnimKind| {
                    let shoot_idx = state
                        .anims
                        .iter()
                        .filter(|a| shoot_anim_kind(a) == Some(kind))
                        .count()
                        + 1;
                    state.new_shoot_kind = kind;
                    state.new_shoot_name = kind.default_name(shoot_idx);
                    state.picking_shoot_for_anim_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.draft_points_lnglat.clear();
                    state.tool_mode = ToolMode::PickShootMarker;
                    state.status_banner = Some(format!(
                        "Klik 1 Model 3D / Unit di peta untuk memasang animasi {} (Esc batal).",
                        kind.label()
                    ));
                };

                let buttons: [(ShootAnimKind, &str, egui::Color32, &str); 5] = [
                    (
                        ShootAnimKind::WaterCannon,
                        "+ Buat Water Cannon (AWC / Damkar)",
                        egui::Color32::from_rgb(2, 132, 199),
                        "Semprotan air bertekanan dari ketinggian (AWC Polisi / Damkar) — 1 animasi untuk 1 Model 3D",
                    ),
                    (
                        ShootAnimKind::StoneThrow,
                        "+ Buat Lempar Batu",
                        egui::Color32::from_rgb(180, 83, 9),
                        "Melempar batu berputar parabolik ke arah depan dari marker dengan percikan debu saat jatuh",
                    ),
                    (
                        ShootAnimKind::TearGas,
                        "+ Buat Gas Air Mata",
                        egui::Color32::from_rgb(71, 85, 105),
                        "Menembakkan peluru gas air mata melambung ke atas; saat menyentuh ground (Alt 0) muncul asap putih yg pelan-pelan menghilang",
                    ),
                    (
                        ShootAnimKind::Bullet,
                        "+ Buat Tembak Peluru",
                        egui::Color32::from_rgb(185, 28, 28),
                        "Menembakkan peluru tracer + kilatan moncong ke depan; peluru otomatis hilang saat mencapai jarak maksimal",
                    ),
                    (
                        ShootAnimKind::Apar,
                        "+ Buat Semprot APAR",
                        egui::Color32::from_rgb(15, 118, 110),
                        "Menyemprotkan kabut serbuk/CO2/busa APAR bertekanan ke depan dengan gerakan sapuan kiri-kanan",
                    ),
                ];

                for (idx, (kind, label, fill, hover)) in buttons.into_iter().enumerate() {
                    if idx > 0 {
                        ui.add_space(4.0);
                    }
                    let btn = egui::Button::new(
                        egui::RichText::new(label)
                            .size(11.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(ui.available_width(), 24.5));
                    if ui.add_enabled(editable, btn).on_hover_text(hover).clicked() {
                        start_pick(state, kind);
                    }
                }
            }
        });
}

fn draw_particle_create_card(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    let is_placing = state.tool_mode == ToolMode::PlaceParticle;

    egui::Frame::new()
        .fill(if is_placing {
            egui::Color32::from_rgb(67, 25, 10)
        } else {
            egui::Color32::from_rgb(22, 30, 45)
        })
        .stroke(egui::Stroke::new(
            1.2,
            if is_placing {
                egui::Color32::from_rgb(251, 146, 60)
            } else {
                egui::Color32::from_rgb(42, 60, 86)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Buat Animasi Particle")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(234, 88, 12))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("PARTICLE")
                                    .size(9.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Pilih jenis efek di bawah lalu klik lokasi di peta 3D. Posisi (Lng / Lat / Alt) bisa langsung digeser lewat Move Gizmo atau Inspector.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(254, 215, 170)),
            );
            ui.add_space(5.0);

            if is_placing {
                ui.label(
                    egui::RichText::new(format!(
                        "Aktif: {} — klik peta untuk meletakkan",
                        state.new_particle_kind.label()
                    ))
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(253, 186, 116)),
                );
                ui.add_space(3.0);
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("Batal Letakkan Partikel (Esc)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(180, 83, 9))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(cancel_btn).clicked() {
                    state.tool_mode = ToolMode::Select;
                }
            } else {
                let start_place = |state: &mut AppState, kind: ParticleAnimKind| {
                    let count = state
                        .anims
                        .iter()
                        .filter(|a| ParticleAnimKind::from_anim_type(&a.anim_type) == Some(kind))
                        .count()
                        + 1;
                    state.new_particle_kind = kind;
                    state.new_particle_name = kind.default_name(count);
                    state.picking_marker_for_anim_id = None;
                    state.picking_shoot_for_anim_id = None;
                    state.draft_points_lnglat.clear();
                    state.tool_mode = ToolMode::PlaceParticle;
                    state.status_banner = Some(format!(
                        "Mode Letakkan {}: Klik 1 titik di peta 3D untuk menempatkan (Esc batal).",
                        kind.label()
                    ));
                };

                let fire_btn = egui::Button::new(
                    egui::RichText::new("+ Buat Api (Fire)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(234, 88, 12))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, fire_btn)
                    .on_hover_text(
                        "Efek kebakaran 3D dengan lidah api dinamis, asap atas, percikan bara, dan arah angin",
                    )
                    .clicked()
                {
                    start_place(state, ParticleAnimKind::Fire);
                }

                ui.add_space(4.0);

                let smoke_btn = egui::Button::new(
                    egui::RichText::new("+ Buat Asap (Smoke)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(71, 85, 105))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, smoke_btn)
                    .on_hover_text(
                        "Kepulan asap taktis (kebakaran, gas air mata, atau asap sinyal merah/hijau/kuning)",
                    )
                    .clicked()
                {
                    start_place(state, ParticleAnimKind::Smoke);
                }

                ui.add_space(4.0);

                let exp_btn = egui::Button::new(
                    egui::RichText::new("+ Buat Ledakan (Explosion)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(185, 28, 28))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, exp_btn)
                    .on_hover_text(
                        "Ledakan 3D dengan bola api, cincin gelombang kejut (shockwave), percikan puing, dan asap jamur",
                    )
                    .clicked()
                {
                    start_place(state, ParticleAnimKind::Explosion);
                }
            }
        });
}

fn draw_route_create_card(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    let is_drawing_route = state.tool_mode == ToolMode::DrawRoute;
    let is_drawing_traffic = state.tool_mode == ToolMode::DrawTrafficLane;
    let is_drawing_traffic_queue = matches!(
        state.tool_mode,
        ToolMode::DrawTrafficQueueLane | ToolMode::DrawTrafficDiversionLane
    );
    let is_picking_unit = state.tool_mode == ToolMode::PickRouteMarker;
    let picking_is_multi = is_picking_unit
        && state
            .picking_marker_for_anim_id
            .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
            .is_some_and(is_multi_route_anim);

    egui::Frame::new()
        .fill(
            if is_drawing_route
                || is_drawing_traffic
                || is_drawing_traffic_queue
                || is_picking_unit
            {
                egui::Color32::from_rgb(6, 56, 46)
            } else {
                egui::Color32::from_rgb(22, 30, 45)
            },
        )
        .stroke(egui::Stroke::new(
            1.2,
            if is_drawing_route
                || is_drawing_traffic
                || is_drawing_traffic_queue
                || is_picking_unit
            {
                egui::Color32::from_rgb(16, 185, 129)
            } else {
                egui::Color32::from_rgb(42, 60, 86)
            },
        ))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Buat Animasi Rute")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(5, 150, 105))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("RUTE")
                                    .size(9.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "1) Gambar garis di peta -> 2) Pilih 1 atau beberapa Model 3D di peta. Klik garis rute kapan saja untuk edit titik.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(186, 230, 253)),
            );
            ui.add_space(5.0);

            if is_drawing_traffic {
                let mode_tag = if state.drawing_traffic_lane_for_anim_id.is_some() {
                    "Tambah Jalur Traffic"
                } else {
                    "Simulasi Traffic (Jalur 1)"
                };
                ui.label(
                    egui::RichText::new(format!(
                        "Menggambar: {mode_tag} ({} titik)",
                        state.draft_points_lnglat.len()
                    ))
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(253, 230, 138)),
                );
                ui.add_space(3.0);
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("Batal Gambar Jalur (Esc)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(180, 83, 9))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(cancel_btn).clicked() {
                    state.tool_mode = ToolMode::Select;
                    state.draft_points_lnglat.clear();
                    state.drawing_traffic_lane_for_anim_id = None;
                }
            } else if is_drawing_route {
                let mode_tag = if state.new_route_is_multi
                    && state.new_route_formation == RouteFormationKind::Patwal
                {
                    "Rute Patwal (Konvoi)"
                } else if state.new_route_is_multi {
                    "Rute Unit Multiple"
                } else {
                    "Rute Unit Tunggal"
                };
                ui.label(
                    egui::RichText::new(format!(
                        "Menggambar: {mode_tag} ({} titik)",
                        state.draft_points_lnglat.len()
                    ))
                    .size(10.5)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 231, 183)),
                );
                ui.add_space(3.0);
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("Batal Gambar Garis (Esc)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(180, 83, 9))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(cancel_btn).clicked() {
                    state.tool_mode = ToolMode::Select;
                    state.draft_points_lnglat.clear();
                }
            } else if is_picking_unit {
                let done_lbl = if picking_is_multi {
                    "Selesai Pilih Unit Formasi (Enter / Esc)"
                } else {
                    "Batal Pilih Unit di Peta (Esc)"
                };
                let cancel_btn = egui::Button::new(
                    egui::RichText::new(done_lbl)
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(if picking_is_multi {
                    egui::Color32::from_rgb(5, 150, 105)
                } else {
                    egui::Color32::from_rgb(14, 116, 144)
                })
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui.add(cancel_btn).clicked() {
                    state.tool_mode = ToolMode::Select;
                    state.picking_marker_for_anim_id = None;
                }
            } else {
                let single_btn = egui::Button::new(
                    egui::RichText::new("+ Rute Unit Tunggal")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(5, 150, 105))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, single_btn)
                    .on_hover_text("1 Model 3D bergerak mengikuti garis rute")
                    .clicked()
                {
                    state.tool_mode = ToolMode::DrawRoute;
                    state.new_route_is_multi = false;
                    state.new_route_formation = RouteFormationKind::Grid;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_route_name = format!("Rute Unit {}", state.anims.len() + 1);
                    state.status_banner = Some(
                        "Langkah 1/2 (Tunggal): Klik titik-titik garis rute di peta (min 2 titik), lalu tekan Enter / Finish untuk memilih Model 3D."
                            .into(),
                    );
                }

                ui.add_space(4.0);

                let multi_btn = egui::Button::new(
                    egui::RichText::new("+ Rute Unit Multiple (Formasi)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(14, 116, 144))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, multi_btn)
                    .on_hover_text(
                        "Banyak Model 3D bergerak serentak dalam formasi (Barisan Grid, Patwal, Bebas/Kustom, Panah/V, Zig-Zag, Kawal VIP)",
                    )
                    .clicked()
                {
                    state.tool_mode = ToolMode::DrawRoute;
                    state.new_route_is_multi = true;
                    state.new_route_formation = RouteFormationKind::Grid;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_route_name = format!("Rute Formasi {}", state.anims.len() + 1);
                    state.status_banner = Some(
                        "Langkah 1/2 (Multiple): Klik titik-titik garis rute di peta (min 2 titik), lalu tekan Enter / Finish untuk memilih beberapa Model 3D."
                            .into(),
                    );
                }

                ui.add_space(4.0);

                let patwal_btn = egui::Button::new(
                    egui::RichText::new("+ Rute Patwal (Konvoi)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(37, 99, 235))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, patwal_btn)
                    .on_hover_text(
                        "Konvoi pengawalan Patwal berbasis peran: Motor Depan (otomatis sejajar/panah/grid) -> Mobil Patwal -> VIP + Motor Samping -> Mobil Penutup",
                    )
                    .clicked()
                {
                    let patwal_idx = state
                        .anims
                        .iter()
                        .filter(|a| {
                            is_multi_route_anim(a)
                                && parse_multi_route_config(a).formation
                                    == RouteFormationKind::Patwal
                        })
                        .count()
                        + 1;
                    state.tool_mode = ToolMode::DrawRoute;
                    state.new_route_is_multi = true;
                    state.new_route_formation = RouteFormationKind::Patwal;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_route_name = format!("Rute Patwal {patwal_idx}");
                    state.status_banner = Some(
                        "Langkah 1/2 (Rute Patwal): Gambar garis rute jalan di peta (min 2 titik), lalu tekan Enter / Finish untuk memilih unit konvoi."
                            .into(),
                    );
                }

                ui.add_space(4.0);
                let traffic_btn = egui::Button::new(
                    egui::RichText::new("+ Simulasi Traffic (Lalu Lintas)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(217, 119, 6))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, traffic_btn)
                    .on_hover_text(
                        "Simulasi arus lalu lintas otomatis: multi-jalur (termasuk 2 arah), pool kendaraan 3D random, kecepatan acak, anti-tabrak & menyalip cerdas",
                    )
                    .clicked()
                {
                    let traffic_idx = state.anims.iter().filter(|a| is_traffic_anim(a)).count() + 1;
                    state.tool_mode = ToolMode::DrawTrafficLane;
                    state.drawing_traffic_lane_for_anim_id = None;
                    state.drawing_traffic_is_signal = false;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_traffic_name = format!("Traffic Lalu Lintas {traffic_idx}");
                    state.status_banner = Some(
                        "Simulasi Traffic: Klik titik-titik Jalur 1 di peta (min 2 titik), lalu tekan Enter / Finish. Setelah itu bisa Duplikat Jalur Balik & pilih kendaraan di Inspector."
                            .into(),
                    );
                }

                ui.add_space(4.0);
                let signal_btn = egui::Button::new(
                    egui::RichText::new("+ Lalu Lintas Lampu Merah")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(225, 29, 72))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, signal_btn)
                    .on_hover_text(
                        "Simulasi arus lalu lintas normal (kecepatan acak & menyalip) + Lampu Merah 3D (Merah/Kuning/Hijau bergantian antar fase) yang bisa dipindah, diputar, & diskala",
                    )
                    .clicked()
                {
                    let sig_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_traffic_signal_anim(a))
                        .count()
                        + 1;
                    state.tool_mode = ToolMode::DrawTrafficLane;
                    state.drawing_traffic_lane_for_anim_id = None;
                    state.drawing_traffic_is_signal = true;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_traffic_signal_name = format!("Lampu Merah {sig_idx}");
                    state.status_banner = Some(
                        "Lalu Lintas Lampu Merah: Klik titik-titik Jalur 1 di peta (min 2 titik), tekan Enter / Finish. Lalu tambah jalur lain & pasang Lampu Merah 3D di Inspector!"
                            .into(),
                    );
                }

                ui.add_space(4.0);
                let queue_btn = egui::Button::new(
                    egui::RichText::new("+ Antrean & Urai Simpang")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(13, 148, 136))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, queue_btn)
                    .on_hover_text(
                        "Simulasi antrean simpang & urai kemacetan: setiap Jalur Antre berpasangan dengan Jalur Lancar wajib, bergantian per durasi, plus tombol 'Lancar' untuk mengurai seluruh kendaraan sampai habis",
                    )
                    .clicked()
                {
                    let q_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_traffic_queue_anim(a))
                        .count()
                        + 1;
                    state.tool_mode = ToolMode::DrawTrafficQueueLane;
                    state.drawing_traffic_queue_target = None;
                    state.drawing_traffic_queue_is_gridlock = false;
                    state.drawing_traffic_queue_is_manual = false;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_traffic_queue_name = format!("Urai Simpang {q_idx}");
                    state.status_banner = Some(
                        "Antrean & Urai Simpang: Klik titik Jalur Antre A di peta (min 2 titik), tekan Enter / Finish. Lalu gambar Jalur Lancar wajibnya di Inspector."
                            .into(),
                    );
                }

                ui.add_space(4.0);
                let gridlock_btn = egui::Button::new(
                    egui::RichText::new("+ Macet Total & Urai")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(190, 18, 60))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, gridlock_btn)
                    .on_hover_text(
                        "Simulasi macet total (semua jalur tertahan diam / mengalir lalu berhenti) hingga tombol 'Lancar' diklik untuk mengurai kendaraan sampai habis",
                    )
                    .clicked()
                {
                    let g_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_traffic_gridlock_anim(a))
                        .count()
                        + 1;
                    state.tool_mode = ToolMode::DrawTrafficQueueLane;
                    state.drawing_traffic_queue_target = None;
                    state.drawing_traffic_queue_is_gridlock = true;
                    state.drawing_traffic_queue_is_manual = false;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_traffic_gridlock_name = format!("Macet Total {g_idx}");
                    state.status_banner = Some(
                        "Macet Total & Urai: Klik titik Jalur Antre A di peta (min 2 titik), tekan Enter / Finish. Lalu gambar Jalur Lancar wajibnya di Inspector."
                            .into(),
                    );
                }

                ui.add_space(4.0);
                let manual_btn = egui::Button::new(
                    egui::RichText::new("+ Kontrol Simpang Manual")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(79, 70, 229))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, manual_btn)
                    .on_hover_text(
                        "Buka-tutup simpang manual secara live oleh operator saat Play (tombol Buka Jalur A/B/C, Tahan Semua, dan Lancar — sinkron ke Layar Besar)",
                    )
                    .clicked()
                {
                    let m_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_traffic_manual_anim(a))
                        .count()
                        + 1;
                    state.tool_mode = ToolMode::DrawTrafficQueueLane;
                    state.drawing_traffic_queue_target = None;
                    state.drawing_traffic_queue_is_gridlock = false;
                    state.drawing_traffic_queue_is_manual = true;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_traffic_manual_name = format!("Simpang Manual {m_idx}");
                    state.status_banner = Some(
                        "Kontrol Simpang Manual: Klik titik Jalur Antre A di peta (min 2 titik), tekan Enter / Finish. Lalu gambar Jalur Lancar wajibnya di Inspector."
                            .into(),
                    );
                }

                ui.add_space(4.0);
                let diversion_btn = egui::Button::new(
                    egui::RichText::new("+ Pengalihan Arus (Diversion)")
                        .size(11.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(2, 132, 199))
                .min_size(egui::vec2(ui.available_width(), 25.0));
                if ui
                    .add_enabled(editable, diversion_btn)
                    .on_hover_text(
                        "Rekayasa pengalihan arus (Jalur Pendekat -> Jalur Utama / Jalur Alternatif): klik 'Alihkan Arus' saat Play untuk membelokkan kendaraan ke jalur alternatif",
                    )
                    .clicked()
                {
                    let d_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_traffic_diversion_anim(a))
                        .count()
                        + 1;
                    state.tool_mode = ToolMode::DrawTrafficDiversionLane;
                    state.drawing_diversion_target = None;
                    state.draft_points_lnglat.clear();
                    state.route_target_marker_id = None;
                    state.picking_marker_for_anim_id = None;
                    state.new_traffic_diversion_name = format!("Pengalihan Arus {d_idx}");
                    state.status_banner = Some(
                        "Pengalihan Arus: Klik titik Jalur Pendekat sampai ke titik simpang pengalihan (min 2 titik), tekan Enter / Finish."
                            .into(),
                    );
                }
            }
        });
}

fn draw_anim_list_filter_pill(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    target: &mut Option<AnimCategory>,
    val: Option<AnimCategory>,
) {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(9.5)
            .strong()
            .color(if active {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(148, 163, 184)
            }),
    )
    .fill(if active {
        egui::Color32::from_rgb(14, 116, 144)
    } else {
        egui::Color32::from_rgb(22, 30, 45)
    })
    .small();
    if ui.add(btn).clicked() {
        *target = val;
    }
}

fn draw_ordered_animation_list(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tcam: &mut TacticalCamera,
    playback: &mut PlaybackState,
    editable: bool,
) {
    state.sort_anims_by_order();

    let total_count = state.anims.len();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Urutan Animasi ({total_count})"))
                .size(11.5)
                .strong()
                .color(egui::Color32::from_rgb(186, 230, 253)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("Urut: Up / Down")
                    .size(9.5)
                    .color(egui::Color32::from_rgb(100, 116, 139)),
            );
        });
    });

    ui.add_space(3.0);
    // Filter pills decoupled from category creation: default 'Semua' shows true Play Skenario sequence!
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.spacing_mut().item_spacing.y = 3.0;
        draw_anim_list_filter_pill(
            ui,
            "Semua",
            state.anim_list_filter.is_none(),
            &mut state.anim_list_filter,
            None,
        );
        for &cat in AnimCategory::ALL {
            draw_anim_list_filter_pill(
                ui,
                cat.label(),
                state.anim_list_filter == Some(cat),
                &mut state.anim_list_filter,
                Some(cat),
            );
        }
    });
    ui.add_space(4.0);

    let list_filter = state.anim_list_filter;
    let visible_anims: Vec<(usize, twg_shared::models::Anim)> = state
        .anims
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            list_filter.is_none_or(|cat| AnimCategory::from_anim_type(&a.anim_type) == cat)
        })
        .map(|(global_idx, a)| (global_idx, a.clone()))
        .collect();

    let mut select_anim: Option<twg_shared::models::Anim> = None;
    let mut play_anim_id: Option<uuid::Uuid> = None;
    let mut delete_anim_id: Option<uuid::Uuid> = None;
    let mut reorder_action: Option<(uuid::Uuid, bool)> = None;
    let mut fly_to: Option<(f64, f64)> = None;

    egui::ScrollArea::vertical()
        .id_salt("anim_tab_ordered_list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if visible_anims.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(if total_count == 0 {
                        "Belum ada animasi.\nPilih kategori dan buat animasi melalui tombol di atas."
                    } else {
                        "Tidak ada animasi di filter kategori ini.\nPilih 'Semua' untuk melihat seluruh urutan skenario."
                    })
                    .size(11.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                return;
            }

            let vis_len = visible_anims.len();
            for (vis_pos, (global_idx, a)) in visible_anims.iter().enumerate() {
                let is_sel = state.selected == SelectedEntity::Anim(a.id);
                let is_locked = state.locked_entity_ids.contains(&a.id);
                let is_multi = is_multi_route_anim(a);
                let bound_ids = anim_bound_marker_ids(a);
                let len_m = route_total_length_m(&a.geom);
                let can_play = anim_is_playable(a, &state.markers);
                let cat = AnimCategory::from_anim_type(&a.anim_type);
                let (cat_tag, cat_col) = match cat {
                    AnimCategory::Route if is_traffic_signal_anim(a) => {
                        ("LAMPU MERAH", egui::Color32::from_rgb(225, 29, 72))
                    }
                    AnimCategory::Route if is_traffic_anim(a) => {
                        ("TRAFFIC", egui::Color32::from_rgb(217, 119, 6))
                    }
                    AnimCategory::Route if is_traffic_gridlock_anim(a) => {
                        ("MACET TOTAL", egui::Color32::from_rgb(190, 18, 60))
                    }
                    AnimCategory::Route if is_traffic_manual_anim(a) => {
                        ("SIMPANG MANUAL", egui::Color32::from_rgb(79, 70, 229))
                    }
                    AnimCategory::Route if is_traffic_queue_anim(a) => {
                        ("URAI SIMPANG", egui::Color32::from_rgb(13, 148, 136))
                    }
                    AnimCategory::Route if is_traffic_diversion_anim(a) => {
                        ("ALIH ARUS", egui::Color32::from_rgb(2, 132, 199))
                    }
                    AnimCategory::Route if is_multi => {
                        if parse_multi_route_config(a).formation == RouteFormationKind::Patwal {
                            ("PATWAL", egui::Color32::from_rgb(37, 99, 235))
                        } else {
                            ("FORMASI", egui::Color32::from_rgb(14, 116, 144))
                        }
                    }
                    AnimCategory::Route => ("RUTE", egui::Color32::from_rgb(5, 150, 105)),
                    AnimCategory::Audio if twg_shared::billboard_anim::is_billboard_anim(a) => {
                        if twg_shared::billboard_anim::parse_billboard_anim_config(a).media_mode
                            == twg_shared::billboard_anim::BillboardMediaMode::Video
                        {
                            ("VIDEOTRON", egui::Color32::from_rgb(2, 132, 199))
                        } else {
                            ("BILLBOARD", egui::Color32::from_rgb(14, 116, 144))
                        }
                    }
                    AnimCategory::Audio => ("PLAY SUARA", egui::Color32::from_rgb(109, 40, 217)),
                    AnimCategory::Camera => {
                        if twg_shared::camera_anim::is_camera_static_anim(a) {
                            ("STATIC CAM", egui::Color32::from_rgb(2, 132, 199))
                        } else {
                            ("FOLLOW CAM", egui::Color32::from_rgb(5, 150, 105))
                        }
                    }
                    AnimCategory::Environment => {
                        if twg_shared::environment_anim::is_water_anim(a) {
                            match twg_shared::environment_anim::parse_water_anim_config(a).kind {
                                twg_shared::environment_anim::WaterSimKind::Ocean => {
                                    ("LAUT", egui::Color32::from_rgb(2, 132, 199))
                                }
                                twg_shared::environment_anim::WaterSimKind::River => {
                                    ("SUNGAI", egui::Color32::from_rgb(13, 148, 136))
                                }
                                twg_shared::environment_anim::WaterSimKind::Flood => {
                                    ("BANJIR", egui::Color32::from_rgb(180, 83, 9))
                                }
                            }
                        } else {
                            match twg_shared::environment_anim::parse_rain_anim_config(a).preset {
                                twg_shared::environment_anim::RainPreset::Storm => {
                                    ("BADAI", egui::Color32::from_rgb(124, 58, 237))
                                }
                                _ => ("HUJAN", egui::Color32::from_rgb(79, 70, 229)),
                            }
                        }
                    }
                    AnimCategory::Shoot => match shoot_anim_kind(a).unwrap_or_default() {
                        ShootAnimKind::WaterCannon => {
                            ("WATER CANNON", egui::Color32::from_rgb(2, 132, 199))
                        }
                        ShootAnimKind::StoneThrow => ("BATU", egui::Color32::from_rgb(180, 83, 9)),
                        ShootAnimKind::TearGas => {
                            ("GAS AIR MATA", egui::Color32::from_rgb(71, 85, 105))
                        }
                        ShootAnimKind::Bullet => ("PELURU", egui::Color32::from_rgb(185, 28, 28)),
                        ShootAnimKind::Apar => ("APAR", egui::Color32::from_rgb(15, 118, 110)),
                    },
                    AnimCategory::Particle => match parse_particle_anim_config(a).kind {
                        ParticleAnimKind::Fire => ("API", egui::Color32::from_rgb(234, 88, 12)),
                        ParticleAnimKind::Smoke => ("ASAP", egui::Color32::from_rgb(71, 85, 105)),
                        ParticleAnimKind::Explosion => {
                            ("LEDAKAN", egui::Color32::from_rgb(185, 28, 28))
                        }
                    },
                    AnimCategory::SpawnHide => match spawn_hide_action(a).unwrap_or_default() {
                        SpawnHideAction::Spawn => ("SPAWN", egui::Color32::from_rgb(5, 150, 105)),
                        SpawnHideAction::Hide => ("HIDE", egui::Color32::from_rgb(190, 18, 60)),
                    },
                    AnimCategory::Group => match group_anim_mode(a).unwrap_or_default() {
                        GroupAnimMode::Combine => ("COMBINE", egui::Color32::from_rgb(37, 99, 235)),
                        GroupAnimMode::Sequence => {
                            ("SEQUENCE", egui::Color32::from_rgb(124, 58, 237))
                        }
                    },
                    AnimCategory::Other => {
                        if is_pointer_arrow_anim(a) {
                            ("PANAH 3D", egui::Color32::from_rgb(180, 83, 9))
                        } else {
                            ("LAIN", egui::Color32::from_rgb(14, 116, 144))
                        }
                    }
                };

                egui::Frame::new()
                    .fill(if is_sel {
                        egui::Color32::from_rgb(20, 48, 66)
                    } else {
                        egui::Color32::from_rgb(22, 30, 45)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if is_sel {
                            egui::Color32::from_rgb(56, 189, 248)
                        } else {
                            egui::Color32::from_rgb(38, 52, 74)
                        },
                    ))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::same(7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        // Row 1: #Order + [CAT] + Name + Right-aligned Up/Down sort buttons
                        ui.horizontal(|ui| {
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 23, 42))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(51, 65, 85),
                                ))
                                .corner_radius(3.0)
                                .inner_margin(egui::Margin::symmetric(4, 1))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("#{}", global_idx + 1))
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(125, 211, 252)),
                                    );
                                });

                            egui::Frame::new()
                                .fill(cat_col)
                                .corner_radius(3.0)
                                .inner_margin(egui::Margin::symmetric(4, 1))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(cat_tag)
                                            .size(8.5)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    );
                                });

                            let title_resp = ui.selectable_label(
                                is_sel,
                                egui::RichText::new(&a.name)
                                    .size(11.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                            if title_resp.clicked() {
                                select_anim = Some(a.clone());
                            }
                            if is_locked {
                                ui.label(
                                    egui::RichText::new("[LOCK]")
                                        .size(9.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(251, 191, 36)),
                                );
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    let can_down = editable && vis_pos + 1 < vis_len;
                                    let can_up = editable && vis_pos > 0;
                                    if draw_reorder_arrow_btn(ui, false, can_down)
                                        .on_hover_text("Turunkan urutan (Down)")
                                        .clicked()
                                        && can_down
                                    {
                                        reorder_action = Some((a.id, false));
                                    }
                                    if draw_reorder_arrow_btn(ui, true, can_up)
                                        .on_hover_text("Naikkan urutan (Up)")
                                        .clicked()
                                        && can_up
                                    {
                                        reorder_action = Some((a.id, true));
                                    }
                                },
                            );
                        });

                        let is_particle = is_particle_anim(a);
                        let is_group = is_group_anim(a);
                        let has_units = !bound_ids.is_empty();
                        let is_ready_meta = if is_particle {
                            !a.geom.is_empty()
                        } else if twg_shared::camera_anim::is_camera_anim(a)
                            || twg_shared::environment_anim::is_environment_anim(a)
                        {
                            twg_shared::route_formation::anim_is_playable(a, &state.markers)
                        } else if is_group {
                            !parse_group_anim_config(a).items.is_empty()
                        } else {
                            has_units
                        };
                        let meta = if twg_shared::audio_anim::is_audio_anim(a) {
                            super::audio_anim::sound_name(state, a)
                        } else if twg_shared::camera_anim::is_camera_anim(a) {
                            super::camera_anim::anim_summary(state, a)
                        } else if twg_shared::environment_anim::is_environment_anim(a) {
                            super::environment_anim::anim_summary(state, a)
                        } else if is_particle {
                            let cfg = parse_particle_anim_config(a);
                            let alt = particle_anim_position(a)[2];
                            if cfg.continuous {
                                format!(
                                    "Radius {:.1}m  •  Tinggi {:.1}m  •  Alt {:.1}m  •  Terus",
                                    cfg.radius_m, cfg.height_m, alt
                                )
                            } else {
                                format!(
                                    "Radius {:.1}m  •  Tinggi {:.1}m  •  {:.1} dtk{}",
                                    cfg.radius_m,
                                    cfg.height_m,
                                    a.duration_sec,
                                    if a.replay { "  •  Repeat" } else { "" }
                                )
                            }
                        } else if is_group {
                            let cfg = parse_group_anim_config(a);
                            let names: Vec<String> = cfg
                                .items
                                .iter()
                                .filter_map(|it| {
                                    state
                                        .anims
                                        .iter()
                                        .find(|x| x.id == it.anim_id)
                                        .map(|x| x.name.clone())
                                })
                                .collect();
                            if names.is_empty() {
                                "Belum ada animasi anggota (tambah di Inspector)".to_string()
                            } else {
                                let sep = match cfg.mode {
                                    GroupAnimMode::Combine => " + ",
                                    GroupAnimMode::Sequence => " → ",
                                };
                                let preview = if names.len() <= 2 {
                                    names.join(sep)
                                } else {
                                    format!("{}{sep}+{} lagi", names[0], names.len() - 1)
                                };
                                match cfg.mode {
                                    GroupAnimMode::Combine => {
                                        format!("{} Animasi ({preview})  •  Bareng", names.len())
                                    }
                                    GroupAnimMode::Sequence => {
                                        let trig = match cfg.sequence_trigger {
                                            twg_shared::group_anim::SequenceTriggerMode::WaitComplete => {
                                                "Tunggu Selesai".to_string()
                                            }
                                            twg_shared::group_anim::SequenceTriggerMode::WaitDelay => {
                                                format!("Jeda {:.2}s", cfg.sequence_delay_sec)
                                            }
                                        };
                                        format!(
                                            "{} Langkah ({preview})  •  {trig}{}",
                                            names.len(),
                                            if a.replay { "  •  Repeat" } else { "" }
                                        )
                                    }
                                }
                            }
                        } else if is_spawn_hide_anim(a) {
                            let cfg = parse_spawn_hide_config(a);
                            let names: Vec<String> = cfg
                                .marker_ids
                                .iter()
                                .filter_map(|mid| {
                                    state
                                        .markers
                                        .iter()
                                        .find(|m| m.id == *mid)
                                        .map(|m| m.name.clone())
                                })
                                .collect();
                            if names.is_empty() {
                                "Belum pilih Target Objek 3D".to_string()
                            } else {
                                let preview = if names.len() <= 2 {
                                    names.join(", ")
                                } else {
                                    format!("{}, +{} lagi", names[0], names.len() - 1)
                                };
                                let mode_seq = if cfg.stagger_sec > 0.001 {
                                    format!("Urut +{:.2}s", cfg.stagger_sec)
                                } else {
                                    "Serentak".to_string()
                                };
                                format!(
                                    "{} Objek ({preview})  •  {}  •  {mode_seq}",
                                    names.len(),
                                    cfg.transition.short_label()
                                )
                            }
                        } else if twg_shared::billboard_anim::is_billboard_anim(a) {
                            super::billboard_anim::asset_summary(state, a)
                        } else if is_traffic_signal_anim(a) {
                            let cfg = parse_traffic_anim_config(a);
                            let active_lanes = cfg.lanes.iter().filter(|l| l.enabled && l.geom.len() >= 2).count();
                            let lights = cfg.traffic_lights.iter().filter(|l| l.enabled).count();
                            let phases = cfg.effective_signal_phases().len();
                            format!(
                                "{active_lanes} Jalur  •  {lights} Lampu ({phases} Fase)  •  {} Model",
                                cfg.vehicle_pool.len(),
                            )
                        } else if is_traffic_anim(a) {
                            let cfg = parse_traffic_anim_config(a);
                            let active_lanes = cfg.lanes.iter().filter(|l| l.enabled && l.geom.len() >= 2).count();
                            format!(
                                "{active_lanes} Jalur  •  {} Model Pool  •  {:.0}–{:.0} m/s{}",
                                cfg.vehicle_pool.len(),
                                cfg.min_speed_mps,
                                cfg.max_speed_mps,
                                if cfg.allow_overtaking { "  •  Salip" } else { "" }
                            )
                        } else if is_traffic_queue_anim(a) {
                            let cfg = parse_traffic_queue_config(a);
                            let complete = cfg.corridors.iter().filter(|c| c.enabled && c.is_complete()).count();
                            let total = cfg.corridors.iter().filter(|c| c.enabled).count();
                            if is_traffic_gridlock_anim(a) {
                                format!(
                                    "{complete}/{total} Jalur  •  {} Model  •  {}",
                                    cfg.vehicle_pool.len(),
                                    if cfg.prefill_initial_queue {
                                        "Pre-fill Diam"
                                    } else {
                                        "Mengalir Macet"
                                    }
                                )
                            } else if is_traffic_manual_anim(a) {
                                format!(
                                    "{complete}/{total} Jalur  •  {} Model  •  Kontrol Manual Live",
                                    cfg.vehicle_pool.len()
                                )
                            } else {
                                format!(
                                    "{complete}/{total} Jalur Lengkap  •  {} Model  •  {:.0}s giliran",
                                    cfg.vehicle_pool.len(),
                                    cfg.turn_duration_sec
                                )
                            }
                        } else if is_traffic_diversion_anim(a) {
                            let cfg = parse_traffic_diversion_config(a);
                            let complete = cfg.corridors.iter().filter(|c| c.enabled && c.is_complete()).count();
                            let total = cfg.corridors.iter().filter(|c| c.enabled).count();
                            format!(
                                "{complete}/{total} Koridor Y  •  {} Model  •  {}",
                                cfg.vehicle_pool.len(),
                                if cfg.start_diverted {
                                    "Awal Dialihkan"
                                } else {
                                    "Awal Normal"
                                }
                            )
                        } else if is_pointer_arrow_anim(a) {
                            let cfg = parse_pointer_arrow_config(a);
                            let unit_name = a
                                .marker_id
                                .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                                .map(|m| m.name.clone());
                            let lbl_part = match cfg.effective_label() {
                                Some(t) => format!("Teks: \"{t}\""),
                                None => "Tanpa teks".to_string(),
                            };
                            match &unit_name {
                                Some(u) => format!(
                                    "Target: {u}  •  {lbl_part}{}",
                                    if cfg.continuous { "  •  Terus" } else { "" }
                                ),
                                None => "Belum pilih Target Objek 3D".to_string(),
                            }
                        } else if let Some(sk) = shoot_anim_kind(a) {
                            let unit_name = a
                                .marker_id
                                .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                                .map(|m| m.name.clone());
                            match &unit_name {
                                Some(u) => match sk {
                                    ShootAnimKind::WaterCannon => {
                                        let cfg = parse_water_cannon_config(a);
                                        format!(
                                            "Unit: {u}  •  Jarak {:.0}m  •  {:.1} bar{}",
                                            cfg.range_m,
                                            cfg.pressure_bar,
                                            if cfg.continuous { "  •  Terus" } else { "" }
                                        )
                                    }
                                    ShootAnimKind::StoneThrow => {
                                        let cfg = parse_stone_throw_config(a);
                                        format!(
                                            "Unit: {u}  •  Jarak {:.0}m  •  {:.1}/dtk{}",
                                            cfg.range_m,
                                            cfg.throw_rate_hz,
                                            if cfg.continuous { "  •  Terus" } else { "" }
                                        )
                                    }
                                    ShootAnimKind::TearGas => {
                                        let cfg = parse_tear_gas_config(a);
                                        format!(
                                            "Unit: {u}  •  Jarak {:.0}m  •  Asap {:.0} dtk{}",
                                            cfg.range_m,
                                            cfg.smoke_duration_sec,
                                            if cfg.continuous { "  •  Terus" } else { "" }
                                        )
                                    }
                                    ShootAnimKind::Bullet => {
                                        let cfg = parse_bullet_shoot_config(a);
                                        format!(
                                            "Unit: {u}  •  Hilang {:.0}m  •  {:.1}/dtk{}",
                                            cfg.max_range_m,
                                            cfg.fire_rate_rps,
                                            if cfg.continuous { "  •  Terus" } else { "" }
                                        )
                                    }
                                    ShootAnimKind::Apar => {
                                        let cfg = parse_apar_spray_config(a);
                                        format!(
                                            "Unit: {u}  •  Jarak {:.1}m  •  APAR{}",
                                            cfg.range_m,
                                            if cfg.continuous { "  •  Terus" } else { "" }
                                        )
                                    }
                                },
                                None => "Belum pilih Model 3D".to_string(),
                            }
                        } else if is_multi {
                            let cfg = parse_multi_route_config(a);
                            if has_units {
                                format!(
                                    "{} ({} unit)  •  {len_m:.0}m  •  {:.1} m/s{}",
                                    cfg.formation.short_label(),
                                    bound_ids.len(),
                                    a.speed,
                                    if a.replay { "  •  Repeat" } else { "" }
                                )
                            } else {
                                format!(
                                    "Belum pilih unit formasi  •  {len_m:.0}m  •  {} titik",
                                    a.geom.len()
                                )
                            }
                        } else {
                            let unit_name = a
                                .marker_id
                                .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                                .map(|m| m.name.clone());
                            match &unit_name {
                                Some(u) => format!(
                                    "Unit: {u}  •  {len_m:.0}m  •  {:.1} m/s{}",
                                    a.speed,
                                    if a.replay { "  •  Repeat" } else { "" }
                                ),
                                None => format!(
                                    "Belum pilih Model 3D  •  {len_m:.0}m  •  {} titik",
                                    a.geom.len()
                                ),
                            }
                        };
                        ui.label(
                            egui::RichText::new(meta)
                                .size(10.0)
                                .color(if is_ready_meta {
                                    egui::Color32::from_rgb(148, 163, 184)
                                } else {
                                    egui::Color32::from_rgb(251, 191, 36)
                                }),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.small_button("Edit").clicked() {
                                select_anim = Some(a.clone());
                            }
                            let go_target = a
                                .geom
                                .first()
                                .map(|p| (p[0], p[1]))
                                .or_else(|| {
                                    a.marker_id
                                        .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                                        .map(|m| (m.lng, m.lat))
                                });
                            if ui
                                .add_enabled(go_target.is_some(), egui::Button::new("Go").small())
                                .clicked()
                                && let Some((lng, lat)) = go_target
                            {
                                fly_to = Some((lng, lat));
                            }
                            let play_btn = egui::Button::new(
                                egui::RichText::new("Play")
                                    .size(10.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(if can_play {
                                egui::Color32::from_rgb(5, 150, 105)
                            } else {
                                egui::Color32::from_rgb(51, 65, 85)
                            })
                            .small();
                            if ui.add_enabled(can_play, play_btn).clicked() {
                                play_anim_id = Some(a.id);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let del = egui::Button::new(
                                        egui::RichText::new("x")
                                            .size(10.5)
                                            .color(egui::Color32::from_rgb(252, 165, 165)),
                                    )
                                    .fill(egui::Color32::from_rgb(90, 24, 24))
                                    .small();
                                    if ui
                                        .add_enabled(editable && !is_locked, del)
                                        .on_hover_text("Hapus animasi")
                                        .clicked()
                                    {
                                        delete_anim_id = Some(a.id);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        });

    if let Some((aid, move_up)) = reorder_action {
        let changed = state.move_anim_in_order(aid, move_up, list_filter);
        if !changed.is_empty() {
            state.status_banner = Some(format!(
                "Urutan animasi diperbarui — {} perubahan belum disimpan (Simpan / Ctrl+S).",
                state.unsaved_change_count()
            ));
        }
    }
    if let Some(a) = select_anim {
        let is_p = is_particle_anim(&a);
        let is_s = shoot_anim_kind(&a).is_some();
        state.selected = SelectedEntity::Anim(a.id);
        state.anim_edit = Some(a);
        if is_p {
            state.particle_preview_reset_nonce = state.particle_preview_reset_nonce.wrapping_add(1);
        }
        if is_s {
            state.shoot_preview_reset_nonce = state.shoot_preview_reset_nonce.wrapping_add(1);
        }
    }
    if let Some(aid) = play_anim_id {
        playback.play_single_immediately(aid);
    }
    if let Some(id) = delete_anim_id {
        state.request_delete_entity(SelectedEntity::Anim(id));
    }
    if let Some((lng, lat)) = fly_to {
        let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
        tcam.target_local = bevy::math::Vec2::new(local[0], local[2]);
    }
}
