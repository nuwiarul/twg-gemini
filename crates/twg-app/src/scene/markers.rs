use std::collections::{HashMap, HashSet};

use bevy::{
    camera::primitives::Aabb,
    ecs::system::SystemParam,
    gltf::Gltf,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    window::PrimaryWindow,
};
use twg_shared::{
    api::{
        CreateAnimRequest, CreateBuildingRequest, CreateMarkerRequest, CreatePlotLineRequest,
        CreateTextMarkerRequest, UpdateAnimRequest, UpdateBuildingRequest, UpdateMarkerRequest,
        UpdatePlotLineRequest, UpdateSkenarioRequest, UpdateTextMarkerRequest,
    },
    models::{Anim, Marker, ModelCatalogEntry, TextMarker},
    ws::TacticalCommand,
};
use uuid::Uuid;

use crate::{
    map::camera::cursor_world_ground,
    net::{
        client::{ApiClientBridge, ApiCommand, AuthCredentials},
        ws_client::{WsClientBridge, WsOutCommand},
    },
    scene::{
        basic_mesh::{
            BASIC_MESH_CATEGORY, BasicMeshConfig, BasicMeshShape, BasicMeshTextureCache,
            build_basic_mesh_geometry, build_basic_mesh_material, parse_basic_mesh_config,
            write_basic_mesh_config,
        },
        buildings::TacticalBuildingEntity,
        gizmo::{
            GIZMO_SCREEN_RADIUS_PX, GizmoHandle, GizmoMode, TransformGizmoState,
            begin_gizmo_drag, compute_gizmo_world_radius, gizmo_mode_for_tool, hit_test_gizmo,
            marker_quat, marker_world_origin, update_gizmo_drag,
        },
        playback::PlaybackOverrides,
        polygon_edit::{
            PolygonEditState, PolygonHoverTarget, PolygonVertexGizmoHandle, begin_vertex_drag,
            ensure_editable_polygon_points, hit_test_polygon_editor, insert_midpoint_after,
            insert_vertex_from_cursor_click, update_vertex_drag,
        },
    },
    state::{AppScreen, AppState, PlaybackState, SelectedEntity, ToolMode, UiPointerState},
};

#[derive(Debug, Clone, Copy)]
pub struct MarkerBoundsInfo {
    pub world_min: Vec3,
    pub world_max: Vec3,
    pub footprint_center: Vec3,
    pub footprint_radius: f32,
}

#[derive(Resource, Default)]
pub struct MarkerWorldBounds {
    pub bounds: HashMap<Uuid, MarkerBoundsInfo>,
}

#[derive(Component)]
pub struct SelectionDiscRoot;

#[derive(Component)]
pub struct HoverDiscRoot;

#[derive(SystemParam)]
pub struct ScenePickParams<'w, 's> {
    pub meshes: Res<'w, Assets<Mesh>>,
    pub marker_roots: Query<'w, 's, (Entity, &'static TacticalMarkerEntity, &'static Visibility)>,
    pub building_roots: Query<'w, 's, (Entity, &'static TacticalBuildingEntity)>,
    pub children: Query<'w, 's, &'static Children>,
    pub mesh_nodes: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            Option<&'static Aabb>,
            Option<&'static Mesh3d>,
        ),
    >,
}

#[derive(Component)]
pub struct TacticalMarkerEntity {
    pub id: Uuid,
    pub model_path: String,
    pub gltf_handle: Option<Handle<Gltf>>,
    pub anim_started: bool,
    pub basic_mesh_child: Option<Entity>,
    pub basic_geom_sig: u64,
    pub basic_mat_sig: u64,
    pub basic_mat_handle: Option<Handle<StandardMaterial>>,
}

pub fn setup_selection_highlight_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let flat_rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    // 1. Selected object filled glowing tactical disc + bright outer halo + subtle inner ring
    let fill_disc = meshes.add(Circle::new(1.0));
    let outer_ring = meshes.add(Annulus::new(0.90, 1.0));
    let inner_ring = meshes.add(Annulus::new(0.68, 0.72));

    let sel_fill_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.08, 0.88, 0.68, 0.24),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let sel_outer_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.18, 1.0, 0.78, 0.94),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let sel_inner_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.92, 0.98, 0.48),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    commands
        .spawn((
            Transform::IDENTITY,
            Visibility::Hidden,
            SelectionDiscRoot,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(fill_disc.clone()),
                MeshMaterial3d(sel_fill_mat),
                Transform::from_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(outer_ring.clone()),
                MeshMaterial3d(sel_outer_mat),
                Transform::from_xyz(0.0, 0.015, 0.0).with_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(inner_ring),
                MeshMaterial3d(sel_inner_mat),
                Transform::from_xyz(0.0, 0.012, 0.0).with_rotation(flat_rot),
            ));
        });

    // 2. Hovered unselected object soft sky-blue filled disc + crisp ring
    let hov_fill_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.74, 0.97, 0.14),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let hov_ring_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.35, 0.86, 1.0, 0.82),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands
        .spawn((Transform::IDENTITY, Visibility::Hidden, HoverDiscRoot))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(fill_disc),
                MeshMaterial3d(hov_fill_mat),
                Transform::from_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(outer_ring),
                MeshMaterial3d(hov_ring_mat),
                Transform::from_xyz(0.0, 0.012, 0.0).with_rotation(flat_rot),
            ));
        });
}

pub const DEFAULT_OBJECT_3D_SCALE: f64 = 0.1;
pub const MIN_OBJECT_3D_SCALE: f64 = 0.01;
pub const MAX_OBJECT_3D_SCALE: f64 = 50.0;

/// Spawns a 3D catalog model into the active scenario at `(lng, lat)`.
/// Shared by both Drag-and-Drop onto the 3D map and the "+ Tambah" button in the 3D Catalog panel.
pub fn spawn_marker_from_catalog(
    state: &mut AppState,
    api: &ApiClientBridge,
    entry: &ModelCatalogEntry,
    lng: f64,
    lat: f64,
) -> bool {
    let Some(sk) = state.active_skenario.clone() else {
        state.status_banner = Some("Pilih atau buka skenario terlebih dahulu.".into());
        return false;
    };

    let default_scale = DEFAULT_OBJECT_3D_SCALE;
    let rot_x = 0.0;
    let rot_y = 0.0;
    let rot_z = 0.0;

    if !state.tool_mode.is_transform_tool() {
        state.tool_mode = ToolMode::Select;
    }
    state.selected_catalog_id = Some(entry.id.clone());

    if state.offline_preview {
        let now = chrono::Utc::now();
        let m = Marker {
            id: Uuid::new_v4(),
            operasi_id: sk.operasi_id,
            skenario_id: sk.id,
            unit_id: entry.id.clone(),
            name: entry.name.clone(),
            category: entry.category.clone(),
            model_path: entry.model_path.clone(),
            quantity: 1,
            lng,
            lat,
            altitude: 0.0,
            rot_x,
            rot_y,
            rot_z,
            scale_x: default_scale,
            scale_y: default_scale,
            scale_z: default_scale,
            is_spawn: true,
            description: None,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        };
        state.selected = SelectedEntity::Marker(m.id);
        state.marker_edit = Some(m.clone());
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.anim_edit = None;
        state.markers.push(m);
        true
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateMarker {
            auth,
            req: CreateMarkerRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                unit_id: entry.id.clone(),
                name: entry.name.clone(),
                category: entry.category.clone(),
                model_path: entry.model_path.clone(),
                quantity: 1,
                lng,
                lat,
                altitude: 0.0,
                rot_x,
                rot_y,
                rot_z,
                scale_x: default_scale,
                scale_y: default_scale,
                scale_z: default_scale,
                is_spawn: true,
                description: None,
                metadata: serde_json::json!({}),
            },
        });
        true
    } else {
        state.status_banner = Some("Sesi login tidak aktif.".into());
        false
    }
}

/// Spawns a 3D catalog model at the current camera target on the map, applying a small spiral
/// offset if another marker already sits at the exact same spot.
pub fn spawn_marker_at_map_center(
    state: &mut AppState,
    tcam: &crate::map::camera::TacticalCamera,
    api: &ApiClientBridge,
    entry: &ModelCatalogEntry,
) -> bool {
    let base = tcam.target_local;
    let mut chosen = base;
    let spacing = 2.5_f32;
    for step in 0..24 {
        let candidate = if step == 0 {
            base
        } else {
            let angle = (step as f32) * 1.15;
            let dist = spacing * (1.0 + (step as f32) * 0.25);
            base + Vec2::new(angle.cos() * dist, angle.sin() * dist)
        };
        let occupied = state.markers.iter().any(|m| {
            let l = state.geo_anchor.lng_lat_alt_to_local(m.lng, m.lat, 0.0);
            Vec2::new(l[0] - candidate.x, l[2] - candidate.y).length() < spacing * 0.65
        });
        if !occupied {
            chosen = candidate;
            break;
        }
    }
    let (lng, lat, _) = state
        .geo_anchor
        .local_to_lng_lat_alt([chosen.x, 0.0, chosen.y]);
    spawn_marker_from_catalog(state, api, entry, lng, lat)
}

