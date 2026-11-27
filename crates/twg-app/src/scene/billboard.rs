//! Procedural 3D Billboard / Videotron renderer + Image/Video playback system for Bevy.

use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bevy::{
    asset::RenderAssetUsages,
    audio::{PlaybackMode, Volume},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use crossbeam_channel::{Receiver, bounded};
use twg_shared::{
    billboard_anim::{
        BillboardAnimConfig, BillboardIdleMode, BillboardMediaMode, BillboardStructureKind,
        is_billboard_anim, parse_billboard_anim_config,
    },
    group_anim::collect_group_descendant_ids,
    models::{Anim, MediaAssetType},
};
use uuid::Uuid;

use crate::{
    net::asset_sync::{AssetSyncBridge, AssetSyncCommand},
    scene::{
        media_preview::{DecodedVideoFrame, check_command_exists, probe_video_stream, silent_command},
        parse_hex_color,
    },
    state::{AnimRunStatus, AppState, PlaybackState},
};

#[derive(Component)]
pub struct BillboardVisualEntity;

#[derive(Component)]
pub struct BillboardAudioEntity;

pub fn billboard_world_origin(state: &AppState, anim: &Anim) -> Vec3 {
    let cfg = parse_billboard_anim_config(anim);
    Vec3::from_array(state.geo_anchor.lng_lat_alt_to_local(
        cfg.pos_lnglat[0],
        cfg.pos_lnglat[1],
        cfg.pos_lnglat[2],
    ))
}

pub fn billboard_quat(cfg: &BillboardAnimConfig) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        (cfg.rot_deg[1] as f32).to_radians(),
        (cfg.rot_deg[0] as f32).to_radians(),
        (cfg.rot_deg[2] as f32).to_radians(),
    )
}

/// Screen-space hit-test against all visible, unlocked 3D Billboards on the map.
pub fn hit_test_billboard(
    cursor_px: Vec2,
    state: &AppState,
    camera: &Camera,
    cam_gtf: &GlobalTransform,
) -> Option<Uuid> {
    let mut best: Option<(Uuid, f32)> = None;
    for anim in &state.anims {
        if !is_billboard_anim(anim)
            || state.hidden_entity_ids.contains(&anim.id)
            || state.locked_entity_ids.contains(&anim.id)
        {
            continue;
        }
        let cfg = parse_billboard_anim_config(anim);
        let base = billboard_world_origin(state, anim);
        let rot = billboard_quat(&cfg);
        let sc = Vec3::new(
            cfg.scale[0] as f32,
            cfg.scale[1] as f32,
            cfg.scale[2] as f32,
        );
        let w = cfg.screen_width_m as f32;
        let h = cfg.screen_height_m as f32;
        let pole_h = match cfg.structure {
            BillboardStructureKind::WallMount => 0.0,
            BillboardStructureKind::Monopole | BillboardStructureKind::TwinPole => {
                cfg.pole_height_m as f32
            }
        };
        let cy = pole_h + h * 0.5;
        let hw = w * 0.5;
        let hh = h * 0.5;
        // Project 4 screen corners + base footing for generous, accurate hit testing
        let local_pts = [
            Vec3::new(-hw, cy - hh, 0.0),
            Vec3::new(hw, cy - hh, 0.0),
            Vec3::new(hw, cy + hh, 0.0),
            Vec3::new(-hw, cy + hh, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, cy, 0.0),
        ];
        let mut min_2d = Vec2::splat(f32::INFINITY);
        let mut max_2d = Vec2::splat(f32::NEG_INFINITY);
        let mut any_ok = false;
        let mut center_dist = f32::INFINITY;
        for (idx, lp) in local_pts.iter().enumerate() {
            let wp = base + rot * (*lp * sc);
            if let Ok(p2) = camera.world_to_viewport(cam_gtf, wp) {
                if idx < 4 {
                    min_2d = min_2d.min(p2);
                    max_2d = max_2d.max(p2);
                    any_ok = true;
                }
                if idx == 5 {
                    center_dist = cursor_px.distance(p2);
                }
            }
        }
        if any_ok {
            let pad = 10.0;
            if cursor_px.x >= min_2d.x - pad
                && cursor_px.x <= max_2d.x + pad
                && cursor_px.y >= min_2d.y - pad
                && cursor_px.y <= max_2d.y + pad
            {
                let score = center_dist.min(cursor_px.distance((min_2d + max_2d) * 0.5));
                if best.is_none_or(|b| score < b.1) {
                    best = Some((anim.id, score));
                }
            }
        }
    }
    best.map(|(id, _)| id)
}

type LoadedImageBytes = Result<(u32, u32, Vec<u8>), String>;
type LoadedVideoAudio = Result<Option<(Arc<[u8]>, f64)>, String>;

enum CachedStaticTex {
    Loading(Receiver<LoadedImageBytes>),
    Ready(Handle<Image>),
    #[allow(dead_code)]
    Failed(String),
}

enum CachedVideoAudio {
    Loading(Receiver<LoadedVideoAudio>),
    Ready(Handle<AudioSource>, f64),
    NoAudio,
    Failed(String),
}

struct ActiveBillboardDecoder {
    stop_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    frame_rx: Receiver<DecodedVideoFrame>,
    ended_flag: Arc<AtomicBool>,
    duration_sec: f64,
}

impl ActiveBillboardDecoder {
    fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.pause_flag.store(false, Ordering::SeqCst);
    }
}

impl Drop for ActiveBillboardDecoder {
    fn drop(&mut self) {
        self.stop();
    }
}

