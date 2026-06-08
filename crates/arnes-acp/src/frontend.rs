// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;

use arnes_core::{
    EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent,
};
use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::types::{
    jsonrpc::Notification,
    session::{MessageContent, SessionUpdate, SessionUpdateParams},
};

pub struct AcpFrontend {
    session_id: String,
    notify_tx: mpsc::UnboundedSender<String>,
    current_message_id: Mutex<Option<String>>,
}

impl AcpFrontend {
    pub fn new(session_id: String, notify_tx: mpsc::UnboundedSender<String>) -> Self {
        Self {
            session_id,
            notify_tx,
            current_message_id: Mutex::new(None),
        }
    }
}

#[async_trait]
impl Frontend for AcpFrontend {
    async fn on_event(&self, event: SessionEvent) {
        match event.kind {
            EventKind::TurnStart => {
                *self.current_message_id.lock().unwrap() = Some(Uuid::now_v7().to_string());
            }
            EventKind::TextDelta { text } => {
                let message_id = self
                    .current_message_id
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| Uuid::now_v7().to_string());
                let notification = Notification::new(
                    "session/update",
                    SessionUpdateParams {
                        session_id: self.session_id.clone(),
                        update: SessionUpdate::AgentMessageChunk {
                            message_id,
                            content: MessageContent { kind: "text", text },
                        },
                    },
                );
                if let Ok(line) = serde_json::to_string(&notification) {
                    let _ = self.notify_tx.send(line);
                }
            }
            EventKind::ThinkingDelta { text } => {
                let message_id = self
                    .current_message_id
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| Uuid::now_v7().to_string());
                let notification = Notification::new(
                    "session/update",
                    SessionUpdateParams {
                        session_id: self.session_id.clone(),
                        update: SessionUpdate::AgentThoughtChunk {
                            message_id,
                            content: MessageContent {
                                kind: "text",
                                text,
                            },
                        },
                    },
                );
                if let Ok(line) = serde_json::to_string(&notification) {
                    let _ = self.notify_tx.send(line);
                }
            }
            EventKind::TurnEnd { .. } => {
                *self.current_message_id.lock().unwrap() = None;
            }
            _ => {}
        }
    }

    async fn request_permission(&self, _req: PermissionRequest) -> Permission {
        Permission::AllowOnce
    }

    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities::default()
    }
}
