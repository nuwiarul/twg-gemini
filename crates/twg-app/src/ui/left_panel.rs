use bevy_egui::egui;
use twg_shared::api::UpdateMarkerRequest;

use crate::{
    map::camera::TacticalCamera,
    net::client::{ApiClientBridge, ApiCommand},
    scene::{
        basic_mesh::{BasicMeshShape, is_basic_mesh_marker, parse_basic_mesh_config},
        markers::{current_auth, spawn_basic_mesh_at_map_center, spawn_marker_at_map_center},
    },
    state::{
        AppState, LayerLockFilter, LayerVisibilityFilter, LeftTab, PlaybackState, SelectedEntity,
        ToolMode,
    },
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
        .default_size(290.0)
        .min_size(230.0)
        .max_size(430.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 23, 35))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 66)))
                .inner_margin(egui::Margin::same(8)),
        )
        .show(viewport_ui, |ui| {
            // Godot / Unity Dock Tab Bar: Layers | 3D Catalog | Basic Mesh
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(12, 17, 27))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::same(3))
                .show(ui, |ui| {
                    ui.columns(3, |cols| {
                        for (idx, (tab, title, active_fill)) in [
                            (LeftTab::Layers, "Layers", egui::Color32::from_rgb(14, 116, 144)),
                            (LeftTab::Catalog, "3D Catalog", egui::Color32::from_rgb(14, 116, 144)),
                            (LeftTab::BasicMesh, "Basic Mesh", egui::Color32::from_rgb(109, 40, 217)),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let is_active = state.left_tab == tab;
                            let btn = egui::Button::new(
                                egui::RichText::new(title)
                                    .size(11.0)
                                    .strong()
                                    .color(if is_active {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(148, 163, 184)
                                    }),
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
            p.rect_filled(rect, 3.0, egui::Color32::from_rgba_unmultiplied(148, 163, 184, 34));
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
                [egui::pos2(c.x - 5.2, c.y - 5.2), egui::pos2(c.x + 5.2, c.y + 5.2)],
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
            p.rect_filled(rect, 3.0, egui::Color32::from_rgba_unmultiplied(148, 163, 184, 34));
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
            p.rect_stroke(body, 1.5, egui::Stroke::new(1.2, col), egui::StrokeKind::Middle);
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

fn draw_filter_pill(ui: &mut egui::Ui, label: &str, active: bool, active_fill: egui::Color32) -> bool {
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
            .add_enabled(!state.layer_search.is_empty(), egui::Button::new("X").small())
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
                                                    egui::RichText::new("Go")
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            125, 211, 252,
                                                        )),
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
                                                    egui::RichText::new("Go")
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            125, 211, 252,
                                                        )),
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

                                ui.label(
                                    egui::RichText::new("[BLD]")
                                        .size(9.5)
                                        .strong()
                                        .color(if sel {
                                            egui::Color32::from_rgb(167, 243, 208)
                                        } else if !vis {
                                            egui::Color32::from_rgb(100, 116, 139)
                                        } else {
                                            egui::Color32::from_rgb(52, 211, 153)
                                        }),
                                );

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
                                                    egui::RichText::new("Go")
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            125, 211, 252,
                                                        )),
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
                                                    egui::RichText::new("Go")
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            125, 211, 252,
                                                        )),
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
                                                    egui::RichText::new("Go")
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            125, 211, 252,
                                                        )),
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

            // 5. Route Animations
            let anims = state.anims.clone();
            let anim_total = anims.len();
            let anim_filtered = anims
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
                for a in &anims {
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
                                                    egui::RichText::new("Play")
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            52, 211, 153,
                                                        )),
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
                            "  (belum ada rute — gunakan tool 'Draw Route')"
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
        playback.play_all_anims = false;
        playback.active_anim_id = Some(aid);
        playback.restart_play_test();
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
    ui.add(
        egui::TextEdit::singleline(&mut state.catalog_search)
            .hint_text("Cari unit, kendaraan, bangunan...")
            .desired_width(f32::INFINITY),
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
            let dragging_id = state
                .dragging_catalog_entry
                .as_ref()
                .map(|e| e.id.clone());

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
                                egui::RichText::new("+ Tambah")
                                    .size(10.5)
                                    .strong()
                                    .color(if editable {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(100, 116, 139)
                                    }),
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
