// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use arnes_core::{Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent};
use async_trait::async_trait;

pub struct RecordingFrontend {
    events: Mutex<Vec<SessionEvent>>,
}

impl RecordingFrontend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
        })
    }

    pub fn events(&self) -> Vec<SessionEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl Frontend for RecordingFrontend {
    async fn on_event(&self, event: SessionEvent) {
        self.events.lock().unwrap().push(event);
    }

    async fn request_permission(&self, _req: PermissionRequest) -> Permission {
        Permission::AllowOnce
    }

    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities::default()
    }
}
