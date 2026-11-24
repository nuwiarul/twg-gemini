use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::UserRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalCommand {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    #[serde(default)]
    pub marker_id: Option<Uuid>,
    pub command: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub save_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalCommandBroadcast {
    pub command_id: Uuid,
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub marker_id: Option<Uuid>,
    pub actor_user_id: Uuid,
    pub actor_username: String,
    pub satker_id: Option<Uuid>,
    pub command: String,
    pub payload: serde_json::Value,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineOperator {
    pub user_id: Uuid,
    pub username: String,
    pub name: String,
    pub role: UserRole,
    pub satker_id: Option<Uuid>,
    pub skenario_id: Option<Uuid>,
    pub connected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    Subscribe { skenario_id: Option<Uuid> },
    Command(TacticalCommand),
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    Connected {
        user_id: Uuid,
        username: String,
        role: UserRole,
    },
    Command(TacticalCommandBroadcast),
    OnlineUsers {
        operators: Vec<OnlineOperator>,
    },
    Error {
        message: String,
    },
    Pong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_message_roundtrip() {
        let cmd = WsClientMessage::Command(TacticalCommand {
            operasi_id: Uuid::new_v4(),
            skenario_id: Uuid::new_v4(),
            marker_id: Some(Uuid::new_v4()),
            command: "move_marker".to_string(),
            payload: serde_json::json!({ "lng": 115.2167, "lat": -8.6705 }),
            save_history: true,
        });
        let raw = serde_json::to_string(&cmd).unwrap();
        let parsed: WsClientMessage = serde_json::from_str(&raw).unwrap();
        match parsed {
            WsClientMessage::Command(c) => assert_eq!(c.command, "move_marker"),
            _ => panic!("expected command variant"),
        }
    }
}
