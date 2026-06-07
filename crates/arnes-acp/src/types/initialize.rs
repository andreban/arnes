// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct InitializeParams {
    pub protocol_version: u32,
    pub client_capabilities: ClientCapabilities,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: u32,
    pub agent_info: AgentInfo,
    pub agent_capabilities: AgentCapabilities,
    pub auth_methods: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub struct AgentInfo {
    pub name: &'static str,
    pub title: &'static str,
    pub version: &'static str,
}

/// See <https://agentclientprotocol.com/protocol/v1/initialization#agent-capabilities>
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub load_session: bool,
    pub prompt_capabilities: PromptCapabilities,
}

/// See <https://agentclientprotocol.com/protocol/v1/initialization#prompt-capabilities>
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilities {
    pub image: bool,
    pub audio: bool,
    pub embedded_context: bool,
}

/// See <https://agentclientprotocol.com/protocol/v1/initialization#client-capabilities>
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ClientCapabilities {
    pub fs: FsCapabilities,
    pub terminal: bool,
}

/// See <https://agentclientprotocol.com/protocol/v1/initialization#file-system>
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct FsCapabilities {
    pub read_text_file: bool,
    pub write_text_file: bool,
}
