// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use arnes_core::{Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent};
use async_trait::async_trait;
use tokio::sync::mpsc;

pub enum UiCommand {
    Event(SessionEvent),
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
    async fn on_event(&self, event: SessionEvent) {
        let _ = self.tx.send(UiCommand::Event(event));
    }

    async fn request_permission(&self, _req: PermissionRequest) -> Permission {
        Permission::AllowOnce
    }

    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities::default()
    }
}