struct BillboardVideoRun {
    trigger: u64,
    config: BillboardAnimConfig,
    delay_elapsed: f64,
    plays_done: u32,
    current_pts: f64,
    decoder: Option<ActiveBillboardDecoder>,
    audio_entity: Option<Entity>,
    audio_spawned_for_loop: bool,
    video_tex: Option<Handle<Image>>,
    video_tex_dims: (u32, u32),
    has_decoded_frame: bool,
    waiting_since: Instant,
    download_requested: bool,
}

struct BillboardVisualSlot {
    root: Entity,
    geom_sig: u64,
    screen_mat: Handle<StandardMaterial>,
    last_bound_tex: Option<Handle<Image>>,
    last_brightness: f32,
    last_is_lit: bool,
}

#[derive(Resource)]
pub struct BillboardRuntimePool {
    ffmpeg_available: bool,
    session_generation: Option<u64>,
    slots: HashMap<Uuid, BillboardVisualSlot>,
    runs: HashMap<Uuid, BillboardVideoRun>,
    last_frame_tex: HashMap<Uuid, Handle<Image>>,
    static_tex_cache: HashMap<String, CachedStaticTex>,
    video_audio_cache: HashMap<String, CachedVideoAudio>,
    download_requested_keys: HashSet<String>,
}

impl Default for BillboardRuntimePool {
    fn default() -> Self {
        Self {
            ffmpeg_available: check_command_exists("ffmpeg") && check_command_exists("ffprobe"),
            session_generation: None,
            slots: HashMap::new(),
            runs: HashMap::new(),
            last_frame_tex: HashMap::new(),
            static_tex_cache: HashMap::new(),
            video_audio_cache: HashMap::new(),
            download_requested_keys: HashSet::new(),
        }
    }
}

fn geometry_signature(cfg: &BillboardAnimConfig) -> u64 {
    let mut h = DefaultHasher::new();
    (cfg.structure as u8).hash(&mut h);
    cfg.screen_width_m.to_bits().hash(&mut h);
    cfg.screen_height_m.to_bits().hash(&mut h);
    cfg.pole_height_m.to_bits().hash(&mut h);
    cfg.double_sided.hash(&mut h);
    cfg.frame_color_hex.hash(&mut h);
    h.finish()
}

