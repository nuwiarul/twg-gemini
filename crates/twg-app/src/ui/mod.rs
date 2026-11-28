pub mod app_icon;
mod assets_page;
mod audio_anim;
mod billboard_anim;
pub(crate) mod camera_anim;
pub(crate) mod dialog_anim;
pub(crate) mod environment_anim;
pub(crate) mod hud_banner_anim;
pub(crate) mod model_action_anim;
pub(crate) mod tactical_arrow_anim;
pub(crate) mod video_recorder_ui;
mod bottom_bar;
mod formation_pad;
mod graphics_page;
mod import_panel;
mod layar_besar_page;
mod left_panel;
mod login;
mod media_assets_ui;
pub mod nav_bar;
mod operasi_page;
mod overlays;
mod pagination;
pub mod password_input;
mod play_panel;
mod profile_page;
mod right_panel;
mod satker_page;
mod skenario_page;
pub mod spinner;
mod table;
mod tactical_page;
mod topbar;
mod user_page;

use bevy::{camera::Viewport, prelude::*, window::PrimaryWindow};
use bevy_egui::{EguiContext, EguiContexts, egui};

use crate::{
    map::camera::{TacticalCamera, screen_pos_to_world_ground},
    net::{asset_sync::AssetSyncBridge, client::ApiClientBridge, ws_client::WsClientBridge},
    scene::{
        asset_preview::AssetPreviewStage,
        basic_mesh::{BasicMeshConfig, BasicMeshShape},
        benchmark::{PerformanceBenchmarkState, draw_benchmark_overlay},
        gizmo::{TransformGizmoState, draw_transform_gizmo},
        markers::{
            MarkerWorldBounds, execute_delete_entity, spawn_basic_mesh_marker,
            spawn_marker_from_catalog,
        },
        media_preview::MediaPreviewPlayer,
        play_rts::PlayRtsState,
        playback::PlaybackOverrides,
        plot_line_edit::{PlotLineEditState, draw_plot_line_overlay},
        pointer_arrow::PointerArrowVisualPool,
        polygon_edit::{PolygonEditState, PolygonHoverTarget, draw_polygon_vertex_editor},
        route_edit::{RouteEditState, RouteHoverTarget, draw_route_editor_overlay},
        tactical_arrow::TacticalArrowVisualPool,
        video_recorder::VideoRecorderState,
    },
    state::{AppScreen, AppState, PlaybackState, PlotExit, ToolMode, UiPointerState},
};

