use std::path::PathBuf;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use reqwest::{Client, multipart};
use serde::de::DeserializeOwned;
use twg_shared::{
    api::{
        ApiErrorBody, ApiResponse, ChangePasswordRequest, CopySkenarioRequest, CreateAnimRequest,
        CreateBuildingRequest, CreateLocationPresetRequest, CreateMarkerRequest,
        CreateModelCategoryRequest, CreateOperasiRequest, CreatePlotLineRequest,
        CreateSatkerRequest, CreateSkenarioRequest, CreateTextMarkerRequest, CreateUserRequest,
        LoginRequest, LoginResponse, MoveSkenarioRequest, PaginatedResponse, ProfileUpdateRequest,
        ResizeTextureRequest,
        UpdateAnimRequest, UpdateBuildingRequest, UpdateLocationPresetRequest, UpdateMarkerRequest,
        UpdateModelCategoryRequest, UpdateOperasiRequest, UpdatePlotLineRequest,
        UpdateSatkerRequest, UpdateSkenarioRequest, UpdateTextMarkerRequest, UpdateUserRequest,
    },
    models::{
        Anim, Building, LocationPreset, Marker, MediaAssetDto, MediaAssetType, ModelAssetDto,
        ModelAssetType, ModelCategoryDto, Operasi, PlotLine, Satker, Skenario, SkenarioBundle,
        TacticalOperasiCard, TextMarker, UserDto,
    },
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MediaAssetUploadPayload {
    pub name: String,
    pub media_type: MediaAssetType,
    pub category: String,
    /// "crop" (center-crop to min(w,h)) or "resize" (fit to min(w,h)) for textures
    pub crop_mode: String,
    pub file_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ModelAssetUploadPayload {
    pub name: String,
    pub category: String,
    pub model_type: ModelAssetType,
    pub default_height: f64,
    pub default_rot_x: f64,
    pub default_rot_y: f64,
    pub default_rot_z: f64,
    /// For Static models
    pub model_file_path: Option<PathBuf>,
    /// For Animated models (idle without animation)
    pub idle_file_path: Option<PathBuf>,
    /// For Animated models (walk with walk animation)
    pub walk_file_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub server_url: String,
    pub access_token: String,
    pub device_id: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ApiCommand {
    TestServerConnection {
        server_url: String,
    },
    Login {
        server_url: String,
        username: String,
        password: String,
        device_id: String,
    },
    UpdateProfile {
        auth: AuthCredentials,
        req: ProfileUpdateRequest,
    },
    ChangePassword {
        auth: AuthCredentials,
        req: ChangePasswordRequest,
    },
    LoadOperasis {
        auth: AuthCredentials,
        page: i64,
        per_page: i64,
        search: Option<String>,
    },
    LoadOperasiOptions {
        auth: AuthCredentials,
    },
    CreateOperasi {
        auth: AuthCredentials,
        req: CreateOperasiRequest,
    },
    UpdateOperasi {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateOperasiRequest,
    },
    DeleteOperasi {
        auth: AuthCredentials,
        id: Uuid,
    },
    LoadSatkers {
        auth: AuthCredentials,
    },
    CreateSatker {
        auth: AuthCredentials,
        req: CreateSatkerRequest,
    },
    UpdateSatker {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateSatkerRequest,
    },
    DeleteSatker {
        auth: AuthCredentials,
        id: Uuid,
    },
    LoadUsers {
        auth: AuthCredentials,
    },
    CreateUser {
        auth: AuthCredentials,
        req: CreateUserRequest,
    },
    UpdateUser {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateUserRequest,
    },
    DeleteUser {
        auth: AuthCredentials,
        id: Uuid,
    },
    LoadTacticalOperasis {
        auth: AuthCredentials,
        page: i64,
        per_page: i64,
        search: Option<String>,
    },
    LoadOperasiSkenariosForTactical {
        auth: AuthCredentials,
        operasi_id: Uuid,
    },
    LoadLocationPresets {
        auth: AuthCredentials,
    },
    CreateLocationPreset {
        auth: AuthCredentials,
        req: CreateLocationPresetRequest,
    },
    UpdateLocationPreset {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateLocationPresetRequest,
    },
    DeleteLocationPreset {
        auth: AuthCredentials,
        id: Uuid,
    },
    LoadSkenarios {
        auth: AuthCredentials,
        operasi_id: Option<Uuid>,
        page: i64,
        per_page: i64,
        search: Option<String>,
    },
    CreateSkenario {
        auth: AuthCredentials,
        req: CreateSkenarioRequest,
    },
    UpdateSkenario {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateSkenarioRequest,
    },
    CopySkenario {
        auth: AuthCredentials,
        id: Uuid,
        req: CopySkenarioRequest,
    },
    MoveSkenario {
        auth: AuthCredentials,
        id: Uuid,
        req: MoveSkenarioRequest,
    },
    DeleteSkenario {
        auth: AuthCredentials,
        id: Uuid,
    },
    LoadBundle {
        auth: AuthCredentials,
        skenario_id: Uuid,
    },
    CreateMarker {
        auth: AuthCredentials,
        req: CreateMarkerRequest,
    },
    UpdateMarker {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateMarkerRequest,
    },
    DeleteMarker {
        auth: AuthCredentials,
        id: Uuid,
    },
    CreateBuilding {
        auth: AuthCredentials,
        req: CreateBuildingRequest,
    },
    UpdateBuilding {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateBuildingRequest,
    },
    DeleteBuilding {
        auth: AuthCredentials,
        id: Uuid,
    },
    CreateLine {
        auth: AuthCredentials,
        req: CreatePlotLineRequest,
    },
    UpdateLine {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdatePlotLineRequest,
    },
    DeleteLine {
        auth: AuthCredentials,
        id: Uuid,
    },
    CreateTextMarker {
        auth: AuthCredentials,
        req: CreateTextMarkerRequest,
    },
    UpdateTextMarker {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateTextMarkerRequest,
    },
    DeleteTextMarker {
        auth: AuthCredentials,
        id: Uuid,
    },
    CreateAnim {
        auth: AuthCredentials,
        req: CreateAnimRequest,
    },
    UpdateAnim {
        auth: AuthCredentials,
        id: Uuid,
        req: UpdateAnimRequest,
    },
    DeleteAnim {
        auth: AuthCredentials,
        id: Uuid,
    },
    CreateModelAsset {
        auth: AuthCredentials,
        payload: ModelAssetUploadPayload,
    },
    UpdateModelAsset {
        auth: AuthCredentials,
        id: Uuid,
        payload: ModelAssetUploadPayload,
    },
    DeleteModelAsset {
        auth: AuthCredentials,
        id: Uuid,
    },
    UploadModelAction {
        auth: AuthCredentials,
        model_id: Uuid,
        name: String,
        file_path: PathBuf,
    },
    DeleteModelAction {
        auth: AuthCredentials,
        model_id: Uuid,
        action_slug: String,
    },
    CreateModelCategory {
        auth: AuthCredentials,
        req: CreateModelCategoryRequest,
    },
    UpdateModelCategory {
        auth: AuthCredentials,
        slug: String,
        req: UpdateModelCategoryRequest,
    },
    DeleteModelCategory {
        auth: AuthCredentials,
        slug: String,
    },
    CreateMediaAsset {
        auth: AuthCredentials,
        payload: MediaAssetUploadPayload,
    },
    UpdateMediaAsset {
        auth: AuthCredentials,
        id: Uuid,
        payload: MediaAssetUploadPayload,
    },
    ResizeMediaTexture {
        auth: AuthCredentials,
        id: Uuid,
        target_size: u32,
    },
    RestoreMediaTextureOriginal {
        auth: AuthCredentials,
        id: Uuid,
    },
    DeleteMediaAsset {
        auth: AuthCredentials,
        id: Uuid,
    },
}

#[derive(Debug)]
pub enum ApiEvent {
    ServerTestResult {
        ok: bool,
        message: String,
    },
    LoginOk(LoginResponse),
    LoginErr(String),
    ProfileUpdated(UserDto),
    ProfileError(String),
    PasswordChanged,
    PasswordError(String),
    OperasisLoaded(PaginatedResponse<Operasi>),
    OperasiOptionsLoaded(Vec<Operasi>),
    OperasiCreated(Operasi),
    OperasiUpdated(Operasi),
    OperasiDeleted(Uuid),
    OperasiError(String),
    SatkersLoaded(Vec<Satker>),
    SatkerCreated(Satker),
    SatkerUpdated(Satker),
    SatkerDeleted(Uuid),
    SatkerError(String),
    UsersLoaded(Vec<UserDto>),
    UserCreated(UserDto),
    UserUpdated(UserDto),
    UserDeleted(Uuid),
    UserError(String),
    TacticalOperasisLoaded(PaginatedResponse<TacticalOperasiCard>),
    TacticalOperasiSkenariosLoaded {
        operasi_id: Uuid,
        items: Vec<Skenario>,
    },
    TacticalError(String),
    LocationPresetsLoaded(Vec<LocationPreset>),
    LocationPresetCreated(LocationPreset),
    LocationPresetUpdated(LocationPreset),
    LocationPresetDeleted(Uuid),
    LocationPresetError(String),
    SkenariosLoaded {
        operasi_id: Option<Uuid>,
        page: PaginatedResponse<Skenario>,
    },
    SkenarioCreated(Skenario),
    SkenarioUpdated(Skenario),
    SkenarioCopied(Skenario),
    SkenarioMoved(Skenario),
    SkenarioDeleted(Uuid),
    SkenarioError(String),
    BundleLoaded(Box<SkenarioBundle>),
    MarkerCreated(Marker),
    MarkerUpdated(Marker),
    MarkerDeleted(Uuid),
    BuildingCreated(Building),
    BuildingUpdated(Building),
    BuildingDeleted(Uuid),
    LineCreated(PlotLine),
    LineUpdated(PlotLine),
    LineDeleted(Uuid),
    TextMarkerCreated(TextMarker),
    TextMarkerUpdated(TextMarker),
    TextMarkerDeleted(Uuid),
    AnimCreated(Anim),
    AnimUpdated(Anim),
    AnimDeleted(Uuid),
    ModelAssetCreated(ModelAssetDto),
    ModelAssetUpdated(ModelAssetDto),
    ModelAssetDeleted(Uuid),
    ModelAssetError(String),
    ModelCategoryCreated(ModelCategoryDto),
    ModelCategoryUpdated(ModelCategoryDto),
    ModelCategoryDeleted(String),
    ModelCategoryError(String),
    MediaAssetCreated(MediaAssetDto),
    MediaAssetUpdated(MediaAssetDto),
    MediaAssetDeleted(Uuid),
    MediaAssetError(String),
    Error(String),
}

#[derive(Resource)]
pub struct ApiClientBridge {
    cmd_tx: Sender<(bool, ApiCommand)>,
    evt_rx: Receiver<(bool, ApiEvent)>,
}

impl ApiClientBridge {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<(bool, ApiCommand)>();
        let (evt_tx, evt_rx) = crossbeam_channel::unbounded::<(bool, ApiEvent)>();

        std::thread::Builder::new()
            .name("twg-http-worker".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("failed to build http tokio runtime");

                let client = Client::builder()
                    .timeout(std::time::Duration::from_secs(120))
                    .build()
                    .expect("failed to build reqwest client");

                rt.block_on(async move {
                    while let Ok((plot_save, cmd)) = cmd_rx.recv() {
                        let c = client.clone();
                        let tx = evt_tx.clone();
                        tokio::spawn(async move {
                            let evt = handle_command(&c, cmd).await;
                            let _ = tx.send((plot_save, evt));
                        });
                    }
                });
            })
            .expect("failed to spawn http worker thread");

        Self { cmd_tx, evt_rx }
    }

    pub fn send(&self, cmd: ApiCommand) {
        let _ = self.cmd_tx.send((false, cmd));
    }

    pub fn send_plot_save(&self, cmd: ApiCommand) {
        let _ = self.cmd_tx.send((true, cmd));
    }

    pub fn drain_events(&self) -> Vec<(bool, ApiEvent)> {
        self.evt_rx.try_iter().collect()
    }
}

async fn test_server_connection(client: &Client, server_url: &str) -> ApiEvent {
    let base_url = server_url.trim_end_matches('/');
    let ready_url = format!("{base_url}/readyz");
    let timeout = std::time::Duration::from_secs(8);
    let started = std::time::Instant::now();

    match client.get(&ready_url).timeout(timeout).send().await {
        Ok(resp) => {
            let status = resp.status();
            let elapsed_ms = started.elapsed().as_millis();
            if status.is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await
                    && json.get("status").and_then(|v| v.as_str()) == Some("ready")
                {
                    return ApiEvent::ServerTestResult {
                        ok: true,
                        message: format!(
                            "Berhasil terhubung ({elapsed_ms} ms) • Server & Database siap."
                        ),
                    };
                }
                return ApiEvent::ServerTestResult {
                    ok: false,
                    message: "Host merespons HTTP 200, tetapi bukan endpoint TWG Server yang valid."
                        .into(),
                };
            }

            // Fallback to /healthz if reverse proxy only exposes /healthz
            if status == reqwest::StatusCode::NOT_FOUND {
                let health_url = format!("{base_url}/healthz");
                let health_start = std::time::Instant::now();
                if let Ok(h_resp) = client.get(&health_url).timeout(timeout).send().await
                    && h_resp.status().is_success()
                    && let Ok(json) = h_resp.json::<serde_json::Value>().await
                    && json.get("status").and_then(|v| v.as_str()) == Some("ok")
                {
                    let ms = health_start.elapsed().as_millis();
                    return ApiEvent::ServerTestResult {
                        ok: true,
                        message: format!("Berhasil terhubung ({ms} ms) • TWG Server aktif."),
                    };
                }
            }

            ApiEvent::ServerTestResult {
                ok: false,
                message: format!(
                    "Server merespons HTTP {} ({elapsed_ms} ms), tetapi layanan belum siap.",
                    status.as_u16()
                ),
            }
        }
        Err(err) => {
            let detail = if err.is_timeout() {
                "Waktu koneksi habis (timeout 8 detik). Periksa host, port, atau firewall."
                    .to_string()
            } else if err.is_connect() {
                format!(
                    "Tidak dapat terhubung ke {base_url}. Pastikan server aktif serta protokol/port sesuai."
                )
            } else {
                format!("Gagal terhubung: {err}")
            };
            ApiEvent::ServerTestResult {
                ok: false,
                message: detail,
            }
        }
    }
}

