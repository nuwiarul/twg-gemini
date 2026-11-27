use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    f32::consts::{FRAC_PI_2, TAU},
    hash::{Hash, Hasher},
};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use twg_shared::{
    environment_anim::{
        RainAnimConfig, RainCoverage, WaterAnimConfig, WaterSimKind, is_rain_anim, is_water_anim,
        parse_rain_anim_config, parse_water_anim_config,
    },
    group_anim::collect_group_descendant_ids,
};
use uuid::Uuid;

use crate::{
    map::camera::TacticalCamera,
    scene::skybox::PrimarySunLight,
    state::{AnimRunStatus, AppState, PlaybackState, SelectedEntity, SkyboxPreset},
};

#[derive(Component)]
pub struct WaterFxEntity;

#[derive(Component)]
pub struct RainFxEntity;

struct WaterSurfaceRig {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    geom_sig: u64,
    /// Base subdivided top-surface XZ vertices (`[x, z]`).
    top_xz: Vec<[f32; 2]>,
    /// Triangle indices referencing `top_xz`.
    top_indices: Vec<u32>,
    /// Perimeter ring XZ vertices (ordered around the polygon outline) for side skirt walls.
    skirt_xz: Vec<[f32; 2]>,
    /// Centroid XZ in local world meters (used for relative wave phase).
    centroid_xz: [f32; 2],
}

impl WaterSurfaceRig {
    fn despawn(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) {
        commands.entity(self.entity).despawn();
        meshes.remove(self.mesh.id());
        materials.remove(self.material.id());
    }
}

struct RainVolumeRig {
    streaks_entity: Entity,
    streaks_mesh: Handle<Mesh>,
    streaks_mat: Handle<StandardMaterial>,
    splashes_entity: Entity,
    splashes_mesh: Handle<Mesh>,
    splashes_mat: Handle<StandardMaterial>,
    cloud_entity: Entity,
    cloud_mat: Handle<StandardMaterial>,
}

impl RainVolumeRig {
    fn despawn(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) {
        commands.entity(self.streaks_entity).despawn();
        commands.entity(self.splashes_entity).despawn();
        commands.entity(self.cloud_entity).despawn();
        meshes.remove(self.streaks_mesh.id());
        meshes.remove(self.splashes_mesh.id());
        materials.remove(self.streaks_mat.id());
        materials.remove(self.splashes_mat.id());
        materials.remove(self.cloud_mat.id());
    }
}

#[derive(Resource, Default)]
pub struct EnvironmentFxPool {
    water_rigs: HashMap<Uuid, WaterSurfaceRig>,
    rain_rigs: HashMap<Uuid, RainVolumeRig>,
    shared_cloud_mesh: Option<Handle<Mesh>>,
    last_skybox: Option<SkyboxPreset>,
    base_sun_lux: Option<f32>,
}

fn parse_hex_rgb(hex: &str, fallback: [f32; 3]) -> [f32; 3] {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        )
    {
        return [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
    }
    fallback
}

#[inline]
fn hash_u32(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x as f32) / (u32::MAX as f32)
}

fn geom_signature(geom: &[[f64; 3]], anchor_lng: f64, anchor_lat: f64) -> u64 {
    let mut h = DefaultHasher::new();
    geom.len().hash(&mut h);
    anchor_lng.to_bits().hash(&mut h);
    anchor_lat.to_bits().hash(&mut h);
    for p in geom {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
    }
    h.finish()
}

