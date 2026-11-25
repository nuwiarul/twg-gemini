use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::Skybox,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

use crate::{
    map::camera::TacticalCamera,
    state::{AppState, SkyboxPreset},
};

const CUBEMAP_FACE_SIZE: u32 = 256;

#[derive(Component)]
pub struct PrimarySunLight;

#[derive(Component)]
pub struct FillSkyLight;

#[derive(Resource, Default)]
pub struct SkyboxManager {
    pub cubemaps: HashMap<SkyboxPreset, Handle<Image>>,
    pub last_applied: Option<(SkyboxPreset, bool)>,
}

/// Keeps the camera's Bevy `Skybox` cubemap, `ClearColor`, `DirectionalLight`s, and
/// `GlobalAmbientLight` smoothly synchronized with `state.active_skybox`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync_skybox_system(
    state: Res<AppState>,
    mut mgr: ResMut<SkyboxManager>,
    mut images: ResMut<Assets<Image>>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut cam_q: Query<&mut Skybox, With<TacticalCamera>>,
    mut sun_q: Query<(&mut DirectionalLight, &mut Transform), (With<PrimarySunLight>, Without<FillSkyLight>)>,
    mut fill_q: Query<(&mut DirectionalLight, &mut Transform), (With<FillSkyLight>, Without<PrimarySunLight>)>,
) {
    let on_map = state.is_3d_map_screen() || state.is_skenario_map_preview_active();
    let effective = if on_map {
        state.active_skybox
    } else {
        SkyboxPreset::None
    };

    if mgr.last_applied == Some((effective, on_map)) {
        return;
    }
    mgr.last_applied = Some((effective, on_map));

    let Ok(mut skybox) = cam_q.single_mut() else {
        return;
    };

    if effective == SkyboxPreset::None {
        skybox.image = None;
        skybox.brightness = 1000.0;
    } else {
        let handle = mgr
            .cubemaps
            .entry(effective)
            .or_insert_with(|| images.add(build_skybox_cubemap_image(effective)))
            .clone();
        skybox.image = Some(handle);
        skybox.brightness = match effective {
            SkyboxPreset::None => 1000.0,
            SkyboxPreset::DayClear => 1060.0,
            SkyboxPreset::DayCloudy => 980.0,
            SkyboxPreset::DaySunset => 1040.0,
            SkyboxPreset::NightStarry => 960.0,
            SkyboxPreset::NightTactical => 900.0,
        };
    }

    // Harmonize ClearColor, Sun/Moon DirectionalLight, and Ambient light with the chosen skybox
    let (bg_rgb, sun_lux, sun_col, sun_pos, fill_lux, fill_col, amb_bright, amb_col) =
        match effective {
            SkyboxPreset::None => (
                Color::srgb(0.04, 0.06, 0.10),
                14_000.0,
                Color::WHITE,
                Vec3::new(120.0, 260.0, 140.0),
                5_000.0,
                Color::WHITE,
                550.0,
                Color::WHITE,
            ),
            SkyboxPreset::DayClear => (
                Color::srgb(0.45, 0.70, 0.94),
                17_500.0,
                Color::srgb(1.0, 0.98, 0.93),
                Vec3::new(170.0, 210.0, -180.0),
                5_800.0,
                Color::srgb(0.76, 0.88, 1.0),
                700.0,
                Color::srgb(0.92, 0.96, 1.0),
            ),
            SkyboxPreset::DayCloudy => (
                Color::srgb(0.62, 0.71, 0.80),
                12_500.0,
                Color::srgb(0.95, 0.97, 1.0),
                Vec3::new(110.0, 260.0, -90.0),
                6_800.0,
                Color::srgb(0.84, 0.89, 0.95),
                780.0,
                Color::srgb(0.90, 0.93, 0.97),
            ),
            SkyboxPreset::DaySunset => (
                Color::srgb(0.24, 0.19, 0.30),
                15_000.0,
                Color::srgb(1.0, 0.76, 0.50),
                Vec3::new(260.0, 75.0, -210.0),
                4_500.0,
                Color::srgb(0.58, 0.62, 0.88),
                560.0,
                Color::srgb(0.96, 0.82, 0.74),
            ),
            SkyboxPreset::NightStarry => (
                Color::srgb(0.03, 0.06, 0.14),
                5_800.0,
                Color::srgb(0.68, 0.82, 1.0),
                Vec3::new(-160.0, 190.0, -200.0),
                2_800.0,
                Color::srgb(0.36, 0.48, 0.78),
                360.0,
                Color::srgb(0.64, 0.76, 0.98),
            ),
            SkyboxPreset::NightTactical => (
                Color::srgb(0.02, 0.04, 0.08),
                4_800.0,
                Color::srgb(0.62, 0.76, 0.96),
                Vec3::new(-130.0, 220.0, -140.0),
                2_400.0,
                Color::srgb(0.32, 0.44, 0.68),
                320.0,
                Color::srgb(0.58, 0.70, 0.92),
            ),
        };

    clear_color.0 = bg_rgb;
    ambient.brightness = amb_bright;
    ambient.color = amb_col;

    if let Ok((mut sun_l, mut sun_tf)) = sun_q.single_mut() {
        sun_l.illuminance = sun_lux;
        sun_l.color = sun_col;
        *sun_tf = Transform::from_translation(sun_pos).looking_at(Vec3::ZERO, Vec3::Y);
    }
    if let Ok((mut fill_l, mut fill_tf)) = fill_q.single_mut() {
        fill_l.illuminance = fill_lux;
        fill_l.color = fill_col;
        *fill_tf = Transform::from_translation(Vec3::new(-sun_pos.x, 180.0, -sun_pos.z))
            .looking_at(Vec3::ZERO, Vec3::Y);
    }
}

