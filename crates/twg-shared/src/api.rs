use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{BaseLayer, IndoorConfig, MapMode, SkyboxPreset, UserDto, UserRole};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: u16,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUpdateRequest {
    pub name: String,
    #[serde(default)]
    pub profile_img: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSatkerRequest {
    pub code: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSatkerRequest {
    pub code: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub username: String,
    pub password: String,
    pub role: UserRole,
    #[serde(default)]
    pub satker_id: Option<Uuid>,
    #[serde(default)]
    pub profile_img: Option<String>,
    #[serde(default)]
    pub units: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub name: String,
    pub role: UserRole,
    #[serde(default)]
    pub satker_id: Option<Uuid>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub profile_img: Option<String>,
    #[serde(default)]
    pub units: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOperasiRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub satker_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOperasiRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub satker_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSkenarioRequest {
    pub operasi_id: Uuid,
    pub name: String,
    pub center_lng: f64,
    pub center_lat: f64,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
    #[serde(default = "default_min_zoom")]
    pub min_zoom: f64,
    #[serde(default = "default_max_zoom")]
    pub max_zoom: f64,
    #[serde(default = "default_pitch")]
    pub pitch: f64,
    #[serde(default)]
    pub bearing: f64,
    #[serde(default)]
    pub map_mode: MapMode,
    #[serde(default)]
    pub base_layer: BaseLayer,
    #[serde(default)]
    pub skybox: SkyboxPreset,
    #[serde(default)]
    pub indoor_config: IndoorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSkenarioRequest {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopySkenarioRequest {
    pub target_operasi_id: Uuid,
    #[serde(default)]
    pub new_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveSkenarioRequest {
    pub target_operasi_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMarkerRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub unit_id: String,
    pub name: String,
    pub category: String,
    pub model_path: String,
    #[serde(default = "default_one_i32")]
    pub quantity: i32,
    pub lng: f64,
    pub lat: f64,
    #[serde(default)]
    pub altitude: f64,
    #[serde(default)]
    pub rot_x: f64,
    #[serde(default)]
    pub rot_y: f64,
    #[serde(default)]
    pub rot_z: f64,
    #[serde(default = "default_one_f64")]
    pub scale_x: f64,
    #[serde(default = "default_one_f64")]
    pub scale_y: f64,
    #[serde(default = "default_one_f64")]
    pub scale_z: f64,
    #[serde(default = "default_true")]
    pub is_spawn: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMarkerRequest {
    pub name: String,
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
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuildingRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_building_color")]
    pub color: String,
    #[serde(default = "default_building_height")]
    pub height: f64,
    pub geom: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBuildingRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub color: String,
    pub height: f64,
    pub geom: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlotLineRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub name: String,
    #[serde(default = "default_line_color")]
    pub color: String,
    #[serde(default = "default_line_width")]
    pub width: f64,
    #[serde(default = "default_one_f64")]
    pub alpha: f64,
    pub geom: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlotLineRequest {
    pub name: String,
    pub color: String,
    pub width: f64,
    pub alpha: f64,
    pub geom: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTextMarkerRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub text: String,
    #[serde(default = "default_text_color")]
    pub color: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    pub lng: f64,
    pub lat: f64,
    #[serde(default = "default_text_alt")]
    pub altitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTextMarkerRequest {
    pub text: String,
    pub color: String,
    pub font_size: f64,
    pub lng: f64,
    pub lat: f64,
    pub altitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateParticleRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub name: String,
    pub particle_type: String,
    pub lng: f64,
    pub lat: f64,
    #[serde(default = "default_true")]
    pub is_spawn: bool,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAlurRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAnimRequest {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    #[serde(default)]
    pub marker_id: Option<Uuid>,
    pub name: String,
    pub anim_type: String,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default = "default_anim_speed")]
    pub speed: f64,
    #[serde(default = "default_anim_duration")]
    pub duration_sec: f64,
    #[serde(default)]
    pub replay: bool,
    #[serde(default)]
    pub autoplay: bool,
    #[serde(default)]
    pub geom: Vec<[f64; 3]>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAnimRequest {
    #[serde(default)]
    pub marker_id: Option<Uuid>,
    pub name: String,
    pub anim_type: String,
    pub sort_order: i32,
    pub speed: f64,
    pub duration_sec: f64,
    pub replay: bool,
    pub autoplay: bool,
    pub geom: Vec<[f64; 3]>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSnapshotRequest {
    pub operasi_id: Uuid,
    pub snapshot_enabled: bool,
    #[serde(default)]
    pub map_state: serde_json::Value,
    #[serde(default)]
    pub markers_state: serde_json::Value,
    #[serde(default)]
    pub anims_state: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    pub url: String,
    pub filename: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelCategoryRequest {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub supports_idle_walk: bool,
    #[serde(default = "default_cat_scale")]
    pub default_scale: f64,
    #[serde(default = "default_cat_sort_order")]
    pub sort_order: i32,
    #[serde(default)]
    pub play_behavior: crate::models::CategoryPlayBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModelCategoryRequest {
    pub name: String,
    pub supports_idle_walk: bool,
    pub default_scale: f64,
    pub sort_order: i32,
    #[serde(default)]
    pub play_behavior: crate::models::CategoryPlayBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLocationPresetRequest {
    pub name: String,
    pub center_lng: f64,
    pub center_lat: f64,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
    #[serde(default = "default_min_zoom")]
    pub min_zoom: f64,
    #[serde(default = "default_max_zoom")]
    pub max_zoom: f64,
    #[serde(default = "default_pitch")]
    pub pitch: f64,
    #[serde(default)]
    pub bearing: f64,
    #[serde(default)]
    pub map_mode: MapMode,
    #[serde(default)]
    pub base_layer: BaseLayer,
    #[serde(default = "default_preset_grid_size")]
    pub grid_size_m: f64,
    #[serde(default = "default_cat_sort_order")]
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateLocationPresetRequest {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeTextureRequest {
    /// Target symmetric square size (target_size x target_size). Must be <= original_width.
    pub target_size: u32,
}

fn default_cat_scale() -> f64 {
    3.5
}

fn default_cat_sort_order() -> i32 {
    100
}

fn default_true() -> bool {
    true
}
fn default_zoom() -> f64 {
    16.0
}
fn default_min_zoom() -> f64 {
    3.0
}
fn default_max_zoom() -> f64 {
    26.0
}
fn default_preset_grid_size() -> f64 {
    10.0
}
fn default_pitch() -> f64 {
    45.0
}
fn default_one_i32() -> i32 {
    1
}
fn default_one_f64() -> f64 {
    1.0
}
fn default_building_color() -> String {
    "#38bdf8".to_string()
}
fn default_building_height() -> f64 {
    24.0
}
fn default_line_color() -> String {
    "#f59e0b".to_string()
}
fn default_line_width() -> f64 {
    3.0
}
fn default_text_color() -> String {
    "#ffffff".to_string()
}
fn default_font_size() -> f64 {
    14.0
}
fn default_text_alt() -> f64 {
    12.0
}
fn default_anim_speed() -> f64 {
    12.0
}
fn default_anim_duration() -> f64 {
    20.0
}
