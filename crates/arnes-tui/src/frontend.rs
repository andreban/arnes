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
        debug!(?event, "TuiFrontend::on_event");
        let _ = self.tx.send(UiCommand::Event(event));
    }

    async fn request_permission(&self, req: PermissionRequest) -> Permission {
        debug!(?req, "TuiFrontend::request_permission");
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