fn make_bevy_rgba_image(w: u32, h: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn load_image_rgba_file(path: PathBuf) -> LoadedImageBytes {
    let img = image::open(&path).map_err(|e| format!("Gagal memuat gambar: {e}"))?;
    let max_dim = 1280u32;
    let img = if img.width() > max_dim || img.height() > max_dim {
        img.resize(max_dim, max_dim, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((w, h, rgba.into_raw()))
}

fn extract_video_first_frame_rgba(path: PathBuf) -> LoadedImageBytes {
    let probe = probe_video_stream(&path);
    let src_w = probe.width.unwrap_or(640).max(16);
    let src_h = probe.height.unwrap_or(360).max(16);
    let scale_f = (720.0_f32 / src_w as f32).min(1.0);
    let out_w = ((src_w as f32 * scale_f) as u32 / 2 * 2).max(16);
    let out_h = ((src_h as f32 * scale_f) as u32 / 2 * 2).max(16);
    let output = silent_command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={out_w}:{out_h}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .output()
        .map_err(|e| format!("FFmpeg thumbnail gagal: {e}"))?;
    let expected = (out_w as usize) * (out_h as usize) * 4;
    if !output.status.success() || output.stdout.len() < expected {
        return Err("Gagal mengambil frame awal video.".into());
    }
    let mut bytes = output.stdout;
    bytes.truncate(expected);
    Ok((out_w, out_h, bytes))
}

fn extract_video_audio_wav(path: PathBuf) -> LoadedVideoAudio {
    let probe = probe_video_stream(&path);
    if !probe.has_audio {
        return Ok(None);
    }
    const SAMPLE_RATE: u32 = 44_100;
    const CHANNELS: u16 = 2;
    let output = silent_command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&path)
        .args([
            "-vn",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-ar",
            "44100",
            "-ac",
            "2",
            "pipe:1",
        ])
        .output()
        .map_err(|e| format!("Ekstrak audio video gagal: {e}"))?;
    if !output.status.success() || output.stdout.is_empty() {
        return Ok(None);
    }
    let wav = super::media_preview::wrap_pcm_s16le_as_wav(&output.stdout, SAMPLE_RATE, CHANNELS);
    let data_bytes = wav.len().saturating_sub(44);
    let duration_sec = data_bytes as f64 / (SAMPLE_RATE as f64 * CHANNELS as f64 * 2.0);
    if duration_sec <= 0.001 || !duration_sec.is_finite() {
        return Ok(None);
    }
    let bytes: Arc<[u8]> = wav.into();
    // Validate with rodio before passing to Bevy AudioSource
    rodio::Decoder::try_from(Cursor::new(bytes.clone()))
        .map_err(|e| format!("Audio video tidak valid: {e}"))?;
    Ok(Some((bytes, duration_sec)))
}

fn spawn_video_decoder_thread(abs: PathBuf) -> ActiveBillboardDecoder {
    let probe = probe_video_stream(&abs);
    let src_w = probe.width.unwrap_or(640).max(16);
    let src_h = probe.height.unwrap_or(360).max(16);
    let scale_f = (720.0_f32 / src_w as f32).min(1.0);
    let out_w = ((src_w as f32 * scale_f) as u32 / 2 * 2).max(16);
    let out_h = ((src_h as f32 * scale_f) as u32 / 2 * 2).max(16);
    let fps = probe.fps.clamp(5.0, 60.0);
    let duration_sec = probe.duration_sec.unwrap_or(0.0);

    let stop_flag = Arc::new(AtomicBool::new(false));
    let pause_flag = Arc::new(AtomicBool::new(false));
    let ended_flag = Arc::new(AtomicBool::new(false));
    let (frame_tx, frame_rx) = bounded::<DecodedVideoFrame>(3);

    let stop_c = Arc::clone(&stop_flag);
    let pause_c = Arc::clone(&pause_flag);
    let ended_c = Arc::clone(&ended_flag);

    std::thread::Builder::new()
        .name("twg-billboard-video".into())
        .spawn(move || {
            let mut cmd = silent_command("ffmpeg");
            cmd.args(["-v", "quiet", "-nostdin", "-i"])
                .arg(&abs)
                .args([
                    "-vf",
                    &format!("scale={out_w}:{out_h}"),
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                    "pipe:1",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());

            let Ok(mut child) = cmd.spawn() else {
                ended_c.store(true, Ordering::SeqCst);
                return;
            };
            let Some(mut stdout) = child.stdout.take() else {
                let _ = child.kill();
                let _ = child.wait();
                ended_c.store(true, Ordering::SeqCst);
                return;
            };

            let frame_bytes = (out_w as usize) * (out_h as usize) * 4;
            let frame_dur = Duration::from_secs_f64(1.0 / (fps as f64));
            let mut frame_idx = 0u64;
            let mut stream_start = Instant::now();
            let mut total_paused = Duration::ZERO;

            loop {
                if stop_c.load(Ordering::Relaxed) {
                    break;
                }
                if pause_c.load(Ordering::Relaxed) {
                    let p_start = Instant::now();
                    while pause_c.load(Ordering::Relaxed) && !stop_c.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    total_paused += p_start.elapsed();
                    continue;
                }

                let mut buf = vec![0u8; frame_bytes];
                if stdout.read_exact(&mut buf).is_err() {
                    break;
                }

                let target_time = frame_dur.mul_f64(frame_idx as f64);
                let elapsed = stream_start.elapsed().saturating_sub(total_paused);
                if target_time > elapsed {
                    let wait = target_time - elapsed;
                    if wait > Duration::from_millis(2) {
                        std::thread::sleep(wait.min(Duration::from_millis(120)));
                    }
                } else if elapsed.saturating_sub(target_time) > Duration::from_secs(2) {
                    stream_start = Instant::now();
                    total_paused = Duration::ZERO;
                    frame_idx = 0;
                }

                let pts_sec = frame_idx as f32 / fps;
                frame_idx += 1;

                let frame = DecodedVideoFrame {
                    width: out_w as usize,
                    height: out_h as usize,
                    rgba: buf,
                    pts_sec,
                };
                if frame_tx.send(frame).is_err() {
                    break;
                }
            }

            let _ = child.kill();
            let _ = child.wait();
            ended_c.store(true, Ordering::SeqCst);
        })
        .ok();

    ActiveBillboardDecoder {
        stop_flag,
        pause_flag,
        frame_rx,
        ended_flag,
        duration_sec,
    }
}

fn spawn_billboard_structure(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cfg: &BillboardAnimConfig,
) -> BillboardVisualSlot {
    let w = cfg.screen_width_m as f32;
    let h = cfg.screen_height_m as f32;
    let pole_h = match cfg.structure {
        BillboardStructureKind::WallMount => 0.0,
        BillboardStructureKind::Monopole | BillboardStructureKind::TwinPole => {
            cfg.pole_height_m as f32
        }
    };
    let bezel = 0.24_f32;
    let cab_depth = 0.36_f32;
    let screen_cy = pole_h + h * 0.5;

    let frame_col = parse_hex_color(&cfg.frame_color_hex, 1.0);
    let frame_mat = materials.add(StandardMaterial {
        base_color: frame_col,
        metallic: 0.55,
        perceptual_roughness: 0.42,
        ..default()
    });
    let footing_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.35, 0.40),
        metallic: 0.15,
        perceptual_roughness: 0.85,
        ..default()
    });
    let screen_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.06, 0.07, 0.10),
        emissive: LinearRgba::rgb(0.02, 0.03, 0.05),
        perceptual_roughness: 0.35,
        ..default()
    });

    let root = commands
        .spawn((
            Transform::default(),
            Visibility::Visible,
            BillboardVisualEntity,
        ))
        .id();

    commands.entity(root).with_children(|child| {
        match cfg.structure {
            BillboardStructureKind::Monopole => {
                if pole_h > 0.1 {
                    let pole_r = (w * 0.045).clamp(0.18, 0.65);
                    child.spawn((
                        Mesh3d(meshes.add(Cuboid::new(pole_r * 3.6, 0.35, pole_r * 3.6))),
                        MeshMaterial3d(footing_mat.clone()),
                        Transform::from_xyz(0.0, 0.175, 0.0),
                    ));
                    child.spawn((
                        Mesh3d(meshes.add(Cylinder::new(pole_r, pole_h + 0.15))),
                        MeshMaterial3d(frame_mat.clone()),
                        Transform::from_xyz(0.0, pole_h * 0.5, 0.0),
                    ));
                    // Horizontal support cross-beam right under the screen cabinet
                    child.spawn((
                        Mesh3d(meshes.add(Cuboid::new((w * 0.65).max(1.2), 0.24, cab_depth * 0.9))),
                        MeshMaterial3d(frame_mat.clone()),
                        Transform::from_xyz(0.0, pole_h - 0.12, 0.0),
                    ));
                }
            }
            BillboardStructureKind::TwinPole => {
                if pole_h > 0.1 {
                    let pole_r = (w * 0.032).clamp(0.14, 0.45);
                    let leg_x = (w * 0.34).max(0.5);
                    for sx in [-1.0_f32, 1.0] {
                        child.spawn((
                            Mesh3d(meshes.add(Cuboid::new(pole_r * 3.4, 0.32, pole_r * 3.4))),
                            MeshMaterial3d(footing_mat.clone()),
                            Transform::from_xyz(sx * leg_x, 0.16, 0.0),
                        ));
                        child.spawn((
                            Mesh3d(meshes.add(Cylinder::new(pole_r, pole_h + 0.15))),
                            MeshMaterial3d(frame_mat.clone()),
                            Transform::from_xyz(sx * leg_x, pole_h * 0.5, 0.0),
                        ));
                    }
                    child.spawn((
                        Mesh3d(meshes.add(Cuboid::new(leg_x * 2.15, 0.22, cab_depth * 0.85))),
                        MeshMaterial3d(frame_mat.clone()),
                        Transform::from_xyz(0.0, pole_h - 0.11, 0.0),
                    ));
                }
            }
            BillboardStructureKind::WallMount => {
                // Sleek rear mounting brackets for attaching flush to a building wall
                let rail_y = h * 0.28;
                for sy in [-1.0_f32, 1.0] {
                    child.spawn((
                        Mesh3d(meshes.add(Cuboid::new(w + bezel * 1.4, 0.14, 0.12))),
                        MeshMaterial3d(frame_mat.clone()),
                        Transform::from_xyz(0.0, screen_cy + sy * rail_y, -cab_depth * 0.55),
                    ));
                }
            }
        }

        // Main LED screen cabinet / bezel housing
        child.spawn((
            Mesh3d(meshes.add(Cuboid::new(w + bezel * 2.0, h + bezel * 2.0, cab_depth))),
            MeshMaterial3d(frame_mat.clone()),
            Transform::from_xyz(0.0, screen_cy, 0.0),
        ));

        // Top visor / rain hood lip above screen
        child.spawn((
            Mesh3d(meshes.add(Cuboid::new(w + bezel * 2.2, 0.10, cab_depth + 0.28))),
            MeshMaterial3d(frame_mat),
            Transform::from_xyz(0.0, screen_cy + h * 0.5 + bezel + 0.03, 0.06),
        ));

        // Front LED display quad (local +Z, Bevy Rectangle UVs map top-left (0,0) -> bottom-right (1,1))
        let screen_mesh = meshes.add(Rectangle::new(w, h));
        child.spawn((
            Mesh3d(screen_mesh.clone()),
            MeshMaterial3d(screen_mat.clone()),
            Transform::from_xyz(0.0, screen_cy, cab_depth * 0.5 + 0.015),
        ));

        // Optional rear display quad when double-sided is enabled
        if cfg.double_sided {
            child.spawn((
                Mesh3d(screen_mesh),
                MeshMaterial3d(screen_mat.clone()),
                Transform::from_xyz(0.0, screen_cy, -(cab_depth * 0.5 + 0.015))
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            ));
        }
    });

    BillboardVisualSlot {
        root,
        geom_sig: geometry_signature(cfg),
        screen_mat,
        last_bound_tex: None,
        last_brightness: -1.0,
        last_is_lit: false,
    }
}

