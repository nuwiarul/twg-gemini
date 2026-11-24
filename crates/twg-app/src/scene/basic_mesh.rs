use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::hash::{Hash, Hasher};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    math::Affine2,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::Face,
};
use serde::{Deserialize, Serialize};
use twg_shared::models::Marker;
use uuid::Uuid;

pub const BASIC_MESH_CATEGORY: &str = "basic_mesh";
pub const BASIC_MESH_PATH_PREFIX: &str = "procedural://";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BasicMeshShape {
    Plane,
    #[default]
    Cube,
    Wall,
    Cylinder,
    Sphere,
    Cone,
    Polygon,
    Ramp,
    Torus,
    Capsule,
}

impl BasicMeshShape {
    pub const ALL: &'static [BasicMeshShape] = &[
        Self::Plane,
        Self::Cube,
        Self::Wall,
        Self::Cylinder,
        Self::Sphere,
        Self::Cone,
        Self::Polygon,
        Self::Ramp,
        Self::Torus,
        Self::Capsule,
    ];

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Cube => "cube",
            Self::Wall => "wall",
            Self::Cylinder => "cylinder",
            Self::Sphere => "sphere",
            Self::Cone => "cone",
            Self::Polygon => "polygon",
            Self::Ramp => "ramp",
            Self::Torus => "torus",
            Self::Capsule => "capsule",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            Self::Plane => "PLN",
            Self::Cube => "BOX",
            Self::Wall => "WAL",
            Self::Cylinder => "CYL",
            Self::Sphere => "SPH",
            Self::Cone => "CON",
            Self::Polygon => "PLY",
            Self::Ramp => "RMP",
            Self::Torus => "TOR",
            Self::Capsule => "CAP",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Plane => "Plane / Ground",
            Self::Cube => "Kubus / Box",
            Self::Wall => "Wall / Dinding",
            Self::Cylinder => "Silinder",
            Self::Sphere => "Bola / Sphere",
            Self::Cone => "Cone / Kerucut",
            Self::Polygon => "Polygon (Height)",
            Self::Ramp => "Ramp / Tanjakan",
            Self::Torus => "Torus / Ring 3D",
            Self::Capsule => "Kapsul / Capsule",
        }
    }

    pub fn subtitle(&self) -> &'static str {
        match self {
            Self::Plane => "Lantai, helipad, zona datar",
            Self::Cube => "Kontainer, blok, bangunan kotak",
            Self::Wall => "Tembok perimeter, partisi indoor",
            Self::Cylinder => "Pilar, tangki, kolom zona",
            Self::Sphere => "Kubah radar, radius sensor",
            Self::Cone => "Traffic cone, tenda, penanda",
            Self::Polygon => "Prisma segi-N / polygon custom + tinggi",
            Self::Ramp => "Tanjakan miring, atap, jembatan",
            Self::Torus => "Cincin 3D, ring target, perimeter",
            Self::Capsule => "Silinder ujung bulat, bollard",
        }
    }

    pub fn is_recommended_extra(&self) -> bool {
        matches!(self, Self::Ramp | Self::Torus | Self::Capsule)
    }

    pub fn model_path(&self) -> String {
        format!("{BASIC_MESH_PATH_PREFIX}{}", self.slug())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BasicMeshMaterialKind {
    #[default]
    SolidColor,
    ImageTexture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicMeshConfig {
    pub shape: BasicMeshShape,
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub radius: f32,
    pub tube_radius: f32,
    pub segments: u32,
    #[serde(default)]
    pub polygon_points: Vec<[f32; 2]>,

    pub material_kind: BasicMeshMaterialKind,
    pub color_rgba: [u8; 4],
    #[serde(default)]
    pub texture_asset_id: Option<Uuid>,
    #[serde(default)]
    pub texture_rel_path: Option<String>,
    #[serde(default = "default_white_rgba")]
    pub texture_tint: [u8; 4],
    #[serde(default = "default_one")]
    pub uv_scale_x: f32,
    #[serde(default = "default_one")]
    pub uv_scale_y: f32,
    #[serde(default = "default_true")]
    pub uv_lock: bool,

    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default = "default_metallic")]
    pub metallic: f32,
    #[serde(default)]
    pub unlit: bool,
    #[serde(default)]
    pub double_sided: bool,
}

fn default_white_rgba() -> [u8; 4] {
    [255, 255, 255, 255]
}
fn default_one() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_roughness() -> f32 {
    0.65
}
fn default_metallic() -> f32 {
    0.05
}

