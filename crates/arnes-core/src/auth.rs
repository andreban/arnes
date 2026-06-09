// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use agent_rig::auth::AuthManager as RigAuthManager;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use crate::{
    Frontend,
    Permission::{AllowOnce, Deny},
    PermissionRequest,
};

pub struct AuthManager<F: Frontend> {
    frontend: Arc<F>,
}

impl<F: Frontend> AuthManager<F> {
    pub fn new(frontend: Arc<F>) -> Self {
        Self { frontend }
    }
}

#[async_trait]
impl<F: Frontend> RigAuthManager for AuthManager<F> {
    fn requires_authorization(&self, name: &str, args: &Value) -> bool {
        debug!(name, ?args, "AuthManager::requires_authorization");
        true
    }

    async fn authorize(&self, name: &str, args: &Value) -> bool {
        debug!(name, ?args, "AuthManager::authorize");
        let permission_request = PermissionRequest {};
        match self.frontend.request_permission(permission_request).await {
            AllowOnce => true,
            Deny => false,
        }
    }
}
