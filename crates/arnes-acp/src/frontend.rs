// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;

use arnes_core::{
    EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent,
    ToolCallOutcome,
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

    fn send(&self, update: SessionUpdate) {
        let notification = Notification::new(
            "session/update",
            SessionUpdateParams {
                session_id: self.session_id.clone(),
                update,
            },
        );
        if let Ok(line) = serde_json::to_string(&notification) {
            let _ = self.notify_tx.send(line);
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
                self.send(SessionUpdate::AgentMessageChunk {
                    message_id,
                    content: MessageContent { kind: "text", text },
                });
            }
            EventKind::ThinkingDelta { text } => {
                let message_id = self
                    .current_message_id
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| Uuid::now_v7().to_string());
                self.send(SessionUpdate::AgentThoughtChunk {
                    message_id,
                    content: MessageContent { kind: "text", text },
                });
            }
            EventKind::ToolCallStarted { id, name, .. } => {
                self.send(SessionUpdate::ToolCall {
                    tool_call_id: id.clone(),
                    title: name,
                    kind: "tool_use",
                    status: "pending",
                });
                self.send(SessionUpdate::ToolCallUpdate {
                    tool_call_id: id,
                    status: "in_progress",
                    content: None,
                });
            }
            EventKind::ToolCallFinished { id, outcome, .. } => {
                let text = match outcome {
                    ToolCallOutcome::Ok(v) => {
                        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                            format!("error: {err}")
                        } else if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                            content.to_string()
                        } else {
                            v.to_string()
                        }
                    }
                    ToolCallOutcome::Err(msg) => msg,
                    ToolCallOutcome::Denied => "denied".to_string(),
                    ToolCallOutcome::Unknown => "unknown".to_string(),
                };
                self.send(SessionUpdate::ToolCallUpdate {
                    tool_call_id: id,
                    status: "completed",
                    content: Some(MessageContent { kind: "text", text }),
                });
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
