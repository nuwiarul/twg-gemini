use std::collections::HashMap;

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use sqlx::Row;
use tokio::sync::{RwLock, broadcast, mpsc, watch};
use twg_shared::{
    models::UserRole,
    ws::{
        BigScreenLiveState, OnlineOperator, TacticalCommandBroadcast, WsClientMessage,
        WsClientMode, WsServerMessage,
    },
};
use uuid::Uuid;

use crate::{
    auth::{middleware::validate_active_session, token::TokenKind},
    error::AppError,
    state::AppState,
};

#[derive(Debug, Clone)]
pub enum HubDelivery {
    /// Broadcast to all connected clients (e.g. online presence).
    All,
    /// Delivered ONLY to `BigScreen` clients authorized for this command's scope
    /// (`load_skenario` when `skenario_id: None`, or `camera_sync` when `skenario_id: Some`).
    BigScreenOnly {
        skenario_id: Option<Uuid>,
        skenario_satker_id: Option<Uuid>,
        actor_role: UserRole,
    },
    /// Delivered to `TacticalLive` clients subscribed to `skenario_id` AND `BigScreen` clients
    /// currently displaying `skenario_id` (or matching scope when `force_big_screen` is true,
    /// e.g. `reload_skenario`).
    TacticalAndBigScreen {
        skenario_id: Uuid,
        skenario_satker_id: Option<Uuid>,
        actor_role: UserRole,
        force_big_screen: bool,
    },
}

#[derive(Debug, Clone)]
pub struct HubEnvelope {
    pub delivery: HubDelivery,
    pub message: WsServerMessage,
}

pub struct WsHub {
    tx: broadcast::Sender<HubEnvelope>,
    connections: RwLock<HashMap<Uuid, OnlineOperator>>,
    /// Cached active Layar Besar state keyed by scope (`None` = global/Pusat, `Some(satker_id)` = Satker).
    big_screen_states: RwLock<HashMap<Option<Uuid>, BigScreenLiveState>>,
}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            connections: RwLock::new(HashMap::new()),
            big_screen_states: RwLock::new(HashMap::new()),
        }
    }

    pub async fn online_operators(&self) -> Vec<OnlineOperator> {
        self.connections.read().await.values().cloned().collect()
    }

    pub async fn broadcast(&self, delivery: HubDelivery, message: WsServerMessage) {
        let _ = self.tx.send(HubEnvelope { delivery, message });
    }

    async fn add_connection(&self, conn_id: Uuid, op: OnlineOperator) {
        self.connections.write().await.insert(conn_id, op);
        self.broadcast_online_users().await;
    }

    async fn update_subscription(
        &self,
        conn_id: Uuid,
        skenario_id: Option<Uuid>,
        client_mode: WsClientMode,
    ) {
        if let Some(op) = self.connections.write().await.get_mut(&conn_id) {
            op.skenario_id = skenario_id;
            op.client_mode = client_mode;
        }
        self.broadcast_online_users().await;
    }

    async fn remove_connection(&self, conn_id: Uuid) {
        self.connections.write().await.remove(&conn_id);
        self.broadcast_online_users().await;
    }

    async fn broadcast_online_users(&self) {
        let operators = self.online_operators().await;
        self.broadcast(HubDelivery::All, WsServerMessage::OnlineUsers { operators })
            .await;
    }

    async fn get_big_screen_state_for_user(
        &self,
        role: UserRole,
        user_satker_id: Option<Uuid>,
    ) -> Option<BigScreenLiveState> {
        let map = self.big_screen_states.read().await;
        if role.is_satker_role() {
            map.get(&user_satker_id).cloned()
        } else {
            map.get(&None).cloned()
        }
    }

    async fn update_big_screen_load(
        &self,
        actor_role: UserRole,
        actor_username: &str,
        operasi_id: Uuid,
        skenario_id: Uuid,
        skenario_satker_id: Option<Uuid>,
    ) {
        let live = BigScreenLiveState {
            operasi_id,
            skenario_id,
            skenario_satker_id,
            actor_username: actor_username.to_string(),
            actor_role,
            camera: None,
            updated_at: Utc::now(),
        };
        let mut map = self.big_screen_states.write().await;
        if actor_role.is_admin() {
            // Super admin drives Pusat/global Layar Besar for any scenario
            map.insert(None, live.clone());
            // If admin opened a satker scenario, also update that satker's Layar Besar state
            if let Some(sid) = skenario_satker_id {
                map.insert(Some(sid), live);
            }
        } else if actor_role.is_admin_satker()
            && let Some(sid) = skenario_satker_id
        {
            map.insert(Some(sid), live);
        }
    }

    async fn update_big_screen_camera(
        &self,
        actor_role: UserRole,
        skenario_id: Uuid,
        skenario_satker_id: Option<Uuid>,
        camera_payload: &serde_json::Value,
    ) {
        let mut map = self.big_screen_states.write().await;
        if actor_role.is_admin()
            && let Some(st) = map.get_mut(&None)
            && st.skenario_id == skenario_id
        {
            st.camera = Some(camera_payload.clone());
            st.updated_at = Utc::now();
        }
        if let Some(sid) = skenario_satker_id
            && let Some(st) = map.get_mut(&Some(sid))
            && st.skenario_id == skenario_id
        {
            st.camera = Some(camera_payload.clone());
            st.updated_at = Utc::now();
        }
    }
}

