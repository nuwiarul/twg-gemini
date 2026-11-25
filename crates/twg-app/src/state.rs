use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use twg_shared::{
    geo::GeoAnchor,
    models::{
        Alur, Anim, BaseLayer, Building, LocationPreset, MapMode, Marker, MediaAssetDto,
        MediaAssetType, ModelAssetDto, ModelAssetType, ModelCatalogEntry, ModelCategoryCount,
        ModelSyncManifest, Operasi, Particle, PlotLine, Satker, Skenario, TacticalOperasiCard,
        TextMarker, UserDto, UserRole, default_model_category_counts,
    },
    ws::{OnlineOperator, TacticalCommandBroadcast},
};
pub use twg_shared::models::SkyboxPreset;
use uuid::Uuid;

use crate::{
    net::asset_sync::{load_cached_sync_manifest, scan_existing_local_models},
    scene::basic_mesh::{BasicMeshShape, is_basic_mesh_marker},
};

#[derive(Debug, Clone)]
pub struct PendingDeleteEntity {
    pub entity: SelectedEntity,
    pub kind_label: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppScreen {
    #[default]
    Login,
    Operasis,
    Skenarios,
    PlotSkenario,
    Tactical,
    LayarBesar,
    Satkers,
    Users,
    Assets,
    Profile,
}

#[derive(Debug, Clone, Default)]
pub struct ProfileFormState {
    pub name: String,
    pub profile_submitting: bool,
    pub profile_error: Option<String>,
    pub profile_success: Option<String>,

