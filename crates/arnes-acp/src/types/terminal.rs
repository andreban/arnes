// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Wire types for the ACP v1 `terminal/*` client methods.
//!
//! See <https://agentclientprotocol.com/protocol/v1/terminals>.

use serde::{Deserialize, Serialize};

/// Params for the `terminal/create` request sent to the ACP client.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateParams {
    pub session_id: String,
    pub command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVariable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_byte_limit: Option<u64>,
}

/// One environment variable in [`TerminalCreateParams::env`].
#[derive(Debug, Serialize)]
pub struct EnvVariable {
    pub name: String,
    pub value: String,
}

/// Result of `terminal/create`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateResult {
    pub terminal_id: String,
}

/// Params shared by `terminal/output`, `terminal/wait_for_exit`,
/// `terminal/kill`, and `terminal/release`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRefParams {
    pub session_id: String,
    pub terminal_id: String,
}

/// Result of `terminal/output`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputResult {
    pub output: String,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub exit_status: Option<TerminalExitStatus>,
}

/// How a terminal's process ended. Also the result of
/// `terminal/wait_for_exit`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalExitStatus {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
}
