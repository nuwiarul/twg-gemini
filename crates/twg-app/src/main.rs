#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod map;
mod net;
mod scene;
mod state;
mod ui;

use std::path::PathBuf;

use bevy::{
    camera::{CameraOutputMode, visibility::RenderLayers},
    light::Skybox,
    prelude::*,
    render::render_resource::BlendState,
};
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use twg_shared::geo::GeoAnchor;

use crate::{
    map::{
        camera::{TacticalCamera, camera_control_system},
        indoor::{indoor_grid_system, setup_indoor_ground},
        tiles::{TileManager, tile_streaming_system},
    },
    net::{
        asset_sync::{AssetSyncBridge, AssetSyncCommand, AssetSyncEvent},
        client::{ApiClientBridge, ApiCommand, ApiEvent},
        ws_client::{WsClientBridge, WsInEvent, WsOutCommand},
    },
    scene::{
        asset_preview::{
            AssetPreviewStage, asset_preview_camera_system, asset_preview_stage_system,
            setup_asset_preview_stage,
        },
        basic_mesh::BasicMeshTextureCache,
        buildings::{draw_building_draft_gizmos, sync_buildings_system},
        gizmo::TransformGizmoState,
        lines::draw_lines_and_routes_system,
        markers::{
            MarkerWorldBounds, animate_markers_system, current_auth, map_interaction_system,
            normalize_skinned_mesh_weights_system, selection_gizmo_system,
            setup_selection_highlight_system, sync_markers_system,
        },
        media_preview::{MediaPreviewPlayer, media_preview_audio_system},
        particle_fx::{ParticleFxPool, particle_fx_visual_system},
        playback::{PlaybackOverrides, playback_system},
        polygon_edit::PolygonEditState,
        route_edit::RouteEditState,
        skybox::{FillSkyLight, PrimarySunLight, SkyboxManager, sync_skybox_system},
        water_cannon::{WaterCannonVisualPool, water_cannon_visual_system},
    },
    state::{
        AppScreen, AppState, AssetFormMode, MediaFormMode, OperasiModalMode, PlaybackState,
        SelectedEntity, SkenarioModalMode, ToolMode, UiPointerState,
    },
    ui::{
        egui_root_system,
        nav_bar::{trigger_load_operasi_options, trigger_load_operasis, trigger_load_skenarios},
    },
};

fn resolve_client_assets_dir() -> PathBuf {
    let dir = std::env::var("TWG_LOCAL_ASSETS_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // Keep client offline synced models in a dedicated folder so server -> client
            // offline sync works identically on the same machine and on any remote PC.
            let cwd_workspace = PathBuf::from("client_assets");
            if PathBuf::from("Cargo.toml").exists() {
                cwd_workspace
            } else if let Ok(exe) = std::env::current_exe()
                && let Some(parent) = exe.parent()
            {
                parent.join("offline_assets")
            } else {
                cwd_workspace
            }
        });

    let _ = std::fs::create_dir_all(&dir);
    clean_absolute_path(&dir)
}

fn clean_absolute_path(path: &std::path::Path) -> PathBuf {
    let abs = std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        abs
    }
}

fn main() {
    let assets_dir = resolve_client_assets_dir();
    let asset_path_str = assets_dir.to_string_lossy().into_owned();
    let initial_state = AppState::new(assets_dir.clone());
    let sync_bridge = AssetSyncBridge::start();
    // Automatically check server & sync 3D models offline on every launch
    sync_bridge.send(AssetSyncCommand::SyncWithServer {
        server_url: initial_state.server_url.clone(),
        local_root: assets_dir,
    });

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.04, 0.06, 0.10)))
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 550.0,
            ..default()
        })
        .insert_resource(initial_state)
        .init_resource::<UiPointerState>()
        .init_resource::<PlaybackState>()
        .init_resource::<PlaybackOverrides>()
        .init_resource::<TransformGizmoState>()
        .init_resource::<PolygonEditState>()
        .init_resource::<RouteEditState>()
        .init_resource::<WaterCannonVisualPool>()
        .init_resource::<ParticleFxPool>()
        .init_resource::<SkyboxManager>()
        .init_resource::<MarkerWorldBounds>()
        .init_resource::<BasicMeshTextureCache>()
        .init_resource::<AssetPreviewStage>()
        .init_resource::<MediaPreviewPlayer>()
        .insert_resource(ApiClientBridge::start())
        .insert_resource(WsClientBridge::start())
        .insert_resource(sync_bridge)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Tactical Wall Game 3D — Full Rust".into(),
                        resolution: (1440, 900).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_path_str,
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .add_systems(
            Startup,
            (
                setup_scene_system,
                setup_indoor_ground,
                setup_asset_preview_stage,
                setup_selection_highlight_system,
            ),
        )
        .add_systems(
            Update,
            (
                process_asset_sync_events_system,
                process_net_events_system,
                map_interaction_system,
                camera_control_system,
                asset_preview_camera_system,
                asset_preview_stage_system,
                media_preview_audio_system,
                tile_streaming_system,
                indoor_grid_system,
                sync_skybox_system,
                playback_system,
                sync_markers_system,
                animate_markers_system,
                selection_gizmo_system,
                sync_buildings_system,
                draw_building_draft_gizmos,
                draw_lines_and_routes_system,
                water_cannon_visual_system,
                particle_fx_visual_system,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            normalize_skinned_mesh_weights_system.before(bevy::asset::AssetEventSystems),
        )
        .add_systems(EguiPrimaryContextPass, egui_root_system)
        .run();
}