    pub old_password: String,
    pub new_password: String,
    pub confirm_password: String,
    pub show_old_password: bool,
    pub show_new_password: bool,
    pub show_confirm_password: bool,
    pub password_submitting: bool,
    pub password_error: Option<String>,
    pub password_success: Option<String>,
}

impl ProfileFormState {
    pub fn load_from_user(&mut self, user: &UserDto) {
        self.name = user.name.clone();
        self.profile_error = None;
        self.profile_success = None;
        self.old_password.clear();
        self.new_password.clear();
        self.confirm_password.clear();
        self.show_old_password = false;
        self.show_new_password = false;
        self.show_confirm_password = false;
        self.password_error = None;
        self.password_success = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaySessionMode {
    /// Play Skenario launched from Plot Editor: 100% local preview (never broadcasts or receives live tactical sync).
    #[default]
    LocalPreview,
    /// Play Skenario launched from Tactical / Simulasi: live realtime sync across peers & Layar Besar.
    TacticalLive,
    /// Fullscreen read-only Layar Besar display driven exclusively by remote WebSocket commands.
    BigScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayPanelTab {
    #[default]
    Animasi,
    Skenario,
}

#[derive(Debug, Clone, Default)]
pub enum SatkerModalMode {
    #[default]
    None,
    Create,
    Edit(Uuid),
    DeleteConfirm(Satker),
}

#[derive(Debug, Clone, Default)]
pub struct SatkerTableState {
    pub satkers: Vec<Satker>,
    pub search: String,
    pub loading: bool,
    pub error: Option<String>,
    pub modal_mode: SatkerModalMode,
    pub form_code: String,
    pub form_name: String,
    pub form_is_active: bool,
    pub submitting: bool,
    pub form_error: Option<String>,
}

impl SatkerTableState {
    pub fn open_create(&mut self) {
        self.modal_mode = SatkerModalMode::Create;
        self.form_code.clear();
        self.form_name.clear();
        self.form_is_active = true;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_edit(&mut self, s: &Satker) {
        self.modal_mode = SatkerModalMode::Edit(s.id);
        self.form_code = s.code.clone();
        self.form_name = s.name.clone();
        self.form_is_active = s.is_active;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_delete(&mut self, s: &Satker) {
        self.modal_mode = SatkerModalMode::DeleteConfirm(s.clone());
        self.submitting = false;
        self.form_error = None;
    }

    pub fn close_modal(&mut self) {
        self.modal_mode = SatkerModalMode::None;
        self.submitting = false;
        self.form_error = None;
    }
}

#[derive(Debug, Clone, Default)]
pub enum UserModalMode {
    #[default]
    None,
    Create,
    Edit(Uuid),
    DeleteConfirm(Box<UserDto>),
}

#[derive(Debug, Clone)]
pub struct UserTableState {
    pub users: Vec<UserDto>,
    pub search: String,
    pub filter_role: Option<UserRole>,
    pub loading: bool,
    pub error: Option<String>,
    pub modal_mode: UserModalMode,
    pub form_name: String,
    pub form_username: String,
    pub form_password: String,
    pub form_show_password: bool,
    pub form_role: UserRole,
    pub form_satker_id: Option<Uuid>,
    pub submitting: bool,
    pub form_error: Option<String>,
}

impl Default for UserTableState {
    fn default() -> Self {
        Self {
            users: Vec::new(),
            search: String::new(),
            filter_role: None,
            loading: false,
            error: None,
            modal_mode: UserModalMode::None,
            form_name: String::new(),
            form_username: String::new(),
            form_password: String::new(),
            form_show_password: false,
            form_role: UserRole::User,
            form_satker_id: None,
            submitting: false,
            form_error: None,
        }
    }
}

impl UserTableState {
    pub fn open_create(&mut self, default_role: UserRole, default_satker_id: Option<Uuid>) {
        self.modal_mode = UserModalMode::Create;
        self.form_name.clear();
        self.form_username.clear();
        self.form_password.clear();
        self.form_show_password = false;
        self.form_role = default_role;
        self.form_satker_id = default_satker_id;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_edit(&mut self, u: &UserDto) {
        self.modal_mode = UserModalMode::Edit(u.id);
        self.form_name = u.name.clone();
        self.form_username = u.username.clone();
        self.form_password.clear();
        self.form_show_password = false;
        self.form_role = u.role;
        self.form_satker_id = u.satker_id;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_delete(&mut self, u: &UserDto) {
        self.modal_mode = UserModalMode::DeleteConfirm(Box::new(u.clone()));
        self.submitting = false;
        self.form_error = None;
    }

    pub fn close_modal(&mut self) {
        self.modal_mode = UserModalMode::None;
        self.submitting = false;
        self.form_error = None;
    }
}

#[derive(Debug, Clone)]
pub struct TacticalPageState {
    pub cards: Vec<TacticalOperasiCard>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub search: String,
    pub applied_search: String,
    pub loading: bool,
    pub error: Option<String>,
    /// Set when opening a scenario from Tactical / Simulasi so BundleLoaded transitions into TacticalLive Play Skenario.
    pub pending_open_tactical_id: Option<Uuid>,
    /// Whether opening/reloading this scenario should also broadcast `load_skenario` / `reload_skenario` to Layar Besar.
    pub pending_broadcast_big_screen: Option<&'static str>,
    /// Skenarios of the currently active Operasi displayed in the "Skenario" tab during Play Skenario Tactical.
    pub active_operasi_skenarios: Vec<Skenario>,
    pub active_operasi_name: String,
    pub play_panel_tab: PlayPanelTab,
}

impl Default for TacticalPageState {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            page: 1,
            per_page: 6,
            total: 0,
            search: String::new(),
            applied_search: String::new(),
            loading: false,
            error: None,
            pending_open_tactical_id: None,
            pending_broadcast_big_screen: None,
            active_operasi_skenarios: Vec::new(),
            active_operasi_name: String::new(),
            play_panel_tab: PlayPanelTab::Animasi,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BigScreenState {
    pub loaded_by_actor: Option<String>,
    pub pending_skenario_id: Option<Uuid>,
    pub pending_camera_payload: Option<serde_json::Value>,
    pub last_event_text: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub enum OperasiModalMode {
    #[default]
    None,
    Create,
    Edit(Uuid),
    Detail(Operasi),
    DeleteConfirm(Operasi),
}

#[derive(Debug, Clone)]
pub struct OperasiTableState {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub search: String,
    pub applied_search: String,
    pub loading: bool,
    pub error: Option<String>,
    pub satkers: Vec<Satker>,

    pub modal_mode: OperasiModalMode,
    pub form_name: String,
    pub form_description: String,
    pub form_satker_id: Option<Uuid>,
    pub submitting: bool,
    pub form_error: Option<String>,
}

impl Default for OperasiTableState {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 10,
            total: 0,
            search: String::new(),
            applied_search: String::new(),
            loading: false,
            error: None,
            satkers: Vec::new(),
            modal_mode: OperasiModalMode::None,
            form_name: String::new(),
            form_description: String::new(),
            form_satker_id: None,
            submitting: false,
            form_error: None,
        }
    }
}

impl OperasiTableState {
    pub fn open_create(&mut self, default_satker_id: Option<Uuid>) {
        self.modal_mode = OperasiModalMode::Create;
        self.form_name.clear();
        self.form_description.clear();
        self.form_satker_id = default_satker_id;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_edit(&mut self, op: &Operasi) {
        self.modal_mode = OperasiModalMode::Edit(op.id);
        self.form_name = op.name.clone();
        self.form_description = op.description.clone().unwrap_or_default();
        self.form_satker_id = op.satker_id;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_detail(&mut self, op: &Operasi) {
        self.modal_mode = OperasiModalMode::Detail(op.clone());
        self.form_error = None;
    }

    pub fn open_delete(&mut self, op: &Operasi) {
        self.modal_mode = OperasiModalMode::DeleteConfirm(op.clone());
        self.submitting = false;
        self.form_error = None;
    }

    pub fn close_modal(&mut self) {
        self.modal_mode = OperasiModalMode::None;
        self.submitting = false;
        self.form_error = None;
    }
}

#[derive(Debug, Clone, Default)]
pub enum SkenarioModalMode {
    #[default]
    None,
    Create,
    Edit(Uuid),
    Copy(Box<Skenario>),
    Move(Box<Skenario>),
    DeleteConfirm(Box<Skenario>),
}

#[derive(Debug, Clone)]
pub struct SkenarioTableState {
    pub operasi: Option<Operasi>,
    pub filter_operasi_id: Option<Uuid>,
    pub operasi_options: Vec<Operasi>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub search: String,
    pub applied_search: String,
    pub loading: bool,
    pub error: Option<String>,
    pub pending_open_plot_id: Option<Uuid>,

    pub modal_mode: SkenarioModalMode,
    pub form_operasi_id: Option<Uuid>,
    pub form_name: String,
    pub form_preset_id: Option<Uuid>,
    pub form_center_lng: f64,
    pub form_center_lat: f64,
    pub form_zoom: f64,
    pub form_min_zoom: f64,
    pub form_max_zoom: f64,
    pub form_pitch: f64,
    pub form_bearing: f64,
    pub form_map_mode: MapMode,
    pub form_base_layer: BaseLayer,
    pub form_skybox: SkyboxPreset,
    pub form_grid_size_m: f64,
    pub map_preview_dirty: bool,
    pub map_preview_snap: bool,
    pub submitting: bool,
    pub form_error: Option<String>,
}

impl Default for SkenarioTableState {
    fn default() -> Self {
        let (_, lng, lat, zoom) = LOCATION_PRESETS[0];
        Self {
            operasi: None,
            filter_operasi_id: None,
            operasi_options: Vec::new(),
            page: 1,
            per_page: 10,
            total: 0,
            search: String::new(),
            applied_search: String::new(),
            loading: false,
            error: None,
            pending_open_plot_id: None,
            modal_mode: SkenarioModalMode::None,
            form_operasi_id: None,
            form_name: String::new(),
            form_preset_id: None,
            form_center_lng: lng,
            form_center_lat: lat,
            form_zoom: zoom,
            form_min_zoom: 3.0,
            form_max_zoom: 26.0,
            form_pitch: 48.0,
            form_bearing: 0.0,
            form_map_mode: MapMode::Outdoor,
            form_base_layer: BaseLayer::Osm,
            form_skybox: SkyboxPreset::DayClear,
            form_grid_size_m: 10.0,
            map_preview_dirty: false,
            map_preview_snap: false,
            submitting: false,
            form_error: None,
        }
    }
}

impl SkenarioTableState {
    pub fn clamp_zoom_bounds(&mut self) {
        self.form_min_zoom = self.form_min_zoom.clamp(1.0, 26.0);
        self.form_max_zoom = self.form_max_zoom.clamp(self.form_min_zoom, 26.0);
        self.form_zoom = self.form_zoom.clamp(self.form_min_zoom, self.form_max_zoom);
    }

    pub fn apply_location_preset(&mut self, preset: &LocationPreset) {
        self.form_preset_id = Some(preset.id);
        self.form_center_lng = preset.center_lng;
        self.form_center_lat = preset.center_lat;
        self.form_min_zoom = preset.min_zoom.clamp(1.0, 26.0);
        self.form_max_zoom = preset.max_zoom.clamp(self.form_min_zoom, 26.0);
        self.form_zoom = preset.zoom.clamp(self.form_min_zoom, self.form_max_zoom);
        self.form_pitch = preset.pitch.clamp(5.0, 82.0);
        self.form_bearing = preset.bearing.rem_euclid(360.0);
        self.form_map_mode = preset.map_mode;
        self.form_base_layer = preset.base_layer;
        self.form_grid_size_m = preset.grid_size_m.clamp(1.0, 200.0);
        self.map_preview_dirty = true;
        self.map_preview_snap = true;
    }

    pub fn open_create(&mut self, presets: &[LocationPreset]) {
        self.modal_mode = SkenarioModalMode::Create;
        self.form_operasi_id = self
            .filter_operasi_id
            .or_else(|| self.operasi.as_ref().map(|o| o.id))
            .or_else(|| self.operasi_options.first().map(|o| o.id));
        self.form_name.clear();
        if let Some(first) = presets.first() {
            self.apply_location_preset(first);
        } else {
            let (_, lng, lat, zoom) = LOCATION_PRESETS[0];
            self.form_preset_id = None;
            self.form_center_lng = lng;
            self.form_center_lat = lat;
            self.form_min_zoom = 3.0;
            self.form_max_zoom = 26.0;
            self.form_zoom = zoom;
            self.form_pitch = 48.0;
            self.form_bearing = 0.0;
            self.form_map_mode = MapMode::Outdoor;
            self.form_base_layer = BaseLayer::Osm;
            self.form_skybox = SkyboxPreset::DayClear;
            self.form_grid_size_m = 10.0;
            self.map_preview_dirty = true;
            self.map_preview_snap = true;
        }
        self.form_skybox = SkyboxPreset::DayClear;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_edit(&mut self, sk: &Skenario) {
        self.modal_mode = SkenarioModalMode::Edit(sk.id);
        self.form_operasi_id = Some(sk.operasi_id);
        self.form_name = sk.name.clone();
        self.form_preset_id = None;
        self.form_center_lng = sk.center_lng;
        self.form_center_lat = sk.center_lat;
        self.form_min_zoom = sk.min_zoom.clamp(1.0, 26.0);
        self.form_max_zoom = sk.max_zoom.clamp(self.form_min_zoom, 26.0);
        self.form_zoom = sk.zoom.clamp(self.form_min_zoom, self.form_max_zoom);
        self.form_pitch = sk.pitch.clamp(5.0, 82.0);
        self.form_bearing = sk.bearing.rem_euclid(360.0);
        self.form_map_mode = sk.map_mode;
        self.form_base_layer = sk.base_layer;
        self.form_skybox = sk.skybox;
        self.form_grid_size_m = sk.indoor_config.grid_size_m.max(1.0);
        self.map_preview_dirty = true;
        self.map_preview_snap = true;
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_copy(&mut self, sk: &Skenario) {
        self.modal_mode = SkenarioModalMode::Copy(Box::new(sk.clone()));
        self.form_operasi_id = Some(sk.operasi_id);
        self.form_name = format!("{} (Salinan)", sk.name);
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_move(&mut self, sk: &Skenario) {
        self.modal_mode = SkenarioModalMode::Move(Box::new(sk.clone()));
        self.form_operasi_id = self
            .operasi_options
            .iter()
            .find(|o| o.id != sk.operasi_id)
            .map(|o| o.id);
        self.submitting = false;
        self.form_error = None;
    }

    pub fn open_delete(&mut self, sk: &Skenario) {
        self.modal_mode = SkenarioModalMode::DeleteConfirm(Box::new(sk.clone()));
        self.submitting = false;
        self.form_error = None;
    }

    pub fn close_modal(&mut self) {
        self.modal_mode = SkenarioModalMode::None;
        self.submitting = false;
        self.form_error = None;
    }
}

#[derive(Debug, Clone)]
pub struct LocationPresetManagerState {
    pub open: bool,
    pub editing_id: Option<Uuid>,
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
    pub confirm_delete_id: Option<Uuid>,
    pub quick_save_open: bool,
    pub quick_save_name: String,
    pub submitting: bool,
    pub error: Option<String>,
    pub success: Option<String>,
}

impl Default for LocationPresetManagerState {
    fn default() -> Self {
        Self {
            open: false,
            editing_id: None,
            name: String::new(),
            center_lng: 115.2167,
            center_lat: -8.6705,
            zoom: 16.5,
            min_zoom: 3.0,
            max_zoom: 26.0,
            pitch: 48.0,
            bearing: 0.0,
            map_mode: MapMode::Outdoor,
            base_layer: BaseLayer::Osm,
            grid_size_m: 10.0,
            sort_order: 60,
            confirm_delete_id: None,
            quick_save_open: false,
            quick_save_name: String::new(),
            submitting: false,
            error: None,
            success: None,
        }
    }
}

impl LocationPresetManagerState {
    pub fn reset_form(&mut self, next_sort: i32) {
        self.editing_id = None;
        self.name.clear();
        self.center_lng = 115.2167;
        self.center_lat = -8.6705;
        self.zoom = 16.5;
        self.min_zoom = 3.0;
        self.max_zoom = 26.0;
        self.pitch = 48.0;
        self.bearing = 0.0;
        self.map_mode = MapMode::Outdoor;
        self.base_layer = BaseLayer::Osm;
        self.grid_size_m = 10.0;
        self.sort_order = next_sort;
        self.error = None;
        self.confirm_delete_id = None;
    }

    pub fn load_preset(&mut self, p: &LocationPreset) {
        self.editing_id = Some(p.id);
        self.name = p.name.clone();
        self.center_lng = p.center_lng;
        self.center_lat = p.center_lat;
        self.min_zoom = p.min_zoom.clamp(1.0, 26.0);
        self.max_zoom = p.max_zoom.clamp(self.min_zoom, 26.0);
        self.zoom = p.zoom.clamp(self.min_zoom, self.max_zoom);
        self.pitch = p.pitch.clamp(5.0, 82.0);
        self.bearing = p.bearing.rem_euclid(360.0);
        self.map_mode = p.map_mode;
        self.base_layer = p.base_layer;
        self.grid_size_m = p.grid_size_m.clamp(1.0, 200.0);
        self.sort_order = p.sort_order;
        self.error = None;
        self.success = None;
        self.confirm_delete_id = None;
    }

    pub fn copy_from_skenario_form(&mut self, sk: &SkenarioTableState) {
        self.center_lng = sk.form_center_lng;
        self.center_lat = sk.form_center_lat;
        self.min_zoom = sk.form_min_zoom.clamp(1.0, 26.0);
        self.max_zoom = sk.form_max_zoom.clamp(self.min_zoom, 26.0);
        self.zoom = sk.form_zoom.clamp(self.min_zoom, self.max_zoom);
        self.pitch = sk.form_pitch.clamp(5.0, 82.0);
        self.bearing = sk.form_bearing.rem_euclid(360.0);
        self.map_mode = sk.form_map_mode;
        self.base_layer = sk.form_base_layer;
        self.grid_size_m = sk.form_grid_size_m.clamp(1.0, 200.0);
    }

    pub fn clamp_zoom_bounds(&mut self) {
        self.min_zoom = self.min_zoom.clamp(1.0, 26.0);
        self.max_zoom = self.max_zoom.clamp(self.min_zoom, 26.0);
        self.zoom = self.zoom.clamp(self.min_zoom, self.max_zoom);
    }
}

pub fn default_location_presets() -> Vec<LocationPreset> {
    let now = chrono::Utc::now();
    LOCATION_PRESETS
        .iter()
        .enumerate()
        .map(|(idx, (name, lng, lat, zoom))| {
            let is_indoor = idx == 4;
            LocationPreset {
                id: Uuid::new_v4(),
                name: (*name).to_string(),
                center_lng: *lng,
                center_lat: *lat,
                zoom: *zoom,
                min_zoom: 3.0,
                max_zoom: 26.0,
                pitch: 48.0,
                bearing: 0.0,
                map_mode: if is_indoor {
                    MapMode::Indoor
                } else {
                    MapMode::Outdoor
                },
                base_layer: if is_indoor {
                    BaseLayer::GridDark
                } else {
                    BaseLayer::Osm
                },
                grid_size_m: 10.0,
                sort_order: ((idx + 1) * 10) as i32,
                created_at: now,
                updated_at: now,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetPreviewLod {
    #[default]
    Lod0,
    Lod2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetPreviewVariant {
    #[default]
    Idle,
    Walk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetsSection {
    #[default]
    Models3d,
    Texture,
    Sound,
    Video,
}

impl AssetsSection {
    pub fn media_type(&self) -> Option<MediaAssetType> {
        match self {
            Self::Models3d => None,
            Self::Texture => Some(MediaAssetType::Texture),
            Self::Sound => Some(MediaAssetType::Sound),
            Self::Video => Some(MediaAssetType::Video),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetFormMode {
    #[default]
    Inspect,
    Create,
    Categories,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaFormMode {
    #[default]
    Inspect,
    Create,
}

#[derive(Debug, Clone)]
pub struct MediaFormState {
    pub editing_id: Option<Uuid>,
    pub name: String,
    pub category: String,
    pub crop_mode: String,
    pub file_path: Option<PathBuf>,
    pub custom_resize_size: u32,
    pub submitting: bool,
    pub error: Option<String>,
    pub success: Option<String>,
    pub confirm_delete_id: Option<Uuid>,
}

impl Default for MediaFormState {
    fn default() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            category: "general".to_string(),
            crop_mode: "crop".to_string(),
            file_path: None,
            custom_resize_size: 256,
            submitting: false,
            error: None,
            success: None,
            confirm_delete_id: None,
        }
    }
}

impl MediaFormState {
    pub fn reset_for_create(&mut self) {
        *self = Self::default();
    }

    pub fn load_from_media(&mut self, media: &MediaAssetDto) {
        self.editing_id = Some(media.id);
        self.name = media.name.clone();
        self.category = media.category.clone();
        self.crop_mode = media
            .crop_mode
            .clone()
            .unwrap_or_else(|| "crop".to_string());
        self.file_path = None;
        self.custom_resize_size = media
            .active_width
            .or(media.original_width)
            .unwrap_or(256)
            .max(16) as u32;
        self.submitting = false;
        self.error = None;
        self.success = None;
        self.confirm_delete_id = None;
    }
}

#[derive(Debug, Clone)]
pub struct CategoryFormState {
    pub new_name: String,
    pub new_slug: String,
    pub slug_manually_edited: bool,
    pub new_supports_idle_walk: bool,
    pub new_default_scale: f64,
    pub new_sort_order: i32,
    pub new_play_behavior: twg_shared::models::CategoryPlayBehavior,

    pub editing_slug: Option<String>,
    pub edit_name: String,
    pub edit_supports_idle_walk: bool,
    pub edit_default_scale: f64,
    pub edit_sort_order: i32,
    pub edit_play_behavior: twg_shared::models::CategoryPlayBehavior,

    pub confirm_delete_slug: Option<String>,
    pub submitting: bool,
    pub error: Option<String>,
    pub success: Option<String>,
}

impl Default for CategoryFormState {
    fn default() -> Self {
        Self {
            new_name: String::new(),
            new_slug: String::new(),
            slug_manually_edited: false,
            new_supports_idle_walk: false,
            new_default_scale: 0.1,
            new_sort_order: 80,
            new_play_behavior: twg_shared::models::CategoryPlayBehavior::RtsUnit,
            editing_slug: None,
            edit_name: String::new(),
            edit_supports_idle_walk: false,
            edit_default_scale: 0.1,
            edit_sort_order: 80,
            edit_play_behavior: twg_shared::models::CategoryPlayBehavior::RtsUnit,
            confirm_delete_slug: None,
            submitting: false,
            error: None,
            success: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelSyncStatus {
    pub is_syncing: bool,
    pub checking: bool,
    pub completed_files: usize,
    pub total_files: usize,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub current_file: String,
    pub summary_text: String,
    pub offline_mode: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssetFormState {
    pub editing_id: Option<Uuid>,
    pub name: String,
    pub category: String,
    pub model_type: ModelAssetType,
    pub default_height: f64,
    pub default_rot_x: f64,
    pub default_rot_y: f64,
    pub default_rot_z: f64,
    pub model_file_path: Option<PathBuf>,
    pub idle_file_path: Option<PathBuf>,
    pub walk_file_path: Option<PathBuf>,
    pub submitting: bool,
    pub error: Option<String>,
    pub success: Option<String>,
    pub confirm_delete_id: Option<Uuid>,
}

impl Default for AssetFormState {
    fn default() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            category: "units".to_string(),
            model_type: ModelAssetType::Animated,
            default_height: 20.0,
            default_rot_x: 0.0,
            default_rot_y: 0.0,
            default_rot_z: 0.0,
            model_file_path: None,
            idle_file_path: None,
            walk_file_path: None,
            submitting: false,
            error: None,
            success: None,
            confirm_delete_id: None,
        }
    }
}

impl AssetFormState {
    pub fn reset_for_create(&mut self, default_category: &str, supports_idle_walk: bool) {
        let cat = if default_category == "all" || default_category.is_empty() {
            "units"
        } else {
            default_category
        };
        *self = Self {
            editing_id: None,
            name: String::new(),
            category: cat.to_string(),
            model_type: if supports_idle_walk {
                ModelAssetType::Animated
            } else {
                ModelAssetType::Static
            },
            default_height: if supports_idle_walk { 20.0 } else { 10.0 },
            default_rot_x: 0.0,
            default_rot_y: 0.0,
            default_rot_z: 0.0,
            model_file_path: None,
            idle_file_path: None,
            walk_file_path: None,
            submitting: false,
            error: None,
            success: None,
            confirm_delete_id: None,
        };
    }

    pub fn load_from_asset(&mut self, asset: &ModelAssetDto) {
        self.editing_id = Some(asset.id);
        self.name = asset.name.clone();
        self.category = asset.category.clone();
        self.model_type = asset.model_type;
        self.default_height = asset.default_height;
        self.default_rot_x = asset.default_rotation_deg[0];
        self.default_rot_y = asset.default_rotation_deg[1];
        self.default_rot_z = asset.default_rotation_deg[2];
        self.model_file_path = None;
        self.idle_file_path = None;
        self.walk_file_path = None;
        self.submitting = false;
        self.error = None;
        self.success = None;
        self.confirm_delete_id = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolMode {
    #[default]
    Select,
    MoveMarker,
    RotateMarker,
    ScaleMarker,
    DrawLine,
    DrawBuilding,
    DrawBasicPolygon,
    PlaceText,
    DrawRoute,
    PickRouteMarker,
    PickShootMarker,
    PickPointerMarker,
    PickSpawnHideMarker,
    PlaceParticle,
    Measure,
}

impl ToolMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::MoveMarker => "Move",
            Self::RotateMarker => "Rotate",
            Self::ScaleMarker => "Scale",
            Self::DrawLine => "Draw Line",
            Self::DrawBuilding => "Draw Bldg",
            Self::DrawBasicPolygon => "Draw Polygon",
            Self::PlaceText => "Add Label",
            Self::DrawRoute => "Draw Route",
            Self::PickRouteMarker => "Pilih Unit",
            Self::PickShootMarker => "Pilih Unit Shoot",
            Self::PickPointerMarker => "Pilih Target Panah",
            Self::PickSpawnHideMarker => "Pilih Objek Spawn/Hide",
            Self::PlaceParticle => "Letakkan Partikel",
            Self::Measure => "Measure",
        }
    }

    pub fn is_transform_tool(&self) -> bool {
        matches!(
            self,
            Self::Select | Self::MoveMarker | Self::RotateMarker | Self::ScaleMarker
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftTab {
    #[default]
    Layers,
    Catalog,
    BasicMesh,
    Animations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimCategory {
    #[default]
    Route,
    Shoot,
    Particle,
    SpawnHide,
    Group,
    Other,
}

impl AnimCategory {
    pub const ALL: &'static [Self] = &[
        Self::Route,
        Self::Shoot,
        Self::Particle,
        Self::SpawnHide,
        Self::Group,
        Self::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Route => "Rute",
            Self::Shoot => "Shoot",
            Self::Particle => "Particle",
            Self::SpawnHide => "Spawn & Hide",
            Self::Group => "Grup",
            Self::Other => "Lainnya",
        }
    }

    pub fn from_anim_type(anim_type: &str) -> Self {
        match anim_type {
            "route" | "single_route" | "route_multi" | "convoy_route" => Self::Route,
            s if s == "shoot" || s.starts_with("shoot_") => Self::Shoot,
            s if s == "particle"
                || s.starts_with("particle_")
                || matches!(s, "fire" | "smoke" | "explosion") =>
            {
                Self::Particle
            }
            s if s == "spawn_hide"
                || s.starts_with("spawn_hide_")
                || matches!(s, "spawn" | "hide") =>
            {
                Self::SpawnHide
            }
            s if s == "group"
                || s.starts_with("group_")
                || matches!(s, "combine" | "sequence") =>
            {
                Self::Group
            }
            s if s == "pointer_arrow" || s.starts_with("pointer_") || s.starts_with("other_") => {
                Self::Other
            }
            _ => Self::Route,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayerVisibilityFilter {
    #[default]
    All,
    VisibleOnly,
    HiddenOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayerLockFilter {
    #[default]
    All,
    UnlockedOnly,
    LockedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum SelectedEntity {
    #[default]
    None,
    Marker(Uuid),
    Building(Uuid),
    Line(Uuid),
    TextMarker(Uuid),
    Anim(Uuid),
}

#[derive(Resource, Default)]
pub struct UiPointerState {
    pub wants_pointer: bool,
    pub wants_keyboard: bool,
    pub viewport_rect_logical: Option<egui::Rect>,
}

use bevy_egui::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimRunStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct AnimRuntimeTrack {
    pub status: AnimRunStatus,
    pub distance_m: f64,
    pub total_length_m: f64,
    pub has_pose_override: bool,
    /// Monotonic trigger sequence incremented each time `play_anim` starts this track,
    /// used to deterministically resolve last-action-wins when Spawn & Hide target the same marker.
    pub trigger_seq: u64,
    /// Number of full cycles completed during the current run (increments on `replay` wrap-around
    /// AND when reaching `Finished`). Enables `Sequence` to advance after 1 cycle of a `repeat` child.
    pub completed_cycles: u32,
    /// Group runtime cursor (`Combine` / `Sequence`): count of steps already launched (`0..=items.len()`).
    pub group_launched_count: usize,
    /// Group elapsed seconds when the most recently launched step started.
    pub group_last_step_start_sec: f64,
    /// Group elapsed seconds when the previous step first reached completion (used for `WaitComplete` + gap).
    pub group_prev_done_at_sec: Option<f64>,
    /// Snapshot of child IDs launched by this group run for synchronized Pause / Resume / Stop.
    pub group_owned_children: Vec<Uuid>,
}

#[derive(Resource)]
pub struct PlaybackState {
    pub play_test_mode: bool,
    /// Non-destructive Plot-Editor preview for a selected Route or Group animation (runs without entering Play Skenario).
    pub editor_preview_anim_id: Option<Uuid>,
    pub playing: bool,
    pub play_all_anims: bool,
    pub active_anim_id: Option<Uuid>,
    pub elapsed_sec: f32,
    pub speed_multiplier: f32,
    pub loop_anim: bool,
    pub hide_gizmos_in_play: bool,
    pub tracks: HashMap<Uuid, AnimRuntimeTrack>,
    pub next_trigger_seq: u64,
    pub play_panel_category: Option<AnimCategory>,
    pub play_panel_search: String,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            play_test_mode: false,
            editor_preview_anim_id: None,
            playing: false,
            play_all_anims: true,
            active_anim_id: None,
            elapsed_sec: 0.0,
            speed_multiplier: 1.0,
            loop_anim: false,
            hide_gizmos_in_play: true,
            tracks: HashMap::new(),
            next_trigger_seq: 0,
            play_panel_category: None,
            play_panel_search: String::new(),
        }
    }
}

impl PlaybackState {
    /// Returns `true` when either full Play Skenario or a Plot-Editor Route/Group preview is active.
    pub fn is_playback_active(&self) -> bool {
        self.play_test_mode || self.editor_preview_anim_id.is_some()
    }

    /// Returns `true` when Plot Editor is actively previewing `anim_id`.
    pub fn is_previewing_anim(&self, anim_id: Uuid) -> bool {
        !self.play_test_mode && self.editor_preview_anim_id == Some(anim_id)
    }

    /// Starts (or restarts) a non-destructive Plot-Editor preview for `anim_id` (Route or Group).
    pub fn start_editor_preview(&mut self, anim_id: Uuid) {
        self.play_test_mode = false;
        self.tracks.clear();
        self.next_trigger_seq = 0;
        self.editor_preview_anim_id = Some(anim_id);
        self.play_anim(anim_id);
    }

    /// Stops any active Plot-Editor Route/Group preview and snaps all markers back to resting positions.
    pub fn stop_editor_preview(&mut self) {
        self.editor_preview_anim_id = None;
        if !self.play_test_mode {
            self.playing = false;
            self.tracks.clear();
            self.next_trigger_seq = 0;
        }
    }

    /// Enters Play Skenario mode with all animations ready (Idle/Stopped) in the Animation Panel.
    pub fn enter_play_mode(&mut self) {
        self.editor_preview_anim_id = None;
        self.play_test_mode = true;
        self.playing = false;
        self.elapsed_sec = 0.0;
        self.hide_gizmos_in_play = true;
        self.tracks.clear();
        self.next_trigger_seq = 0;
        self.play_panel_search.clear();
    }

    pub fn track_status(&self, anim_id: Uuid) -> AnimRunStatus {
        self.tracks
            .get(&anim_id)
            .map(|t| t.status)
            .unwrap_or(AnimRunStatus::Stopped)
    }

    /// Starts (or restarts) an animation from the beginning (`t = 0`, `completed_cycles = 0`).
    pub fn play_anim(&mut self, anim_id: Uuid) {
        if self.editor_preview_anim_id.is_none() {
            self.play_test_mode = true;
        }
        self.next_trigger_seq = self.next_trigger_seq.wrapping_add(1).max(1);
        let seq = self.next_trigger_seq;
        let entry = self.tracks.entry(anim_id).or_default();
        entry.distance_m = 0.0;
        entry.status = AnimRunStatus::Playing;
        entry.has_pose_override = true;
        entry.trigger_seq = seq;
        entry.completed_cycles = 0;
        entry.group_launched_count = 0;
        entry.group_last_step_start_sec = 0.0;
        entry.group_prev_done_at_sec = None;
        entry.group_owned_children.clear();
        self.active_anim_id = Some(anim_id);
        self.playing = true;
    }

    /// Pauses an active animation: freezes marker at its current position and restores Idle.
    pub fn pause_anim(&mut self, anim_id: Uuid) {
        if let Some(entry) = self.tracks.get_mut(&anim_id)
            && entry.status == AnimRunStatus::Playing
        {
            entry.status = AnimRunStatus::Paused;
            entry.has_pose_override = true;
        }
        self.playing = self
            .tracks
            .values()
            .any(|t| t.status == AnimRunStatus::Playing);
    }

    /// Pauses an animation and (if it is a `Grup` Combine/Sequence) all active descendant children.
    pub fn pause_anim_tree(&mut self, anim_id: Uuid, anims: &[Anim]) {
        self.pause_anim(anim_id);
        for cid in twg_shared::group_anim::collect_group_descendant_ids(anim_id, anims) {
            self.pause_anim(cid);
        }
    }

    /// Resumes a paused animation from its exact last position and switches back to Walk.
    pub fn resume_anim(&mut self, anim_id: Uuid) {
        if let Some(entry) = self.tracks.get_mut(&anim_id)
            && entry.status == AnimRunStatus::Paused
        {
            entry.status = AnimRunStatus::Playing;
            entry.has_pose_override = true;
            self.playing = true;
        }
    }

    /// Resumes a paused animation and (if it is a `Grup` Combine/Sequence) all paused descendant children.
    pub fn resume_anim_tree(&mut self, anim_id: Uuid, anims: &[Anim]) {
        self.resume_anim(anim_id);
        for cid in twg_shared::group_anim::collect_group_descendant_ids(anim_id, anims) {
            self.resume_anim(cid);
        }
    }

    /// Stops a single animation: resets buttons to Play, snaps marker to route start point, restores Idle.
    pub fn stop_anim(&mut self, anim_id: Uuid) {
        let entry = self.tracks.entry(anim_id).or_default();
        entry.status = AnimRunStatus::Stopped;
        entry.distance_m = 0.0;
        entry.has_pose_override = true;
        entry.trigger_seq = 0;
        entry.completed_cycles = 0;
        entry.group_launched_count = 0;
        entry.group_last_step_start_sec = 0.0;
        entry.group_prev_done_at_sec = None;
        entry.group_owned_children.clear();
        self.playing = self
            .tracks
            .values()
            .any(|t| t.status == AnimRunStatus::Playing);
    }

    /// Stops an animation and (if it is a `Grup` Combine/Sequence) stops/resets all descendant children.
    pub fn stop_anim_tree(&mut self, anim_id: Uuid, anims: &[Anim]) {
        let owned = self
            .tracks
            .get(&anim_id)
            .map(|t| t.group_owned_children.clone())
            .unwrap_or_default();
        self.stop_anim(anim_id);
        let mut to_stop = twg_shared::group_anim::collect_group_descendant_ids(anim_id, anims);
        for oid in owned {
            if !to_stop.contains(&oid) {
                to_stop.push(oid);
            }
        }
        for cid in to_stop {
            self.stop_anim(cid);
        }
    }

    pub fn play_all_routes(&mut self, anims: &[Anim]) {
        self.play_test_mode = true;
        for a in anims {
            let playable = if twg_shared::particle_anim::is_particle_anim(a) {
                !a.geom.is_empty()
            } else if twg_shared::shoot_anim::is_shoot_anim(a)
                || twg_shared::pointer_anim::is_pointer_arrow_anim(a)
            {
                a.marker_id.is_some()
            } else if twg_shared::spawn_hide_anim::is_spawn_hide_anim(a) {
                let cfg = twg_shared::spawn_hide_anim::parse_spawn_hide_config(a);
                cfg.include_in_play_all && !cfg.marker_ids.is_empty()
            } else if twg_shared::group_anim::is_group_anim(a) {
                let cfg = twg_shared::group_anim::parse_group_anim_config(a);
                cfg.include_in_play_all && !cfg.items.is_empty()
            } else {
                !twg_shared::route_formation::anim_bound_marker_ids(a).is_empty()
                    && a.geom.len() >= 2
            };
            if playable {
                self.play_anim(a.id);
            }
        }
    }

    pub fn stop_all_routes(&mut self, anims: &[Anim]) {
        for a in anims {
            self.stop_anim(a.id);
        }
        self.playing = false;
    }

    /// Exits Play Skenario mode completely and returns to the Scenario Editor panels.
    pub fn stop_play_test(&mut self) {
        self.play_test_mode = false;
        self.editor_preview_anim_id = None;
        self.playing = false;
        self.elapsed_sec = 0.0;
        self.tracks.clear();
        self.next_trigger_seq = 0;
        self.play_panel_search.clear();
    }

    /// Enters Play Mode and immediately plays a specific animation (e.g. from Layers quick Play).
    pub fn play_single_immediately(&mut self, anim_id: Uuid) {
        self.enter_play_mode();
        self.play_anim(anim_id);
    }
}

#[derive(Debug, Clone)]
pub struct CommandLogEntry {
    pub timestamp: String,
    pub actor: String,
    pub command: String,
    pub detail: String,
}

#[derive(Resource)]
pub struct AppState {
    pub screen: AppScreen,
    pub offline_preview: bool,
    pub server_url: String,
    pub server_settings: crate::settings::LocalServerSettings,
    pub server_settings_saved_locally: bool,
    pub show_server_settings: bool,
    pub testing_server_connection: bool,
    pub server_settings_feedback: Option<(bool, String)>,
    pub username: String,
    pub password: String,
    pub login_show_password: bool,
    pub login_error: Option<String>,
    pub status_banner: Option<String>,
    pub loading: bool,

    pub auth_token: Option<String>,
    pub refresh_token: Option<String>,
    pub current_user: Option<UserDto>,
    pub device_id: String,

    pub operasis: Vec<Operasi>,
    pub selected_operasi_id: Option<Uuid>,
    pub operasi_table: OperasiTableState,
    pub skenarios: Vec<Skenario>,
    pub skenario_table: SkenarioTableState,
    pub satker_table: SatkerTableState,
    pub user_table: UserTableState,
    pub profile_form: ProfileFormState,
    pub tactical_page: TacticalPageState,
    pub big_screen: BigScreenState,
    pub play_session_mode: PlaySessionMode,
    pub smoothed_fps: f32,
    pub location_presets: Vec<LocationPreset>,
    pub preset_manager: LocationPresetManagerState,
    pub active_skenario: Option<Skenario>,
    pub geo_anchor: GeoAnchor,

    pub markers: Vec<Marker>,
    pub buildings: Vec<Building>,
    pub lines: Vec<PlotLine>,
    pub text_markers: Vec<TextMarker>,
    pub particles: Vec<Particle>,
    pub alurs: Vec<Alur>,
    pub anims: Vec<Anim>,

    pub assets_root: PathBuf,
    pub available_model_paths: HashSet<String>,
    pub catalog: Vec<ModelCatalogEntry>,
    pub catalog_search: String,
    pub catalog_category: String,
    pub catalog_ready_only: bool,
    pub selected_catalog_id: Option<String>,
    pub dragging_catalog_entry: Option<ModelCatalogEntry>,
    pub dragging_basic_mesh: Option<BasicMeshShape>,
    pub catalog_drop_world_pos: Option<Vec3>,

    // Server-synced 3D model assets & Admin Assets Manager state
    pub assets_section: AssetsSection,
    pub model_assets: Vec<ModelAssetDto>,
    pub model_categories: Vec<ModelCategoryCount>,
    pub media_assets: Vec<MediaAssetDto>,
    pub sync_status: ModelSyncStatus,
    pub asset_search: String,
    pub asset_category_filter: String,
    pub asset_type_filter: String,
    pub selected_asset_id: Option<Uuid>,
    pub preview_lod: AssetPreviewLod,
    pub preview_variant: AssetPreviewVariant,
    pub preview_anim_playing: bool,
    pub preview_anim_speed: f32,
    pub preview_auto_rotate: bool,
    pub preview_reload_nonce: u64,
    pub asset_form_mode: AssetFormMode,
    pub asset_form: AssetFormState,
    pub category_form: CategoryFormState,

    // Media Assets (Texture, Sound, Video) UI state
    pub media_search: String,
    pub selected_media_id: Option<Uuid>,
    pub media_form_mode: MediaFormMode,
    pub media_form: MediaFormState,
    pub texture_preview_original: bool,
    pub texture_preview_tiled: bool,
    pub texture_preview_zoom: f32,
    pub media_reload_nonce: u64,

    pub tool_mode: ToolMode,
    pub left_tab: LeftTab,
    pub selected: SelectedEntity,
    pub layer_search: String,
    pub layer_vis_filter: LayerVisibilityFilter,
    pub layer_lock_filter: LayerLockFilter,
    pub hidden_entity_ids: HashSet<Uuid>,
    pub locked_entity_ids: HashSet<Uuid>,
    pub uniform_scale_lock: bool,

    pub show_unit_labels: bool,
    pub show_text_markers: bool,
    pub show_gizmos: bool,
    /// Trackpad-friendly navigation (2-finger scroll pans map, pinch/Cmd+scroll zooms, Alt+drag orbits).
    /// Defaults to `true` on macOS and `false` on Windows/Linux; toggleable anytime in Topbar / Play Panel.
    pub trackpad_mode: bool,
    pub active_base_layer: BaseLayer,
    pub active_map_mode: MapMode,
    pub active_skybox: SkyboxPreset,
    pub pending_delete_entity: Option<PendingDeleteEntity>,

    pub draft_points_lnglat: Vec<[f64; 2]>,
    pub measure_points_lnglat: Vec<[f64; 2]>,
    pub cursor_lnglat: Option<[f64; 2]>,

    pub online_operators: Vec<OnlineOperator>,
    pub ws_connected: bool,
    pub command_log: Vec<CommandLogEntry>,

    // Inspector & creation drafts
    pub marker_edit: Option<Marker>,
    pub building_edit: Option<Building>,
    pub line_edit: Option<PlotLine>,
    pub text_edit: Option<TextMarker>,
    pub anim_edit: Option<Anim>,
    pub anim_category_filter: AnimCategory,
    pub anim_list_filter: Option<AnimCategory>,
    pub picking_marker_for_anim_id: Option<Uuid>,
    pub picking_shoot_for_anim_id: Option<Uuid>,
    pub picking_pointer_for_anim_id: Option<Uuid>,
    pub picking_spawn_hide_for_anim_id: Option<Uuid>,
    pub route_target_marker_id: Option<Uuid>,
    pub new_route_is_multi: bool,
    pub new_route_formation: twg_shared::route_formation::RouteFormationKind,
    pub new_route_name: String,
    pub new_route_speed: f64,
    pub new_route_loop: bool,
    pub new_shoot_kind: twg_shared::shoot_anim::ShootAnimKind,
    pub new_shoot_name: String,
    pub shoot_preview_reset_nonce: u64,
    pub new_pointer_name: String,
    pub new_spawn_hide_action: twg_shared::spawn_hide_anim::SpawnHideAction,
    pub new_spawn_hide_name: String,
    pub spawn_hide_preview: Option<(Uuid, f32)>,
    pub new_group_mode: twg_shared::group_anim::GroupAnimMode,
    pub new_group_name: String,
    pub group_add_search: String,
    pub group_hovered_child_anim_id: Option<Uuid>,
    pub new_particle_kind: twg_shared::particle_anim::ParticleAnimKind,
    pub new_particle_name: String,
    pub particle_preview_reset_nonce: u64,
    pub formation_pad_selected_marker: Option<Uuid>,
    pub formation_pad_snap_m: f64,
    pub formation_random_seed: u64,
    pub new_text_label: String,
    pub new_building_height: f64,
    pub new_building_color: String,
    pub new_polygon_height: f64,
    pub new_line_color: String,

    // Scenario Editor dirty-tracking for toolbar "Simpan" batch save
    pub dirty_marker_ids: HashSet<Uuid>,
    pub dirty_building_ids: HashSet<Uuid>,
    pub dirty_line_ids: HashSet<Uuid>,
    pub dirty_text_ids: HashSet<Uuid>,
    pub dirty_anim_ids: HashSet<Uuid>,
    pub skenario_dirty: bool,
    pub saving_pending_count: usize,
}

impl AppState {
    pub fn new(assets_root: PathBuf) -> Self {
        let (server_settings, server_settings_saved_locally) =
            crate::settings::load_local_server_settings();
        let server_url = server_settings
            .build_url()
            .unwrap_or_else(|_| "http://127.0.0.1:8010".to_string());

        let mut available_model_paths = scan_existing_local_models(&assets_root);
        let cached_sync = load_cached_sync_manifest(&assets_root);
        let model_assets: Vec<ModelAssetDto> = cached_sync
            .as_ref()
            .map(|c| c.manifest.models.clone())
            .unwrap_or_default();
        let model_categories: Vec<ModelCategoryCount> = cached_sync
            .as_ref()
            .map(|c| c.manifest.categories.clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(default_model_category_counts);
        let media_assets: Vec<MediaAssetDto> = cached_sync
            .as_ref()
            .map(|c| c.manifest.media_assets.clone())
            .unwrap_or_default();

        let (catalog, catalog_avail) = if !model_assets.is_empty() {
            let cat: Vec<ModelCatalogEntry> = model_assets
                .iter()
                .map(|m| {
                    let ready = available_model_paths.contains(&m.model_file.rel_path);
                    m.to_catalog_entry(ready)
                })
                .collect();
            (cat, HashSet::new())
        } else {
            load_local_catalog(&assets_root)
        };
        available_model_paths.extend(catalog_avail);

        let selected_catalog_id = catalog
            .iter()
            .find(|c| c.ready)
            .or_else(|| catalog.first())
            .map(|c| c.id.clone());

        let selected_asset_id = model_assets.first().map(|m| m.id);
        let mut asset_form = AssetFormState::default();
        if let Some(first) = model_assets.first() {
            asset_form.load_from_asset(first);
        }

        let summary_text = if model_assets.is_empty() && media_assets.is_empty() {
            "Memeriksa sinkronisasi assets dengan server...".to_string()
        } else {
            format!(
                "Cache lokal: {} model 3D, {} media ({} file)",
                model_assets.len(),
                media_assets.len(),
                available_model_paths.len()
            )
        };

        Self {
            screen: AppScreen::Login,
            offline_preview: false,
            server_url,
            server_settings,
            server_settings_saved_locally,
            show_server_settings: false,
            testing_server_connection: false,
            server_settings_feedback: None,
            username: String::new(),
            password: String::new(),
            login_show_password: false,
            login_error: None,
            status_banner: None,
            loading: false,
            auth_token: None,
            refresh_token: None,
            current_user: None,
            device_id: format!("desktop-{}", Uuid::new_v4().simple()),
            operasis: Vec::new(),
            selected_operasi_id: None,
            operasi_table: OperasiTableState::default(),
            skenarios: Vec::new(),
            skenario_table: SkenarioTableState::default(),
            satker_table: SatkerTableState::default(),
            user_table: UserTableState::default(),
            profile_form: ProfileFormState::default(),
            tactical_page: TacticalPageState::default(),
            big_screen: BigScreenState::default(),
            play_session_mode: PlaySessionMode::LocalPreview,
            smoothed_fps: 60.0,
            location_presets: default_location_presets(),
            preset_manager: LocationPresetManagerState::default(),
            active_skenario: None,
            geo_anchor: GeoAnchor::default(),
            markers: Vec::new(),
            buildings: Vec::new(),
            lines: Vec::new(),
            text_markers: Vec::new(),
            particles: Vec::new(),
            alurs: Vec::new(),
            anims: Vec::new(),
            assets_root,
            available_model_paths,
            catalog,
            catalog_search: String::new(),
            catalog_category: "all".to_string(),
            catalog_ready_only: true,
            selected_catalog_id,
            dragging_catalog_entry: None,
            dragging_basic_mesh: None,
            catalog_drop_world_pos: None,
            assets_section: AssetsSection::Models3d,
            model_assets,
            model_categories,
            media_assets,
            sync_status: ModelSyncStatus {
                summary_text,
                ..default()
            },
            asset_search: String::new(),
            asset_category_filter: "all".to_string(),
            asset_type_filter: "all".to_string(),
            selected_asset_id,
            preview_lod: AssetPreviewLod::Lod0,
            preview_variant: AssetPreviewVariant::Idle,
            preview_anim_playing: true,
            preview_anim_speed: 1.0,
            preview_auto_rotate: true,
            preview_reload_nonce: 0,
            asset_form_mode: AssetFormMode::Inspect,
            asset_form,
            category_form: CategoryFormState::default(),
            media_search: String::new(),
            selected_media_id: None,
            media_form_mode: MediaFormMode::Inspect,
            media_form: MediaFormState::default(),
            texture_preview_original: false,
            texture_preview_tiled: false,
            texture_preview_zoom: 1.0,
            media_reload_nonce: 0,
            tool_mode: ToolMode::Select,
            left_tab: LeftTab::Layers,
            selected: SelectedEntity::None,
            layer_search: String::new(),
            layer_vis_filter: LayerVisibilityFilter::All,
            layer_lock_filter: LayerLockFilter::All,
            hidden_entity_ids: HashSet::new(),
            locked_entity_ids: HashSet::new(),
            uniform_scale_lock: true,
            show_unit_labels: false,
            show_text_markers: true,
            show_gizmos: true,
            trackpad_mode: cfg!(target_os = "macos"),
            active_base_layer: BaseLayer::Osm,
            active_map_mode: MapMode::Outdoor,
            active_skybox: SkyboxPreset::DayClear,
            pending_delete_entity: None,
            draft_points_lnglat: Vec::new(),
            measure_points_lnglat: Vec::new(),
            cursor_lnglat: None,
            online_operators: Vec::new(),
            ws_connected: false,
            command_log: Vec::new(),
            marker_edit: None,
            building_edit: None,
            line_edit: None,
            text_edit: None,
            anim_edit: None,
            anim_category_filter: AnimCategory::Route,
            anim_list_filter: None,
            picking_marker_for_anim_id: None,
            picking_shoot_for_anim_id: None,
            picking_pointer_for_anim_id: None,
            picking_spawn_hide_for_anim_id: None,
            route_target_marker_id: None,
            new_route_is_multi: false,
            new_route_formation: twg_shared::route_formation::RouteFormationKind::Grid,
            new_route_name: "Rute Unit 1".to_string(),
            new_route_speed: 5.0,
            new_route_loop: false,
            new_shoot_kind: twg_shared::shoot_anim::ShootAnimKind::WaterCannon,
            new_shoot_name: "Water Cannon 1".to_string(),
            shoot_preview_reset_nonce: 0,
            new_pointer_name: "Panah Penunjuk 1".to_string(),
            new_spawn_hide_action: twg_shared::spawn_hide_anim::SpawnHideAction::Spawn,
            new_spawn_hide_name: "Spawn Objek 1".to_string(),
            spawn_hide_preview: None,
            new_group_mode: twg_shared::group_anim::GroupAnimMode::Combine,
            new_group_name: "Combine 1".to_string(),
            group_add_search: String::new(),
            group_hovered_child_anim_id: None,
            new_particle_kind: twg_shared::particle_anim::ParticleAnimKind::Fire,
            new_particle_name: "Api 1".to_string(),
            particle_preview_reset_nonce: 0,
            formation_pad_selected_marker: None,
            formation_pad_snap_m: 0.5,
            formation_random_seed: 1,
            new_text_label: "TITIK TAKTIS".to_string(),
            new_building_height: 24.0,
            new_building_color: "#38bdf8".to_string(),
            new_polygon_height: 6.0,
            new_line_color: "#f59e0b".to_string(),
            dirty_marker_ids: HashSet::new(),
            dirty_building_ids: HashSet::new(),
            dirty_line_ids: HashSet::new(),
            dirty_text_ids: HashSet::new(),
            dirty_anim_ids: HashSet::new(),
            skenario_dirty: false,
            saving_pending_count: 0,
        }
    }

    pub fn unsaved_change_count(&self) -> usize {
        self.dirty_marker_ids.len()
            + self.dirty_building_ids.len()
            + self.dirty_line_ids.len()
            + self.dirty_text_ids.len()
            + self.dirty_anim_ids.len()
            + usize::from(self.skenario_dirty)
    }

    pub fn clear_all_dirty(&mut self) {
        self.dirty_marker_ids.clear();
        self.dirty_building_ids.clear();
        self.dirty_line_ids.clear();
        self.dirty_text_ids.clear();
        self.dirty_anim_ids.clear();
        self.skenario_dirty = false;
        self.saving_pending_count = 0;
    }

    /// Keeps `self.anims` consistently sorted by `(sort_order, created_at, id)`.
    pub fn sort_anims_by_order(&mut self) {
        self.anims.sort_by(|a, b| {
            a.sort_order
                .cmp(&b.sort_order)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    /// Moves an animation one step up or down in the active list (`filter = None` for global order,
    /// or `Some(cat)` for category-filtered view), renormalizes `sort_order = 1..=N`, marks modified
    /// animations dirty, and returns the IDs whose `sort_order` changed.
    pub fn move_anim_in_order(
        &mut self,
        anim_id: Uuid,
        move_up: bool,
        filter: Option<AnimCategory>,
    ) -> Vec<Uuid> {
        self.sort_anims_by_order();
        let matching_indices: Vec<usize> = self
            .anims
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                filter.is_none_or(|cat| AnimCategory::from_anim_type(&a.anim_type) == cat)
            })
            .map(|(i, _)| i)
            .collect();

        let Some(pos) = matching_indices
            .iter()
            .position(|&i| self.anims[i].id == anim_id)
        else {
            return Vec::new();
        };

        let target_pos = if move_up {
            if pos == 0 {
                return Vec::new();
            }
            pos - 1
        } else {
            if pos + 1 >= matching_indices.len() {
                return Vec::new();
            }
            pos + 1
        };

        let idx_a = matching_indices[pos];
        let idx_b = matching_indices[target_pos];
        self.anims.swap(idx_a, idx_b);

        let mut changed = Vec::new();
        for (i, a) in self.anims.iter_mut().enumerate() {
            let new_order = (i + 1) as i32;
            if a.sort_order != new_order {
                a.sort_order = new_order;
                self.dirty_anim_ids.insert(a.id);
                changed.push(a.id);
            }
        }
        if let Some(edit_id) = self.anim_edit.as_ref().map(|e| e.id)
            && let Some(fresh) = self.anims.iter().find(|a| a.id == edit_id).cloned()
        {
            self.anim_edit = Some(fresh);
        }
        changed
    }

    pub fn is_marker_locked(&self, m: &Marker) -> bool {
        m.metadata
            .get("locked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || self.locked_entity_ids.contains(&m.id)
    }

    pub fn set_marker_locked(&mut self, id: Uuid, locked: bool) {
        if locked {
            self.locked_entity_ids.insert(id);
        } else {
            self.locked_entity_ids.remove(&id);
        }
        if let Some(m) = self.markers.iter_mut().find(|x| x.id == id) {
            if let Some(obj) = m.metadata.as_object_mut() {
                obj.insert("locked".to_string(), serde_json::Value::Bool(locked));
            } else {
                m.metadata = serde_json::json!({ "locked": locked });
            }
            let updated = m.clone();
            if self.marker_edit.as_ref().is_some_and(|e| e.id == id) {
                self.marker_edit = Some(updated);
            }
            self.dirty_marker_ids.insert(id);
        }
    }

    pub fn is_entity_locked(&self, entity: SelectedEntity) -> bool {
        match entity {
            SelectedEntity::None => false,
            SelectedEntity::Marker(id) => self
                .markers
                .iter()
                .find(|m| m.id == id)
                .is_some_and(|m| self.is_marker_locked(m)),
            SelectedEntity::Building(id)
            | SelectedEntity::Line(id)
            | SelectedEntity::TextMarker(id)
            | SelectedEntity::Anim(id) => self.locked_entity_ids.contains(&id),
        }
    }

    pub fn request_delete_entity(&mut self, entity: SelectedEntity) {
        if entity == SelectedEntity::None || self.is_entity_locked(entity) {
            return;
        }
        let info = match entity {
            SelectedEntity::None => None,
            SelectedEntity::Marker(id) => self.markers.iter().find(|m| m.id == id).map(|m| {
                let kind = if is_basic_mesh_marker(m) {
                    "Basic Mesh"
                } else {
                    "Unit 3D"
                };
                (kind.to_string(), m.name.clone())
            }),
            SelectedEntity::Building(id) => self
                .buildings
                .iter()
                .find(|b| b.id == id)
                .map(|b| ("Building".to_string(), b.name.clone())),
            SelectedEntity::Line(id) => self
                .lines
                .iter()
                .find(|l| l.id == id)
                .map(|l| ("Plot Line".to_string(), l.name.clone())),
            SelectedEntity::TextMarker(id) => self
                .text_markers
                .iter()
                .find(|t| t.id == id)
                .map(|t| ("Text Label".to_string(), t.text.clone())),
            SelectedEntity::Anim(id) => self
                .anims
                .iter()
                .find(|a| a.id == id)
                .map(|a| {
                    let kind = if twg_shared::shoot_anim::is_shoot_anim(a) {
                        "Shoot Animation"
                    } else if twg_shared::particle_anim::is_particle_anim(a) {
                        "Particle Animation"
                    } else if twg_shared::pointer_anim::is_pointer_arrow_anim(a) {
                        "Panah Penunjuk 3D"
                    } else if let Some(act) = twg_shared::spawn_hide_anim::spawn_hide_action(a) {
                        match act {
                            twg_shared::spawn_hide_anim::SpawnHideAction::Spawn => "Animasi Spawn",
                            twg_shared::spawn_hide_anim::SpawnHideAction::Hide => "Animasi Hide",
                        }
                    } else if let Some(gm) = twg_shared::group_anim::group_anim_mode(a) {
                        match gm {
                            twg_shared::group_anim::GroupAnimMode::Combine => "Animasi Combine",
                            twg_shared::group_anim::GroupAnimMode::Sequence => "Animasi Sequence",
                        }
                    } else {
                        "Route Animation"
                    };
                    (kind.to_string(), a.name.clone())
                }),
        };
        if let Some((kind_label, name)) = info {
            self.pending_delete_entity = Some(PendingDeleteEntity {
                entity,
                kind_label,
                name,
            });
        }
    }

    pub fn matches_layer_filters(&self, visible: bool, locked: bool) -> bool {
        let vis_ok = match self.layer_vis_filter {
            LayerVisibilityFilter::All => true,
            LayerVisibilityFilter::VisibleOnly => visible,
            LayerVisibilityFilter::HiddenOnly => !visible,
        };
        let lock_ok = match self.layer_lock_filter {
            LayerLockFilter::All => true,
            LayerLockFilter::UnlockedOnly => !locked,
            LayerLockFilter::LockedOnly => locked,
        };
        vis_ok && lock_ok
    }

    /// Global administrators (`admin` and `admin_monitor`): can manage Assets 3D, Categories,
    /// Satkers, all Operasis, all Skenarios, and all Users.
    pub fn is_super_admin(&self) -> bool {
        self.current_user
            .as_ref()
            .is_some_and(|u| u.role.is_global_admin())
    }

    pub fn can_manage_satkers(&self) -> bool {
        self.current_user
            .as_ref()
            .is_some_and(|u| u.role.can_manage_satkers())
    }

    pub fn can_manage_users(&self) -> bool {
        self.current_user
            .as_ref()
            .is_some_and(|u| u.role.can_manage_users())
    }

    pub fn can_write_admin(&self) -> bool {
        self.current_user
            .as_ref()
            .is_some_and(|u| u.role.can_write_admin())
    }

    /// True whenever the 3D scenario world should be rendered (Plot Skenario or Layar Besar).
    pub fn is_3d_map_screen(&self) -> bool {
        matches!(self.screen, AppScreen::PlotSkenario | AppScreen::LayarBesar)
    }

    /// True when Play Skenario was opened from Tactical / Simulasi (realtime WS broadcast active).
    pub fn is_tactical_live(&self) -> bool {
        self.screen == AppScreen::PlotSkenario
            && self.play_session_mode == PlaySessionMode::TacticalLive
    }

    pub fn is_big_screen_only_role(&self) -> bool {
        self.current_user
            .as_ref()
            .is_some_and(|u| u.role.is_big_screen_only_role())
    }

    pub fn is_big_screen(&self) -> bool {
        self.screen == AppScreen::LayarBesar
    }

    /// True if the signed-in user may control Layar Besar (camera sync & load/reload scenario)
    /// for the given `Skenario`.
    pub fn can_control_skenario_big_screen(&self, sk: &Skenario) -> bool {
        self.current_user
            .as_ref()
            .is_some_and(|u| u.role.can_control_big_screen(sk.satker_id, u.satker_id))
    }

    /// True if the signed-in user may control Layar Besar for the currently active `Skenario`.
    pub fn can_control_active_big_screen(&self) -> bool {
        let Some(sk) = &self.active_skenario else {
            return false;
        };
        self.can_control_skenario_big_screen(sk)
    }

    /// Resolves the Play Skenario interaction behavior for a 3D marker based on its category
    /// record in `state.model_categories` (falling back to built-in category defaults offline).
    pub fn marker_play_behavior(&self, marker: &Marker) -> twg_shared::models::CategoryPlayBehavior {
        if crate::scene::basic_mesh::is_basic_mesh_marker(marker) {
            return twg_shared::models::CategoryPlayBehavior::StaticObstacle;
        }
        let cat_clean = marker.category.trim().to_lowercase();
        if let Some(cat) = self
            .model_categories
            .iter()
            .find(|c| c.category.eq_ignore_ascii_case(&cat_clean))
        {
            return cat.play_behavior;
        }
        twg_shared::models::CategoryPlayBehavior::default_for_slug(&cat_clean)
    }

    pub fn is_skenario_map_preview_active(&self) -> bool {
        self.screen == AppScreen::Skenarios
            && matches!(
                self.skenario_table.modal_mode,
                SkenarioModalMode::Create | SkenarioModalMode::Edit(_)
            )
    }

    pub fn restore_active_skenario_map_state(&mut self) {
        if let Some(sk) = &self.active_skenario {
            self.geo_anchor = GeoAnchor::new(sk.center_lng, sk.center_lat);
            self.active_base_layer = sk.base_layer;
            self.active_map_mode = sk.map_mode;
            self.active_skybox = sk.skybox;
        }
    }

    pub fn selected_asset(&self) -> Option<&ModelAssetDto> {
        let id = self.selected_asset_id?;
        self.model_assets.iter().find(|m| m.id == id)
    }

    pub fn select_asset(&mut self, id: Uuid) {
        self.selected_asset_id = Some(id);
        self.asset_form_mode = AssetFormMode::Inspect;
        if let Some(asset) = self.model_assets.iter().find(|m| m.id == id).cloned() {
            self.asset_form.load_from_asset(&asset);
            if asset.model_type == ModelAssetType::Static {
                self.preview_variant = AssetPreviewVariant::Idle;
            }
        }
    }

    pub fn category_supports_idle_walk(&self, slug: &str) -> bool {
        self.model_categories
            .iter()
            .find(|c| c.category == slug)
            .map(|c| c.supports_idle_walk)
            .unwrap_or_else(|| twg_shared::models::category_supports_idle_walk(slug))
    }

    pub fn refresh_category_counts(&mut self) {
        for cat in &mut self.model_categories {
            let mut total = 0usize;
            let mut static_count = 0usize;
            let mut animated_count = 0usize;
            for m in &self.model_assets {
                if m.category == cat.category {
                    total += 1;
                    match m.model_type {
                        ModelAssetType::Static => static_count += 1,
                        ModelAssetType::Animated => animated_count += 1,
                    }
                }
            }
            cat.total = total;
            cat.static_count = static_count;
            cat.animated_count = animated_count;
        }
    }

    pub fn selected_media(&self) -> Option<&MediaAssetDto> {
        let id = self.selected_media_id?;
        self.media_assets.iter().find(|m| m.id == id)
    }

    pub fn select_media(&mut self, id: Uuid) {
        self.selected_media_id = Some(id);
        self.media_form_mode = MediaFormMode::Inspect;
        self.texture_preview_original = false;
        if let Some(m) = self.media_assets.iter().find(|x| x.id == id).cloned() {
            self.media_form.load_from_media(&m);
        }
    }

    pub fn ensure_media_selection_for_section(&mut self) {
        let Some(mt) = self.assets_section.media_type() else {
            return;
        };
        let valid = self
            .selected_media()
            .is_some_and(|m| m.media_type == mt);
        if !valid {
            if let Some(first_id) = self
                .media_assets
                .iter()
                .find(|m| m.media_type == mt)
                .map(|m| m.id)
            {
                self.select_media(first_id);
            } else {
                self.selected_media_id = None;
            }
        }
    }

    pub fn apply_manifest(&mut self, manifest: ModelSyncManifest) {
        self.model_assets = manifest.models;
        self.media_assets = manifest.media_assets;
        if !manifest.categories.is_empty() {
            self.model_categories = manifest.categories;
        }
        self.refresh_category_counts();
        self.rebuild_catalog_from_assets();
        self.ensure_media_selection_for_section();

        if self.asset_category_filter != "all"
            && !self
                .model_categories
                .iter()
                .any(|c| c.category == self.asset_category_filter)
        {
            self.asset_category_filter = "all".to_string();
        }
        if self.catalog_category != "all"
            && !self
                .model_categories
                .iter()
                .any(|c| c.category == self.catalog_category)
        {
            self.catalog_category = "all".to_string();
        }

        // Keep selection valid or pick the first available model
        if let Some(sel) = self.selected_asset_id {
            if !self.model_assets.iter().any(|m| m.id == sel) {
                self.selected_asset_id = self.model_assets.first().map(|m| m.id);
            }
        } else {
            self.selected_asset_id = self.model_assets.first().map(|m| m.id);
        }

        if self.asset_form_mode == AssetFormMode::Inspect
            && let Some(sel_id) = self.selected_asset_id
            && self.asset_form.editing_id != Some(sel_id)
            && let Some(asset) = self.model_assets.iter().find(|m| m.id == sel_id).cloned()
        {
            self.asset_form.load_from_asset(&asset);
        }
    }

    pub fn rebuild_catalog_from_assets(&mut self) {
        self.catalog = self
            .model_assets
            .iter()
            .map(|m| {
                let ready = self.available_model_paths.contains(&m.model_file.rel_path)
                    || self.assets_root.join(&m.model_file.rel_path).exists();
                m.to_catalog_entry(ready)
            })
            .collect();
        if self.selected_catalog_id.is_none() {
            self.selected_catalog_id = self
                .catalog
                .iter()
                .find(|c| c.ready)
                .or_else(|| self.catalog.first())
                .map(|c| c.id.clone());
        }
    }

    pub fn push_log(&mut self, actor: impl Into<String>, command: impl Into<String>, detail: impl Into<String>) {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        self.command_log.insert(
            0,
            CommandLogEntry {
                timestamp: ts,
                actor: actor.into(),
                command: command.into(),
                detail: detail.into(),
            },
        );
        if self.command_log.len() > 100 {
            self.command_log.truncate(100);
        }
    }

    pub fn push_ws_broadcast(&mut self, msg: &TacticalCommandBroadcast) {
        self.push_log(
            msg.actor_username.clone(),
            msg.command.clone(),
            msg.payload.to_string(),
        );
    }

    #[allow(dead_code)]
    pub fn enter_offline_demo(&mut self) {
        use chrono::Utc;
        let now = Utc::now();
        let op_id = Uuid::new_v4();
        let sk_id = Uuid::new_v4();

        self.offline_preview = true;
        self.screen = AppScreen::PlotSkenario;
        self.login_error = None;
        self.status_banner = Some("Offline 3D Preview Mode (Denpasar OSM / ArcGIS + glTF)".into());
        self.geo_anchor = GeoAnchor::new(115.2167, -8.6705);
        self.active_base_layer = BaseLayer::Osm;
        self.active_map_mode = MapMode::Outdoor;

        let op = Operasi {
            id: op_id,
            name: "Operasi Demo Denpasar (Offline)".into(),
            description: Some("Mode pratinjau offline untuk menguji Plot Skenario 3D & Play Test".into()),
            satker_id: None,
            satker_name: Some("Polda Bali (Offline)".into()),
            skenario_count: 1,
            created_at: now,
            updated_at: now,
        };
        self.operasis = vec![op.clone()];
        self.selected_operasi_id = Some(op_id);
        self.operasi_table.total = 1;
        self.operasi_table.page = 1;
        self.skenario_table.operasi = Some(op.clone());
        self.skenario_table.filter_operasi_id = Some(op_id);
        self.skenario_table.operasi_options = vec![op];
        self.skenario_table.total = 1;
        self.skenario_table.page = 1;

        let sk = Skenario {
            id: sk_id,
            operasi_id: op_id,
            operasi_name: Some("Operasi Demo Denpasar (Offline)".into()),
            satker_id: None,
            name: "Offline Demo Denpasar 3D".into(),
            center_lng: 115.2167,
            center_lat: -8.6705,
            zoom: 16.5,
            min_zoom: 3.0,
            max_zoom: 26.0,
            pitch: 48.0,
            bearing: 0.0,
            map_mode: MapMode::Outdoor,
            base_layer: BaseLayer::Osm,
            skybox: SkyboxPreset::DayClear,
            indoor_config: Default::default(),
            marker_count: 3,
            building_count: 1,
            line_count: 1,
            text_count: 0,
            particle_count: 0,
            alur_count: 0,
            anim_count: 1,
            created_at: now,
            updated_at: now,
        };
        self.active_skenario = Some(sk.clone());
        self.skenarios = vec![sk];

        let patwal_id = Uuid::new_v4();
        self.markers = vec![
            Marker {
                id: patwal_id,
                operasi_id: op_id,
                skenario_id: sk_id,
                unit_id: "patwal-demo".into(),
                name: "Mobil Patwal 01".into(),
                category: "kendaraan".into(),
                model_path: "models/kendaraans/mobil_patwal_anim_1.glb".into(),
                quantity: 1,
                lng: 115.2160,
                lat: -8.6710,
                altitude: 0.0,
                rot_x: 0.0,
                rot_y: 0.0,
                rot_z: 0.0,
                scale_x: 4.0,
                scale_y: 4.0,
                scale_z: 4.0,
                is_spawn: true,
                description: Some("Demo unit patwal".into()),
                metadata: serde_json::json!({}),
                created_at: now,
                updated_at: now,
            },
            Marker {
                id: Uuid::new_v4(),
                operasi_id: op_id,
                skenario_id: sk_id,
                unit_id: "heli-demo".into(),
                name: "Heli Polisi 01".into(),
                category: "kendaraan".into(),
                model_path: "models/kendaraans/helikopter_polisi_anim_1.glb".into(),
                quantity: 1,
                lng: 115.2175,
                lat: -8.6698,
                altitude: 45.0,
                rot_x: 0.0,
                rot_y: -35.0,
                rot_z: 0.0,
                scale_x: 3.5,
                scale_y: 3.5,
                scale_z: 3.5,
                is_spawn: true,
                description: Some("Demo heli pengawas".into()),
                metadata: serde_json::json!({}),
                created_at: now,
                updated_at: now,
            },
            Marker {
                id: Uuid::new_v4(),
                operasi_id: op_id,
                skenario_id: sk_id,
                unit_id: "brimob-demo".into(),
                name: "Regu Brimob 01".into(),
                category: "unit".into(),
                model_path: "models/units/brimob_walk.glb".into(),
                quantity: 1,
                lng: 115.2168,
                lat: -8.6707,
                altitude: 0.0,
                rot_x: 0.0,
                rot_y: 0.0,
                rot_z: 0.0,
                scale_x: 5.0,
                scale_y: 5.0,
                scale_z: 5.0,
                is_spawn: true,
                description: None,
                metadata: serde_json::json!({}),
                created_at: now,
                updated_at: now,
            },
        ];

        self.buildings = vec![Building {
            id: Uuid::new_v4(),
            operasi_id: op_id,
            skenario_id: sk_id,
            name: "Pos Komando Utama".into(),
            description: None,
            color: "#38bdf8".into(),
            height: 28.0,
            geom: vec![
                [115.2171, -8.6702],
                [115.2175, -8.6702],
                [115.2175, -8.6706],
                [115.2171, -8.6706],
            ],
            created_at: now,
            updated_at: now,
        }];

        self.lines = vec![PlotLine {
            id: Uuid::new_v4(),
            operasi_id: op_id,
            skenario_id: sk_id,
            name: "Ring Pengamanan Zona 1".into(),
            color: "#f59e0b".into(),
            width: 3.0,
            alpha: 0.95,
            geom: vec![
                [115.2153, -8.6696],
                [115.2180, -8.6696],
                [115.2180, -8.6714],
                [115.2153, -8.6714],
                [115.2153, -8.6696],
            ],
            created_at: now,
            updated_at: now,
        }];

        self.text_markers = vec![TextMarker {
            id: Uuid::new_v4(),
            operasi_id: op_id,
            skenario_id: sk_id,
            text: "POSKO UTAMA SEKTOR A".into(),
            color: "#38bdf8".into(),
            font_size: 15.0,
            lng: 115.2173,
            lat: -8.6704,
            altitude: 34.0,
            created_at: now,
            updated_at: now,
        }];

        self.anims = vec![Anim {
            id: Uuid::new_v4(),
            operasi_id: op_id,
            skenario_id: sk_id,
            marker_id: Some(patwal_id),
            name: "Patroli Keliling Sektor 1".into(),
            anim_type: "route".into(),
            sort_order: 1,
            speed: 18.0,
            duration_sec: 24.0,
            replay: true,
            autoplay: true,
            geom: vec![
                [115.2160, -8.6710, 0.0],
                [115.2177, -8.6710, 0.0],
                [115.2177, -8.6699, 0.0],
                [115.2160, -8.6699, 0.0],
                [115.2160, -8.6710, 0.0],
            ],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }];
    }
}

fn load_local_catalog(assets_root: &std::path::Path) -> (Vec<ModelCatalogEntry>, HashSet<String>) {
    let catalog_path = assets_root.join("catalog").join("models.json");
    let catalog: Vec<ModelCatalogEntry> = std::fs::read_to_string(&catalog_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    let mut available = HashSet::new();
    for entry in &catalog {
        if entry.ready && assets_root.join(&entry.model_path).exists() {
            available.insert(entry.model_path.clone());
        }
        if let Some(walk) = &entry.walk_model_path
            && assets_root.join(walk).exists()
        {
            available.insert(walk.clone());
        }
    }
    (catalog, available)
}

pub const LOCATION_PRESETS: &[(&str, f64, f64, f64)] = &[
    ("Denpasar (Puputan)", 115.2167, -8.6705, 16.5),
    ("Jakarta (Monas)", 106.8272, -6.1754, 16.0),
    ("Surabaya (Tugu Pahlawan)", 112.7378, -7.2458, 16.0),
    ("IKN Nusantara", 116.7069, -0.9718, 15.5),
    ("Indoor Tactical Grid", 0.0, 0.0, 18.0),
];

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn dummy_anim(name: &str, anim_type: &str, sort_order: i32) -> Anim {
        let now = Utc::now();
        Anim {
            id: Uuid::new_v4(),
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: None,
            name: name.to_string(),
            anim_type: anim_type.to_string(),
            sort_order,
            speed: 5.0,
            duration_sec: 10.0,
            replay: false,
            autoplay: false,
            geom: vec![[115.21, -8.67, 0.0], [115.22, -8.67, 0.0]],
            data: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn move_anim_in_order_handles_global_and_filtered_reordering() {
        let mut st = AppState::new(PathBuf::from("."));
        let a1 = dummy_anim("Rute A", "route", 1);
        let a2 = dummy_anim("Shoot B", "shoot", 2);
        let a3 = dummy_anim("Rute C", "route", 3);
        let (id1, id2, id3) = (a1.id, a2.id, a3.id);
        st.anims = vec![a3, a1, a2];

        // Global move up: Rute C (pos 3) swaps with Shoot B (pos 2)
        let changed = st.move_anim_in_order(id3, true, None);
        assert!(!changed.is_empty());
        assert_eq!(
            st.anims.iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![id1, id3, id2]
        );
        assert_eq!(
            st.anims.iter().map(|a| a.sort_order).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        // Filtered move up inside Route only: Rute C swaps with Rute A
        let changed_route = st.move_anim_in_order(id3, true, Some(AnimCategory::Route));
        assert!(!changed_route.is_empty());
        assert_eq!(
            st.anims.iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![id3, id1, id2]
        );
        assert_eq!(
            st.anims.iter().map(|a| a.sort_order).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(st.dirty_anim_ids.contains(&id3));
        assert!(st.dirty_anim_ids.contains(&id1));

        // Boundary move up at top does nothing
        assert!(st.move_anim_in_order(id3, true, None).is_empty());
    }
}
