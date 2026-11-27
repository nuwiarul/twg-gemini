use bevy::{
    camera::primitives::Aabb,
    gltf::Gltf,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};
use twg_shared::models::{ModelAssetDto, ModelAssetType};

use crate::{
    map::camera::TacticalCamera,
    net::asset_sync::{AssetSyncBridge, AssetSyncCommand},
    state::{
        AppScreen, AppState, AssetPreviewLod, AssetPreviewVariant, AssetsSection, UiPointerState,
    },
};

#[derive(Component)]
pub struct PreviewStageFloor;

#[derive(Component)]
pub struct PreviewModelTurntable;

#[derive(Component)]
pub struct PreviewModelInner;

#[derive(Resource)]
pub struct AssetPreviewStage {
    pub turntable_entity: Option<Entity>,
    pub inner_entity: Option<Entity>,
    pub loaded_path: Option<String>,
    pub loaded_nonce: u64,
    pub gltf_handle: Option<Handle<Gltf>>,
    pub anim_started: bool,
    pub framed: bool,
    pub frame_wait_ticks: u32,
    pub priority_requested_path: Option<String>,

    // Orbit camera state
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub distance: f32,
    pub target_y: f32,
    pub model_radius: f32,
    pub model_spin_rad: f32,
    pub reset_camera_requested: bool,
}

impl Default for AssetPreviewStage {
    fn default() -> Self {
        Self {
            turntable_entity: None,
            inner_entity: None,
            loaded_path: None,
            loaded_nonce: 0,
            gltf_handle: None,
            anim_started: false,
            framed: false,
            frame_wait_ticks: 0,
            priority_requested_path: None,
            yaw_deg: 35.0,
            pitch_deg: 22.0,
            distance: 6.0,
            target_y: 1.0,
            model_radius: 1.5,
            model_spin_rad: 0.0,
            reset_camera_requested: false,
        }
    }
}

pub fn resolve_preview_file_path(
    asset: &ModelAssetDto,
    variant: &AssetPreviewVariant,
    lod: AssetPreviewLod,
) -> String {
    match (asset.model_type, variant, lod) {
        (ModelAssetType::Animated, AssetPreviewVariant::Action(slug), AssetPreviewLod::Lod2) => {
            if let Some(act) = asset.extra_animations.iter().find(|a| a.slug == *slug) {
                act.lod2_file
                    .as_ref()
                    .unwrap_or(&act.file)
                    .rel_path
                    .clone()
            } else {
                asset
                    .lod2_file
                    .as_ref()
                    .unwrap_or(&asset.model_file)
                    .rel_path
                    .clone()
            }
        }
        (ModelAssetType::Animated, AssetPreviewVariant::Action(slug), AssetPreviewLod::Lod0) => {
            if let Some(act) = asset.extra_animations.iter().find(|a| a.slug == *slug) {
                act.file.rel_path.clone()
            } else {
                asset.model_file.rel_path.clone()
            }
        }
        (ModelAssetType::Animated, AssetPreviewVariant::Walk, AssetPreviewLod::Lod2) => asset
            .walk_lod2_file
            .as_ref()
            .or(asset.walk_file.as_ref())
            .or(asset.lod2_file.as_ref())
            .unwrap_or(&asset.model_file)
            .rel_path
            .clone(),
        (ModelAssetType::Animated, AssetPreviewVariant::Walk, AssetPreviewLod::Lod0) => asset
            .walk_file
            .as_ref()
            .unwrap_or(&asset.model_file)
            .rel_path
            .clone(),
        (_, _, AssetPreviewLod::Lod2) => asset
            .lod2_file
            .as_ref()
            .unwrap_or(&asset.model_file)
            .rel_path
            .clone(),
        (_, _, AssetPreviewLod::Lod0) => asset.model_file.rel_path.clone(),
    }
}

