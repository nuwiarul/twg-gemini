pub mod auth;
pub mod entities;
pub mod health;
pub mod location_presets;
pub mod media_assets;
pub mod model_assets;
pub mod operasis;
pub mod satkers;
pub mod skenarios;
pub mod snapshots;
pub mod uploads;
pub mod users;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, put},
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};

use crate::{state::AppState, ws};

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let upload_dir = state.config.upload_dir.clone();
    let assets_dir = state.config.assets_dir.clone();

    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/ws", get(ws::ws_handler))
        // Auth & Profile
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/identify", get(auth::identify))
        .route("/api/v1/profile", put(auth::update_profile))
        .route("/api/v1/profile/password", put(auth::change_password))
        // Satkers & Users
        .route(
            "/api/v1/satkers",
            get(satkers::list_satkers).post(satkers::create_satker),
        )
        .route(
            "/api/v1/satkers/{id}",
            get(satkers::get_satker)
                .put(satkers::update_satker)
                .delete(satkers::delete_satker),
        )
        .route(
            "/api/v1/users",
            get(users::list_users).post(users::create_user),
        )
        .route("/api/v1/users/online", get(users::list_online_users))
        .route(
            "/api/v1/users/{id}",
            get(users::get_user)
                .put(users::update_user)
                .delete(users::delete_user),
        )
        // Master Data: Location Presets
        .route(
            "/api/v1/location-presets",
            get(location_presets::list_location_presets)
                .post(location_presets::create_location_preset),
        )
        .route(
            "/api/v1/location-presets/{id}",
            put(location_presets::update_location_preset)
                .delete(location_presets::delete_location_preset),
        )
        // Operasis & Skenarios
        .route(
            "/api/v1/operasis",
            get(operasis::list_operasis).post(operasis::create_operasi),
        )
        .route(
            "/api/v1/operasis/{id}",
            get(operasis::get_operasi)
                .put(operasis::update_operasi)
                .delete(operasis::delete_operasi),
        )
        .route(
            "/api/v1/operasis/{operasi_id}/skenarios",
            get(skenarios::list_skenarios_by_operasi),
        )
        .route(
            "/api/v1/skenarios",
            get(skenarios::list_skenarios).post(skenarios::create_skenario),
        )
        .route(
            "/api/v1/skenarios/{id}",
            get(skenarios::get_skenario)
                .put(skenarios::update_skenario)
                .delete(skenarios::delete_skenario),
        )
        .route(
            "/api/v1/skenarios/{id}/bundle",
            get(skenarios::get_skenario_bundle),
        )
        // Scenario Entities
        .route("/api/v1/markers", post(entities::create_marker))
        .route(
            "/api/v1/markers/{id}",
            put(entities::update_marker).delete(entities::delete_marker),
        )
        .route("/api/v1/buildings", post(entities::create_building))
        .route(
            "/api/v1/buildings/{id}",
            put(entities::update_building).delete(entities::delete_building),
        )
        .route("/api/v1/lines", post(entities::create_line))
        .route(
            "/api/v1/lines/{id}",
            put(entities::update_line).delete(entities::delete_line),
        )
        .route("/api/v1/text-markers", post(entities::create_text_marker))
        .route(
            "/api/v1/text-markers/{id}",
            put(entities::update_text_marker).delete(entities::delete_text_marker),
        )
        .route("/api/v1/particles", post(entities::create_particle))
        .route(
            "/api/v1/particles/{id}",
            axum::routing::delete(entities::delete_particle),
        )
        .route("/api/v1/alurs", post(entities::create_alur))
        .route(
            "/api/v1/alurs/{id}",
            axum::routing::delete(entities::delete_alur),
        )
        .route("/api/v1/anims", post(entities::create_anim))
        .route(
            "/api/v1/anims/{id}",
            put(entities::update_anim).delete(entities::delete_anim),
        )
        // Snapshots & Uploads & Catalog
        .route(
            "/api/v1/skenarios/{skenario_id}/snapshot",
            get(snapshots::get_snapshot)
                .put(snapshots::upsert_snapshot)
                .delete(snapshots::delete_snapshot),
        )
        .route("/api/v1/uploads", post(uploads::upload_file))
        .route("/api/v1/catalog", get(uploads::get_model_catalog))
        // 3D Model Assets (Manifest + Offline Sync + Admin CRUD + Dynamic Categories)
        .route(
            "/api/v1/assets/categories",
            get(model_assets::list_model_categories).post(model_assets::create_model_category),
        )
        .route(
            "/api/v1/assets/categories/{slug}",
            put(model_assets::update_model_category).delete(model_assets::delete_model_category),
        )
        .route(
            "/api/v1/assets/models/manifest",
            get(model_assets::get_sync_manifest),
        )
        .route(
            "/api/v1/assets/models/file/{*rel_path}",
            get(model_assets::download_model_file),
        )
        .route(
            "/api/v1/assets/models",
            get(model_assets::list_model_assets).post(model_assets::create_model_asset),
        )
        .route(
            "/api/v1/assets/models/{id}",
            get(model_assets::get_model_asset)
                .put(model_assets::update_model_asset)
                .delete(model_assets::delete_model_asset),
        )
        // Media Assets (Image Texture + Sound + Video)
        .route(
            "/api/v1/assets/media",
            get(media_assets::list_media_assets).post(media_assets::create_media_asset),
        )
        .route(
            "/api/v1/assets/media/{id}",
            get(media_assets::get_media_asset)
                .put(media_assets::update_media_asset)
                .delete(media_assets::delete_media_asset),
        )
        .route(
            "/api/v1/assets/media/{id}/resize",
            post(media_assets::resize_media_texture),
        )
        .route(
            "/api/v1/assets/media/{id}/restore-original",
            post(media_assets::restore_media_texture_original),
        )
        .nest_service("/uploads", ServeDir::new(upload_dir))
        .nest_service("/assets", ServeDir::new(assets_dir))
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
