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
use tokio::sync::{RwLock, broadcast};
use twg_shared::{
    models::UserRole,
    ws::{OnlineOperator, TacticalCommandBroadcast, WsClientMessage, WsServerMessage},
};
use uuid::Uuid;

use crate::{
    auth::{
        middleware::validate_active_session,
        token::TokenKind,
    },
    error::AppError,
    state::AppState,
};

#[derive(Debug, Clone)]
pub struct HubEnvelope {
    pub target_skenario_id: Option<Uuid>,
    pub message: WsServerMessage,
}

pub struct WsHub {
    tx: broadcast::Sender<HubEnvelope>,
    connections: RwLock<HashMap<Uuid, OnlineOperator>>,
}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self {
            tx,
            connections: RwLock::new(HashMap::new()),
        }
    }

    pub async fn online_operators(&self) -> Vec<OnlineOperator> {
        self.connections.read().await.values().cloned().collect()
    }

    pub async fn broadcast(&self, target_skenario_id: Option<Uuid>, message: WsServerMessage) {
        let _ = self.tx.send(HubEnvelope {
            target_skenario_id,
            message,
        });
    }

    async fn add_connection(&self, conn_id: Uuid, op: OnlineOperator) {
        self.connections.write().await.insert(conn_id, op);
        self.broadcast_online_users().await;
    }

    async fn update_subscription(&self, conn_id: Uuid, skenario_id: Option<Uuid>) {
        if let Some(op) = self.connections.write().await.get_mut(&conn_id) {
            op.skenario_id = skenario_id;
        }
        self.broadcast_online_users().await;
    }

    async fn remove_connection(&self, conn_id: Uuid) {
        self.connections.write().await.remove(&conn_id);
        self.broadcast_online_users().await;
    }

    async fn broadcast_online_users(&self) {
        let operators = self.online_operators().await;
        self.broadcast(None, WsServerMessage::OnlineUsers { operators })
            .await;
    }
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
    pub device_id: String,
    pub skenario_id: Option<Uuid>,
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

    let device_id = query.device_id.trim();
    if device_id.is_empty() {
        return Err(AppError::Unauthorized("device_id is required".into()));
    }
    validate_active_session(&state, payload.user_id, device_id).await?;

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
            query.skenario_id,
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
    initial_skenario_id: Option<Uuid>,
) {
    let conn_id = Uuid::new_v4();
    let mut subscribed_skenario = initial_skenario_id;
    let mut rx = state.ws_hub.tx.subscribe();

    let (mut sender, mut receiver) = socket.split();

    let connected_msg = WsServerMessage::Connected {
        user_id,
        username: username.clone(),
        role,
    };
    if let Ok(txt) = serde_json::to_string(&connected_msg) {
        let _ = sender.send(Message::Text(txt.into())).await;
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
                connected_at: Utc::now(),
            },
        )
        .await;

    let send_task = tokio::spawn(async move {
        while let Ok(env) = rx.recv().await {
            if let Some(target_sid) = env.target_skenario_id
                && let Some(sub_sid) = subscribed_skenario
                && target_sid != sub_sid
            {
                continue;
            }
            if let Ok(txt) = serde_json::to_string(&env.message)
                && sender.send(Message::Text(txt.into())).await.is_err()
            {
                break;
            }
        }
    });

    // Note: keep track of subscription updates cleanly without sharing mutable state across tasks:
    // we handle client messages in this loop; any `Subscribe` updates WsHub presence directly,
    // and client side also ignores broadcasts outside its active scenario if mismatched.
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) else {
                    continue;
                };
                match client_msg {
                    WsClientMessage::Subscribe { skenario_id } => {
                        subscribed_skenario = skenario_id;
                        state
                            .ws_hub
                            .update_subscription(conn_id, skenario_id)
                            .await;
                    }
                    WsClientMessage::Ping => {
                        state
                            .ws_hub
                            .broadcast(subscribed_skenario, WsServerMessage::Pong)
                            .await;
                    }
                    WsClientMessage::Command(cmd) => {
                        if let Err(err) = process_command(
                            &state,
                            user_id,
                            &username,
                            role,
                            satker_id,
                            cmd,
                        )
                        .await
                        {
                            tracing::warn!(%user_id, ?err, "rejected ws command");
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

async fn process_command(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    role: UserRole,
    user_satker_id: Option<Uuid>,
    cmd: twg_shared::ws::TacticalCommand,
) -> Result<(), AppError> {
    // Check satker isolation on target skenario
    let row = sqlx::query("SELECT satker_id FROM skenarios WHERE id = $1")
        .bind(cmd.skenario_id)
        .fetch_optional(&state.db)
        .await?;

    if let Some(row) = row {
        let sk_satker: Option<Uuid> = row.try_get("satker_id")?;
        if role.is_satker_role() && sk_satker.is_some() && sk_satker != user_satker_id {
            return Err(AppError::Forbidden("cross-satker command forbidden".into()));
        }
    }

    let command_id = Uuid::new_v4();
    if cmd.save_history && !cmd.command.eq_ignore_ascii_case("reload") {
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
        .bind(cmd.command.trim())
        .bind(&cmd.payload)
        .execute(&state.db)
        .await?;
    }

    let broadcast = TacticalCommandBroadcast {
        command_id,
        operasi_id: cmd.operasi_id,
        skenario_id: cmd.skenario_id,
        marker_id: cmd.marker_id,
        actor_user_id: user_id,
        actor_username: username.to_string(),
        satker_id: user_satker_id,
        command: cmd.command,
        payload: cmd.payload,
        sent_at: Utc::now(),
    };

    state
        .ws_hub
        .broadcast(Some(cmd.skenario_id), WsServerMessage::Command(broadcast))
        .await;
    Ok(())
}
