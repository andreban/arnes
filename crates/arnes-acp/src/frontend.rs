// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;

use arnes_core::{
    EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent,
    ToolCallOutcome,
};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    host::PendingRequests,
    types::{
        jsonrpc::{Notification, OutboundRequest},
        permission::{
            self, PermissionOutcome, PermissionToolCall, RequestPermissionParams,
            RequestPermissionResult,
        },
        session::{MessageContent, SessionUpdate, SessionUpdateParams},
    },
};

pub struct AcpFrontend {
    session_id: String,
    notify_tx: mpsc::UnboundedSender<String>,
    pending: PendingRequests,
    current_message_id: Mutex<Option<String>>,
}

impl AcpFrontend {
    pub fn new(
        session_id: String,
        notify_tx: mpsc::UnboundedSender<String>,
        pending: PendingRequests,
    ) -> Self {
        Self {
            session_id,
            notify_tx,
            pending,
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

    async fn request_permission(&self, req: PermissionRequest) -> Permission {
        let request_id = Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request_id.clone(), tx);

        // Reuse the tool-call id from authorize so the prompt correlates with
        // the tool_call card we echo in session/update.
        let params = RequestPermissionParams {
            session_id: self.session_id.clone(),
            tool_call: PermissionToolCall {
                tool_call_id: req.tool_call_id,
                title: req.tool_name,
                raw_input: req.args,
            },
            options: permission::default_options(),
        };
        let outbound =
            OutboundRequest::new(request_id.clone(), "session/request_permission", params);
        let Ok(serialized) = serde_json::to_string(&outbound) else {
            self.pending.lock().await.remove(&request_id);
            return Permission::Deny;
        };
        if self.notify_tx.send(serialized).is_err() {
            self.pending.lock().await.remove(&request_id);
            return Permission::Deny;
        }

        // No timeout: the user may take a while to answer. agent-rig races
        // this future against session cancellation, so a stuck prompt is
        // dropped when the turn is cancelled. A closed channel (client gone)
        // or any non-allow outcome denies.
        match rx.await {
            Ok(Ok(value)) => permission_from_value(value),
            _ => Permission::Deny,
        }
    }

    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities::default()
    }
}

/// Maps a `session/request_permission` result to a verdict. Anything other
/// than an explicit allow-once selection — reject, cancel, or an unparseable
/// payload — denies.
fn permission_from_value(value: Value) -> Permission {
    match serde_json::from_value::<RequestPermissionResult>(value) {
        Ok(RequestPermissionResult {
            outcome: PermissionOutcome::Selected { option_id },
        }) if option_id == permission::ALLOW_ONCE => Permission::AllowOnce,
        _ => Permission::Deny,
    }
}
