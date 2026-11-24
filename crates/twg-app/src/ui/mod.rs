mod assets_page;
mod bottom_bar;
mod formation_pad;
mod left_panel;
mod login;
mod media_assets_ui;
pub mod nav_bar;
mod operasi_page;
mod overlays;
mod pagination;
mod play_panel;
mod right_panel;
mod skenario_page;
pub mod spinner;
mod table;
mod topbar;

use bevy::{camera::Viewport, prelude::*, window::PrimaryWindow};
use bevy_egui::{EguiContext, EguiContexts, egui};

use crate::{
    map::camera::{TacticalCamera, screen_pos_to_world_ground},
    net::{
        asset_sync::AssetSyncBridge,
        client::ApiClientBridge,
        ws_client::WsClientBridge,
    },
    scene::{
        asset_preview::AssetPreviewStage,
        basic_mesh::{BasicMeshConfig, BasicMeshShape},
        gizmo::{TransformGizmoState, draw_transform_gizmo},
        markers::{
            MarkerWorldBounds, execute_delete_entity, spawn_basic_mesh_marker,
            spawn_marker_from_catalog,
        },
        media_preview::MediaPreviewPlayer,
        playback::PlaybackOverrides,
        polygon_edit::{PolygonEditState, PolygonHoverTarget, draw_polygon_vertex_editor},
        route_edit::{RouteEditState, RouteHoverTarget, draw_route_editor_overlay},
    },
    state::{AppScreen, AppState, PlaybackState, ToolMode, UiPointerState},
};