fn big_screen_matches_scope(
    receiver_role: UserRole,
    receiver_satker_id: Option<Uuid>,
    skenario_satker_id: Option<Uuid>,
    actor_role: UserRole,
) -> bool {
    if receiver_role.is_satker_role() {
        receiver_satker_id.is_some() && skenario_satker_id == receiver_satker_id
    } else {
        // Global/Pusat Layar Besar only obeys super `admin` for scenario load & camera control
        actor_role.is_admin()
    }
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
    pub device_id: String,
    pub skenario_id: Option<Uuid>,
    #[serde(default)]
    pub client_mode: WsClientMode,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let payload = state
        .tokens
        .verify(query.token.trim(), TokenKind::Access)
        .map_err(|_| AppError::Unauthorized("invalid or expired websocket token".into()))?;

    let device_id = query.device_id.trim().to_string();
    if device_id.is_empty() {
        return Err(AppError::Unauthorized("device_id is required".into()));
    }
    validate_active_session(&state, payload.user_id, &device_id).await?;

    let user_row = sqlx::query("SELECT name FROM users WHERE id = $1")
        .bind(payload.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;
    let name: String = user_row.try_get("name")?;

    Ok(ws.on_upgrade(move |socket| {
        handle_socket(
            socket,
            state,
            payload.user_id,
            payload.username,
            name,
            payload.role,
            payload.satker_id,
            device_id,
            query.skenario_id,
            query.client_mode,
        )
    }))
}

#[allow(clippy::too_many_arguments)]
async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    user_id: Uuid,
    username: String,
    name: String,
    role: UserRole,
    satker_id: Option<Uuid>,
    device_id: String,
    initial_skenario_id: Option<Uuid>,
    initial_mode: WsClientMode,
) {
    let conn_id = Uuid::new_v4();
    let (sub_tx, sub_rx) = watch::channel((initial_skenario_id, initial_mode));
    let (direct_tx, mut direct_rx) = mpsc::unbounded_channel::<WsServerMessage>();
    let mut hub_rx = state.ws_hub.tx.subscribe();

    let (mut sender, mut receiver) = socket.split();

    let _ = direct_tx.send(WsServerMessage::Connected {
        user_id,
        username: username.clone(),
        role,
    });

    // If connecting directly in BigScreen mode, send any cached active BigScreen scenario right away
    if initial_mode == WsClientMode::BigScreen
        && let Some(live) = state
            .ws_hub
            .get_big_screen_state_for_user(role, satker_id)
            .await
    {
        let _ = direct_tx.send(WsServerMessage::BigScreenState(live));
    }

    state
        .ws_hub
        .add_connection(
            conn_id,
            OnlineOperator {
                user_id,
                username: username.clone(),
                name,
                role,
                satker_id,
                skenario_id: initial_skenario_id,
                client_mode: initial_mode,
                connected_at: Utc::now(),
            },
        )
        .await;

    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(direct_msg) = direct_rx.recv() => {
                    if let Ok(txt) = serde_json::to_string(&direct_msg)
                        && sender.send(Message::Text(txt.into())).await.is_err()
                    {
                        break;
                    }
                }
                hub_res = hub_rx.recv() => {
                    match hub_res {
                        Ok(env) => {
                            let (sub_sid, mode) = *sub_rx.borrow();
                            let should_deliver = match env.delivery {
                                HubDelivery::All => true,
                                HubDelivery::BigScreenOnly {
                                    skenario_id,
                                    skenario_satker_id,
                                    actor_role,
                                } => {
                                    mode == WsClientMode::BigScreen
                                        && big_screen_matches_scope(
                                            role,
                                            satker_id,
                                            skenario_satker_id,
                                            actor_role,
                                        )
                                        && skenario_id.is_none_or(|sid| sub_sid == Some(sid))
                                }
                                HubDelivery::TacticalAndBigScreen {
                                    skenario_id,
                                    skenario_satker_id,
                                    actor_role,
                                    force_big_screen,
                                } => match mode {
                                    WsClientMode::Idle => false,
                                    WsClientMode::TacticalLive => sub_sid == Some(skenario_id),
                                    WsClientMode::BigScreen => {
                                        if force_big_screen {
                                            big_screen_matches_scope(
                                                role,
                                                satker_id,
                                                skenario_satker_id,
                                                actor_role,
                                            )
                                        } else {
                                            sub_sid == Some(skenario_id)
                                        }
                                    }
                                },
                            };

                            if should_deliver
                                && let Ok(txt) = serde_json::to_string(&env.message)
                                && sender.send(Message::Text(txt.into())).await.is_err()
                            {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) else {
                    continue;
                };
                match client_msg {
                    WsClientMessage::Subscribe {
                        skenario_id,
                        client_mode,
                    } => {
                        let prev_mode = sub_tx.borrow().1;
                        let _ = sub_tx.send((skenario_id, client_mode));
                        state
                            .ws_hub
                            .update_subscription(conn_id, skenario_id, client_mode)
                            .await;

                        // When a client enters BigScreen mode (with no scenario loaded yet), push cached state
                        if client_mode == WsClientMode::BigScreen
                            && prev_mode != WsClientMode::BigScreen
                            && let Some(live) = state
                                .ws_hub
                                .get_big_screen_state_for_user(role, satker_id)
                                .await
                        {
                            let _ = direct_tx.send(WsServerMessage::BigScreenState(live));
                        }
                    }
                    WsClientMessage::Ping => {
                        let _ = direct_tx.send(WsServerMessage::Pong);
                    }
                    WsClientMessage::Command(cmd) => {
                        if let Err(err) = process_command(
                            &state,
                            user_id,
                            &username,
                            role,
                            satker_id,
                            &device_id,
                            cmd,
                        )
                        .await
                        {
                            tracing::warn!(%user_id, ?err, "rejected ws command");
                            let _ = direct_tx.send(WsServerMessage::Error {
                                message: format!("Perintah WS ditolak: {err}"),
                            });
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
    state.ws_hub.remove_connection(conn_id).await;
}

#[allow(clippy::too_many_arguments)]
async fn process_command(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    role: UserRole,
    user_satker_id: Option<Uuid>,
    device_id: &str,
    cmd: twg_shared::ws::TacticalCommand,
) -> Result<(), AppError> {
    let row = sqlx::query("SELECT satker_id FROM skenarios WHERE id = $1")
        .bind(cmd.skenario_id)
        .fetch_optional(&state.db)
        .await?;

    if role.is_big_screen_only_role() {
        return Err(AppError::Forbidden(
            "akun Layar Besar bersifat read-only dan tidak dapat mengirim perintah".into(),
        ));
    }

    let sk_satker: Option<Uuid> = match row {
        Some(r) => r.try_get("satker_id")?,
        None => return Err(AppError::NotFound("skenario not found".into())),
    };

    // Enforce satker isolation for all commands
    if role.is_satker_role() && (user_satker_id.is_none() || sk_satker != user_satker_id) {
        return Err(AppError::Forbidden("cross-satker command forbidden".into()));
    }
    if role == UserRole::User && sk_satker.is_some() {
        return Err(AppError::Forbidden(
            "standard user cannot command satker skenario".into(),
        ));
    }

    let cmd_name = cmd.command.trim();
    let is_big_screen_control = matches!(
        cmd_name,
        "camera_sync" | "load_skenario" | "reload_skenario" | "reload"
    );

    if is_big_screen_control && !role.can_control_big_screen(sk_satker, user_satker_id) {
        return Err(AppError::Forbidden(
            "role ini tidak memiliki izin mengendalikan Layar Besar".into(),
        ));
    }

    let command_id = Uuid::new_v4();
    if cmd.save_history && !matches!(cmd_name, "reload" | "reload_skenario" | "camera_sync") {
        sqlx::query(
            r#"
            INSERT INTO commands (id, operasi_id, skenario_id, marker_id, actor_user_id, command, payload)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(command_id)
        .bind(cmd.operasi_id)
        .bind(cmd.skenario_id)
        .bind(cmd.marker_id)
        .bind(user_id)
        .bind(cmd_name)
        .bind(&cmd.payload)
        .execute(&state.db)
        .await?;
    }

    // Update cached Layar Besar state on server for late-joining BigScreens
    match cmd_name {
        "load_skenario" | "reload_skenario" | "reload" => {
            state
                .ws_hub
                .update_big_screen_load(role, username, cmd.operasi_id, cmd.skenario_id, sk_satker)
                .await;
        }
        "camera_sync" => {
            state
                .ws_hub
                .update_big_screen_camera(role, cmd.skenario_id, sk_satker, &cmd.payload)
                .await;
        }
        _ => {}
    }

    let delivery = match cmd_name {
        "camera_sync" => HubDelivery::BigScreenOnly {
            skenario_id: Some(cmd.skenario_id),
            skenario_satker_id: sk_satker,
            actor_role: role,
        },
        "load_skenario" => HubDelivery::BigScreenOnly {
            skenario_id: None,
            skenario_satker_id: sk_satker,
            actor_role: role,
        },
        "reload_skenario" | "reload" => HubDelivery::TacticalAndBigScreen {
            skenario_id: cmd.skenario_id,
            skenario_satker_id: sk_satker,
            actor_role: role,
            force_big_screen: true,
        },
        _ => HubDelivery::TacticalAndBigScreen {
            skenario_id: cmd.skenario_id,
            skenario_satker_id: sk_satker,
            actor_role: role,
            force_big_screen: false,
        },
    };

    let broadcast = TacticalCommandBroadcast {
        command_id,
        operasi_id: cmd.operasi_id,
        skenario_id: cmd.skenario_id,
        marker_id: cmd.marker_id,
        actor_user_id: user_id,
        actor_username: username.to_string(),
        actor_role: role,
        actor_device_id: device_id.to_string(),
        satker_id: user_satker_id,
        skenario_satker_id: sk_satker,
        command: cmd_name.to_string(),
        payload: cmd.payload,
        sent_at: Utc::now(),
    };

    state
        .ws_hub
        .broadcast(delivery, WsServerMessage::Command(broadcast))
        .await;
    Ok(())
}