/// Spawns a procedural Basic Mesh into the active scenario at `(lng, lat)`.
/// Basic Meshes spawn at scale `1.0, 1.0, 1.0` so meter dimensions in Inspector match 1:1 on the map,
/// while remaining freely scalable down to `0.01` via Scale Gizmo or Inspector.
pub fn spawn_basic_mesh_marker(
    state: &mut AppState,
    api: &ApiClientBridge,
    cfg: BasicMeshConfig,
    custom_name: Option<String>,
    lng: f64,
    lat: f64,
) -> bool {
    let Some(sk) = state.active_skenario.clone() else {
        state.status_banner = Some("Pilih atau buka skenario terlebih dahulu.".into());
        return false;
    };

    if !state.tool_mode.is_transform_tool() {
        state.tool_mode = ToolMode::Select;
    }

    let count = state
        .markers
        .iter()
        .filter(|m| m.category == BASIC_MESH_CATEGORY)
        .count()
        + 1;
    let name = custom_name.unwrap_or_else(|| format!("{} {}", cfg.shape.label(), count));
    let mut dummy_meta = serde_json::json!({});
    if let Ok(val) = serde_json::to_value(&cfg) {
        dummy_meta = serde_json::json!({ "basic_mesh": val });
    }

    if state.offline_preview {
        let now = chrono::Utc::now();
        let mut m = Marker {
            id: Uuid::new_v4(),
            operasi_id: sk.operasi_id,
            skenario_id: sk.id,
            unit_id: format!("basic-{}", cfg.shape.slug()),
            name,
            category: BASIC_MESH_CATEGORY.to_string(),
            model_path: cfg.shape.model_path(),
            quantity: 1,
            lng,
            lat,
            altitude: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            is_spawn: true,
            description: None,
            metadata: dummy_meta,
            created_at: now,
            updated_at: now,
        };
        write_basic_mesh_config(&mut m, &cfg);
        state.selected = SelectedEntity::Marker(m.id);
        state.marker_edit = Some(m.clone());
        state.building_edit = None;
        state.line_edit = None;
        state.text_edit = None;
        state.anim_edit = None;
        state.markers.push(m);
        true
    } else if let Some(auth) = current_auth(state) {
        api.send(ApiCommand::CreateMarker {
            auth,
            req: CreateMarkerRequest {
                operasi_id: sk.operasi_id,
                skenario_id: sk.id,
                unit_id: format!("basic-{}", cfg.shape.slug()),
                name,
                category: BASIC_MESH_CATEGORY.to_string(),
                model_path: cfg.shape.model_path(),
                quantity: 1,
                lng,
                lat,
                altitude: 0.0,
                rot_x: 0.0,
                rot_y: 0.0,
                rot_z: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                scale_z: 1.0,
                is_spawn: true,
                description: None,
                metadata: dummy_meta,
            },
        });
        true
    } else {
        state.status_banner = Some("Sesi login tidak aktif.".into());
        false
    }
}

pub fn spawn_basic_mesh_at_map_center(
    state: &mut AppState,
    tcam: &crate::map::camera::TacticalCamera,
    api: &ApiClientBridge,
    shape: BasicMeshShape,
) -> bool {
    let base = tcam.target_local;
    let mut chosen = base;
    let spacing = 4.0_f32;
    for step in 0..24 {
        let candidate = if step == 0 {
            base
        } else {
            let angle = (step as f32) * 1.15;
            let dist = spacing * (1.0 + (step as f32) * 0.25);
            base + Vec2::new(angle.cos() * dist, angle.sin() * dist)
        };
        let occupied = state.markers.iter().any(|m| {
            let l = state.geo_anchor.lng_lat_alt_to_local(m.lng, m.lat, 0.0);
            Vec2::new(l[0] - candidate.x, l[2] - candidate.y).length() < spacing * 0.55
        });
        if !occupied {
            chosen = candidate;
            break;
        }
    }
    let (lng, lat, _) = state
        .geo_anchor
        .local_to_lng_lat_alt([chosen.x, 0.0, chosen.y]);
    spawn_basic_mesh_marker(state, api, BasicMeshConfig::default_for(shape), None, lng, lat)
}