pub fn setup_asset_preview_stage(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let disc = meshes.add(Cylinder::new(4.5, 0.06));
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.11, 0.17),
        perceptual_roughness: 0.85,
        metallic: 0.15,
        ..default()
    });
    commands.spawn((
        Mesh3d(disc),
        MeshMaterial3d(mat),
        Transform::from_xyz(0.0, -0.035, 0.0),
        Visibility::Hidden,
        PreviewStageFloor,
    ));
}

#[allow(clippy::too_many_arguments)]
pub fn asset_preview_stage_system(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<AppState>,
    mut stage: ResMut<AssetPreviewStage>,
    asset_server: Res<AssetServer>,
    sync_bridge: Res<AssetSyncBridge>,
    gltfs: Res<Assets<Gltf>>,
    mut anim_graphs: ResMut<Assets<AnimationGraph>>,
    mut gizmos: Gizmos,
    mut floor_q: Query<(&mut Transform, &mut Visibility), (With<PreviewStageFloor>, Without<PreviewModelTurntable>, Without<PreviewModelInner>)>,
    mut turntable_q: Query<&mut Transform, (With<PreviewModelTurntable>, Without<PreviewModelInner>)>,
    mut inner_q: Query<&mut Transform, With<PreviewModelInner>>,
    children_q: Query<&Children>,
    aabb_q: Query<(&Aabb, &GlobalTransform)>,
    mut anim_player_q: Query<(Entity, &mut AnimationPlayer)>,
) {
    if state.screen != AppScreen::Assets || state.assets_section != AssetsSection::Models3d {
        if let Ok((_, mut vis)) = floor_q.single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Some(ent) = stage.turntable_entity.take() {
            commands.entity(ent).despawn();
        }
        stage.inner_entity = None;
        stage.loaded_path = None;
        stage.gltf_handle = None;
        stage.anim_started = false;
        stage.framed = false;
        return;
    }

    // Show studio pedestal & scale it gracefully to the framed model radius
    let pad_scale = (stage.model_radius * 0.45).clamp(0.4, 14.0);
    if let Ok((mut floor_tf, mut vis)) = floor_q.single_mut() {
        *vis = Visibility::Visible;
        floor_tf.scale = Vec3::new(pad_scale, 1.0, pad_scale);
    }

    // Studio grid + subtle axis gizmos
    let grid_step = (stage.model_radius * 0.5).clamp(0.5, 10.0);
    let half = 6;
    let span = grid_step * half as f32;
    let minor = Color::srgba(0.32, 0.45, 0.62, 0.25);
    let major = Color::srgba(0.22, 0.74, 0.97, 0.45);
    for i in -half..=half {
        let off = i as f32 * grid_step;
        let col = if i == 0 { major } else { minor };
        gizmos.line(Vec3::new(off, 0.005, -span), Vec3::new(off, 0.005, span), col);
        gizmos.line(Vec3::new(-span, 0.005, off), Vec3::new(span, 0.005, off), col);
    }
    gizmos.circle(
        Isometry3d::new(
            Vec3::new(0.0, 0.01, 0.0),
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        ),
        (4.5 * pad_scale).max(1.2),
        Color::srgba(0.22, 0.74, 0.97, 0.55),
    );

    let Some(asset) = state.selected_asset() else {
        if let Some(ent) = stage.turntable_entity.take() {
            commands.entity(ent).despawn();
        }
        stage.inner_entity = None;
        stage.loaded_path = None;
        stage.gltf_handle = None;
        return;
    };

    let target_rel_path = resolve_preview_file_path(asset, &state.preview_variant, state.preview_lod);
    let file_ready_locally = state.available_model_paths.contains(&target_rel_path)
        || state.assets_root.join(&target_rel_path).exists();

    if !file_ready_locally {
        // Request priority download if not already requested for this path
        if stage.priority_requested_path.as_deref() != Some(target_rel_path.as_str()) {
            stage.priority_requested_path = Some(target_rel_path.clone());
            let files: Vec<_> = asset.all_files().into_iter().cloned().collect();
            sync_bridge.send(AssetSyncCommand::PriorityDownload {
                server_url: state.server_url.clone(),
                local_root: state.assets_root.clone(),
                files,
            });
        }
        if let Some(ent) = stage.turntable_entity.take() {
            commands.entity(ent).despawn();
        }
        stage.inner_entity = None;
        stage.loaded_path = None;
        stage.gltf_handle = None;
        return;
    }

    let needs_spawn = stage.loaded_path.as_deref() != Some(target_rel_path.as_str())
        || stage.loaded_nonce != state.preview_reload_nonce
        || stage.turntable_entity.is_none();

    if needs_spawn {
        if let Some(ent) = stage.turntable_entity.take() {
            commands.entity(ent).despawn();
        }

        let gltf_handle: Handle<Gltf> = asset_server.load(target_rel_path.clone());
        let scene_handle =
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(target_rel_path.clone()));

        let mut inner_id = None;
        let turntable_id = commands
            .spawn((
                Transform::IDENTITY,
                Visibility::Visible,
                PreviewModelTurntable,
            ))
            .with_children(|parent| {
                let id = parent
                    .spawn((
                        Transform::IDENTITY,
                        Visibility::Visible,
                        PreviewModelInner,
                        WorldAssetRoot(scene_handle),
                    ))
                    .id();
                inner_id = Some(id);
            })
            .id();

        stage.turntable_entity = Some(turntable_id);
        stage.inner_entity = inner_id;
        stage.loaded_path = Some(target_rel_path);
        stage.loaded_nonce = state.preview_reload_nonce;
        stage.gltf_handle = Some(gltf_handle);
        stage.anim_started = false;
        stage.framed = false;
        stage.frame_wait_ticks = 0;
        stage.model_spin_rad = 0.0;
        return;
    }

    // Auto-frame once child mesh AABBs are populated
    if !stage.framed
        && let Some(inner_ent) = stage.inner_entity
    {
        stage.frame_wait_ticks += 1;
        if stage.frame_wait_ticks >= 2 {
            let mut min_pt = Vec3::splat(f32::INFINITY);
            let mut max_pt = Vec3::splat(f32::NEG_INFINITY);
            let mut found = false;

            let mut stack = vec![inner_ent];
            while let Some(e) = stack.pop() {
                if let Ok((aabb, gtf)) = aabb_q.get(e) {
                    let c = Vec3::from(aabb.center);
                    let he = Vec3::from(aabb.half_extents);
                    let m = gtf.to_matrix();
                    for sx in [-1.0_f32, 1.0] {
                        for sy in [-1.0_f32, 1.0] {
                            for sz in [-1.0_f32, 1.0] {
                                let corner = m.transform_point3(c + Vec3::new(sx * he.x, sy * he.y, sz * he.z));
                                min_pt = min_pt.min(corner);
                                max_pt = max_pt.max(corner);
                                found = true;
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

            if found && min_pt.is_finite() && max_pt.is_finite() {
                let size = (max_pt - min_pt).max(Vec3::splat(0.05));
                let center = (min_pt + max_pt) * 0.5;
                if let Ok(mut inner_tf) = inner_q.get_mut(inner_ent) {
                    // Place geometric X/Z center at origin and bottom cleanly at Y = 0
                    inner_tf.translation = Vec3::new(-center.x, -min_pt.y, -center.z);
                }
                let radius = (size.length() * 0.5).max(0.35);
                stage.model_radius = radius;
                stage.target_y = (size.y * 0.48).max(0.2);
                stage.distance = (radius * 2.55).clamp(1.2, 180.0);
                stage.yaw_deg = 35.0;
                stage.pitch_deg = 20.0;
                stage.framed = true;
            }
        }
    }

    // Attach & drive glTF AnimationPlayer if the loaded GLB has animations (e.g. *_walk.glb)
    if let Some(inner_ent) = stage.inner_entity
        && let Some(gltf_handle) = &stage.gltf_handle
        && let Some(gltf) = gltfs.get(gltf_handle)
        && !gltf.animations.is_empty()
    {
        let mut stack = vec![inner_ent];
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

        if !player_entities.is_empty() {
            if !stage.anim_started {
                let (graph, node_idx) = AnimationGraph::from_clip(gltf.animations[0].clone());
                let graph_handle = anim_graphs.add(graph);
                for pe in &player_entities {
                    commands.entity(*pe).insert(AnimationGraphHandle(graph_handle.clone()));
                    if let Ok((_, mut player)) = anim_player_q.get_mut(*pe) {
                        player.play(node_idx).repeat().set_speed(state.preview_anim_speed);
                    }
                }
                stage.anim_started = true;
            } else {
                for pe in &player_entities {
                    if let Ok((_, mut player)) = anim_player_q.get_mut(*pe) {
                        for (_, active) in player.playing_animations_mut() {
                            active.set_speed(state.preview_anim_speed);
                            if state.preview_anim_playing {
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

    // Smooth turntable rotation
    if state.preview_auto_rotate
        && let Some(turn_ent) = stage.turntable_entity
        && let Ok(mut tf) = turntable_q.get_mut(turn_ent)
    {
        stage.model_spin_rad = (stage.model_spin_rad + time.delta_secs() * 0.45) % std::f32::consts::TAU;
        tf.rotation = Quat::from_rotation_y(stage.model_spin_rad);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn asset_preview_camera_system(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut motion_ev: MessageReader<MouseMotion>,
    mut wheel_ev: MessageReader<MouseWheel>,
    ui_state: Res<UiPointerState>,
    state: Res<AppState>,
    mut stage: ResMut<AssetPreviewStage>,
    mut cam_q: Query<&mut Transform, With<TacticalCamera>>,
) {
    if state.screen != AppScreen::Assets || state.assets_section != AssetsSection::Models3d {
        motion_ev.clear();
        wheel_ev.clear();
        return;
    }

    let Ok(mut cam_tf) = cam_q.single_mut() else {
        return;
    };

    let mut total_motion = Vec2::ZERO;
    for ev in motion_ev.read() {
        total_motion += ev.delta;
    }
    let mut scroll_y = 0.0_f32;
    for ev in wheel_ev.read() {
        scroll_y += ev.y;
    }

    if stage.reset_camera_requested
        || (!ui_state.wants_keyboard && keys.just_pressed(KeyCode::KeyR))
    {
        stage.reset_camera_requested = false;
        stage.yaw_deg = 35.0;
        stage.pitch_deg = 20.0;
        stage.distance = (stage.model_radius * 2.55).clamp(1.2, 180.0);
        stage.model_spin_rad = 0.0;
    }

    if !ui_state.wants_pointer {
        if scroll_y.abs() > 0.0 {
            let factor = (1.0 - scroll_y * 0.12).clamp(0.45, 1.75);
            let min_d = (stage.model_radius * 0.65).max(0.4);
            let max_d = (stage.model_radius * 12.0).max(40.0);
            stage.distance = (stage.distance * factor).clamp(min_d, max_d);
        }
        if mouse_buttons.pressed(MouseButton::Left) {
            stage.yaw_deg = (stage.yaw_deg - total_motion.x * 0.42) % 360.0;
            stage.pitch_deg = (stage.pitch_deg + total_motion.y * 0.35).clamp(-45.0, 82.0);
        }
        if mouse_buttons.pressed(MouseButton::Right) || mouse_buttons.pressed(MouseButton::Middle) {
            let pan_scale = stage.distance * 0.0025;
            stage.target_y = (stage.target_y + total_motion.y * pan_scale)
                .clamp(-stage.model_radius * 2.0, stage.model_radius * 4.0);
        }
    }

    let yaw = stage.yaw_deg.to_radians();
    let pitch = stage.pitch_deg.to_radians();
    let horiz = stage.distance * pitch.cos();
    let y = stage.target_y + stage.distance * pitch.sin();
    let target = Vec3::new(0.0, stage.target_y, 0.0);
    cam_tf.translation = Vec3::new(yaw.sin() * horiz, y, yaw.cos() * horiz);
    cam_tf.look_at(target, Vec3::Y);
}
