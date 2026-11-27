use std::collections::HashMap;
use std::path::PathBuf;

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use crossbeam_channel::{Receiver, bounded};
use twg_shared::models::{BaseLayer, MapMode};

use crate::state::AppState;

#[derive(Component)]
pub struct IndoorGroundPlane;

#[derive(Component)]
pub struct IndoorFloorPlanPlane;

type LoadedFloorPlanResult = Result<(u32, u32, Vec<u8>), String>;

enum CachedFloorPlanTex {
    Loading(Receiver<LoadedFloorPlanResult>),
    Ready(Handle<Image>),
    Failed(#[allow(dead_code)] String),
}

#[derive(Default)]
pub struct FloorPlanTexCache {
    entries: HashMap<String, CachedFloorPlanTex>,
    applied_url: Option<String>,
}

fn floor_plan_cache_path(url: &str) -> PathBuf {
    let cache_root = std::env::var("TWG_TILE_CACHE_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tile_cache"));
    let safe_name: String = url
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = if safe_name.len() > 140 {
        safe_name[safe_name.len() - 140..].to_string()
    } else {
        safe_name
    };
    cache_root.join("floor_plans").join(trimmed)
}

fn decode_floor_plan_bytes(bytes: &[u8]) -> LoadedFloorPlanResult {
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Gagal decode gambar Floor Plan: {e}"))?;
    const MAX_DIM: u32 = 4096;
    let img = if img.width() > MAX_DIM || img.height() > MAX_DIM {
        img.resize(MAX_DIM, MAX_DIM, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((w, h, rgba.into_raw()))
}

fn fetch_or_load_floor_plan(resolved_url: String) -> LoadedFloorPlanResult {
    // 1. Direct local path fallback
    let direct_path = PathBuf::from(&resolved_url);
    if direct_path.is_file() {
        let bytes = std::fs::read(&direct_path)
            .map_err(|e| format!("Gagal membaca file Floor Plan: {e}"))?;
        return decode_floor_plan_bytes(&bytes);
    }

    // 2. Local disk cache hit
    let cache_path = floor_plan_cache_path(&resolved_url);
    if cache_path.is_file()
        && let Ok(bytes) = std::fs::read(&cache_path)
        && let Ok(decoded) = decode_floor_plan_bytes(&bytes)
    {
        return Ok(decoded);
    }

    // 3. HTTP download from server (works cleanly with `/uploads/...` and full URLs)
    if !resolved_url.starts_with("http://") && !resolved_url.starts_with("https://") {
        return Err(format!("URL Floor Plan tidak valid: {resolved_url}"));
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Runtime download gagal: {e}"))?;

    let bytes = rt.block_on(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("HTTP client gagal: {e}"))?;
        let resp = client
            .get(&resolved_url)
            .send()
            .await
            .map_err(|e| format!("Download Floor Plan gagal: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Server mengembalikan HTTP {}", resp.status()));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| format!("Gagal membaca body Floor Plan: {e}"))
    })?;

    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache_path, &bytes);

    decode_floor_plan_bytes(&bytes)
}

