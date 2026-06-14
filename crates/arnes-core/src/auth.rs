// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use agent_rig::auth::AuthManager as RigAuthManager;
use async_trait::async_trait;
use serde_json::Value;

#[derive(Default)]
pub struct AuthManager {}

#[async_trait]
impl RigAuthManager for AuthManager {
    fn requires_authorization(&self, _name: &str, _args: &Value) -> bool {
        true
    }
}