impl BasicMeshConfig {
    pub fn default_for(shape: BasicMeshShape) -> Self {
        let (width, height, depth, radius, tube_radius, segments, color_rgba, double_sided) =
            match shape {
                BasicMeshShape::Plane => (12.0, 0.05, 12.0, 6.0, 0.5, 24, [71, 85, 105, 235], true),
                BasicMeshShape::Cube => (4.0, 4.0, 4.0, 2.0, 0.5, 24, [56, 189, 248, 255], false),
                BasicMeshShape::Wall => (12.0, 3.2, 0.45, 2.0, 0.5, 24, [148, 163, 184, 255], false),
                BasicMeshShape::Cylinder => (4.0, 5.0, 4.0, 2.2, 0.5, 24, [52, 211, 153, 255], false),
                BasicMeshShape::Sphere => (4.0, 4.0, 4.0, 2.4, 0.5, 24, [96, 165, 250, 230], false),
                BasicMeshShape::Cone => (4.0, 5.0, 4.0, 2.2, 0.5, 24, [245, 158, 11, 255], false),
                BasicMeshShape::Polygon => (10.0, 6.0, 10.0, 5.5, 0.5, 6, [129, 140, 248, 255], false),
                BasicMeshShape::Ramp => (4.5, 2.5, 8.0, 2.5, 0.5, 24, [100, 116, 139, 255], false),
                BasicMeshShape::Torus => (8.0, 1.0, 8.0, 4.5, 0.45, 32, [34, 211, 238, 255], false),
                BasicMeshShape::Capsule => (2.4, 4.5, 2.4, 1.1, 0.5, 20, [244, 114, 182, 255], false),
            };
        Self {
            shape,
            width,
            height,
            depth,
            radius,
            tube_radius,
            segments,
            polygon_points: Vec::new(),
            material_kind: BasicMeshMaterialKind::SolidColor,
            color_rgba,
            texture_asset_id: None,
            texture_rel_path: None,
            texture_tint: [255, 255, 255, 255],
            uv_scale_x: 1.0,
            uv_scale_y: 1.0,
            uv_lock: true,
            roughness: 0.65,
            metallic: 0.05,
            unlit: false,
            double_sided,
        }
    }

    pub fn geometry_signature(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.shape.hash(&mut h);
        self.width.to_bits().hash(&mut h);
        self.height.to_bits().hash(&mut h);
        self.depth.to_bits().hash(&mut h);
        self.radius.to_bits().hash(&mut h);
        self.tube_radius.to_bits().hash(&mut h);
        self.segments.hash(&mut h);
        self.polygon_points.len().hash(&mut h);
        for p in &self.polygon_points {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
        h.finish()
    }

    pub fn material_signature(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.material_kind.hash(&mut h);
        self.color_rgba.hash(&mut h);
        self.texture_asset_id.hash(&mut h);
        self.texture_rel_path.hash(&mut h);
        self.texture_tint.hash(&mut h);
        self.uv_scale_x.to_bits().hash(&mut h);
        self.uv_scale_y.to_bits().hash(&mut h);
        self.roughness.to_bits().hash(&mut h);
        self.metallic.to_bits().hash(&mut h);
        self.unlit.hash(&mut h);
        self.double_sided.hash(&mut h);
        h.finish()
    }
}

pub fn is_basic_mesh_marker(marker: &Marker) -> bool {
    marker.category == BASIC_MESH_CATEGORY
        || marker.model_path.starts_with(BASIC_MESH_PATH_PREFIX)
        || marker.metadata.get("basic_mesh").is_some()
}

pub fn parse_basic_mesh_config(marker: &Marker) -> Option<BasicMeshConfig> {
    if let Some(val) = marker.metadata.get("basic_mesh")
        && let Ok(cfg) = serde_json::from_value::<BasicMeshConfig>(val.clone())
    {
        return Some(cfg);
    }
    if let Some(slug) = marker.model_path.strip_prefix(BASIC_MESH_PATH_PREFIX) {
        let shape = BasicMeshShape::ALL
            .iter()
            .copied()
            .find(|s| s.slug() == slug)
            .unwrap_or(BasicMeshShape::Cube);
        return Some(BasicMeshConfig::default_for(shape));
    }
    None
}

pub fn write_basic_mesh_config(marker: &mut Marker, cfg: &BasicMeshConfig) {
    marker.category = BASIC_MESH_CATEGORY.to_string();
    marker.model_path = cfg.shape.model_path();
    if let Ok(val) = serde_json::to_value(cfg) {
        if let Some(obj) = marker.metadata.as_object_mut() {
            obj.insert("basic_mesh".to_string(), val);
        } else {
            marker.metadata = serde_json::json!({ "basic_mesh": val });
        }
    }
}

#[derive(Resource, Default)]
pub struct BasicMeshTextureCache {
    pub handles: HashMap<String, Handle<Image>>,
}

impl BasicMeshTextureCache {
    pub fn get_or_load(
        &mut self,
        rel_path: &str,
        asset_server: &AssetServer,
        assets_root: &std::path::Path,
        available_paths: &std::collections::HashSet<String>,
    ) -> Option<Handle<Image>> {
        if rel_path.trim().is_empty() {
            return None;
        }
        if let Some(h) = self.handles.get(rel_path) {
            return Some(h.clone());
        }
        if !available_paths.contains(rel_path) && !assets_root.join(rel_path).exists() {
            return None;
        }
        let handle = asset_server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    address_mode_w: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
            })
            .load(rel_path.to_string());
        self.handles.insert(rel_path.to_string(), handle.clone());
        Some(handle)
    }
}