fn make_floor_plan_bevy_image(w: u32, h: u32, rgba: Vec<u8>) -> Image {
    let mut img = Image::new(
        Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

pub fn setup_indoor_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Plane3d::default().mesh().size(4000.0, 4000.0));
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.03, 0.05, 0.09),
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_xyz(0.0, -0.1, 0.0),
        Visibility::Hidden,
        IndoorGroundPlane,
    ));

    // Unit 1m x 1m horizontal quad scaled dynamically to (width_m, 1.0, height_m) and rotated around Y
    let fp_mesh = meshes.add(Plane3d::default().mesh().size(1.0, 1.0));
    let fp_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(fp_mesh),
        MeshMaterial3d(fp_mat),
        Transform::from_xyz(0.0, 0.01, 0.0),
        Visibility::Hidden,
        IndoorFloorPlanPlane,
    ));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn indoor_grid_system(
    state: Res<AppState>,
    mut gizmos: Gizmos,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut fp_cache: Local<FloorPlanTexCache>,
    mut ground_q: Query<
        (&MeshMaterial3d<StandardMaterial>, &mut Visibility),
        (With<IndoorGroundPlane>, Without<IndoorFloorPlanPlane>),
    >,
    mut floor_plan_q: Query<
        (
            &MeshMaterial3d<StandardMaterial>,
            &mut Transform,
            &mut Visibility,
        ),
        (With<IndoorFloorPlanPlane>, Without<IndoorGroundPlane>),
    >,
) {
    // Poll async floor plan loaders
    for cached in fp_cache.entries.values_mut() {
        if let CachedFloorPlanTex::Loading(rx) = cached {
            match rx.try_recv() {
                Ok(Ok((w, h, rgba))) => {
                    let handle = images.add(make_floor_plan_bevy_image(w, h, rgba));
                    *cached = CachedFloorPlanTex::Ready(handle);
                }
                Ok(Err(err)) => {
                    *cached = CachedFloorPlanTex::Failed(err);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    *cached = CachedFloorPlanTex::Failed("Thread loader terputus.".into());
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
    }

    let map_visible = state.is_3d_map_screen() || state.is_skenario_map_preview_active();
    let is_indoor = map_visible
        && (state.active_map_mode == MapMode::Indoor || !state.active_base_layer.is_outdoor_tile());

    let Ok((mat_handle, mut vis)) = ground_q.single_mut() else {
        return;
    };

    if !is_indoor {
        *vis = Visibility::Hidden;
        if let Ok((_, _, mut fp_vis)) = floor_plan_q.single_mut() {
            *fp_vis = Visibility::Hidden;
        }
        return;
    }

    *vis = Visibility::Visible;
    let is_floor_plan = state.active_base_layer == BaseLayer::FloorPlan;
    let is_light = matches!(
        state.active_base_layer,
        BaseLayer::GridLight | BaseLayer::BlankLight
    );
    let has_grid = !matches!(
        state.active_base_layer,
        BaseLayer::BlankDark | BaseLayer::BlankLight
    );

    if let Some(mut mat) = materials.get_mut(&mat_handle.0) {
        mat.base_color = if is_light {
            Color::srgb(0.95, 0.97, 0.99)
        } else if is_floor_plan {
            Color::srgb(0.05, 0.07, 0.11)
        } else {
            Color::srgb(0.03, 0.05, 0.08)
        };
    }

    // Render Floor Plan image plane when `BaseLayer::FloorPlan` is active
    if let Ok((fp_mat_handle, mut fp_tf, mut fp_vis)) = floor_plan_q.single_mut() {
        if is_floor_plan {
            let (raw_url, width_m, height_m, rot_deg, opacity) =
                if state.is_skenario_map_preview_active() {
                    (
                        state.skenario_table.form_floor_plan_url.clone(),
                        state.skenario_table.form_floor_plan_width_m as f32,
                        state.skenario_table.form_floor_plan_height_m as f32,
                        state.skenario_table.form_floor_plan_rotation_deg as f32,
                        state.skenario_table.form_floor_plan_opacity,
                    )
                } else if let Some(sk) = &state.active_skenario {
                    (
                        sk.indoor_config.floor_plan_url.clone(),
                        sk.indoor_config.floor_plan_width_m.unwrap_or(100.0) as f32,
                        sk.indoor_config.floor_plan_height_m.unwrap_or(100.0) as f32,
                        sk.indoor_config.floor_plan_rotation_deg.unwrap_or(0.0) as f32,
                        sk.indoor_config.floor_plan_opacity.unwrap_or(1.0),
                    )
                } else {
                    (None, 100.0, 100.0, 0.0, 1.0)
                };

            let w_clamped = width_m.clamp(1.0, 4000.0);
            let h_clamped = height_m.clamp(1.0, 4000.0);
            let op_clamped = opacity.clamp(0.1, 1.0);
            let rot = Quat::from_rotation_y(-rot_deg.to_radians());

            let resolved_url = raw_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|u| {
                    if u.starts_with("http://") || u.starts_with("https://") {
                        u.to_string()
                    } else if u.starts_with('/') {
                        format!("{}{}", state.server_url.trim_end_matches('/'), u)
                    } else {
                        u.to_string()
                    }
                });

            let mut ready_tex: Option<Handle<Image>> = None;
            if let Some(url) = &resolved_url {
                if !fp_cache.entries.contains_key(url) {
                    let (tx, rx) = bounded::<LoadedFloorPlanResult>(1);
                    let url_owned = url.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(fetch_or_load_floor_plan(url_owned));
                    });
                    fp_cache
                        .entries
                        .insert(url.clone(), CachedFloorPlanTex::Loading(rx));
                }
                if let Some(CachedFloorPlanTex::Ready(h)) = fp_cache.entries.get(url) {
                    ready_tex = Some(h.clone());
                }
            }

            if let Some(tex_h) = ready_tex {
                fp_tf.translation = Vec3::new(0.0, 0.01, 0.0);
                fp_tf.rotation = rot;
                fp_tf.scale = Vec3::new(w_clamped, 1.0, h_clamped);
                if let Some(mut mat) = materials.get_mut(&fp_mat_handle.0) {
                    mat.base_color = Color::srgba(1.0, 1.0, 1.0, op_clamped);
                    if fp_cache.applied_url.as_deref() != resolved_url.as_deref()
                        || mat.base_color_texture.is_none()
                    {
                        mat.base_color_texture = Some(tex_h);
                        fp_cache.applied_url = resolved_url.clone();
                    }
                }
                *fp_vis = Visibility::Visible;
            } else {
                *fp_vis = Visibility::Hidden;
            }

            // Draw clean footprint boundary gizmo so dimensions & rotation are always crystal clear
            let hw = w_clamped * 0.5;
            let hh = h_clamped * 0.5;
            let corners = [
                rot * Vec3::new(-hw, 0.03, -hh),
                rot * Vec3::new(hw, 0.03, -hh),
                rot * Vec3::new(hw, 0.03, hh),
                rot * Vec3::new(-hw, 0.03, hh),
            ];
            let border_col = if resolved_url.is_some() {
                Color::srgba(0.22, 0.74, 0.97, 0.45)
            } else {
                Color::srgba(0.98, 0.75, 0.14, 0.75)
            };
            for i in 0..4 {
                gizmos.line(corners[i], corners[(i + 1) % 4], border_col);
            }
            // Subtle North / Front direction tick on the top edge (-Z) so rotation angle is obvious
            let north_mid = rot * Vec3::new(0.0, 0.03, -hh);
            let north_tip = rot * Vec3::new(0.0, 0.03, -hh - (w_clamped.min(h_clamped) * 0.06).clamp(1.5, 12.0));
            gizmos.line(north_mid, north_tip, Color::srgba(0.22, 0.85, 0.65, 0.75));
        } else {
            *fp_vis = Visibility::Hidden;
        }
    }

    if has_grid {
        let step = if state.is_skenario_map_preview_active() {
            (state.skenario_table.form_grid_size_m as f32).max(1.0)
        } else {
            state
                .active_skenario
                .as_ref()
                .map(|s| s.indoor_config.grid_size_m as f32)
                .filter(|v| *v >= 1.0)
                .unwrap_or(10.0)
        };
        let half_lines = 30;
        let span = step * half_lines as f32;
        let (minor_color, major_color) = if is_light {
            (
                Color::srgba(0.15, 0.23, 0.35, 0.22),
                Color::srgba(0.05, 0.4, 0.65, 0.45),
            )
        } else if is_floor_plan {
            (
                Color::srgba(0.35, 0.52, 0.72, 0.12),
                Color::srgba(0.22, 0.74, 0.97, 0.24),
            )
        } else {
            (
                Color::srgba(0.35, 0.52, 0.72, 0.22),
                Color::srgba(0.22, 0.74, 0.97, 0.45),
            )
        };

        for i in -half_lines..=half_lines {
            let offset = i as f32 * step;
            let color = if i % 5 == 0 { major_color } else { minor_color };
            gizmos.line(
                Vec3::new(offset, 0.02, -span),
                Vec3::new(offset, 0.02, span),
                color,
            );
            gizmos.line(
                Vec3::new(-span, 0.02, offset),
                Vec3::new(span, 0.02, offset),
                color,
            );
        }
    }
}
