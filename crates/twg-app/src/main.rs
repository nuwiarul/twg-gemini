#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod map;
mod net;
mod scene;
mod settings;
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
        play_rts::{PlayRtsState, play_rts_interaction_system, play_rts_movement_system},
        playback::{PlaybackOverrides, playback_system},
        pointer_arrow::{PointerArrowVisualPool, pointer_arrow_visual_system},
        polygon_edit::PolygonEditState,
        plot_line_edit::PlotLineEditState,
        route_edit::RouteEditState,
        scenario_audio::{ScenarioAudio, scenario_audio_system},
        shoot_fx::{ShootFxVisualPool, shoot_fx_visual_system},
        skybox::{FillSkyLight, PrimarySunLight, SkyboxManager, sync_skybox_system},
        traffic::{TrafficVisualPool, traffic_visual_system},
        water_cannon::{WaterCannonVisualPool, water_cannon_visual_system},
    },
    state::{
        AppScreen, AppState, AssetFormMode, MediaFormMode, OperasiModalMode, PlaySessionMode,
        PlaybackState, SatkerModalMode, SelectedEntity, SkenarioModalMode, ToolMode,
        UiPointerState, UserModalMode,
    },
    ui::{
        app_icon::set_winit_window_icon_system,
        egui_root_system,
        nav_bar::{
            trigger_load_operasi_options, trigger_load_operasis, trigger_load_satkers,
            trigger_load_skenarios, trigger_load_users,
        },
    },
};
use twg_shared::ws::{TacticalCommand, WsClientMode};

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
        .init_resource::<PlayRtsState>()
        .init_resource::<TransformGizmoState>()
        .init_resource::<PolygonEditState>()
        .init_resource::<PlotLineEditState>()
        .init_resource::<RouteEditState>()
        .init_resource::<WaterCannonVisualPool>()
        .init_resource::<ShootFxVisualPool>()
        .init_resource::<ParticleFxPool>()
        .init_resource::<PointerArrowVisualPool>()
        .init_resource::<TrafficVisualPool>()
        .init_resource::<SkyboxManager>()
        .init_resource::<MarkerWorldBounds>()
        .init_resource::<BasicMeshTextureCache>()
        .init_resource::<AssetPreviewStage>()
        .init_resource::<MediaPreviewPlayer>()
        .init_resource::<ScenarioAudio>()
        .insert_resource(ApiClientBridge::start())
        .insert_resource(WsClientBridge::start())
        .insert_resource(sync_bridge)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    close_when_requested: false,
                    primary_window: Some(Window {
                        title: "Tactical Wall Game 3D — Full Rust".into(),
                        name: Some("twg-app".into()),
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
                set_winit_window_icon_system,
                (
                    process_asset_sync_events_system,
                    process_net_events_system,
                    map_interaction_system,
                    play_rts_interaction_system,
                    camera_control_system,
                    asset_preview_camera_system,
                    asset_preview_stage_system,
                    media_preview_audio_system,
                    tile_streaming_system,
                    indoor_grid_system,
                    sync_skybox_system,
                )
                    .chain(),
                (
                    playback_system,
                    scenario_audio_system,
                    play_rts_movement_system,
                    sync_markers_system,
                    animate_markers_system,
                    selection_gizmo_system,
                    sync_buildings_system,
                    draw_building_draft_gizmos,
                    draw_lines_and_routes_system,
                    water_cannon_visual_system,
                    shoot_fx_visual_system,
                    particle_fx_visual_system,
                    pointer_arrow_visual_system,
                    traffic_visual_system,
                )
                    .chain(),
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            normalize_skinned_mesh_weights_system.before(bevy::asset::AssetEventSystems),
        )
        .add_systems(Update, ui::handle_close_requests)
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
    mut play_rts: ResMut<PlayRtsState>,
    overrides: Res<PlaybackOverrides>,
    api: Res<ApiClientBridge>,
    ws: Res<WsClientBridge>,
    sync_bridge: Res<AssetSyncBridge>,
    mut cam_q: Query<&mut TacticalCamera>,
) {
    for (plot_save, evt) in api.drain_events() {
        match evt {
            ApiEvent::ServerTestResult { ok, message } => {
                state.testing_server_connection = false;
                state.server_settings_feedback = Some((ok, message));
            }
            ApiEvent::LoginOk(login) => {
                let role = login.user.role;
                let username = login.user.username.clone();
                state.loading = false;
                state.login_error = None;
                state.offline_preview = false;
                state.auth_token = Some(login.access_token.clone());
                state.refresh_token = Some(login.refresh_token);
                state.profile_form.load_from_user(&login.user);
                state.current_user = Some(login.user);
                state.operasi_table.page = 1;
                state.operasi_table.search.clear();
                state.operasi_table.applied_search.clear();

                // Re-sync 3D models with the authenticated server URL
                sync_bridge.send(AssetSyncCommand::SyncWithServer {
                    server_url: state.server_url.clone(),
                    local_root: state.assets_root.clone(),
                });

                if role.is_big_screen_only_role() {
                    // Dedicated Layar Besar accounts jump straight into fullscreen read-only Layar Besar!
                    state.play_session_mode = PlaySessionMode::BigScreen;
                    state.active_skenario = None;
                    state.screen = AppScreen::LayarBesar;
                    state.status_banner = Some(format!(
                        "Login berhasil sebagai {username} [{}] — Mode Layar Besar",
                        role.label()
                    ));
                    ws.send(WsOutCommand::Connect {
                        server_url: state.server_url.clone(),
                        token: login.access_token,
                        device_id: state.device_id.clone(),
                        skenario_id: None,
                        client_mode: WsClientMode::BigScreen,
                    });
                } else {
                    state.play_session_mode = PlaySessionMode::LocalPreview;
                    state.screen = AppScreen::Operasis;
                    state.status_banner = Some(format!(
                        "Login berhasil sebagai {username} [{}] — Daftar Operasi",
                        role.label()
                    ));

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
                        skenario_id: None,
                        client_mode: WsClientMode::Idle,
                    });
                }
            }
            ApiEvent::LoginErr(err) => {
                state.loading = false;
                state.login_error = Some(err);
            }
            ApiEvent::ProfileUpdated(updated_user) => {
                state.profile_form.profile_submitting = false;
                state.profile_form.profile_error = None;
                state.profile_form.name = updated_user.name.clone();
                state.profile_form.profile_success = Some("Profil berhasil diperbarui!".into());
                state.current_user = Some(updated_user);
                state.status_banner = Some("Profil akun berhasil diperbarui".into());
            }
            ApiEvent::ProfileError(err) => {
                state.profile_form.profile_submitting = false;
                state.profile_form.profile_success = None;
                state.profile_form.profile_error = Some(err.clone());
                state.status_banner = Some(err);
            }
            ApiEvent::PasswordChanged => {
                state.profile_form.password_submitting = false;
                state.profile_form.password_error = None;
                state.profile_form.old_password.clear();
                state.profile_form.new_password.clear();
                state.profile_form.confirm_password.clear();
                state.profile_form.show_old_password = false;
                state.profile_form.show_new_password = false;
                state.profile_form.show_confirm_password = false;
                state.profile_form.password_success =
                    Some("Password berhasil diubah! Gunakan password baru pada login berikutnya.".into());
                state.status_banner = Some("Password akun berhasil diubah".into());
            }
            ApiEvent::PasswordError(err) => {
                state.profile_form.password_submitting = false;
                state.profile_form.password_success = None;
                state.profile_form.password_error = Some(err.clone());
                state.status_banner = Some(err);
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
                state.operasi_table.satkers = satkers.clone();
                state.satker_table.satkers = satkers;
                state.satker_table.loading = false;
                state.satker_table.error = None;
            }
            ApiEvent::SatkerCreated(s) => {
                let name = s.name.clone();
                state.satker_table.close_modal();
                state.status_banner = Some(format!("Satker '{name}' berhasil ditambahkan"));
                trigger_load_satkers(&mut state, &api);
            }
            ApiEvent::SatkerUpdated(s) => {
                let name = s.name.clone();
                state.satker_table.close_modal();
                state.status_banner = Some(format!("Satker '{name}' berhasil diperbarui"));
                trigger_load_satkers(&mut state, &api);
            }
            ApiEvent::SatkerDeleted(id) => {
                state.satker_table.satkers.retain(|s| s.id != id);
                state.operasi_table.satkers.retain(|s| s.id != id);
                state.satker_table.close_modal();
                state.status_banner = Some("Satker berhasil dihapus".into());
                trigger_load_satkers(&mut state, &api);
            }
            ApiEvent::SatkerError(err) => {
                state.satker_table.loading = false;
                state.satker_table.submitting = false;
                if matches!(state.satker_table.modal_mode, SatkerModalMode::None) {
                    state.satker_table.error = Some(err.clone());
                } else {
                    state.satker_table.form_error = Some(err.clone());
                }
                state.status_banner = Some(err);
            }
            ApiEvent::UsersLoaded(users) => {
                state.user_table.users = users;
                state.user_table.loading = false;
                state.user_table.error = None;
            }
            ApiEvent::UserCreated(u) => {
                let uname = u.username.clone();
                state.user_table.close_modal();
                state.status_banner = Some(format!("User '@{uname}' berhasil ditambahkan"));
                trigger_load_users(&mut state, &api);
            }
            ApiEvent::UserUpdated(u) => {
                let uname = u.username.clone();
                state.user_table.close_modal();
                state.status_banner = Some(format!("User '@{uname}' berhasil diperbarui"));
                trigger_load_users(&mut state, &api);
            }
            ApiEvent::UserDeleted(id) => {
                state.user_table.users.retain(|u| u.id != id);
                state.user_table.close_modal();
                state.status_banner = Some("User berhasil dihapus".into());
                trigger_load_users(&mut state, &api);
            }
            ApiEvent::UserError(err) => {
                state.user_table.loading = false;
                state.user_table.submitting = false;
                if matches!(state.user_table.modal_mode, UserModalMode::None) {
                    state.user_table.error = Some(err.clone());
                } else {
                    state.user_table.form_error = Some(err.clone());
                }
                state.status_banner = Some(err);
            }
            ApiEvent::TacticalOperasisLoaded(page) => {
                state.tactical_page.cards = page.items;
                state.tactical_page.total = page.total;
                state.tactical_page.page = page.page;
                state.tactical_page.per_page = page.per_page;
                state.tactical_page.loading = false;
                state.tactical_page.error = None;
            }
            ApiEvent::TacticalOperasiSkenariosLoaded { operasi_id, items } => {
                if state
                    .active_skenario
                    .as_ref()
                    .is_some_and(|s| s.operasi_id == operasi_id)
                {
                    state.tactical_page.active_operasi_skenarios = items;
                }
            }
            ApiEvent::TacticalError(err) => {
                state.tactical_page.loading = false;
                state.tactical_page.error = Some(err.clone());
                state.status_banner = Some(err);
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 && state.unsaved_change_count() == 0 {
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
            ApiEvent::SkenarioCopied(sk) => {
                let name = sk.name.clone();
                let op_name = sk
                    .operasi_name
                    .clone()
                    .unwrap_or_else(|| "operasi tujuan".to_string());
                state.skenario_table.close_modal();
                state.skenario_table.page = 1;
                state.status_banner = Some(format!(
                    "Skenario '{name}' berhasil disalin ke '{op_name}' beserta seluruh isinya"
                ));
                trigger_load_skenarios(&mut state, &api);
                trigger_load_operasis(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
            }
            ApiEvent::SkenarioMoved(sk) => {
                let name = sk.name.clone();
                let op_name = sk
                    .operasi_name
                    .clone()
                    .unwrap_or_else(|| "operasi tujuan".to_string());
                if state.active_skenario.as_ref().is_some_and(|a| a.id == sk.id) {
                    state.active_skenario = Some(sk.clone());
                }
                state.skenario_table.close_modal();
                if state.skenarios.len() <= 1 && state.skenario_table.page > 1 {
                    state.skenario_table.page -= 1;
                }
                state.status_banner = Some(format!(
                    "Skenario '{name}' berhasil dipindahkan ke '{op_name}'"
                ));
                trigger_load_skenarios(&mut state, &api);
                trigger_load_operasis(&mut state, &api);
                trigger_load_operasi_options(&state, &api);
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    state.pending_plot_exit = None;
                }
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
                state.reload_plot_on_return = false;
                let sk = bundle.skenario;
                let sk_id = sk.id;
                let sk_op_id = sk.operasi_id;
                let sk_name = sk.name.clone();
                state.geo_anchor = GeoAnchor::new(sk.center_lng, sk.center_lat);
                state.active_base_layer = sk.base_layer;
                state.active_map_mode = sk.map_mode;
                state.active_skybox = sk.skybox;
                if let Ok(mut tcam) = cam_q.single_mut() {
                    tcam.apply_skenario_view(sk.zoom, sk.pitch, sk.bearing, state.geo_anchor.cos_lat);
                }
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
                state.picking_shoot_for_anim_id = None;
                state.picking_pointer_for_anim_id = None;
                state.picking_spawn_hide_for_anim_id = None;
                state.spawn_hide_preview = None;
                state.clear_all_dirty();
                play_rts.reset_all();
                playback.stop_play_test();
                playback.play_all_anims = true;
                playback.active_anim_id = state.anims.first().map(|a| a.id);
                playback.loop_anim = state.anims.first().is_none_or(|a| a.replay);

                if state.screen == AppScreen::LayarBesar {
                    // Layar Besar remote scenario load/reload
                    state.play_session_mode = PlaySessionMode::BigScreen;
                    state.big_screen.pending_skenario_id = None;
                    playback.enter_play_mode();
                    if let Some(cam_payload) = state.big_screen.pending_camera_payload.take()
                        && let Ok(mut tcam) = cam_q.single_mut()
                    {
                        apply_remote_camera_payload(&state.geo_anchor, &mut tcam, &cam_payload);
                    }
                    ws.send(WsOutCommand::Subscribe {
                        skenario_id: Some(sk_id),
                        client_mode: WsClientMode::BigScreen,
                    });
                    state.status_banner = Some(format!("Layar Besar aktif: {sk_name}"));
                } else if state.tactical_page.pending_open_tactical_id == Some(sk_id) {
                    // Play Skenario launched from Tactical / Simulasi (Realtime Live Mode!)
                    state.tactical_page.pending_open_tactical_id = None;
                    state.screen = AppScreen::PlotSkenario;
                    state.play_session_mode = PlaySessionMode::TacticalLive;
                    playback.enter_play_mode();
                    ws.send(WsOutCommand::Subscribe {
                        skenario_id: Some(sk_id),
                        client_mode: WsClientMode::TacticalLive,
                    });
                    if let Some(bcast_cmd) = state.tactical_page.pending_broadcast_big_screen.take()
                        && state.can_control_active_big_screen()
                    {
                        ws.send(WsOutCommand::SendCommand(TacticalCommand {
                            operasi_id: sk_op_id,
                            skenario_id: sk_id,
                            marker_id: None,
                            command: bcast_cmd.to_string(),
                            payload: serde_json::json!({ "skenario_name": sk_name }),
                            save_history: false,
                        }));
                    }
                    state.status_banner = Some(format!("Tactical Live siap: {sk_name}"));
                } else {
                    // Opened into Plot Editor (isolated local mode — no live WS subscribe)
                    state.play_session_mode = PlaySessionMode::LocalPreview;
                    ws.send(WsOutCommand::Subscribe {
                        skenario_id: None,
                        client_mode: WsClientMode::Idle,
                    });
                    state.status_banner = Some(format!("Editor siap: {sk_name}"));
                    if state.skenario_table.pending_open_plot_id == Some(sk_id) {
                        state.skenario_table.pending_open_plot_id = None;
                        state.screen = AppScreen::PlotSkenario;
                    }
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 && state.unsaved_change_count() == 0 {
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 && state.unsaved_change_count() == 0 {
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 && state.unsaved_change_count() == 0 {
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 && state.unsaved_change_count() == 0 {
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
                let is_audio = twg_shared::audio_anim::is_audio_anim(&a);
                let is_particle = twg_shared::particle_anim::is_particle_anim(&a);
                let is_traffic = twg_shared::traffic_anim::is_traffic_anim(&a);
                let is_traffic_queue = twg_shared::traffic_queue::is_traffic_queue_anim(&a);
                let is_traffic_gridlock = twg_shared::traffic_queue::is_traffic_gridlock_anim(&a);
                let is_traffic_manual = twg_shared::traffic_queue::is_traffic_manual_anim(&a);
                let is_traffic_diversion =
                    twg_shared::traffic_diversion::is_traffic_diversion_anim(&a);
                let is_shoot = twg_shared::shoot_anim::is_shoot_anim(&a);
                let is_pointer = twg_shared::pointer_anim::is_pointer_arrow_anim(&a);
                let spawn_hide_act = twg_shared::spawn_hide_anim::spawn_hide_action(&a);
                let is_spawn_hide = spawn_hide_act.is_some();
                let group_mode_opt = twg_shared::group_anim::group_anim_mode(&a);
                let is_group = group_mode_opt.is_some();
                let is_multi = twg_shared::route_formation::is_multi_route_anim(&a);
                let bound_empty = twg_shared::route_formation::anim_bound_marker_ids(&a).is_empty();
                let needs_marker_pick = !is_particle
                    && !twg_shared::audio_anim::is_audio_anim(&a)
                    && !is_traffic
                    && !is_traffic_queue
                    && !is_traffic_diversion
                    && !is_shoot
                    && !is_pointer
                    && !is_spawn_hide
                    && !is_group
                    && bound_empty;
                playback.active_anim_id = Some(anim_id);
                state.selected = SelectedEntity::Anim(anim_id);
                state.anim_edit = Some(a.clone());
                state.marker_edit = None;
                state.building_edit = None;
                state.line_edit = None;
                state.text_edit = None;
                state.anims.push(a);
                state.sort_anims_by_order();
                if is_audio {
                    state.status_banner = Some(format!("Play Suara '{anim_name}' dibuat — pilih Sound dari Assets di Inspector."));
                } else if is_particle {
                    state.particle_preview_reset_nonce =
                        state.particle_preview_reset_nonce.wrapping_add(1);
                    state.status_banner = Some(format!(
                        "Partikel '{anim_name}' diletakkan — geser posisi via Move Gizmo (X / Alt / Z) atau ubah setting di Inspector!"
                    ));
                } else if is_traffic {
                    state.status_banner = Some(format!(
                        "Simulasi Traffic '{anim_name}' dibuat — tambah jalur balik / atur pool kendaraan di Inspector lalu klik Preview!"
                    ));
                } else if is_traffic_queue {
                    state.status_banner = Some(if is_traffic_gridlock {
                        format!(
                            "Macet Total & Urai '{anim_name}' dibuat — sekarang gambar Jalur Lancar untuk Jalur A di Inspector!"
                        )
                    } else if is_traffic_manual {
                        format!(
                            "Kontrol Simpang Manual '{anim_name}' dibuat — sekarang gambar Jalur Lancar untuk Jalur A di Inspector!"
                        )
                    } else {
                        format!(
                            "Antrean & Urai Simpang '{anim_name}' dibuat — sekarang gambar Jalur Lancar untuk Jalur A di Inspector!"
                        )
                    });
                } else if is_traffic_diversion {
                    state.status_banner = Some(format!(
                        "Pengalihan Arus '{anim_name}' dibuat — sekarang gambar Jalur Utama & Jalur Alternatif di Inspector!"
                    ));
                } else if is_shoot {
                    let kind_label = twg_shared::shoot_anim::shoot_anim_kind(
                        state.anims.iter().find(|x| x.id == anim_id).unwrap(),
                    )
                    .map(|k| k.label())
                    .unwrap_or("Shoot");
                    state.shoot_preview_reset_nonce =
                        state.shoot_preview_reset_nonce.wrapping_add(1);
                    state.status_banner = Some(format!(
                        "Animasi {kind_label} '{anim_name}' dibuat — live preview aktif di peta! Atur parameter di Inspector."
                    ));
                } else if is_pointer {
                    state.status_banner = Some(format!(
                        "Panah Penunjuk 3D '{anim_name}' dibuat — live preview aktif di peta! Atur ukuran, warna & teks di Inspector."
                    ));
                } else if let Some(act) = spawn_hide_act {
                    if bound_empty {
                        state.picking_spawn_hide_for_anim_id = Some(anim_id);
                        state.tool_mode = ToolMode::PickSpawnHideMarker;
                        state.status_banner = Some(format!(
                            "Animasi {} '{anim_name}' dibuat! Klik 1 atau beberapa Marker / Objek 3D di peta untuk memilih target (Enter / Esc selesai).",
                            act.short_tag()
                        ));
                    } else {
                        state.status_banner = Some(format!(
                            "Animasi {} '{anim_name}' disimpan — atur transisi di Inspector atau tekan Play Skenario!",
                            act.short_tag()
                        ));
                    }
                } else if let Some(gm) = group_mode_opt {
                    state.status_banner = Some(format!(
                        "Grup {} '{anim_name}' dibuat — tambahkan animasi anggota di Inspector!",
                        gm.short_tag()
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    if state.saving_pending_count == 0 && state.unsaved_change_count() == 0 {
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
                if plot_save && state.saving_pending_count > 0 {
                    state.saving_pending_count -= 1;
                    state.pending_plot_exit = None;
                }
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
            WsInEvent::BigScreenState(live) => {
                if !state.is_big_screen() {
                    continue;
                }
                if state.big_screen.skip_next_cached_state {
                    state.big_screen.skip_next_cached_state = false;
                    continue;
                }
                state.big_screen.loaded_by_actor = Some(live.actor_username.clone());
                let cur_id = state.active_skenario.as_ref().map(|s| s.id);
                if cur_id != Some(live.skenario_id) {
                    state.big_screen.pending_skenario_id = Some(live.skenario_id);
                    state.big_screen.pending_camera_payload = live.camera;
                    if let Some(auth) = current_auth(&state) {
                        api.send(ApiCommand::LoadBundle {
                            auth,
                            skenario_id: live.skenario_id,
                        });
                    }
                } else if let Some(cam_payload) = live.camera
                    && let Ok(mut tcam) = cam_q.single_mut()
                {
                    apply_remote_camera_payload(&state.geo_anchor, &mut tcam, &cam_payload);
                }
            }
            WsInEvent::Broadcast(msg) => {
                state.push_ws_broadcast(&msg);

                // Anti-echo: ignore commands sent by this exact device
                if !msg.actor_device_id.is_empty() && msg.actor_device_id == state.device_id {
                    continue;
                }

                // Strict session isolation: Plot Editor (`LocalPreview`) and CRUD screens NEVER
                // react to live Tactical / Layar Besar broadcasts!
                let is_big = state.is_big_screen();
                let is_live = state.is_tactical_live();
                if !is_big && !is_live {
                    continue;
                }

                let active_sk_id = state.active_skenario.as_ref().map(|s| s.id);

                // 1. Remote Scenario Load / Reload commands (can arrive even before scenario matches on Layar Besar)
                match msg.command.as_str() {
                    "load_skenario" => {
                        if is_big
                            && let Some(auth) = current_auth(&state)
                        {
                            state.big_screen.skip_next_cached_state = false;
                            state.big_screen.loaded_by_actor = Some(msg.actor_username.clone());
                            state.big_screen.pending_skenario_id = Some(msg.skenario_id);
                            state.big_screen.pending_camera_payload = None;
                            api.send(ApiCommand::LoadBundle {
                                auth,
                                skenario_id: msg.skenario_id,
                            });
                        }
                        continue;
                    }
                    "reload_skenario" | "reload" => {
                        if (is_big || (is_live && active_sk_id == Some(msg.skenario_id)))
                            && let Some(auth) = current_auth(&state)
                        {
                            play_rts.reset_all();
                            if is_big {
                                state.big_screen.skip_next_cached_state = false;
                                state.big_screen.loaded_by_actor =
                                    Some(msg.actor_username.clone());
                                state.big_screen.pending_skenario_id = Some(msg.skenario_id);
                                state.big_screen.pending_camera_payload = None;
                            } else if is_live {
                                state.tactical_page.pending_open_tactical_id =
                                    Some(msg.skenario_id);
                                state.tactical_page.pending_broadcast_big_screen = None;
                            }
                            api.send(ApiCommand::LoadBundle {
                                auth,
                                skenario_id: msg.skenario_id,
                            });
                        }
                        continue;
                    }
                    _ => {}
                }

                // 2. All other live commands apply ONLY to the active scenario
                if active_sk_id != Some(msg.skenario_id) {
                    continue;
                }

                match msg.command.as_str() {
                    "camera_sync" => {
                        // Camera movement from Admin / Admin Satker ONLY affects Layar Besar
                        // (other Tactical operators keep independent camera control).
                        if is_big
                            && let Ok(mut tcam) = cam_q.single_mut()
                        {
                            apply_remote_camera_payload(&state.geo_anchor, &mut tcam, &msg.payload);
                        }
                    }
                    "play_anim" => {
                        if let Some(spd) = msg
                            .payload
                            .get("speed_multiplier")
                            .and_then(|v| v.as_f64())
                        {
                            playback.speed_multiplier = (spd as f32).clamp(0.25, 4.0);
                        }
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
                                playback.resume_anim_tree(aid, &state.anims);
                            } else {
                                playback.play_anim(aid);
                            }
                        } else if play_all {
                            playback.play_all_routes(&state.anims);
                        }
                    }
                    "play_all_anim" => {
                        if let Some(spd) = msg
                            .payload
                            .get("speed_multiplier")
                            .and_then(|v| v.as_f64())
                        {
                            playback.speed_multiplier = (spd as f32).clamp(0.25, 4.0);
                        }
                        playback.play_all_routes(&state.anims);
                    }
                    "pause_anim" => {
                        if let Some(aid) = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                        {
                            playback.pause_anim_tree(aid, &state.anims);
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
                            playback.stop_anim_tree(aid, &state.anims);
                        } else {
                            playback.stop_all_routes(&state.anims);
                        }
                    }
                    "stop_all_anim" => {
                        playback.stop_all_routes(&state.anims);
                    }
                    "traffic_queue_clear" => {
                        if let Some(aid) = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                        {
                            let trig = msg
                                .payload
                                .get("trigger_elapsed_sec")
                                .and_then(|v| v.as_f64());
                            playback.trigger_queue_clearance(aid, trig);
                        }
                    }
                    "traffic_queue_manual" => {
                        if let Some(aid) = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                            && let Some(act_val) = msg.payload.get("manual_action")
                            && let Ok(act) = serde_json::from_value::<
                                twg_shared::traffic_queue::QueueManualAction,
                            >(act_val.clone())
                        {
                            let trig = msg
                                .payload
                                .get("trigger_elapsed_sec")
                                .and_then(|v| v.as_f64());
                            playback.push_queue_manual_action(aid, act, trig);
                        }
                    }
                    "traffic_diversion_toggle" => {
                        if let Some(aid) = msg
                            .payload
                            .get("anim_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                            && let Some(is_div) =
                                msg.payload.get("is_diverted").and_then(|v| v.as_bool())
                        {
                            let trig = msg
                                .payload
                                .get("trigger_elapsed_sec")
                                .and_then(|v| v.as_f64());
                            playback.push_diversion_toggle(aid, is_div, trig);
                        }
                    }
                    "set_speed" => {
                        if let Some(spd) = msg
                            .payload
                            .get("speed_multiplier")
                            .and_then(|v| v.as_f64())
                        {
                            playback.speed_multiplier = (spd as f32).clamp(0.25, 4.0);
                        }
                    }
                    "rts_move" => {
                        play_rts.apply_remote_rts_move(&state, &overrides, &msg.payload);
                    }
                    "gizmo_move" => {
                        if let Some(mid) = msg.marker_id {
                            play_rts.apply_remote_gizmo_move(mid, &msg.payload);
                        }
                    }
                    "reset_positions" => {
                        play_rts.reset_positions();
                    }
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
                    _ => {}
                }
            }
        }
    }
}

fn apply_remote_camera_payload(
    geo_anchor: &GeoAnchor,
    tcam: &mut TacticalCamera,
    payload: &serde_json::Value,
) {
    if let (Some(lng), Some(lat)) = (
        payload.get("lng").and_then(|v| v.as_f64()),
        payload.get("lat").and_then(|v| v.as_f64()),
    ) {
        let local = geo_anchor.lng_lat_alt_to_local(lng, lat, 0.0);
        tcam.target_local = Vec2::new(local[0], local[2]);
    }
    if let Some(pitch) = payload.get("pitch").and_then(|v| v.as_f64()) {
        tcam.pitch_deg = (pitch as f32).clamp(5.0, 82.0);
    }
    if let Some(bearing) = payload.get("bearing").and_then(|v| v.as_f64()) {
        tcam.bearing_deg = (bearing as f32).rem_euclid(360.0);
    }
    if let Some(zoom) = payload.get("zoom").and_then(|v| v.as_f64()) {
        tcam.distance_m = twg_shared::geo::camera_distance_for_zoom(
            zoom,
            820.0,
            45.0_f32.to_radians(),
            geo_anchor.cos_lat,
        );
    }
}
