// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use arnes_core::{
    EditTextFileProposal, EventKind, Frontend, FrontendCapabilities, Media, Permission,
    PermissionRequest, ToolCallOutcome, ToolResultContent,
};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    host::{PendingRequestGuard, PendingRequests},
    types::{
        jsonrpc::{Notification, OutboundRequest},
        permission::{
            self, PermissionOutcome, PermissionToolCall, RequestPermissionParams,
            RequestPermissionResult,
        },
        session::{MessageContent, SessionUpdate, SessionUpdateParams, ToolCallContent},
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
    async fn on_event(&self, event: EventKind) {
        match event {
            EventKind::TurnStart => {
                *self
                    .current_message_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(Uuid::now_v7().to_string());
            }
            EventKind::TextDelta { text } => {
                let message_id = self
                    .current_message_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
                    .unwrap_or_else(|| Uuid::now_v7().to_string());
                self.send(SessionUpdate::AgentMessageChunk {
                    message_id,
                    content: MessageContent::Text { text },
                });
            }
            EventKind::ThinkingDelta { text } => {
                let message_id = self
                    .current_message_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
                    .unwrap_or_else(|| Uuid::now_v7().to_string());
                self.send(SessionUpdate::AgentThoughtChunk {
                    message_id,
                    content: MessageContent::Text { text },
                });
            }
            EventKind::ToolCallStarted(start) => {
                self.send(SessionUpdate::ToolCall {
                    tool_call_id: start.tool_call.id.clone(),
                    title: Some(start.title.clone()),
                    kind: "tool_use",
                    status: "pending",
                    raw_input: Some(start.tool_call.args.clone()),
                });
                self.send(SessionUpdate::ToolCallUpdate {
                    tool_call_id: start.tool_call.id.clone(),
                    title: Some(start.title.clone()),
                    status: "in_progress",
                    content: None,
                });
            }
            EventKind::ToolCallFinished(finish) => {
                self.send(SessionUpdate::ToolCallUpdate {
                    tool_call_id: finish.tool_call.id.clone(),
                    status: "completed",
                    title: None,
                    content: Some(outcome_content(finish.outcome)),
                });
            }
            EventKind::TurnEnd { .. } => {
                *self
                    .current_message_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = None;
            }
            _ => {}
        }
    }

    async fn request_permission(&self, req: PermissionRequest) -> Permission {
        let request_id = Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(request_id.clone(), tx);
        let _guard = PendingRequestGuard::new(Arc::clone(&self.pending), request_id.clone());

        // When the tool resolved a file edit, attach the before/after diff to
        // the tool-call card so the client renders it. ACP's permission request
        // references the card by id rather than carrying content itself; the
        // card already exists because `tool_call` is emitted before authorize.
        if let Some(edit) = EditTextFileProposal::from_proposal(&req.proposal) {
            self.send(SessionUpdate::ToolCallUpdate {
                tool_call_id: req.tool_call.id.clone(),
                title: None,
                status: "in_progress",
                content: Some(vec![ToolCallContent::Diff {
                    path: edit.path.display().to_string(),
                    old_text: Some(edit.old_content),
                    new_text: edit.new_content,
                }]),
            });
        }

        // Reuse the tool-call id from authorize so the prompt correlates with
        // the tool_call card we echo in session/update. Clients (e.g. Zed)
        // require a title on the permission tool call; the tool name serves.
        let params = RequestPermissionParams {
            session_id: self.session_id.clone(),
            tool_call: PermissionToolCall {
                tool_call_id: req.tool_call.id.clone(),
                title: Some(req.tool_call.name.clone()),
                raw_input: req.tool_call.args.clone(),
                kind: permission::acp_kind(req.kind),
            },
            options: permission::default_options(),
        };
        let outbound = OutboundRequest::new(request_id, "session/request_permission", params);
        let Ok(serialized) = serde_json::to_string(&outbound) else {
            return Permission::Deny;
        };
        if self.notify_tx.send(serialized).is_err() {
            return Permission::Deny;
        }

        // No timeout: the user may take a while to answer. The session races
        // tool execution against turn cancellation, so a stuck prompt is
        // dropped when the turn is cancelled; `_guard` then removes the
        // pending entry. A closed channel (client gone) or any non-allow
        // outcome denies.
        match rx.await {
            Ok(Ok(value)) => permission_from_value(value),
            _ => Permission::Deny,
        }
    }

    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities::default()
    }
}

/// Maps a finished tool call to the content of its ACP tool-call card: one
/// block per result block on success, a single text block otherwise.
fn outcome_content(outcome: ToolCallOutcome) -> Vec<ToolCallContent> {
    let contents = match outcome {
        ToolCallOutcome::Ok(blocks) => blocks.into_iter().map(block_content).collect(),
        ToolCallOutcome::Err(msg) => vec![MessageContent::Text { text: msg }],
        ToolCallOutcome::Denied => vec![MessageContent::Text {
            text: "denied".to_string(),
        }],
        ToolCallOutcome::Unknown => vec![MessageContent::Text {
            text: "unknown".to_string(),
        }],
    };
    contents
        .into_iter()
        .map(|content| ToolCallContent::Content { content })
        .collect()
}

fn block_content(block: ToolResultContent) -> MessageContent {
    match block {
        ToolResultContent::Json(v) => MessageContent::Text {
            text: json_text(&v),
        },
        ToolResultContent::Media(Media::Inline { mime_type, data })
            if mime_type.starts_with("image/") =>
        {
            MessageContent::Image { data, mime_type }
        }
        // ACP image blocks only carry images; other media still gets a card entry.
        ToolResultContent::Media(Media::Inline { mime_type, .. }) => MessageContent::Text {
            text: format!("unsupported media: {mime_type}"),
        },
        // `Media` is non-exhaustive.
        ToolResultContent::Media(_) => MessageContent::Text {
            text: "unsupported media".to_string(),
        },
    }
}

/// Shows a tool's JSON output as text: its `error` or `content` string field
/// when present, the raw JSON otherwise.
fn json_text(v: &Value) -> String {
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        format!("error: {err}")
    } else if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
        content.to_string()
    } else {
        v.to_string()
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

#[cfg(test)]
#[path = "frontend_tests.rs"]
mod tests;
