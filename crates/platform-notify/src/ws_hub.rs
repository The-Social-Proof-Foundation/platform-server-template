use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};

#[derive(Clone, Debug, serde::Serialize)]
pub struct WsOutbound {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

#[derive(Clone, Default)]
pub struct WsHub {
    sessions: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

impl WsHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, user_id: String) -> broadcast::Receiver<String> {
        let mut sessions = self.sessions.write().await;
        let sender = sessions
            .entry(user_id.clone())
            .or_insert_with(|| broadcast::channel(256).0);
        sender.subscribe()
    }

    pub async fn unregister(&self, user_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(user_id);
    }

    pub async fn send_to_user(&self, user_id: &str, payload: WsOutbound) -> bool {
        let sessions = self.sessions.read().await;
        let Some(sender) = sessions.get(user_id) else {
            return false;
        };
        if let Ok(json) = serde_json::to_string(&payload) {
            let _ = sender.send(json);
            return true;
        }
        false
    }

    pub async fn handle_socket(&self, user_id: String, socket: WebSocket) {
        let mut rx = self.register(user_id.clone()).await;
        let (mut sender, mut receiver) = socket.split();

        let hub = self.clone();
        let user_id_clone = user_id.clone();
        let forward = tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sender.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            hub.unregister(&user_id_clone).await;
        });

        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }

        forward.abort();
        self.unregister(&user_id).await;
    }
}