#[allow(clippy::too_many_arguments)]
pub fn egui_root_system(
    mut commands: Commands,
    mut contexts: EguiContexts,
    time: Res<Time>,
    mut state: ResMut<AppState>,
    (mut playback, mut play_rts, mut recorder, mut bench): (
        ResMut<PlaybackState>,
        ResMut<PlayRtsState>,
        ResMut<VideoRecorderState>,
        ResMut<PerformanceBenchmarkState>,
    ),
    (overrides, marker_bounds, pointer_pool, tactical_arrow_pool): (
        Res<PlaybackOverrides>,
        Res<MarkerWorldBounds>,
        Res<PointerArrowVisualPool>,
        Res<TacticalArrowVisualPool>,
    ),
    gizmo_state: Res<TransformGizmoState>,
    (mut poly_edit, plot_line_edit, mut route_edit): (
        ResMut<PolygonEditState>,
        Res<PlotLineEditState>,
        ResMut<RouteEditState>,
    ),
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

    // Enforce role-based screen guards:
    // Dedicated Layar Besar accounts (`layar_besar`, `layar_besar_satker`) can ONLY view Layar Besar or Profile.
    if state.is_big_screen_only_role()
        && !matches!(
            state.screen,
            AppScreen::LayarBesar | AppScreen::Profile | AppScreen::Graphics
        )
    {
        nav_bar::enter_layar_besar(&mut state, &mut playback, &ws);
    }
    if state.screen == AppScreen::Assets && !state.can_view_assets_screen() {
        state.screen = AppScreen::Operasis;
    }
    if state.screen == AppScreen::Satkers && !state.can_manage_satkers() {
        state.screen = AppScreen::Operasis;
    }
    if state.screen == AppScreen::Users && !state.can_manage_users() {
        state.screen = AppScreen::Operasis;
    }

    if state.screen == AppScreen::Graphics {
        graphics_page::draw_graphics_screen(
            &mut viewport_ui,
            &mut state,
            &mut playback,
            &mut bench,
            &api,
            &ws,
            &sync_bridge,
        );
        ui_state.wants_pointer = true;
        ui_state.wants_keyboard = true;
        ui_state.viewport_rect_logical = None;
        return Ok(());
    }

    if state.screen == AppScreen::Profile {
        profile_page::draw_profile_screen(
            &mut viewport_ui,
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

    if state.screen == AppScreen::Satkers {
        satker_page::draw_satker_screen(
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

    if state.screen == AppScreen::Users && !state.is_user_assets_assign_preview_active() {
        user_page::draw_user_screen(
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

    if state.screen == AppScreen::Tactical {
        tactical_page::draw_tactical_screen(
            &mut viewport_ui,
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

    if state.is_user_assets_assign_preview_active() {
        let insets = user_page::draw_user_assign_assets_screen(
            &mut viewport_ui,
            ctx,
            &mut state,
            &mut playback,
            &mut preview_stage,
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
        ui_state.wants_pointer = !pointer_in_center
            || ctx.egui_wants_pointer_input()
            || ctx.is_pointer_over_egui();
        ui_state.wants_keyboard = ctx.egui_wants_keyboard_input();
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

    // Dedicated Fullscreen Read-Only Layar Besar Display (camera & map interaction 100% locked)
    if state.screen == AppScreen::LayarBesar {
        let center_rect =
            apply_camera_viewport(&mut camera, &window, screen_rect, 0.0, 0.0, 0.0, 0.0);
        ui_state.viewport_rect_logical = Some(center_rect);

        overlays::draw_projected_labels(
            ctx,
            center_rect,
            &state,
            &playback,
            &overrides,
            &marker_bounds,
            &pointer_pool,
            &tactical_arrow_pool,
            &camera,
            cam_gtf,
        );
        overlays::draw_hud_banners(
            ctx,
            center_rect,
            &state,
            &playback,
            time.elapsed_secs_f64(),
        );
        crate::branding::draw_layar_besar_brand_hud(ctx, center_rect);
        overlays::draw_compass_and_telemetry_hud(
            ctx,
            center_rect,
            &state,
            &mut tcam,
            &camera,
            false,
        );
        layar_besar_page::draw_layar_besar_overlay(
            ctx,
            center_rect,
            &mut state,
            &mut tcam,
            &mut playback,
            &mut play_rts,
            &mut recorder,
            &api,
            &ws,
            &sync_bridge,
        );
        draw_benchmark_overlay(ctx, center_rect, &mut bench, &state, &tcam);

        ui_state.wants_pointer = true;
        ui_state.wants_keyboard = true;
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
    if playback.play_test_mode && state.import_panel.preview_open {
        media_player.stop_all_playback();
        state.import_panel.close_preview();
    }

    let bottom =
        bottom_bar::draw_bottom_bar(&mut viewport_ui, &mut state, &tcam, &mut playback, &ws);
    let (left, right) = if playback.play_test_mode {
        // Play Skenario Mode: hide Editor panels (Layers, 3D Catalog, Basic Mesh, Inspector)
        // and show the dedicated Animation Player panel on the left.
        let play_left = play_panel::draw_play_animation_panel(
            &mut viewport_ui,
            &mut state,
            &mut tcam,
            &mut playback,
            &mut play_rts,
            &mut recorder,
            &api,
            &ws,
        );
        (play_left, 0.0)
    } else {
        let l = left_panel::draw_left_panel(
            &mut viewport_ui,
            &mut state,
            &mut tcam,
            &mut playback,
            &mut media_player,
            &api,
        );
        let r = if state.is_plot_import_preview_active() {
            import_panel::draw_import_preview_right_panel(
                &mut viewport_ui,
                &mut state,
                &mut preview_stage,
                &tcam,
                &playback,
                &mut media_player,
                &api,
            )
        } else {
            right_panel::draw_right_panel(
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
            )
        };
        (l, r)
    };

    let center_rect =
        apply_camera_viewport(&mut camera, &window, screen_rect, left, right, top, bottom);
    ui_state.viewport_rect_logical = Some(center_rect);

    // Temporary Center Viewport Preview for Plot Editor Import tab (3D Model / Texture / Sound / Video)
    if state.is_plot_import_preview_active() {
        if let Some(media_type) = state.is_plot_import_media_preview_active() {
            media_assets_ui::draw_media_center_preview(
                ctx,
                center_rect,
                &mut state,
                &mut media_player,
                &sync_bridge,
                media_type,
            );
            let mut close_preview = false;
            egui::Area::new(egui::Id::new("import_media_back_to_map_btn"))
                .fixed_pos(egui::pos2(
                    (center_rect.right() - 164.0).max(center_rect.left() + 12.0),
                    center_rect.top() + 14.0,
                ))
                .show(ctx, |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("< Kembali ke Peta")
                                    .size(11.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(14, 116, 144))
                            .min_size(egui::vec2(148.0, 28.0)),
                        )
                        .clicked()
                    {
                        close_preview = true;
                    }
                });
            if close_preview {
                media_player.stop_all_playback();
                state.import_panel.close_preview();
            }
            draw_plot_exit_confirm_modal(ctx, &mut commands, &mut state, &mut playback, &api, &ws);
            ui_state.wants_pointer = true;
            ui_state.wants_keyboard = ctx.egui_wants_keyboard_input();
            return Ok(());
        } else {
            import_panel::draw_import_3d_center_hud(ctx, center_rect, &mut state);
            draw_plot_exit_confirm_modal(ctx, &mut commands, &mut state, &mut playback, &api, &ws);
            let has_modal = state.pending_plot_exit.is_some();
            let pointer_in_center = ctx
                .pointer_latest_pos()
                .is_some_and(|p| center_rect.contains(p));
            ui_state.wants_pointer = has_modal
                || !pointer_in_center
                || ctx.egui_wants_pointer_input()
                || ctx.is_pointer_over_egui();
            ui_state.wants_keyboard = has_modal || ctx.egui_wants_keyboard_input();
            return Ok(());
        }
    }

    overlays::draw_projected_labels(
        ctx,
        center_rect,
        &state,
        &playback,
        &overrides,
        &marker_bounds,
        &pointer_pool,
        &tactical_arrow_pool,
        &camera,
        cam_gtf,
    );
    overlays::draw_hud_banners(
        ctx,
        center_rect,
        &state,
        &playback,
        time.elapsed_secs_f64(),
    );
    draw_benchmark_overlay(ctx, center_rect, &mut bench, &state, &tcam);

    draw_transform_gizmo(
        ctx,
        center_rect,
        &state,
        &playback,
        &overrides,
        &play_rts,
        &gizmo_state,
        &poly_edit,
        &camera,
        cam_gtf,
    );

    overlays::draw_play_rts_overlay(ctx, center_rect, &state, &playback, &play_rts);

    if state.is_tactical_live() && playback.play_test_mode {
        overlays::draw_compass_and_telemetry_hud(
            ctx,
            center_rect,
            &state,
            &mut tcam,
            &camera,
            true,
        );
    }

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
    draw_plot_line_overlay(
        ctx,
        center_rect,
        &state,
        &playback,
        &plot_line_edit,
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
                    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
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
                    let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
                    if drag_shape == BasicMeshShape::Polygon {
                        state.tool_mode = ToolMode::DrawBasicPolygon;
                        state.draft_points_lnglat = vec![[lng, lat]];
                        state.status_banner = Some(
                            "Titik #1 Polygon diletakkan! Klik titik berikutnya di peta, lalu klik titik #1 / Enter untuk selesai."
                                .into(),
                        );
                    } else if drag_shape == BasicMeshShape::GroundPolygon {
                        state.tool_mode = ToolMode::DrawGroundPolygon;
                        state.draft_points_lnglat = vec![[lng, lat]];
                        state.status_banner = Some(
                            "Titik #1 Ground Polygon diletakkan! Klik titik berikutnya di peta, lalu klik titik #1 / Enter untuk selesai."
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
    } else if play_rts.marquee_drag.is_some()
        || route_edit.add_point_on_map_mode
        || poly_edit.add_point_on_map_mode
        || matches!(
            state.tool_mode,
            ToolMode::DrawBasicPolygon
                | ToolMode::DrawGroundPolygon
                | ToolMode::DrawBuilding
                | ToolMode::DrawLine
                | ToolMode::DrawRoute
                | ToolMode::DrawTrafficLane
                | ToolMode::DrawTrafficQueueLane
                | ToolMode::DrawTrafficDiversionLane
                | ToolMode::PickRouteMarker
                | ToolMode::PickShootMarker
                | ToolMode::PickPointerMarker
                | ToolMode::PickSpawnHideMarker
                | ToolMode::PickCameraFollowMarker
                | ToolMode::PickModelActionMarker
                | ToolMode::PickDialogSpeaker
                | ToolMode::PlaceParticle
                | ToolMode::DrawWaterPolygon
                | ToolMode::DrawTacticalArrow
                | ToolMode::PlaceRainZone
        )
    {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }

    // Modal confirmation dialog before deleting any map object
    draw_delete_entity_confirm_modal(ctx, &mut state, &mut poly_edit, &mut route_edit, &api);
    draw_plot_exit_confirm_modal(ctx, &mut commands, &mut state, &mut playback, &api, &ws);

    let has_modal = state.pending_delete_entity.is_some() || state.pending_plot_exit.is_some();
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

fn draw_plot_exit_confirm_modal(
    ctx: &egui::Context,
    commands: &mut Commands,
    state: &mut AppState,
    playback: &mut PlaybackState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) {
    let Some(action) = state.pending_plot_exit else {
        return;
    };

    let saving = state.saving_pending_count > 0;
    let mut proceed = !saving && state.unsaved_change_count() == 0;
    let mut save = false;
    let mut discard = false;
    let mut cancel = false;

    if !proceed {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            state.pending_plot_exit = None;
            return;
        }

        let screen = ctx.viewport_rect();
        egui::Area::new(egui::Id::new("plot_exit_confirmation_backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(4, 8, 16, 175),
                );
            });

        let change_count = state.unsaved_change_count();
        egui::Window::new("Simpan Perubahan Plot Editor")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .fixed_size(egui::vec2(430.0, 0.0))
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(17, 24, 39))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(217, 119, 6)))
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
                        .fill(egui::Color32::from_rgb(120, 53, 15))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(7, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("PERUBAHAN")
                                    .size(10.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(254, 243, 199)),
                            );
                        });
                    ui.label(
                        egui::RichText::new("Simpan Perubahan?")
                            .size(14.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                });

                ui.add_space(10.0);
                let message = if saving {
                    "Perubahan sedang disimpan. Mohon tunggu hingga proses selesai."
                } else {
                    "Ada perubahan Plot Editor yang belum disimpan. Simpan sebelum melanjutkan?"
                };
                ui.label(
                    egui::RichText::new(message)
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
                                .fill(egui::Color32::from_rgb(146, 64, 14))
                                .corner_radius(3.0)
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("PLOT EDITOR")
                                            .size(10.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(254, 243, 199)),
                                    );
                                });
                            ui.label(
                                egui::RichText::new(if saving {
                                    "Menyimpan perubahan...".to_string()
                                } else {
                                    format!("{change_count} perubahan belum disimpan")
                                })
                                .size(12.5)
                                .strong()
                                .color(egui::Color32::WHITE),
                            );
                        });
                    });

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(if saving {
                        "Jangan tutup aplikasi hingga penyimpanan selesai."
                    } else {
                        "Pilih Jangan Simpan untuk membuang perubahan ini."
                    })
                    .size(11.0)
                    .color(egui::Color32::from_rgb(251, 191, 36)),
                );

                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_button = egui::Button::new(
                        egui::RichText::new(if saving { "Menyimpan..." } else { "Ya, Simpan" })
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(14, 116, 144))
                    .min_size(egui::vec2(104.0, 28.0));
                    if ui.add_enabled(!saving, save_button).clicked() {
                        save = true;
                    }

                    let discard_button = egui::Button::new(
                        egui::RichText::new("Jangan Simpan")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(254, 202, 202)),
                    )
                    .fill(egui::Color32::from_rgb(127, 29, 29))
                    .min_size(egui::vec2(116.0, 28.0));
                    if ui.add_enabled(!saving, discard_button).clicked() {
                        discard = true;
                    }

                    let cancel_button = egui::Button::new(
                        egui::RichText::new("Batal")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(226, 232, 240)),
                    )
                    .fill(egui::Color32::from_rgb(30, 41, 59))
                    .min_size(egui::vec2(78.0, 28.0));
                    if ui.add(cancel_button).clicked() {
                        cancel = true;
                    }
                });
            });
    }

    if save {
        crate::scene::markers::save_all_scenario_changes(state, api, ws);
        if state.saving_pending_count == 0 {
            if state.unsaved_change_count() == 0 {
                proceed = true;
            } else {
                state.pending_plot_exit = None;
            }
        }
    } else if discard {
        state.discard_plot_changes();
        proceed = true;
    } else if cancel {
        state.pending_plot_exit = None;
    }

    if proceed {
        state.pending_plot_exit = None;
        match action {
            PlotExit::Page(page) => topbar::leave_plot(page, state, playback, api, ws),
            PlotExit::Close(window) => {
                commands.entity(window).try_despawn();
            }
        }
    }
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