pub fn build_basic_mesh_material(
    cfg: &BasicMeshConfig,
    tex_cache: &mut BasicMeshTextureCache,
    asset_server: &AssetServer,
    assets_root: &std::path::Path,
    available_paths: &std::collections::HashSet<String>,
) -> StandardMaterial {
    let (base_color, base_color_texture, alpha_u8) = match cfg.material_kind {
        BasicMeshMaterialKind::SolidColor => {
            let [r, g, b, a] = cfg.color_rgba;
            (Color::srgba_u8(r, g, b, a), None, a)
        }
        BasicMeshMaterialKind::ImageTexture => {
            let [r, g, b, a] = cfg.texture_tint;
            let tex = cfg.texture_rel_path.as_deref().and_then(|rel| {
                tex_cache.get_or_load(rel, asset_server, assets_root, available_paths)
            });
            (Color::srgba_u8(r, g, b, a), tex, a)
        }
    };

    let alpha_mode = if alpha_u8 < 252 {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    };

    StandardMaterial {
        base_color,
        base_color_texture,
        uv_transform: Affine2::from_scale(Vec2::new(
            cfg.uv_scale_x.clamp(0.05, 50.0),
            cfg.uv_scale_y.clamp(0.05, 50.0),
        )),
        perceptual_roughness: cfg.roughness.clamp(0.04, 1.0),
        metallic: cfg.metallic.clamp(0.0, 1.0),
        unlit: cfg.unlit,
        double_sided: cfg.double_sided,
        cull_mode: if cfg.double_sided {
            None
        } else {
            Some(Face::Back)
        },
        alpha_mode,
        ..default()
    }
}

/// Builds a procedural 3D `Mesh` with `RenderAssetUsages::default()` so both GPU rendering
/// and CPU triangle raycast picking work identically. Pivot is anchored cleanly at bottom center (`y = 0`).
pub fn build_basic_mesh_geometry(cfg: &BasicMeshConfig) -> Mesh {
    match cfg.shape {
        BasicMeshShape::Plane => build_plane_mesh(cfg.width.max(0.2), cfg.depth.max(0.2)),
        BasicMeshShape::Cube | BasicMeshShape::Wall => build_box_mesh(
            cfg.width.max(0.1),
            cfg.height.max(0.1),
            cfg.depth.max(0.05),
        ),
        BasicMeshShape::Cylinder => build_cylinder_mesh(
            cfg.radius.max(0.1),
            cfg.height.max(0.1),
            cfg.segments.clamp(6, 64),
        ),
        BasicMeshShape::Sphere => {
            build_sphere_mesh(cfg.radius.max(0.1), cfg.segments.clamp(8, 48))
        }
        BasicMeshShape::Cone => build_cone_mesh(
            cfg.radius.max(0.1),
            cfg.height.max(0.1),
            cfg.segments.clamp(6, 64),
        ),
        BasicMeshShape::Polygon => build_polygon_prism_mesh(cfg),
        BasicMeshShape::Ramp => build_ramp_mesh(
            cfg.width.max(0.1),
            cfg.height.max(0.1),
            cfg.depth.max(0.1),
        ),
        BasicMeshShape::Torus => build_torus_mesh(
            cfg.radius.max(0.2),
            cfg.tube_radius.clamp(0.05, cfg.radius.max(0.2) * 0.85),
            cfg.segments.clamp(12, 64),
        ),
        BasicMeshShape::Capsule => build_capsule_mesh(
            cfg.radius.max(0.1),
            cfg.height.max(cfg.radius.max(0.1) * 2.0 + 0.1),
            cfg.segments.clamp(10, 48),
        ),
    }
}

