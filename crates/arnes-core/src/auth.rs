// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, path::PathBuf, sync::Arc};

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
/// semantic kind and the argument keys that name filesystem paths.
pub struct ToolPermissionMeta {
    pub kind: ToolKind,
    pub location_keys: &'static [&'static str],
}

pub struct AuthManager<F: Frontend> {
    frontend: Arc<F>,
    /// Per-tool enrichment metadata, keyed by tool name.
    metadata: HashMap<String, ToolPermissionMeta>,
    /// Session working directory, used to make relative location paths absolute.
    cwd: PathBuf,
}

impl<F: Frontend> AuthManager<F> {
    pub fn new(
        frontend: Arc<F>,
        metadata: HashMap<String, ToolPermissionMeta>,
        cwd: PathBuf,
    ) -> Self {
        Self {
            frontend,
            metadata,
            cwd,
        }
    }

    /// Pulls the filesystem paths a call touches out of its args, resolving
    /// relative paths against the session cwd so the frontend gets absolute
    /// paths it can show and follow.
    fn paths(&self, name: &str, args: &Value) -> Vec<PathBuf> {
        let Some(meta) = self.metadata.get(name) else {
            return Vec::new();
        };
        meta.location_keys
            .iter()
            .filter_map(|key| args.get(*key).and_then(Value::as_str))
            .map(|path| {
                let path = PathBuf::from(path);
                if path.is_relative() {
                    self.cwd.join(path)
                } else {
                    path
                }
            })
            .collect()
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
            paths: self.paths(name, args),
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

    fn manager(cwd: PathBuf) -> (AuthManager<CapturingFrontend>, Arc<CapturingFrontend>) {
        let frontend = Arc::new(CapturingFrontend {
            last: Mutex::new(None),
        });
        let mut metadata = HashMap::new();
        metadata.insert(
            "read_text_file".to_string(),
            ToolPermissionMeta {
                kind: ToolKind::Read,
                location_keys: &["path"],
            },
        );
        (AuthManager::new(frontend.clone(), metadata, cwd), frontend)
    }

    #[tokio::test]
    async fn enriches_known_tool_and_resolves_relative_path() {
        let cwd = PathBuf::from("/work");
        let (auth, frontend) = manager(cwd.clone());
        auth.authorize("call-1", "read_text_file", &json!({ "path": "Cargo.toml" }))
            .await;

        let req = frontend.last.lock().unwrap().take().unwrap();
        assert_eq!(req.kind, ToolKind::Read);
        assert_eq!(req.paths, vec![cwd.join("Cargo.toml")]);
    }

    #[tokio::test]
    async fn leaves_absolute_path_untouched() {
        let cwd = std::env::temp_dir();
        let absolute = cwd.join("notes.txt");
        let (auth, frontend) = manager(cwd);
        auth.authorize(
            "call-1",
            "read_text_file",
            &json!({ "path": absolute.to_str().unwrap() }),
        )
        .await;

        let req = frontend.last.lock().unwrap().take().unwrap();
        assert_eq!(req.paths, vec![absolute]);
    }

    #[tokio::test]
    async fn unknown_tool_gets_default_kind_and_no_paths() {
        let (auth, frontend) = manager(PathBuf::from("/work"));
        auth.authorize("call-1", "mystery_tool", &json!({ "path": "x" }))
            .await;

        let req = frontend.last.lock().unwrap().take().unwrap();
        assert_eq!(req.kind, ToolKind::Other);
        assert!(req.paths.is_empty());
    }
}
