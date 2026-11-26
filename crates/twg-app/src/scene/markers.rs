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
    particle_anim::{
        ParticleAnimConfig, is_particle_anim, particle_anim_position,
    },
    pointer_anim::{ANIM_TYPE_POINTER_ARROW, PointerArrowConfig, is_pointer_arrow_anim},
    route_formation::{
        ANIM_TYPE_ROUTE_MULTI, ANIM_TYPE_ROUTE_SINGLE, MultiRouteConfig, RouteFormationKind,
        anim_bound_marker_ids, is_multi_route_anim, parse_multi_route_config,
        write_multi_route_config,
    },
    shoot_anim::{
        AparSprayConfig, BulletShootConfig, ShootAnimKind, StoneThrowConfig, TearGasConfig,
        WaterCannonConfig, shoot_anim_kind,
    },
    spawn_hide_anim::{
        SpawnHideAction, SpawnHideConfig, parse_spawn_hide_config, spawn_hide_action,
        write_spawn_hide_config,
    },
    traffic_anim::{
        ANIM_TYPE_TRAFFIC_FLOW, TrafficAnimConfig, TrafficLane, TrafficVehicleEntry,
        is_traffic_anim, parse_traffic_anim_config, write_traffic_anim_config,
    },
};
use uuid::Uuid;