fn setup_scene_system(
    mut commands: Commands,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    egui_global_settings.auto_create_primary_context = false;

    let tile_mgr = TileManager::start(&mut meshes);
    commands.insert_resource(tile_mgr);

    // Sun & fill directional lights so glTF models look crisp from all angles
    commands.spawn((
        PrimarySunLight,
        DirectionalLight {
            illuminance: 14_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(120.0, 260.0, 140.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        FillSkyLight,
        DirectionalLight {
            illuminance: 5_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-140.0, 180.0, -120.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // World 3D Camera
    let tcam = TacticalCamera::default();
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Skybox {
            image: None,
            brightness: 1000.0,
            rotation: Quat::IDENTITY,
        },
        Transform::from_xyz(0.0, 460.0, 460.0).looking_at(Vec3::ZERO, Vec3::Y),
        tcam,
    ));

    // Egui Overlay Camera
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

fn process_asset_sync_events_system(
    mut state: ResMut<AppState>,
    sync_bridge: Res<AssetSyncBridge>,
    asset_server: Res<AssetServer>,
) {
    for evt in sync_bridge.drain_events() {
        match evt {
            AssetSyncEvent::Checking { server_url } => {
                state.sync_status.is_syncing = true;
                state.sync_status.checking = true;
                state.sync_status.offline_mode = false;
                state.sync_status.last_error = None;
                state.sync_status.summary_text =
                    format!("Mengecek model terbaru di {server_url}...");
            }
            AssetSyncEvent::ManifestLoaded {
                manifest,
                to_download,
                to_delete,
                download_bytes,
                already_synced,
            } => {
                let total_models = manifest.total_models;
                state.apply_manifest(manifest);
                state.sync_status.checking = false;
                state.sync_status.completed_files = 0;
                state.sync_status.total_files = to_download;
                state.sync_status.downloaded_bytes = 0;
                state.sync_status.total_bytes = download_bytes;
                if to_download == 0 {
                    state.sync_status.summary_text = format!(
                        "{total_models} model ({already_synced} file) up-to-date (-{to_delete} dihapus)"
                    );
                } else {
                    state.sync_status.summary_text = format!(
                        "Menyinkronkan {to_download} file ({:.1} MB)...",
                        download_bytes as f64 / (1024.0 * 1024.0)
                    );
                }
            }
            AssetSyncEvent::Progress {
                completed_files,
                total_files,
                downloaded_bytes,
                total_bytes,
                current_file,
            } => {
                state.sync_status.completed_files = completed_files;
                state.sync_status.total_files = total_files;
                state.sync_status.downloaded_bytes = downloaded_bytes;
                state.sync_status.total_bytes = total_bytes;
                state.sync_status.current_file = current_file;
            }
            AssetSyncEvent::FileSynced {
                rel_path,
                was_updated,
            } => {
                state.available_model_paths.insert(rel_path.clone());
                if rel_path.starts_with("media/") {
                    state.media_reload_nonce = state.media_reload_nonce.wrapping_add(1);
                }
                if was_updated {
                    asset_server.reload(rel_path);
                    state.preview_reload_nonce = state.preview_reload_nonce.wrapping_add(1);
                    state.media_reload_nonce = state.media_reload_nonce.wrapping_add(1);
                }
            }
            AssetSyncEvent::FileRemoved { rel_path } => {
                state.available_model_paths.remove(&rel_path);
            }
            AssetSyncEvent::Completed {
                manifest,
                downloaded,
                updated,
                deleted,
                skipped,
            } => {
                let total_models = manifest.total_models;
                let total_media = manifest.total_media;
                state.apply_manifest(manifest);
                state.sync_status.is_syncing = false;
                state.sync_status.checking = false;
                state.sync_status.offline_mode = false;
                state.sync_status.summary_text = if downloaded == 0 && updated == 0 && deleted == 0
                {
                    format!(
                        "Tersinkron offline: {total_models} 3D, {total_media} media ({skipped} file)"
                    )
                } else {
                    format!(
                        "Sinkron selesai: {total_models} 3D, {total_media} media (+{downloaded}, ~{updated}, -{deleted})"
                    )
                };
                if updated > 0 {
                    state.preview_reload_nonce = state.preview_reload_nonce.wrapping_add(1);
                    state.media_reload_nonce = state.media_reload_nonce.wrapping_add(1);
                }
            }
            AssetSyncEvent::OfflineFallback {
                reason,
                cached_manifest,
            } => {
                state.sync_status.is_syncing = false;
                state.sync_status.checking = false;
                state.sync_status.offline_mode = true;
                state.sync_status.last_error = Some(reason.clone());
                if state.model_assets.is_empty()
                    && let Some(manifest) = cached_manifest
                {
                    state.apply_manifest(manifest);
                }
                state.sync_status.summary_text = format!(
                    "Offline ({} model di cache lokal) — {reason}",
                    state.model_assets.len()
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_net_events_system(
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    api: Res<ApiClientBridge>,
    ws: Res<WsClientBridge>,
    sync_bridge: Res<AssetSyncBridge>,
    mut cam_q: Query<&mut TacticalCamera>,
) {
    for evt in api.drain_events() {
        match evt {
            ApiEvent::LoginOk(login) => {
                let role = login.user.role;
                let username = login.user.username.clone();
                state.loading = false;
                state.login_error = None;
                state.offline_preview = false;
                state.auth_token = Some(login.access_token.clone());
                state.refresh_token = Some(login.refresh_token);
                state.current_user = Some(login.user);
                state.screen = AppScreen::Operasis;
                state.operasi_table.page = 1;
                state.operasi_table.search.clear();
                state.operasi_table.applied_search.clear();
                state.status_banner = Some(format!(
                    "Login berhasil sebagai {username} [{}] — Daftar Operasi",
                    role.as_str()
                ));

                // Re-sync 3D models with the authenticated server URL
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });

                trigger_load_operasis(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
                if let Some(auth) = current_auth(&state) {
                    api.send(ApiCommand::LoadLocationPresets { auth: auth.clone() });
                    if role.as_str() != "user" {
                        api.send(ApiCommand::LoadSatkers { auth });
                    }
                }
                ws.send(WsOutCommand::Connect {
                    server_url: state.server_url.clone(),
                    token: login.access_token,
                    device_id: state.device_id.clone(),
                    skenario_id: state.active_skenario.as_ref().map(|s| s.id),
                });
            }
            ApiEvent::LoginErr(err) => {
                state.loading = false;
                state.login_error = Some(err);
            }
            ApiEvent::OperasisLoaded(paged) => {
                state.operasis = paged.items;
                state.operasi_table.total = paged.total;
                state.operasi_table.page = paged.page;
                state.operasi_table.per_page = paged.per_page;
                state.operasi_table.loading = false;
                state.operasi_table.error = None;

                if let Some(cur_op_id) = state.skenario_table.operasi.as_ref().map(|o| o.id)
                    && let Some(updated) = state.operasis.iter().find(|o| o.id == cur_op_id).cloned()
                {
                    state.skenario_table.operasi = Some(updated);
                }
            }
            ApiEvent::OperasiOptionsLoaded(items) => {
                state.skenario_table.operasi_options = items;
                if let Some(fid) = state.skenario_table.filter_operasi_id {
                    state.skenario_table.operasi = state
                        .skenario_table
                        .operasi_options
                        .iter()
                        .find(|o| o.id == fid)
                        .cloned();
                }
            }
            ApiEvent::OperasiCreated(op) => {
                let name = op.name.clone();
                state.operasi_table.close_modal();
                state.operasi_table.page = 1;
                state.status_banner = Some(format!("Operasi '{name}' berhasil ditambahkan"));
                trigger_load_operasis(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
            }
            ApiEvent::OperasiUpdated(op) => {
                let name = op.name.clone();
                if state
                    .skenario_table
                    .operasi
                    .as_ref()
                    .is_some_and(|o| o.id == op.id)
                {
                    state.skenario_table.operasi = Some(op);
                }
                state.operasi_table.close_modal();
                state.status_banner = Some(format!("Operasi '{name}' berhasil diperbarui"));
                trigger_load_operasis(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
            }
            ApiEvent::OperasiDeleted(id) => {
                state.operasi_table.close_modal();
                if state.selected_operasi_id == Some(id) {
                    state.selected_operasi_id = None;
                }
                if state.skenario_table.filter_operasi_id == Some(id) {
                    state.skenario_table.filter_operasi_id = None;
                    state.skenario_table.operasi = None;
                    state.skenarios.clear();
                    state.skenario_table.total = 0;
                }
                if state
                    .active_skenario
                    .as_ref()
                    .is_some_and(|s| s.operasi_id == id)
                {
                    state.active_skenario = None;
                }
                if state.operasis.len() <= 1 && state.operasi_table.page > 1 {
                    state.operasi_table.page -= 1;
                }
                state.status_banner = Some("Operasi berhasil dihapus".into());
                trigger_load_operasis(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
            }
            ApiEvent::OperasiError(err) => {
                state.operasi_table.loading = false;
                state.operasi_table.submitting = false;
                if matches!(state.operasi_table.modal_mode, OperasiModalMode::None) {
                    state.operasi_table.error = Some(err.clone());
                } else {
                    state.operasi_table.form_error = Some(err.clone());
                }
                state.status_banner = Some(err);
            }
            ApiEvent::SatkersLoaded(satkers) => {
                state.operasi_table.satkers = satkers;
            }
            ApiEvent::LocationPresetsLoaded(presets) => {
                state.location_presets = presets;
            }
            ApiEvent::LocationPresetCreated(preset) => {
                let name = preset.name.clone();
                let pid = preset.id;
                state.location_presets.retain(|p| p.id != pid);
                state.location_presets.push(preset);
                state.location_presets.sort_by(|a, b| {
                    a.sort_order.cmp(&b.sort_order).then_with(|| a.name.cmp(&b.name))
                });
                let next_sort = state
                    .location_presets
                    .iter()
                    .map(|p| p.sort_order)
                    .max()
                    .unwrap_or(50)
                    + 10;
                state.preset_manager.submitting = false;
                state.preset_manager.quick_save_open = false;
                state.preset_manager.quick_save_name.clear();
                state.preset_manager.reset_form(next_sort);
                state.preset_manager.success =
                    Some(format!("Preset lokasi '{name}' berhasil disimpan ke master data!"));
                state.skenario_table.form_preset_id = Some(pid);
                state.status_banner = Some(format!("Master Preset Lokasi '{name}' ditambahkan"));
            }
            ApiEvent::LocationPresetUpdated(preset) => {
                let name = preset.name.clone();
                let pid = preset.id;
                if let Some(existing) = state.location_presets.iter_mut().find(|p| p.id == pid) {
                    *existing = preset.clone();
                } else {
                    state.location_presets.push(preset.clone());
                }
                state.location_presets.sort_by(|a, b| {
                    a.sort_order.cmp(&b.sort_order).then_with(|| a.name.cmp(&b.name))
                });
                state.preset_manager.submitting = false;
                state.preset_manager.load_preset(&preset);
                state.preset_manager.success =
                    Some(format!("Preset lokasi '{name}' berhasil diperbarui!"));
                state.status_banner = Some(format!("Master Preset Lokasi '{name}' diperbarui"));
            }
            ApiEvent::LocationPresetDeleted(id) => {
                state.location_presets.retain(|p| p.id != id);
                if state.skenario_table.form_preset_id == Some(id) {
                    state.skenario_table.form_preset_id = None;
                }
                let next_sort = state
                    .location_presets
                    .iter()
                    .map(|p| p.sort_order)
                    .max()
                    .unwrap_or(50)
                    + 10;
                state.preset_manager.submitting = false;
                state.preset_manager.confirm_delete_id = None;
                if state.preset_manager.editing_id == Some(id) {
                    state.preset_manager.reset_form(next_sort);
                }
                state.preset_manager.success = Some("Preset lokasi berhasil dihapus.".into());
                state.status_banner = Some("Master Preset Lokasi dihapus".into());
            }
            ApiEvent::LocationPresetError(err) => {
                state.preset_manager.submitting = false;
                state.preset_manager.error = Some(err.clone());
                state.status_banner = Some(err);
            }
            ApiEvent::SkenariosLoaded { operasi_id, page } => {
                if state.skenario_table.filter_operasi_id == operasi_id {
                    state.skenarios = page.items;
                    state.skenario_table.total = page.total;
                    state.skenario_table.page = page.page;
                    state.skenario_table.per_page = page.per_page;
                    state.skenario_table.loading = false;
                    state.skenario_table.error = None;
                }
            }
            ApiEvent::SkenarioCreated(sk) => {
                let name = sk.name.clone();
                state.skenario_table.close_modal();
                state.restore_active_skenario_map_state();
                state.skenario_table.page = 1;
                state.status_banner = Some(format!("Skenario '{name}' berhasil ditambahkan"));
                trigger_load_skenarios(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
            }
            ApiEvent::SkenarioUpdated(sk) => {
                let name = sk.name.clone();
                if let Some(existing) = state.skenarios.iter_mut().find(|s| s.id == sk.id) {
                    *existing = sk.clone();
                }
                if state.active_skenario.as_ref().is_some_and(|a| a.id == sk.id) {
                    state.active_skenario = Some(sk);
                }
                state.skenario_dirty = false;
                state.skenario_table.close_modal();
                state.restore_active_skenario_map_state();
                if state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 {
                        state.status_banner =
                            Some("Semua perubahan skenario berhasil disimpan!".into());
                    }
                } else {
                    state.status_banner = Some(format!("Skenario '{name}' disimpan"));
                }
                if state.screen == AppScreen::Skenarios {
                    trigger_load_skenarios(&mut state, &api);
                }
            }
            ApiEvent::SkenarioDeleted(id) => {
                state.skenario_table.close_modal();
                if state.active_skenario.as_ref().is_some_and(|a| a.id == id) {
                    state.active_skenario = None;
                }
                if state.skenarios.len() <= 1 && state.skenario_table.page > 1 {
                    state.skenario_table.page -= 1;
                }
                state.status_banner = Some("Skenario berhasil dihapus".into());
                trigger_load_skenarios(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
            }
            ApiEvent::SkenarioError(err) => {
                state.skenario_table.loading = false;
                state.skenario_table.submitting = false;
                state.skenario_table.pending_open_plot_id = None;
                if matches!(state.skenario_table.modal_mode, SkenarioModalMode::None) {
                    state.skenario_table.error = Some(err.clone());
                } else {
                    state.skenario_table.form_error = Some(err.clone());
                }
                state.status_banner = Some(err);
            }
            ApiEvent::BundleLoaded(bundle) => {
                let sk = bundle.skenario;
                let sk_id = sk.id;
                state.geo_anchor = GeoAnchor::new(sk.center_lng, sk.center_lat);
                state.active_base_layer = sk.base_layer;
                state.active_map_mode = sk.map_mode;
                state.active_skybox = sk.skybox;
                if let Ok(mut tcam) = cam_q.single_mut() {
                    tcam.apply_skenario_view(sk.zoom, sk.pitch, sk.bearing, state.geo_anchor.cos_lat);
                }
                ws.send(WsOutCommand::Subscribe(Some(sk_id)));
                state.status_banner = Some(format!("Editor siap: {}", sk.name));
                state.active_skenario = Some(sk);
                state.markers = bundle.markers;
                state.buildings = bundle.buildings;
                state.lines = bundle.lines;
                state.text_markers = bundle.text_markers;
                state.particles = bundle.particles;
                state.alurs = bundle.alurs;
                state.anims = bundle.anims;
                state.sort_anims_by_order();
                state.selected = SelectedEntity::None;
                state.marker_edit = None;
                state.building_edit = None;
                state.line_edit = None;
                state.text_edit = None;
                state.anim_edit = None;
                state.draft_points_lnglat.clear();
                state.measure_points_lnglat.clear();
                state.hidden_entity_ids.clear();
                state.locked_entity_ids.clear();
                state.pending_delete_entity = None;
                state.picking_marker_for_anim_id = None;
                state.clear_all_dirty();
                playback.stop_play_test();
                playback.play_all_anims = true;
                playback.active_anim_id = state.anims.first().map(|a| a.id);
                playback.loop_anim = state.anims.first().is_none_or(|a| a.replay);

                if state.skenario_table.pending_open_plot_id == Some(sk_id) {
                    state.skenario_table.pending_open_plot_id = None;
                    state.screen = AppScreen::PlotSkenario;
                }
            }
            ApiEvent::MarkerCreated(m) => {
                state.selected = SelectedEntity::Marker(m.id);
                state.marker_edit = Some(m.clone());
                state.building_edit = None;
                state.line_edit = None;
                state.text_edit = None;
                state.anim_edit = None;
                state.markers.push(m);
            }
            ApiEvent::MarkerUpdated(m) => {
                state.dirty_marker_ids.remove(&m.id);
                if let Some(existing) = state.markers.iter_mut().find(|x| x.id == m.id) {
                    *existing = m.clone();
                }
                if state.marker_edit.as_ref().is_some_and(|e| e.id == m.id) {
                    state.marker_edit = Some(m);
                }
                if state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 {
                        state.status_banner =
                            Some("Semua perubahan skenario berhasil disimpan!".into());
                    }
                } else {
                    state.status_banner = Some("Objek 3D disimpan".into());
                }
            }
            ApiEvent::MarkerDeleted(id) => {
                state.dirty_marker_ids.remove(&id);
                state.markers.retain(|m| m.id != id);
                if matches!(state.selected, SelectedEntity::Marker(s) if s == id) {
                    state.selected = SelectedEntity::None;
                    state.marker_edit = None;
                }
            }
            ApiEvent::BuildingCreated(b) => {
                state.selected = SelectedEntity::Building(b.id);
                state.building_edit = Some(b.clone());
                state.marker_edit = None;
                state.line_edit = None;
                state.text_edit = None;
                state.anim_edit = None;
                state.buildings.push(b);
            }
            ApiEvent::BuildingUpdated(b) => {
                state.dirty_building_ids.remove(&b.id);
                if let Some(existing) = state.buildings.iter_mut().find(|x| x.id == b.id) {
                    *existing = b.clone();
                }
                if state.building_edit.as_ref().is_some_and(|e| e.id == b.id) {
                    state.building_edit = Some(b);
                }
                if state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 {
                        state.status_banner =
                            Some("Semua perubahan skenario berhasil disimpan!".into());
                    }
                } else {
                    state.status_banner = Some("Building disimpan".into());
                }
            }
            ApiEvent::BuildingDeleted(id) => {
                state.dirty_building_ids.remove(&id);
                state.buildings.retain(|b| b.id != id);
                if matches!(state.selected, SelectedEntity::Building(s) if s == id) {
                    state.selected = SelectedEntity::None;
                    state.building_edit = None;
                }
            }
            ApiEvent::LineCreated(l) => {
                state.selected = SelectedEntity::Line(l.id);
                state.line_edit = Some(l.clone());
                state.marker_edit = None;
                state.building_edit = None;
                state.text_edit = None;
                state.anim_edit = None;
                state.lines.push(l);
            }
            ApiEvent::LineUpdated(l) => {
                state.dirty_line_ids.remove(&l.id);
                if let Some(existing) = state.lines.iter_mut().find(|x| x.id == l.id) {
                    *existing = l.clone();
                }
                if state.line_edit.as_ref().is_some_and(|e| e.id == l.id) {
                    state.line_edit = Some(l);
                }
                if state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 {
                        state.status_banner =
                            Some("Semua perubahan skenario berhasil disimpan!".into());
                    }
                } else {
                    state.status_banner = Some("Plot line disimpan".into());
                }
            }
            ApiEvent::LineDeleted(id) => {
                state.dirty_line_ids.remove(&id);
                state.lines.retain(|l| l.id != id);
                if matches!(state.selected, SelectedEntity::Line(s) if s == id) {
                    state.selected = SelectedEntity::None;
                    state.line_edit = None;
                }
            }
            ApiEvent::TextMarkerCreated(t) => {
                state.selected = SelectedEntity::TextMarker(t.id);
                state.text_edit = Some(t.clone());
                state.marker_edit = None;
                state.building_edit = None;
                state.line_edit = None;
                state.anim_edit = None;
                state.text_markers.push(t);
            }
            ApiEvent::TextMarkerUpdated(t) => {
                state.dirty_text_ids.remove(&t.id);
                if let Some(existing) = state.text_markers.iter_mut().find(|x| x.id == t.id) {
                    *existing = t.clone();
                }
                if state.text_edit.as_ref().is_some_and(|e| e.id == t.id) {
                    state.text_edit = Some(t);
                }
                if state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 {
                        state.status_banner =
                            Some("Semua perubahan skenario berhasil disimpan!".into());
                    }
                } else {
                    state.status_banner = Some("Label teks disimpan".into());
                }
            }
            ApiEvent::TextMarkerDeleted(id) => {
                state.dirty_text_ids.remove(&id);
                state.text_markers.retain(|t| t.id != id);
                if matches!(state.selected, SelectedEntity::TextMarker(s) if s == id) {
                    state.selected = SelectedEntity::None;
                    state.text_edit = None;
                }
            }
            ApiEvent::AnimCreated(a) => {
                let anim_id = a.id;
                let anim_name = a.name.clone();
                let is_particle = twg_shared::particle_anim::is_particle_anim(&a);
                let is_shoot = twg_shared::shoot_anim::is_shoot_anim(&a);
                let is_multi = twg_shared::route_formation::is_multi_route_anim(&a);
                let needs_marker_pick = !is_particle
                    && !is_shoot
                    && twg_shared::route_formation::anim_bound_marker_ids(&a).is_empty();
                playback.active_anim_id = Some(anim_id);
                state.selected = SelectedEntity::Anim(anim_id);
                state.anim_edit = Some(a.clone());
                state.marker_edit = None;
                state.building_edit = None;
                state.line_edit = None;
                state.text_edit = None;
                state.anims.push(a);
                state.sort_anims_by_order();
                if is_particle {
                    state.particle_preview_reset_nonce =
                        state.particle_preview_reset_nonce.wrapping_add(1);
                    state.status_banner = Some(format!(
                        "Partikel '{anim_name}' diletakkan — geser posisi via Move Gizmo (X / Alt / Z) atau ubah setting di Inspector!"
                    ));
                } else if is_shoot {
                    state.status_banner = Some(format!(
                        "Animasi Water Cannon '{anim_name}' dibuat — live preview aktif di peta! Atur tinggi, jangkauan & tekanan di Inspector."
                    ));
                } else if needs_marker_pick {
                    state.picking_marker_for_anim_id = Some(anim_id);
                    state.tool_mode = ToolMode::PickRouteMarker;
                    state.status_banner = Some(if is_multi {
                        format!(
                            "Garis '{anim_name}' dibuat! Klik beberapa Model 3D / Unit di peta untuk dimasukkan ke formasi (Enter / Esc selesai)."
                        )
                    } else {
                        format!(
                            "Garis '{anim_name}' dibuat! Sekarang klik Model 3D / Unit di peta untuk mengikat rute ini."
                        )
                    });
                } else {
                    state.status_banner =
                        Some("Rute animasi disimpan — tekan Play Skenario untuk test!".into());
                }
            }
            ApiEvent::AnimUpdated(a) => {
                state.dirty_anim_ids.remove(&a.id);
                if let Some(existing) = state.anims.iter_mut().find(|x| x.id == a.id) {
                    *existing = a.clone();
                }
                state.sort_anims_by_order();
                if state.anim_edit.as_ref().is_some_and(|e| e.id == a.id) {
                    state.anim_edit = Some(a);
                }
                if state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 {
                        state.status_banner =
                            Some("Semua perubahan skenario berhasil disimpan!".into());
                    }
                } else {
                    state.status_banner = Some("Animasi rute diperbarui".into());
                }
            }
            ApiEvent::AnimDeleted(id) => {
                state.dirty_anim_ids.remove(&id);
                state.anims.retain(|a| a.id != id);
                if matches!(state.selected, SelectedEntity::Anim(s) if s == id) {
                    state.selected = SelectedEntity::None;
                    state.anim_edit = None;
                }
            }
            ApiEvent::ModelAssetCreated(dto) => {
                let id = dto.id;
                let files: Vec<_> = dto.all_files().into_iter().cloned().collect();
                state.model_assets.retain(|m| m.id != id);
                state.model_assets.push(dto.clone());
                state.model_assets.sort_by(|a, b| {
                    a.category.cmp(&b.category).then_with(|| a.name.cmp(&b.name))
                });
                state.refresh_category_counts();
                state.rebuild_catalog_from_assets();
                state.select_asset(id);
                state.asset_form.success =
                    Some(format!("Model '{}' berhasil ditambahkan & dikonversi (LOD0 + LOD2)!", dto.name));
                state.status_banner = Some(format!("Model '{}' ditambahkan ke server", dto.name));
                state.preview_reload_nonce = state.preview_reload_nonce.wrapping_add(1);
                sync_bridge.send(AssetSyncCommand::PriorityDownload {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                    files,
                });
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::ModelAssetUpdated(dto) => {
                let id = dto.id;
                let files: Vec<_> = dto.all_files().into_iter().cloned().collect();
                if let Some(existing) = state.model_assets.iter_mut().find(|m| m.id == id) {
                    *existing = dto.clone();
                } else {
                    state.model_assets.push(dto.clone());
                }
                state.model_assets.sort_by(|a, b| {
                    a.category.cmp(&b.category).then_with(|| a.name.cmp(&b.name))
                });
                state.refresh_category_counts();
                state.rebuild_catalog_from_assets();
                state.asset_form.load_from_asset(&dto);
                state.asset_form.success = Some(format!(
                    "Model '{}' diperbarui (v{}) — sinkronisasi offline...",
                    dto.name, dto.version
                ));
                state.status_banner = Some(format!("Model '{}' diperbarui", dto.name));
                state.preview_reload_nonce = state.preview_reload_nonce.wrapping_add(1);
                sync_bridge.send(AssetSyncCommand::PriorityDownload {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                    files,
                });
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::ModelAssetDeleted(id) => {
                state.model_assets.retain(|m| m.id != id);
                state.refresh_category_counts();
                state.rebuild_catalog_from_assets();
                state.asset_form.submitting = false;
                state.asset_form.confirm_delete_id = None;
                state.asset_form_mode = AssetFormMode::Inspect;
                if state.selected_asset_id == Some(id) {
                    state.selected_asset_id = state.model_assets.first().map(|m| m.id);
                    if let Some(next) = state.selected_asset().cloned() {
                        state.asset_form.load_from_asset(&next);
                    }
                }
                state.asset_form.success = Some("Model berhasil dihapus dari server.".into());
                state.status_banner = Some("Model dihapus — menyinkronkan cache lokal".into());
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::ModelAssetError(err) => {
                state.asset_form.submitting = false;
                state.asset_form.error = Some(err.clone());
                state.status_banner = Some(err);
            }
            ApiEvent::ModelCategoryCreated(dto) => {
                let entry = twg_shared::models::ModelCategoryCount::from_dto(&dto);
                state.model_categories.retain(|c| c.category != dto.slug);
                state.model_categories.push(entry);
                state.model_categories.sort_by(|a, b| {
                    a.sort_order
                        .cmp(&b.sort_order)
                        .then_with(|| a.category.cmp(&b.category))
                });
                state.refresh_category_counts();
                let next_sort = state
                    .model_categories
                    .iter()
                    .map(|c| c.sort_order)
                    .max()
                    .unwrap_or(70)
                    + 10;
                state.category_form.submitting = false;
                state.category_form.error = None;
                state.category_form.success = Some(format!(
                    "Kategori '{}' ({}) berhasil ditambahkan!",
                    dto.name, dto.slug
                ));
                state.category_form.new_name.clear();
                state.category_form.new_slug.clear();
                state.category_form.slug_manually_edited = false;
                state.category_form.new_supports_idle_walk = false;
                state.category_form.new_default_scale = 3.5;
                state.category_form.new_sort_order = next_sort;
                state.status_banner = Some(format!("Kategori '{}' ditambahkan", dto.slug));
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::ModelCategoryUpdated(dto) => {
                let entry = twg_shared::models::ModelCategoryCount::from_dto(&dto);
                if let Some(existing) = state
                    .model_categories
                    .iter_mut()
                    .find(|c| c.category == dto.slug)
                {
                    *existing = entry;
                } else {
                    state.model_categories.push(entry);
                }
                state.model_categories.sort_by(|a, b| {
                    a.sort_order
                        .cmp(&b.sort_order)
                        .then_with(|| a.category.cmp(&b.category))
                });
                state.refresh_category_counts();
                state.category_form.submitting = false;
                state.category_form.editing_slug = None;
                state.category_form.error = None;
                state.category_form.success = Some(format!(
                    "Kategori '{}' berhasil diperbarui!",
                    dto.slug
                ));
                state.status_banner = Some(format!("Kategori '{}' diperbarui", dto.slug));
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::ModelCategoryDeleted(slug) => {
                state.model_categories.retain(|c| c.category != slug);
                if state.asset_category_filter == slug {
                    state.asset_category_filter = "all".to_string();
                }
                if state.catalog_category == slug {
                    state.catalog_category = "all".to_string();
                }
                state.category_form.submitting = false;
                state.category_form.confirm_delete_slug = None;
                if state.category_form.editing_slug.as_deref() == Some(slug.as_str()) {
                    state.category_form.editing_slug = None;
                }
                state.category_form.error = None;
                state.category_form.success = Some(format!("Kategori '{slug}' berhasil dihapus."));
                state.status_banner = Some(format!("Kategori '{slug}' dihapus"));
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::ModelCategoryError(err) => {
                state.category_form.submitting = false;
                state.category_form.error = Some(err.clone());
                state.status_banner = Some(err);
            }
            ApiEvent::MediaAssetCreated(dto) => {
                let id = dto.id;
                let files: Vec<_> = dto.all_files().into_iter().cloned().collect();
                state.media_assets.retain(|m| m.id != id);
                state.media_assets.push(dto.clone());
                state.media_assets.sort_by(|a, b| {
                    a.media_type
                        .as_str()
                        .cmp(b.media_type.as_str())
                        .then_with(|| a.name.cmp(&b.name))
                });
                state.select_media(id);
                state.media_form.success = Some(format!(
                    "{} '{}' berhasil diunggah ke server!",
                    dto.media_type.label(),
                    dto.name
                ));
                state.status_banner = Some(format!("Media '{}' ditambahkan", dto.name));
                state.media_reload_nonce = state.media_reload_nonce.wrapping_add(1);
                sync_bridge.send(AssetSyncCommand::PriorityDownload {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                    files,
                });
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::MediaAssetUpdated(dto) => {
                let id = dto.id;
                let files: Vec<_> = dto.all_files().into_iter().cloned().collect();
                if let Some(existing) = state.media_assets.iter_mut().find(|m| m.id == id) {
                    *existing = dto.clone();
                } else {
                    state.media_assets.push(dto.clone());
                }
                state.media_assets.sort_by(|a, b| {
                    a.media_type
                        .as_str()
                        .cmp(b.media_type.as_str())
                        .then_with(|| a.name.cmp(&b.name))
                });
                state.media_form.load_from_media(&dto);
                let extra = if let (Some(aw), Some(ah)) = (dto.active_width, dto.active_height) {
                    format!(" ({aw}x{ah})")
                } else {
                    String::new()
                };
                state.media_form.success = Some(format!(
                    "'{}' diperbarui{extra} — v{} (sinkronisasi offline...)",
                    dto.name, dto.version
                ));
                state.status_banner = Some(format!("Media '{}' diperbarui", dto.name));
                state.media_reload_nonce = state.media_reload_nonce.wrapping_add(1);
                sync_bridge.send(AssetSyncCommand::PriorityDownload {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                    files,
                });
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::MediaAssetDeleted(id) => {
                state.media_assets.retain(|m| m.id != id);
                state.media_form.submitting = false;
                state.media_form.confirm_delete_id = None;
                state.media_form_mode = MediaFormMode::Inspect;
                state.ensure_media_selection_for_section();
                state.media_form.success = Some("Media asset berhasil dihapus dari server.".into());
                state.status_banner = Some("Media dihapus — menyinkronkan cache lokal".into());
                state.media_reload_nonce = state.media_reload_nonce.wrapping_add(1);
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });
            }
            ApiEvent::MediaAssetError(err) => {
                state.media_form.submitting = false;
                state.media_form.error = Some(err.clone());
                state.status_banner = Some(err);
            }
            ApiEvent::Error(err) => {
                state.skenario_table.pending_open_plot_id = None;
                state.status_banner = Some(err);
            }
        }
    }

    for evt in ws.drain_events() {
        match evt {
            WsInEvent::Connected => {
                state.ws_connected = true;
                state.push_log("system", "ws_connected", "WebSocket stream active");
            }
            WsInEvent::Disconnected(reason) => {
                state.ws_connected = false;
                state.online_operators.clear();
                state.push_log("system", "ws_disconnected", reason);
            }
            WsInEvent::OnlineUsers(ops) => {
                state.online_operators = ops;
            }
            WsInEvent::Error(msg) => {
                state.push_log("ws", "error", msg);
            }
            WsInEvent::Broadcast(msg) => {
                state.push_ws_broadcast(&msg);
                let active_sk_id = state.active_skenario.as_ref().map(|s| s.id);
                if active_sk_id != Some(msg.skenario_id) {
                    continue;
                }

                match msg.command.as_str() {
                    "move_marker" => {
                        if let Some(mid) = msg.marker_id
                            && let Some(m) = state.markers.iter_mut().find(|x| x.id == mid)
                        {
                            if let Some(lng) = msg.payload.get("lng").and_then(|v| v.as_f64()) {
                                m.lng = lng;
                            }
                            if let Some(lat) = msg.payload.get("lat").and_then(|v| v.as_f64()) {
                                m.lat = lat;
                            }
                            if let Some(alt) = msg.payload.get("altitude").and_then(|v| v.as_f64()) {
                                m.altitude = alt;
                            }
                            if let Some(ry) = msg.payload.get("rot_y").and_then(|v| v.as_f64()) {
                                m.rot_y = ry;
                            }
                        }
                    }
                    "play_anim" => {
                        let target_aid = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok());
                        let play_all = msg
                            .payload
                            .get("play_all")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if let Some(aid) = target_aid {
                            if playback.track_status(aid) == state::AnimRunStatus::Paused {
                                playback.resume_anim(aid);
                            } else {
                                playback.play_anim(aid);
                            }
                        } else if play_all {
                            playback.play_all_routes(&state.anims);
                        } else {
                            playback.enter_play_mode();
                        }
                    }
                    "pause_anim" => {
                        if let Some(aid) = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                        {
                            playback.pause_anim(aid);
                        } else {
                            let ids: Vec<uuid::Uuid> = playback.tracks.keys().copied().collect();
                            for aid in ids {
                                playback.pause_anim(aid);
                            }
                        }
                    }
                    "stop_anim" => {
                        if let Some(aid) = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                        {
                            playback.stop_anim(aid);
                        } else {
                            playback.stop_all_routes(&state.anims);
                        }
                    }
                    "camera_sync" => {
                        if let Ok(mut tcam) = cam_q.single_mut() {
                            if let (Some(lng), Some(lat)) = (
                                msg.payload.get("lng").and_then(|v| v.as_f64()),
                                msg.payload.get("lat").and_then(|v| v.as_f64()),
                            ) {
                                let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
                                tcam.target_local = Vec2::new(local[0], local[2]);
                            }
                            if let Some(pitch) = msg.payload.get("pitch").and_then(|v| v.as_f64()) {
                                tcam.pitch_deg = (pitch as f32).clamp(5.0, 82.0);
                            }
                            if let Some(bearing) = msg.payload.get("bearing").and_then(|v| v.as_f64()) {
                                tcam.bearing_deg = bearing as f32;
                            }
                            if let Some(zoom) = msg.payload.get("zoom").and_then(|v| v.as_f64()) {
                                tcam.distance_m = twg_shared::geo::camera_distance_for_zoom(
                                    zoom,
                                    820.0,
                                    45.0_f32.to_radians(),
                                    state.geo_anchor.cos_lat,
                                );
                            }
                        }
                    }
                    "reload" => {
                        if let Some(auth) = current_auth(&state) {
                            api.send(ApiCommand::LoadBundle {
                                auth,
                                skenario_id: msg.skenario_id,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