/// Batch-saves all dirty scenario edits (markers, basic meshes, buildings, lines, text labels,
/// route animations, and scenario settings) to the backend API and broadcasts updates via WebSocket.
pub fn save_all_scenario_changes(
    state: &mut AppState,
    api: &ApiClientBridge,
    ws: &WsClientBridge,
) {
    if state.offline_preview {
        let count = state.unsaved_change_count();
        state.clear_all_dirty();
        state.status_banner = Some(if count > 0 {
            format!("{count} perubahan tersimpan di mode Offline Preview.")
        } else {
            "Semua perubahan sudah tersimpan (Offline Preview).".into()
        });
        return;
    }

    let Some(auth) = current_auth(state) else {
        state.status_banner = Some("Sesi login tidak aktif — gagal menyimpan.".into());
        return;
    };

    let marker_ids: Vec<Uuid> = state.dirty_marker_ids.drain().collect();
    let building_ids: Vec<Uuid> = state.dirty_building_ids.drain().collect();
    let line_ids: Vec<Uuid> = state.dirty_line_ids.drain().collect();
    let text_ids: Vec<Uuid> = state.dirty_text_ids.drain().collect();
    let anim_ids: Vec<Uuid> = state.dirty_anim_ids.drain().collect();
    let save_skenario = state.skenario_dirty;
    state.skenario_dirty = false;

    let mut sent = 0usize;

    for id in marker_ids {
        if let Some(m) = state.markers.iter().find(|x| x.id == id).cloned() {
            api.send(ApiCommand::UpdateMarker {
                auth: auth.clone(),
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
                command: "move_marker".to_string(),
                payload: serde_json::json!({
                    "lng": m.lng,
                    "lat": m.lat,
                    "altitude": m.altitude,
                    "rot_x": m.rot_x,
                    "rot_y": m.rot_y,
                    "rot_z": m.rot_z,
                    "scale_x": m.scale_x,
                    "scale_y": m.scale_y,
                    "scale_z": m.scale_z,
                }),
                save_history: true,
            }));
            sent += 1;
        }
    }

    for id in building_ids {
        if let Some(b) = state.buildings.iter().find(|x| x.id == id).cloned() {
            api.send(ApiCommand::UpdateBuilding {
                auth: auth.clone(),
                id: b.id,
                req: UpdateBuildingRequest {
                    name: b.name,
                    description: b.description,
                    color: b.color,
                    height: b.height,
                    geom: b.geom,
                },
            });
            sent += 1;
        }
    }

    for id in line_ids {
        if let Some(l) = state.lines.iter().find(|x| x.id == id).cloned() {
            api.send(ApiCommand::UpdateLine {
                auth: auth.clone(),
                id: l.id,
                req: UpdatePlotLineRequest {
                    name: l.name,
                    color: l.color,
                    width: l.width,
                    alpha: l.alpha,
                    geom: l.geom,
                },
            });
            sent += 1;
        }
    }

    for id in text_ids {
        if let Some(t) = state.text_markers.iter().find(|x| x.id == id).cloned() {
            api.send(ApiCommand::UpdateTextMarker {
                auth: auth.clone(),
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
            sent += 1;
        }
    }

    for id in anim_ids {
        if let Some(a) = state.anims.iter().find(|x| x.id == id).cloned() {
            api.send(ApiCommand::UpdateAnim {
                auth: auth.clone(),
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
            sent += 1;
        }
    }

    if save_skenario
        && let Some(sk) = state.active_skenario.clone()
    {
        api.send(ApiCommand::UpdateSkenario {
            auth: auth.clone(),
            id: sk.id,
            req: UpdateSkenarioRequest {
                name: sk.name,
                center_lng: sk.center_lng,
                center_lat: sk.center_lat,
                zoom: sk.zoom,
                min_zoom: sk.min_zoom,
                max_zoom: sk.max_zoom,
                pitch: sk.pitch,
                bearing: sk.bearing,
                map_mode: state.active_map_mode,
                base_layer: state.active_base_layer,
                skybox: state.active_skybox,
                indoor_config: sk.indoor_config,
            },
        });
        sent += 1;
    }

    if sent > 0 {
        state.saving_pending_count += sent;
        state.status_banner = Some(format!("Menyimpan {sent} perubahan skenario..."));
    } else {
        state.status_banner = Some("Semua perubahan skenario sudah tersimpan.".into());
    }
}

fn resolve_marker_render_path(
    state: &AppState,
    marker: &Marker,
    is_moving_in_play: bool,
) -> String {
    if is_moving_in_play
        && let Some(entry) = state
            .catalog
            .iter()
            .find(|c| c.model_path == marker.model_path || c.id == marker.unit_id)
        && let Some(walk_path) = &entry.walk_model_path
        && (state.available_model_paths.contains(walk_path)
            || state.assets_root.join(walk_path).exists())
    {
        return walk_path.clone();
    }
    marker.model_path.clone()
}

#[allow(clippy::too_many_arguments)]
pub fn sync_markers_system(
    mut commands: Commands,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    asset_server: Res<AssetServer>,
    mut tex_cache: ResMut<BasicMeshTextureCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut existing_q: Query<(
        Entity,
        &mut TacticalMarkerEntity,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    if state.screen != AppScreen::PlotSkenario {
        for (entity, _, _, _) in &existing_q {
            commands.entity(entity).despawn();
        }
        return;
    }

    let wanted: HashMap<Uuid, (&Marker, String)> = state
        .markers
        .iter()
        .map(|m| {
            let moving = playback.play_test_mode
                && overrides.markers.get(&m.id).is_some_and(|ov| ov.moving);
            let path = resolve_marker_render_path(&state, m, moving);
            (m.id, (m, path))
        })
        .collect();
    let mut present: HashSet<Uuid> = HashSet::new();

    for (entity, mut comp, mut tf, mut vis) in &mut existing_q {
        let Some((marker, wanted_path)) = wanted.get(&comp.id) else {
            commands.entity(entity).despawn();
            continue;
        };

        // Procedural Basic Mesh: update geometry & material smoothly IN-PLACE (zero flicker!)
        if let Some(cfg) = parse_basic_mesh_config(marker) {
            if comp.basic_mesh_child.is_none() {
                commands.entity(entity).despawn();
                continue;
            }
            present.insert(comp.id);
            comp.model_path = wanted_path.clone();

            let geom_sig = cfg.geometry_signature();
            if geom_sig != comp.basic_geom_sig
                && let Some(child_e) = comp.basic_mesh_child
            {
                let new_mesh = meshes.add(build_basic_mesh_geometry(&cfg));
                commands
                    .entity(child_e)
                    .insert(Mesh3d(new_mesh))
                    .remove::<Aabb>();
                comp.basic_geom_sig = geom_sig;
            }

            let mat_sig = cfg.material_signature();
            if mat_sig != comp.basic_mat_sig
                && let Some(mat_h) = &comp.basic_mat_handle
                && let Some(mut mat) = materials.get_mut(mat_h)
            {
                *mat = build_basic_mesh_material(
                    &cfg,
                    &mut tex_cache,
                    &asset_server,
                    &state.assets_root,
                    &state.available_model_paths,
                );
                comp.basic_mat_sig = mat_sig;
            }
        } else {
            if comp.basic_mesh_child.is_some() || &comp.model_path != wanted_path {
                commands.entity(entity).despawn();
                continue;
            }
            present.insert(comp.id);
        }

        let (lng, lat, alt, rot_y) = if let Some(ov) = overrides.markers.get(&marker.id) {
            (ov.lng, ov.lat, ov.altitude, ov.rot_y)
        } else {
            (marker.lng, marker.lat, marker.altitude, marker.rot_y)
        };

        let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
        tf.translation = Vec3::from_array(local);
        tf.rotation = Quat::from_euler(
            EulerRot::YXZ,
            (rot_y as f32).to_radians(),
            (marker.rot_x as f32).to_radians(),
            (marker.rot_z as f32).to_radians(),
        );
        tf.scale = Vec3::new(
            (marker.scale_x as f32).max(MIN_OBJECT_3D_SCALE as f32),
            (marker.scale_y as f32).max(MIN_OBJECT_3D_SCALE as f32),
            (marker.scale_z as f32).max(MIN_OBJECT_3D_SCALE as f32),
        );
        *vis = if marker.is_spawn {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    for marker in &state.markers {
        if present.contains(&marker.id) {
            continue;
        }

        let Some((_, render_path)) = wanted.get(&marker.id) else {
            continue;
        };
        let render_path = render_path.clone();

        let (lng, lat, alt, rot_y) = if let Some(ov) = overrides.markers.get(&marker.id) {
            (ov.lng, ov.lat, ov.altitude, ov.rot_y)
        } else {
            (marker.lng, marker.lat, marker.altitude, marker.rot_y)
        };

        let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
        let root_tf = Transform {
            translation: Vec3::from_array(local),
            rotation: Quat::from_euler(
                EulerRot::YXZ,
                (rot_y as f32).to_radians(),
                (marker.rot_x as f32).to_radians(),
                (marker.rot_z as f32).to_radians(),
            ),
            scale: Vec3::new(
                (marker.scale_x as f32).max(MIN_OBJECT_3D_SCALE as f32),
                (marker.scale_y as f32).max(MIN_OBJECT_3D_SCALE as f32),
                (marker.scale_z as f32).max(MIN_OBJECT_3D_SCALE as f32),
            ),
        };

        let vis = if marker.is_spawn {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        // Case 1: Procedural Basic Mesh
        if let Some(cfg) = parse_basic_mesh_config(marker) {
            let geom_sig = cfg.geometry_signature();
            let mat_sig = cfg.material_signature();
            let mesh_h = meshes.add(build_basic_mesh_geometry(&cfg));
            let mat_h = materials.add(build_basic_mesh_material(
                &cfg,
                &mut tex_cache,
                &asset_server,
                &state.assets_root,
                &state.available_model_paths,
            ));

            let child_id = commands
                .spawn((
                    Mesh3d(mesh_h),
                    MeshMaterial3d(mat_h.clone()),
                    Transform::IDENTITY,
                ))
                .id();
            let parent = commands
                .spawn((
                    root_tf,
                    vis,
                    TacticalMarkerEntity {
                        id: marker.id,
                        model_path: render_path,
                        gltf_handle: None,
                        anim_started: false,
                        basic_mesh_child: Some(child_id),
                        basic_geom_sig: geom_sig,
                        basic_mat_sig: mat_sig,
                        basic_mat_handle: Some(mat_h),
                    },
                ))
                .id();
            commands.entity(parent).add_child(child_id);
            continue;
        }

        // Case 2: glTF 3D Model or fallback primitive
        let has_glb = state.available_model_paths.contains(&render_path)
            || state.assets_root.join(&render_path).exists();

        let gltf_handle: Option<Handle<Gltf>> = if has_glb {
            Some(asset_server.load(render_path.clone()))
        } else {
            None
        };

        let parent = commands
            .spawn((
                root_tf,
                vis,
                TacticalMarkerEntity {
                    id: marker.id,
                    model_path: render_path.clone(),
                    gltf_handle,
                    anim_started: false,
                    basic_mesh_child: None,
                    basic_geom_sig: 0,
                    basic_mat_sig: 0,
                    basic_mat_handle: None,
                },
            ))
            .id();

        if has_glb {
            let scene_handle =
                asset_server.load(GltfAssetLabel::Scene(0).from_asset(render_path));
            commands.entity(parent).with_children(|child| {
                child.spawn(WorldAssetRoot(scene_handle));
            });
        } else {
            let color = match marker.category.as_str() {
                "kendaraan" | "kendaraans" | "kendaraan_umums" => Color::srgb(0.15, 0.55, 0.95),
                "bangunan" | "bangunans" => Color::srgb(0.22, 0.80, 0.55),
                "alat" | "tools" => Color::srgb(0.95, 0.65, 0.15),
                _ => Color::srgb(0.92, 0.28, 0.32),
            };
            let body_mesh = meshes.add(Cylinder::new(0.8, 1.8));
            let nose_mesh = meshes.add(Cone::new(0.45, 1.1));
            let mat = materials.add(StandardMaterial {
                base_color: color,
                metallic: 0.25,
                perceptual_roughness: 0.5,
                ..default()
            });
            commands.entity(parent).with_children(|child| {
                child.spawn((
                    Mesh3d(body_mesh),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_xyz(0.0, 0.9, 0.0),
                ));
                child.spawn((
                    Mesh3d(nose_mesh),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(0.0, 0.9, -1.2)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ));
            });
        }
    }
}

/// Drives embedded glTF AnimationPlayers (rotors, walk cycles, patrol rigs) on 3D markers
/// whenever the user runs Play Test Mode in the Scenario Plot Editor.
#[allow(clippy::too_many_arguments)]
pub fn animate_markers_system(
    mut commands: Commands,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    gltfs: Res<Assets<Gltf>>,
    mut anim_graphs: ResMut<Assets<AnimationGraph>>,
    mut marker_q: Query<(Entity, &mut TacticalMarkerEntity)>,
    children_q: Query<&Children>,
    mut anim_player_q: Query<(Entity, &mut AnimationPlayer)>,
) {
    if state.screen != AppScreen::PlotSkenario {
        return;
    }

    let should_play = playback.play_test_mode && playback.playing;
    let speed = playback.speed_multiplier.max(0.1);

    for (root_entity, mut comp) in &mut marker_q {
        let Some(gltf_handle) = &comp.gltf_handle else {
            continue;
        };
        let Some(gltf) = gltfs.get(gltf_handle) else {
            continue;
        };
        if gltf.animations.is_empty() {
            continue;
        }

        let mut stack = vec![root_entity];
        let mut player_entities = Vec::new();
        while let Some(e) = stack.pop() {
            if anim_player_q.contains(e) {
                player_entities.push(e);
            }
            if let Ok(children) = children_q.get(e) {
                for child in children.iter() {
                    stack.push(child);
                }
            }
        }

        if player_entities.is_empty() {
            continue;
        }

        if !comp.anim_started {
            let (graph, node_idx) = AnimationGraph::from_clip(gltf.animations[0].clone());
            let graph_handle = anim_graphs.add(graph);
            for pe in &player_entities {
                commands
                    .entity(*pe)
                    .insert(AnimationGraphHandle(graph_handle.clone()));
                if let Ok((_, mut player)) = anim_player_q.get_mut(*pe) {
                    let active = player.play(node_idx).repeat();
                    active.set_speed(speed);
                    if !should_play {
                        active.pause();
                    }
                }
            }
            comp.anim_started = true;
        } else {
            for pe in &player_entities {
                if let Ok((_, mut player)) = anim_player_q.get_mut(*pe) {
                    for (_, active) in player.playing_animations_mut() {
                        active.set_speed(speed);
                        if should_play {
                            if active.is_paused() {
                                active.resume();
                            }
                        } else if !active.is_paused() {
                            active.pause();
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn selection_gizmo_system(
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    gizmo_state: Res<TransformGizmoState>,
    mut marker_bounds: ResMut<MarkerWorldBounds>,
    marker_roots_q: Query<(Entity, &TacticalMarkerEntity, &Visibility)>,
    children_q: Query<&Children>,
    mesh_aabb_q: Query<(&GlobalTransform, &Aabb)>,
    mut sel_disc_q: Query<
        (&mut Transform, &mut Visibility),
        (With<SelectionDiscRoot>, Without<TacticalMarkerEntity>, Without<HoverDiscRoot>),
    >,
    mut hov_disc_q: Query<
        (&mut Transform, &mut Visibility),
        (With<HoverDiscRoot>, Without<TacticalMarkerEntity>, Without<SelectionDiscRoot>),
    >,
    mut gizmos: Gizmos,
) {
    let hide_discs = |sel_q: &mut Query<
        (&mut Transform, &mut Visibility),
        (With<SelectionDiscRoot>, Without<TacticalMarkerEntity>, Without<HoverDiscRoot>),
    >,
                      hov_q: &mut Query<
        (&mut Transform, &mut Visibility),
        (With<HoverDiscRoot>, Without<TacticalMarkerEntity>, Without<SelectionDiscRoot>),
    >| {
        if let Ok((_, mut vis)) = sel_q.single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = hov_q.single_mut() {
            *vis = Visibility::Hidden;
        }
    };

    if state.screen != AppScreen::PlotSkenario {
        marker_bounds.bounds.clear();
        hide_discs(&mut sel_disc_q, &mut hov_disc_q);
        return;
    }

    // Recompute exact real-world 3D bounding boxes for every spawned 3D model / Basic Mesh
    marker_bounds.bounds.clear();
    let mut stack: Vec<Entity> = Vec::with_capacity(16);
    for (root_entity, comp, vis) in &marker_roots_q {
        if *vis == Visibility::Hidden {
            continue;
        }
        let mut wmin = Vec3::splat(f32::INFINITY);
        let mut wmax = Vec3::splat(f32::NEG_INFINITY);
        let mut found_any = false;

        stack.clear();
        stack.push(root_entity);
        while let Some(e) = stack.pop() {
            if let Ok((gtf, aabb)) = mesh_aabb_q.get(e) {
                let c = Vec3::from(aabb.center);
                let h = Vec3::from(aabb.half_extents);
                let aff = gtf.affine();
                for sx in [-1.0_f32, 1.0] {
                    for sy in [-1.0_f32, 1.0] {
                        for sz in [-1.0_f32, 1.0] {
                            let pt = aff.transform_point3(c + Vec3::new(sx * h.x, sy * h.y, sz * h.z));
                            wmin = wmin.min(pt);
                            wmax = wmax.max(pt);
                            found_any = true;
                        }
                    }
                }
            }
            if let Ok(children) = children_q.get(e) {
                for child in children.iter() {
                    stack.push(child);
                }
            }
        }

        if found_any && wmin.is_finite() && wmax.is_finite() {
            let cx = (wmin.x + wmax.x) * 0.5;
            let cz = (wmin.z + wmax.z) * 0.5;
            let hx = (wmax.x - wmin.x) * 0.5;
            let hz = (wmax.z - wmin.z) * 0.5;
            let footprint_radius = (hx.hypot(hz) * 1.15).max(0.45);
            let ground_y = wmin.y.clamp(0.04, 0.06);
            marker_bounds.bounds.insert(
                comp.id,
                MarkerBoundsInfo {
                    world_min: wmin,
                    world_max: wmax,
                    footprint_center: Vec3::new(cx, ground_y, cz),
                    footprint_radius,
                },
            );
        }
    }

    // 3D perspective landing reticle while dragging a 3D Catalog model OR Basic Mesh onto the map
    if (state.dragging_catalog_entry.is_some() || state.dragging_basic_mesh.is_some())
        && let Some(drop_pos) = state.catalog_drop_world_pos
    {
        let center = Vec3::new(drop_pos.x, 0.18, drop_pos.z);
        let ring_r = 4.0_f32;
        gizmos.circle(
            Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ring_r,
            Color::srgb(0.22, 0.85, 0.98),
        );
        gizmos.circle(
            Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ring_r * 0.45,
            Color::srgb(0.20, 0.95, 0.55),
        );
        gizmos.line(
            center - Vec3::X * (ring_r * 1.3),
            center + Vec3::X * (ring_r * 1.3),
            Color::srgb(0.22, 0.85, 0.98),
        );
        gizmos.line(
            center - Vec3::Z * (ring_r * 1.3),
            center + Vec3::Z * (ring_r * 1.3),
            Color::srgb(0.22, 0.85, 0.98),
        );
        gizmos.line(
            center,
            center + Vec3::Y * 8.0,
            Color::srgb(0.20, 0.95, 0.55),
        );
    }

    if !state.show_gizmos || (playback.play_test_mode && playback.hide_gizmos_in_play) {
        hide_discs(&mut sel_disc_q, &mut hov_disc_q);
        return;
    }

    // Update Hover Disc (soft glowing filled disc scaled to actual 3D model bounds)
    let mut show_hover_disc = false;
    if let Some(SelectedEntity::Marker(hid)) = gizmo_state.hovered_entity
        && state.selected != SelectedEntity::Marker(hid)
        && let Some(m) = state.markers.iter().find(|m| m.id == hid)
        && let Ok((mut htf, mut hvis)) = hov_disc_q.single_mut()
    {
        let (center, radius) = if let Some(b) = marker_bounds.bounds.get(&hid) {
            (b.footprint_center, b.footprint_radius)
        } else {
            let (lng, lat, alt) = if let Some(ov) = overrides.markers.get(&m.id) {
                (ov.lng, ov.lat, ov.altitude)
            } else {
                (m.lng, m.lat, m.altitude)
            };
            let p = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
            (
                Vec3::new(p[0], 0.06, p[2]),
                (m.scale_x.max(m.scale_z) as f32 * 2.2).max(0.75),
            )
        };
        htf.translation = center;
        htf.scale = Vec3::new(radius, 1.0, radius);
        *hvis = Visibility::Visible;
        show_hover_disc = true;
    }
    if !show_hover_disc
        && let Ok((_, mut hvis)) = hov_disc_q.single_mut()
    {
        *hvis = Visibility::Hidden;
    }

    // Update Selected Highlight (filled translucent glowing ground disc + 3D corner brackets!)
    let mut show_sel_disc = false;
    match state.selected {
        SelectedEntity::Marker(id) => {
            if let Some(m) = state.markers.iter().find(|m| m.id == id) {
                let pulse = 1.0 + 0.025 * (time.elapsed_secs() * 3.2).sin();
                if let Some(b) = marker_bounds.bounds.get(&id).copied() {
                    if let Ok((mut stf, mut svis)) = sel_disc_q.single_mut() {
                        stf.translation = b.footprint_center;
                        let r = b.footprint_radius * pulse;
                        stf.scale = Vec3::new(r, 1.0, r);
                        *svis = Visibility::Visible;
                        show_sel_disc = true;
                    }
                    // Crisp 8-corner 3D bounding box brackets around the exact model volume
                    draw_3d_corner_brackets(
                        &mut gizmos,
                        b.world_min,
                        b.world_max,
                        Color::srgba(0.20, 0.96, 0.78, 0.92),
                    );
                } else {
                    let (lng, lat, alt) = if let Some(ov) = overrides.markers.get(&m.id) {
                        (ov.lng, ov.lat, ov.altitude)
                    } else {
                        (m.lng, m.lat, m.altitude)
                    };
                    let p = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
                    let r = (m.scale_x.max(m.scale_z) as f32 * 2.2).max(0.75) * pulse;
                    if let Ok((mut stf, mut svis)) = sel_disc_q.single_mut() {
                        stf.translation = Vec3::new(p[0], 0.06, p[2]);
                        stf.scale = Vec3::new(r, 1.0, r);
                        *svis = Visibility::Visible;
                        show_sel_disc = true;
                    }
                }
            }
        }
        SelectedEntity::Building(id) => {
            if let Some(b) = state.buildings.iter().find(|b| b.id == id) {
                for w in b.geom.windows(2) {
                    let a = state
                        .geo_anchor
                        .lng_lat_alt_to_local(w[0][0], w[0][1], b.height + 0.3);
                    let c = state
                        .geo_anchor
                        .lng_lat_alt_to_local(w[1][0], w[1][1], b.height + 0.3);
                    gizmos.line(
                        Vec3::from_array(a),
                        Vec3::from_array(c),
                        Color::srgb(0.2, 1.0, 0.6),
                    );
                }
            }
        }
        SelectedEntity::TextMarker(id) => {
            if let Some(t) = state.text_markers.iter().find(|t| t.id == id) {
                let p = state.geo_anchor.lng_lat_alt_to_local(t.lng, t.lat, t.altitude);
                gizmos.sphere(
                    Isometry3d::from_translation(Vec3::from_array(p)),
                    1.6,
                    Color::srgb(0.75, 0.45, 0.98),
                );
            }
        }
        _ => {}
    }

    if !show_sel_disc
        && let Ok((_, mut svis)) = sel_disc_q.single_mut()
    {
        *svis = Visibility::Hidden;
    }
}

/// Draws crisp Unity / Godot style 3D corner L-brackets framing the exact world bounding box.
fn draw_3d_corner_brackets(gizmos: &mut Gizmos, min: Vec3, max: Vec3, color: Color) {
    let size = (max - min).max(Vec3::splat(0.15));
    let pad = (size.max_element() * 0.03).clamp(0.03, 0.35);
    let lo = min - Vec3::new(pad, 0.0, pad);
    let hi = max + Vec3::new(pad, pad, pad);
    let ext = hi - lo;
    let lx = (ext.x * 0.22).clamp(0.12, 3.5);
    let ly = (ext.y * 0.22).clamp(0.12, 3.5);
    let lz = (ext.z * 0.22).clamp(0.12, 3.5);

    for sx in [0, 1] {
        let x = if sx == 0 { lo.x } else { hi.x };
        let dx = if sx == 0 { lx } else { -lx };
        for sy in [0, 1] {
            let y = if sy == 0 { lo.y } else { hi.y };
            let dy = if sy == 0 { ly } else { -ly };
            for sz in [0, 1] {
                let z = if sz == 0 { lo.z } else { hi.z };
                let dz = if sz == 0 { lz } else { -lz };
                let corner = Vec3::new(x, y, z);
                gizmos.line(corner, corner + Vec3::X * dx, color);
                gizmos.line(corner, corner + Vec3::Z * dz, color);
                if ext.y > 0.2 {
                    gizmos.line(corner, corner + Vec3::Y * dy, color);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn map_interaction_system(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_state: Res<UiPointerState>,
    mut state: ResMut<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    marker_bounds: Res<MarkerWorldBounds>,
    mut gizmo_state: ResMut<TransformGizmoState>,
    mut poly_edit: ResMut<PolygonEditState>,
    api: Res<ApiClientBridge>,
    ws: Res<WsClientBridge>,
    cam_q: Query<(&Camera, &GlobalTransform), With<crate::map::camera::TacticalCamera>>,
    pick: ScenePickParams,
) {
    if state.screen != AppScreen::PlotSkenario {
        gizmo_state.cancel();
        poly_edit.clear();
        state.dragging_catalog_entry = None;
        state.dragging_basic_mesh = None;
        state.catalog_drop_world_pos = None;
        return;
    }

    if playback.play_test_mode {
        gizmo_state.cancel();
        poly_edit.clear();
        state.dragging_catalog_entry = None;
        state.dragging_basic_mesh = None;
        state.catalog_drop_world_pos = None;
    }

    // Keep PolygonEditState synchronized with current selection, lock status & shape
    if let Some(edit_id) = poly_edit.editing_marker_id {
        let valid = matches!(state.selected, SelectedEntity::Marker(sid) if sid == edit_id)
            && state
                .markers
                .iter()
                .find(|m| m.id == edit_id)
                .is_some_and(|m| {
                    !state.is_marker_locked(m)
                        && parse_basic_mesh_config(m)
                            .is_some_and(|c| c.shape == BasicMeshShape::Polygon)
                });
        if !valid {
            poly_edit.clear();
        } else if let Some(vi) = poly_edit.selected_vertex {
            let n = state
                .markers
                .iter()
                .find(|m| m.id == edit_id)
                .and_then(parse_basic_mesh_config)
                .map(|c| c.polygon_points.len())
                .unwrap_or(0);
            if vi >= n {
                poly_edit.selected_vertex = None;
            }
        }
    }

    if !mouse_buttons.pressed(MouseButton::Left) {
        gizmo_state.suppress_camera_pan = false;
    }

    // Keyboard shortcuts: Ctrl+S save, Delete selected (with confirm), transform toolbar (1/2/3/4), draft undo/finish/cancel
    if state.pending_delete_entity.is_some() {
        if keys.just_pressed(KeyCode::Escape) {
            state.pending_delete_entity = None;
        }
        return;
    }

    if !ui_state.wants_keyboard {
        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        if ctrl && keys.just_pressed(KeyCode::KeyS) && !playback.play_test_mode {
            save_all_scenario_changes(&mut state, &api, &ws);
            return;
        }
        if keys.just_pressed(KeyCode::Delete)
            && !playback.play_test_mode
            && !poly_edit.is_active()
            && state.selected != SelectedEntity::None
            && !state.is_entity_locked(state.selected)
        {
            let sel = state.selected;
            state.request_delete_entity(sel);
            return;
        }
        if keys.just_pressed(KeyCode::Backspace)
            && matches!(
                state.tool_mode,
                ToolMode::DrawLine
                    | ToolMode::DrawBuilding
                    | ToolMode::DrawBasicPolygon
                    | ToolMode::DrawRoute
            )
            && !state.draft_points_lnglat.is_empty()
        {
            state.draft_points_lnglat.pop();
            return;
        }
        if keys.just_pressed(KeyCode::Escape) {
            if poly_edit.selected_vertex.is_some() {
                poly_edit.selected_vertex = None;
                poly_edit.active_drag = None;
                return;
            }
            if poly_edit.is_active() {
                poly_edit.clear();
                return;
            }
            state.draft_points_lnglat.clear();
            state.measure_points_lnglat.clear();
            state.dragging_catalog_entry = None;
            state.dragging_basic_mesh = None;
            state.catalog_drop_world_pos = None;
            state.tool_mode = ToolMode::Select;
            gizmo_state.cancel();
            return;
        }
        if keys.just_pressed(KeyCode::Enter) {
            finish_active_draft(&mut state, &api);
            return;
        }
        if !playback.play_test_mode && !ctrl {
            if keys.just_pressed(KeyCode::Digit1) {
                state.tool_mode = ToolMode::Select;
                state.draft_points_lnglat.clear();
                gizmo_state.cancel();
                poly_edit.clear();
            } else if keys.just_pressed(KeyCode::Digit2) {
                state.tool_mode = ToolMode::MoveMarker;
                state.draft_points_lnglat.clear();
                gizmo_state.cancel();
                poly_edit.clear();
            } else if keys.just_pressed(KeyCode::Digit3) {
                state.tool_mode = ToolMode::RotateMarker;
                state.draft_points_lnglat.clear();
                gizmo_state.cancel();
                poly_edit.clear();
            } else if keys.just_pressed(KeyCode::Digit4) {
                state.tool_mode = ToolMode::ScaleMarker;
                state.draft_points_lnglat.clear();
                gizmo_state.cancel();
                poly_edit.clear();
            }
        }
    }

    // Do not interact with 3D gizmos or entities while dragging a 3D Catalog or Basic Mesh item
    if state.dragging_catalog_entry.is_some() || state.dragging_basic_mesh.is_some() {
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
        gizmo_state.empty_click_start_px = None;
        poly_edit.hovered = None;
        return;
    }

    let Ok((camera, cam_gtf)) = cam_q.single() else {
        return;
    };

    // Release active polygon vertex gizmo drag
    if (mouse_buttons.just_released(MouseButton::Left) || !mouse_buttons.pressed(MouseButton::Left))
        && let Some(finished) = poly_edit.active_drag.take()
    {
        if finished.moved {
            state.status_banner = Some(format!(
                "Titik #{} polygon dipindahkan — {} perubahan belum disimpan (Simpan / Ctrl+S)",
                finished.vertex_idx + 1,
                state.unsaved_change_count()
            ));
        }
        gizmo_state.suppress_camera_pan = false;
        gizmo_state.empty_click_start_px = None;
        return;
    }

    // Continue active polygon vertex gizmo drag smoothly
    if poly_edit.active_drag.is_some() && mouse_buttons.pressed(MouseButton::Left) {
        if let Some(cursor_px) = window.cursor_position() {
            let mut drag = poly_edit.active_drag.take().unwrap();
            update_vertex_drag(&mut drag, cursor_px, &mut state, camera, cam_gtf);
            poly_edit.active_drag = Some(drag);
        }
        return;
    }

    // Release active transform gizmo drag: mark marker dirty so toolbar Simpan saves all edits
    if (mouse_buttons.just_released(MouseButton::Left) || !mouse_buttons.pressed(MouseButton::Left))
        && let Some(finished) = gizmo_state.active_drag.take()
    {
        if finished.moved {
            state.dirty_marker_ids.insert(finished.marker_id);
            state.status_banner = Some(format!(
                "{} perubahan belum disimpan — klik Simpan (Ctrl+S) di toolbar",
                state.unsaved_change_count()
            ));
        }
        gizmo_state.suppress_camera_pan = false;
        gizmo_state.empty_click_start_px = None;
        return;
    }

    // Continue active transform gizmo drag smoothly
    if gizmo_state.active_drag.is_some() && mouse_buttons.pressed(MouseButton::Left) {
        if let Some(cursor_px) = window.cursor_position() {
            let mut drag = gizmo_state.active_drag.take().unwrap();
            update_gizmo_drag(&mut drag, cursor_px, &mut state, camera, cam_gtf);
            gizmo_state.active_drag = Some(drag);
        }
        return;
    }

    // Cancel pending empty-map click-to-deselect if user dragged to pan the map
    if let Some(start_px) = gizmo_state.empty_click_start_px {
        if mouse_buttons.pressed(MouseButton::Left) {
            if let Some(cursor_px) = window.cursor_position()
                && cursor_px.distance(start_px) > 5.0
            {
                gizmo_state.empty_click_start_px = None;
            }
        } else if mouse_buttons.just_released(MouseButton::Left) {
            gizmo_state.empty_click_start_px = None;
            if !ui_state.wants_pointer
                && window
                    .cursor_position()
                    .is_some_and(|p| p.distance(start_px) <= 5.0)
            {
                clear_selection(&mut state);
                gizmo_state.hovered_handle = None;
            }
        } else {
            gizmo_state.empty_click_start_px = None;
        }
    }

    if ui_state.wants_pointer {
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
        poly_edit.hovered = None;
        return;
    }

    let Some(cursor_px) = window.cursor_position() else {
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
        poly_edit.hovered = None;
        return;
    };

    // Continuous polygon vertex hover hit-testing when Polygon Vertex Edit Mode is active
    if !playback.play_test_mode
        && let Some(edit_id) = poly_edit.editing_marker_id
    {
        poly_edit.hovered = state.markers.iter().find(|m| m.id == edit_id).and_then(|m| {
            let cfg = parse_basic_mesh_config(m)?;
            hit_test_polygon_editor(
                cursor_px,
                &state,
                m,
                &cfg,
                poly_edit.selected_vertex,
                camera,
                cam_gtf,
            )
        });
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
    } else {
        poly_edit.hovered = None;

        // Continuous gizmo hover hit-testing for crisp highlight & cursor feedback
        gizmo_state.hovered_handle = if !playback.play_test_mode && state.show_gizmos {
            if let (Some(mode), SelectedEntity::Marker(sel_id)) =
                (gizmo_mode_for_tool(state.tool_mode), state.selected)
            {
                state
                    .markers
                    .iter()
                    .find(|m| m.id == sel_id && !state.is_marker_locked(m))
                    .and_then(|m| {
                        let origin = marker_world_origin(&state, m);
                        let rot_quat = marker_quat(m);
                        let radius = compute_gizmo_world_radius(
                            camera,
                            cam_gtf,
                            origin,
                            GIZMO_SCREEN_RADIUS_PX,
                        );
                        hit_test_gizmo(cursor_px, mode, origin, rot_quat, radius, camera, cam_gtf)
                    })
            } else {
                None
            }
        } else {
            None
        };
    }

    let hit_opt = cursor_world_ground(&window, camera, cam_gtf);

    // Continuous 3D body / label / highlight hover detection in transform modes
    gizmo_state.hovered_entity = if !poly_edit.is_active()
        && state.tool_mode.is_transform_tool()
        && gizmo_state.hovered_handle.is_none()
    {
        pick_scene_entity(
            &state,
            &overrides,
            &marker_bounds,
            cursor_px,
            hit_opt,
            camera,
            cam_gtf,
            &pick,
        )
    } else {
        None
    };

    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // Priority 0: Interactive Polygon Vertex Editing (Vertex click -> Move Gizmo, Midpoint '+' insert, Map Click Add)
    if !playback.play_test_mode
        && let Some(edit_id) = poly_edit.editing_marker_id
    {
        match poly_edit.hovered {
            Some(PolygonHoverTarget::GizmoHandle(handle)) => {
                if let Some(vi) = poly_edit.selected_vertex
                    && let Some(marker) = state.markers.iter().find(|m| m.id == edit_id).cloned()
                    && let Some(cfg) = parse_basic_mesh_config(&marker)
                    && let Some(drag) = begin_vertex_drag(
                        &state,
                        &marker,
                        &cfg,
                        vi,
                        handle,
                        cursor_px,
                        camera,
                        cam_gtf,
                    )
                {
                    poly_edit.active_drag = Some(drag);
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                }
                return;
            }
            Some(PolygonHoverTarget::Vertex(vi)) => {
                poly_edit.selected_vertex = Some(vi);
                poly_edit.add_point_on_map_mode = false;
                gizmo_state.suppress_camera_pan = true;
                gizmo_state.empty_click_start_px = None;
                state.status_banner = Some(format!(
                    "Titik #{} dipilih — geser sumbu Merah (X), Biru (Z), atau Kuning Tengah (Free).",
                    vi + 1
                ));
                return;
            }
            Some(PolygonHoverTarget::Midpoint(edge_idx)) => {
                if let Some(mut marker_clone) =
                    state.markers.iter().find(|m| m.id == edit_id).cloned()
                    && let Some(mut cfg) = parse_basic_mesh_config(&marker_clone)
                {
                    ensure_editable_polygon_points(&mut cfg);
                    let new_idx = insert_midpoint_after(&mut cfg.polygon_points, edge_idx);
                    write_basic_mesh_config(&mut marker_clone, &cfg);
                    if let Some(m) = state.markers.iter_mut().find(|x| x.id == edit_id) {
                        *m = marker_clone.clone();
                    }
                    if state.marker_edit.as_ref().is_some_and(|e| e.id == edit_id) {
                        state.marker_edit = Some(marker_clone.clone());
                    }
                    state.dirty_marker_ids.insert(edit_id);
                    poly_edit.selected_vertex = Some(new_idx);
                    poly_edit.add_point_on_map_mode = false;
                    // Immediately start a Free drag so click-and-drag on '+' pulls out a new corner in one gesture!
                    if let Some(drag) = begin_vertex_drag(
                        &state,
                        &marker_clone,
                        &cfg,
                        new_idx,
                        PolygonVertexGizmoHandle::Free,
                        cursor_px,
                        camera,
                        cam_gtf,
                    ) {
                        poly_edit.active_drag = Some(drag);
                    }
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                    state.status_banner = Some(format!(
                        "Titik #{} disisipkan pada sisi polygon — tarik untuk membentuk sudut baru.",
                        new_idx + 1
                    ));
                }
                return;
            }
            None => {
                if poly_edit.add_point_on_map_mode {
                    if let Some(new_idx) =
                        insert_vertex_from_cursor_click(&mut state, edit_id, cursor_px, camera, cam_gtf)
                    {
                        poly_edit.selected_vertex = Some(new_idx);
                        gizmo_state.suppress_camera_pan = true;
                        gizmo_state.empty_click_start_px = None;
                        state.status_banner = Some(format!(
                            "Titik #{} ditambahkan pada sisi terdekat — geser Gizmo atau klik peta untuk tambah lagi.",
                            new_idx + 1
                        ));
                    }
                    return;
                }
                // Clicking empty map while editing polygon vertices hides the vertex Move Gizmo
                // while staying in vertex-edit mode and still letting the user pan the camera.
                if poly_edit.selected_vertex.is_some() {
                    poly_edit.selected_vertex = None;
                }
                return;
            }
        }
    }

    // Priority 1: Clicking a handle on the active 3D Transform Gizmo (Move / Rotate / Scale)
    if !playback.play_test_mode
        && let (Some(mode), Some(handle), SelectedEntity::Marker(sel_id)) = (
            gizmo_mode_for_tool(state.tool_mode),
            gizmo_state.hovered_handle,
            state.selected,
        )
        && let Some(marker) = state
            .markers
            .iter()
            .find(|m| m.id == sel_id && !state.is_marker_locked(m))
            .cloned()
        && let Some(drag) =
            begin_gizmo_drag(&marker, mode, handle, cursor_px, &state, camera, cam_gtf)
    {
        gizmo_state.active_drag = Some(drag);
        gizmo_state.suppress_camera_pan = true;
        gizmo_state.empty_click_start_px = None;
        return;
    }

    match state.tool_mode {
        ToolMode::Select
        | ToolMode::MoveMarker
        | ToolMode::RotateMarker
        | ToolMode::ScaleMarker => {
            match gizmo_state.hovered_entity {
                Some(SelectedEntity::Marker(id)) => {
                    state.selected = SelectedEntity::Marker(id);
                    state.marker_edit = state.markers.iter().find(|m| m.id == id).cloned();
                    state.building_edit = None;
                    state.line_edit = None;
                    state.text_edit = None;
                    state.anim_edit = None;
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;

                    // Optional quick Shift+Drag free move in Select mode (disabled if locked)
                    if state.tool_mode == ToolMode::Select
                        && !playback.play_test_mode
                        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
                        && let Some(marker) = state
                            .markers
                            .iter()
                            .find(|m| m.id == id && !state.is_marker_locked(m))
                            .cloned()
                        && let Some(drag) = begin_gizmo_drag(
                            &marker,
                            GizmoMode::Move,
                            GizmoHandle::Free,
                            cursor_px,
                            &state,
                            camera,
                            cam_gtf,
                        )
                    {
                        gizmo_state.active_drag = Some(drag);
                    }
                }
                Some(SelectedEntity::Building(id)) => {
                    state.selected = SelectedEntity::Building(id);
                    state.building_edit = state.buildings.iter().find(|b| b.id == id).cloned();
                    state.marker_edit = None;
                    state.line_edit = None;
                    state.text_edit = None;
                    state.anim_edit = None;
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                }
                Some(SelectedEntity::TextMarker(id)) => {
                    state.selected = SelectedEntity::TextMarker(id);
                    state.text_edit = state.text_markers.iter().find(|t| t.id == id).cloned();
                    state.marker_edit = None;
                    state.building_edit = None;
                    state.line_edit = None;
                    state.anim_edit = None;
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                }
                _ => {
                    // Clicked on empty map: deselects on clean click release (not pan drag)
                    if state.selected != SelectedEntity::None {
                        if mouse_buttons.just_released(MouseButton::Left) {
                            clear_selection(&mut state);
                            gizmo_state.hovered_handle = None;
                            gizmo_state.empty_click_start_px = None;
                        } else {
                            gizmo_state.empty_click_start_px = Some(cursor_px);
                        }
                    }
                }
            }
        }
        ToolMode::DrawLine
        | ToolMode::DrawBuilding
        | ToolMode::DrawBasicPolygon
        | ToolMode::DrawRoute => {
            if playback.play_test_mode {
                return;
            }
            // Click near Point #1 when drawing a polygon (>= 3 points) to close & extrude immediately!
            if matches!(state.tool_mode, ToolMode::DrawBasicPolygon | ToolMode::DrawBuilding)
                && state.draft_points_lnglat.len() >= 3
            {
                let p0 = state.draft_points_lnglat[0];
                let p0_local = state.geo_anchor.lng_lat_alt_to_local(p0[0], p0[1], 0.25);
                if let Ok(p0_2d) = camera.world_to_viewport(cam_gtf, Vec3::from_array(p0_local))
                    && cursor_px.distance(p0_2d) <= 16.0
                {
                    finish_active_draft(&mut state, &api);
                    return;
                }
            }
            let Some(hit) = hit_opt else {
                return;
            };
            let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
            state.draft_points_lnglat.push([lng, lat]);
        }
        ToolMode::PlaceText => {
            if playback.play_test_mode {
                return;
            }
            let Some(hit) = hit_opt else {
                return;
            };
            let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
            let Some(sk) = state.active_skenario.clone() else {
                return;
            };
            let label = if state.new_text_label.trim().is_empty() {
                "LABEL TAKTIS".to_string()
            } else {
                state.new_text_label.trim().to_string()
            };
            if state.offline_preview {
                let now = chrono::Utc::now();
                state.text_markers.push(TextMarker {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    text: label,
                    color: "#38bdf8".into(),
                    font_size: 14.0,
                    lng,
                    lat,
                    altitude: 12.0,
                    created_at: now,
                    updated_at: now,
                });
            } else if let Some(auth) = current_auth(&state) {
                api.send(ApiCommand::CreateTextMarker {
                    auth,
                    req: CreateTextMarkerRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        text: label,
                        color: "#38bdf8".into(),
                        font_size: 14.0,
                        lng,
                        lat,
                        altitude: 12.0,
                    },
                });
            }
        }
        ToolMode::Measure => {
            let Some(hit) = hit_opt else {
                return;
            };
            let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
            state.measure_points_lnglat.push([lng, lat]);
        }
    }
}

pub fn finish_active_draft(state: &mut AppState, api: &ApiClientBridge) {
    let Some(sk) = state.active_skenario.clone() else {
        state.draft_points_lnglat.clear();
        return;
    };

    let pts = std::mem::take(&mut state.draft_points_lnglat);
    match state.tool_mode {
        ToolMode::DrawLine => {
            if pts.len() < 2 {
                state.status_banner = Some("Line needs at least 2 points".into());
                return;
            }
            if state.offline_preview {
                let now = chrono::Utc::now();
                state.lines.push(twg_shared::models::PlotLine {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    name: format!("Line {}", state.lines.len() + 1),
                    color: state.new_line_color.clone(),
                    width: 3.0,
                    alpha: 1.0,
                    geom: pts,
                    created_at: now,
                    updated_at: now,
                });
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::CreateLine {
                    auth,
                    req: CreatePlotLineRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        name: format!("Line {}", state.lines.len() + 1),
                        color: state.new_line_color.clone(),
                        width: 3.0,
                        alpha: 1.0,
                        geom: pts,
                    },
                });
            }
            state.tool_mode = ToolMode::Select;
        }
        ToolMode::DrawBasicPolygon => {
            if pts.len() < 3 {
                state.status_banner = Some("Polygon 3D butuh minimal 3 titik (klik di peta).".into());
                return;
            }
            let local_pts: Vec<[f32; 2]> = pts
                .iter()
                .map(|p| {
                    let l = state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.0);
                    [l[0], l[2]]
                })
                .collect();
            let mut cx = 0.0_f32;
            let mut cz = 0.0_f32;
            for p in &local_pts {
                cx += p[0];
                cz += p[1];
            }
            cx /= local_pts.len() as f32;
            cz /= local_pts.len() as f32;

            let centered: Vec<[f32; 2]> = local_pts
                .into_iter()
                .map(|[x, z]| [x - cx, z - cz])
                .collect();
            let (c_lng, c_lat, _) = state.geo_anchor.local_to_lng_lat_alt([cx, 0.0, cz]);

            let mut cfg = BasicMeshConfig::default_for(BasicMeshShape::Polygon);
            cfg.height = (state.new_polygon_height as f32).clamp(0.1, 400.0);
            cfg.polygon_points = centered;

            if spawn_basic_mesh_marker(state, api, cfg, None, c_lng, c_lat) {
                state.status_banner = Some(
                    "Polygon 3D berhasil dibuat! Klik 'Edit Titik di Peta' di Inspector untuk mengubah/menambah titik."
                        .into(),
                );
            }
            state.tool_mode = ToolMode::Select;
        }
        ToolMode::DrawBuilding => {
            if pts.len() < 3 {
                state.status_banner = Some("Building polygon needs at least 3 vertices".into());
                return;
            }
            if state.offline_preview {
                let now = chrono::Utc::now();
                state.buildings.push(twg_shared::models::Building {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    name: format!("Building {}", state.buildings.len() + 1),
                    description: None,
                    color: state.new_building_color.clone(),
                    height: state.new_building_height,
                    geom: pts,
                    created_at: now,
                    updated_at: now,
                });
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::CreateBuilding {
                    auth,
                    req: CreateBuildingRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        name: format!("Building {}", state.buildings.len() + 1),
                        description: None,
                        color: state.new_building_color.clone(),
                        height: state.new_building_height,
                        geom: pts,
                    },
                });
            }
            state.tool_mode = ToolMode::Select;
        }
        ToolMode::DrawRoute => {
            if pts.len() < 2 {
                state.status_banner = Some("Rute animasi butuh minimal 2 titik waypoint.".into());
                return;
            }
            let target_marker_id = state.route_target_marker_id.or_else(|| {
                if let SelectedEntity::Marker(mid) = state.selected {
                    Some(mid)
                } else {
                    state.markers.first().map(|m| m.id)
                }
            });
            let target_alt = target_marker_id
                .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                .map(|m| m.altitude)
                .unwrap_or(0.0);
            let geom: Vec<[f64; 3]> = pts.into_iter().map(|[lng, lat]| [lng, lat, target_alt]).collect();
            let name = if state.new_route_name.trim().is_empty() {
                format!("Rute Animasi {}", state.anims.len() + 1)
            } else {
                state.new_route_name.trim().to_string()
            };
            let sort_order = (state.anims.len() + 1) as i32;
            let duration_sec = state.new_route_duration.max(1.0);
            let speed = state.new_route_speed.max(1.0);
            let replay = state.new_route_loop;

            if state.offline_preview {
                let now = chrono::Utc::now();
                let a = Anim {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    marker_id: target_marker_id,
                    name,
                    anim_type: "route".into(),
                    sort_order,
                    speed,
                    duration_sec,
                    replay,
                    autoplay: false,
                    geom,
                    data: serde_json::json!({}),
                    created_at: now,
                    updated_at: now,
                };
                state.selected = SelectedEntity::Anim(a.id);
                state.anim_edit = Some(a.clone());
                state.anims.push(a);
                state.status_banner = Some("Rute animasi ditambahkan! Tekan Play Skenario untuk menguji.".into());
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::CreateAnim {
                    auth,
                    req: CreateAnimRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: target_marker_id,
                        name,
                        anim_type: "route".into(),
                        sort_order,
                        speed,
                        duration_sec,
                        replay,
                        autoplay: false,
                        geom,
                        data: serde_json::json!({}),
                    },
                });
            }
            state.tool_mode = ToolMode::Select;
        }
        _ => {}
    }
}

pub fn current_auth(state: &AppState) -> Option<AuthCredentials> {
    let token = state.auth_token.as_ref()?.clone();
    Some(AuthCredentials {
        server_url: state.server_url.clone(),
        access_token: token,
        device_id: state.device_id.clone(),
    })
}

fn clear_selection(state: &mut AppState) {
    state.selected = SelectedEntity::None;
    state.marker_edit = None;
    state.building_edit = None;
    state.line_edit = None;
    state.text_edit = None;
    state.anim_edit = None;
}

/// Executes a confirmed deletion of any map entity (offline or via server API).
pub fn execute_delete_entity(
    state: &mut AppState,
    api: &ApiClientBridge,
    entity: SelectedEntity,
) {
    if state.is_entity_locked(entity) {
        return;
    }
    match entity {
        SelectedEntity::None => {}
        SelectedEntity::Marker(id) => {
            state.dirty_marker_ids.remove(&id);
            state.hidden_entity_ids.remove(&id);
            state.locked_entity_ids.remove(&id);
            if state.offline_preview {
                state.markers.retain(|m| m.id != id);
                if matches!(state.selected, SelectedEntity::Marker(s) if s == id) {
                    clear_selection(state);
                }
                state.status_banner = Some("Objek berhasil dihapus".into());
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::DeleteMarker { auth, id });
            }
        }
        SelectedEntity::Building(id) => {
            state.dirty_building_ids.remove(&id);
            state.hidden_entity_ids.remove(&id);
            state.locked_entity_ids.remove(&id);
            if state.offline_preview {
                state.buildings.retain(|b| b.id != id);
                if matches!(state.selected, SelectedEntity::Building(s) if s == id) {
                    clear_selection(state);
                }
                state.status_banner = Some("Building berhasil dihapus".into());
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::DeleteBuilding { auth, id });
            }
        }
        SelectedEntity::Line(id) => {
            state.dirty_line_ids.remove(&id);
            state.hidden_entity_ids.remove(&id);
            state.locked_entity_ids.remove(&id);
            if state.offline_preview {
                state.lines.retain(|l| l.id != id);
                if matches!(state.selected, SelectedEntity::Line(s) if s == id) {
                    clear_selection(state);
                }
                state.status_banner = Some("Plot Line berhasil dihapus".into());
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::DeleteLine { auth, id });
            }
        }
        SelectedEntity::TextMarker(id) => {
            state.dirty_text_ids.remove(&id);
            state.hidden_entity_ids.remove(&id);
            state.locked_entity_ids.remove(&id);
            if state.offline_preview {
                state.text_markers.retain(|t| t.id != id);
                if matches!(state.selected, SelectedEntity::TextMarker(s) if s == id) {
                    clear_selection(state);
                }
                state.status_banner = Some("Text Label berhasil dihapus".into());
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::DeleteTextMarker { auth, id });
            }
        }
        SelectedEntity::Anim(id) => {
            state.dirty_anim_ids.remove(&id);
            state.hidden_entity_ids.remove(&id);
            state.locked_entity_ids.remove(&id);
            if state.offline_preview {
                state.anims.retain(|a| a.id != id);
                if matches!(state.selected, SelectedEntity::Anim(s) if s == id) {
                    clear_selection(state);
                }
                state.status_banner = Some("Route Animation berhasil dihapus".into());
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::DeleteAnim { auth, id });
            }
        }
    }
}

/// Slab ray-AABB intersection in local space. Returns parametric hit distance `t >= 0` if hit.
pub fn intersect_ray_aabb(origin: Vec3, dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<f32> {
    let mut tmin = 0.0_f32;
    let mut tmax = f32::INFINITY;
    for i in 0..3 {
        let o = origin[i];
        let d = dir[i];
        let lo = aabb_min[i];
        let hi = aabb_max[i];
        if d.abs() < 1e-8 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let inv_d = 1.0 / d;
            let mut t1 = (lo - o) * inv_d;
            let mut t2 = (hi - o) * inv_d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmax < 0.0 { None } else { Some(tmin.max(0.0)) }
}

/// Double-sided Möller–Trumbore ray-triangle intersection in local space.
pub fn intersect_ray_triangle(
    origin: Vec3,
    dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<f32> {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = dir.cross(e2);
    let a = e1.dot(h);
    if a.abs() < 1e-7 {
        return None;
    }
    let f = 1.0 / a;
    let s = origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = f * dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * e2.dot(q);
    if t > 1e-5 { Some(t) } else { None }
}

fn intersect_ray_mesh_triangles(origin: Vec3, dir: Vec3, mesh: &Mesh) -> Option<f32> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };

    let mut best_t: Option<f32> = None;
    let mut test_tri = |i0: usize, i1: usize, i2: usize| {
        if i0 < positions.len() && i1 < positions.len() && i2 < positions.len() {
            let v0 = Vec3::from_array(positions[i0]);
            let v1 = Vec3::from_array(positions[i1]);
            let v2 = Vec3::from_array(positions[i2]);
            if let Some(t) = intersect_ray_triangle(origin, dir, v0, v1, v2)
                && best_t.is_none_or(|cur| t < cur)
            {
                best_t = Some(t);
            }
        }
    };

    match mesh.indices() {
        Some(Indices::U32(idx)) => {
            for tri in idx.as_chunks::<3>().0 {
                test_tri(tri[0] as usize, tri[1] as usize, tri[2] as usize);
            }
        }
        Some(Indices::U16(idx)) => {
            for tri in idx.as_chunks::<3>().0 {
                test_tri(tri[0] as usize, tri[1] as usize, tri[2] as usize);
            }
        }
        None => {
            for base in (0..positions.len()).step_by(3) {
                if base + 2 < positions.len() {
                    test_tri(base, base + 1, base + 2);
                }
            }
        }
    }

    best_t
}

/// Raycasts a single mesh node against `world_ray`.
/// Because `local_dir` is kept unnormalized, the returned `t` is the exact world-space distance.
fn raycast_mesh_node(
    world_ray: Ray3d,
    gtf: &GlobalTransform,
    aabb_opt: Option<&Aabb>,
    mesh3d_opt: Option<&Mesh3d>,
    meshes: &Assets<Mesh>,
    allow_aabb_fallback: bool,
) -> Option<f32> {
    let inv = gtf.affine().inverse();
    let local_origin = inv.transform_point3(world_ray.origin);
    let local_dir = inv.transform_vector3(world_ray.direction.as_vec3());
    if local_dir.length_squared() < 1e-12 {
        return None;
    }

    let aabb_hit = if let Some(aabb) = aabb_opt {
        let min = Vec3::from(aabb.center - aabb.half_extents);
        let max = Vec3::from(aabb.center + aabb.half_extents);
        let t = intersect_ray_aabb(local_origin, local_dir, min, max)?;
        Some(t)
    } else {
        None
    };

    if let Some(mesh3d) = mesh3d_opt
        && let Some(mesh) = meshes.get(&mesh3d.0)
        && let Some(t_tri) = intersect_ray_mesh_triangles(local_origin, local_dir, mesh)
    {
        return Some(t_tri);
    }

    if allow_aabb_fallback {
        aabb_hit
    } else {
        None
    }
}

/// Unified 3D entity picker supporting:
/// 1) Direct clicks on any 3D model mesh body (glTF / rigged characters / fallback primitives) & 3D buildings
/// 2) Clicks on projected floating unit label badges ("tulisan")
/// 3) Clicks on the ground highlight ring / pivot area ("highlight UI di peta")
#[allow(clippy::too_many_arguments)]
fn pick_scene_entity(
    state: &AppState,
    overrides: &PlaybackOverrides,
    marker_bounds: &MarkerWorldBounds,
    cursor_px: Vec2,
    ground_hit: Option<Vec3>,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
    pick: &ScenePickParams,
) -> Option<SelectedEntity> {
    // Priority 1: Exact 3D body raycast across all visible 3D markers and extruded buildings
    if let Ok(world_ray) = camera.viewport_to_world(cam_gtf, cursor_px) {
        let mut best_hit: Option<(SelectedEntity, f32)> = None;

        // 1a. 3D Markers (walk entire spawned glTF/mesh descendant hierarchy; skip locked)
        let mut stack: Vec<Entity> = Vec::with_capacity(16);
        for (root_entity, marker_comp, vis) in &pick.marker_roots {
            if *vis == Visibility::Hidden {
                continue;
            }
            if state
                .markers
                .iter()
                .find(|m| m.id == marker_comp.id)
                .is_some_and(|m| state.is_marker_locked(m))
            {
                continue;
            }
            stack.clear();
            stack.push(root_entity);
            while let Some(e) = stack.pop() {
                if let Ok((gtf, aabb_opt, mesh3d_opt)) = pick.mesh_nodes.get(e)
                    && (aabb_opt.is_some() || mesh3d_opt.is_some())
                    && let Some(t) = raycast_mesh_node(
                        world_ray,
                        gtf,
                        aabb_opt,
                        mesh3d_opt,
                        &pick.meshes,
                        true,
                    )
                    && best_hit.is_none_or(|cur| t < cur.1)
                {
                    best_hit = Some((SelectedEntity::Marker(marker_comp.id), t));
                }
                if let Ok(children) = pick.children.get(e) {
                    for child in children.iter() {
                        stack.push(child);
                    }
                }
            }
        }

        // 1b. 3D Extruded Buildings (exact triangle intersection; skip hidden or locked)
        for (entity, bldg_comp) in &pick.building_roots {
            if state.hidden_entity_ids.contains(&bldg_comp.id)
                || state.locked_entity_ids.contains(&bldg_comp.id)
            {
                continue;
            }
            if let Ok((gtf, aabb_opt, mesh3d_opt)) = pick.mesh_nodes.get(entity)
                && let Some(t) = raycast_mesh_node(
                    world_ray,
                    gtf,
                    aabb_opt,
                    mesh3d_opt,
                    &pick.meshes,
                    false,
                )
                && best_hit.is_none_or(|cur| t < cur.1)
            {
                best_hit = Some((SelectedEntity::Building(bldg_comp.id), t));
            }
        }

        if let Some((entity, _)) = best_hit {
            return Some(entity);
        }
    }

    // Priority 2: Click on floating unit label badge ("tulisan")
    if state.show_unit_labels {
        let mut best_label: Option<(Uuid, f32)> = None;
        for m in &state.markers {
            if !m.is_spawn || state.is_marker_locked(m) {
                continue;
            }
            let world_label_pos = if let Some(b) = marker_bounds.bounds.get(&m.id) {
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
            if let Ok(screen) = camera.world_to_viewport(cam_gtf, world_label_pos) {
                let half_w = (m.name.len() as f32 * 3.4 + 10.0).clamp(24.0, 140.0);
                let half_h = 11.5_f32;
                if (cursor_px.x - screen.x).abs() <= half_w
                    && (cursor_px.y - screen.y).abs() <= half_h
                {
                    let d = cursor_px.distance(screen);
                    if best_label.is_none_or(|cur| d < cur.1) {
                        best_label = Some((m.id, d));
                    }
                }
            }
        }
        if let Some((id, _)) = best_label {
            return Some(SelectedEntity::Marker(id));
        }
    }

    // Priority 2b: Click on projected tactical TextMarker badge
    if state.show_text_markers {
        let mut best_tm: Option<(Uuid, f32)> = None;
        for tm in &state.text_markers {
            if state.hidden_entity_ids.contains(&tm.id) || state.locked_entity_ids.contains(&tm.id)
            {
                continue;
            }
            let local = state
                .geo_anchor
                .lng_lat_alt_to_local(tm.lng, tm.lat, tm.altitude);
            if let Ok(screen) = camera.world_to_viewport(cam_gtf, Vec3::from_array(local)) {
                let half_w = (tm.text.len() as f32 * 3.8 + 10.0).clamp(24.0, 160.0);
                let half_h = (tm.font_size as f32 * 0.75 + 4.0).clamp(11.0, 22.0);
                if (cursor_px.x - screen.x).abs() <= half_w
                    && (cursor_px.y - screen.y).abs() <= half_h
                {
                    let d = cursor_px.distance(screen);
                    if best_tm.is_none_or(|cur| d < cur.1) {
                        best_tm = Some((tm.id, d));
                    }
                }
            }
        }
        if let Some((id, _)) = best_tm {
            return Some(SelectedEntity::TextMarker(id));
        }
    }

    // Priority 3: Click on ground highlight disc or very close to the unit's base pivot
    let mut best_ring: Option<(Uuid, f32)> = None;
    for m in &state.markers {
        if !m.is_spawn || state.is_marker_locked(m) {
            continue;
        }
        let (center_3d, ring_r) = if let Some(b) = marker_bounds.bounds.get(&m.id) {
            (b.footprint_center, b.footprint_radius)
        } else {
            let (lng, lat, alt) = if let Some(ov) = overrides.markers.get(&m.id) {
                (ov.lng, ov.lat, ov.altitude)
            } else {
                (m.lng, m.lat, m.altitude)
            };
            let p = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
            (
                Vec3::from_array(p),
                (m.scale_x.max(m.scale_z) as f32 * 2.2).max(0.75),
            )
        };

        let hit_ground_ring = ground_hit.is_some_and(|hit| {
            Vec2::new(center_3d.x - hit.x, center_3d.z - hit.z).length() <= ring_r
        });
        let hit_pivot_px = camera
            .world_to_viewport(cam_gtf, center_3d)
            .ok()
            .filter(|scr| cursor_px.distance(*scr) <= 16.0);

        if hit_ground_ring || hit_pivot_px.is_some() {
            let d = hit_pivot_px
                .map(|scr| cursor_px.distance(scr))
                .unwrap_or(0.0);
            if best_ring.is_none_or(|cur| d < cur.1) {
                best_ring = Some((m.id, d));
            }
        }
    }
    best_ring.map(|(id, _)| SelectedEntity::Marker(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_aabb_hits_and_misses_accurately() {
        let hit = intersect_ray_aabb(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::NEG_Z,
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        assert!(hit.is_some_and(|t| (t - 4.0).abs() < 1e-4));

        let miss = intersect_ray_aabb(
            Vec3::new(2.5, 0.0, 5.0),
            Vec3::NEG_Z,
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        assert!(miss.is_none());
    }

    #[test]
    fn ray_triangle_hits_center_and_misses_outside() {
        let v0 = Vec3::new(-1.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 2.0, 0.0);

        let hit = intersect_ray_triangle(Vec3::new(0.0, 0.8, 4.0), Vec3::NEG_Z, v0, v1, v2);
        assert!(hit.is_some_and(|t| (t - 4.0).abs() < 1e-4));

        let miss = intersect_ray_triangle(Vec3::new(1.5, 0.8, 4.0), Vec3::NEG_Z, v0, v1, v2);
        assert!(miss.is_none());
    }
}
