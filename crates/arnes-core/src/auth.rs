// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use agent_rig::auth::AuthManager as RigAuthManager;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use crate::{
    Frontend,
    Permission::{AllowOnce, Deny},
    PermissionRequest, ToolKind,
};

/// What the permission layer needs to enrich a prompt for one tool: its
/// semantic kind.
pub struct ToolPermissionMeta {
    pub kind: ToolKind,
}

pub struct AuthManager<F: Frontend> {
    frontend: Arc<F>,
    /// Per-tool enrichment metadata, keyed by tool name.
    metadata: HashMap<String, ToolPermissionMeta>,
}

impl<F: Frontend> AuthManager<F> {
    pub fn new(frontend: Arc<F>, metadata: HashMap<String, ToolPermissionMeta>) -> Self {
        Self { frontend, metadata }
    }

    fn kind(&self, name: &str) -> ToolKind {
        self.metadata
            .get(name)
            .map(|meta| meta.kind)
            .unwrap_or_default()
    }
}

#[async_trait]
impl<F: Frontend> RigAuthManager for AuthManager<F> {
    fn requires_authorization(&self, name: &str, args: &Value) -> bool {
        debug!(name, ?args, "AuthManager::requires_authorization");
        true
    }

    async fn authorize(&self, id: &str, name: &str, args: &Value) -> bool {
        debug!(name, ?args, "AuthManager::authorize");
        let permission_request = PermissionRequest {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            args: args.clone(),
            kind: self.kind(name),
        };
        match self.frontend.request_permission(permission_request).await {
            AllowOnce => true,
            Deny => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::{FrontendCapabilities, Permission, SessionEvent};

    /// Captures the `PermissionRequest` the auth manager hands the frontend so a
    /// test can assert how the call was enriched. Always allows.
    struct CapturingFrontend {
        last: Mutex<Option<PermissionRequest>>,
    }

    #[async_trait]
    impl Frontend for CapturingFrontend {
        async fn on_event(&self, _event: SessionEvent) {}

        async fn request_permission(&self, req: PermissionRequest) -> Permission {
            *self.last.lock().unwrap() = Some(req);
            Permission::AllowOnce
        }

        fn capabilities(&self) -> FrontendCapabilities {
            FrontendCapabilities::default()
        }
    }

    fn manager() -> (AuthManager<CapturingFrontend>, Arc<CapturingFrontend>) {
        let frontend = Arc::new(CapturingFrontend {
            last: Mutex::new(None),
        });
        let mut metadata = HashMap::new();
        metadata.insert(
            "read_text_file".to_string(),
            ToolPermissionMeta {
                kind: ToolKind::Read,
            },
        );
        (AuthManager::new(frontend.clone(), metadata), frontend)
    }

    #[tokio::test]
    async fn enriches_known_tool_with_kind() {
        let (auth, frontend) = manager();
        auth.authorize("call-1", "read_text_file", &json!({ "path": "Cargo.toml" }))
            .await;

        let req = frontend.last.lock().unwrap().take().unwrap();
        assert_eq!(req.kind, ToolKind::Read);
    }

    #[tokio::test]
    async fn unknown_tool_falls_back_to_default_kind() {
        let (auth, frontend) = manager();
        auth.authorize("call-1", "mystery_tool", &json!({})).await;

        let req = frontend.last.lock().unwrap().take().unwrap();
        assert_eq!(req.kind, ToolKind::Other);
    }
}
