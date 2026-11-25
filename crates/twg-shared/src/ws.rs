use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::UserRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WsClientMode {
    #[default]
    Idle,
    TacticalLive,
    BigScreen,
}

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
    #[serde(default)]
    pub actor_role: UserRole,
    #[serde(default)]
    pub actor_device_id: String,
    pub satker_id: Option<Uuid>,
    #[serde(default)]
    pub skenario_satker_id: Option<Uuid>,
    pub command: String,
    pub payload: serde_json::Value,
    pub sent_at: DateTime<Utc>,
}

/// Cached active Layar Besar state on the server so a Layar Besar opened mid-session
/// immediately loads the current scenario and syncs to the latest camera pose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigScreenLiveState {
    pub operasi_id: Uuid,
    pub skenario_id: Uuid,
    pub skenario_satker_id: Option<Uuid>,
    pub actor_username: String,
    pub actor_role: UserRole,
    #[serde(default)]
    pub camera: Option<serde_json::Value>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineOperator {
    pub user_id: Uuid,
    pub username: String,
    pub name: String,
    pub role: UserRole,
    pub satker_id: Option<Uuid>,
    pub skenario_id: Option<Uuid>,
    #[serde(default)]
    pub client_mode: WsClientMode,
    pub connected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    Subscribe {
        skenario_id: Option<Uuid>,
        #[serde(default)]
        client_mode: WsClientMode,
    },
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
    BigScreenState(BigScreenLiveState),
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

        let sub = WsClientMessage::Subscribe {
            skenario_id: Some(Uuid::new_v4()),
            client_mode: WsClientMode::BigScreen,
        };
        let raw_sub = serde_json::to_string(&sub).unwrap();
        let parsed_sub: WsClientMessage = serde_json::from_str(&raw_sub).unwrap();
        match parsed_sub {
            WsClientMessage::Subscribe { client_mode, .. } => {
                assert_eq!(client_mode, WsClientMode::BigScreen)
            }
            _ => panic!("expected subscribe variant"),
        }
    }

    #[test]
    fn big_screen_control_permissions_match_role_matrix() {
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        assert!(UserRole::Admin.can_control_big_screen(None, None));
        assert!(UserRole::Admin.can_control_big_screen(Some(s1), None));

        assert!(!UserRole::AdminMonitor.can_control_big_screen(None, None));
        assert!(!UserRole::AdminMonitor.can_control_big_screen(Some(s1), None));

        assert!(UserRole::AdminSatker.can_control_big_screen(Some(s1), Some(s1)));
        assert!(!UserRole::AdminSatker.can_control_big_screen(Some(s2), Some(s1)));
        assert!(!UserRole::AdminSatker.can_control_big_screen(None, Some(s1)));

        assert!(!UserRole::User.can_control_big_screen(None, None));
        assert!(!UserRole::UserSatker.can_control_big_screen(Some(s1), Some(s1)));

        assert!(UserRole::LayarBesar.is_big_screen_only_role());
        assert!(!UserRole::LayarBesar.is_satker_role());
        assert!(!UserRole::LayarBesar.can_control_big_screen(None, None));
        assert!(!UserRole::LayarBesar.can_control_big_screen(Some(s1), None));

        assert!(UserRole::LayarBesarSatker.is_big_screen_only_role());
        assert!(UserRole::LayarBesarSatker.is_satker_role());
        assert!(!UserRole::LayarBesarSatker.can_control_big_screen(Some(s1), Some(s1)));

        assert_eq!(UserRole::from_db("layar_besar"), UserRole::LayarBesar);
        assert_eq!(UserRole::from_db("layer_besar"), UserRole::LayarBesar);
        assert_eq!(
            UserRole::from_db("layar_besar_satker"),
            UserRole::LayarBesarSatker
        );
        assert_eq!(
            UserRole::from_db("layer_besar_satker"),
            UserRole::LayarBesarSatker
        );
    }
}