fn stop_billboard_audio(
    commands: &mut Commands,
    sinks: &Query<&mut AudioSink, With<BillboardAudioEntity>>,
    entity: Option<Entity>,
) {
    if let Some(ent) = entity {
        if let Ok(sink) = sinks.get(ent) {
            sink.stop();
        }
        commands.entity(ent).despawn();
    }
}

fn ensure_static_image_texture(
    pool: &mut BillboardRuntimePool,
    sync: &AssetSyncBridge,
    state: &AppState,
    cache_key: &str,
    abs_path: &Path,
    download_file: Option<&twg_shared::models::ModelFileMeta>,
    extract_video_frame: bool,
) -> Option<Handle<Image>> {
    if let Some(entry) = pool.static_tex_cache.get(cache_key) {
        return match entry {
            CachedStaticTex::Ready(h) => Some(h.clone()),
            CachedStaticTex::Loading(_) | CachedStaticTex::Failed(_) => None,
        };
    }

    if !abs_path.is_file() {
        if let Some(fmeta) = download_file
            && pool.download_requested_keys.insert(cache_key.to_string())
        {
            sync.send(AssetSyncCommand::PriorityDownload {
                server_url: state.server_url.clone(),
                local_root: state.assets_root.clone(),
                files: vec![fmeta.clone()],
            });
        }
        return None;
    }

    let (tx, rx) = bounded::<LoadedImageBytes>(1);
    let path_owned = abs_path.to_path_buf();
    let ffmpeg_ok = pool.ffmpeg_available;
    std::thread::spawn(move || {
        let res = if extract_video_frame {
            if ffmpeg_ok {
                extract_video_first_frame_rgba(path_owned)
            } else {
                Err("FFmpeg belum tersedia.".into())
            }
        } else {
            load_image_rgba_file(path_owned)
        };
        let _ = tx.send(res);
    });
    pool.static_tex_cache
        .insert(cache_key.to_string(), CachedStaticTex::Loading(rx));
    None
}