/// Generates a seamless 6-face RGBA8 sRGB cubemap `Image` for the requested `SkyboxPreset`.
pub fn build_skybox_cubemap_image(preset: SkyboxPreset) -> Image {
    let size = CUBEMAP_FACE_SIZE;
    let face_pixels = (size * size) as usize;
    let mut data = vec![0u8; face_pixels * 6 * 4];

    for face in 0..6u32 {
        let face_offset = face as usize * face_pixels * 4;
        for py in 0..size {
            let v = ((py as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            for px in 0..size {
                let u = ((px as f32 + 0.5) / size as f32) * 2.0 - 1.0;
                // Standard WebGPU/Vulkan cubemap face orientation:
                let cube_dir = match face {
                    0 => Vec3::new(1.0, -v, -u),  // +X
                    1 => Vec3::new(-1.0, -v, u),  // -X
                    2 => Vec3::new(u, 1.0, v),    // +Y
                    3 => Vec3::new(u, -1.0, -v),  // -Y
                    4 => Vec3::new(u, -v, 1.0),   // +Z
                    _ => Vec3::new(-u, -v, -1.0), // -Z
                };
                // Bevy's skybox shader samples `ray_direction * vec3(1, 1, -1)`
                let world_dir = Vec3::new(cube_dir.x, cube_dir.y, -cube_dir.z).normalize();
                let rgb = sample_sky_srgb(preset, world_dir);

                let idx = face_offset + ((py * size + px) as usize) * 4;
                data[idx] = (rgb.x.clamp(0.0, 1.0) * 255.0).round() as u8;
                data[idx + 1] = (rgb.y.clamp(0.0, 1.0) * 255.0).round() as u8;
                data[idx + 2] = (rgb.z.clamp(0.0, 1.0) * 255.0).round() as u8;
                data[idx + 3] = 255;
            }
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        ..default()
    });
    image
}

/// Evaluates seamless 3D procedural sky color (in sRGB 0..1) along normalized `world_dir`.
pub fn sample_sky_srgb(preset: SkyboxPreset, dir: Vec3) -> Vec3 {
    let y = dir.y;
    let elev = y.max(0.0);

    match preset {
        SkyboxPreset::None => Vec3::new(0.04, 0.06, 0.10),
        SkyboxPreset::DayClear => {
            let zenith = Vec3::new(0.14, 0.42, 0.80);
            let mid = Vec3::new(0.42, 0.72, 0.96);
            let horizon = Vec3::new(0.84, 0.93, 0.99);
            let ground = Vec3::new(0.50, 0.62, 0.74);

            let mut col = if y >= 0.0 {
                let t1 = smoothstep(0.0, 0.24, elev);
                let t2 = smoothstep(0.18, 0.88, elev);
                horizon.lerp(mid, t1).lerp(zenith, t2)
            } else {
                horizon.lerp(ground, smoothstep(0.0, 0.35, -y))
            };

            // Warm golden sun disc & soft atmospheric scatter
            let sun_dir = Vec3::new(0.55, 0.56, -0.62).normalize();
            let sun_cos = dir.dot(sun_dir).max(0.0);
            let mie = sun_cos.powf(7.0) * 0.26 + sun_cos.powf(48.0) * 0.42;
            let disc = smoothstep(0.9985, 0.9996, sun_cos);
            col += Vec3::new(1.0, 0.94, 0.80) * mie;
            col = col.lerp(Vec3::new(1.0, 0.99, 0.92), disc);

            // Soft wispy daytime clouds projected onto sky dome
            if y > 0.02 {
                let c_uv = Vec2::new(dir.x, dir.z) / (y + 0.22) * 1.45;
                let n = fbm2(c_uv + Vec2::new(3.1, 7.4));
                let cloud = smoothstep(0.54, 0.82, n) * smoothstep(0.02, 0.20, y) * 0.68;
                let cloud_col = Vec3::new(0.98, 0.99, 1.0);
                col = col.lerp(cloud_col, cloud);
            }
            col
        }
        SkyboxPreset::DayCloudy => {
            let zenith = Vec3::new(0.38, 0.52, 0.69);
            let mid = Vec3::new(0.62, 0.72, 0.82);
            let horizon = Vec3::new(0.81, 0.86, 0.91);
            let ground = Vec3::new(0.46, 0.52, 0.59);

            let mut col = if y >= 0.0 {
                let t1 = smoothstep(0.0, 0.28, elev);
                let t2 = smoothstep(0.22, 0.85, elev);
                horizon.lerp(mid, t1).lerp(zenith, t2)
            } else {
                horizon.lerp(ground, smoothstep(0.0, 0.35, -y))
            };

            // Soft diffused sun glow behind cloud deck
            let sun_dir = Vec3::new(0.42, 0.72, -0.55).normalize();
            let sun_cos = dir.dot(sun_dir).max(0.0);
            col += Vec3::new(0.98, 0.97, 0.94) * (sun_cos.powf(5.0) * 0.16);

            // Thick soft cumulus layers
            if y > 0.01 {
                let c_uv = Vec2::new(dir.x, dir.z) / (y + 0.19) * 1.65;
                let base_n = fbm2(c_uv + Vec2::new(11.3, 4.7));
                let detail_n = fbm2(c_uv * 2.1 + Vec2::new(-5.2, 8.9));
                let density = smoothstep(0.36, 0.74, base_n) * smoothstep(0.01, 0.16, y);
                let shade = smoothstep(0.32, 0.75, detail_n);
                let cloud_dark = Vec3::new(0.68, 0.73, 0.79);
                let cloud_bright = Vec3::new(0.95, 0.97, 0.99);
                let cloud_col = cloud_dark.lerp(cloud_bright, shade);
                col = col.lerp(cloud_col, density * 0.86);
            }
            col
        }
        SkyboxPreset::DaySunset => {
            let zenith = Vec3::new(0.11, 0.21, 0.44);
            let upper = Vec3::new(0.42, 0.30, 0.49);
            let warm_band = Vec3::new(0.92, 0.48, 0.28);
            let horizon = Vec3::new(0.99, 0.78, 0.46);
            let ground = Vec3::new(0.18, 0.16, 0.24);

            let mut col = if y >= 0.0 {
                let t1 = smoothstep(0.0, 0.11, elev);
                let t2 = smoothstep(0.09, 0.34, elev);
                let t3 = smoothstep(0.30, 0.85, elev);
                horizon
                    .lerp(warm_band, t1)
                    .lerp(upper, t2)
                    .lerp(zenith, t3)
            } else {
                warm_band.lerp(ground, smoothstep(0.0, 0.28, -y))
            };

            // Low glowing golden-hour sun
            let sun_dir = Vec3::new(0.75, 0.16, -0.64).normalize();
            let sun_cos = dir.dot(sun_dir).max(0.0);
            let glow = sun_cos.powf(4.0) * 0.38 + sun_cos.powf(28.0) * 0.45;
            let disc = smoothstep(0.9980, 0.9994, sun_cos);
            col += Vec3::new(1.0, 0.68, 0.32) * glow;
            col = col.lerp(Vec3::new(1.0, 0.95, 0.80), disc);

            // Sunset-lit clouds
            if y > 0.02 {
                let c_uv = Vec2::new(dir.x, dir.z) / (y + 0.17) * 1.5;
                let n = fbm2(c_uv + Vec2::new(6.2, -3.4));
                let cloud = smoothstep(0.48, 0.78, n) * smoothstep(0.02, 0.18, y);
                let sun_rim = sun_cos.powf(2.0);
                let cloud_col = Vec3::new(0.55, 0.34, 0.46)
                    .lerp(Vec3::new(1.0, 0.66, 0.38), (0.35 + sun_rim * 0.65).min(1.0));
                col = col.lerp(cloud_col, cloud * 0.72);
            }
            col
        }
        SkyboxPreset::NightStarry => {
            let zenith = Vec3::new(0.02, 0.04, 0.10);
            let mid = Vec3::new(0.05, 0.10, 0.21);
            let horizon = Vec3::new(0.11, 0.17, 0.30);
            let ground = Vec3::new(0.03, 0.05, 0.09);

            let mut col = if y >= 0.0 {
                let t1 = smoothstep(0.0, 0.22, elev);
                let t2 = smoothstep(0.18, 0.82, elev);
                horizon.lerp(mid, t1).lerp(zenith, t2)
            } else {
                horizon.lerp(ground, smoothstep(0.0, 0.30, -y))
            };

            // Subtle Milky Way nebula ribbon across upper sky
            if y > 0.05 {
                let band = (1.0 - (dir.x * 0.65 + dir.z * 0.55 - 0.15).abs() * 2.2).max(0.0);
                let mw_noise = fbm2(Vec2::new(dir.x * 4.5, dir.z * 4.5) + Vec2::new(2.0, 9.0));
                let mw = band.powf(2.0) * smoothstep(0.35, 0.80, mw_noise) * smoothstep(0.05, 0.35, y);
                col += Vec3::new(0.16, 0.22, 0.36) * (mw * 0.45);
            }

            // Glowing full moon + subtle maria texture + cool atmospheric halo
            let moon_dir = Vec3::new(-0.52, 0.54, -0.66).normalize();
            let moon_cos = dir.dot(moon_dir).max(0.0);
            let halo = moon_cos.powf(9.0) * 0.22 + moon_cos.powf(60.0) * 0.36;
            col += Vec3::new(0.58, 0.74, 1.0) * halo;
            let moon_disc = smoothstep(0.9986, 0.9993, moon_cos);
            if moon_disc > 0.001 {
                let maria = fbm2(Vec2::new(dir.x * 38.0, dir.y * 38.0));
                let moon_rgb = Vec3::new(0.78, 0.83, 0.90)
                    .lerp(Vec3::new(0.96, 0.98, 1.0), smoothstep(0.35, 0.72, maria));
                col = col.lerp(moon_rgb, moon_disc);
            }

            // Crisp procedural starfield (two density scales, masked near horizon & moon)
            if y > 0.03 && moon_cos < 0.997 {
                let star_vis = smoothstep(0.03, 0.24, y);
                col += sample_star_layer(dir, 58.0, 0.982, 0.95) * star_vis;
                col += sample_star_layer(dir, 105.0, 0.988, 0.62) * star_vis;
            }
            col
        }
        SkyboxPreset::NightTactical => {
            let zenith = Vec3::new(0.015, 0.028, 0.062);
            let mid = Vec3::new(0.035, 0.062, 0.125);
            let horizon = Vec3::new(0.075, 0.118, 0.205);
            let ground = Vec3::new(0.02, 0.03, 0.055);

            let mut col = if y >= 0.0 {
                let t1 = smoothstep(0.0, 0.20, elev);
                let t2 = smoothstep(0.16, 0.80, elev);
                horizon.lerp(mid, t1).lerp(zenith, t2)
            } else {
                horizon.lerp(ground, smoothstep(0.0, 0.30, -y))
            };

            // Restrained cool moon haze & dim crisp stars for tactical readability
            let moon_dir = Vec3::new(-0.45, 0.62, -0.64).normalize();
            let moon_cos = dir.dot(moon_dir).max(0.0);
            col += Vec3::new(0.35, 0.50, 0.78) * (moon_cos.powf(10.0) * 0.14);

            if y > 0.05 {
                let star_vis = smoothstep(0.05, 0.28, y);
                col += sample_star_layer(dir, 64.0, 0.989, 0.55) * star_vis;
            }
            col
        }
    }
}

fn sample_star_layer(dir: Vec3, grid_scale: f32, threshold: f32, intensity: f32) -> Vec3 {
    let p = dir * grid_scale;
    let cell = p.floor();
    let f = p - cell;
    let h1 = hash3(cell);
    if h1 < threshold {
        return Vec3::ZERO;
    }
    let jitter = Vec3::new(
        hash3(cell + Vec3::new(13.7, 2.3, 5.1)),
        hash3(cell + Vec3::new(4.1, 19.3, 8.7)),
        hash3(cell + Vec3::new(7.9, 11.2, 23.5)),
    );
    let center = Vec3::splat(0.18) + jitter * 0.64;
    let dist = f.distance(center);
    let spark = (1.0 - smoothstep(0.05, 0.24, dist)).powf(2.2);
    let mag = ((h1 - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
    let temp = hash3(cell + Vec3::new(31.1, 17.4, 9.2));
    let star_col = if temp < 0.33 {
        Vec3::new(0.78, 0.88, 1.0)
    } else if temp < 0.72 {
        Vec3::new(0.97, 0.98, 1.0)
    } else {
        Vec3::new(1.0, 0.91, 0.78)
    };
    star_col * (spark * (0.35 + 0.65 * mag) * intensity)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash2(p: Vec2) -> f32 {
    let h = p.x * 127.1 + p.y * 311.7;
    (h.sin() * 43_758.547).fract().abs()
}

fn hash3(p: Vec3) -> f32 {
    let h = p.x * 127.1 + p.y * 311.7 + p.z * 74.7;
    (h.sin() * 43_758.547).fract().abs()
}

fn value_noise2(p: Vec2) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec2::splat(3.0) - f * 2.0);
    let a = hash2(i);
    let b = hash2(i + Vec2::new(1.0, 0.0));
    let c = hash2(i + Vec2::new(0.0, 1.0));
    let d = hash2(i + Vec2::new(1.0, 1.0));
    let x1 = a + (b - a) * u.x;
    let x2 = c + (d - c) * u.x;
    x1 + (x2 - x1) * u.y
}

fn fbm2(mut p: Vec2) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.52;
    for _ in 0..4 {
        sum += value_noise2(p) * amp;
        p = Vec2::new(p.x * 1.92 - p.y * 0.45, p.x * 0.45 + p.y * 1.92);
        amp *= 0.5;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skybox_cubemaps_generate_valid_6_face_cube_textures() {
        for &preset in SkyboxPreset::ALL {
            if preset == SkyboxPreset::None {
                continue;
            }
            let img = build_skybox_cubemap_image(preset);
            assert_eq!(img.texture_descriptor.size.width, CUBEMAP_FACE_SIZE);
            assert_eq!(img.texture_descriptor.size.height, CUBEMAP_FACE_SIZE);
            assert_eq!(img.texture_descriptor.size.depth_or_array_layers, 6);
            assert_eq!(
                img.texture_view_descriptor
                    .as_ref()
                    .and_then(|d| d.dimension),
                Some(TextureViewDimension::Cube)
            );
            let expected_bytes = (CUBEMAP_FACE_SIZE * CUBEMAP_FACE_SIZE * 6 * 4) as usize;
            assert_eq!(img.data.as_ref().map(|d| d.len()), Some(expected_bytes));
        }
    }

    #[test]
    fn day_skybox_is_brighter_than_night_skybox_at_zenith() {
        let day = sample_sky_srgb(SkyboxPreset::DayClear, Vec3::Y);
        let night = sample_sky_srgb(SkyboxPreset::NightStarry, Vec3::Y);
        assert!(day.length() > night.length() * 3.0);
    }
}
