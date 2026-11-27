use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    AdminMonitor,
    AdminSatker,
    UserSatker,
    #[default]
    User,
    LayarBesar,
    LayarBesarSatker,
}

impl UserRole {
    pub const ALL: &'static [Self] = &[
        Self::Admin,
        Self::AdminMonitor,
        Self::AdminSatker,
        Self::User,
        Self::UserSatker,
        Self::LayarBesar,
        Self::LayarBesarSatker,
    ];

    pub const SATKER_ROLES: &'static [Self] = &[
        Self::AdminSatker,
        Self::UserSatker,
        Self::LayarBesarSatker,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::AdminMonitor => "admin_monitor",
            Self::AdminSatker => "admin_satker",
            Self::UserSatker => "user_satker",
            Self::User => "user",
            Self::LayarBesar => "layar_besar",
            Self::LayarBesarSatker => "layar_besar_satker",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Admin => "Admin",
            Self::AdminMonitor => "Admin Monitor",
            Self::AdminSatker => "Admin Satker",
            Self::User => "User",
            Self::UserSatker => "User Satker",
            Self::LayarBesar => "Layar Besar",
            Self::LayarBesarSatker => "Layar Besar Satker",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "admin" => Self::Admin,
            "admin_monitor" => Self::AdminMonitor,
            "admin_satker" => Self::AdminSatker,
            "user_satker" => Self::UserSatker,
            "layar_besar" | "layer_besar" => Self::LayarBesar,
            "layar_besar_satker" | "layer_besar_satker" => Self::LayarBesarSatker,
            _ => Self::User,
        }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }

    pub fn is_admin_monitor(&self) -> bool {
        matches!(self, Self::AdminMonitor)
    }

    pub fn is_admin_satker(&self) -> bool {
        matches!(self, Self::AdminSatker)
    }

    pub fn is_user_satker(&self) -> bool {
        matches!(self, Self::UserSatker)
    }

    /// Dedicated read-only Layar Besar display accounts (`layar_besar` & `layar_besar_satker`).
    pub fn is_big_screen_only_role(&self) -> bool {
        matches!(self, Self::LayarBesar | Self::LayarBesarSatker)
    }

    /// Global administrators (`admin` and `admin_monitor`): full CRUD on satkers, users,
    /// assets/categories, operasis, skenarios, and plot editor.
    pub fn is_global_admin(&self) -> bool {
        matches!(self, Self::Admin | Self::AdminMonitor)
    }

    pub fn is_admin_like(&self) -> bool {
        matches!(self, Self::Admin | Self::AdminMonitor | Self::AdminSatker)
    }

    pub fn is_satker_role(&self) -> bool {
        matches!(
            self,
            Self::AdminSatker | Self::UserSatker | Self::LayarBesarSatker
        )
    }

    /// Roles allowed to create/edit/delete Operasis, Skenarios, and Users (scoped by satker
    /// for `admin_satker`).
    pub fn can_write_admin(&self) -> bool {
        matches!(self, Self::Admin | Self::AdminMonitor | Self::AdminSatker)
    }

    pub fn can_manage_satkers(&self) -> bool {
        self.is_global_admin()
    }

    pub fn can_manage_assets(&self) -> bool {
        self.is_global_admin()
    }

    pub fn can_manage_users(&self) -> bool {
        self.can_write_admin()
    }

    /// Whether this role can drive Layar Besar camera (pan/zoom/pitch/bearing) and load/reload
    /// scenarios on Layar Besar from Tactical Play Skenario.
    /// - `admin`: all scenarios
    /// - `admin_satker`: only scenarios belonging to their own satker
    /// - all other roles: never
    pub fn can_control_big_screen(
        &self,
        skenario_satker_id: Option<Uuid>,
        user_satker_id: Option<Uuid>,
    ) -> bool {
        match self {
            Self::Admin => true,
            Self::AdminSatker => user_satker_id.is_some() && skenario_satker_id == user_satker_id,
            Self::AdminMonitor
            | Self::User
            | Self::UserSatker
            | Self::LayarBesar
            | Self::LayarBesarSatker => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MapMode {
    #[default]
    Outdoor,
    Indoor,
}

impl MapMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Outdoor => "outdoor",
            Self::Indoor => "indoor",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "indoor" => Self::Indoor,
            _ => Self::Outdoor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BaseLayer {
    #[default]
    Osm,
    Satellite,
    GridDark,
    GridLight,
    BlankDark,
    BlankLight,
    FloorPlan,
}

impl BaseLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Osm => "osm",
            Self::Satellite => "satellite",
            Self::GridDark => "grid_dark",
            Self::GridLight => "grid_light",
            Self::BlankDark => "blank_dark",
            Self::BlankLight => "blank_light",
            Self::FloorPlan => "floor_plan",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "satellite" => Self::Satellite,
            "grid_dark" => Self::GridDark,
            "grid_light" => Self::GridLight,
            "blank_dark" => Self::BlankDark,
            "blank_light" => Self::BlankLight,
            "floor_plan" => Self::FloorPlan,
            _ => Self::Osm,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Osm => "OSM Street",
            Self::Satellite => "ArcGIS Satellite",
            Self::GridDark => "Indoor Grid Dark",
            Self::GridLight => "Indoor Grid Light",
            Self::BlankDark => "Indoor Blank Dark",
            Self::BlankLight => "Indoor Blank Light",
            Self::FloorPlan => "Indoor Floor Plan",
        }
    }

    pub fn is_outdoor_tile(&self) -> bool {
        matches!(self, Self::Osm | Self::Satellite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkyboxPreset {
    None,
    #[default]
    DayClear,
    DayCloudy,
    DaySunset,
    NightStarry,
    NightTactical,
}

impl SkyboxPreset {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::DayClear,
        Self::DayCloudy,
        Self::DaySunset,
        Self::NightStarry,
        Self::NightTactical,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::DayClear => "day_clear",
            Self::DayCloudy => "day_cloudy",
            Self::DaySunset => "day_sunset",
            Self::NightStarry => "night_starry",
            Self::NightTactical => "night_tactical",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "none" => Self::None,
            "day_cloudy" => Self::DayCloudy,
            "day_sunset" => Self::DaySunset,
            "night_starry" => Self::NightStarry,
            "night_tactical" => Self::NightTactical,
            _ => Self::DayClear,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Tidak Ada Skybox",
            Self::DayClear => "Siang - Cerah",
            Self::DayCloudy => "Siang - Berawan",
            Self::DaySunset => "Siang - Senja / Sore",
            Self::NightStarry => "Malam - Berbintang",
            Self::NightTactical => "Malam - Taktis Gelap",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::None => "Tanpa Skybox",
            Self::DayClear => "Siang Cerah",
            Self::DayCloudy => "Siang Berawan",
            Self::DaySunset => "Siang Senja",
            Self::NightStarry => "Malam Bintang",
            Self::NightTactical => "Malam Taktis",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndoorConfig {
    #[serde(default = "default_grid_size")]
    pub grid_size_m: f64,
    #[serde(default)]
    pub floor: i32,
    #[serde(default)]
    pub floor_plan_url: Option<String>,
    #[serde(default)]
    pub floor_plan_width_m: Option<f64>,
    #[serde(default)]
    pub floor_plan_height_m: Option<f64>,
    #[serde(default)]
    pub floor_plan_rotation_deg: Option<f64>,
    #[serde(default)]
    pub floor_plan_opacity: Option<f32>,
}

fn default_grid_size() -> f64 {
    10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationPreset {
    pub id: Uuid,
    pub name: String,
    pub center_lng: f64,
    pub center_lat: f64,
    pub zoom: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub pitch: f64,
    pub bearing: f64,
    pub map_mode: MapMode,
    pub base_layer: BaseLayer,
    pub grid_size_m: f64,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Satker {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDto {
    pub id: Uuid,
    pub name: String,
    pub username: String,
    pub role: UserRole,
    pub satker_id: Option<Uuid>,
    pub satker_name: Option<String>,
    pub profile_img: Option<String>,
    pub units: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operasi {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub satker_id: Option<Uuid>,
    pub satker_name: Option<String>,
    #[serde(default)]
    pub skenario_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Paginated card item for the Tactical / Simulasi screen: one Operasi + all its Skenarios.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalOperasiCard {
    pub operasi: Operasi,
    pub skenarios: Vec<Skenario>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skenario {
    pub id: Uuid,
    pub operasi_id: Uuid,
    #[serde(default)]
    pub operasi_name: Option<String>,
    pub satker_id: Option<Uuid>,
    pub name: String,
    pub center_lng: f64,
    pub center_lat: f64,
    pub zoom: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub pitch: f64,
    pub bearing: f64,
    pub map_mode: MapMode,
    pub base_layer: BaseLayer,
    #[serde(default)]
    pub skybox: SkyboxPreset,
    pub indoor_config: IndoorConfig,
    #[serde(default)]
    pub marker_count: i64,
    #[serde(default)]
    pub building_count: i64,
    #[serde(default)]
    pub line_count: i64,
    #[serde(default)]
    pub text_count: i64,
    #[serde(default)]
    pub particle_count: i64,
    #[serde(default)]
    pub alur_count: i64,
    #[serde(default)]
    pub anim_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Skenario {
    pub fn total_content_count(&self) -> i64 {
        self.marker_count
            + self.building_count
            + self.line_count
            + self.text_count
            + self.particle_count
            + self.alur_count
            + self.anim_count
    }

    pub fn has_simulation_content(&self) -> bool {
        self.total_content_count() > 0
    }

    pub fn content_breakdown(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.marker_count > 0 {
            parts.push(format!("{} marker/objek 3D", self.marker_count));
        }
        if self.building_count > 0 {
            parts.push(format!("{} polygon/bangunan", self.building_count));
        }
        if self.line_count > 0 {
            parts.push(format!("{} garis", self.line_count));
        }
        if self.text_count > 0 {
            parts.push(format!("{} teks 3D", self.text_count));
        }
        if self.particle_count > 0 {
            parts.push(format!("{} partikel", self.particle_count));
        }
        if self.alur_count > 0 {
            parts.push(format!("{} alur", self.alur_count));
        }
        if self.anim_count > 0 {
            parts.push(format!("{} animasi", self.anim_count));
        }
        parts
    }

    pub fn content_summary(&self) -> String {
        let parts = self.content_breakdown();
        if parts.is_empty() {
            "Kosong".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub unit_id: String,
    pub name: String,
    pub category: String,
    pub model_path: String,
    pub quantity: i32,
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
    pub rot_x: f64,
    pub rot_y: f64,
    pub rot_z: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub scale_z: f64,
    pub is_spawn: bool,
    pub description: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub height: f64,
    /// Outer polygon ring in [lng, lat] pairs
    pub geom: Vec<[f64; 2]>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotLine {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub name: String,
    pub color: String,
    pub width: f64,
    pub alpha: f64,
    /// Polyline vertices in [lng, lat] pairs
    pub geom: Vec<[f64; 2]>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextMarker {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub text: String,
    pub color: String,
    pub font_size: f64,
    #[serde(default = "default_text_show_background")]
    pub show_background: bool,
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_text_show_background() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Particle {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub name: String,
    pub particle_type: String,
    pub lng: f64,
    pub lat: f64,
    pub is_spawn: bool,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alur {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub title: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anim {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub marker_id: Option<Uuid>,
    pub name: String,
    pub anim_type: String,
    pub sort_order: i32,
    pub speed: f64,
    pub duration_sec: f64,
    pub replay: bool,
    pub autoplay: bool,
    /// Optional route coordinates [lng, lat, alt]
    pub geom: Vec<[f64; 3]>,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSessionSnapshot {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub snapshot_enabled: bool,
    pub map_state: serde_json::Value,
    pub markers_state: serde_json::Value,
    pub anims_state: serde_json::Value,
    pub version: i32,
    pub updated_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub marker_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub command: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkenarioBundle {
    pub skenario: Skenario,
    pub markers: Vec<Marker>,
    pub buildings: Vec<Building>,
    pub lines: Vec<PlotLine>,
    pub text_markers: Vec<TextMarker>,
    pub particles: Vec<Particle>,
    pub alurs: Vec<Alur>,
    pub anims: Vec<Anim>,
    pub snapshot: Option<GameSessionSnapshot>,
}

pub const MODEL_CATEGORIES: &[&str] = &[
    "bangunans",
    "kendaraan_umums",
    "kendaraans",
    "peoples",
    "stackholders",
    "tools",
    "units",
];

pub fn is_valid_model_category(cat: &str) -> bool {
    MODEL_CATEGORIES.contains(&cat)
}

pub fn category_supports_idle_walk(cat: &str) -> bool {
    matches!(cat, "peoples" | "stackholders" | "units")
}

pub fn model_category_label(cat: &str) -> &'static str {
    match cat {
        "bangunans" => "Bangunans",
        "kendaraan_umums" => "Kendaraan Umums",
        "kendaraans" => "Kendaraans",
        "peoples" => "Peoples",
        "stackholders" => "Stackholders",
        "tools" => "Tools",
        "units" => "Units",
        _ => "Other",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelAssetType {
    #[default]
    Static,
    Animated,
}

impl ModelAssetType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Animated => "animated",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "animated" => Self::Animated,
            _ => Self::Static,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Static => "Statis",
            Self::Animated => "Animasi (Idle + Walk)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelFileMeta {
    pub rel_path: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub tris: i32,
    pub download_url: String,
}

/// An extra named action animation clip uploaded for an `Animated` 3D model
/// (e.g., `Melambai`, `Menembak`, `Pingsan`, `Hormat`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelActionClipDto {
    pub slug: String,
    pub name: String,
    pub file: ModelFileMeta,
    #[serde(default)]
    pub lod2_file: Option<ModelFileMeta>,
    /// Optional GLB animation clip names detected in the JSON header.
    #[serde(default)]
    pub clip_names: Vec<String>,
}

/// Lightweight catalog entry for an extra action clip synced to clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCatalogActionClip {
    pub slug: String,
    pub name: String,
    pub model_path: String,
    #[serde(default)]
    pub lod2_path: Option<String>,
    #[serde(default)]
    pub clip_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAssetDto {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub category: String,
    pub model_type: ModelAssetType,
    pub model_file: ModelFileMeta,
    #[serde(default)]
    pub lod2_file: Option<ModelFileMeta>,
    #[serde(default)]
    pub walk_file: Option<ModelFileMeta>,
    #[serde(default)]
    pub walk_lod2_file: Option<ModelFileMeta>,
    #[serde(default)]
    pub extra_animations: Vec<ModelActionClipDto>,
    pub default_height: f64,
    pub default_rotation_deg: [f64; 3],
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ModelAssetDto {
    pub fn all_files(&self) -> Vec<&ModelFileMeta> {
        let mut out = Vec::with_capacity(4 + self.extra_animations.len() * 2);
        out.push(&self.model_file);
        if let Some(f) = &self.lod2_file {
            out.push(f);
        }
        if let Some(f) = &self.walk_file {
            out.push(f);
        }
        if let Some(f) = &self.walk_lod2_file {
            out.push(f);
        }
        for act in &self.extra_animations {
            out.push(&act.file);
            if let Some(f) = &act.lod2_file {
                out.push(f);
            }
        }
        out
    }

    pub fn to_catalog_entry(&self, ready: bool) -> ModelCatalogEntry {
        ModelCatalogEntry {
            id: self.id.to_string(),
            slug: self.slug.clone(),
            name: self.name.clone(),
            category: self.category.clone(),
            model_type: self.model_type,
            model_path: self.model_file.rel_path.clone(),
            lod2_path: self.lod2_file.as_ref().map(|f| f.rel_path.clone()),
            walk_model_path: self.walk_file.as_ref().map(|f| f.rel_path.clone()),
            walk_lod2_path: self.walk_lod2_file.as_ref().map(|f| f.rel_path.clone()),
            extra_animations: self
                .extra_animations
                .iter()
                .map(|a| ModelCatalogActionClip {
                    slug: a.slug.clone(),
                    name: a.name.clone(),
                    model_path: a.file.rel_path.clone(),
                    lod2_path: a.lod2_file.as_ref().map(|f| f.rel_path.clone()),
                    clip_names: a.clip_names.clone(),
                })
                .collect(),
            default_height: self.default_height,
            default_rotation_deg: self.default_rotation_deg,
            ready,
        }
    }
}

/// Interaction & movement behavior of a 3D model category during `Play Skenario`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CategoryPlayBehavior {
    /// Buildings / static props: non-selectable in Play Skenario and act as pathfinding obstacles.
    StaticObstacle,
    /// Tactical equipment / tools: selectable in Play Skenario and moved via 3D Move Gizmo (X/Y/Z + Free XZ).
    GizmoMove,
    /// Personnel, animals, vehicles: single-select or drag-box multi-select, moved via right-click
    /// in formation with obstacle avoidance.
    #[default]
    RtsUnit,
}

impl CategoryPlayBehavior {
    pub const ALL: &'static [Self] = &[Self::RtsUnit, Self::GizmoMove, Self::StaticObstacle];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaticObstacle => "static_obstacle",
            Self::GizmoMove => "gizmo_move",
            Self::RtsUnit => "rts_unit",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "static_obstacle" => Self::StaticObstacle,
            "gizmo_move" => Self::GizmoMove,
            _ => Self::RtsUnit,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::StaticObstacle => "Bangunan / Statis (Tidak bisa dipilih, Obstacle)",
            Self::GizmoMove => "Peralatan / Gizmo (Geser pakai Gizmo Move 3D)",
            Self::RtsUnit => "Unit / Orang / Kendaraan (Klik Kanan + Formasi)",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::StaticObstacle => "Statis / Obstacle",
            Self::GizmoMove => "Gizmo Move 3D",
            Self::RtsUnit => "Gerak RTS",
        }
    }

    pub fn default_for_slug(slug: &str) -> Self {
        match slug.trim().to_lowercase().as_str() {
            "bangunans" | "bangunan" => Self::StaticObstacle,
            "tools" | "alat" | "peralatan" => Self::GizmoMove,
            _ => Self::RtsUnit,
        }
    }
}

pub fn is_system_category_slug(slug: &str) -> bool {
    MODEL_CATEGORIES.contains(&slug)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCategoryDto {
    pub slug: String,
    pub name: String,
    pub supports_idle_walk: bool,
    pub default_scale: f64,
    pub sort_order: i32,
    #[serde(default)]
    pub play_behavior: CategoryPlayBehavior,
    #[serde(default)]
    pub is_system: bool,
    #[serde(default)]
    pub total_models: usize,
    #[serde(default)]
    pub static_count: usize,
    #[serde(default)]
    pub animated_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_category_scale() -> f64 {
    0.1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCategoryCount {
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub supports_idle_walk: bool,
    #[serde(default = "default_category_scale")]
    pub default_scale: f64,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub play_behavior: CategoryPlayBehavior,
    #[serde(default)]
    pub is_system: bool,
    pub total: usize,
    pub static_count: usize,
    pub animated_count: usize,
}

impl ModelCategoryCount {
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.category
        } else {
            &self.name
        }
    }

    pub fn from_dto(dto: &ModelCategoryDto) -> Self {
        Self {
            category: dto.slug.clone(),
            name: dto.name.clone(),
            supports_idle_walk: dto.supports_idle_walk,
            default_scale: dto.default_scale,
            sort_order: dto.sort_order,
            play_behavior: dto.play_behavior,
            is_system: dto.is_system || is_system_category_slug(&dto.slug),
            total: dto.total_models,
            static_count: dto.static_count,
            animated_count: dto.animated_count,
        }
    }
}

pub fn default_model_category_counts() -> Vec<ModelCategoryCount> {
    MODEL_CATEGORIES
        .iter()
        .enumerate()
        .map(|(idx, &slug)| ModelCategoryCount {
            category: slug.to_string(),
            name: model_category_label(slug).to_string(),
            supports_idle_walk: category_supports_idle_walk(slug),
            default_scale: 0.1,
            sort_order: ((idx + 1) * 10) as i32,
            play_behavior: CategoryPlayBehavior::default_for_slug(slug),
            is_system: true,
            total: 0,
            static_count: 0,
            animated_count: 0,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaAssetType {
    #[default]
    Texture,
    Sound,
    Video,
}

impl MediaAssetType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Texture => "texture",
            Self::Sound => "sound",
            Self::Video => "video",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "sound" => Self::Sound,
            "video" => Self::Video,
            _ => Self::Texture,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Texture => "Image Texture",
            Self::Sound => "Sound",
            Self::Video => "Video",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAssetDto {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub media_type: MediaAssetType,
    pub category: String,
    pub mime_type: String,
    /// Original uploaded file (for textures: symmetric SxS original, NEVER deleted on resize)
    pub original_file: ModelFileMeta,
    /// Active file in use (for textures: equals original or downscaled symmetric TxT <= original)
    pub active_file: ModelFileMeta,
    #[serde(default)]
    pub thumbnail_file: Option<ModelFileMeta>,
    #[serde(default)]
    pub original_width: Option<i32>,
    #[serde(default)]
    pub original_height: Option<i32>,
    #[serde(default)]
    pub active_width: Option<i32>,
    #[serde(default)]
    pub active_height: Option<i32>,
    #[serde(default)]
    pub duration_sec: Option<f64>,
    #[serde(default)]
    pub crop_mode: Option<String>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MediaAssetDto {
    pub fn is_resized_texture(&self) -> bool {
        self.media_type == MediaAssetType::Texture
            && self.active_file.rel_path != self.original_file.rel_path
    }

    pub fn all_files(&self) -> Vec<&ModelFileMeta> {
        let mut out = Vec::with_capacity(3);
        out.push(&self.original_file);
        if self.active_file.rel_path != self.original_file.rel_path {
            out.push(&self.active_file);
        }
        if let Some(thumb) = &self.thumbnail_file {
            out.push(thumb);
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSyncManifest {
    pub manifest_hash: String,
    pub generated_at: DateTime<Utc>,
    pub total_models: usize,
    #[serde(default)]
    pub total_media: usize,
    pub total_files: usize,
    pub total_bytes: u64,
    pub categories: Vec<ModelCategoryCount>,
    pub models: Vec<ModelAssetDto>,
    #[serde(default)]
    pub media_assets: Vec<MediaAssetDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalogEntry {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub model_type: ModelAssetType,
    pub model_path: String,
    #[serde(default)]
    pub lod2_path: Option<String>,
    pub walk_model_path: Option<String>,
    #[serde(default)]
    pub walk_lod2_path: Option<String>,
    #[serde(default)]
    pub extra_animations: Vec<ModelCatalogActionClip>,
    pub default_height: f64,
    pub default_rotation_deg: [f64; 3],
    pub ready: bool,
}