/// Builds a subdivided, crack-free triangle mesh covering the exact polygon `ring_xz`,
/// so vertex waves deform smoothly while preserving exact concave river/coastline boundaries.
fn build_subdivided_water_topology(ring_xz: &[[f32; 2]]) -> Option<(Vec<[f32; 2]>, Vec<u32>, Vec<[f32; 2]>)> {
    let mut cleaned: Vec<[f32; 2]> = Vec::with_capacity(ring_xz.len());
    for &p in ring_xz {
        if cleaned
            .last()
            .is_none_or(|prev| (p[0] - prev[0]).hypot(p[1] - prev[1]) > 0.05)
        {
            cleaned.push(p);
        }
    }
    if cleaned.len() >= 3
        && let (Some(&first), Some(&last)) = (cleaned.first(), cleaned.last())
        && (first[0] - last[0]).hypot(first[1] - last[1]) <= 0.05
    {
        cleaned.pop();
    }
    if cleaned.len() < 3 {
        return None;
    }

    // Ensure CCW winding in XZ (so +Y is upward normal when emitting (a, c, b) or (a, b, c))
    let mut signed_area = 0.0_f64;
    for i in 0..cleaned.len() {
        let p0 = cleaned[i];
        let p1 = cleaned[(i + 1) % cleaned.len()];
        signed_area += (p0[0] as f64) * (p1[1] as f64) - (p1[0] as f64) * (p0[1] as f64);
    }
    if signed_area < 0.0 {
        cleaned.reverse();
    }

    let flat: Vec<f64> = cleaned.iter().flat_map(|p| [p[0] as f64, p[1] as f64]).collect();
    let tris = earcutr::earcut(&flat, &[], 2).ok()?;
    if tris.len() < 3 {
        return None;
    }

    let (mut min_x, mut max_x, mut min_z, mut max_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in &cleaned {
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
        min_z = min_z.min(p[1]);
        max_z = max_z.max(p[1]);
    }
    let span = (max_x - min_x).max(max_z - min_z).max(4.0);
    let max_edge = (span / 20.0).clamp(1.5, 45.0);

    let mut verts: Vec<[f32; 2]> = cleaned.clone();
    let mut cur_tris: Vec<[u32; 3]> = tris
        .chunks_exact(3)
        .map(|c| [c[0] as u32, c[1] as u32, c[2] as u32])
        .collect();

    // Up to 5 longest-edge midpoint subdivision passes (deduplicated edges -> zero T-junction cracks!)
    for _ in 0..5 {
        if verts.len() >= 1500 {
            break;
        }
        let mut edge_mid: HashMap<(u32, u32), u32> = HashMap::new();
        let mut next_tris: Vec<[u32; 3]> = Vec::with_capacity(cur_tris.len() * 2);
        let mut any_split = false;

        let mut get_mid = |a: u32, b: u32, verts: &mut Vec<[f32; 2]>| -> u32 {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&idx) = edge_mid.get(&key) {
                return idx;
            }
            let pa = verts[a as usize];
            let pb = verts[b as usize];
            let idx = verts.len() as u32;
            verts.push([(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5]);
            edge_mid.insert(key, idx);
            idx
        };

        for [i0, i1, i2] in cur_tris {
            let p0 = verts[i0 as usize];
            let p1 = verts[i1 as usize];
            let p2 = verts[i2 as usize];
            let d01 = (p0[0] - p1[0]).hypot(p0[1] - p1[1]);
            let d12 = (p1[0] - p2[0]).hypot(p1[1] - p2[1]);
            let d20 = (p2[0] - p0[0]).hypot(p2[1] - p0[1]);
            let s01 = d01 > max_edge;
            let s12 = d12 > max_edge;
            let s20 = d20 > max_edge;

            match (s01, s12, s20) {
                (false, false, false) => next_tris.push([i0, i1, i2]),
                (true, true, true) => {
                    any_split = true;
                    let m01 = get_mid(i0, i1, &mut verts);
                    let m12 = get_mid(i1, i2, &mut verts);
                    let m20 = get_mid(i2, i0, &mut verts);
                    next_tris.push([i0, m01, m20]);
                    next_tris.push([m01, i1, m12]);
                    next_tris.push([m20, m12, i2]);
                    next_tris.push([m01, m12, m20]);
                }
                (true, false, false) => {
                    any_split = true;
                    let m = get_mid(i0, i1, &mut verts);
                    next_tris.push([i0, m, i2]);
                    next_tris.push([m, i1, i2]);
                }
                (false, true, false) => {
                    any_split = true;
                    let m = get_mid(i1, i2, &mut verts);
                    next_tris.push([i0, i1, m]);
                    next_tris.push([i0, m, i2]);
                }
                (false, false, true) => {
                    any_split = true;
                    let m = get_mid(i2, i0, &mut verts);
                    next_tris.push([i0, i1, m]);
                    next_tris.push([m, i1, i2]);
                }
                (true, true, false) => {
                    any_split = true;
                    let m01 = get_mid(i0, i1, &mut verts);
                    let m12 = get_mid(i1, i2, &mut verts);
                    next_tris.push([m01, i1, m12]);
                    if d01 >= d12 {
                        next_tris.push([i0, m01, m12]);
                        next_tris.push([i0, m12, i2]);
                    } else {
                        next_tris.push([i0, m01, i2]);
                        next_tris.push([m01, m12, i2]);
                    }
                }
                (false, true, true) => {
                    any_split = true;
                    let m12 = get_mid(i1, i2, &mut verts);
                    let m20 = get_mid(i2, i0, &mut verts);
                    next_tris.push([m20, m12, i2]);
                    if d12 >= d20 {
                        next_tris.push([i0, i1, m12]);
                        next_tris.push([i0, m12, m20]);
                    } else {
                        next_tris.push([i0, i1, m20]);
                        next_tris.push([i1, m12, m20]);
                    }
                }
                (true, false, true) => {
                    any_split = true;
                    let m01 = get_mid(i0, i1, &mut verts);
                    let m20 = get_mid(i2, i0, &mut verts);
                    next_tris.push([i0, m01, m20]);
                    if d01 >= d20 {
                        next_tris.push([m01, i1, i2]);
                        next_tris.push([m01, i2, m20]);
                    } else {
                        next_tris.push([m01, i1, m20]);
                        next_tris.push([m20, i1, i2]);
                    }
                }
            }
        }
        cur_tris = next_tris;
        if !any_split {
            break;
        }
    }

    // Verify upward winding (+Y) on top surface: in right-handed Bevy (X right, Y up, Z south),
    // cross(p1 - p0, p2 - p0).y = (z1 - z0)*(x2 - x0) - (x1 - x0)*(z2 - z0).
    let mut indices = Vec::with_capacity(cur_tris.len() * 3);
    for [a, b, c] in cur_tris {
        let pa = verts[a as usize];
        let pb = verts[b as usize];
        let pc = verts[c as usize];
        let ny = (pb[1] - pa[1]) * (pc[0] - pa[0]) - (pb[0] - pa[0]) * (pc[1] - pa[1]);
        if ny >= 0.0 {
            indices.extend_from_slice(&[a, b, c]);
        } else {
            indices.extend_from_slice(&[a, c, b]);
        }
    }

    // Subdivide perimeter ring moderately so flood skirt walls match top wave edges smoothly
    let mut skirt = Vec::with_capacity(cleaned.len() * 2);
    for i in 0..cleaned.len() {
        let a = cleaned[i];
        let b = cleaned[(i + 1) % cleaned.len()];
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let segs = ((len / max_edge).ceil() as usize).clamp(1, 16);
        for s in 0..segs {
            let f = s as f32 / segs as f32;
            skirt.push([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
        }
    }

    Some((verts, indices, skirt))
}

#[inline]
fn eval_water_wave(
    x: f32,
    z: f32,
    t: f32,
    cfg: &WaterAnimConfig,
    fx: f32,
    fz: f32,
) -> (f32, f32, f32, f32) {
    let amp = cfg.wave_height_m as f32;
    if amp <= 1e-4 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let freq_scale = cfg.wave_frequency as f32;
    let speed = cfg.flow_speed_mps as f32;
    let px = -fz;
    let pz = fx;

    let (k1, k2, k3, a1, a2, a3, s1, s2, s3) = match cfg.kind {
        WaterSimKind::Ocean => (
            0.14 * freq_scale,
            0.23 * freq_scale,
            0.41 * freq_scale,
            amp * 0.62,
            amp * 0.28,
            amp * 0.14,
            speed * 0.85 + 0.9,
            speed * 1.15 + 1.3,
            speed * 1.65 + 2.1,
        ),
        WaterSimKind::River => (
            0.28 * freq_scale,
            0.52 * freq_scale,
            0.85 * freq_scale,
            amp * 0.55,
            amp * 0.30,
            amp * 0.18,
            speed * 1.45 + 1.2,
            speed * 1.85 + 1.8,
            speed * 2.40 + 2.5,
        ),
        WaterSimKind::Flood => (
            0.18 * freq_scale,
            0.34 * freq_scale,
            0.60 * freq_scale,
            amp * 0.50,
            amp * 0.32,
            amp * 0.18,
            speed * 0.90 + 0.6,
            speed * 1.25 + 1.1,
            speed * 1.70 + 1.7,
        ),
    };

    // Wave 1 along flow direction
    let d1x = fx;
    let d1z = fz;
    let ph1 = k1 * (x * d1x + z * d1z) - s1 * t;
    let h1 = a1 * ph1.sin();
    let dh1 = a1 * k1 * ph1.cos();

    // Wave 2 angled +35°
    let c2 = 0.819_f32;
    let sn2 = 0.574_f32;
    let d2x = fx * c2 + px * sn2;
    let d2z = fz * c2 + pz * sn2;
    let ph2 = k2 * (x * d2x + z * d2z) - s2 * t;
    let h2 = a2 * ph2.sin();
    let dh2 = a2 * k2 * ph2.cos();

    // Wave 3 angled -48°
    let c3 = 0.669_f32;
    let sn3 = -0.743_f32;
    let d3x = fx * c3 + px * sn3;
    let d3z = fz * c3 + pz * sn3;
    let ph3 = k3 * (x * d3x + z * d3z) - s3 * t;
    let h3 = a3 * ph3.sin();
    let dh3 = a3 * k3 * ph3.cos();

    let h = h1 + h2 + h3;
    let dh_dx = dh1 * d1x + dh2 * d2x + dh3 * d3x;
    let dh_dz = dh1 * d1z + dh2 * d2z + dh3 * d3z;
    let norm_crest = ((h / amp.max(0.01)) * 0.5 + 0.5).clamp(0.0, 1.0);
    (h, dh_dx, dh_dz, norm_crest)
}

fn build_animated_water_mesh(
    rig: &WaterSurfaceRig,
    cfg: &WaterAnimConfig,
    level_m: f32,
    t: f32,
) -> Mesh {
    let dir_rad = (cfg.flow_dir_deg as f32).to_radians();
    let fx = dir_rad.sin();
    let fz = -dir_rad.cos();
    let base_rgb = parse_hex_rgb(&cfg.color_hex, [0.05, 0.36, 0.54]);
    let alpha = cfg.opacity as f32;
    let foam_k = cfg.foam_intensity as f32;

    let top_n = rig.top_xz.len();
    let skirt_n = rig.skirt_xz.len();
    let has_skirt = level_m > 0.14 && skirt_n >= 3;

    let mut positions = Vec::with_capacity(top_n + if has_skirt { skirt_n * 4 } else { 0 });
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(rig.top_indices.len() + if has_skirt { skirt_n * 6 } else { 0 });

    for &[x, z] in &rig.top_xz {
        let rx = x - rig.centroid_xz[0];
        let rz = z - rig.centroid_xz[1];
        let (h, dh_dx, dh_dz, crest) = eval_water_wave(rx, rz, t, cfg, fx, fz);
        let y = (level_m + h).max(0.03);
        let n = Vec3::new(-dh_dx, 1.0, -dh_dz).normalize_or_zero();
        positions.push([x, y, z]);
        normals.push([n.x, n.y, n.z]);
        uvs.push([rx * 0.05, rz * 0.05]);

        // Deepen troughs and add crisp bright foam on crests
        let shade = 0.72 + 0.36 * crest;
        let foam = ((crest - 0.62).max(0.0) / 0.38).powf(1.8) * foam_k;
        let r = (base_rgb[0] * shade + foam * 0.75).clamp(0.0, 1.0);
        let g = (base_rgb[1] * shade + foam * 0.80).clamp(0.0, 1.0);
        let b = (base_rgb[2] * shade + foam * 0.85).clamp(0.0, 1.0);
        let a = (alpha + foam * 0.14).clamp(0.2, 0.98);
        colors.push([r, g, b, a]);
    }
    indices.extend_from_slice(&rig.top_indices);

    // Side skirt walls so elevated flood/river water has solid visible thickness
    if has_skirt {
        let y_bot = 0.01_f32;
        let wall_col = [
            base_rgb[0] * 0.55,
            base_rgb[1] * 0.62,
            base_rgb[2] * 0.68,
            (alpha * 0.92).clamp(0.25, 0.96),
        ];
        for i in 0..skirt_n {
            let a = rig.skirt_xz[i];
            let b = rig.skirt_xz[(i + 1) % skirt_n];
            let (ha, _, _, _) = eval_water_wave(
                a[0] - rig.centroid_xz[0],
                a[1] - rig.centroid_xz[1],
                t,
                cfg,
                fx,
                fz,
            );
            let (hb, _, _, _) = eval_water_wave(
                b[0] - rig.centroid_xz[0],
                b[1] - rig.centroid_xz[1],
                t,
                cfg,
                fx,
                fz,
            );
            let ya = (level_m + ha).max(0.04);
            let yb = (level_m + hb).max(0.04);
            let edge = Vec2::new(b[0] - a[0], b[1] - a[1]);
            let out_n = Vec3::new(edge.y, 0.0, -edge.x).normalize_or_zero();
            let base = positions.len() as u32;
            positions.push([a[0], y_bot, a[1]]);
            positions.push([b[0], y_bot, b[1]]);
            positions.push([b[0], yb, b[1]]);
            positions.push([a[0], ya, a[1]]);
            for _ in 0..4 {
                normals.push([out_n.x, 0.0, out_n.z]);
                colors.push(wall_col);
            }
            uvs.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn build_rain_streaks_mesh(
    cfg: &RainAnimConfig,
    center_xz: Vec2,
    radius_m: f32,
    ceiling_m: f32,
    cam_pos: Vec3,
    t: f32,
    ramp: f32,
) -> Mesh {
    let drop_count = ((360.0 * cfg.intensity as f32 * ramp).round() as usize).clamp(0, 760);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(drop_count * 4);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(drop_count * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(drop_count * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(drop_count * 6);

    let fall_spd = cfg.fall_speed_mps as f32;
    let wind_rad = (cfg.wind_dir_deg as f32).to_radians();
    let wind_spd = cfg.wind_speed_mps as f32;
    let vel = Vec3::new(wind_rad.sin() * wind_spd, -fall_spd, -wind_rad.cos() * wind_spd);
    let vel_dir = vel.normalize_or_zero();
    let streak_len = (cfg.drop_length_m as f32) * (0.75 + 0.004 * radius_m).clamp(0.8, 2.4);
    let streak_half_w = (0.045 + radius_m * 0.00045).clamp(0.045, 0.22);
    let rgb = parse_hex_rgb(&cfg.color_hex, [0.81, 0.91, 1.0]);
    let tip_alpha = (0.62 * ramp).clamp(0.0, 0.82);

    let cycle_dur = (ceiling_m / fall_spd.max(5.0)).max(0.4);

    for i in 0..drop_count {
        let idx = i as u32 + 1;
        let phase_off = hash_u32(idx.wrapping_mul(73_856_093));
        let u_raw = (t / cycle_dur) + phase_off;
        let cycle_id = u_raw.floor() as i32 as u32;
        let frac = u_raw.fract();

        let seed = idx
            .wrapping_mul(19_349_663)
            .wrapping_add(cycle_id.wrapping_mul(83_492_791));
        let r_frac = hash_u32(seed).sqrt();
        let ang = hash_u32(seed.wrapping_add(17)) * TAU;
        let base_x = center_xz.x + ang.cos() * r_frac * radius_m;
        let base_z = center_xz.y + ang.sin() * r_frac * radius_m;

        // Drop descends from ceiling_m to 0 while drifting with wind
        let fall_t = frac * cycle_dur;
        let tip = Vec3::new(
            base_x + vel.x * (fall_t - cycle_dur * 0.5),
            ceiling_m * (1.0 - frac),
            base_z + vel.z * (fall_t - cycle_dur * 0.5),
        );
        let tail = tip - vel_dir * streak_len;

        let to_cam = (cam_pos - tip).normalize_or_zero();
        let side = vel_dir.cross(to_cam).normalize_or_zero() * streak_half_w;
        if side.length_squared() < 1e-8 {
            continue;
        }

        let base = positions.len() as u32;
        positions.push((tip - side).to_array());
        positions.push((tip + side).to_array());
        positions.push((tail + side).to_array());
        positions.push((tail - side).to_array());
        for _ in 0..4 {
            normals.push([0.0, 1.0, 0.0]);
        }
        colors.push([rgb[0], rgb[1], rgb[2], tip_alpha]);
        colors.push([rgb[0], rgb[1], rgb[2], tip_alpha]);
        colors.push([rgb[0], rgb[1], rgb[2], 0.02]);
        colors.push([rgb[0], rgb[1], rgb[2], 0.02]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn build_rain_splashes_mesh(
    cfg: &RainAnimConfig,
    center_xz: Vec2,
    radius_m: f32,
    t: f32,
    ramp: f32,
) -> Mesh {
    let count = if cfg.splash_enabled {
        ((90.0 * cfg.intensity as f32 * ramp).round() as usize).clamp(0, 160)
    } else {
        0
    };
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(count * 8);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(count * 8);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(count * 8);
    let mut indices: Vec<u32> = Vec::with_capacity(count * 24);

    let rgb = parse_hex_rgb(&cfg.color_hex, [0.85, 0.94, 1.0]);
    let splash_dur = 0.34_f32;
    let max_r = (0.35 + radius_m * 0.0028).clamp(0.35, 1.4);

    for i in 0..count {
        let idx = i as u32 + 991;
        let phase = hash_u32(idx.wrapping_mul(265_443_576));
        let u = t / splash_dur + phase;
        let cyc = u.floor() as i32 as u32;
        let f = u.fract();

        let seed = idx
            .wrapping_mul(1_597_334_677)
            .wrapping_add(cyc.wrapping_mul(381_201_580));
        let r_frac = hash_u32(seed).sqrt();
        let ang = hash_u32(seed.wrapping_add(31)) * TAU;
        let cx = center_xz.x + ang.cos() * r_frac * radius_m;
        let cz = center_xz.y + ang.sin() * r_frac * radius_m;

        let r_out = max_r * (0.18 + 0.82 * f);
        let r_in = (r_out - (0.08 + max_r * 0.14) * (1.0 - f * 0.5)).max(0.02);
        let alpha = (1.0 - f).powi(2) * 0.52 * ramp;
        let col = [rgb[0], rgb[1], rgb[2], alpha];

        let base = positions.len() as u32;
        for k in 0..4u32 {
            let a = (k as f32) * FRAC_PI_2;
            let ca = a.cos();
            let sa = a.sin();
            positions.push([cx + ca * r_in, 0.09, cz + sa * r_in]);
            positions.push([cx + ca * r_out, 0.09, cz + sa * r_out]);
            normals.push([0.0, 1.0, 0.0]);
            normals.push([0.0, 1.0, 0.0]);
            colors.push(col);
            colors.push(col);
        }
        for k in 0..4u32 {
            let nk = (k + 1) % 4;
            let i0 = base + k * 2;
            let o0 = i0 + 1;
            let i1 = base + nk * 2;
            let o1 = i1 + 1;
            indices.extend_from_slice(&[i0, o1, o0, i0, i1, o1]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Evaluates deterministic lightning flash factor (`0.0..=1.0`) and optional 3D jagged bolt
/// polyline at `t`.
fn eval_lightning_bolt(
    cfg: &RainAnimConfig,
    center_xz: Vec2,
    radius_m: f32,
    ceiling_m: f32,
    t: f32,
) -> (f32, Option<Vec<Vec3>>) {
    if !cfg.lightning_enabled {
        return (0.0, None);
    }
    let interval = cfg.lightning_interval_sec as f32;
    let slot = (t / interval).floor() as i32 as u32;
    let local_t = t - (slot as f32) * interval;
    let trigger_offset = 0.25 + hash_u32(slot.wrapping_mul(918_273) + 7) * (interval * 0.65);
    let dt = local_t - trigger_offset;
    if !(0.0..=0.26).contains(&dt) {
        return (0.0, None);
    }

    // Double-flicker lightning envelope
    let flash = if dt < 0.07 {
        1.0
    } else if dt < 0.12 {
        0.25
    } else if dt < 0.18 {
        0.85
    } else {
        ((0.26 - dt) / 0.08).clamp(0.0, 1.0)
    };

    let seed = slot.wrapping_mul(2_654_435_761).wrapping_add(1337);
    let ang = hash_u32(seed) * TAU;
    let r = (0.15 + 0.72 * hash_u32(seed + 1)) * radius_m;
    let bx = center_xz.x + ang.cos() * r;
    let bz = center_xz.y + ang.sin() * r;
    let top_y = (ceiling_m * 1.15).max(55.0);

    let segs = 10usize;
    let mut pts = Vec::with_capacity(segs + 1);
    for s in 0..=segs {
        let f = s as f32 / segs as f32;
        let jitter = if s == 0 || s == segs {
            0.0
        } else {
            (radius_m * 0.07).clamp(2.0, 14.0)
        };
        let jx = (hash_u32(seed + (s as u32) * 19 + 3) - 0.5) * 2.0 * jitter;
        let jz = (hash_u32(seed + (s as u32) * 37 + 5) - 0.5) * 2.0 * jitter;
        pts.push(Vec3::new(bx + jx, top_y * (1.0 - f), bz + jz));
    }
    (flash, Some(pts))
}

#[allow(clippy::too_many_arguments)]
pub fn environment_fx_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<AppState>,
    playback: Res<PlaybackState>,
    mut pool: ResMut<EnvironmentFxPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam_q: Query<(&TacticalCamera, &GlobalTransform)>,
    mut sun_q: Query<&mut DirectionalLight, With<PrimarySunLight>>,
    mut tf_q: Query<(&mut Transform, &mut Visibility), Or<(With<WaterFxEntity>, With<RainFxEntity>)>>,
    mut gizmos: Gizmos,
) {
    if !state.is_3d_map_screen() {
        for (_, rig) in pool.water_rigs.drain() {
            rig.despawn(&mut commands, &mut meshes, &mut materials);
        }
        for (_, rig) in pool.rain_rigs.drain() {
            rig.despawn(&mut commands, &mut meshes, &mut materials);
        }
        state.environment_status.clear();
        return;
    }

    // Track baseline sunlight whenever skybox preset changes
    if pool.last_skybox != Some(state.active_skybox) {
        pool.last_skybox = Some(state.active_skybox);
        if let Ok(sun) = sun_q.single() {
            pool.base_sun_lux = Some(sun.illuminance);
        }
    } else if pool.base_sun_lux.is_none()
        && let Ok(sun) = sun_q.single()
    {
        pool.base_sun_lux = Some(sun.illuminance);
    }

    let Ok((tcam, cam_gtf)) = cam_q.single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();
    let wall_t = time.elapsed_secs();

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

    let mut active_water_ids = HashSet::new();
    let mut active_rain_ids = HashSet::new();
    let mut max_overcast = 0.0_f32;
    let mut max_lightning_flash = 0.0_f32;

    let env_anims: Vec<_> = state
        .anims
        .iter()
        .filter(|a| is_water_anim(a) || is_rain_anim(a))
        .cloned()
        .collect();

    for anim in &env_anims {
        let hidden = state.hidden_entity_ids.contains(&anim.id);
        let is_selected = state.selected == SelectedEntity::Anim(anim.id);
        let track = playback.tracks.get(&anim.id);
        let in_scope = preview_scope
            .as_ref()
            .is_none_or(|s| s.contains(&anim.id));

        if is_water_anim(anim) {
            if anim.geom.len() < 3 || hidden {
                continue;
            }
            let cfg = parse_water_anim_config(anim);
            let track_active = in_scope
                && track.is_some_and(|tr| {
                    matches!(tr.status, AnimRunStatus::Playing | AnimRunStatus::Paused)
                });
            let track_finished = in_scope
                && track.is_some_and(|tr| tr.status == AnimRunStatus::Finished);

            let (should_show, sim_t, level_m, status_txt) = if playback.play_test_mode {
                if track_active || track_finished {
                    let tr_t = track.map(|tr| tr.distance_m).unwrap_or(0.0);
                    if tr_t < cfg.delay_sec {
                        (
                            cfg.kind != WaterSimKind::Flood,
                            0.0_f32,
                            cfg.start_level_m as f32,
                            format!("Jeda awal: {:.1} dtk", cfg.delay_sec - tr_t),
                        )
                    } else {
                        let el = tr_t - cfg.delay_sec;
                        let lvl = cfg.water_level_at(el) as f32;
                        let lbl = if cfg.kind == WaterSimKind::Flood {
                            format!("Muka air: {:.2} m  •  {:.1} dtk", lvl, el)
                        } else {
                            format!("{} aktif  •  {:.1} dtk", cfg.kind.label(), el)
                        };
                        (true, el as f32, lvl, lbl)
                    }
                } else if cfg.kind != WaterSimKind::Flood {
                    // Ocean & River remain gently visible in Play Skenario even before Play!
                    (true, wall_t * 0.45, cfg.start_level_m as f32, "Standby".into())
                } else {
                    (false, 0.0, cfg.start_level_m as f32, "Standby (belum naik)".into())
                }
            } else if is_editor_preview && track_active {
                let tr_t = track.map(|tr| tr.distance_m).unwrap_or(0.0);
                let el = (tr_t - cfg.delay_sec).max(0.0);
                let lvl = cfg.water_level_at(el) as f32;
                (
                    true,
                    el as f32,
                    lvl,
                    format!("Preview  •  Muka air {:.2} m  •  {:.1} dtk", lvl, el),
                )
            } else if cfg.always_preview_in_editor || is_selected {
                let lvl = if cfg.kind == WaterSimKind::Flood {
                    cfg.target_level_m as f32
                } else {
                    cfg.start_level_m as f32
                };
                (
                    true,
                    wall_t,
                    lvl,
                    format!("Preview Editor  •  Muka air {:.2} m", lvl),
                )
            } else {
                (false, 0.0, cfg.start_level_m as f32, "Standby".into())
            };

            state.environment_status.insert(anim.id, status_txt);
            if !should_show {
                continue;
            }
            active_water_ids.insert(anim.id);

            let sig = geom_signature(
                &anim.geom,
                state.geo_anchor.center_lng,
                state.geo_anchor.center_lat,
            );
            let needs_rebuild = pool
                .water_rigs
                .get(&anim.id)
                .is_none_or(|r| r.geom_sig != sig);
            if needs_rebuild {
                if let Some(old) = pool.water_rigs.remove(&anim.id) {
                    old.despawn(&mut commands, &mut meshes, &mut materials);
                }
                let ring_xz: Vec<[f32; 2]> = anim
                    .geom
                    .iter()
                    .map(|p| {
                        let l = state.geo_anchor.lng_lat_alt_to_local(p[0], p[1], 0.0);
                        [l[0], l[2]]
                    })
                    .collect();
                if let Some((top_xz, top_indices, skirt_xz)) = build_subdivided_water_topology(&ring_xz)
                {
                    let n_inv = 1.0 / (ring_xz.len() as f32);
                    let cx = ring_xz.iter().map(|p| p[0]).sum::<f32>() * n_inv;
                    let cz = ring_xz.iter().map(|p| p[1]).sum::<f32>() * n_inv;

                    let mesh_h = meshes.add(Mesh::new(
                        PrimitiveTopology::TriangleList,
                        RenderAssetUsages::default(),
                    ));
                    let mat_h = materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        perceptual_roughness: 0.14,
                        metallic: 0.12,
                        reflectance: 0.92,
                        alpha_mode: AlphaMode::Blend,
                        cull_mode: None,
                        ..default()
                    });
                    let entity = commands
                        .spawn((
                            Mesh3d(mesh_h.clone()),
                            MeshMaterial3d(mat_h.clone()),
                            Transform::IDENTITY,
                            Visibility::Visible,
                            WaterFxEntity,
                        ))
                        .id();
                    pool.water_rigs.insert(
                        anim.id,
                        WaterSurfaceRig {
                            entity,
                            mesh: mesh_h,
                            material: mat_h,
                            geom_sig: sig,
                            top_xz,
                            top_indices,
                            skirt_xz,
                            centroid_xz: [cx, cz],
                        },
                    );
                }
            }

            if let Some(rig) = pool.water_rigs.get(&anim.id) {
                let updated_mesh = build_animated_water_mesh(rig, &cfg, level_m, sim_t);
                if let Some(mut m) = meshes.get_mut(&rig.mesh) {
                    *m = updated_mesh;
                }
                if let Some(mut mat) = materials.get_mut(&rig.material) {
                    mat.perceptual_roughness = match cfg.kind {
                        WaterSimKind::Ocean => 0.12,
                        WaterSimKind::River => 0.16,
                        WaterSimKind::Flood => 0.28,
                    };
                }
                // Show polygon outline gizmo in Plot Editor when selected
                if !playback.play_test_mode && is_selected && rig.skirt_xz.len() >= 3 {
                    let outline_col = Color::srgb(0.22, 0.88, 0.98);
                    let y_line = (level_m + 0.15).max(0.2);
                    for i in 0..rig.skirt_xz.len() {
                        let a = rig.skirt_xz[i];
                        let b = rig.skirt_xz[(i + 1) % rig.skirt_xz.len()];
                        gizmos.line(
                            Vec3::new(a[0], y_line, a[1]),
                            Vec3::new(b[0], y_line, b[1]),
                            outline_col,
                        );
                    }
                }
            }
        } else if is_rain_anim(anim) {
            if hidden {
                continue;
            }
            let cfg = parse_rain_anim_config(anim);
            let track_active = in_scope
                && track.is_some_and(|tr| {
                    matches!(tr.status, AnimRunStatus::Playing | AnimRunStatus::Paused)
                });
            let (should_show, sim_t, ramp, status_txt) = if playback.play_test_mode {
                if track_active {
                    let tr_t = track.map(|tr| tr.distance_m).unwrap_or(0.0);
                    if tr_t < cfg.delay_sec {
                        (
                            false,
                            0.0_f32,
                            0.0_f32,
                            format!("Jeda awal: {:.1} dtk", cfg.delay_sec - tr_t),
                        )
                    } else {
                        let el = tr_t - cfg.delay_sec;
                        let r = if cfg.ramp_up_sec > 1e-3 {
                            (el / cfg.ramp_up_sec).clamp(0.0, 1.0) as f32
                        } else {
                            1.0
                        };
                        (
                            true,
                            el as f32,
                            r,
                            format!("{} aktif  •  {:.1} dtk", cfg.preset.label(), el),
                        )
                    }
                } else {
                    (false, 0.0, 0.0, "Standby".into())
                }
            } else if is_editor_preview && track_active {
                let tr_t = track.map(|tr| tr.distance_m).unwrap_or(0.0);
                let el = (tr_t - cfg.delay_sec).max(0.0);
                let r = if cfg.ramp_up_sec > 1e-3 {
                    (el / cfg.ramp_up_sec).clamp(0.0, 1.0) as f32
                } else {
                    1.0
                };
                (
                    true,
                    el as f32,
                    r,
                    format!("Preview {}  •  {:.1} dtk", cfg.preset.label(), el),
                )
            } else if cfg.always_preview_in_editor {
                (true, wall_t, 1.0, format!("Preview Editor ({})", cfg.preset.label()))
            } else {
                // Draw local zone outline in Plot Editor when selected even if rain preview is off
                if is_selected && cfg.coverage == RainCoverage::LocalArea && cfg.has_playable_setup(anim) {
                    let l = state.geo_anchor.lng_lat_alt_to_local(cfg.lng, cfg.lat, 0.25);
                    let iso = Isometry3d::new(
                        Vec3::new(l[0], 0.25, l[2]),
                        Quat::from_rotation_x(FRAC_PI_2),
                    );
                    gizmos.circle(iso, cfg.radius_m as f32, Color::srgb(0.38, 0.74, 0.98));
                }
                (false, 0.0, 0.0, "Standby".into())
            };

            state.environment_status.insert(anim.id, status_txt);
            if !should_show || !cfg.has_playable_setup(anim) {
                continue;
            }
            active_rain_ids.insert(anim.id);

            let (center_xz, radius_m, ceiling_m, show_cloud) = match cfg.coverage {
                RainCoverage::Global => {
                    let r = (tcam.smooth_distance * 0.72).clamp(38.0, 380.0);
                    let h = (tcam.smooth_distance * 0.62).clamp(32.0, 240.0);
                    (tcam.smooth_target, r, h, false)
                }
                RainCoverage::LocalArea => {
                    let l = state.geo_anchor.lng_lat_alt_to_local(cfg.lng, cfg.lat, 0.0);
                    let r = cfg.radius_m as f32;
                    let h = (r * 0.65).clamp(36.0, 190.0);
                    (Vec2::new(l[0], l[2]), r, h, true)
                }
            };

            // Draw local zone ground ring in Editor when selected
            if !playback.play_test_mode && is_selected && show_cloud {
                let iso = Isometry3d::new(
                    Vec3::new(center_xz.x, 0.25, center_xz.y),
                    Quat::from_rotation_x(FRAC_PI_2),
                );
                gizmos.circle(iso, radius_m, Color::srgb(0.38, 0.74, 0.98));
            }

            if !pool.rain_rigs.contains_key(&anim.id) {
                let cloud_mesh = pool
                    .shared_cloud_mesh
                    .get_or_insert_with(|| meshes.add(Circle::new(1.0)))
                    .clone();
                let streaks_mesh = meshes.add(Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                ));
                let streaks_mat = materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    ..default()
                });
                let streaks_entity = commands
                    .spawn((
                        Mesh3d(streaks_mesh.clone()),
                        MeshMaterial3d(streaks_mat.clone()),
                        Transform::IDENTITY,
                        Visibility::Visible,
                        RainFxEntity,
                    ))
                    .id();

                let splashes_mesh = meshes.add(Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                ));
                let splashes_mat = materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    ..default()
                });
                let splashes_entity = commands
                    .spawn((
                        Mesh3d(splashes_mesh.clone()),
                        MeshMaterial3d(splashes_mat.clone()),
                        Transform::IDENTITY,
                        Visibility::Visible,
                        RainFxEntity,
                    ))
                    .id();

                let cloud_mat = materials.add(StandardMaterial {
                    base_color: Color::srgba(0.18, 0.22, 0.28, 0.42),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    ..default()
                });
                let cloud_entity = commands
                    .spawn((
                        Mesh3d(cloud_mesh),
                        MeshMaterial3d(cloud_mat.clone()),
                        Transform::IDENTITY,
                        Visibility::Hidden,
                        RainFxEntity,
                    ))
                    .id();

                pool.rain_rigs.insert(
                    anim.id,
                    RainVolumeRig {
                        streaks_entity,
                        streaks_mesh,
                        streaks_mat,
                        splashes_entity,
                        splashes_mesh,
                        splashes_mat,
                        cloud_entity,
                        cloud_mat,
                    },
                );
            }

            if let Some(rig) = pool.rain_rigs.get(&anim.id) {
                let s_mesh =
                    build_rain_streaks_mesh(&cfg, center_xz, radius_m, ceiling_m, cam_pos, sim_t, ramp);
                if let Some(mut m) = meshes.get_mut(&rig.streaks_mesh) {
                    *m = s_mesh;
                }
                let sp_mesh = build_rain_splashes_mesh(&cfg, center_xz, radius_m, sim_t, ramp);
                if let Some(mut m) = meshes.get_mut(&rig.splashes_mesh) {
                    *m = sp_mesh;
                }
                if let Ok((mut ctf, mut cvis)) = tf_q.get_mut(rig.cloud_entity) {
                    if show_cloud {
                        *ctf = Transform::from_xyz(center_xz.x, ceiling_m, center_xz.y)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                            .with_scale(Vec3::splat(radius_m * 1.05));
                        *cvis = Visibility::Visible;
                    } else {
                        *cvis = Visibility::Hidden;
                    }
                }
                if let Some(mut cmat) = materials.get_mut(&rig.cloud_mat) {
                    cmat.base_color = Color::srgba(
                        0.15,
                        0.19,
                        0.26,
                        (0.38 * ramp * (0.5 + cfg.overcast_strength as f32 * 0.5)).clamp(0.0, 0.55),
                    );
                }
            }

            max_overcast = max_overcast.max((cfg.overcast_strength as f32) * ramp);
            let (flash, bolt) = eval_lightning_bolt(&cfg, center_xz, radius_m, ceiling_m, sim_t);
            if flash > 0.01 {
                max_lightning_flash = max_lightning_flash.max(flash * ramp);
                if let Some(pts) = bolt {
                    let bolt_col = Color::srgba(0.88, 0.95, 1.0, flash);
                    for w in pts.windows(2) {
                        gizmos.line(w[0], w[1], bolt_col);
                    }
                }
            }
        }
    }

    // Despawn inactive water/rain rigs
    let stale_water: Vec<Uuid> = pool
        .water_rigs
        .keys()
        .copied()
        .filter(|id| !active_water_ids.contains(id))
        .collect();
    for id in stale_water {
        if let Some(rig) = pool.water_rigs.remove(&id) {
            rig.despawn(&mut commands, &mut meshes, &mut materials);
        }
    }

    let stale_rain: Vec<Uuid> = pool
        .rain_rigs
        .keys()
        .copied()
        .filter(|id| !active_rain_ids.contains(id))
        .collect();
    for id in stale_rain {
        if let Some(rig) = pool.rain_rigs.remove(&id) {
            rig.despawn(&mut commands, &mut meshes, &mut materials);
        }
    }

    // Apply atmospheric overcast dimming & lightning flash cleanly onto PrimarySunLight
    if let (Some(base_lux), Ok(mut sun)) = (pool.base_sun_lux, sun_q.single_mut()) {
        let dim_mult = (1.0 - max_overcast * 0.56).clamp(0.32, 1.0);
        let flash_boost = max_lightning_flash * base_lux * 1.35;
        let target_lux = base_lux * dim_mult + flash_boost;
        sun.illuminance += (target_lux - sun.illuminance) * (time.delta_secs() * 14.0).clamp(0.0, 1.0);
    }
}