async fn handle_command(client: &Client, cmd: ApiCommand) -> ApiEvent {
    match cmd {
        ApiCommand::TestServerConnection { server_url } => {
            test_server_connection(client, &server_url).await
        }
        ApiCommand::Login {
            server_url,
            username,
            password,
            device_id,
        } => {
            let url = format!("{}/api/v1/auth/login", server_url.trim_end_matches('/'));
            let body = LoginRequest {
                username,
                password,
                device_id: Some(device_id.clone()),
            };
            match client
                .post(&url)
                .header("X-Device-ID", device_id)
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => match parse_api_json::<LoginResponse>(resp).await {
                    Ok(login) => ApiEvent::LoginOk(login),
                    Err(err) => ApiEvent::LoginErr(err),
                },
                Err(err) => ApiEvent::LoginErr(format!("Connection failed: {err}")),
            }
        }
        ApiCommand::UpdateProfile { auth, req } => {
            let url = format!("{}/api/v1/profile", base(&auth));
            match authed_put::<_, UserDto>(client, &auth, &url, &req).await {
                Ok(u) => ApiEvent::ProfileUpdated(u),
                Err(e) => ApiEvent::ProfileError(e),
            }
        }
        ApiCommand::ChangePassword { auth, req } => {
            let url = format!("{}/api/v1/profile/password", base(&auth));
            match authed_put::<_, serde_json::Value>(client, &auth, &url, &req).await {
                Ok(_) => ApiEvent::PasswordChanged,
                Err(e) => ApiEvent::PasswordError(e),
            }
        }
        ApiCommand::LoadOperasis {
            auth,
            page,
            per_page,
            search,
        } => {
            let url = {
                let mut qs = url::form_urlencoded::Serializer::new(String::new());
                qs.append_pair("page", &page.max(1).to_string());
                qs.append_pair("per_page", &per_page.clamp(1, 200).to_string());
                if let Some(q) = search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    qs.append_pair("search", q);
                }
                format!("{}/api/v1/operasis?{}", base(&auth), qs.finish())
            };
            match authed_get::<PaginatedResponse<Operasi>>(client, &auth, &url).await {
                Ok(p) => ApiEvent::OperasisLoaded(p),
                Err(e) => ApiEvent::OperasiError(e),
            }
        }
        ApiCommand::LoadOperasiOptions { auth } => {
            let url = format!("{}/api/v1/operasis?page=1&per_page=200", base(&auth));
            match authed_get::<PaginatedResponse<Operasi>>(client, &auth, &url).await {
                Ok(p) => ApiEvent::OperasiOptionsLoaded(p.items),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateOperasi { auth, req } => {
            let url = format!("{}/api/v1/operasis", base(&auth));
            match authed_post::<_, Operasi>(client, &auth, &url, &req).await {
                Ok(op) => ApiEvent::OperasiCreated(op),
                Err(e) => ApiEvent::OperasiError(e),
            }
        }
        ApiCommand::UpdateOperasi { auth, id, req } => {
            let url = format!("{}/api/v1/operasis/{id}", base(&auth));
            match authed_put::<_, Operasi>(client, &auth, &url, &req).await {
                Ok(op) => ApiEvent::OperasiUpdated(op),
                Err(e) => ApiEvent::OperasiError(e),
            }
        }
        ApiCommand::DeleteOperasi { auth, id } => {
            let url = format!("{}/api/v1/operasis/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::OperasiDeleted(id),
                Err(e) => ApiEvent::OperasiError(e),
            }
        }
        ApiCommand::LoadSatkers { auth } => {
            let url = format!("{}/api/v1/satkers", base(&auth));
            match authed_get::<Vec<Satker>>(client, &auth, &url).await {
                Ok(items) => ApiEvent::SatkersLoaded(items),
                Err(e) => ApiEvent::SatkerError(e),
            }
        }
        ApiCommand::CreateSatker { auth, req } => {
            let url = format!("{}/api/v1/satkers", base(&auth));
            match authed_post::<_, Satker>(client, &auth, &url, &req).await {
                Ok(s) => ApiEvent::SatkerCreated(s),
                Err(e) => ApiEvent::SatkerError(e),
            }
        }
        ApiCommand::UpdateSatker { auth, id, req } => {
            let url = format!("{}/api/v1/satkers/{id}", base(&auth));
            match authed_put::<_, Satker>(client, &auth, &url, &req).await {
                Ok(s) => ApiEvent::SatkerUpdated(s),
                Err(e) => ApiEvent::SatkerError(e),
            }
        }
        ApiCommand::DeleteSatker { auth, id } => {
            let url = format!("{}/api/v1/satkers/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::SatkerDeleted(id),
                Err(e) => ApiEvent::SatkerError(e),
            }
        }
        ApiCommand::LoadUsers { auth } => {
            let url = format!("{}/api/v1/users", base(&auth));
            match authed_get::<Vec<UserDto>>(client, &auth, &url).await {
                Ok(items) => ApiEvent::UsersLoaded(items),
                Err(e) => ApiEvent::UserError(e),
            }
        }
        ApiCommand::CreateUser { auth, req } => {
            let url = format!("{}/api/v1/users", base(&auth));
            match authed_post::<_, UserDto>(client, &auth, &url, &req).await {
                Ok(u) => ApiEvent::UserCreated(u),
                Err(e) => ApiEvent::UserError(e),
            }
        }
        ApiCommand::UpdateUser { auth, id, req } => {
            let url = format!("{}/api/v1/users/{id}", base(&auth));
            match authed_put::<_, UserDto>(client, &auth, &url, &req).await {
                Ok(u) => ApiEvent::UserUpdated(u),
                Err(e) => ApiEvent::UserError(e),
            }
        }
        ApiCommand::DeleteUser { auth, id } => {
            let url = format!("{}/api/v1/users/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::UserDeleted(id),
                Err(e) => ApiEvent::UserError(e),
            }
        }
        ApiCommand::LoadTacticalOperasis {
            auth,
            page,
            per_page,
            search,
        } => {
            let url = {
                let mut qs = url::form_urlencoded::Serializer::new(String::new());
                qs.append_pair("page", &page.max(1).to_string());
                qs.append_pair("per_page", &per_page.clamp(1, 50).to_string());
                if let Some(q) = search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    qs.append_pair("search", q);
                }
                format!("{}/api/v1/tactical/operasis?{}", base(&auth), qs.finish())
            };
            match authed_get::<PaginatedResponse<TacticalOperasiCard>>(client, &auth, &url).await {
                Ok(p) => ApiEvent::TacticalOperasisLoaded(p),
                Err(e) => ApiEvent::TacticalError(e),
            }
        }
        ApiCommand::LoadOperasiSkenariosForTactical { auth, operasi_id } => {
            let url = format!(
                "{}/api/v1/operasis/{operasi_id}/skenarios?page=1&per_page=200",
                base(&auth)
            );
            match authed_get::<PaginatedResponse<Skenario>>(client, &auth, &url).await {
                Ok(p) => ApiEvent::TacticalOperasiSkenariosLoaded {
                    operasi_id,
                    items: p.items,
                },
                Err(e) => ApiEvent::TacticalError(e),
            }
        }
        ApiCommand::LoadLocationPresets { auth } => {
            let url = format!("{}/api/v1/location-presets", base(&auth));
            match authed_get::<Vec<LocationPreset>>(client, &auth, &url).await {
                Ok(items) => ApiEvent::LocationPresetsLoaded(items),
                Err(e) => ApiEvent::LocationPresetError(e),
            }
        }
        ApiCommand::CreateLocationPreset { auth, req } => {
            let url = format!("{}/api/v1/location-presets", base(&auth));
            match authed_post::<_, LocationPreset>(client, &auth, &url, &req).await {
                Ok(p) => ApiEvent::LocationPresetCreated(p),
                Err(e) => ApiEvent::LocationPresetError(e),
            }
        }
        ApiCommand::UpdateLocationPreset { auth, id, req } => {
            let url = format!("{}/api/v1/location-presets/{id}", base(&auth));
            match authed_put::<_, LocationPreset>(client, &auth, &url, &req).await {
                Ok(p) => ApiEvent::LocationPresetUpdated(p),
                Err(e) => ApiEvent::LocationPresetError(e),
            }
        }
        ApiCommand::DeleteLocationPreset { auth, id } => {
            let url = format!("{}/api/v1/location-presets/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::LocationPresetDeleted(id),
                Err(e) => ApiEvent::LocationPresetError(e),
            }
        }
        ApiCommand::LoadSkenarios {
            auth,
            operasi_id,
            page,
            per_page,
            search,
        } => {
            let url = {
                let mut qs = url::form_urlencoded::Serializer::new(String::new());
                qs.append_pair("page", &page.max(1).to_string());
                qs.append_pair("per_page", &per_page.clamp(1, 200).to_string());
                if let Some(op_id) = operasi_id {
                    qs.append_pair("operasi_id", &op_id.to_string());
                }
                if let Some(q) = search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    qs.append_pair("search", q);
                }
                format!("{}/api/v1/skenarios?{}", base(&auth), qs.finish())
            };
            match authed_get::<PaginatedResponse<Skenario>>(client, &auth, &url).await {
                Ok(page) => ApiEvent::SkenariosLoaded { operasi_id, page },
                Err(e) => ApiEvent::SkenarioError(e),
            }
        }
        ApiCommand::CreateSkenario { auth, req } => {
            let url = format!("{}/api/v1/skenarios", base(&auth));
            match authed_post::<_, Skenario>(client, &auth, &url, &req).await {
                Ok(sk) => ApiEvent::SkenarioCreated(sk),
                Err(e) => ApiEvent::SkenarioError(e),
            }
        }
        ApiCommand::UpdateSkenario { auth, id, req } => {
            let url = format!("{}/api/v1/skenarios/{id}", base(&auth));
            match authed_put::<_, Skenario>(client, &auth, &url, &req).await {
                Ok(sk) => ApiEvent::SkenarioUpdated(sk),
                Err(e) => ApiEvent::SkenarioError(e),
            }
        }
        ApiCommand::CopySkenario { auth, id, req } => {
            let url = format!("{}/api/v1/skenarios/{id}/copy", base(&auth));
            match authed_post::<_, Skenario>(client, &auth, &url, &req).await {
                Ok(sk) => ApiEvent::SkenarioCopied(sk),
                Err(e) => ApiEvent::SkenarioError(e),
            }
        }
        ApiCommand::MoveSkenario { auth, id, req } => {
            let url = format!("{}/api/v1/skenarios/{id}/move", base(&auth));
            match authed_post::<_, Skenario>(client, &auth, &url, &req).await {
                Ok(sk) => ApiEvent::SkenarioMoved(sk),
                Err(e) => ApiEvent::SkenarioError(e),
            }
        }
        ApiCommand::DeleteSkenario { auth, id } => {
            let url = format!("{}/api/v1/skenarios/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::SkenarioDeleted(id),
                Err(e) => ApiEvent::SkenarioError(e),
            }
        }
        ApiCommand::LoadBundle { auth, skenario_id } => {
            let url = format!("{}/api/v1/skenarios/{skenario_id}/bundle", base(&auth));
            match authed_get::<SkenarioBundle>(client, &auth, &url).await {
                Ok(b) => ApiEvent::BundleLoaded(Box::new(b)),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateMarker { auth, req } => {
            let url = format!("{}/api/v1/markers", base(&auth));
            match authed_post::<_, Marker>(client, &auth, &url, &req).await {
                Ok(m) => ApiEvent::MarkerCreated(m),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::UpdateMarker { auth, id, req } => {
            let url = format!("{}/api/v1/markers/{id}", base(&auth));
            match authed_put::<_, Marker>(client, &auth, &url, &req).await {
                Ok(m) => ApiEvent::MarkerUpdated(m),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::DeleteMarker { auth, id } => {
            let url = format!("{}/api/v1/markers/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::MarkerDeleted(id),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateBuilding { auth, req } => {
            let url = format!("{}/api/v1/buildings", base(&auth));
            match authed_post::<_, Building>(client, &auth, &url, &req).await {
                Ok(b) => ApiEvent::BuildingCreated(b),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::UpdateBuilding { auth, id, req } => {
            let url = format!("{}/api/v1/buildings/{id}", base(&auth));
            match authed_put::<_, Building>(client, &auth, &url, &req).await {
                Ok(b) => ApiEvent::BuildingUpdated(b),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::DeleteBuilding { auth, id } => {
            let url = format!("{}/api/v1/buildings/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::BuildingDeleted(id),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateLine { auth, req } => {
            let url = format!("{}/api/v1/lines", base(&auth));
            match authed_post::<_, PlotLine>(client, &auth, &url, &req).await {
                Ok(l) => ApiEvent::LineCreated(l),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::UpdateLine { auth, id, req } => {
            let url = format!("{}/api/v1/lines/{id}", base(&auth));
            match authed_put::<_, PlotLine>(client, &auth, &url, &req).await {
                Ok(l) => ApiEvent::LineUpdated(l),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::DeleteLine { auth, id } => {
            let url = format!("{}/api/v1/lines/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::LineDeleted(id),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateTextMarker { auth, req } => {
            let url = format!("{}/api/v1/text-markers", base(&auth));
            match authed_post::<_, TextMarker>(client, &auth, &url, &req).await {
                Ok(t) => ApiEvent::TextMarkerCreated(t),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::UpdateTextMarker { auth, id, req } => {
            let url = format!("{}/api/v1/text-markers/{id}", base(&auth));
            match authed_put::<_, TextMarker>(client, &auth, &url, &req).await {
                Ok(t) => ApiEvent::TextMarkerUpdated(t),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::DeleteTextMarker { auth, id } => {
            let url = format!("{}/api/v1/text-markers/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::TextMarkerDeleted(id),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateAnim { auth, req } => {
            let url = format!("{}/api/v1/anims", base(&auth));
            match authed_post::<_, Anim>(client, &auth, &url, &req).await {
                Ok(a) => ApiEvent::AnimCreated(a),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::UpdateAnim { auth, id, req } => {
            let url = format!("{}/api/v1/anims/{id}", base(&auth));
            match authed_put::<_, Anim>(client, &auth, &url, &req).await {
                Ok(a) => ApiEvent::AnimUpdated(a),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::DeleteAnim { auth, id } => {
            let url = format!("{}/api/v1/anims/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::AnimDeleted(id),
                Err(e) => ApiEvent::Error(e),
            }
        }
        ApiCommand::CreateModelAsset { auth, payload } => {
            let url = format!("{}/api/v1/assets/models", base(&auth));
            match build_model_multipart(payload).await {
                Ok(form) => {
                    match client
                        .post(&url)
                        .bearer_auth(&auth.access_token)
                        .header("X-Device-ID", &auth.device_id)
                        .multipart(form)
                        .send()
                        .await
                    {
                        Ok(resp) => match parse_api_json::<ModelAssetDto>(resp).await {
                            Ok(dto) => ApiEvent::ModelAssetCreated(dto),
                            Err(e) => ApiEvent::ModelAssetError(e),
                        },
                        Err(e) => ApiEvent::ModelAssetError(format!("Upload gagal: {e}")),
                    }
                }
                Err(e) => ApiEvent::ModelAssetError(e),
            }
        }
        ApiCommand::UpdateModelAsset { auth, id, payload } => {
            let url = format!("{}/api/v1/assets/models/{id}", base(&auth));
            match build_model_multipart(payload).await {
                Ok(form) => {
                    match client
                        .put(&url)
                        .bearer_auth(&auth.access_token)
                        .header("X-Device-ID", &auth.device_id)
                        .multipart(form)
                        .send()
                        .await
                    {
                        Ok(resp) => match parse_api_json::<ModelAssetDto>(resp).await {
                            Ok(dto) => ApiEvent::ModelAssetUpdated(dto),
                            Err(e) => ApiEvent::ModelAssetError(e),
                        },
                        Err(e) => ApiEvent::ModelAssetError(format!("Update gagal: {e}")),
                    }
                }
                Err(e) => ApiEvent::ModelAssetError(e),
            }
        }
        ApiCommand::DeleteModelAsset { auth, id } => {
            let url = format!("{}/api/v1/assets/models/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::ModelAssetDeleted(id),
                Err(e) => ApiEvent::ModelAssetError(e),
            }
        }
        ApiCommand::UploadModelAction {
            auth,
            model_id,
            name,
            file_path,
        } => {
            let url = format!("{}/api/v1/assets/models/{model_id}/actions", base(&auth));
            match tokio::fs::read(&file_path).await {
                Ok(bytes) => {
                    let fname = file_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("action.glb")
                        .to_string();
                    let part = multipart::Part::bytes(bytes).file_name(fname);
                    let form = multipart::Form::new()
                        .text("name", name)
                        .part("action_file", part);
                    match client
                        .post(&url)
                        .bearer_auth(&auth.access_token)
                        .header("X-Device-ID", &auth.device_id)
                        .multipart(form)
                        .send()
                        .await
                    {
                        Ok(resp) => match parse_api_json::<ModelAssetDto>(resp).await {
                            Ok(dto) => ApiEvent::ModelAssetUpdated(dto),
                            Err(e) => ApiEvent::ModelAssetError(e),
                        },
                        Err(e) => ApiEvent::ModelAssetError(format!("Upload aksi gagal: {e}")),
                    }
                }
                Err(e) => {
                    ApiEvent::ModelAssetError(format!("Gagal membaca file aksi: {e}"))
                }
            }
        }
        ApiCommand::DeleteModelAction {
            auth,
            model_id,
            action_slug,
        } => {
            let url = format!(
                "{}/api/v1/assets/models/{model_id}/actions/{action_slug}",
                base(&auth)
            );
            match client
                .delete(&url)
                .bearer_auth(&auth.access_token)
                .header("X-Device-ID", &auth.device_id)
                .send()
                .await
            {
                Ok(resp) => match parse_api_json::<ModelAssetDto>(resp).await {
                    Ok(dto) => ApiEvent::ModelAssetUpdated(dto),
                    Err(e) => ApiEvent::ModelAssetError(e),
                },
                Err(e) => ApiEvent::ModelAssetError(format!("Hapus aksi gagal: {e}")),
            }
        }
        ApiCommand::CreateModelCategory { auth, req } => {
            let url = format!("{}/api/v1/assets/categories", base(&auth));
            match authed_post::<_, ModelCategoryDto>(client, &auth, &url, &req).await {
                Ok(dto) => ApiEvent::ModelCategoryCreated(dto),
                Err(e) => ApiEvent::ModelCategoryError(e),
            }
        }
        ApiCommand::UpdateModelCategory { auth, slug, req } => {
            let url = format!("{}/api/v1/assets/categories/{slug}", base(&auth));
            match authed_put::<_, ModelCategoryDto>(client, &auth, &url, &req).await {
                Ok(dto) => ApiEvent::ModelCategoryUpdated(dto),
                Err(e) => ApiEvent::ModelCategoryError(e),
            }
        }
        ApiCommand::DeleteModelCategory { auth, slug } => {
            let url = format!("{}/api/v1/assets/categories/{slug}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::ModelCategoryDeleted(slug),
                Err(e) => ApiEvent::ModelCategoryError(e),
            }
        }
        ApiCommand::CreateMediaAsset { auth, payload } => {
            let url = format!("{}/api/v1/assets/media", base(&auth));
            match build_media_multipart(payload).await {
                Ok(form) => {
                    match client
                        .post(&url)
                        .bearer_auth(&auth.access_token)
                        .header("X-Device-ID", &auth.device_id)
                        .multipart(form)
                        .send()
                        .await
                    {
                        Ok(resp) => match parse_api_json::<MediaAssetDto>(resp).await {
                            Ok(dto) => ApiEvent::MediaAssetCreated(dto),
                            Err(e) => ApiEvent::MediaAssetError(e),
                        },
                        Err(e) => ApiEvent::MediaAssetError(format!("Upload media gagal: {e}")),
                    }
                }
                Err(e) => ApiEvent::MediaAssetError(e),
            }
        }
        ApiCommand::UpdateMediaAsset { auth, id, payload } => {
            let url = format!("{}/api/v1/assets/media/{id}", base(&auth));
            match build_media_multipart(payload).await {
                Ok(form) => {
                    match client
                        .put(&url)
                        .bearer_auth(&auth.access_token)
                        .header("X-Device-ID", &auth.device_id)
                        .multipart(form)
                        .send()
                        .await
                    {
                        Ok(resp) => match parse_api_json::<MediaAssetDto>(resp).await {
                            Ok(dto) => ApiEvent::MediaAssetUpdated(dto),
                            Err(e) => ApiEvent::MediaAssetError(e),
                        },
                        Err(e) => ApiEvent::MediaAssetError(format!("Update media gagal: {e}")),
                    }
                }
                Err(e) => ApiEvent::MediaAssetError(e),
            }
        }
        ApiCommand::ResizeMediaTexture {
            auth,
            id,
            target_size,
        } => {
            let url = format!("{}/api/v1/assets/media/{id}/resize", base(&auth));
            let body = ResizeTextureRequest { target_size };
            match authed_post::<_, MediaAssetDto>(client, &auth, &url, &body).await {
                Ok(dto) => ApiEvent::MediaAssetUpdated(dto),
                Err(e) => ApiEvent::MediaAssetError(e),
            }
        }
        ApiCommand::RestoreMediaTextureOriginal { auth, id } => {
            let url = format!(
                "{}/api/v1/assets/media/{id}/restore-original",
                base(&auth)
            );
            let empty = serde_json::json!({});
            match authed_post::<_, MediaAssetDto>(client, &auth, &url, &empty).await {
                Ok(dto) => ApiEvent::MediaAssetUpdated(dto),
                Err(e) => ApiEvent::MediaAssetError(e),
            }
        }
        ApiCommand::DeleteMediaAsset { auth, id } => {
            let url = format!("{}/api/v1/assets/media/{id}", base(&auth));
            match authed_delete(client, &auth, &url).await {
                Ok(()) => ApiEvent::MediaAssetDeleted(id),
                Err(e) => ApiEvent::MediaAssetError(e),
            }
        }
    }
}

async fn build_media_multipart(payload: MediaAssetUploadPayload) -> Result<multipart::Form, String> {
    let mut form = multipart::Form::new()
        .text("name", payload.name)
        .text("media_type", payload.media_type.as_str().to_string())
        .text("category", payload.category)
        .text("crop_mode", payload.crop_mode);

    if let Some(p) = payload.file_path {
        let part = read_file_part(&p).await?;
        form = form.part("file", part);
    }

    Ok(form)
}

async fn build_model_multipart(payload: ModelAssetUploadPayload) -> Result<multipart::Form, String> {
    let mut form = multipart::Form::new()
        .text("name", payload.name)
        .text("category", payload.category)
        .text("model_type", payload.model_type.as_str().to_string())
        .text("default_height", payload.default_height.to_string())
        .text("default_rot_x", payload.default_rot_x.to_string())
        .text("default_rot_y", payload.default_rot_y.to_string())
        .text("default_rot_z", payload.default_rot_z.to_string());

    if let Some(p) = payload.model_file_path {
        let part = read_file_part(&p).await?;
        form = form.part("model_file", part);
    }
    if let Some(p) = payload.idle_file_path {
        let part = read_file_part(&p).await?;
        form = form.part("idle_file", part);
    }
    if let Some(p) = payload.walk_file_path {
        let part = read_file_part(&p).await?;
        form = form.part("walk_file", part);
    }

    Ok(form)
}

async fn read_file_part(path: &std::path::Path) -> Result<multipart::Part, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Gagal membaca file {}: {e}", path.display()))?;
    let fname = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("model.glb")
        .to_string();
    Ok(multipart::Part::bytes(bytes).file_name(fname))
}

fn base(auth: &AuthCredentials) -> &str {
    auth.server_url.trim_end_matches('/')
}

async fn authed_get<T: DeserializeOwned>(
    client: &Client,
    auth: &AuthCredentials,
    url: &str,
) -> Result<T, String> {
    let resp = client
        .get(url)
        .bearer_auth(&auth.access_token)
        .header("X-Device-ID", &auth.device_id)
        .send()
        .await
        .map_err(|e| format!("HTTP GET failed: {e}"))?;
    parse_api_json(resp).await
}

async fn authed_post<B: serde::Serialize, T: DeserializeOwned>(
    client: &Client,
    auth: &AuthCredentials,
    url: &str,
    body: &B,
) -> Result<T, String> {
    let resp = client
        .post(url)
        .bearer_auth(&auth.access_token)
        .header("X-Device-ID", &auth.device_id)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("HTTP POST failed: {e}"))?;
    parse_api_json(resp).await
}

async fn authed_put<B: serde::Serialize, T: DeserializeOwned>(
    client: &Client,
    auth: &AuthCredentials,
    url: &str,
    body: &B,
) -> Result<T, String> {
    let resp = client
        .put(url)
        .bearer_auth(&auth.access_token)
        .header("X-Device-ID", &auth.device_id)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("HTTP PUT failed: {e}"))?;
    parse_api_json(resp).await
}

async fn authed_delete(
    client: &Client,
    auth: &AuthCredentials,
    url: &str,
) -> Result<(), String> {
    let resp = client
        .delete(url)
        .bearer_auth(&auth.access_token)
        .header("X-Device-ID", &auth.device_id)
        .send()
        .await
        .map_err(|e| format!("HTTP DELETE failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<ApiErrorBody>(&text) {
            return Err(format!("HTTP {}: {}", err.code, err.message));
        }
        return Err(format!("HTTP {status}: {text}"));
    }
    Ok(())
}

async fn parse_api_json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed reading response body: {e}"))?;
    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<ApiErrorBody>(&text) {
            return Err(format!("HTTP {}: {}", err.code, err.message));
        }
        return Err(format!("HTTP {status}: {text}"));
    }
    let wrapped: ApiResponse<T> = serde_json::from_str(&text)
        .map_err(|e| format!("invalid API JSON ({e}): {text}"))?;
    Ok(wrapped.data)
}
