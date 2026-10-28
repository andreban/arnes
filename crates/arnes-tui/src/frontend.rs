// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use arnes_core::{
    EventKind, FilesystemCapabilities, Frontend, FrontendCapabilities, Permission,
    PermissionRequest,
};
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

pub enum UiCommand {
    Event(EventKind),
    /// A tool is asking to run; the UI answers through `responder`.
    PermissionRequest {
        request: PermissionRequest,
        responder: oneshot::Sender<Permission>,
    },
}

pub struct TuiFrontend {
    tx: mpsc::UnboundedSender<UiCommand>,
}

impl TuiFrontend {
    pub fn new(tx: mpsc::UnboundedSender<UiCommand>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl Frontend for TuiFrontend {
    async fn on_event(&self, event: EventKind) {
        log_event(&event);
        let _ = self.tx.send(UiCommand::Event(event));
    }

    async fn request_permission(&self, req: PermissionRequest) -> Permission {
        debug!(tool = %req.tool_call.name, id = %req.tool_call.id, "TuiFrontend::request_permission");
        let (responder, response) = oneshot::channel();
        if self
            .tx
            .send(UiCommand::PermissionRequest {
                request: req,
                responder,
            })
            .is_err()
        {
            // UI is gone; deny rather than block forever.
            return Permission::Deny;
        }
        // Deny if the UI drops the responder without answering.
        response.await.unwrap_or(Permission::Deny)
    }

    fn capabilities(&self) -> FrontendCapabilities {
        debug!("TuiFrontend::capabilities");
        FrontendCapabilities {
            fs: FilesystemCapabilities {
                read_file: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Logs `event` without tool arguments, titles or output: those can carry
/// secrets, such as a token in a bash command line, into the on-disk log.
fn log_event(event: &EventKind) {
    let (stage, tool_call) = match event {
        EventKind::ToolCallStarted(start) => ("started", &start.tool_call),
        EventKind::ToolCallUpdated(update) => ("updated", &update.tool_call),
        EventKind::ToolCallFinished(finish) => ("finished", &finish.tool_call),
        other => {
            debug!(event = ?other, "TuiFrontend::on_event");
            return;
        }
    };
    debug!(tool = %tool_call.name, id = %tool_call.id, stage, "TuiFrontend::on_event: tool call");
}
