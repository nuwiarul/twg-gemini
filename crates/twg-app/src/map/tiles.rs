use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use crossbeam_channel::{Receiver, Sender};
use tokio::sync::Semaphore;
use twg_shared::{
    geo::{TileCoord, arcgis_satellite_tile_url, lng_lat_to_tile, osm_tile_url},
    models::{BaseLayer, MapMode},
};

use crate::{
    map::camera::TacticalCamera,
    state::{AppScreen, AppState},
};

const MAX_ACTIVE_TILES: usize = 180;
const MAX_IN_FLIGHT_REQUESTS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileProvider {
    Osm,
    Satellite,
}

impl TileProvider {
    fn from_layer(layer: BaseLayer) -> Option<Self> {
        match layer {
            BaseLayer::Osm => Some(Self::Osm),
            BaseLayer::Satellite => Some(Self::Satellite),
            _ => None,
        }
    }

    fn cache_dir_name(&self) -> &'static str {
        match self {
            Self::Osm => "osm",
            Self::Satellite => "satellite",
        }
    }

    fn max_native_zoom(&self) -> u8 {
        match self {
            Self::Osm => 19,
            Self::Satellite => 18,
        }
    }

    fn tile_url(&self, tile: TileCoord) -> String {
        match self {
            Self::Osm => osm_tile_url(tile),
            Self::Satellite => arcgis_satellite_tile_url(tile),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub provider: TileProvider,
    pub coord: TileCoord,
}

pub struct TileLoaded {
    pub key: TileKey,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Component)]
pub struct MapTileEntity {
    pub key: TileKey,
    pub last_used_frame: u64,
}

#[derive(Resource)]
pub struct TileManager {
    req_tx: Sender<TileKey>,
    loaded_rx: Receiver<TileLoaded>,
    failed_rx: Receiver<TileKey>,
    unit_plane: Handle<Mesh>,
    in_flight: HashSet<TileKey>,
    failed_keys: HashSet<TileKey>,
    spawned: HashMap<TileKey, Entity>,
    active_provider: Option<TileProvider>,
    last_anchor_coords: (f64, f64),
    frame_counter: u64,
}

impl TileManager {
    pub fn start(meshes: &mut Assets<Mesh>) -> Self {
        let (req_tx, req_rx) = crossbeam_channel::unbounded::<TileKey>();
        let (loaded_tx, loaded_rx) = crossbeam_channel::unbounded::<TileLoaded>();
        let (failed_tx, failed_rx) = crossbeam_channel::unbounded::<TileKey>();

        let cache_root = std::env::var("TWG_TILE_CACHE_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".tile_cache"));
        std::thread::Builder::new()
            .name("twg-tile-fetcher".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("failed to build tile tokio runtime");

                let client = reqwest::Client::builder()
                    .user_agent("TacticalWallGameRust/0.1 (Desktop Map Renderer)")
                    .timeout(std::time::Duration::from_secs(12))
                    .build()
                    .expect("failed to build tile reqwest client");
                let sem = Arc::new(Semaphore::new(10));

                rt.block_on(async move {
                    while let Ok(key) = req_rx.recv() {
                        let client = client.clone();
                        let loaded_tx = loaded_tx.clone();
                        let failed_tx = failed_tx.clone();
                        let cache_root = cache_root.clone();
                        let sem = Arc::clone(&sem);

                        tokio::spawn(async move {
                            let _permit = sem.acquire().await.ok();
                            match fetch_and_decode_tile(&client, &cache_root, key).await {
                                Some(tile) => {
                                    let _ = loaded_tx.send(tile);
                                }
                                None => {
                                    let _ = failed_tx.send(key);
                                }
                            }
                        });
                    }
                });
            })
            .expect("failed to spawn tile fetcher thread");

        let unit_plane = meshes.add(Plane3d::default().mesh().size(1.0, 1.0));

        Self {
            req_tx,
            loaded_rx,
            failed_rx,
            unit_plane,
            in_flight: HashSet::new(),
            failed_keys: HashSet::new(),
            spawned: HashMap::new(),
            active_provider: None,
            last_anchor_coords: (f64::NAN, f64::NAN),
            frame_counter: 0,
        }
    }
}

