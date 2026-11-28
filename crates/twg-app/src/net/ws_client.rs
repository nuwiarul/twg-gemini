use std::time::Duration;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use twg_shared::ws::{
    BigScreenLiveState, OnlineOperator, TacticalCommand, TacticalCommandBroadcast, WsClientMessage,
    WsClientMode, WsServerMessage,
};
use uuid::Uuid;

#[derive(Debug)]
pub enum WsOutCommand {
    Connect {
        server_url: String,
        token: String,
        device_id: String,
        skenario_id: Option<Uuid>,
        client_mode: WsClientMode,
    },
    Disconnect,
    Subscribe {
        skenario_id: Option<Uuid>,
        client_mode: WsClientMode,
    },
    SendCommand(TacticalCommand),
}

#[derive(Debug)]
pub enum WsInEvent {
    Connected,
    Disconnected(String),
    OnlineUsers(Vec<OnlineOperator>),
    Broadcast(TacticalCommandBroadcast),
    BigScreenState(BigScreenLiveState),
    AssetsChanged {
        operasi_id: Option<Uuid>,
        kind: String,
        #[allow(dead_code)]
        asset_id: Uuid,
        actor_username: String,
    },
    Error(String),
}

#[derive(Resource)]
pub struct WsClientBridge {
    cmd_tx: Sender<WsOutCommand>,
    evt_rx: Receiver<WsInEvent>,
}

impl WsClientBridge {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<WsOutCommand>();
        let (evt_tx, evt_rx) = crossbeam_channel::unbounded::<WsInEvent>();

        std::thread::Builder::new()
            .name("twg-ws-worker".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("failed to build ws tokio runtime");

                rt.block_on(async move {
                    let mut active_out: Option<mpsc::UnboundedSender<WsClientMessage>> = None;
                    let mut active_task: Option<tokio::task::JoinHandle<()>> = None;

                    while let Ok(cmd) = cmd_rx.recv() {
                        match cmd {
                            WsOutCommand::Disconnect => {
                                active_out = None;
                                if let Some(handle) = active_task.take() {
                                    handle.abort();
                                }
                                let _ = evt_tx.send(WsInEvent::Disconnected("disconnected".into()));
                            }
                            WsOutCommand::Connect {
                                server_url,
                                token,
                                device_id,
                                skenario_id,
                                client_mode,
                            } => {
                                if let Some(handle) = active_task.take() {
                                    handle.abort();
                                }

                                let (out_tx, out_rx) = mpsc::unbounded_channel();
                                active_out = Some(out_tx);
                                let evt_tx_clone = evt_tx.clone();

                                active_task = Some(tokio::spawn(run_ws_reconnect_loop(
                                    server_url,
                                    token,
                                    device_id,
                                    skenario_id,
                                    client_mode,
                                    out_rx,
                                    evt_tx_clone,
                                )));
                            }
                            WsOutCommand::Subscribe {
                                skenario_id,
                                client_mode,
                            } => {
                                if let Some(tx) = &active_out {
                                    let _ = tx.send(WsClientMessage::Subscribe {
                                        skenario_id,
                                        client_mode,
                                    });
                                }
                            }
                            WsOutCommand::SendCommand(tcmd) => {
                                if let Some(tx) = &active_out {
                                    let _ = tx.send(WsClientMessage::Command(tcmd));
                                }
                            }
                        }
                    }
                });
            })
            .expect("failed to spawn ws worker thread");

        Self { cmd_tx, evt_rx }
    }

    pub fn send(&self, cmd: WsOutCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn drain_events(&self) -> Vec<WsInEvent> {
        self.evt_rx.try_iter().collect()
    }
}

fn client_mode_query(mode: WsClientMode) -> &'static str {
    match mode {
        WsClientMode::Idle => "idle",
        WsClientMode::TacticalLive => "tactical_live",
        WsClientMode::BigScreen => "big_screen",
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_ws_reconnect_loop(
    server_url: String,
    token: String,
    device_id: String,
    initial_skenario_id: Option<Uuid>,
    initial_mode: WsClientMode,
    mut out_rx: mpsc::UnboundedReceiver<WsClientMessage>,
    evt_tx: Sender<WsInEvent>,
) {
    let mut current_skenario_id = initial_skenario_id;
    let mut current_mode = initial_mode;

    let ws_base = server_url
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);

    loop {
        let mut url = format!(
            "{ws_base}/ws?token={token}&device_id={device_id}&client_mode={}",
            client_mode_query(current_mode)
        );
        if let Some(sid) = current_skenario_id {
            url.push_str(&format!("&skenario_id={sid}"));
        }

        let ws_stream = match connect_async(&url).await {
            Ok((stream, _)) => stream,
            Err(_) => {
                let _ = evt_tx.send(WsInEvent::Disconnected("WS reconnecting...".into()));
                // Keep draining outgoing subscriptions while waiting to reconnect
                let sleep = tokio::time::sleep(Duration::from_secs(2));
                tokio::pin!(sleep);
                loop {
                    tokio::select! {
                        _ = &mut sleep => break,
                        maybe_msg = out_rx.recv() => {
                            match maybe_msg {
                                Some(WsClientMessage::Subscribe { skenario_id, client_mode }) => {
                                    current_skenario_id = skenario_id;
                                    current_mode = client_mode;
                                }
                                Some(_) => {}
                                None => return,
                            }
                        }
                    }
                }
                continue;
            }
        };

        let _ = evt_tx.send(WsInEvent::Connected);
        let (mut write, mut read) = ws_stream.split();

        let mut channel_closed = false;
        loop {
            tokio::select! {
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(Message::Text(txt))) => {
                            if let Ok(server_msg) = serde_json::from_str::<WsServerMessage>(&txt) {
                                match server_msg {
                                    WsServerMessage::Connected { .. } => {
                                        let _ = evt_tx.send(WsInEvent::Connected);
                                    }
                                    WsServerMessage::OnlineUsers { operators } => {
                                        let _ = evt_tx.send(WsInEvent::OnlineUsers(operators));
                                    }
                                    WsServerMessage::Command(b) => {
                                        let _ = evt_tx.send(WsInEvent::Broadcast(b));
                                    }
                                    WsServerMessage::BigScreenState(st) => {
                                        let _ = evt_tx.send(WsInEvent::BigScreenState(st));
                                    }
                                    WsServerMessage::AssetsChanged {
                                        operasi_id,
                                        kind,
                                        asset_id,
                                        actor_username,
                                    } => {
                                        let _ = evt_tx.send(WsInEvent::AssetsChanged {
                                            operasi_id,
                                            kind,
                                            asset_id,
                                            actor_username,
                                        });
                                    }
                                    WsServerMessage::Error { message } => {
                                        let _ = evt_tx.send(WsInEvent::Error(message));
                                    }
                                    WsServerMessage::Pong => {}
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                            break;
                        }
                        Some(Ok(_)) => {}
                    }
                }
                outgoing = out_rx.recv() => {
                    match outgoing {
                        Some(client_msg) => {
                            if let WsClientMessage::Subscribe { skenario_id, client_mode } = &client_msg {
                                current_skenario_id = *skenario_id;
                                current_mode = *client_mode;
                            }
                            if let Ok(txt) = serde_json::to_string(&client_msg)
                                && write.send(Message::Text(txt.into())).await.is_err()
                            {
                                break;
                            }
                        }
                        None => {
                            channel_closed = true;
                            break;
                        }
                    }
                }
            }
        }

        if channel_closed {
            return;
        }
        let _ = evt_tx.send(WsInEvent::Disconnected("WS reconnecting...".into()));
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