use crate::{
    map::camera::cursor_world_ground,
    net::{
        client::{ApiClientBridge, ApiCommand, AuthCredentials},
        ws_client::WsClientBridge,
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
            begin_gizmo_drag, begin_particle_move_drag, begin_text_marker_move_drag,
            begin_traffic_preview_scale_drag, compute_gizmo_world_radius,
            gizmo_mode_for_tool, hit_test_gizmo, marker_quat, marker_world_origin,
            particle_anim_world_origin, update_gizmo_drag,
        },
        play_rts::PlayRtsState,
        playback::{PlaybackOverrides, route_estimated_duration_sec},
        plot_line_edit::{
            PlotLineEditState, PlotLineGizmoHandle, PlotLineHoverTarget, begin_plot_line_drag,
            hit_test_any_plot_line, hit_test_plot_line_editor, insert_plot_line_midpoint,
            remove_plot_line_point, update_plot_line_drag,
        },
        polygon_edit::{
            PolygonEditState, PolygonHoverTarget, PolygonVertexGizmoHandle, begin_vertex_drag,
            ensure_editable_polygon_points, hit_test_polygon_editor, insert_midpoint_after,
            insert_vertex_from_cursor_click, update_vertex_drag,
        },
        route_edit::{
            RouteEditState, RouteHoverTarget, RouteWaypointGizmoHandle,
            append_waypoint_from_cursor_click, begin_waypoint_drag, editable_anim_geom,
            hit_test_any_route_line, hit_test_route_editor, hit_test_traffic_lane_index,
            insert_route_midpoint_after, remove_route_waypoint, update_waypoint_drag,
            write_editable_anim_geom,
        },
        traffic::resolve_active_traffic_scale_gizmo_pose,
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightDiscKind {
    Selected,
    Hover,
    RouteBound,
    PickCandidate,
}

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

#[derive(Clone)]
pub struct SubtreeAnimGraph {
    pub handle: Handle<AnimationGraph>,
    pub idle_node: AnimationNodeIndex,
    pub walk_node: AnimationNodeIndex,
}

#[derive(Component)]
pub struct TacticalMarkerEntity {
    pub id: Uuid,
    pub model_path: String,
    pub gltf_handle: Option<Handle<Gltf>>,
    pub idle_child: Option<Entity>,
    pub idle_anim_graph: Option<SubtreeAnimGraph>,
    pub walk_model_path: Option<String>,
    pub walk_gltf_handle: Option<Handle<Gltf>>,
    pub walk_child: Option<Entity>,
    pub walk_anim_graph: Option<SubtreeAnimGraph>,
    pub is_walking_now: bool,
    pub basic_mesh_child: Option<Entity>,
    pub basic_geom_sig: u64,
    pub basic_mat_sig: u64,
    pub basic_mat_handle: Option<StandardMaterialHandleAlias>,
}

type StandardMaterialHandleAlias = Handle<StandardMaterial>;

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
            HighlightDiscKind::Selected,
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
                Mesh3d(inner_ring.clone()),
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
        .spawn((
            Transform::IDENTITY,
            Visibility::Hidden,
            HighlightDiscKind::Hover,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(fill_disc.clone()),
                MeshMaterial3d(hov_fill_mat),
                Transform::from_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(outer_ring.clone()),
                MeshMaterial3d(hov_ring_mat),
                Transform::from_xyz(0.0, 0.012, 0.0).with_rotation(flat_rot),
            ));
        });

    // 3. Route-Bound 3D Marker highlight: Warm Amber / Gold tactical target disc (distinct from Emerald Select!)
    let bound_fill_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.98, 0.62, 0.08, 0.24),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let bound_outer_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.72, 0.12, 0.96),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let bound_inner_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.88, 0.36, 0.65),
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
            HighlightDiscKind::RouteBound,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(fill_disc.clone()),
                MeshMaterial3d(bound_fill_mat),
                Transform::from_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(outer_ring.clone()),
                MeshMaterial3d(bound_outer_mat),
                Transform::from_xyz(0.0, 0.018, 0.0).with_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(inner_ring.clone()),
                MeshMaterial3d(bound_inner_mat),
                Transform::from_xyz(0.0, 0.015, 0.0).with_rotation(flat_rot),
            ));
        });

    // 4. Pick-Unit Candidate hover highlight: Vivid Violet / Magenta target-acquire disc
    let pick_fill_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.74, 0.24, 0.98, 0.28),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let pick_outer_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.92, 0.42, 1.0, 0.98),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let pick_inner_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.66, 0.96, 0.72),
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
            HighlightDiscKind::PickCandidate,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(fill_disc),
                MeshMaterial3d(pick_fill_mat),
                Transform::from_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(outer_ring),
                MeshMaterial3d(pick_outer_mat),
                Transform::from_xyz(0.0, 0.020, 0.0).with_rotation(flat_rot),
            ));
            parent.spawn((
                Mesh3d(inner_ring),
                MeshMaterial3d(pick_inner_mat),
                Transform::from_xyz(0.0, 0.017, 0.0).with_rotation(flat_rot),
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
    if state.saving_pending_count > 0 {
        return;
    }
    if state.offline_preview {
        let count = state.unsaved_change_count();
        state.remember_offline_plot();
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

    let marker_ids: Vec<Uuid> = state.dirty_marker_ids.iter().copied().collect();
    let building_ids: Vec<Uuid> = state.dirty_building_ids.iter().copied().collect();
    let line_ids: Vec<Uuid> = state.dirty_line_ids.iter().copied().collect();
    let text_ids: Vec<Uuid> = state.dirty_text_ids.iter().copied().collect();
    let anim_ids: Vec<Uuid> = state.dirty_anim_ids.iter().copied().collect();
    let save_skenario = state.skenario_dirty;

    let mut sent = 0usize;

    for id in marker_ids {
        if let Some(m) = state.markers.iter().find(|x| x.id == id).cloned() {
            api.send_plot_save(ApiCommand::UpdateMarker {
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
            let _ = ws;
            sent += 1;
        }
    }

    for id in building_ids {
        if let Some(b) = state.buildings.iter().find(|x| x.id == id).cloned() {
            api.send_plot_save(ApiCommand::UpdateBuilding {
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
            api.send_plot_save(ApiCommand::UpdateLine {
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
            api.send_plot_save(ApiCommand::UpdateTextMarker {
                auth: auth.clone(),
                id: t.id,
                req: UpdateTextMarkerRequest {
                    text: t.text,
                    color: t.color,
                    font_size: t.font_size,
                    show_background: t.show_background,
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
            api.send_plot_save(ApiCommand::UpdateAnim {
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
        && state.can_write_admin()
        && let Some(sk) = state.active_skenario.clone()
    {
        api.send_plot_save(ApiCommand::UpdateSkenario {
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
    } else if state.unsaved_change_count() > 0 {
        state.status_banner = Some("Perubahan belum dapat disimpan. Periksa akses edit skenario.".into());
    } else {
        state.status_banner = Some("Semua perubahan skenario sudah tersimpan.".into());
    }
}

fn normalize_rel_path(p: &str) -> String {
    p.trim().replace('\\', "/").trim_start_matches('/').to_string()
}

fn resolve_walk_model_path(state: &AppState, marker: &Marker) -> Option<String> {
    let exists_locally = |p: &str| {
        let norm = normalize_rel_path(p);
        state.available_model_paths.contains(p)
            || state.available_model_paths.contains(&norm)
            || state.assets_root.join(&norm).exists()
    };

    let norm_marker_path = normalize_rel_path(&marker.model_path);

    if let Some(entry) = state.catalog.iter().find(|c| {
        normalize_rel_path(&c.model_path).eq_ignore_ascii_case(&norm_marker_path)
            || c.id.eq_ignore_ascii_case(&marker.unit_id)
    }) && let Some(walk_path) = &entry.walk_model_path
        && exists_locally(walk_path)
    {
        return Some(normalize_rel_path(walk_path));
    }

    if let Some(asset) = state.model_assets.iter().find(|a| {
        normalize_rel_path(&a.model_file.rel_path).eq_ignore_ascii_case(&norm_marker_path)
            || a.lod2_file.as_ref().is_some_and(|lf| {
                normalize_rel_path(&lf.rel_path).eq_ignore_ascii_case(&norm_marker_path)
            })
            || a.id.to_string().eq_ignore_ascii_case(&marker.unit_id)
    }) {
        if let Some(wf) = &asset.walk_file
            && exists_locally(&wf.rel_path)
        {
            return Some(normalize_rel_path(&wf.rel_path));
        }
        if let Some(wl) = &asset.walk_lod2_file
            && exists_locally(&wl.rel_path)
        {
            return Some(normalize_rel_path(&wl.rel_path));
        }
    }

    let lower = norm_marker_path.to_lowercase();
    if lower.ends_with("_idle.glb") {
        let stem = &norm_marker_path[..norm_marker_path.len() - "_idle.glb".len()];
        let candidate = format!("{stem}_walk.glb");
        if exists_locally(&candidate) {
            return Some(candidate);
        }
    } else if lower.ends_with("_idle_lod2.glb") {
        let stem = &norm_marker_path[..norm_marker_path.len() - "_idle_lod2.glb".len()];
        for candidate in [
            format!("{stem}_walk_lod2.glb"),
            format!("{stem}_walk.glb"),
        ] {
            if exists_locally(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
    mut child_vis_q: Query<&mut Visibility, Without<TacticalMarkerEntity>>,
) {
    if !state.is_3d_map_screen() {
        for (entity, _, _, _) in &existing_q {
            commands.entity(entity).despawn();
        }
        return;
    }

    let wanted: HashMap<Uuid, (&Marker, Option<String>, bool)> = state
        .markers
        .iter()
        .map(|m| {
            let is_moving = playback.is_playback_active()
                && overrides.markers.get(&m.id).is_some_and(|ov| ov.moving);
            let walk_path = if parse_basic_mesh_config(m).is_some() {
                None
            } else {
                resolve_walk_model_path(&state, m)
            };
            (m.id, (m, walk_path, is_moving))
        })
        .collect();
    let mut present: HashSet<Uuid> = HashSet::new();

    for (entity, mut comp, mut tf, mut vis) in &mut existing_q {
        let Some((marker, wanted_walk, is_moving)) = wanted.get(&comp.id) else {
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
            comp.model_path = marker.model_path.clone();
            comp.is_walking_now = *is_moving;

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
            if comp.basic_mesh_child.is_some()
                || comp.model_path != marker.model_path
                || comp.walk_model_path != *wanted_walk
            {
                commands.entity(entity).despawn();
                continue;
            }
            present.insert(comp.id);
            comp.is_walking_now = *is_moving;

            // Zero-flicker Walk <-> Idle visibility switch when marker has a walk animation model
            let show_walk = *is_moving && comp.walk_child.is_some();
            if let Some(idle_e) = comp.idle_child
                && let Ok(mut iv) = child_vis_q.get_mut(idle_e)
            {
                *iv = if show_walk {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
            }
            if let Some(walk_e) = comp.walk_child
                && let Ok(mut wv) = child_vis_q.get_mut(walk_e)
            {
                *wv = if show_walk {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }

        let (lng, lat, base_alt, rot_y) = if let Some(ov) = overrides.markers.get(&marker.id) {
            (ov.lng, ov.lat, ov.altitude, ov.rot_y)
        } else {
            (marker.lng, marker.lat, marker.altitude, marker.rot_y)
        };
        let vis_ov = overrides.visibility.get(&marker.id).copied();
        let is_visible = vis_ov.map(|v| v.visible).unwrap_or(marker.is_spawn);
        let scale_mult = vis_ov
            .map(|v| v.scale_factor.max(0.001))
            .unwrap_or(1.0);
        let alt = base_alt + vis_ov.map(|v| v.altitude_offset_m).unwrap_or(0.0);

        let local = state.geo_anchor.lng_lat_alt_to_local(lng, lat, alt);
        tf.translation = Vec3::from_array(local);
        tf.rotation = Quat::from_euler(
            EulerRot::YXZ,
            (rot_y as f32).to_radians(),
            (marker.rot_x as f32).to_radians(),
            (marker.rot_z as f32).to_radians(),
        );
        tf.scale = Vec3::new(
            (marker.scale_x as f32).max(MIN_OBJECT_3D_SCALE as f32) * scale_mult,
            (marker.scale_y as f32).max(MIN_OBJECT_3D_SCALE as f32) * scale_mult,
            (marker.scale_z as f32).max(MIN_OBJECT_3D_SCALE as f32) * scale_mult,
        );
        *vis = if is_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    for marker in &state.markers {
        if present.contains(&marker.id) {
            continue;
        }

        let Some((_, walk_path_opt, is_moving)) = wanted.get(&marker.id) else {
            continue;
        };
        let walk_path_opt = walk_path_opt.clone();
        let is_moving = *is_moving;
        let render_path = marker.model_path.clone();

        let (lng, lat, base_alt, rot_y) = if let Some(ov) = overrides.markers.get(&marker.id) {
            (ov.lng, ov.lat, ov.altitude, ov.rot_y)
        } else {
            (marker.lng, marker.lat, marker.altitude, marker.rot_y)
        };
        let vis_ov = overrides.visibility.get(&marker.id).copied();
        let is_visible = vis_ov.map(|v| v.visible).unwrap_or(marker.is_spawn);
        let scale_mult = vis_ov
            .map(|v| v.scale_factor.max(0.001))
            .unwrap_or(1.0);
        let alt = base_alt + vis_ov.map(|v| v.altitude_offset_m).unwrap_or(0.0);

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
                (marker.scale_x as f32).max(MIN_OBJECT_3D_SCALE as f32) * scale_mult,
                (marker.scale_y as f32).max(MIN_OBJECT_3D_SCALE as f32) * scale_mult,
                (marker.scale_z as f32).max(MIN_OBJECT_3D_SCALE as f32) * scale_mult,
            ),
        };

        let vis = if is_visible {
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
                        idle_child: None,
                        idle_anim_graph: None,
                        walk_model_path: None,
                        walk_gltf_handle: None,
                        walk_child: None,
                        walk_anim_graph: None,
                        is_walking_now: is_moving,
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

        // Case 2: glTF 3D Model (with optional preloaded Walk sibling for instant Idle <-> Walk swap)
        let has_glb = state.available_model_paths.contains(&render_path)
            || state.assets_root.join(&render_path).exists();

        let gltf_handle: Option<Handle<Gltf>> = if has_glb {
            Some(asset_server.load(render_path.clone()))
        } else {
            None
        };

        let walk_gltf_handle: Option<Handle<Gltf>> = walk_path_opt
            .as_ref()
            .map(|wp| asset_server.load(wp.clone()));

        let show_walk = is_moving && walk_path_opt.is_some();

        let idle_child = if has_glb {
            let scene_handle =
                asset_server.load(GltfAssetLabel::Scene(0).from_asset(render_path.clone()));
            Some(
                commands
                    .spawn((
                        WorldAssetRoot(scene_handle),
                        Transform::IDENTITY,
                        if show_walk {
                            Visibility::Hidden
                        } else {
                            Visibility::Inherited
                        },
                    ))
                    .id(),
            )
        } else {
            None
        };

        let walk_child = walk_path_opt.as_ref().map(|wp| {
            let walk_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(wp.clone()));
            commands
                .spawn((
                    WorldAssetRoot(walk_scene),
                    Transform::IDENTITY,
                    if show_walk {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                ))
                .id()
        });

        let parent = commands
            .spawn((
                root_tf,
                vis,
                TacticalMarkerEntity {
                    id: marker.id,
                    model_path: render_path,
                    gltf_handle,
                    idle_child,
                    idle_anim_graph: None,
                    walk_model_path: walk_path_opt,
                    walk_gltf_handle,
                    walk_child,
                    walk_anim_graph: None,
                    is_walking_now: is_moving,
                    basic_mesh_child: None,
                    basic_geom_sig: 0,
                    basic_mat_sig: 0,
                    basic_mat_handle: None,
                },
            ))
            .id();

        if let Some(ic) = idle_child {
            commands.entity(parent).add_child(ic);
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
                    Transform::from_xyz(0.0, 0.9, 1.2)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            });
        }
        if let Some(wc) = walk_child {
            commands.entity(parent).add_child(wc);
        }
    }
}

/// Selects `(idle_clip, walk_clip)` from a loaded glTF asset.
/// Supports both:
/// - Dual-clip single-file models (e.g. vehicles/helicopters `*_anim_1.glb` containing `Idle|Idle` + `Idle|Walk`)
/// - Single-clip files (e.g. humanoid `*_idle.glb` / `*_walk.glb` containing 1 clip).
fn select_idle_and_walk_clips(gltf: &Gltf) -> Option<(Handle<AnimationClip>, Handle<AnimationClip>)> {
    if gltf.animations.is_empty() {
        return None;
    }
    if gltf.animations.len() == 1 {
        let only = gltf.animations[0].clone();
        return Some((only.clone(), only));
    }

    let mut named: Vec<(&str, &Handle<AnimationClip>)> = gltf
        .named_animations
        .iter()
        .map(|(k, v)| (k.as_ref(), v))
        .collect();
    named.sort_by_key(|(k, _)| *k);

    let is_walk_name = |raw: &str| {
        let lower = raw.to_lowercase();
        let tail = lower.rsplit('|').next().unwrap_or(&lower);
        tail.contains("walk")
            || tail.contains("run")
            || tail.contains("move")
            || tail.contains("drive")
            || tail.contains("fly")
            || lower.contains("walk")
    };
    let is_idle_name = |raw: &str| {
        let lower = raw.to_lowercase();
        let tail = lower.rsplit('|').next().unwrap_or(&lower);
        !is_walk_name(raw) && (tail.contains("idle") || tail.contains("stand") || tail.contains("rest"))
    };

    let walk_clip = named
        .iter()
        .find(|(name, _)| is_walk_name(name))
        .map(|(_, h)| (*h).clone())
        .unwrap_or_else(|| gltf.animations[1].clone());

    let idle_clip = named
        .iter()
        .find(|(name, _)| is_idle_name(name))
        .map(|(_, h)| (*h).clone())
        .or_else(|| {
            gltf.animations
                .iter()
                .find(|h| h.id() != walk_clip.id())
                .cloned()
        })
        .unwrap_or_else(|| gltf.animations[0].clone());

    Some((idle_clip, walk_clip))
}

#[allow(clippy::too_many_arguments)]
fn drive_subtree_gltf_anim(
    commands: &mut Commands,
    subtree_root: Entity,
    gltf_handle: &Handle<Gltf>,
    graph_slot: &mut Option<SubtreeAnimGraph>,
    prefer_walk_clip: bool,
    should_play: bool,
    speed: f32,
    gltfs: &Assets<Gltf>,
    anim_graphs: &mut Assets<AnimationGraph>,
    children_q: &Query<&Children>,
    anim_player_q: &mut Query<(Entity, &mut AnimationPlayer, Option<&AnimationGraphHandle>)>,
) {
    let Some(gltf) = gltfs.get(gltf_handle) else {
        return;
    };
    if gltf.animations.is_empty() {
        return;
    }

    let mut stack = vec![subtree_root];
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
        return;
    }

    // Lazily build the AnimationGraph once per subtree (supporting both 1-clip and Idle+Walk dual-clip GLBs)
    let anim_graph = if let Some(existing) = graph_slot.as_ref() {
        existing.clone()
    } else {
        let Some((idle_clip, walk_clip)) = select_idle_and_walk_clips(gltf) else {
            return;
        };
        let built = if idle_clip.id() == walk_clip.id() {
            let (graph, node) = AnimationGraph::from_clip(idle_clip);
            SubtreeAnimGraph {
                handle: anim_graphs.add(graph),
                idle_node: node,
                walk_node: node,
            }
        } else {
            let (graph, indices) = AnimationGraph::from_clips([idle_clip, walk_clip]);
            SubtreeAnimGraph {
                handle: anim_graphs.add(graph),
                idle_node: indices[0],
                walk_node: indices[1],
            }
        };
        *graph_slot = Some(built.clone());
        built
    };

    let target_node = if prefer_walk_clip {
        anim_graph.walk_node
    } else {
        anim_graph.idle_node
    };

    // Self-healing per-AnimationPlayer attachment & clip switching so late-spawned or multi-player
    // GLB hierarchies always play the exact right clip (Walk vs Idle).
    for pe in &player_entities {
        let Ok((_, mut player, existing_graph_h)) = anim_player_q.get_mut(*pe) else {
            continue;
        };
        let needs_graph = existing_graph_h.is_none_or(|gh| gh.0.id() != anim_graph.handle.id());
        if needs_graph {
            commands
                .entity(*pe)
                .insert(AnimationGraphHandle(anim_graph.handle.clone()));
        }

        let needs_switch = !player.is_playing_animation(target_node)
            || player.playing_animations().count() != 1;
        if needs_graph || needs_switch {
            player.stop_all();
            let active = player.play(target_node).repeat();
            active.set_speed(speed);
            if !should_play {
                active.pause();
            }
        } else if let Some(active) = player.animation_mut(target_node) {
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

/// Drives embedded glTF AnimationPlayers independently per marker:
/// - Markers with separate Idle + Walk `.glb` files play Walk ONLY while moving and Idle when resting.
/// - Single-file dual-clip animated markers (e.g. `*_anim_1.glb` vehicles/helicopters with `Idle|Idle`
///   and `Idle|Walk`) switch cleanly to their Walk clip while moving along any route and Idle at rest.
#[allow(clippy::too_many_arguments)]
pub fn animate_markers_system(
    mut commands: Commands,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    gltfs: Res<Assets<Gltf>>,
    mut anim_graphs: ResMut<Assets<AnimationGraph>>,
    mut marker_q: Query<(Entity, &mut TacticalMarkerEntity)>,
    children_q: Query<&Children>,
    mut anim_player_q: Query<(Entity, &mut AnimationPlayer, Option<&AnimationGraphHandle>)>,
) {
    if !state.is_3d_map_screen() {
        return;
    }

    let speed = playback.speed_multiplier.max(0.1);
    let playback_active = playback.is_playback_active();

    for (root_entity, mut comp) in &mut marker_q {
        let has_walk_variant = comp.walk_child.is_some() && comp.walk_gltf_handle.is_some();

        // 1. Dedicated Walk subtree (`*_walk.glb`): plays ONLY while actively moving on a route
        if let (Some(walk_root), Some(walk_gltf)) = (comp.walk_child, comp.walk_gltf_handle.clone())
        {
            let play_walk = playback_active && comp.is_walking_now;
            drive_subtree_gltf_anim(
                &mut commands,
                walk_root,
                &walk_gltf,
                &mut comp.walk_anim_graph,
                true,
                play_walk,
                speed,
                &gltfs,
                &mut anim_graphs,
                &children_q,
                &mut anim_player_q,
            );
        }

        // 2. Primary / Idle subtree (also drives dual-clip single-file animated vehicles `*_anim_1.glb`)
        if let Some(idle_gltf) = comp.gltf_handle.clone() {
            let idle_root = comp.idle_child.unwrap_or(root_entity);
            let is_dual_clip = !has_walk_variant
                && gltfs
                    .get(&idle_gltf)
                    .is_some_and(|g| g.animations.len() >= 2);
            let prefer_walk = !has_walk_variant && comp.is_walking_now;
            let play_primary = if has_walk_variant {
                playback_active && !comp.is_walking_now
            } else if is_dual_clip {
                // Play Walk clip while moving, Idle clip while resting in Play Skenario / Preview
                playback_active
            } else {
                playback_active && comp.is_walking_now
            };
            drive_subtree_gltf_anim(
                &mut commands,
                idle_root,
                &idle_gltf,
                &mut comp.idle_anim_graph,
                prefer_walk,
                play_primary,
                speed,
                &gltfs,
                &mut anim_graphs,
                &children_q,
                &mut anim_player_q,
            );
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn selection_gizmo_system(
    time: Res<Time>,
    state: Res<AppState>,
    playback: Res<PlaybackState>,
    overrides: Res<PlaybackOverrides>,
    play_rts: Res<PlayRtsState>,
    gizmo_state: Res<TransformGizmoState>,
    mut marker_bounds: ResMut<MarkerWorldBounds>,
    marker_roots_q: Query<(Entity, &TacticalMarkerEntity, &Visibility)>,
    children_q: Query<&Children>,
    mesh_aabb_q: Query<(&GlobalTransform, &Aabb)>,
    mut disc_q: Query<
        (&HighlightDiscKind, &mut Transform, &mut Visibility),
        Without<TacticalMarkerEntity>,
    >,
    mut gizmos: Gizmos,
) {
    let hide_all_discs = |dq: &mut Query<
        (&HighlightDiscKind, &mut Transform, &mut Visibility),
        Without<TacticalMarkerEntity>,
    >| {
        for (_, _, mut vis) in dq.iter_mut() {
            *vis = Visibility::Hidden;
        }
    };

    if !state.is_3d_map_screen() {
        marker_bounds.bounds.clear();
        hide_all_discs(&mut disc_q);
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

        let hidden_branch = if comp.is_walking_now && comp.walk_child.is_some() {
            comp.idle_child
        } else {
            comp.walk_child
        };
        stack.clear();
        stack.push(root_entity);
        while let Some(e) = stack.pop() {
            if Some(e) == hidden_branch {
                continue;
            }
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

    // Play Skenario Mode: render crisp RTS selection rings for selected units,
    // selected `GizmoMove` (`tools`) object, and hovered selectable markers.
    if playback.play_test_mode {
        let elapsed = time.elapsed_secs();
        let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let pulse = 1.0 + 0.03 * (elapsed * 4.2).sin();

        let play_footprint = |m: &Marker| -> (Vec3, f32) {
            if let Some(b) = marker_bounds.bounds.get(&m.id) {
                (b.footprint_center, b.footprint_radius)
            } else {
                let p = play_rts.marker_runtime_world_origin(&state, &overrides, m);
                (
                    Vec3::new(p.x, 0.06, p.z),
                    (m.scale_x.max(m.scale_z) as f32 * 2.2).max(0.75),
                )
            }
        };

        let mut sel_disc_pose: Option<(Vec3, f32)> = None;
        let mut hov_disc_pose: Option<(Vec3, f32)> = None;

        for &uid in &play_rts.selected_unit_ids {
            let Some(m) = state.markers.iter().find(|x| x.id == uid) else {
                continue;
            };
            if !overrides.is_marker_visible(m, true) {
                continue;
            }
            let (center, r) = play_footprint(m);
            let ring_y = center.y.max(0.08);
            let c3 = Vec3::new(center.x, ring_y, center.z);
            gizmos.circle(
                Isometry3d::new(c3, flat_rot),
                r * pulse,
                Color::srgba(0.14, 0.98, 0.66, 0.96),
            );
            gizmos.circle(
                Isometry3d::new(c3 + Vec3::Y * 0.02, flat_rot),
                (r * 0.84) * pulse,
                Color::srgba(0.24, 0.94, 0.88, 0.52),
            );
            if play_rts.selected_unit_ids.len() == 1 {
                sel_disc_pose = Some((center, r));
            }
        }

        if let Some(gid) = play_rts.selected_gizmo_marker_id
            && let Some(m) = state.markers.iter().find(|x| x.id == gid)
            && overrides.is_marker_visible(m, true)
        {
            let (center, r) = play_footprint(m);
            let c3 = Vec3::new(center.x, center.y.max(0.08), center.z);
            gizmos.circle(
                Isometry3d::new(c3, flat_rot),
                r * pulse,
                Color::srgba(0.98, 0.78, 0.16, 0.96),
            );
            gizmos.circle(
                Isometry3d::new(c3 + Vec3::Y * 0.02, flat_rot),
                r * 0.80,
                Color::srgba(0.22, 0.88, 0.98, 0.60),
            );
            sel_disc_pose = Some((center, r));
        }

        if let Some(hid) = play_rts.hovered_marker_id
            && !play_rts.selected_unit_ids.contains(&hid)
            && play_rts.selected_gizmo_marker_id != Some(hid)
            && let Some(m) = state.markers.iter().find(|x| x.id == hid)
            && overrides.is_marker_visible(m, true)
        {
            hov_disc_pose = Some(play_footprint(m));
        }

        for (kind, mut tf, mut vis) in &mut disc_q {
            let pose = match kind {
                HighlightDiscKind::Selected => sel_disc_pose,
                HighlightDiscKind::Hover => hov_disc_pose,
                HighlightDiscKind::RouteBound | HighlightDiscKind::PickCandidate => None,
            };
            if let Some((center, radius)) = pose {
                tf.translation = center;
                tf.rotation = Quat::IDENTITY;
                tf.scale = Vec3::splat(radius);
                *vis = Visibility::Visible;
            } else {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }

    if !state.show_gizmos || (playback.play_test_mode && playback.hide_gizmos_in_play) {
        hide_all_discs(&mut disc_q);
        return;
    }

    let marker_footprint = |m: &Marker| -> (Vec3, f32, Option<(Vec3, Vec3)>) {
        if let Some(b) = marker_bounds.bounds.get(&m.id).copied() {
            (b.footprint_center, b.footprint_radius, Some((b.world_min, b.world_max)))
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
                None,
            )
        }
    };

    let elapsed = time.elapsed_secs();
    let is_pick_route_mode = !playback.play_test_mode
        && matches!(
            state.tool_mode,
            ToolMode::PickRouteMarker
                | ToolMode::PickShootMarker
                | ToolMode::PickPointerMarker
                | ToolMode::PickSpawnHideMarker
        );

    // Resolve active Animation whose bound marker(s) should be highlighted in Warm Amber/Gold
    // (or Emerald/Rose for Spawn/Hide): either currently being picked, or currently selected!
    let active_route_anim = if !playback.play_test_mode
        && state.tool_mode == ToolMode::PickRouteMarker
    {
        state
            .picking_marker_for_anim_id
            .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
    } else if !playback.play_test_mode && state.tool_mode == ToolMode::PickShootMarker {
        state
            .picking_shoot_for_anim_id
            .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
    } else if !playback.play_test_mode && state.tool_mode == ToolMode::PickPointerMarker {
        state
            .picking_pointer_for_anim_id
            .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
    } else if !playback.play_test_mode && state.tool_mode == ToolMode::PickSpawnHideMarker {
        state
            .picking_spawn_hide_for_anim_id
            .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
    } else if !playback.play_test_mode
        && let SelectedEntity::Anim(aid) = state.selected
    {
        state.anims.iter().find(|a| a.id == aid)
    } else {
        None
    };

    let route_start_world = active_route_anim.and_then(|a| {
        a.geom.first().map(|p| {
            Vec3::from_array(
                state
                    .geo_anchor
                    .lng_lat_alt_to_local(p[0], p[1], p[2].max(0.45)),
            )
        })
    });

    // Compute desired disc transforms per HighlightDiscKind
    let mut sel_pose: Option<(Vec3, f32)> = None;
    let mut hov_pose: Option<(Vec3, f32)> = None;
    let mut bound_pose: Option<(Vec3, f32)> = None;
    let mut pick_pose: Option<(Vec3, f32)> = None;

    // 1. Pick-Unit Candidate Highlight (Vivid Violet/Magenta) during PickRouteMarker hover
    let hovered_pick_marker_id = if is_pick_route_mode
        && let Some(SelectedEntity::Marker(hid)) = gizmo_state.hovered_entity
    {
        Some(hid)
    } else {
        None
    };

    if let Some(hid) = hovered_pick_marker_id
        && let Some(m) = state.markers.iter().find(|m| m.id == hid)
    {
        let pulse = 1.0 + 0.045 * (elapsed * 5.2).sin();
        let (center, radius, vol) = marker_footprint(m);
        let r = radius * 1.12 * pulse;
        pick_pose = Some((center, r));
        let violet = Color::srgba(0.92, 0.40, 1.0, 0.96);
        draw_tactical_target_ticks(&mut gizmos, center, r, -elapsed * 1.4, violet);
        if let Some((wmin, wmax)) = vol {
            draw_3d_corner_brackets(&mut gizmos, wmin, wmax, violet);
        }
        if let Some(start_w) = route_start_world {
            let from = Vec3::new(center.x, center.y.max(0.45), center.z);
            draw_dashed_tether_line(&mut gizmos, from, start_w, violet);
        }
    }

    // 2. Bound Marker Highlights for ALL targets of the active animation
    let bound_ids: Vec<Uuid> = active_route_anim
        .map(anim_bound_marker_ids)
        .unwrap_or_default();
    let flat_rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let (amber, amber_soft) = match active_route_anim.and_then(spawn_hide_action) {
        Some(SpawnHideAction::Spawn) => (
            Color::srgba(0.14, 0.94, 0.62, 0.96),
            Color::srgba(0.35, 0.98, 0.76, 0.68),
        ),
        Some(SpawnHideAction::Hide) => (
            Color::srgba(0.98, 0.34, 0.46, 0.96),
            Color::srgba(1.0, 0.56, 0.64, 0.68),
        ),
        None => (
            Color::srgba(1.0, 0.72, 0.14, 0.96),
            Color::srgba(1.0, 0.84, 0.34, 0.68),
        ),
    };
    let mut used_primary_disc = false;
    for bound_mid in &bound_ids {
        if hovered_pick_marker_id == Some(*bound_mid) {
            continue;
        }
        let Some(m) = state.markers.iter().find(|m| m.id == *bound_mid) else {
            continue;
        };
        let pulse = 1.0 + 0.03 * (elapsed * 3.4).sin();
        let (center, radius, vol) = marker_footprint(m);
        let r = radius * 1.10 * pulse;
        if !used_primary_disc {
            bound_pose = Some((center, r));
            used_primary_disc = true;
        } else {
            // Additional multi-route units get crisp concentric Amber tactical rings
            let ring_pos = Vec3::new(center.x, center.y + 0.025, center.z);
            gizmos.circle(Isometry3d::new(ring_pos, flat_rot), r, amber);
            gizmos.circle(Isometry3d::new(ring_pos, flat_rot), r * 0.93, amber);
            gizmos.circle(Isometry3d::new(ring_pos, flat_rot), r * 0.70, amber_soft);
        }
        draw_tactical_target_ticks(&mut gizmos, center, r, elapsed * 0.75, amber);
        if let Some((wmin, wmax)) = vol {
            draw_3d_corner_brackets(&mut gizmos, wmin, wmax, amber);
        }
        if let Some(start_w) = route_start_world {
            let from = Vec3::new(center.x, center.y.max(0.45), center.z);
            draw_dashed_tether_line(&mut gizmos, from, start_w, amber);
        }
    }

    // 2b. Group (`Combine` / `Sequence`) Member & Hovered Candidate 3D Marker Highlights
    if !playback.play_test_mode
        && let Some((group_mode, entries)) = super::resolve_group_highlight_entries(&state)
    {
        // Map each unique marker_id -> (is_focused_candidate, optional route start world pos)
        let mut marker_hl: HashMap<Uuid, (bool, Option<Vec3>)> = HashMap::new();
        for entry in &entries {
            let Some(child_anim) = state.anims.iter().find(|a| a.id == entry.anim_id) else {
                continue;
            };
            let child_route_start = if child_anim.geom.len() >= 2 {
                child_anim.geom.first().map(|p| {
                    Vec3::from_array(
                        state
                            .geo_anchor
                            .lng_lat_alt_to_local(p[0], p[1], p[2].max(0.45)),
                    )
                })
            } else {
                None
            };
            for mid in anim_bound_marker_ids(child_anim) {
                let prev = marker_hl.get(&mid).copied();
                let replace = prev.is_none_or(|(was_foc, _)| !was_foc && entry.is_focused_candidate);
                if replace {
                    marker_hl.insert(mid, (entry.is_focused_candidate, child_route_start));
                }
            }
        }

        let (base_col, base_soft) = match group_mode {
            twg_shared::group_anim::GroupAnimMode::Combine => (
                Color::srgba(0.28, 0.70, 1.0, 0.96),
                Color::srgba(0.45, 0.82, 1.0, 0.65),
            ),
            twg_shared::group_anim::GroupAnimMode::Sequence => (
                Color::srgba(0.72, 0.44, 1.0, 0.96),
                Color::srgba(0.84, 0.64, 1.0, 0.65),
            ),
        };
        let focus_col = Color::srgba(1.0, 0.88, 0.16, 0.98);
        let focus_soft = Color::srgba(1.0, 0.94, 0.48, 0.72);

        for (mid, (is_foc, child_start)) in marker_hl {
            let Some(m) = state.markers.iter().find(|m| m.id == mid) else {
                continue;
            };
            let (col, col_soft) = if is_foc {
                (focus_col, focus_soft)
            } else {
                (base_col, base_soft)
            };
            let pulse = if is_foc {
                1.0 + 0.055 * (elapsed * 5.8).sin()
            } else {
                1.0 + 0.03 * (elapsed * 3.2).sin()
            };
            let (center, radius, vol) = marker_footprint(m);
            let r = radius * (if is_foc { 1.18 } else { 1.10 }) * pulse;
            let ring_pos = Vec3::new(center.x, center.y + 0.03, center.z);
            gizmos.circle(Isometry3d::new(ring_pos, flat_rot), r, col);
            gizmos.circle(Isometry3d::new(ring_pos, flat_rot), r * 0.92, col);
            gizmos.circle(Isometry3d::new(ring_pos, flat_rot), r * 0.68, col_soft);
            draw_tactical_target_ticks(
                &mut gizmos,
                center,
                r,
                if is_foc { -elapsed * 1.5 } else { elapsed * 0.75 },
                col,
            );
            if let Some((wmin, wmax)) = vol {
                draw_3d_corner_brackets(&mut gizmos, wmin, wmax, col);
            }
            if let Some(start_w) = child_start {
                let from = Vec3::new(center.x, center.y.max(0.45), center.z);
                draw_dashed_tether_line(&mut gizmos, from, start_w, col);
            }
        }
    }

    // 3. Regular Hover Disc (Sky Blue) — suppressed in PickRouteMarker mode so Violet stands out cleanly
    if !is_pick_route_mode
        && let Some(SelectedEntity::Marker(hid)) = gizmo_state.hovered_entity
        && state.selected != SelectedEntity::Marker(hid)
        && !bound_ids.contains(&hid)
        && let Some(m) = state.markers.iter().find(|m| m.id == hid)
    {
        let (center, radius, _) = marker_footprint(m);
        hov_pose = Some((center, radius));
    }

    // 4. Regular Selected Highlight (Emerald-Mint Green)
    match state.selected {
        SelectedEntity::Marker(id) => {
            if let Some(m) = state.markers.iter().find(|m| m.id == id) {
                let pulse = 1.0 + 0.025 * (elapsed * 3.2).sin();
                let (center, radius, vol) = marker_footprint(m);
                sel_pose = Some((center, radius * pulse));
                if let Some((wmin, wmax)) = vol {
                    draw_3d_corner_brackets(
                        &mut gizmos,
                        wmin,
                        wmax,
                        Color::srgba(0.20, 0.96, 0.78, 0.92),
                    );
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

    for (kind, mut tf, mut vis) in &mut disc_q {
        let target = match kind {
            HighlightDiscKind::Selected => sel_pose,
            HighlightDiscKind::Hover => hov_pose,
            HighlightDiscKind::RouteBound => bound_pose,
            HighlightDiscKind::PickCandidate => pick_pose,
        };
        if let Some((center, r)) = target {
            tf.translation = center;
            tf.scale = Vec3::new(r, 1.0, r);
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// Draws 4 rotating tactical target-lock crosshair ticks around a ground footprint ring.
fn draw_tactical_target_ticks(
    gizmos: &mut Gizmos,
    center: Vec3,
    radius: f32,
    angle_offset: f32,
    color: Color,
) {
    let y = center.y + 0.04;
    let r_in = radius * 0.88;
    let r_out = radius * 1.28;
    for k in 0..4 {
        let ang = angle_offset + k as f32 * std::f32::consts::FRAC_PI_2;
        let dir = Vec3::new(ang.cos(), 0.0, ang.sin());
        let p0 = Vec3::new(center.x, y, center.z) + dir * r_in;
        let p1 = Vec3::new(center.x, y, center.z) + dir * r_out;
        gizmos.line(p0, p1, color);
    }
}

/// Draws a clean dashed guide tether linking a 3D marker to its route's start point (#1).
fn draw_dashed_tether_line(gizmos: &mut Gizmos, from: Vec3, to: Vec3, color: Color) {
    let delta = to - from;
    let dist = delta.length();
    if dist < 0.35 {
        return;
    }
    let steps = ((dist / 1.8).ceil() as usize).clamp(4, 36);
    for i in (0..steps).step_by(2) {
        let t0 = i as f32 / steps as f32;
        let t1 = ((i + 1).min(steps)) as f32 / steps as f32;
        gizmos.line(from + delta * t0, from + delta * t1, color);
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
    mut plot_line_edit: ResMut<PlotLineEditState>,
    mut route_edit: ResMut<RouteEditState>,
    api: Res<ApiClientBridge>,
    ws: Res<WsClientBridge>,
    cam_q: Query<(&Camera, &GlobalTransform), With<crate::map::camera::TacticalCamera>>,
    pick: ScenePickParams,
) {
    if state.pending_plot_exit.is_some() {
        return;
    }
    if state.screen != AppScreen::PlotSkenario {
        gizmo_state.cancel();
        poly_edit.clear();
        plot_line_edit.clear();
        route_edit.clear();
        state.dragging_catalog_entry = None;
        state.dragging_basic_mesh = None;
        state.catalog_drop_world_pos = None;
        return;
    }

    if playback.play_test_mode {
        // Editor gizmos, polygon/route editing, and catalog placement are inactive in Play Skenario;
        // Play Skenario RTS selection & `tools` 3D Move Gizmo are owned by `play_rts_interaction_system`.
        poly_edit.clear();
        plot_line_edit.clear();
        route_edit.clear();
        state.picking_marker_for_anim_id = None;
        state.picking_shoot_for_anim_id = None;
        state.picking_pointer_for_anim_id = None;
        state.picking_spawn_hide_for_anim_id = None;
        state.dragging_catalog_entry = None;
        state.dragging_basic_mesh = None;
        state.catalog_drop_world_pos = None;
        return;
    }

    // Auto-clear on-map Traffic Scale Preview if user switches selection or starts Play/Preview
    if let Some(ref preview) = state.traffic_scale_preview {
        let prev_aid = preview.anim_id;
        let still_valid = !playback.play_test_mode
            && playback.editor_preview_anim_id.is_none()
            && state.selected == SelectedEntity::Anim(prev_aid)
            && !state.locked_entity_ids.contains(&prev_aid)
            && resolve_active_traffic_scale_gizmo_pose(&state).is_some();
        if !still_valid {
            state.traffic_scale_preview = None;
        }
    }

    // Keep RouteEditState synchronized with current selection & lock status
    // Auto-enable interactive waypoint editing whenever an unlocked Route Animation is selected
    if !playback.play_test_mode
        && state.tool_mode.is_transform_tool()
        && state.traffic_scale_preview.is_none()
        && let SelectedEntity::Anim(sel_id) = state.selected
        && !state.locked_entity_ids.contains(&sel_id)
        && state
            .anims
            .iter()
            .find(|a| a.id == sel_id)
            .is_some_and(|a| !is_traffic_anim(a) && a.geom.len() >= 2)
        && route_edit.editing_anim_id != Some(sel_id)
    {
        route_edit.editing_anim_id = Some(sel_id);
        route_edit.traffic_lane_idx = None;
        route_edit.selected_waypoint = None;
        route_edit.add_point_on_map_mode = false;
        route_edit.active_drag = None;
    }

    if let Some(edit_id) = route_edit.editing_anim_id {
        let lane_idx = route_edit.traffic_lane_idx;
        let valid = !playback.play_test_mode
            && state.tool_mode.is_transform_tool()
            && state.traffic_scale_preview.is_none()
            && matches!(state.selected, SelectedEntity::Anim(sid) if sid == edit_id)
            && !state.locked_entity_ids.contains(&edit_id)
            && state
                .anims
                .iter()
                .find(|a| a.id == edit_id)
                .is_some_and(|a| editable_anim_geom(a, lane_idx).len() >= 2);
        if !valid {
            route_edit.clear();
        } else if let Some(wi) = route_edit.selected_waypoint {
            let n = state
                .anims
                .iter()
                .find(|a| a.id == edit_id)
                .map(|a| editable_anim_geom(a, lane_idx).len())
                .unwrap_or(0);
            if wi >= n {
                route_edit.selected_waypoint = None;
            }
        }
    }

    // Selecting a Plot Line opens its waypoint editor directly on the map.
    if let SelectedEntity::Line(line_id) = state.selected
        && state.tool_mode.is_transform_tool()
        && !state.locked_entity_ids.contains(&line_id)
        && state
            .lines
            .iter()
            .find(|line| line.id == line_id)
            .is_some_and(|line| line.geom.len() >= 2)
    {
        if plot_line_edit.editing_line_id != Some(line_id) {
            plot_line_edit.editing_line_id = Some(line_id);
            plot_line_edit.selected_point = None;
            plot_line_edit.hovered = None;
            plot_line_edit.active_drag = None;
        }
    } else {
        plot_line_edit.clear();
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
        if !playback.play_test_mode
            && let Some(line_id) = plot_line_edit.editing_line_id
            && let Some(point_index) = plot_line_edit.selected_point
            && (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace))
        {
            if remove_plot_line_point(&mut state, line_id, point_index) {
                plot_line_edit.selected_point = None;
            }
            return;
        }
        // Delete / Backspace removes the currently selected route waypoint directly on the map!
        if !playback.play_test_mode
            && let Some(edit_id) = route_edit.editing_anim_id
            && let Some(wi) = route_edit.selected_waypoint
            && (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace))
        {
            remove_route_waypoint(&mut state, &mut route_edit, edit_id, wi);
            return;
        }
        if keys.just_pressed(KeyCode::Delete)
            && !playback.play_test_mode
            && !poly_edit.is_active()
            && route_edit.selected_waypoint.is_none()
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
                    | ToolMode::DrawTrafficLane
            )
            && !state.draft_points_lnglat.is_empty()
        {
            state.draft_points_lnglat.pop();
            return;
        }
        if keys.just_pressed(KeyCode::Escape) {
            if state.tool_mode == ToolMode::DrawTrafficLane {
                state.draft_points_lnglat.clear();
                state.drawing_traffic_lane_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                state.status_banner = Some("Penggambaran Jalur Traffic dibatalkan.".into());
                return;
            }
            if state.tool_mode == ToolMode::PickShootMarker {
                state.picking_shoot_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                state.status_banner = Some("Pemilihan unit Shoot dibatalkan.".into());
                return;
            }
            if state.tool_mode == ToolMode::PickPointerMarker {
                state.picking_pointer_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                state.status_banner = Some("Pemilihan target Panah Penunjuk 3D dibatalkan.".into());
                return;
            }
            if state.tool_mode == ToolMode::PickSpawnHideMarker {
                let picked_anim = state
                    .picking_spawn_hide_for_anim_id
                    .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                    .cloned();
                state.picking_spawn_hide_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                if let Some(a) = picked_anim {
                    let cnt = anim_bound_marker_ids(&a).len();
                    state.status_banner = Some(format!(
                        "Selesai pilih objek untuk '{}' ({cnt} objek terpilih).",
                        a.name
                    ));
                } else {
                    state.status_banner = Some("Pemilihan objek Spawn/Hide selesai.".into());
                }
                return;
            }
            if state.tool_mode == ToolMode::PickRouteMarker {
                let picked_anim = state
                    .picking_marker_for_anim_id
                    .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                    .cloned();
                state.picking_marker_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                if let Some(a) = picked_anim
                    && is_multi_route_anim(&a)
                {
                    let cnt = anim_bound_marker_ids(&a).len();
                    state.status_banner = Some(format!(
                        "Selesai pilih unit untuk '{}' ({cnt} unit terikat).",
                        a.name
                    ));
                } else {
                    state.status_banner = Some("Pemilihan unit rute dibatalkan.".into());
                }
                return;
            }
            if route_edit.selected_waypoint.is_some() {
                route_edit.selected_waypoint = None;
                route_edit.active_drag = None;
                return;
            }
            if route_edit.add_point_on_map_mode {
                route_edit.add_point_on_map_mode = false;
                return;
            }
            if route_edit.is_active() {
                route_edit.clear();
                clear_selection(&mut state);
                return;
            }
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
            state.picking_marker_for_anim_id = None;
            state.picking_shoot_for_anim_id = None;
            state.picking_pointer_for_anim_id = None;
            state.picking_spawn_hide_for_anim_id = None;
            state.dragging_catalog_entry = None;
            state.dragging_basic_mesh = None;
            state.catalog_drop_world_pos = None;
            state.tool_mode = ToolMode::Select;
            gizmo_state.cancel();
            return;
        }
        if keys.just_pressed(KeyCode::Enter) {
            if state.tool_mode == ToolMode::PickRouteMarker {
                let picked_anim = state
                    .picking_marker_for_anim_id
                    .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                    .cloned();
                state.picking_marker_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                if let Some(a) = picked_anim {
                    let cnt = anim_bound_marker_ids(&a).len();
                    state.status_banner = Some(format!(
                        "Selesai memilih unit untuk '{}' ({cnt} unit terikat).",
                        a.name
                    ));
                }
                return;
            }
            if state.tool_mode == ToolMode::PickSpawnHideMarker {
                let picked_anim = state
                    .picking_spawn_hide_for_anim_id
                    .and_then(|aid| state.anims.iter().find(|a| a.id == aid))
                    .cloned();
                state.picking_spawn_hide_for_anim_id = None;
                state.tool_mode = ToolMode::Select;
                if let Some(a) = picked_anim {
                    let cnt = anim_bound_marker_ids(&a).len();
                    state.status_banner = Some(format!(
                        "Selesai memilih objek untuk '{}' ({cnt} objek terpilih).",
                        a.name
                    ));
                }
                return;
            }
            finish_active_draft(&mut state, &api);
            return;
        }
        if !playback.play_test_mode && !ctrl {
            if keys.just_pressed(KeyCode::Digit1) {
                state.tool_mode = ToolMode::Select;
                state.draft_points_lnglat.clear();
                state.picking_marker_for_anim_id = None;
                state.picking_spawn_hide_for_anim_id = None;
                gizmo_state.cancel();
                poly_edit.clear();
                route_edit.clear();
            } else if keys.just_pressed(KeyCode::Digit2) {
                state.tool_mode = ToolMode::MoveMarker;
                state.draft_points_lnglat.clear();
                state.picking_marker_for_anim_id = None;
                gizmo_state.cancel();
                poly_edit.clear();
                route_edit.clear();
            } else if keys.just_pressed(KeyCode::Digit3) {
                state.tool_mode = ToolMode::RotateMarker;
                state.draft_points_lnglat.clear();
                state.picking_marker_for_anim_id = None;
                gizmo_state.cancel();
                poly_edit.clear();
                route_edit.clear();
            } else if keys.just_pressed(KeyCode::Digit4) {
                state.tool_mode = ToolMode::ScaleMarker;
                state.draft_points_lnglat.clear();
                state.picking_marker_for_anim_id = None;
                gizmo_state.cancel();
                poly_edit.clear();
                route_edit.clear();
            }
        }
    }

    // Do not interact with 3D gizmos or entities while dragging a 3D Catalog or Basic Mesh item
    if state.dragging_catalog_entry.is_some() || state.dragging_basic_mesh.is_some() {
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
        gizmo_state.empty_click_start_px = None;
        poly_edit.hovered = None;
        route_edit.hovered = None;
        return;
    }

    let Ok((camera, cam_gtf)) = cam_q.single() else {
        return;
    };

    // Release an active Plot Line point drag. A click without moving is only a selection.
    if (mouse_buttons.just_released(MouseButton::Left) || !mouse_buttons.pressed(MouseButton::Left))
        && let Some(finished) = plot_line_edit.active_drag.take()
    {
        gizmo_state.suppress_camera_pan = false;
        gizmo_state.empty_click_start_px = None;
        if finished.moved {
            state.status_banner = Some(format!(
                "Titik Plot Line dipindahkan — {} perubahan belum disimpan (Simpan / Ctrl+S)",
                state.unsaved_change_count()
            ));
        }
        return;
    }

    // Continue Plot Line drags from the original click, matching route waypoint behavior.
    if plot_line_edit.active_drag.is_some() && mouse_buttons.pressed(MouseButton::Left) {
        if let Some(cursor_px) = window.cursor_position() {
            let mut drag = plot_line_edit.active_drag.take().unwrap();
            update_plot_line_drag(&mut drag, cursor_px, &mut state, camera, cam_gtf);
            plot_line_edit.active_drag = Some(drag);
        }
        return;
    }

    // Release active route waypoint gizmo drag
    if (mouse_buttons.just_released(MouseButton::Left) || !mouse_buttons.pressed(MouseButton::Left))
        && let Some(finished) = route_edit.active_drag.take()
    {
        if finished.moved {
            state.status_banner = Some(format!(
                "Waypoint #{} rute dipindahkan — {} perubahan belum disimpan (Simpan / Ctrl+S)",
                finished.waypoint_idx + 1,
                state.unsaved_change_count()
            ));
        }
        gizmo_state.suppress_camera_pan = false;
        gizmo_state.empty_click_start_px = None;
        return;
    }

    // Continue active route waypoint gizmo drag smoothly
    if route_edit.active_drag.is_some() && mouse_buttons.pressed(MouseButton::Left) {
        if let Some(cursor_px) = window.cursor_position() {
            let mut drag = route_edit.active_drag.take().unwrap();
            update_waypoint_drag(&mut drag, cursor_px, &mut state, camera, cam_gtf);
            route_edit.active_drag = Some(drag);
        }
        return;
    }

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

    // Release active transform gizmo drag and mark its edited entity for toolbar Simpan.
    if (mouse_buttons.just_released(MouseButton::Left) || !mouse_buttons.pressed(MouseButton::Left))
        && let Some(finished) = gizmo_state.active_drag.take()
    {
        if finished.moved {
            if finished.is_text_marker {
                state.dirty_text_ids.insert(finished.marker_id);
            } else if finished.is_particle_anim || finished.traffic_scale_preview.is_some() {
                state.dirty_anim_ids.insert(finished.marker_id);
            } else {
                state.dirty_marker_ids.insert(finished.marker_id);
            }
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
        plot_line_edit.hovered = None;
        route_edit.hovered = None;
        return;
    }

    let Some(cursor_px) = window.cursor_position() else {
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
        poly_edit.hovered = None;
        plot_line_edit.hovered = None;
        route_edit.hovered = None;
        return;
    };

    // Continuous hover hit-testing: Plot Line -> Route -> Polygon -> Object Transform Gizmo
    if !playback.play_test_mode
        && let Some(line_id) = plot_line_edit.editing_line_id
    {
        plot_line_edit.hovered = hit_test_plot_line_editor(
            cursor_px,
            &state,
            line_id,
            plot_line_edit.selected_point,
            camera,
            cam_gtf,
        );
        route_edit.hovered = None;
        poly_edit.hovered = None;
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
    } else if !playback.play_test_mode
        && let Some(edit_id) = route_edit.editing_anim_id
    {
        route_edit.hovered = state.anims.iter().find(|a| a.id == edit_id).and_then(|a| {
            hit_test_route_editor(
                cursor_px,
                &state,
                a,
                route_edit.traffic_lane_idx,
                route_edit.selected_waypoint,
                camera,
                cam_gtf,
            )
        });
        poly_edit.hovered = None;
        gizmo_state.hovered_handle = None;
        gizmo_state.hovered_entity = None;
    } else if !playback.play_test_mode
        && let Some(edit_id) = poly_edit.editing_marker_id
    {
        route_edit.hovered = None;
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
        plot_line_edit.hovered = None;
        route_edit.hovered = None;
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
            } else if state.tool_mode.is_transform_tool()
                && let SelectedEntity::Anim(sel_id) = state.selected
                && !state.locked_entity_ids.contains(&sel_id)
                && let Some(sp) = resolve_active_traffic_scale_gizmo_pose(&state)
                && sp.anim_id == sel_id
            {
                let origin = sp.world_pos;
                let rot_quat = Quat::from_rotation_y(sp.model_yaw_deg.to_radians());
                let radius = compute_gizmo_world_radius(
                    camera,
                    cam_gtf,
                    origin,
                    GIZMO_SCREEN_RADIUS_PX,
                );
                hit_test_gizmo(
                    cursor_px,
                    GizmoMode::Scale,
                    origin,
                    rot_quat,
                    radius,
                    camera,
                    cam_gtf,
                )
            } else if matches!(state.tool_mode, ToolMode::Select | ToolMode::MoveMarker)
                && let SelectedEntity::Anim(sel_id) = state.selected
                && !state.locked_entity_ids.contains(&sel_id)
            {
                state
                    .anims
                    .iter()
                    .find(|a| a.id == sel_id && is_particle_anim(a) && !a.geom.is_empty())
                    .and_then(|a| {
                        let origin = particle_anim_world_origin(&state, a);
                        let radius = compute_gizmo_world_radius(
                            camera,
                            cam_gtf,
                            origin,
                            GIZMO_SCREEN_RADIUS_PX,
                        );
                        hit_test_gizmo(
                            cursor_px,
                            GizmoMode::Move,
                            origin,
                            Quat::IDENTITY,
                            radius,
                            camera,
                            cam_gtf,
                        )
                    })
            } else if matches!(state.tool_mode, ToolMode::Select | ToolMode::MoveMarker)
                && let SelectedEntity::TextMarker(sel_id) = state.selected
                && !state.locked_entity_ids.contains(&sel_id)
            {
                state
                    .text_markers
                    .iter()
                    .find(|text| text.id == sel_id)
                    .and_then(|text| {
                        let origin = Vec3::from_array(
                            state.geo_anchor.lng_lat_alt_to_local(
                                text.lng,
                                text.lat,
                                text.altitude,
                            ),
                        );
                        let radius = compute_gizmo_world_radius(
                            camera,
                            cam_gtf,
                            origin,
                            GIZMO_SCREEN_RADIUS_PX,
                        );
                        hit_test_gizmo(
                            cursor_px,
                            GizmoMode::Move,
                            origin,
                            Quat::IDENTITY,
                            radius,
                            camera,
                            cam_gtf,
                        )
                    })
            } else {
                None
            }
        } else {
            None
        };
    }

    let hit_opt = cursor_world_ground(&window, camera, cam_gtf);

    // Continuous 3D body / label / route-line hover detection in transform modes & Pick modes
    gizmo_state.hovered_entity = if !poly_edit.is_active()
        && plot_line_edit.editing_line_id.is_none()
        && route_edit.hovered.is_none()
        && !route_edit.add_point_on_map_mode
        && (state.tool_mode.is_transform_tool()
            || matches!(
                state.tool_mode,
                ToolMode::PickRouteMarker
                    | ToolMode::PickShootMarker
                    | ToolMode::PickPointerMarker
                    | ToolMode::PickSpawnHideMarker
            ))
        && gizmo_state.hovered_handle.is_none()
    {
        let picked = pick_scene_entity(
            &state,
            &playback,
            &overrides,
            &marker_bounds,
            cursor_px,
            hit_opt,
            camera,
            cam_gtf,
            &pick,
        );
        if matches!(
            state.tool_mode,
            ToolMode::PickRouteMarker
                | ToolMode::PickShootMarker
                | ToolMode::PickPointerMarker
                | ToolMode::PickSpawnHideMarker
        ) {
            match picked {
                Some(SelectedEntity::Marker(mid)) => Some(SelectedEntity::Marker(mid)),
                _ => None,
            }
        } else {
            picked
        }
    } else {
        None
    };

    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // Priority 0a: Interactive Plot Line point editing.
    if !playback.play_test_mode
        && let Some(line_id) = plot_line_edit.editing_line_id
    {
        match plot_line_edit.hovered {
            Some(PlotLineHoverTarget::GizmoHandle(handle)) => {
                if let Some(index) = plot_line_edit.selected_point
                    && let Some(drag) = begin_plot_line_drag(
                        &state, line_id, index, handle, cursor_px, camera, cam_gtf,
                    )
                {
                    plot_line_edit.active_drag = Some(drag);
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                }
                return;
            }
            Some(PlotLineHoverTarget::Point(index)) => {
                plot_line_edit.selected_point = Some(index);
                plot_line_edit.active_drag = begin_plot_line_drag(
                    &state,
                    line_id,
                    index,
                    PlotLineGizmoHandle::Free,
                    cursor_px,
                    camera,
                    cam_gtf,
                );
                gizmo_state.suppress_camera_pan = true;
                gizmo_state.empty_click_start_px = None;
                state.status_banner = Some(format!(
                    "Titik #{} Plot Line dipilih — tarik untuk memindahkan titik.",
                    index + 1
                ));
                return;
            }
            Some(PlotLineHoverTarget::Midpoint(segment_index)) => {
                if let Some(index) = insert_plot_line_midpoint(&mut state, line_id, segment_index)
                {
                    plot_line_edit.selected_point = Some(index);
                    plot_line_edit.active_drag = begin_plot_line_drag(
                        &state,
                        line_id,
                        index,
                        PlotLineGizmoHandle::Free,
                        cursor_px,
                        camera,
                        cam_gtf,
                    );
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                    state.status_banner = Some(format!(
                        "Titik #{} disisipkan pada Plot Line — tarik untuk membentuk belokan.",
                        index + 1
                    ));
                }
                return;
            }
            Some(PlotLineHoverTarget::DeletePoint(index)) => {
                if remove_plot_line_point(&mut state, line_id, index) {
                    let count = state
                        .lines
                        .iter()
                        .find(|line| line.id == line_id)
                        .map(|line| line.geom.len())
                        .unwrap_or(0);
                    plot_line_edit.selected_point = (count > 0).then_some(index.min(count - 1));
                    state.status_banner = Some(
                        "Titik Plot Line dihapus — klik Simpan (Ctrl+S) untuk menyimpan.".into(),
                    );
                }
                gizmo_state.suppress_camera_pan = true;
                gizmo_state.empty_click_start_px = None;
                return;
            }
            None => {
                plot_line_edit.selected_point = None;
            }
        }
    }

    // Priority 0a: Interactive Route Waypoint Editing (direct drag-on-dot, Move Gizmo, Midpoint '+' insert, Map Click Append)
    if !playback.play_test_mode
        && let Some(edit_id) = route_edit.editing_anim_id
    {
        match route_edit.hovered {
            Some(RouteHoverTarget::GizmoHandle(handle)) => {
                if let Some(wi) = route_edit.selected_waypoint
                    && let Some(anim) = state.anims.iter().find(|a| a.id == edit_id).cloned()
                    && let Some(drag) = begin_waypoint_drag(
                        &state,
                        &anim,
                        route_edit.traffic_lane_idx,
                        wi,
                        handle,
                        cursor_px,
                        camera,
                        cam_gtf,
                    )
                {
                    route_edit.active_drag = Some(drag);
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                }
                return;
            }
            Some(RouteHoverTarget::DeleteWaypoint(wi)) => {
                remove_route_waypoint(&mut state, &mut route_edit, edit_id, wi);
                gizmo_state.suppress_camera_pan = true;
                gizmo_state.empty_click_start_px = None;
                return;
            }
            Some(RouteHoverTarget::Waypoint(wi)) => {
                route_edit.selected_waypoint = Some(wi);
                route_edit.add_point_on_map_mode = false;
                // Start Free drag immediately so the user can drag the waypoint directly in 1 motion,
                // while a simple click-and-release keeps the X/Alt/Z/Free Move Gizmo + Delete 'x' visible!
                if let Some(anim) = state.anims.iter().find(|a| a.id == edit_id).cloned()
                    && let Some(drag) = begin_waypoint_drag(
                        &state,
                        &anim,
                        route_edit.traffic_lane_idx,
                        wi,
                        RouteWaypointGizmoHandle::Free,
                        cursor_px,
                        camera,
                        cam_gtf,
                    )
                {
                    route_edit.active_drag = Some(drag);
                }
                gizmo_state.suppress_camera_pan = true;
                gizmo_state.empty_click_start_px = None;
                state.status_banner = Some(format!(
                    "Titik #{} dipilih — tarik langsung, geser X / Alt (Hijau) / Z, atau klik (x) merah / Del untuk hapus.",
                    wi + 1
                ));
                return;
            }
            Some(RouteHoverTarget::Midpoint(seg_idx)) => {
                let lane_idx = route_edit.traffic_lane_idx;
                if let Some(mut anim_clone) = state.anims.iter().find(|a| a.id == edit_id).cloned()
                {
                    let mut geom = editable_anim_geom(&anim_clone, lane_idx);
                    let new_idx = insert_route_midpoint_after(&mut geom, seg_idx);
                    write_editable_anim_geom(&mut anim_clone, lane_idx, geom);
                    if let Some(a) = state.anims.iter_mut().find(|x| x.id == edit_id) {
                        *a = anim_clone.clone();
                    }
                    if state.anim_edit.as_ref().is_some_and(|e| e.id == edit_id) {
                        state.anim_edit = Some(anim_clone.clone());
                    }
                    state.dirty_anim_ids.insert(edit_id);
                    route_edit.selected_waypoint = Some(new_idx);
                    route_edit.add_point_on_map_mode = false;
                    if let Some(drag) = begin_waypoint_drag(
                        &state,
                        &anim_clone,
                        lane_idx,
                        new_idx,
                        RouteWaypointGizmoHandle::Free,
                        cursor_px,
                        camera,
                        cam_gtf,
                    ) {
                        route_edit.active_drag = Some(drag);
                    }
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                    state.status_banner = Some(format!(
                        "Titik #{} disisipkan di jalur — tarik untuk membentuk belokan.",
                        new_idx + 1
                    ));
                }
                return;
            }
            None => {
                if route_edit.add_point_on_map_mode {
                    if let Some(new_idx) = append_waypoint_from_cursor_click(
                        &mut state,
                        edit_id,
                        route_edit.traffic_lane_idx,
                        cursor_px,
                        camera,
                        cam_gtf,
                    ) {
                        route_edit.selected_waypoint = Some(new_idx);
                        gizmo_state.suppress_camera_pan = true;
                        gizmo_state.empty_click_start_px = None;
                        state.status_banner = Some(format!(
                            "Titik #{} ditambahkan di ujung jalur — geser titik atau klik peta lagi.",
                            new_idx + 1
                        ));
                    }
                    return;
                }
                // If clicking the same route segment (not a dot), just hide any open waypoint gizmo
                if gizmo_state.hovered_entity == Some(SelectedEntity::Anim(edit_id)) {
                    route_edit.selected_waypoint = None;
                    gizmo_state.empty_click_start_px = None;
                    return;
                }
                // Clicking empty map while a waypoint Move Gizmo is open closes the gizmo first;
                // clicking another entity or empty map cleanly falls through to normal selection/deselect!
                if gizmo_state.hovered_entity.is_none() && route_edit.selected_waypoint.is_some() {
                    route_edit.selected_waypoint = None;
                    return;
                }
            }
        }
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

    // Priority 1: Clicking a handle on the active 3D Move/Transform Gizmo.
    if !playback.play_test_mode
        && let Some(handle) = gizmo_state.hovered_handle
    {
        if let (Some(mode), SelectedEntity::Marker(sel_id)) =
            (gizmo_mode_for_tool(state.tool_mode), state.selected)
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
        if state.tool_mode.is_transform_tool()
            && let SelectedEntity::Anim(sel_id) = state.selected
            && !state.locked_entity_ids.contains(&sel_id)
            && state
                .traffic_scale_preview
                .as_ref()
                .is_some_and(|p| p.anim_id == sel_id)
            && let Some(drag) =
                begin_traffic_preview_scale_drag(&state, handle, cursor_px, camera, cam_gtf)
        {
            gizmo_state.active_drag = Some(drag);
            gizmo_state.suppress_camera_pan = true;
            gizmo_state.empty_click_start_px = None;
            return;
        }
        if matches!(state.tool_mode, ToolMode::Select | ToolMode::MoveMarker)
            && let SelectedEntity::Anim(sel_id) = state.selected
            && !state.locked_entity_ids.contains(&sel_id)
            && let Some(anim) = state
                .anims
                .iter()
                .find(|a| a.id == sel_id && is_particle_anim(a) && !a.geom.is_empty())
                .cloned()
            && let Some(drag) =
                begin_particle_move_drag(&anim, handle, cursor_px, &state, camera, cam_gtf)
        {
            gizmo_state.active_drag = Some(drag);
            gizmo_state.suppress_camera_pan = true;
            gizmo_state.empty_click_start_px = None;
            return;
        }
        if matches!(state.tool_mode, ToolMode::Select | ToolMode::MoveMarker)
            && let SelectedEntity::TextMarker(sel_id) = state.selected
            && !state.locked_entity_ids.contains(&sel_id)
            && let Some(text) = state.text_markers.iter().find(|text| text.id == sel_id)
            && let Some(drag) = begin_text_marker_move_drag(
                text.id,
                Vec3::from_array(
                    state
                        .geo_anchor
                        .lng_lat_alt_to_local(text.lng, text.lat, text.altitude),
                ),
                handle,
                cursor_px,
                camera,
                cam_gtf,
            )
        {
            gizmo_state.active_drag = Some(drag);
            gizmo_state.suppress_camera_pan = true;
            gizmo_state.empty_click_start_px = None;
            return;
        }
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
                Some(SelectedEntity::Line(id)) => {
                    state.selected = SelectedEntity::Line(id);
                    state.line_edit = state.lines.iter().find(|line| line.id == id).cloned();
                    state.marker_edit = None;
                    state.building_edit = None;
                    state.text_edit = None;
                    state.anim_edit = None;
                    route_edit.clear();
                    poly_edit.clear();
                    plot_line_edit.editing_line_id = Some(id);
                    plot_line_edit.selected_point = None;
                    plot_line_edit.hovered = None;
                    plot_line_edit.active_drag = None;
                    gizmo_state.suppress_camera_pan = true;
                    gizmo_state.empty_click_start_px = None;
                    state.status_banner = Some(
                        "Plot Line dipilih — klik titik untuk edit atau tombol (+) untuk menyisipkan titik."
                            .into(),
                    );
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
                Some(SelectedEntity::Anim(id)) => {
                    let clicked_anim = state.anims.iter().find(|a| a.id == id).cloned();
                    let is_route_poly = clicked_anim
                        .as_ref()
                        .is_some_and(|a| !is_traffic_anim(a) && a.geom.len() >= 2);
                    let clicked_traffic_lane = clicked_anim.as_ref().and_then(|a| {
                        if is_traffic_anim(a) {
                            hit_test_traffic_lane_index(cursor_px, &state, a, camera, cam_gtf)
                                .or(Some(0))
                        } else {
                            None
                        }
                    });
                    let is_part = clicked_anim.as_ref().is_some_and(is_particle_anim);
                    state.selected = SelectedEntity::Anim(id);
                    state.anim_edit = clicked_anim;
                    state.marker_edit = None;
                    state.building_edit = None;
                    state.line_edit = None;
                    state.text_edit = None;
                    if (is_route_poly || clicked_traffic_lane.is_some())
                        && state.traffic_scale_preview.is_none()
                        && !playback.play_test_mode
                        && !state.locked_entity_ids.contains(&id)
                    {
                        route_edit.editing_anim_id = Some(id);
                        route_edit.traffic_lane_idx = clicked_traffic_lane;
                        route_edit.selected_waypoint = None;
                        route_edit.add_point_on_map_mode = false;
                        route_edit.active_drag = None;
                    } else {
                        route_edit.clear();
                    }
                    if is_part {
                        state.particle_preview_reset_nonce =
                            state.particle_preview_reset_nonce.wrapping_add(1);
                    }
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
        | ToolMode::DrawRoute
        | ToolMode::DrawTrafficLane => {
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
                let text_id = Uuid::new_v4();
                state.text_markers.push(TextMarker {
                    id: text_id,
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    text: label,
                    color: "#38bdf8".into(),
                    font_size: 14.0,
                    show_background: true,
                    lng,
                    lat,
                    altitude: 0.0,
                    created_at: now,
                    updated_at: now,
                });
                state.selected = SelectedEntity::TextMarker(text_id);
                state.text_edit = state
                    .text_markers
                    .iter()
                    .find(|text| text.id == text_id)
                    .cloned();
            } else if let Some(auth) = current_auth(&state) {
                api.send(ApiCommand::CreateTextMarker {
                    auth,
                    req: CreateTextMarkerRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        text: label,
                        color: "#38bdf8".into(),
                        font_size: 14.0,
                        show_background: true,
                        lng,
                        lat,
                        altitude: 0.0,
                    },
                });
            }
            // Add Label is deliberately a one-shot tool to prevent accidental duplicates.
            state.tool_mode = ToolMode::Select;
            state.status_banner = Some("Label ditambahkan — kembali ke mode Select.".into());
        }
        ToolMode::PickRouteMarker => {
            if playback.play_test_mode {
                return;
            }
            if let Some(SelectedEntity::Marker(marker_id)) = gizmo_state.hovered_entity
                && let Some(anim_id) = state.picking_marker_for_anim_id
            {
                let marker_clone = state
                    .markers
                    .iter()
                    .find(|m| m.id == marker_id)
                    .cloned();
                if let Some(m_obj) = marker_clone
                    && let Some(anim) = state.anims.iter_mut().find(|a| a.id == anim_id)
                {
                    let m_name = m_obj.name.clone();
                    let m_alt = m_obj.altitude;
                    let is_multi = is_multi_route_anim(anim);
                    let status_msg = if is_multi {
                        let mut cfg = parse_multi_route_config(anim);
                        let was_empty = cfg.marker_ids.is_empty();
                        let added = cfg.toggle_marker(marker_id);
                        let assigned_role_lbl = if added && cfg.formation == RouteFormationKind::Patwal
                        {
                            let inferred = cfg.infer_patwal_role_for_marker(&m_obj);
                            cfg.set_patwal_role(marker_id, inferred);
                            Some(inferred.label())
                        } else {
                            None
                        };
                        write_multi_route_config(anim, &cfg);
                        if was_empty && added {
                            for pt in &mut anim.geom {
                                pt[2] = m_alt;
                            }
                        }
                        let cnt = cfg.marker_ids.len();
                        if added {
                            if let Some(rlbl) = assigned_role_lbl {
                                format!(
                                    "Unit '{m_name}' ditambahkan (#{cnt}, peran: {rlbl}) ke '{}'! Klik unit lain atau Enter / Esc untuk selesai.",
                                    anim.name
                                )
                            } else {
                                format!(
                                    "Unit '{m_name}' ditambahkan (#{cnt}) ke '{}'! Klik unit lain untuk tambah/lepas, atau tekan Enter / Esc untuk selesai.",
                                    anim.name
                                )
                            }
                        } else {
                            format!(
                                "Unit '{m_name}' dilepas dari '{}' (sisa {cnt} unit). Klik unit lain atau Enter / Esc untuk selesai.",
                                anim.name
                            )
                        }
                    } else {
                        anim.marker_id = Some(marker_id);
                        for pt in &mut anim.geom {
                            pt[2] = m_alt;
                        }
                        format!(
                            "Unit '{m_name}' diikat ke rute '{}'! Atur speed/repeat atau klik Play Skenario.",
                            anim.name
                        )
                    };

                    anim.duration_sec = route_estimated_duration_sec(&anim.geom, anim.speed);
                    let updated = anim.clone();
                    state.selected = SelectedEntity::Anim(anim_id);
                    state.anim_edit = Some(updated.clone());
                    state.marker_edit = None;
                    state.building_edit = None;
                    state.line_edit = None;
                    state.text_edit = None;
                    state.dirty_anim_ids.insert(anim_id);

                    if !state.offline_preview
                        && let Some(auth) = current_auth(&state)
                    {
                        api.send(ApiCommand::UpdateAnim {
                            auth,
                            id: anim_id,
                            req: UpdateAnimRequest {
                                marker_id: updated.marker_id,
                                name: updated.name,
                                anim_type: updated.anim_type,
                                sort_order: updated.sort_order,
                                speed: updated.speed,
                                duration_sec: updated.duration_sec,
                                replay: updated.replay,
                                autoplay: updated.autoplay,
                                geom: updated.geom,
                                data: updated.data,
                            },
                        });
                    }
                    // Single route exits pick mode immediately; Multi-unit stays active so user can pick several units!
                    if !is_multi {
                        state.picking_marker_for_anim_id = None;
                        state.tool_mode = ToolMode::Select;
                    }
                    gizmo_state.suppress_camera_pan = true;
                    state.status_banner = Some(status_msg);
                }
            }
        }
        ToolMode::PickShootMarker => {
            if playback.play_test_mode {
                return;
            }
            if let Some(SelectedEntity::Marker(marker_id)) = gizmo_state.hovered_entity
                && let Some(marker_name) = state
                    .markers
                    .iter()
                    .find(|m| m.id == marker_id)
                    .map(|m| m.name.clone())
            {
                if let Some(anim_id) = state.picking_shoot_for_anim_id.take() {
                    // Rebind an existing Water Cannon animation to a different 3D marker
                    if let Some(anim) = state.anims.iter_mut().find(|a| a.id == anim_id) {
                        anim.marker_id = Some(marker_id);
                        let updated = anim.clone();
                        state.selected = SelectedEntity::Anim(anim_id);
                        state.anim_edit = Some(updated.clone());
                        state.marker_edit = None;
                        state.building_edit = None;
                        state.line_edit = None;
                        state.text_edit = None;
                        state.dirty_anim_ids.insert(anim_id);
                        let kind_label = shoot_anim_kind(&updated)
                            .map(|k| k.label())
                            .unwrap_or("Shoot");
                        if !state.offline_preview
                            && let Some(auth) = current_auth(&state)
                        {
                            api.send(ApiCommand::UpdateAnim {
                                auth,
                                id: anim_id,
                                req: UpdateAnimRequest {
                                    marker_id: updated.marker_id,
                                    name: updated.name.clone(),
                                    anim_type: updated.anim_type,
                                    sort_order: updated.sort_order,
                                    speed: updated.speed,
                                    duration_sec: updated.duration_sec,
                                    replay: updated.replay,
                                    autoplay: updated.autoplay,
                                    geom: updated.geom,
                                    data: updated.data,
                                },
                            });
                        }
                        state.shoot_preview_reset_nonce =
                            state.shoot_preview_reset_nonce.wrapping_add(1);
                        state.status_banner = Some(format!(
                            "{kind_label} '{}' dipindahkan ke unit '{marker_name}' — preview aktif di peta!",
                            updated.name
                        ));
                    }
                } else if let Some(sk) = state.active_skenario.clone() {
                    // Create a brand-new Shoot animation of `state.new_shoot_kind` bound to the clicked 3D marker
                    let kind = state.new_shoot_kind;
                    let auto_h = marker_bounds
                        .bounds
                        .get(&marker_id)
                        .map(|b| (b.world_max.y - b.world_min.y) as f64)
                        .filter(|&h| h.is_finite() && h >= 0.4)
                        .map(|h| {
                            ((h * kind.origin_height_ratio()).clamp(0.4, 30.0) * 10.0).round()
                                / 10.0
                        })
                        .unwrap_or_else(|| kind.fallback_origin_height_m());
                    let (data, duration_sec) = match kind {
                        ShootAnimKind::WaterCannon => {
                            let cfg = WaterCannonConfig {
                                nozzle_height_m: auto_h,
                                ..Default::default()
                            };
                            (serde_json::json!({ "water_cannon": cfg }), kind.default_duration_sec())
                        }
                        ShootAnimKind::StoneThrow => {
                            let cfg = StoneThrowConfig {
                                origin_height_m: auto_h,
                                ..Default::default()
                            };
                            (serde_json::json!({ "stone_throw": cfg }), kind.default_duration_sec())
                        }
                        ShootAnimKind::TearGas => {
                            let cfg = TearGasConfig {
                                origin_height_m: auto_h,
                                ..Default::default()
                            };
                            let dur = cfg.total_cycle_duration_sec();
                            (serde_json::json!({ "tear_gas": cfg }), dur)
                        }
                        ShootAnimKind::Bullet => {
                            let cfg = BulletShootConfig {
                                origin_height_m: auto_h,
                                ..Default::default()
                            };
                            (serde_json::json!({ "bullet": cfg }), kind.default_duration_sec())
                        }
                        ShootAnimKind::Apar => {
                            let cfg = AparSprayConfig {
                                origin_height_m: auto_h,
                                ..Default::default()
                            };
                            (serde_json::json!({ "apar": cfg }), kind.default_duration_sec())
                        }
                    };
                    let name = if state.new_shoot_name.trim().is_empty() {
                        format!("{} — {marker_name}", kind.label())
                    } else {
                        state.new_shoot_name.trim().to_string()
                    };
                    let sort_order = (state.anims.len() + 1) as i32;
                    let anim_type_str = kind.anim_type_str().to_string();

                    if state.offline_preview {
                        let now = chrono::Utc::now();
                        let a = Anim {
                            id: Uuid::new_v4(),
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: Some(marker_id),
                            name: name.clone(),
                            anim_type: anim_type_str,
                            sort_order,
                            speed: 1.0,
                            duration_sec,
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
                        state.shoot_preview_reset_nonce =
                            state.shoot_preview_reset_nonce.wrapping_add(1);
                        state.status_banner = Some(format!(
                            "{} '{name}' dibuat pada '{marker_name}' — live preview aktif di peta!",
                            kind.label()
                        ));
                    } else if let Some(auth) = current_auth(&state) {
                        api.send(ApiCommand::CreateAnim {
                            auth,
                            req: CreateAnimRequest {
                                operasi_id: sk.operasi_id,
                                skenario_id: sk.id,
                                marker_id: Some(marker_id),
                                name: name.clone(),
                                anim_type: anim_type_str,
                                sort_order,
                                speed: 1.0,
                                duration_sec,
                                replay: false,
                                autoplay: false,
                                geom: Vec::new(),
                                data,
                            },
                        });
                    }
                    let next_idx = state
                        .anims
                        .iter()
                        .filter(|a| shoot_anim_kind(a) == Some(kind))
                        .count()
                        + 2;
                    state.new_shoot_name = kind.default_name(next_idx);
                }
                state.tool_mode = ToolMode::Select;
                gizmo_state.suppress_camera_pan = true;
            }
        }
        ToolMode::PickPointerMarker => {
            if playback.play_test_mode {
                return;
            }
            if let Some(SelectedEntity::Marker(marker_id)) = gizmo_state.hovered_entity
                && let Some(marker_name) = state
                    .markers
                    .iter()
                    .find(|m| m.id == marker_id)
                    .map(|m| m.name.clone())
            {
                if let Some(anim_id) = state.picking_pointer_for_anim_id.take() {
                    if let Some(anim) = state.anims.iter_mut().find(|a| a.id == anim_id) {
                        anim.marker_id = Some(marker_id);
                        let updated = anim.clone();
                        state.selected = SelectedEntity::Anim(anim_id);
                        state.anim_edit = Some(updated.clone());
                        state.marker_edit = None;
                        state.building_edit = None;
                        state.line_edit = None;
                        state.text_edit = None;
                        state.dirty_anim_ids.insert(anim_id);
                        if !state.offline_preview
                            && let Some(auth) = current_auth(&state)
                        {
                            api.send(ApiCommand::UpdateAnim {
                                auth,
                                id: anim_id,
                                req: UpdateAnimRequest {
                                    marker_id: updated.marker_id,
                                    name: updated.name.clone(),
                                    anim_type: updated.anim_type,
                                    sort_order: updated.sort_order,
                                    speed: updated.speed,
                                    duration_sec: updated.duration_sec,
                                    replay: updated.replay,
                                    autoplay: updated.autoplay,
                                    geom: updated.geom,
                                    data: updated.data,
                                },
                            });
                        }
                        state.status_banner = Some(format!(
                            "Panah Penunjuk 3D '{}' dipindahkan ke '{marker_name}' — live preview aktif!",
                            updated.name
                        ));
                    }
                } else if let Some(sk) = state.active_skenario.clone() {
                    let cfg = PointerArrowConfig::default();
                    let data = serde_json::json!({ "pointer_arrow": cfg });
                    let name = if state.new_pointer_name.trim().is_empty() {
                        format!("Panah — {marker_name}")
                    } else {
                        state.new_pointer_name.trim().to_string()
                    };
                    let sort_order = (state.anims.len() + 1) as i32;

                    if state.offline_preview {
                        let now = chrono::Utc::now();
                        let a = Anim {
                            id: Uuid::new_v4(),
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: Some(marker_id),
                            name: name.clone(),
                            anim_type: ANIM_TYPE_POINTER_ARROW.into(),
                            sort_order,
                            speed: 1.0,
                            duration_sec: 15.0,
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
                            "Panah Penunjuk 3D '{name}' dipasang di atas '{marker_name}' — atur teks, warna & skala di Inspector!"
                        ));
                    } else if let Some(auth) = current_auth(&state) {
                        api.send(ApiCommand::CreateAnim {
                            auth,
                            req: CreateAnimRequest {
                                operasi_id: sk.operasi_id,
                                skenario_id: sk.id,
                                marker_id: Some(marker_id),
                                name: name.clone(),
                                anim_type: ANIM_TYPE_POINTER_ARROW.into(),
                                sort_order,
                                speed: 1.0,
                                duration_sec: 15.0,
                                replay: false,
                                autoplay: false,
                                geom: Vec::new(),
                                data,
                            },
                        });
                    }
                    let next_idx = state
                        .anims
                        .iter()
                        .filter(|a| is_pointer_arrow_anim(a))
                        .count()
                        + 2;
                    state.new_pointer_name = format!("Panah Penunjuk {next_idx}");
                }
                state.tool_mode = ToolMode::Select;
                gizmo_state.suppress_camera_pan = true;
            }
        }
        ToolMode::PickSpawnHideMarker => {
            if playback.play_test_mode {
                return;
            }
            if let Some(SelectedEntity::Marker(marker_id)) = gizmo_state.hovered_entity
                && let Some(marker_name) = state
                    .markers
                    .iter()
                    .find(|m| m.id == marker_id)
                    .map(|m| m.name.clone())
            {
                if let Some(anim_id) = state.picking_spawn_hide_for_anim_id {
                    if let Some(anim) = state.anims.iter_mut().find(|a| a.id == anim_id) {
                        let mut cfg = parse_spawn_hide_config(anim);
                        let added = cfg.toggle_marker(marker_id);
                        write_spawn_hide_config(anim, &cfg);
                        let cnt = cfg.marker_ids.len();
                        let act_tag = cfg.action.short_tag();
                        let anim_name = anim.name.clone();
                        let updated = anim.clone();

                        state.selected = SelectedEntity::Anim(anim_id);
                        state.anim_edit = Some(updated.clone());
                        state.marker_edit = None;
                        state.building_edit = None;
                        state.line_edit = None;
                        state.text_edit = None;
                        state.dirty_anim_ids.insert(anim_id);

                        if !state.offline_preview
                            && let Some(auth) = current_auth(&state)
                        {
                            api.send(ApiCommand::UpdateAnim {
                                auth,
                                id: anim_id,
                                req: UpdateAnimRequest {
                                    marker_id: updated.marker_id,
                                    name: updated.name,
                                    anim_type: updated.anim_type,
                                    sort_order: updated.sort_order,
                                    speed: updated.speed,
                                    duration_sec: updated.duration_sec,
                                    replay: updated.replay,
                                    autoplay: updated.autoplay,
                                    geom: updated.geom,
                                    data: updated.data,
                                },
                            });
                        }

                        state.status_banner = Some(if added {
                            format!(
                                "[{act_tag}] '{marker_name}' ditambahkan (#{cnt}) ke '{anim_name}'. Klik objek lain untuk tambah/lepas, atau Enter / Esc selesai."
                            )
                        } else {
                            format!(
                                "[{act_tag}] '{marker_name}' dilepas dari '{anim_name}' (sisa {cnt} objek). Klik objek lain atau Enter / Esc selesai."
                            )
                        });
                    }
                } else if let Some(sk) = state.active_skenario.clone() {
                    // Fallback: create a new Spawn/Hide animation starting with this clicked marker
                    let action = state.new_spawn_hide_action;
                    let mut cfg = SpawnHideConfig::default_for(action);
                    cfg.marker_ids.push(marker_id);
                    let dur = cfg.total_duration_sec();
                    let data = serde_json::json!({ "spawn_hide": cfg });
                    let name = if state.new_spawn_hide_name.trim().is_empty() {
                        let idx = state
                            .anims
                            .iter()
                            .filter(|a| spawn_hide_action(a) == Some(action))
                            .count()
                            + 1;
                        action.default_name(idx)
                    } else {
                        state.new_spawn_hide_name.trim().to_string()
                    };
                    let sort_order = (state.anims.len() + 1) as i32;
                    let anim_type = action.anim_type_str().to_string();

                    if state.offline_preview {
                        let now = chrono::Utc::now();
                        let a = Anim {
                            id: Uuid::new_v4(),
                            operasi_id: sk.operasi_id,
                            skenario_id: sk.id,
                            marker_id: Some(marker_id),
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
                        state.status_banner = Some(format!(
                            "[{}] '{marker_name}' ditambahkan (#1) ke '{name}'. Klik objek lain untuk tambah/lepas, atau Enter / Esc selesai.",
                            action.short_tag()
                        ));
                    } else if let Some(auth) = current_auth(&state) {
                        api.send(ApiCommand::CreateAnim {
                            auth,
                            req: CreateAnimRequest {
                                operasi_id: sk.operasi_id,
                                skenario_id: sk.id,
                                marker_id: Some(marker_id),
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
                    let next_idx = state
                        .anims
                        .iter()
                        .filter(|a| spawn_hide_action(a) == Some(action))
                        .count()
                        + 2;
                    state.new_spawn_hide_name = action.default_name(next_idx);
                }
                gizmo_state.suppress_camera_pan = true;
            }
        }
        ToolMode::PlaceParticle => {
            if playback.play_test_mode {
                return;
            }
            let Some(hit) = hit_opt else {
                return;
            };
            let Some(sk) = state.active_skenario.clone() else {
                return;
            };
            let (lng, lat, _) = state.geo_anchor.local_to_lng_lat_alt([hit.x, 0.0, hit.z]);
            let kind = state.new_particle_kind;
            let cfg = ParticleAnimConfig::default_for(kind);
            let data = serde_json::json!({ "particle": cfg });
            let part_count = state
                .anims
                .iter()
                .filter(|a| is_particle_anim(a))
                .count()
                + 1;
            let name = if state.new_particle_name.trim().is_empty() {
                kind.default_name(part_count)
            } else {
                state.new_particle_name.trim().to_string()
            };
            let sort_order = (state.anims.len() + 1) as i32;
            let duration_sec = ParticleAnimConfig::default_duration_sec(kind);

            if state.offline_preview {
                let now = chrono::Utc::now();
                let a = Anim {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    marker_id: None,
                    name: name.clone(),
                    anim_type: kind.anim_type_str().into(),
                    sort_order,
                    speed: 1.0,
                    duration_sec,
                    replay: false,
                    autoplay: false,
                    geom: vec![[lng, lat, 0.0]],
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
                state.particle_preview_reset_nonce =
                    state.particle_preview_reset_nonce.wrapping_add(1);
                state.status_banner = Some(format!(
                    "Partikel '{name}' diletakkan — geser posisi via Move Gizmo (X / Alt / Z) atau ubah setting di Inspector!"
                ));
            } else if let Some(auth) = current_auth(&state) {
                api.send(ApiCommand::CreateAnim {
                    auth,
                    req: CreateAnimRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: None,
                        name: name.clone(),
                        anim_type: kind.anim_type_str().into(),
                        sort_order,
                        speed: 1.0,
                        duration_sec,
                        replay: false,
                        autoplay: false,
                        geom: vec![[lng, lat, 0.0]],
                        data,
                    },
                });
            }
            state.new_particle_name = kind.default_name(part_count + 1);
            state.tool_mode = ToolMode::Select;
            gizmo_state.suppress_camera_pan = true;
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
            let is_multi = state.new_route_is_multi;
            let is_patwal = is_multi && state.new_route_formation == RouteFormationKind::Patwal;
            let target_marker_id = if is_multi {
                state.route_target_marker_id = None;
                None
            } else {
                state.route_target_marker_id.take()
            };
            let target_alt = target_marker_id
                .and_then(|mid| state.markers.iter().find(|m| m.id == mid))
                .map(|m| m.altitude)
                .unwrap_or(0.0);
            let geom: Vec<[f64; 3]> = pts
                .into_iter()
                .map(|[lng, lat]| [lng, lat, target_alt])
                .collect();
            let name = if state.new_route_name.trim().is_empty() {
                if is_patwal {
                    format!("Rute Patwal {}", state.anims.len() + 1)
                } else if is_multi {
                    format!("Rute Formasi {}", state.anims.len() + 1)
                } else {
                    format!("Rute Unit {}", state.anims.len() + 1)
                }
            } else {
                state.new_route_name.trim().to_string()
            };
            let anim_type = if is_multi {
                ANIM_TYPE_ROUTE_MULTI.to_string()
            } else {
                ANIM_TYPE_ROUTE_SINGLE.to_string()
            };
            let data = if is_multi {
                let multi_cfg = MultiRouteConfig {
                    formation: state.new_route_formation,
                    ..Default::default()
                };
                serde_json::json!({
                    "multi_route": multi_cfg
                })
            } else {
                serde_json::json!({})
            };
            let sort_order = (state.anims.len() + 1) as i32;
            let speed = state.new_route_speed.max(0.1);
            let duration_sec = route_estimated_duration_sec(&geom, speed);
            let replay = state.new_route_loop;

            if state.offline_preview {
                let now = chrono::Utc::now();
                let a = Anim {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    marker_id: target_marker_id,
                    name: name.clone(),
                    anim_type,
                    sort_order,
                    speed,
                    duration_sec,
                    replay,
                    autoplay: false,
                    geom,
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
                if target_marker_id.is_none() {
                    state.picking_marker_for_anim_id = Some(aid);
                    state.tool_mode = ToolMode::PickRouteMarker;
                    state.status_banner = Some(if is_multi {
                        format!(
                            "Garis '{name}' dibuat! Klik beberapa Model 3D / Unit di peta untuk masuk formasi (Enter / Esc selesai)."
                        )
                    } else {
                        format!(
                            "Garis '{name}' dibuat! Sekarang klik Model 3D / Unit di peta untuk mengikat rute ini."
                        )
                    });
                } else {
                    state.tool_mode = ToolMode::Select;
                    state.status_banner = Some(
                        "Rute animasi ditambahkan! Tekan Play Skenario untuk menguji.".into(),
                    );
                }
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::CreateAnim {
                    auth,
                    req: CreateAnimRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: target_marker_id,
                        name,
                        anim_type,
                        sort_order,
                        speed,
                        duration_sec,
                        replay,
                        autoplay: false,
                        geom,
                        data,
                    },
                });
                state.tool_mode = ToolMode::Select;
            }
            // Prepare next default route name cleanly
            state.new_route_name = if is_patwal {
                format!("Rute Patwal {}", state.anims.len() + 2)
            } else if is_multi {
                format!("Rute Formasi {}", state.anims.len() + 2)
            } else {
                format!("Rute Unit {}", state.anims.len() + 2)
            };
        }
        ToolMode::DrawTrafficLane => {
            if pts.len() < 2 {
                state.status_banner = Some("Jalur Traffic butuh minimal 2 titik waypoint.".into());
                return;
            }
            let geom: Vec<[f64; 3]> = pts.into_iter().map(|[lng, lat]| [lng, lat, 0.0]).collect();

            if let Some(target_aid) = state.drawing_traffic_lane_for_anim_id.take() {
                if let Some(existing) = state
                    .anims
                    .iter_mut()
                    .find(|a| a.id == target_aid && is_traffic_anim(a))
                {
                    let mut cfg = parse_traffic_anim_config(existing);
                    let lane_num = cfg.lanes.len() + 1;
                    cfg.lanes
                        .push(TrafficLane::new(format!("Jalur {lane_num}"), geom));
                    write_traffic_anim_config(existing, &cfg);
                    let updated = existing.clone();
                    state.selected = SelectedEntity::Anim(target_aid);
                    state.anim_edit = Some(updated);
                    state.dirty_anim_ids.insert(target_aid);
                    state.status_banner = Some(format!(
                        "Jalur {lane_num} ditambahkan ke Simulasi Traffic! Klik Preview untuk menguji atau Simpan untuk menyimpan."
                    ));
                }
                state.tool_mode = ToolMode::Select;
                return;
            }

            let traffic_cnt = state.anims.iter().filter(|a| is_traffic_anim(a)).count() + 1;
            let name = if state.new_traffic_name.trim().is_empty() {
                format!("Traffic Lalu Lintas {traffic_cnt}")
            } else {
                state.new_traffic_name.trim().to_string()
            };

            // Pre-populate vehicle pool with up to 4 ready vehicle models from the 3D catalog so preview works immediately
            let mut auto_pool: Vec<TrafficVehicleEntry> = state
                .catalog
                .iter()
                .filter(|m| {
                    let cat = m.category.to_lowercase();
                    m.ready && (cat.contains("kendaraan") || cat.contains("vehicle"))
                })
                .take(4)
                .map(|m| TrafficVehicleEntry {
                    model_path: m.model_path.clone(),
                    name: m.name.clone(),
                    scale: 1.0,
                    length_m: TrafficVehicleEntry::estimate_length_m(&m.name, &m.model_path),
                    weight: 1.0,
                    rot_y_offset_deg: m.default_rotation_deg[1],
                })
                .collect();
            if auto_pool.is_empty() {
                // Fallback to any ready model if no vehicle category exists yet
                auto_pool = state
                    .catalog
                    .iter()
                    .filter(|m| m.ready)
                    .take(2)
                    .map(|m| TrafficVehicleEntry {
                        model_path: m.model_path.clone(),
                        name: m.name.clone(),
                        scale: 1.0,
                        length_m: TrafficVehicleEntry::estimate_length_m(&m.name, &m.model_path),
                        weight: 1.0,
                        rot_y_offset_deg: m.default_rotation_deg[1],
                    })
                    .collect();
            }

            let cfg = TrafficAnimConfig {
                lanes: vec![TrafficLane::new("Jalur 1", geom.clone())],
                vehicle_pool: auto_pool,
                seed: 20_261_126_u64.wrapping_add((state.anims.len() as u64 + 1) * 7919),
                ..Default::default()
            };
            let data = serde_json::json!({ "traffic": cfg });
            let sort_order = (state.anims.len() + 1) as i32;

            if state.offline_preview {
                let now = chrono::Utc::now();
                let a = Anim {
                    id: Uuid::new_v4(),
                    operasi_id: sk.operasi_id,
                    skenario_id: sk.id,
                    marker_id: None,
                    name: name.clone(),
                    anim_type: ANIM_TYPE_TRAFFIC_FLOW.to_string(),
                    sort_order,
                    speed: 10.0,
                    duration_sec: 60.0,
                    replay: true,
                    autoplay: false,
                    geom,
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
                state.status_banner = Some(format!(
                    "Simulasi Traffic '{name}' dibuat! Gunakan 'Duplikat Jalur Balik' atau atur pool kendaraan di Inspector."
                ));
            } else if let Some(auth) = current_auth(state) {
                api.send(ApiCommand::CreateAnim {
                    auth,
                    req: CreateAnimRequest {
                        operasi_id: sk.operasi_id,
                        skenario_id: sk.id,
                        marker_id: None,
                        name,
                        anim_type: ANIM_TYPE_TRAFFIC_FLOW.to_string(),
                        sort_order,
                        speed: 10.0,
                        duration_sec: 60.0,
                        replay: true,
                        autoplay: false,
                        geom,
                        data,
                    },
                });
            }
            state.tool_mode = ToolMode::Select;
            state.new_traffic_name = format!("Traffic Lalu Lintas {}", traffic_cnt + 1);
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
pub(crate) fn raycast_mesh_node(
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
    playback: &PlaybackState,
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
            let hidden_branch = if marker_comp.is_walking_now && marker_comp.walk_child.is_some() {
                marker_comp.idle_child
            } else {
                marker_comp.walk_child
            };
            stack.clear();
            stack.push(root_entity);
            while let Some(e) = stack.pop() {
                if Some(e) == hidden_branch {
                    continue;
                }
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

    // Priority 2: Click on floating unit label badge ("tulisan") — hidden in Play Skenario
    if state.show_unit_labels && !playback.play_test_mode {
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
                let meters_per_px = compute_gizmo_world_radius(
                    camera,
                    cam_gtf,
                    Vec3::from_array(local),
                    1.0,
                );
                let font_px = (tm.font_size as f32 / meters_per_px).clamp(3.0, 512.0);
                let half_w = (tm.text.chars().count() as f32 * font_px * 0.34 + font_px * 0.75)
                    .clamp(12.0, 2_048.0);
                let half_h = (font_px * 0.65 + 5.0).clamp(5.0, 512.0);
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

    // Priority 2c: Direct click on any visible, unlocked Particle Animation pin on the map
    if !playback.play_test_mode && state.show_gizmos {
        let mut best_part: Option<(Uuid, f32)> = None;
        for anim in &state.anims {
            if !is_particle_anim(anim)
                || anim.geom.is_empty()
                || state.hidden_entity_ids.contains(&anim.id)
                || state.locked_entity_ids.contains(&anim.id)
            {
                continue;
            }
            let p = particle_anim_position(anim);
            let local = state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], p[2]);
            if let Ok(scr) = camera.world_to_viewport(cam_gtf, Vec3::from_array(local)) {
                let d = cursor_px.distance(scr);
                if d <= 22.0 && best_part.is_none_or(|cur| d < cur.1) {
                    best_part = Some((anim.id, d));
                }
            }
        }
        if let Some((aid, _)) = best_part {
            return Some(SelectedEntity::Anim(aid));
        }
    }

    // Priority 2d: Direct click on any visible, unlocked Route Animation polyline or waypoint
    if !playback.play_test_mode
        && let Some(line_id) = hit_test_any_plot_line(cursor_px, state, camera, cam_gtf)
    {
        return Some(SelectedEntity::Line(line_id));
    }

    // Priority 2e: Direct click on any visible, unlocked Route Animation polyline or waypoint
    if !playback.play_test_mode
        && state.show_gizmos
        && let Some(aid) = hit_test_any_route_line(cursor_px, state, camera, cam_gtf)
    {
        return Some(SelectedEntity::Anim(aid));
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

pub use twg_shared::glb_skin::normalize_joint_weight;

fn mesh_needs_joint_weight_normalization(mesh: &Mesh) -> bool {
    let Ok(VertexAttributeValues::Float32x4(weights)) =
        mesh.try_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
    else {
        return false;
    };
    weights
        .iter()
        .any(|&w| twg_shared::glb_skin::joint_weight_needs_normalization(w))
}

/// Normalizes `Mesh::ATTRIBUTE_JOINT_WEIGHT` in-place and returns the number of vertices repaired.
pub fn normalize_mesh_joint_weights(mesh: &mut Mesh) -> usize {
    let Ok(VertexAttributeValues::Float32x4(weights)) =
        mesh.try_attribute_mut(Mesh::ATTRIBUTE_JOINT_WEIGHT)
    else {
        return 0;
    };
    let mut fixed_count = 0usize;
    for w in weights.iter_mut() {
        let (norm, changed) = normalize_joint_weight(*w);
        if changed {
            *w = norm;
            fixed_count += 1;
        }
    }
    fixed_count
}

/// Automatically repairs unnormalized `WEIGHTS_0` attributes on ALL loaded skinned glTF meshes
/// strictly BEFORE `AssetEventSystems` and `ExtractSchedule` upload them to the GPU.
///
/// Scanning `Assets<Mesh>::ids()` directly in `PostUpdate.before(AssetEventSystems)` guarantees
/// zero missed sub-assets, zero race conditions with `MeshAllocator` slab offsets, and works
/// for every existing and future uploaded `.glb` model (e.g. `brimob_walk.glb`, `babin_walk.glb`).
pub fn normalize_skinned_mesh_weights_system(
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh3d_q: Query<&mut Mesh3d>,
    mut checked: Local<HashSet<AssetId<Mesh>>>,
) {
    for ev in mesh_events.read() {
        match *ev {
            AssetEvent::Removed { id }
            | AssetEvent::Unused { id }
            | AssetEvent::Modified { id } => {
                checked.remove(&id);
            }
            AssetEvent::Added { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }

    let unchecked: Vec<AssetId<Mesh>> = meshes
        .ids()
        .filter(|id| !checked.contains(id))
        .collect();

    if unchecked.is_empty() {
        return;
    }

    let mut repaired_ids: HashSet<AssetId<Mesh>> = HashSet::new();
    for id in unchecked {
        let Some(mesh_ro) = meshes.get(id) else {
            continue;
        };
        let needs_fix = mesh_needs_joint_weight_normalization(mesh_ro);
        checked.insert(id);
        if needs_fix
            && let Some(mut mesh_mut) = meshes.get_mut(id)
        {
            let repaired = normalize_mesh_joint_weights(&mut mesh_mut);
            if repaired > 0 {
                repaired_ids.insert(id);
                bevy::log::info!(
                    "Normalized {repaired} unnormalized skinning vertex weights on mesh {id:?}"
                );
            }
        }
    }

    // If any entity already referenced a repaired mesh (e.g. after hot-reload), mark its Mesh3d
    // changed so `bevy_pbr` immediately refreshes the entity's GPU slab vertex offset.
    if !repaired_ids.is_empty() {
        for mut mesh3d in &mut mesh3d_q {
            if repaired_ids.contains(&mesh3d.0.id()) {
                mesh3d.set_changed();
            }
        }
    }
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

    #[test]
    fn normalize_joint_weights_fixes_unnormalized_and_zero_weights() {
        // Exact v8730 weight from brimob_walk.glb (sum = 0.92791225 -> caused world-origin spike)
        let (norm, changed) =
            normalize_joint_weight([0.6743563, 0.1148743, 0.07808124, 0.06060036]);
        assert!(changed);
        let sum: f32 = norm.iter().sum();
        assert!((sum - 1.0).abs() <= 1e-6, "expected unit sum, got {sum}");
        assert!(norm[0] > norm[1] && norm[1] > norm[2] && norm[2] > norm[3]);

        // Zero-weight vertex must bind to joint 0 instead of collapsing to world origin (0,0,0)
        let (z_norm, z_changed) = normalize_joint_weight([0.0, 0.0, 0.0, 0.0]);
        assert!(z_changed);
        assert_eq!(z_norm, [1.0, 0.0, 0.0, 0.0]);

        // Negative / NaN components are sanitized and normalized
        let (bad_norm, bad_changed) = normalize_joint_weight([-0.2, f32::NAN, 0.4, 0.4]);
        assert!(bad_changed);
        assert!((bad_norm.iter().sum::<f32>() - 1.0).abs() <= 1e-6);
        assert_eq!(bad_norm[0], 0.0);
        assert_eq!(bad_norm[1], 0.0);

        // Already normalized weights remain untouched
        let (ok_norm, ok_changed) = normalize_joint_weight([0.6, 0.4, 0.0, 0.0]);
        assert!(!ok_changed);
        assert_eq!(ok_norm, [0.6, 0.4, 0.0, 0.0]);

        // Full Bevy Mesh repair test (including babin_walk.glb v4965 sum=0.9544)
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            vec![
                [0.6743563, 0.1148743, 0.07808124, 0.06060036],
                [0.5, 0.5, 0.0, 0.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.800534, 0.07298717, 0.06364223, 0.01723978],
            ],
        );
        assert!(mesh_needs_joint_weight_normalization(&mesh));
        let fixed = normalize_mesh_joint_weights(&mut mesh);
        assert_eq!(fixed, 3);
        assert!(!mesh_needs_joint_weight_normalization(&mesh));

        // Verify PostUpdate system repairs newly inserted Mesh BEFORE AssetEventSystems flush
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.add_systems(
            PostUpdate,
            normalize_skinned_mesh_weights_system.before(bevy::asset::AssetEventSystems),
        );
        let mut bad_mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        bad_mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            vec![[0.800534, 0.07298717, 0.06364223, 0.01723978]],
        );
        let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(bad_mesh);
        app.update();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let repaired_mesh = meshes.get(&handle).expect("mesh exists");
        assert!(
            !mesh_needs_joint_weight_normalization(repaired_mesh),
            "Mesh must be normalized on its very first frame before GPU extraction"
        );
    }
}
