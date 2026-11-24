use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use twg_shared::ws::{
    OnlineOperator, TacticalCommand, TacticalCommandBroadcast, WsClientMessage, WsServerMessage,
};
use uuid::Uuid;

#[derive(Debug)]
pub enum WsOutCommand {
    Connect {
        server_url: String,
        token: String,
        device_id: String,
        skenario_id: Option<Uuid>,
    },
    Disconnect,
    Subscribe(Option<Uuid>),
    SendCommand(TacticalCommand),
}

#[derive(Debug)]
pub enum WsInEvent {
    Connected,
    Disconnected(String),
    OnlineUsers(Vec<OnlineOperator>),
    Broadcast(TacticalCommandBroadcast),
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
                            } => {
                                if let Some(handle) = active_task.take() {
                                    handle.abort();
                                }

                                let (out_tx, out_rx) = mpsc::unbounded_channel();
                                active_out = Some(out_tx);
                                let evt_tx_clone = evt_tx.clone();

                                active_task = Some(tokio::spawn(run_ws_session(
                                    server_url,
                                    token,
                                    device_id,
                                    skenario_id,
                                    out_rx,
                                    evt_tx_clone,
                                )));
                            }
                            WsOutCommand::Subscribe(skenario_id) => {
                                if let Some(tx) = &active_out {
                                    let _ = tx.send(WsClientMessage::Subscribe { skenario_id });
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

async fn run_ws_session(
    server_url: String,
    token: String,
    device_id: String,
    skenario_id: Option<Uuid>,
    mut out_rx: mpsc::UnboundedReceiver<WsClientMessage>,
    evt_tx: Sender<WsInEvent>,
) {
    let ws_base = server_url
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);

    let mut url = format!("{ws_base}/ws?token={token}&device_id={device_id}");
    if let Some(sid) = skenario_id {
        url.push_str(&format!("&skenario_id={sid}"));
    }

    let Ok((ws_stream, _)) = connect_async(&url).await else {
        let _ = evt_tx.send(WsInEvent::Disconnected("WS connect failed".into()));
        return;
    };

    let _ = evt_tx.send(WsInEvent::Connected);
    let (mut write, mut read) = ws_stream.split();

    let evt_tx_read = evt_tx.clone();
    let read_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            if let Message::Text(txt) = msg
                && let Ok(server_msg) = serde_json::from_str::<WsServerMessage>(&txt)
            {
                match server_msg {
                    WsServerMessage::Connected { .. } => {
                        let _ = evt_tx_read.send(WsInEvent::Connected);
                    }
                    WsServerMessage::OnlineUsers { operators } => {
                        let _ = evt_tx_read.send(WsInEvent::OnlineUsers(operators));
                    }
                    WsServerMessage::Command(b) => {
                        let _ = evt_tx_read.send(WsInEvent::Broadcast(b));
                    }
                    WsServerMessage::Error { message } => {
                        let _ = evt_tx_read.send(WsInEvent::Error(message));
                    }
                    WsServerMessage::Pong => {}
                }
            }
        }
    });

    while let Some(client_msg) = out_rx.recv().await {
        if let Ok(txt) = serde_json::to_string(&client_msg)
            && write.send(Message::Text(txt.into())).await.is_err()
        {
            break;
        }
    }

    read_task.abort();
    let _ = evt_tx.send(WsInEvent::Disconnected("WS closed".into()));
}