async fn fetch_and_decode_tile(
    client: &reqwest::Client,
    cache_root: &std::path::Path,
    key: TileKey,
) -> Option<TileLoaded> {
    let cache_path = cache_root
        .join(key.provider.cache_dir_name())
        .join(key.coord.z.to_string())
        .join(key.coord.x.to_string())
        .join(format!("{}.tile", key.coord.y));

    let raw_bytes = if let Ok(bytes) = tokio::fs::read(&cache_path).await
        && !bytes.is_empty()
    {
        bytes
    } else {
        let url = key.provider.tile_url(key.coord);
        let resp = client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let bytes = resp.bytes().await.ok()?.to_vec();
        if bytes.is_empty() {
            return None;
        }
        if let Some(parent) = cache_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&cache_path, &bytes).await;
        bytes
    };

    let decoded = tokio::task::spawn_blocking(move || -> Option<(u32, u32, Vec<u8>)> {
        let img = image::load_from_memory(&raw_bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some((w, h, img.into_raw()))
    })
    .await
    .ok()??;

    Some(TileLoaded {
        key,
        width: decoded.0,
        height: decoded.1,
        rgba: decoded.2,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn tile_streaming_system(
    mut commands: Commands,
    state: Res<AppState>,
    mut tile_mgr: ResMut<TileManager>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam_q: Query<(&TacticalCamera, &Camera)>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut tile_q: Query<(Entity, &mut MapTileEntity, &mut Transform, &mut Visibility)>,
) {
    tile_mgr.frame_counter += 1;
    let frame = tile_mgr.frame_counter;

    // Drain failed requests
    while let Ok(key) = tile_mgr.failed_rx.try_recv() {
        tile_mgr.in_flight.remove(&key);
        tile_mgr.failed_keys.insert(key);
    }

    // Drain loaded tile images and spawn mesh entities
    let anchor = state.geo_anchor;
    while let Ok(loaded) = tile_mgr.loaded_rx.try_recv() {
        tile_mgr.in_flight.remove(&loaded.key);
        if tile_mgr.spawned.contains_key(&loaded.key) {
            continue;
        }

        let mut img = Image::new(
            Extent3d {
                width: loaded.width,
                height: loaded.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            loaded.rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..default()
        });

        let img_handle = images.add(img);
        let mat_handle = materials.add(StandardMaterial {
            base_color_texture: Some(img_handle),
            unlit: true,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        });

        let (center, w_m, h_m) = anchor.tile_local_rect(loaded.key.coord);
        let entity = commands
            .spawn((
                Mesh3d(tile_mgr.unit_plane.clone()),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(center[0], -0.08, center[2]).with_scale(Vec3::new(w_m, 1.0, h_m)),
                Visibility::Visible,
                MapTileEntity {
                    key: loaded.key,
                    last_used_frame: frame,
                },
            ))
            .id();

        tile_mgr.spawned.insert(loaded.key, entity);
    }

    let map_visible =
        state.screen == AppScreen::PlotSkenario || state.is_skenario_map_preview_active();
    let wanted_provider = if map_visible && state.active_map_mode == MapMode::Outdoor {
        TileProvider::from_layer(state.active_base_layer)
    } else {
        None
    };

    // If anchor changed, despawn old tiles so coordinates align exactly
    let anchor_pair = (anchor.center_lng, anchor.center_lat);
    if (tile_mgr.last_anchor_coords.0 - anchor_pair.0).abs() > 1e-7
        || (tile_mgr.last_anchor_coords.1 - anchor_pair.1).abs() > 1e-7
    {
        tile_mgr.last_anchor_coords = anchor_pair;
        for (_, entity) in tile_mgr.spawned.drain() {
            commands.entity(entity).despawn();
        }
        tile_mgr.in_flight.clear();
        tile_mgr.failed_keys.clear();
    }

    if tile_mgr.active_provider != wanted_provider {
        tile_mgr.active_provider = wanted_provider;
        tile_mgr.failed_keys.clear();
    }

    let Some(provider) = wanted_provider else {
        for (_, _, _, mut vis) in &mut tile_q {
            *vis = Visibility::Hidden;
        }
        return;
    };

    let Ok((tcam, camera)) = cam_q.single() else {
        return;
    };

    let viewport_h = camera
        .logical_viewport_rect()
        .map(|r| r.height())
        .unwrap_or_else(|| window.height())
        .max(200.0);

    let zoom = tcam.current_zoom(viewport_h, anchor.cos_lat);
    let fine_z = (zoom.round() as i32).clamp(2, provider.max_native_zoom() as i32) as u8;
    let coarse_z = fine_z.saturating_sub(2).max(2);

    let (target_lng, target_lat, _) =
        anchor.local_to_lng_lat_alt([tcam.smooth_target.x, 0.0, tcam.smooth_target.y]);

    let mut desired: Vec<(TileKey, f32)> = Vec::new();

    // Fine grid around camera target
    let center_fine = lng_lat_to_tile(target_lng, target_lat, fine_z);
    let max_idx_fine = (1u32 << fine_z).saturating_sub(1);
    let radius = if tcam.pitch_deg > 52.0 { 4_i32 } else { 3_i32 };
    let mut ring_coords: Vec<(i32, i32)> = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            ring_coords.push((dx, dy));
        }
    }
    ring_coords.sort_by_key(|(dx, dy)| dx * dx + dy * dy);

    for (dx, dy) in ring_coords {
        let tx = if fine_z < 6 {
            (center_fine.x as i64 + dx as i64).rem_euclid(1i64 << fine_z) as u32
        } else {
            (center_fine.x as i64 + dx as i64).clamp(0, max_idx_fine as i64) as u32
        };
        let ty = (center_fine.y as i64 + dy as i64).clamp(0, max_idx_fine as i64) as u32;
        desired.push((
            TileKey {
                provider,
                coord: TileCoord {
                    z: fine_z,
                    x: tx,
                    y: ty,
                },
            },
            -0.05,
        ));
    }

    // Coarse fallback grid slightly lower in Y so zooming/panning never reveals blank void
    if coarse_z < fine_z {
        let center_coarse = lng_lat_to_tile(target_lng, target_lat, coarse_z);
        let max_idx_coarse = (1u32 << coarse_z).saturating_sub(1);
        for dy in -1_i32..=1 {
            for dx in -1_i32..=1 {
                let tx = (center_coarse.x as i64 + dx as i64).clamp(0, max_idx_coarse as i64) as u32;
                let ty = (center_coarse.y as i64 + dy as i64).clamp(0, max_idx_coarse as i64) as u32;
                desired.push((
                    TileKey {
                        provider,
                        coord: TileCoord {
                            z: coarse_z,
                            x: tx,
                            y: ty,
                        },
                    },
                    -0.35,
                ));
            }
        }
    }

    let mut desired_y: HashMap<TileKey, f32> = HashMap::with_capacity(desired.len());
    for (key, y) in &desired {
        desired_y.entry(*key).or_insert(*y);
        if !tile_mgr.spawned.contains_key(key)
            && !tile_mgr.in_flight.contains(key)
            && !tile_mgr.failed_keys.contains(key)
            && tile_mgr.in_flight.len() < MAX_IN_FLIGHT_REQUESTS
        {
            tile_mgr.in_flight.insert(*key);
            let _ = tile_mgr.req_tx.send(*key);
        }
    }

    // Update visibility & Y layer of spawned tiles
    for (_, mut comp, mut tf, mut vis) in &mut tile_q {
        if let Some(&y_offset) = desired_y.get(&comp.key) {
            comp.last_used_frame = frame;
            tf.translation.y = y_offset;
            *vis = Visibility::Visible;
        } else if comp.key.provider != provider {
            *vis = Visibility::Hidden;
        } else {
            // Keep adjacent cached tiles slightly lower as visual buffer
            tf.translation.y = -0.6;
            *vis = Visibility::Visible;
        }
    }

    // LRU eviction if cache exceeds MAX_ACTIVE_TILES
    if tile_mgr.spawned.len() > MAX_ACTIVE_TILES {
        let mut entries: Vec<(TileKey, Entity, u64)> = tile_q
            .iter()
            .filter(|(_, comp, _, _)| !desired_y.contains_key(&comp.key))
            .map(|(e, comp, _, _)| (comp.key, e, comp.last_used_frame))
            .collect();
        entries.sort_by_key(|x| x.2);

        let to_remove = tile_mgr.spawned.len().saturating_sub(MAX_ACTIVE_TILES);
        for (key, entity, _) in entries.into_iter().take(to_remove) {
            tile_mgr.spawned.remove(&key);
            commands.entity(entity).despawn();
        }
    }
}
