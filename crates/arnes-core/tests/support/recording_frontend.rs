// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use arnes_core::{EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest};
use async_trait::async_trait;

pub struct RecordingFrontend {
    events: Mutex<Vec<EventKind>>,
    verdict: Permission,
}

impl RecordingFrontend {
    /// A frontend that allows every permission request.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            verdict: Permission::AllowOnce,
        })
    }

    /// A frontend that denies every permission request.
    // Each test binary compiles `support` separately; not all of them deny.
    #[allow(dead_code, reason = "unused in test binaries that never deny")]
    pub fn denying() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            verdict: Permission::Deny,
        })
    }

    pub fn events(&self) -> Vec<EventKind> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl Frontend for RecordingFrontend {
    async fn on_event(&self, event: EventKind) {
        self.events.lock().unwrap().push(event);
    }

    async fn request_permission(&self, _req: PermissionRequest) -> Permission {
        self.verdict.clone()
    }

    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities::default()
    }
}