#[allow(clippy::too_many_arguments)]
pub fn egui_root_system(
    mut contexts: EguiContexts,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    marker_bounds: Res<MarkerWorldBounds>,
    gizmo_state: Res<TransformGizmoState>,
    mut poly_edit: ResMut<PolygonEditState>,
    mut route_edit: ResMut<RouteEditState>,
    mut preview_stage: ResMut<AssetPreviewStage>,
    mut media_player: ResMut<MediaPreviewPlayer>,
    mut ui_state: ResMut<UiPointerState>,
    api: Res<ApiClientBridge>,
    ws: Res<WsClientBridge>,
    sync_bridge: Res<AssetSyncBridge>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam_q: Query<(&mut Camera, &mut TacticalCamera, &GlobalTransform), Without<EguiContext>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let screen_rect = ctx.viewport_rect();
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(screen_rect),
    );

    if state.screen == AppScreen::Login {
        login::draw_login_screen(&mut viewport_ui, &mut state, &api, &sync_bridge);
        ui_state.wants_pointer = true;
        ui_state.wants_keyboard = true;
        return Ok(());
    }

    // Enforce admin-only access to the Assets page
    if state.screen == AppScreen::Assets && !state.is_super_admin() {
        state.screen = AppScreen::Operasis;
    }

    if state.screen == AppScreen::Operasis {
        operasi_page::draw_operasi_screen(
            &mut viewport_ui,
            ctx,
            &mut state,
            &mut playback,
            &api,
            &ws,
            &sync_bridge,
        );
        ui_state.wants_pointer = true;
        ui_state.wants_keyboard = true;
        ui_state.viewport_rect_logical = None;
        return Ok(());
    }

    if state.screen == AppScreen::Skenarios && !state.is_skenario_map_preview_active() {
        skenario_page::draw_skenario_screen(
            &mut viewport_ui,
            ctx,
            &mut state,
            &mut playback,
            &api,
            &ws,
            &sync_bridge,
        );
        ui_state.wants_pointer = true;
        ui_state.wants_keyboard = true;
        ui_state.viewport_rect_logical = None;
        return Ok(());
    }

    let Ok((mut camera, mut tcam, cam_gtf)) = cam_q.single_mut() else {
        return Ok(());
    };

    if state.is_skenario_map_preview_active() {
        let insets = skenario_page::draw_skenario_form_screen(
            &mut viewport_ui,
            ctx,
            &mut state,
            &mut playback,
            &api,
            &ws,
            &sync_bridge,
        );
        let center_rect = apply_camera_viewport(
            &mut camera,
            &window,
            screen_rect,
            insets.left,
            insets.right,
            insets.top,
            insets.bottom,
        );
        ui_state.viewport_rect_logical = Some(center_rect);

        let pointer_in_center = ctx
            .pointer_latest_pos()
            .is_some_and(|p| center_rect.contains(p));
        ui_state.wants_pointer = state.preset_manager.open
            || !pointer_in_center
            || ctx.egui_wants_pointer_input()
            || ctx.is_pointer_over_egui();
        ui_state.wants_keyboard = state.preset_manager.open || ctx.egui_wants_keyboard_input();
        return Ok(());
    }

    if state.screen == AppScreen::Assets {
        let insets = assets_page::draw_assets_screen(
            &mut viewport_ui,
            ctx,
            &mut state,
            &mut playback,
            &mut preview_stage,
            &mut media_player,
            &api,
            &ws,
            &sync_bridge,
        );
        let center_rect = apply_camera_viewport(
            &mut camera,
            &window,
            screen_rect,
            insets.left,
            insets.right,
            insets.top,
            insets.bottom,
        );
        ui_state.viewport_rect_logical = Some(center_rect);

        let pointer_in_center = ctx
            .pointer_latest_pos()
            .is_some_and(|p| center_rect.contains(p));
        ui_state.wants_pointer = state.preset_manager.open
            || !pointer_in_center
            || ctx.egui_wants_pointer_input()
            || ctx.is_pointer_over_egui();
        ui_state.wants_keyboard = state.preset_manager.open || ctx.egui_wants_keyboard_input();
        return Ok(());
    }

    // Dedicated Scenario Plot Editor (hide top toolbar & editor panels during Play Skenario)
    let top = if playback.play_test_mode {
        0.0
    } else {
        topbar::draw_top_bar(
            &mut viewport_ui,
            &mut state,
            &mut tcam,
            &mut playback,
            &api,
            &ws,
            &sync_bridge,
        )
    };
    let bottom = bottom_bar::draw_bottom_bar(&mut viewport_ui, &mut state, &tcam, &mut playback, &ws);
    let (left, right) = if playback.play_test_mode {
        // Play Skenario Mode: hide Editor panels (Layers, 3D Catalog, Basic Mesh, Inspector)
        // and show the dedicated Animation Player panel on the left.
        let play_left = play_panel::draw_play_animation_panel(
            &mut viewport_ui,
            &mut state,
            &mut tcam,
            &mut playback,
            &ws,
        );
        (play_left, 0.0)
    } else {
        let l = left_panel::draw_left_panel(
            &mut viewport_ui,
            &mut state,
            &mut tcam,
            &mut playback,
            &api,
        );
        let r = right_panel::draw_right_panel(
            &mut viewport_ui,
            ctx,
            &mut state,
            &mut tcam,
            &mut playback,
            &mut poly_edit,
            &mut route_edit,
            &mut media_player,
            &api,
            &ws,
        );
        (l, r)
    };

    let center_rect =
        apply_camera_viewport(&mut camera, &window, screen_rect, left, right, top, bottom);
    ui_state.viewport_rect_logical = Some(center_rect);

    overlays::draw_projected_labels(
        ctx,
        center_rect,
        &state,
        &playback,
        &overrides,
        &marker_bounds,
        &camera,
        cam_gtf,
    );

    draw_transform_gizmo(
        ctx,
        center_rect,
        &state,
        &playback,
        &gizmo_state,
        &poly_edit,
        &camera,
        cam_gtf,
    );

    draw_polygon_vertex_editor(
        ctx,
        center_rect,
        &state,
        &playback,
        &poly_edit,
        &camera,
        cam_gtf,
    );

    draw_route_editor_overlay(
        ctx,
        center_rect,
        &state,
        &playback,
        &route_edit,
        &gizmo_state,
        &marker_bounds,
        &camera,
        cam_gtf,
    );

    // 3D Catalog or Basic Mesh Drag-and-Drop onto the 3D map viewport
    if let Some(drag_entry) = state.dragging_catalog_entry.clone() {
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        if esc_pressed || playback.play_test_mode {
            state.dragging_catalog_entry = None;
            state.catalog_drop_world_pos = None;
        } else {
            let drop_hit = pointer_pos.and_then(|p| {
                if center_rect.contains(p) {
                    screen_pos_to_world_ground(Vec2::new(p.x, p.y), &camera, cam_gtf)
                } else {
                    None
                }
            });
            state.catalog_drop_world_pos = drop_hit;

            if let Some(ptr) = pointer_pos {
                let drop_lnglat = drop_hit.map(|h| {
                    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([h.x, 0.0, h.z]);
                    (lng, lat)
                });
                overlays::draw_catalog_drag_ghost(ctx, ptr, &drag_entry, drop_lnglat);
            }
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);

            if !primary_down {
                state.dragging_catalog_entry = None;
                state.catalog_drop_world_pos = None;
                if let Some(hit) = drop_hit {
                    let (lng, lat, _) =
                        state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
                    if spawn_marker_from_catalog(&mut state, &api, &drag_entry, lng, lat) {
                        state.status_banner = Some(format!(
                            "Model '{}' diletakkan di peta (Lat {:.5}, Lng {:.5})",
                            drag_entry.name, lat, lng
                        ));
                    }
                }
            }
        }
    } else if let Some(drag_shape) = state.dragging_basic_mesh {
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        if esc_pressed || playback.play_test_mode {
            state.dragging_basic_mesh = None;
            state.catalog_drop_world_pos = None;
        } else {
            let drop_hit = pointer_pos.and_then(|p| {
                if center_rect.contains(p) {
                    screen_pos_to_world_ground(Vec2::new(p.x, p.y), &camera, cam_gtf)
                } else {
                    None
                }
            });
            state.catalog_drop_world_pos = drop_hit;

            if let Some(ptr) = pointer_pos {
                let drop_lnglat = drop_hit.map(|h| {
                    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([h.x, 0.0, h.z]);
                    (lng, lat)
                });
                overlays::draw_basic_mesh_drag_ghost(ctx, ptr, drag_shape, drop_lnglat);
            }
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);

            if !primary_down {
                state.dragging_basic_mesh = None;
                state.catalog_drop_world_pos = None;
                if let Some(hit) = drop_hit {
                    let (lng, lat, _) =
                        state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
                    if drag_shape == BasicMeshShape::Polygon {
                        state.tool_mode = ToolMode::DrawBasicPolygon;
                        state.draft_points_lnglat = vec![[lng, lat]];
                        state.status_banner = Some(
                            "Titik #1 Polygon diletakkan! Klik titik berikutnya di peta, lalu klik titik #1 / Enter untuk selesai."
                                .into(),
                        );
                    } else if spawn_basic_mesh_marker(
                        &mut state,
                        &api,
                        BasicMeshConfig::default_for(drag_shape),
                        None,
                        lng,
                        lat,
                    ) {
                        state.status_banner = Some(format!(
                            "Basic Mesh '{}' diletakkan di peta (Lat {:.5}, Lng {:.5})",
                            drag_shape.label(),
                            lat,
                            lng
                        ));
                    }
                }
            }
        }
    } else if route_edit.is_dragging() || poly_edit.is_dragging() || gizmo_state.is_dragging() {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if matches!(
        route_edit.hovered,
        Some(RouteHoverTarget::GizmoHandle(_) | RouteHoverTarget::Waypoint(_))
    ) || matches!(poly_edit.hovered, Some(PolygonHoverTarget::GizmoHandle(_)))
        || gizmo_state.hovered_handle.is_some()
    {
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
    } else if matches!(
        route_edit.hovered,
        Some(RouteHoverTarget::Midpoint(_) | RouteHoverTarget::DeleteWaypoint(_))
    ) || matches!(
        poly_edit.hovered,
        Some(PolygonHoverTarget::Vertex(_) | PolygonHoverTarget::Midpoint(_))
    ) || gizmo_state.hovered_entity.is_some()
    {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if route_edit.add_point_on_map_mode
        || poly_edit.add_point_on_map_mode
        || matches!(
            state.tool_mode,
            ToolMode::DrawBasicPolygon
                | ToolMode::DrawBuilding
                | ToolMode::DrawLine
                | ToolMode::DrawRoute
                | ToolMode::PickRouteMarker
                | ToolMode::PickShootMarker
                | ToolMode::PlaceParticle
        )
    {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }

    // Modal confirmation dialog before deleting any map object
    draw_delete_entity_confirm_modal(ctx, &mut state, &mut poly_edit, &mut route_edit, &api);

    let has_modal = state.pending_delete_entity.is_some();
    let pointer_in_center = ctx
        .pointer_latest_pos()
        .is_some_and(|p| center_rect.contains(p));
    ui_state.wants_pointer = has_modal
        || state.dragging_catalog_entry.is_some()
        || state.dragging_basic_mesh.is_some()
        || !pointer_in_center
        || ctx.egui_wants_pointer_input()
        || ctx.is_pointer_over_egui();
    ui_state.wants_keyboard = has_modal || ctx.egui_wants_keyboard_input();

    Ok(())
}

fn draw_delete_entity_confirm_modal(
    ctx: &egui::Context,
    state: &mut AppState,
    poly_edit: &mut PolygonEditState,
    route_edit: &mut RouteEditState,
    api: &ApiClientBridge,
) {
    let Some(pending) = state.pending_delete_entity.clone() else {
        return;
    };

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.pending_delete_entity = None;
        return;
    }

    let screen = ctx.viewport_rect();
    let backdrop_painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("delete_entity_modal_backdrop"),
    ));
    backdrop_painter.rect_filled(
        screen,
        0.0,
        egui::Color32::from_rgba_unmultiplied(4, 8, 16, 175),
    );

    let mut confirm = false;
    let mut cancel = false;

    egui::Window::new("Konfirmasi Hapus Objek")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size(egui::vec2(380.0, 0.0))
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 39))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(185, 28, 28)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(16))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 2,
                    color: egui::Color32::from_black_alpha(150),
                }),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(127, 29, 29))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(7, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("HAPUS")
                                .size(10.0)
                                .strong()
                                .color(egui::Color32::from_rgb(254, 202, 202)),
                        );
                    });
                ui.label(
                    egui::RichText::new("Konfirmasi Hapus Objek")
                        .size(14.5)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
            });

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Apakah Anda yakin ingin menghapus objek berikut dari peta?")
                    .size(12.0)
                    .color(egui::Color32::from_rgb(203, 213, 225)),
            );
            ui.add_space(8.0);

            egui::Frame::new()
                .fill(egui::Color32::from_rgb(12, 18, 30))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(30, 58, 138))
                            .corner_radius(3.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(&pending.kind_label)
                                        .size(10.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(191, 219, 254)),
                                );
                            });
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&pending.name)
                                    .size(12.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .wrap(),
                        );
                    });
                });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Tindakan ini tidak dapat dibatalkan.")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(248, 113, 113)),
            );

            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let del_btn = egui::Button::new(
                    egui::RichText::new("Ya, Hapus")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(185, 28, 28))
                .min_size(egui::vec2(96.0, 28.0));
                if ui.add(del_btn).clicked() {
                    confirm = true;
                }

                let cancel_btn = egui::Button::new(
                    egui::RichText::new("Batal")
                        .size(12.0)
                        .color(egui::Color32::from_rgb(226, 232, 240)),
                )
                .fill(egui::Color32::from_rgb(30, 41, 59))
                .min_size(egui::vec2(78.0, 28.0));
                if ui.add(cancel_btn).clicked() {
                    cancel = true;
                }
            });
        });

    if confirm {
        state.pending_delete_entity = None;
        if let crate::state::SelectedEntity::Marker(mid) = pending.entity
            && poly_edit.is_editing(mid)
        {
            poly_edit.clear();
        }
        if let crate::state::SelectedEntity::Anim(aid) = pending.entity
            && route_edit.is_editing(aid)
        {
            route_edit.clear();
        }
        execute_delete_entity(state, api, pending.entity);
    } else if cancel {
        state.pending_delete_entity = None;
    }
}

fn apply_camera_viewport(
    camera: &mut Camera,
    window: &Window,
    screen_rect: egui::Rect,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> egui::Rect {
    let center_rect = egui::Rect::from_min_max(
        egui::pos2(left, top),
        egui::pos2(
            (screen_rect.max.x - right).max(left + 64.0),
            (screen_rect.max.y - bottom).max(top + 64.0),
        ),
    );

    let sf = window.scale_factor();
    let max_w = window.physical_width();
    let max_h = window.physical_height();
    if max_w >= 4 && max_h >= 4 {
        let pos_x = ((left * sf).round() as u32).min(max_w.saturating_sub(4));
        let pos_y = ((top * sf).round() as u32).min(max_h.saturating_sub(4));
        let right_px = (right * sf).round() as u32;
        let bottom_px = (bottom * sf).round() as u32;
        let avail_w = max_w.saturating_sub(pos_x);
        let avail_h = max_h.saturating_sub(pos_y);
        let size_w = avail_w.saturating_sub(right_px).clamp(4, avail_w);
        let size_h = avail_h.saturating_sub(bottom_px).clamp(4, avail_h);
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(pos_x, pos_y),
            physical_size: UVec2::new(size_w, size_h),
            ..default()
        });
    }

    center_rect
}