fn assemble_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    n: [f32; 3],
    u_max: f32,
    v_max: f32,
) {
    let base = positions.len() as u32;
    positions.extend_from_slice(&[p0, p1, p2, p3]);
    normals.extend_from_slice(&[n, n, n, n]);
    uvs.extend_from_slice(&[[0.0, v_max], [u_max, v_max], [u_max, 0.0], [0.0, 0.0]]);
    let v0 = Vec3::from_array(p0);
    let v1 = Vec3::from_array(p1);
    let v2 = Vec3::from_array(p2);
    let nv = Vec3::from_array(n);
    if (v1 - v0).cross(v2 - v0).dot(nv) >= 0.0 {
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

fn build_plane_mesh(w: f32, d: f32) -> Mesh {
    let hw = w * 0.5;
    let hd = d * 0.5;
    let y = 0.03;
    let mut pos = Vec::with_capacity(4);
    let mut norm = Vec::with_capacity(4);
    let mut uv = Vec::with_capacity(4);
    let mut idx = Vec::with_capacity(6);
    push_quad(
        &mut pos,
        &mut norm,
        &mut uv,
        &mut idx,
        [-hw, y, hd],
        [hw, y, hd],
        [hw, y, -hd],
        [-hw, y, -hd],
        [0.0, 1.0, 0.0],
        1.0,
        1.0,
    );
    assemble_mesh(pos, norm, uv, idx)
}

fn build_box_mesh(w: f32, h: f32, d: f32) -> Mesh {
    let hw = w * 0.5;
    let hd = d * 0.5;
    let mut pos = Vec::with_capacity(24);
    let mut norm = Vec::with_capacity(24);
    let mut uv = Vec::with_capacity(24);
    let mut idx = Vec::with_capacity(36);

    // Front (+Z)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [-hw, 0.0, hd], [hw, 0.0, hd], [hw, h, hd], [-hw, h, hd], [0.0, 0.0, 1.0], 1.0, 1.0);
    // Back (-Z)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [hw, 0.0, -hd], [-hw, 0.0, -hd], [-hw, h, -hd], [hw, h, -hd], [0.0, 0.0, -1.0], 1.0, 1.0);
    // Right (+X)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [hw, 0.0, hd], [hw, 0.0, -hd], [hw, h, -hd], [hw, h, hd], [1.0, 0.0, 0.0], 1.0, 1.0);
    // Left (-X)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [-hw, 0.0, -hd], [-hw, 0.0, hd], [-hw, h, hd], [-hw, h, -hd], [-1.0, 0.0, 0.0], 1.0, 1.0);
    // Top (+Y)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [-hw, h, hd], [hw, h, hd], [hw, h, -hd], [-hw, h, -hd], [0.0, 1.0, 0.0], 1.0, 1.0);
    // Bottom (-Y)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [-hw, 0.0, -hd], [hw, 0.0, -hd], [hw, 0.0, hd], [-hw, 0.0, hd], [0.0, -1.0, 0.0], 1.0, 1.0);

    assemble_mesh(pos, norm, uv, idx)
}

fn build_cylinder_mesh(r: f32, h: f32, segs: u32) -> Mesh {
    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    // Side wall
    let side_base = pos.len() as u32;
    for i in 0..=segs {
        let u = i as f32 / segs as f32;
        let ang = u * TAU;
        let (s, c) = ang.sin_cos();
        pos.push([c * r, 0.0, s * r]);
        norm.push([c, 0.0, s]);
        uv.push([u, 1.0]);

        pos.push([c * r, h, s * r]);
        norm.push([c, 0.0, s]);
        uv.push([u, 0.0]);
    }
    for i in 0..segs {
        let a = side_base + i * 2;
        let b = a + 1;
        let c = a + 2;
        let d = a + 3;
        idx.extend_from_slice(&[a, b, c, b, d, c]);
    }

    // Top cap (+Y)
    let top_center = pos.len() as u32;
    pos.push([0.0, h, 0.0]);
    norm.push([0.0, 1.0, 0.0]);
    uv.push([0.5, 0.5]);
    for i in 0..=segs {
        let ang = (i as f32 / segs as f32) * TAU;
        let (s, c) = ang.sin_cos();
        pos.push([c * r, h, s * r]);
        norm.push([0.0, 1.0, 0.0]);
        uv.push([0.5 + c * 0.5, 0.5 + s * 0.5]);
    }
    for i in 0..segs {
        idx.extend_from_slice(&[top_center, top_center + 1 + i + 1, top_center + 1 + i]);
    }

    // Bottom cap (-Y)
    let bot_center = pos.len() as u32;
    pos.push([0.0, 0.0, 0.0]);
    norm.push([0.0, -1.0, 0.0]);
    uv.push([0.5, 0.5]);
    for i in 0..=segs {
        let ang = (i as f32 / segs as f32) * TAU;
        let (s, c) = ang.sin_cos();
        pos.push([c * r, 0.0, s * r]);
        norm.push([0.0, -1.0, 0.0]);
        uv.push([0.5 + c * 0.5, 0.5 + s * 0.5]);
    }
    for i in 0..segs {
        idx.extend_from_slice(&[bot_center, bot_center + 1 + i, bot_center + 1 + i + 1]);
    }

    assemble_mesh(pos, norm, uv, idx)
}

fn build_sphere_mesh(r: f32, segs: u32) -> Mesh {
    let stacks = (segs / 2).max(6);
    let slices = segs.max(8);
    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    for st in 0..=stacks {
        let v = st as f32 / stacks as f32;
        let phi = v * PI;
        let sp = phi.sin();
        let cp = phi.cos();
        for sl in 0..=slices {
            let u = sl as f32 / slices as f32;
            let theta = u * TAU;
            let nx = sp * theta.cos();
            let ny = cp;
            let nz = sp * theta.sin();
            pos.push([nx * r, r + ny * r, nz * r]);
            norm.push([nx, ny, nz]);
            uv.push([u, v]);
        }
    }
    let row = slices + 1;
    for st in 0..stacks {
        for sl in 0..slices {
            let a = st * row + sl;
            let b = a + row;
            let c = b + 1;
            let d = a + 1;
            idx.extend_from_slice(&[a, d, b, d, c, b]);
        }
    }
    assemble_mesh(pos, norm, uv, idx)
}

fn build_cone_mesh(r: f32, h: f32, segs: u32) -> Mesh {
    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    let slope_len = (r * r + h * h).sqrt().max(1e-4);
    let ny = r / slope_len;
    let nr = h / slope_len;

    let base_idx = pos.len() as u32;
    for i in 0..=segs {
        let u = i as f32 / segs as f32;
        let ang = u * TAU;
        let (s, c) = ang.sin_cos();
        pos.push([c * r, 0.0, s * r]);
        norm.push([c * nr, ny, s * nr]);
        uv.push([u, 1.0]);

        pos.push([0.0, h, 0.0]);
        norm.push([c * nr, ny, s * nr]);
        uv.push([u, 0.0]);
    }
    for i in 0..segs {
        let a = base_idx + i * 2;
        let tip = a + 1;
        let b = a + 2;
        idx.extend_from_slice(&[a, tip, b]);
    }

    // Bottom cap (-Y)
    let bot_center = pos.len() as u32;
    pos.push([0.0, 0.0, 0.0]);
    norm.push([0.0, -1.0, 0.0]);
    uv.push([0.5, 0.5]);
    for i in 0..=segs {
        let ang = (i as f32 / segs as f32) * TAU;
        let (s, c) = ang.sin_cos();
        pos.push([c * r, 0.0, s * r]);
        norm.push([0.0, -1.0, 0.0]);
        uv.push([0.5 + c * 0.5, 0.5 + s * 0.5]);
    }
    for i in 0..segs {
        idx.extend_from_slice(&[bot_center, bot_center + 1 + i, bot_center + 1 + i + 1]);
    }

    assemble_mesh(pos, norm, uv, idx)
}

fn build_polygon_prism_mesh(cfg: &BasicMeshConfig) -> Mesh {
    let ring: Vec<[f32; 2]> = if cfg.polygon_points.len() >= 3 {
        cfg.polygon_points.clone()
    } else {
        let sides = cfg.segments.clamp(3, 32);
        let r = cfg.radius.max(0.2);
        (0..sides)
            .map(|i| {
                let ang = (i as f32 / sides as f32) * TAU - FRAC_PI_2;
                [ang.cos() * r, ang.sin() * r]
            })
            .collect()
    };

    // Remove consecutive duplicate / near-zero vertices before triangulation
    let mut cleaned: Vec<[f32; 2]> = Vec::with_capacity(ring.len());
    for pt in ring {
        if cleaned
            .last()
            .is_none_or(|prev| (pt[0] - prev[0]).hypot(pt[1] - prev[1]) > 1e-3)
        {
            cleaned.push(pt);
        }
    }
    if cleaned.len() > 3 {
        let first = cleaned[0];
        let last = *cleaned.last().unwrap();
        if (first[0] - last[0]).hypot(first[1] - last[1]) < 1e-3 {
            cleaned.pop();
        }
    }
    let mut ring = cleaned;
    if ring.len() < 3 {
        return build_box_mesh(4.0, cfg.height.max(0.2), 4.0);
    }

    // Normalize winding so ring is CCW when viewed from +Y (signed_area_xz <= 0.0).
    // With CCW in XZ (looking from +Y), outward wall normal is `edge.cross(Vec3::Y)`.
    let signed_area_xz: f32 = ring
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let b = ring[(i + 1) % ring.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if signed_area_xz > 0.0 {
        ring.reverse();
    }

    let flat_2d: Vec<f64> = ring
        .iter()
        .flat_map(|p| [p[0] as f64, p[1] as f64])
        .collect();
    let Ok(tris) = earcutr::earcut(&flat_2d, &[], 2) else {
        return build_box_mesh(4.0, cfg.height.max(0.2), 4.0);
    };
    if tris.len() < 3 {
        return build_box_mesh(4.0, cfg.height.max(0.2), 4.0);
    }

    let h = cfg.height.max(0.1);
    let (mut min_x, mut max_x, mut min_z, mut max_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in &ring {
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
        min_z = min_z.min(p[1]);
        max_z = max_z.max(p[1]);
    }
    let span_x = (max_x - min_x).max(0.1);
    let span_z = (max_z - min_z).max(0.1);

    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    // 1. Bottom cap (-Y)
    let bot_base = pos.len() as u32;
    for p in &ring {
        pos.push([p[0], 0.0, p[1]]);
        norm.push([0.0, -1.0, 0.0]);
        uv.push([(p[0] - min_x) / span_x, (p[1] - min_z) / span_z]);
    }
    for tri in tris.as_chunks::<3>().0 {
        let p0 = Vec3::from_array(pos[bot_base as usize + tri[0]]);
        let p1 = Vec3::from_array(pos[bot_base as usize + tri[1]]);
        let p2 = Vec3::from_array(pos[bot_base as usize + tri[2]]);
        if (p1 - p0).cross(p2 - p0).y <= 0.0 {
            idx.extend_from_slice(&[
                bot_base + tri[0] as u32,
                bot_base + tri[1] as u32,
                bot_base + tri[2] as u32,
            ]);
        } else {
            idx.extend_from_slice(&[
                bot_base + tri[0] as u32,
                bot_base + tri[2] as u32,
                bot_base + tri[1] as u32,
            ]);
        }
    }

    // 2. Side walls with outward normals (`edge x +Y`) and perimeter-proportional UVs
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let edge = Vec3::new(b[0] - a[0], 0.0, b[1] - a[1]);
        let seg_len = edge.length().max(0.05);
        let n = edge.cross(Vec3::Y).normalize_or_zero();
        push_quad(
            &mut pos,
            &mut norm,
            &mut uv,
            &mut idx,
            [a[0], 0.0, a[1]],
            [b[0], 0.0, b[1]],
            [b[0], h, b[1]],
            [a[0], h, a[1]],
            [n.x, n.y, n.z],
            (seg_len / h.max(1.0)).max(0.25),
            1.0,
        );
    }

    // 3. Roof cap (+Y) drawn last so even translucent polygons render the top cap cleanly
    let roof_base = pos.len() as u32;
    for p in &ring {
        pos.push([p[0], h, p[1]]);
        norm.push([0.0, 1.0, 0.0]);
        uv.push([(p[0] - min_x) / span_x, (p[1] - min_z) / span_z]);
    }
    for tri in tris.as_chunks::<3>().0 {
        let p0 = Vec3::from_array(pos[roof_base as usize + tri[0]]);
        let p1 = Vec3::from_array(pos[roof_base as usize + tri[1]]);
        let p2 = Vec3::from_array(pos[roof_base as usize + tri[2]]);
        if (p1 - p0).cross(p2 - p0).y >= 0.0 {
            idx.extend_from_slice(&[
                roof_base + tri[0] as u32,
                roof_base + tri[1] as u32,
                roof_base + tri[2] as u32,
            ]);
        } else {
            idx.extend_from_slice(&[
                roof_base + tri[0] as u32,
                roof_base + tri[2] as u32,
                roof_base + tri[1] as u32,
            ]);
        }
    }

    assemble_mesh(pos, norm, uv, idx)
}

fn build_ramp_mesh(w: f32, h: f32, d: f32) -> Mesh {
    let hw = w * 0.5;
    let hd = d * 0.5;
    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    // Bottom face (-Y)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [-hw, 0.0, -hd], [hw, 0.0, -hd], [hw, 0.0, hd], [-hw, 0.0, hd], [0.0, -1.0, 0.0], 1.0, 1.0);
    // High back vertical face (+Z)
    push_quad(&mut pos, &mut norm, &mut uv, &mut idx, [-hw, 0.0, hd], [hw, 0.0, hd], [hw, h, hd], [-hw, h, hd], [0.0, 0.0, 1.0], 1.0, 1.0);
    // Sloped ramp top face (from z=-hd,y=0 up to z=+hd,y=h)
    let slope_n = Vec3::new(0.0, d, -h).normalize();
    push_quad(
        &mut pos,
        &mut norm,
        &mut uv,
        &mut idx,
        [-hw, 0.0, -hd],
        [hw, 0.0, -hd],
        [hw, h, hd],
        [-hw, h, hd],
        [slope_n.x, slope_n.y, slope_n.z],
        1.0,
        1.0,
    );

    // Right triangle (+X)
    let b_r = pos.len() as u32;
    pos.extend_from_slice(&[[hw, 0.0, -hd], [hw, 0.0, hd], [hw, h, hd]]);
    norm.extend_from_slice(&[[1.0, 0.0, 0.0]; 3]);
    uv.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
    idx.extend_from_slice(&[b_r, b_r + 2, b_r + 1]);

    // Left triangle (-X)
    let b_l = pos.len() as u32;
    pos.extend_from_slice(&[[-hw, 0.0, -hd], [-hw, 0.0, hd], [-hw, h, hd]]);
    norm.extend_from_slice(&[[-1.0, 0.0, 0.0]; 3]);
    uv.extend_from_slice(&[[1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]);
    idx.extend_from_slice(&[b_l, b_l + 1, b_l + 2]);

    assemble_mesh(pos, norm, uv, idx)
}

fn build_torus_mesh(major_r: f32, tube_r: f32, segs: u32) -> Mesh {
    let major_segs = segs.max(12);
    let minor_segs = (segs / 2).clamp(8, 32);
    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    for i in 0..=major_segs {
        let u = i as f32 / major_segs as f32;
        let theta = u * TAU;
        let (st, ct) = theta.sin_cos();
        for j in 0..=minor_segs {
            let v = j as f32 / minor_segs as f32;
            let phi = v * TAU;
            let (sp, cp) = phi.sin_cos();
            let ring_r = major_r + tube_r * cp;
            let x = ring_r * ct;
            let y = tube_r + tube_r * sp;
            let z = ring_r * st;
            let nx = cp * ct;
            let ny = sp;
            let nz = cp * st;
            pos.push([x, y, z]);
            norm.push([nx, ny, nz]);
            uv.push([u * 4.0, v]);
        }
    }

    let stride = minor_segs + 1;
    for i in 0..major_segs {
        for j in 0..minor_segs {
            let a = i * stride + j;
            let b = (i + 1) * stride + j;
            let c = b + 1;
            let d = a + 1;
            idx.extend_from_slice(&[a, d, b, b, d, c]);
        }
    }

    assemble_mesh(pos, norm, uv, idx)
}

fn build_capsule_mesh(r: f32, total_h: f32, segs: u32) -> Mesh {
    let cyl_h = (total_h - 2.0 * r).max(0.05);
    let hemi_rings = (segs / 4).clamp(4, 16);
    let slices = segs.max(10);

    // Build profile rings from top pole down to bottom pole
    let mut profile: Vec<(f32, f32, f32, f32, f32)> = Vec::new(); // (rad, y, nr, ny, v)
    let total_rings = hemi_rings * 2 + 1;
    let mut ring_idx = 0usize;

    // Top hemisphere: phi from 0 (north pole) to pi/2 (equator)
    for k in 0..=hemi_rings {
        let t = k as f32 / hemi_rings as f32;
        let phi = t * FRAC_PI_2;
        let nr = phi.sin();
        let ny = phi.cos();
        let y = r + cyl_h + ny * r;
        let v = ring_idx as f32 / total_rings as f32;
        profile.push((nr * r, y, nr, ny, v));
        ring_idx += 1;
    }
    // Bottom hemisphere: phi from pi/2 (equator) to pi (south pole)
    for k in 0..=hemi_rings {
        let t = k as f32 / hemi_rings as f32;
        let phi = FRAC_PI_2 + t * FRAC_PI_2;
        let nr = phi.sin();
        let ny = phi.cos();
        let y = r + ny * r;
        let v = ring_idx as f32 / total_rings as f32;
        profile.push((nr * r, y, nr, ny, v));
        ring_idx += 1;
    }

    let mut pos = Vec::new();
    let mut norm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    for &(rad, y, nr, ny, v) in &profile {
        for sl in 0..=slices {
            let u = sl as f32 / slices as f32;
            let theta = u * TAU;
            let (st, ct) = theta.sin_cos();
            pos.push([rad * ct, y, rad * st]);
            norm.push([nr * ct, ny, nr * st]);
            uv.push([u, v]);
        }
    }

    let row = slices + 1;
    for r_i in 0..(profile.len() - 1) {
        for sl in 0..slices {
            let a = (r_i as u32) * row + sl;
            let b = a + row;
            let c = b + 1;
            let d = a + 1;
            idx.extend_from_slice(&[a, d, b, d, c, b]);
        }
    }

    assemble_mesh(pos, norm, uv, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_basic_mesh_shapes_generate_valid_indexed_triangles() {
        for &shape in BasicMeshShape::ALL {
            let cfg = BasicMeshConfig::default_for(shape);
            let mesh = build_basic_mesh_geometry(&cfg);
            assert_eq!(mesh.primitive_topology(), PrimitiveTopology::TriangleList);
            let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
            let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap();
            assert_eq!(positions.len(), uvs.len(), "shape {shape:?} pos/uv mismatch");
            assert!(positions.len() >= 4, "shape {shape:?} too few vertices");
            let indices = mesh.indices().unwrap();
            assert!(indices.len() >= 6 && indices.len().is_multiple_of(3));
        }
    }

    #[test]
    fn all_basic_mesh_shapes_have_outward_normals_and_closed_volume() {
        use bevy::mesh::VertexAttributeValues;

        for &shape in BasicMeshShape::ALL {
            let cfg = BasicMeshConfig::default_for(shape);
            let mesh = build_basic_mesh_geometry(&cfg);
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions for {shape:?}");
            };
            let Some(VertexAttributeValues::Float32x3(norms)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("missing normals for {shape:?}");
            };
            let Some(Indices::U32(idx)) = mesh.indices() else {
                panic!("missing u32 indices for {shape:?}");
            };

            let mut signed_vol_6 = 0.0_f32;
            for tri in idx.as_chunks::<3>().0 {
                let v0 = Vec3::from_array(pos[tri[0] as usize]);
                let v1 = Vec3::from_array(pos[tri[1] as usize]);
                let v2 = Vec3::from_array(pos[tri[2] as usize]);
                let geom_n = (v1 - v0).cross(v2 - v0);
                if geom_n.length_squared() > 1e-8 {
                    let avg_n = Vec3::from_array(norms[tri[0] as usize])
                        + Vec3::from_array(norms[tri[1] as usize])
                        + Vec3::from_array(norms[tri[2] as usize]);
                    assert!(
                        geom_n.dot(avg_n) > 0.0,
                        "shape {shape:?} has inverted triangle winding!"
                    );
                }
                signed_vol_6 += v0.dot(v1.cross(v2));
            }
            if shape != BasicMeshShape::Plane {
                assert!(
                    signed_vol_6 > 0.0,
                    "closed shape {shape:?} must have positive outward volume (got {signed_vol_6})"
                );
            }
        }
    }

    #[test]
    fn custom_concave_polygon_prism_is_watertight_and_capped_on_top() {
        use bevy::mesh::VertexAttributeValues;

        // L-shaped concave polygon (both CW and CCW input orders must work identically!)
        let l_shape = vec![
            [-6.0, -6.0],
            [6.0, -6.0],
            [6.0, -1.0],
            [0.0, -1.0],
            [0.0, 6.0],
            [-6.0, 6.0],
        ];
        for reversed in [false, true] {
            let mut cfg = BasicMeshConfig::default_for(BasicMeshShape::Polygon);
            cfg.polygon_points = l_shape.clone();
            if reversed {
                cfg.polygon_points.reverse();
            }
            cfg.height = 10.0;
            let mesh = build_basic_mesh_geometry(&cfg);
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!();
            };
            let Some(VertexAttributeValues::Float32x3(norms)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!();
            };
            let Some(Indices::U32(idx)) = mesh.indices() else {
                panic!();
            };

            let mut roof_area = 0.0_f32;
            let mut signed_vol_6 = 0.0_f32;
            for tri in idx.as_chunks::<3>().0 {
                let v0 = Vec3::from_array(pos[tri[0] as usize]);
                let v1 = Vec3::from_array(pos[tri[1] as usize]);
                let v2 = Vec3::from_array(pos[tri[2] as usize]);
                let geom_n = (v1 - v0).cross(v2 - v0);
                let avg_n = Vec3::from_array(norms[tri[0] as usize])
                    + Vec3::from_array(norms[tri[1] as usize])
                    + Vec3::from_array(norms[tri[2] as usize]);
                assert!(geom_n.dot(avg_n) > 0.0, "inverted triangle (reversed={reversed})");
                if (v0.y - 10.0).abs() < 1e-3
                    && (v1.y - 10.0).abs() < 1e-3
                    && (v2.y - 10.0).abs() < 1e-3
                {
                    assert!(geom_n.y > 0.0, "roof cap must face +Y!");
                    roof_area += geom_n.y * 0.5;
                }
                signed_vol_6 += v0.dot(v1.cross(v2));
            }
            // L-shape area: 12*5 (bottom bar) + 6*7 (left arm) = 60 + 42 = 102 m^2, volume = 1020 m^3
            assert!((roof_area - 102.0).abs() < 0.1, "roof area mismatch: {roof_area}");
            let volume = signed_vol_6 / 6.0;
            assert!((volume - 1020.0).abs() < 1.0, "volume mismatch: {volume}");
        }
    }
}