/// Renders a single-line search box paired with a `Reset` button that clears `value`
/// (and returns `true` when clicked so callers can also clear extra filter pills).
pub fn draw_search_input_with_reset(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
    extra_active: bool,
    reset_tooltip: &str,
) -> bool {
    let mut reset_clicked = false;
    let has_any_filter = !value.is_empty() || extra_active;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let btn_w = 48.0_f32;
        let input_w = (ui.available_width() - btn_w - 4.0).max(90.0);
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(input_w),
        );
        let btn = egui::Button::new(egui::RichText::new("Reset").size(10.5).strong().color(
            if has_any_filter {
                egui::Color32::from_rgb(254, 202, 202)
            } else {
                egui::Color32::from_rgb(100, 116, 139)
            },
        ))
        .fill(if has_any_filter {
            egui::Color32::from_rgb(127, 29, 29)
        } else {
            egui::Color32::from_rgb(30, 41, 59)
        });
        if ui
            .add_enabled(has_any_filter, ui_btn_sized(btn, btn_w, 21.0))
            .on_hover_text(reset_tooltip)
            .on_disabled_hover_text("Pencarian sudah kosong")
            .clicked()
        {
            value.clear();
            reset_clicked = true;
        }
    });
    reset_clicked
}

fn ui_btn_sized(btn: egui::Button<'_>, w: f32, h: f32) -> egui::Button<'_> {
    btn.min_size(egui::vec2(w, h))
}

pub fn handle_close_requests(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    primary: Query<(), With<PrimaryWindow>>,
    mut state: ResMut<AppState>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if primary.contains(request.window)
            && state.screen == AppScreen::PlotSkenario
            && (state.unsaved_change_count() > 0 || state.saving_pending_count > 0)
        {
            state
                .pending_plot_exit
                .get_or_insert(PlotExit::Close(request.window));
        } else {
            commands.entity(request.window).try_despawn();
        }
    }
}

#[cfg(test)]
mod plot_exit_tests {
    use super::*;
    use bevy::window::WindowCloseRequested;

    #[test]
    fn close_waits_for_confirmation_only_when_plot_has_changes() {
        let mut app = App::new();
        let mut state = AppState::new(".".into());
        state.screen = AppScreen::PlotSkenario;
        state.skenario_dirty = true;
        app.insert_resource(state)
            .add_message::<WindowCloseRequested>()
            .add_systems(Update, handle_close_requests);
        let window = app.world_mut().spawn(PrimaryWindow).id();
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        assert!(app.world().get_entity(window).is_ok());
        assert!(matches!(
            app.world().resource::<AppState>().pending_plot_exit,
            Some(PlotExit::Close(id)) if id == window
        ));

        let mut state = app.world_mut().resource_mut::<AppState>();
        state.pending_plot_exit = None;
        state.clear_all_dirty();
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        assert!(app.world().get_entity(window).is_err());
    }
}