/// Synchronizes all procedural 3D Billboard / Videotron entities, static images, video frame
/// decoding, and optional synchronized video audio across Plot Editor, Preview, Play Skenario,
/// and Layar Besar.
#[allow(clippy::too_many_arguments)]
pub fn billboard_visual_system(
    mut commands: Commands,
    mut state: ResMut<AppState>,
    mut playback: ResMut<PlaybackState>,
    time: Res<Time<Real>>,
    sync: Res<AssetSyncBridge>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut pool: ResMut<BillboardRuntimePool>,
    mut root_tf_q: Query<(&mut Transform, &mut Visibility), With<BillboardVisualEntity>>,
    mut sinks_q: Query<&mut AudioSink, With<BillboardAudioEntity>>,
) {
    // Poll async image loaders
    for cached in pool.static_tex_cache.values_mut() {
        if let CachedStaticTex::Loading(rx) = cached {
            match rx.try_recv() {
                Ok(Ok((w, h, rgba))) => {
                    let h_img = images.add(make_bevy_rgba_image(w, h, rgba));
                    *cached = CachedStaticTex::Ready(h_img);
                }
                Ok(Err(err)) => *cached = CachedStaticTex::Failed(err),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    *cached = CachedStaticTex::Failed("Gagal memuat gambar.".into());
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
    }

    // Poll async video audio extractors
    for cached in pool.video_audio_cache.values_mut() {
        if let CachedVideoAudio::Loading(rx) = cached {
            match rx.try_recv() {
                Ok(Ok(None)) => *cached = CachedVideoAudio::NoAudio,
                Ok(Ok(Some((bytes, duration_sec)))) => {
                    let handle = audio_sources.add(AudioSource { bytes });
                    *cached = CachedVideoAudio::Ready(handle, duration_sec);
                }
                Ok(Err(err)) => *cached = CachedVideoAudio::Failed(err),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    *cached = CachedVideoAudio::Failed("Gagal memuat audio video.".into());
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
    }

    if !state.is_3d_map_screen() {
        for (_, slot) in pool.slots.drain() {
            commands.entity(slot.root).despawn();
        }
        for (_, run) in pool.runs.drain() {
            stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
        }
        pool.last_frame_tex.clear();
        state.billboard_status.clear();
        return;
    }

    // Reset active video runs when playback session changes (Stop All / restart Preview / exit Play)
    let cur_gen = Some(playback.session_generation);
    if pool.session_generation != cur_gen {
        pool.session_generation = cur_gen;
        for (_, run) in pool.runs.drain() {
            stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
        }
        pool.last_frame_tex.clear();
    }

    let is_editor_preview =
        !playback.play_test_mode && playback.editor_preview_anim_id.is_some();
    let preview_scope: Option<HashSet<Uuid>> = if is_editor_preview {
        playback.editor_preview_anim_id.map(|pid| {
            let mut set: HashSet<Uuid> = collect_group_descendant_ids(pid, &state.anims)
                .into_iter()
                .collect();
            set.insert(pid);
            set
        })
    } else {
        None
    };

    let billboard_anims: Vec<Anim> = state
        .anims
        .iter()
        .filter(|a| is_billboard_anim(a))
        .cloned()
        .collect();
    let valid_ids: HashSet<Uuid> = billboard_anims.iter().map(|a| a.id).collect();

    // Despawn removed billboards & stop removed runs
    let stale_slots: Vec<Uuid> = pool
        .slots
        .keys()
        .copied()
        .filter(|id| !valid_ids.contains(id))
        .collect();
    for id in stale_slots {
        if let Some(slot) = pool.slots.remove(&id) {
            commands.entity(slot.root).despawn();
        }
        if let Some(run) = pool.runs.remove(&id) {
            stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
        }
        pool.last_frame_tex.remove(&id);
        state.billboard_status.remove(&id);
    }

    let audible = !playback.play_test_mode || state.scenario_audio_enabled;
    let dt = time.delta_secs_f64();
    let mut completed_ids: Vec<Uuid> = Vec::new();

    for anim in &billboard_anims {
        let hidden = state.hidden_entity_ids.contains(&anim.id);
        let cfg = parse_billboard_anim_config(anim);
        let sig = geometry_signature(&cfg);

        // Spawn or rebuild 3D procedural billboard structure if geometry changed
        let needs_rebuild = pool
            .slots
            .get(&anim.id)
            .is_none_or(|slot| slot.geom_sig != sig);
        if needs_rebuild {
            if let Some(old) = pool.slots.remove(&anim.id) {
                commands.entity(old.root).despawn();
            }
            let slot = spawn_billboard_structure(&mut commands, &mut meshes, &mut materials, &cfg);
            pool.slots.insert(anim.id, slot);
        }

        // Update 3D root Transform & Visibility
        let world_pos = billboard_world_origin(&state, anim);
        let rot = billboard_quat(&cfg);
        let new_tf = Transform {
            translation: world_pos,
            rotation: rot,
            scale: Vec3::new(
                cfg.scale[0] as f32,
                cfg.scale[1] as f32,
                cfg.scale[2] as f32,
            ),
        };
        let vis = if hidden {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        let root_ent = pool.slots[&anim.id].root;
        if let Ok((mut tf, mut v)) = root_tf_q.get_mut(root_ent) {
            *tf = new_tf;
            *v = vis;
        } else {
            commands.entity(root_ent).insert((new_tf, vis));
        }

        if hidden {
            if let Some(run) = pool.runs.remove(&anim.id) {
                stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
            }
            continue;
        }

        // Determine desired display texture for the LED screen
        let mut desired_tex: Option<Handle<Image>> = None;

        match cfg.media_mode {
            BillboardMediaMode::Image => {
                // Image mode is static: stop any lingering video run and always display image
                if let Some(run) = pool.runs.remove(&anim.id) {
                    stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
                }
                if let Some(img_id) = cfg.image_asset_id {
                    let found_asset = state
                        .media_assets
                        .iter()
                        .find(|m| m.id == img_id && m.media_type == MediaAssetType::Texture)
                        .cloned();
                    if let Some(asset) = found_asset {
                        let key = format!(
                            "img:{}:{}",
                            asset.active_file.rel_path, asset.active_file.sha256
                        );
                        let abs = state.assets_root.join(&asset.active_file.rel_path);
                        desired_tex = ensure_static_image_texture(
                            &mut pool,
                            &sync,
                            &state,
                            &key,
                            &abs,
                            Some(&asset.active_file),
                            false,
                        );
                        state.billboard_status.insert(
                            anim.id,
                            if desired_tex.is_some() {
                                format!("Gambar statis · {}", asset.name)
                            } else {
                                "Memuat gambar...".into()
                            },
                        );
                    } else {
                        state
                            .billboard_status
                            .insert(anim.id, "Aset gambar tidak ditemukan".into());
                    }
                } else {
                    state
                        .billboard_status
                        .insert(anim.id, "Belum memilih Gambar".into());
                }
            }
            BillboardMediaMode::Video => {
                let in_scope = preview_scope
                    .as_ref()
                    .is_none_or(|scope| scope.contains(&anim.id));
                let track_state = if in_scope {
                    playback
                        .tracks
                        .get(&anim.id)
                        .map(|t| (t.status, t.trigger_seq))
                } else {
                    None
                };

                let is_active_playback = matches!(
                    track_state,
                    Some((AnimRunStatus::Playing | AnimRunStatus::Paused, _))
                ) && (playback.play_test_mode || is_editor_preview);

                if !is_active_playback {
                    // Stop any running decoder/audio cleanly
                    if let Some(run) = pool.runs.remove(&anim.id) {
                        stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
                        if run.has_decoded_frame
                            && let Some(vt) = run.video_tex
                        {
                            pool.last_frame_tex.insert(anim.id, vt);
                        }
                    }

                    // Resolve idle screen look
                    match cfg.idle_mode {
                        BillboardIdleMode::Dark => {
                            desired_tex = None;
                        }
                        BillboardIdleMode::HoldLastFrame => {
                            if let Some(last) = pool.last_frame_tex.get(&anim.id) {
                                desired_tex = Some(last.clone());
                            } else {
                                desired_tex = resolve_video_idle_thumbnail(
                                    &mut pool,
                                    &sync,
                                    &state,
                                    cfg.video_asset_id,
                                );
                            }
                        }
                        BillboardIdleMode::Thumbnail => {
                            desired_tex = resolve_video_idle_thumbnail(
                                &mut pool,
                                &sync,
                                &state,
                                cfg.video_asset_id,
                            );
                        }
                    }

                    if !pool.ffmpeg_available {
                        state.billboard_status.insert(
                            anim.id,
                            "FFmpeg/FFprobe belum ditemukan (taruh di folder bin/)".into(),
                        );
                    } else if cfg.video_asset_id.is_none() {
                        state
                            .billboard_status
                            .insert(anim.id, "Belum memilih Video".into());
                    } else if matches!(track_state, Some((AnimRunStatus::Finished, _))) {
                        state.billboard_status.insert(anim.id, "Selesai".into());
                    } else {
                        state.billboard_status.insert(anim.id, "Standby".into());
                    }
                } else {
                    let (t_status, trigger_seq) = track_state.unwrap();
                    // Restart run if retriggered
                    if pool
                        .runs
                        .get(&anim.id)
                        .is_some_and(|r| r.trigger != trigger_seq)
                        && let Some(old) = pool.runs.remove(&anim.id)
                    {
                        stop_billboard_audio(&mut commands, &sinks_q, old.audio_entity);
                    }

                    let mut run = pool.runs.remove(&anim.id).unwrap_or_else(|| BillboardVideoRun {
                        trigger: trigger_seq,
                        config: cfg.clone(),
                        delay_elapsed: 0.0,
                        plays_done: 0,
                        current_pts: 0.0,
                        decoder: None,
                        audio_entity: None,
                        audio_spawned_for_loop: false,
                        video_tex: None,
                        video_tex_dims: (0, 0),
                        has_decoded_frame: false,
                        waiting_since: Instant::now(),
                        download_requested: false,
                    });

                    // Live-sync audio toggle & volume so Inspector changes take effect immediately
                    run.config.play_audio = cfg.play_audio;
                    run.config.volume = cfg.volume;

                    let run_cfg = run.config.clone();
                    let step_res: Result<bool, String> = (|| {
                        if !pool.ffmpeg_available {
                            return Err(
                                "FFmpeg/FFprobe tidak ditemukan. Letakkan di folder bin/ aplikasi atau instal FFmpeg."
                                    .into(),
                            );
                        }
                        let Some(vid_id) = run_cfg.video_asset_id else {
                            return Err("Pilih Video dari Assets terlebih dahulu.".into());
                        };
                        let Some(asset) = state
                            .media_assets
                            .iter()
                            .find(|m| m.id == vid_id && m.media_type == MediaAssetType::Video)
                            .cloned()
                        else {
                            if state.sync_status.checking
                                && run.waiting_since.elapsed().as_secs() < 90
                            {
                                state.billboard_status.insert(
                                    anim.id,
                                    "Menunggu daftar Video dari server...".into(),
                                );
                                return Ok(false);
                            }
                            return Err("Aset video tidak ditemukan di Assets.".into());
                        };

                        let file = &asset.active_file;
                        let abs = state.assets_root.join(&file.rel_path);
                        if !abs.is_file() {
                            if !run.download_requested {
                                sync.send(AssetSyncCommand::PriorityDownload {
                                    server_url: state.server_url.clone(),
                                    local_root: state.assets_root.clone(),
                                    files: vec![file.clone()],
                                });
                                run.download_requested = true;
                            }
                            if run.waiting_since.elapsed().as_secs() > 120 {
                                return Err("Unduhan video timeout. Periksa koneksi lalu Play ulang.".into());
                            }
                            state
                                .billboard_status
                                .insert(anim.id, "Mengunduh video dari server...".into());
                            return Ok(false);
                        }

                        // Kick off optional video audio extraction cache
                        let audio_key = format!("vaudio:{}:{}", file.rel_path, file.sha256);
                        if run_cfg.play_audio && !pool.video_audio_cache.contains_key(&audio_key) {
                            let (tx, rx) = bounded::<LoadedVideoAudio>(1);
                            let p_abs = abs.clone();
                            std::thread::spawn(move || {
                                let _ = tx.send(extract_video_audio_wav(p_abs));
                            });
                            pool.video_audio_cache
                                .insert(audio_key.clone(), CachedVideoAudio::Loading(rx));
                        }

                        // Handle Paused state
                        if t_status == AnimRunStatus::Paused {
                            if let Some(dec) = &run.decoder {
                                dec.pause_flag.store(true, Ordering::SeqCst);
                            }
                            if let Some(aent) = run.audio_entity
                                && let Ok(sink) = sinks_q.get(aent)
                            {
                                sink.pause();
                            }
                            state.billboard_status.insert(anim.id, "Dijeda".into());
                            return Ok(false);
                        }

                        // Handle initial delay before starting playback
                        if run.delay_elapsed < run_cfg.delay_sec {
                            run.delay_elapsed = (run.delay_elapsed + dt).min(run_cfg.delay_sec);
                            if let Some(tr) = playback.tracks.get_mut(&anim.id) {
                                tr.distance_m = run.delay_elapsed;
                            }
                            state.billboard_status.insert(
                                anim.id,
                                format!(
                                    "Jeda awal: {:.1} dtk",
                                    run_cfg.delay_sec - run.delay_elapsed
                                ),
                            );
                            return Ok(false);
                        }

                        // Ensure decoder is running
                        if run.decoder.is_none() {
                            run.decoder = Some(spawn_video_decoder_thread(abs.clone()));
                            run.audio_spawned_for_loop = false;
                            run.current_pts = 0.0;
                        }

                        if let Some(dec) = &run.decoder {
                            dec.pause_flag.store(false, Ordering::SeqCst);
                        }

                        // Manage synchronized audio player for this loop if enabled
                        let mut audio_note: Option<String> = None;
                        if !run_cfg.play_audio {
                            if run.audio_entity.is_some() || run.audio_spawned_for_loop {
                                stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity.take());
                                run.audio_spawned_for_loop = false;
                            }
                        } else {
                            match pool.video_audio_cache.get(&audio_key) {
                                Some(CachedVideoAudio::Ready(a_handle, audio_dur)) => {
                                    if !run.audio_spawned_for_loop {
                                        if run.current_pts + 0.08 < *audio_dur {
                                            stop_billboard_audio(
                                                &mut commands,
                                                &sinks_q,
                                                run.audio_entity.take(),
                                            );
                                            let settings = PlaybackSettings {
                                                paused: false,
                                                mode: PlaybackMode::Once,
                                                volume: Volume::Linear(if audible {
                                                    run_cfg.volume
                                                } else {
                                                    0.0
                                                }),
                                                start_position: if run.current_pts > 0.12 {
                                                    Some(Duration::from_secs_f64(
                                                        run.current_pts.min((audio_dur - 0.05).max(0.0)),
                                                    ))
                                                } else {
                                                    None
                                                },
                                                ..default()
                                            };
                                            let ent = commands
                                                .spawn((
                                                    AudioPlayer::new(a_handle.clone()),
                                                    settings,
                                                    BillboardAudioEntity,
                                                ))
                                                .id();
                                            run.audio_entity = Some(ent);
                                        }
                                        run.audio_spawned_for_loop = true;
                                    } else if let Some(aent) = run.audio_entity
                                        && let Ok(mut sink) = sinks_q.get_mut(aent)
                                    {
                                        sink.play();
                                        sink.set_volume(Volume::Linear(if audible {
                                            run_cfg.volume
                                        } else {
                                            0.0
                                        }));
                                    }
                                    if !audible {
                                        audio_note = Some("output dibisukan".into());
                                    }
                                }
                                Some(CachedVideoAudio::Loading(_)) => {
                                    audio_note = Some("menyiapkan audio...".into());
                                }
                                Some(CachedVideoAudio::NoAudio) => {
                                    audio_note = Some("video tanpa trek audio".into());
                                }
                                Some(CachedVideoAudio::Failed(err)) => {
                                    audio_note = Some(format!("audio gagal: {err}"));
                                }
                                None => {}
                            }
                        }

                        // Drain latest decoded RGBA frame and update Bevy GPU texture in place
                        let mut latest_frame: Option<DecodedVideoFrame> = None;
                        let mut dec_ended = false;
                        let mut dec_dur = asset.duration_sec.unwrap_or(0.0);
                        if let Some(dec) = &run.decoder {
                            while let Ok(f) = dec.frame_rx.try_recv() {
                                latest_frame = Some(f);
                            }
                            dec_ended = dec.ended_flag.load(Ordering::Relaxed);
                            if dec.duration_sec > 0.0 {
                                dec_dur = dec.duration_sec;
                            }
                        }

                        if let Some(frame) = latest_frame {
                            run.current_pts = frame.pts_sec as f64;
                            run.has_decoded_frame = true;
                            let fw = frame.width as u32;
                            let fh = frame.height as u32;
                            if let Some(ref handle) = run.video_tex
                                && run.video_tex_dims == (fw, fh)
                                && let Some(mut img) = images.get_mut(handle)
                            {
                                img.data = Some(frame.rgba);
                            } else {
                                let handle = images.add(make_bevy_rgba_image(fw, fh, frame.rgba));
                                run.video_tex = Some(handle);
                                run.video_tex_dims = (fw, fh);
                            }
                        }

                        if let Some(tr) = playback.tracks.get_mut(&anim.id) {
                            let clip_len = dec_dur.max(0.0);
                            tr.distance_m =
                                run_cfg.delay_sec + clip_len * run.plays_done as f64 + run.current_pts;
                            tr.total_length_m = run_cfg
                                .plays()
                                .map(|n| run_cfg.delay_sec + clip_len * n as f64)
                                .unwrap_or(0.0);
                        }

                        let dur_lbl = if dec_dur > 0.0 {
                            format!("{:.1} / {:.1} dtk", run.current_pts, dec_dur)
                        } else {
                            format!("{:.1} dtk", run.current_pts)
                        };
                        let status_text = if let Some(note) = audio_note {
                            format!(
                                "Memutar · {dur_lbl} · putaran {} · {note}",
                                run.plays_done + 1
                            )
                        } else {
                            format!(
                                "Memutar · {dur_lbl} · putaran {}",
                                run.plays_done + 1
                            )
                        };
                        state.billboard_status.insert(anim.id, status_text);

                        if dec_ended {
                            run.decoder = None;
                            stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity.take());
                            run.audio_spawned_for_loop = false;
                            run.plays_done += 1;
                            if run_cfg.plays().is_some_and(|n| run.plays_done >= n) {
                                return Ok(true);
                            }
                            run.current_pts = 0.0;
                        }

                        Ok(false)
                    })();

                    match step_res {
                        Ok(false) => {
                            if run.has_decoded_frame {
                                desired_tex = run.video_tex.clone();
                            } else if run_cfg.idle_mode != BillboardIdleMode::Dark {
                                desired_tex = resolve_video_idle_thumbnail(
                                    &mut pool,
                                    &sync,
                                    &state,
                                    run_cfg.video_asset_id,
                                );
                            }
                            pool.runs.insert(anim.id, run);
                        }
                        Ok(true) => {
                            stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
                            if run.has_decoded_frame
                                && let Some(vt) = run.video_tex
                            {
                                pool.last_frame_tex.insert(anim.id, vt.clone());
                                if run_cfg.idle_mode == BillboardIdleMode::HoldLastFrame {
                                    desired_tex = Some(vt);
                                }
                            }
                            completed_ids.push(anim.id);
                            state.billboard_status.insert(anim.id, "Selesai".into());
                        }
                        Err(err) => {
                            stop_billboard_audio(&mut commands, &sinks_q, run.audio_entity);
                            completed_ids.push(anim.id);
                            state
                                .billboard_status
                                .insert(anim.id, format!("Gagal: {err}"));
                            state.status_banner =
                                Some(format!("Billboard '{}': {err}", anim.name));
                        }
                    }
                }
            }
        }

        // Bind display texture & LED brightness onto the screen material
        let slot = pool.slots.get_mut(&anim.id).unwrap();
        let is_lit = desired_tex.is_some();
        if slot.last_bound_tex != desired_tex
            || (slot.last_brightness - cfg.brightness).abs() > 1e-3
            || slot.last_is_lit != is_lit
        {
            slot.last_bound_tex = desired_tex.clone();
            slot.last_brightness = cfg.brightness;
            slot.last_is_lit = is_lit;
            if let Some(mut mat) = materials.get_mut(&slot.screen_mat) {
                if let Some(tex) = desired_tex {
                    let b = cfg.brightness.clamp(0.15, 4.0);
                    mat.base_color = Color::WHITE;
                    mat.base_color_texture = Some(tex.clone());
                    mat.emissive = LinearRgba::rgb(b, b, b);
                    mat.emissive_texture = Some(tex);
                } else {
                    mat.base_color = Color::srgb(0.05, 0.06, 0.09);
                    mat.base_color_texture = None;
                    mat.emissive = LinearRgba::rgb(0.015, 0.02, 0.035);
                    mat.emissive_texture = None;
                }
            }
        }
    }

    if !completed_ids.is_empty() {
        for aid in completed_ids {
            if let Some(tr) = playback.tracks.get_mut(&aid) {
                tr.status = AnimRunStatus::Finished;
                tr.completed_cycles = tr.completed_cycles.saturating_add(1);
            }
        }
        playback.playing = playback
            .tracks
            .values()
            .any(|t| t.status == AnimRunStatus::Playing);
    }
}

fn resolve_video_idle_thumbnail(
    pool: &mut BillboardRuntimePool,
    sync: &AssetSyncBridge,
    state: &AppState,
    video_asset_id: Option<Uuid>,
) -> Option<Handle<Image>> {
    let vid_id = video_asset_id?;
    let asset = state
        .media_assets
        .iter()
        .find(|m| m.id == vid_id && m.media_type == MediaAssetType::Video)?;
    if let Some(thumb) = &asset.thumbnail_file {
        let key = format!("vthumb:{}:{}", thumb.rel_path, thumb.sha256);
        let abs = state.assets_root.join(&thumb.rel_path);
        return ensure_static_image_texture(pool, sync, state, &key, &abs, Some(thumb), false);
    }
    // Fallback: extract frame 1 directly from the local video file
    let key = format!(
        "vframe0:{}:{}",
        asset.active_file.rel_path, asset.active_file.sha256
    );
    let abs = state.assets_root.join(&asset.active_file.rel_path);
    ensure_static_image_texture(
        pool,
        sync,
        state,
        &key,
        &abs,
        Some(&asset.active_file),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::Source;

    #[test]
    fn wrapped_pcm_wav_decodes_and_seeks_in_rodio() {
        // 1.0 second of stereo 44.1 kHz s16le PCM (+ 1 odd trailing byte to test frame alignment)
        let pcm = vec![0u8; 44_100 * 2 * 2 + 1];
        let wav = super::super::media_preview::wrap_pcm_s16le_as_wav(&pcm, 44_100, 2);
        let mut decoder = rodio::Decoder::try_from(Cursor::new(wav)).expect("valid WAV");
        let dur = decoder.total_duration().expect("known duration");
        assert!((dur.as_secs_f64() - 1.0).abs() < 0.001);
        decoder
            .try_seek(Duration::from_millis(500))
            .expect("seekable WAV");
    }
}
