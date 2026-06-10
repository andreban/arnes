// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Frontend trait and the event vocabulary it consumes.

use async_trait::async_trait;
use serde_json::Value;

use crate::{AgentId, TurnUsage};

/// Implemented by every UI adapter.
#[async_trait]
pub trait Frontend: Send + Sync + 'static {
    async fn on_event(&self, event: SessionEvent);
    async fn request_permission(&self, req: PermissionRequest) -> Permission;
    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities {
            ..Default::default()
        }
    }
}

/// One event from the session loop.
#[derive(Clone, Debug)]
pub struct SessionEvent {
    pub agent_id: AgentId,
    pub depth: u8,
    pub kind: EventKind,
}

#[derive(Clone, Debug)]
pub enum EventKind {
    TurnStart,
    TextDelta {
        text: String,
    },
    /// Model reasoning, distinct from response text.
    ThinkingDelta {
        text: String,
    },
    TurnEnd {
        stop_reason: StopReason,
        usage: TurnUsage,
    },
    Error {
        message: String,
    },
    ToolCallStarted {
        id: String,
        name: String,
        args: Value,
    },
    ToolCallFinished {
        id: String,
        name: String,
        outcome: ToolCallOutcome,
    },
}

#[derive(Clone, Debug)]
pub enum ToolCallOutcome {
    Ok(Value),
    Err(String),
    Denied,
    Unknown,
}

#[derive(Clone, Debug)]
pub enum StopReason {
    EndTurn,
    Cancelled,
}

/// Describes the gated tool call awaiting the user's approval.
#[derive(Clone, Debug, Default)]
pub struct PermissionRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: Value,
}

/// Frontend's verdict on a permission request.
#[derive(Clone, Debug)]
pub enum Permission {
    AllowOnce,
    Deny,
}

/// Frontend features the agent can use.
///
/// Defaults to nothing supported; set a field to `true` only when a
/// real caller branches on it. New capabilities are added the same
/// way — opt-in, never speculative.
#[derive(Clone, Debug, Default)]
pub struct FrontendCapabilities {
    pub fs: FilesystemCapabilities,
    pub terminal: bool,
}

/// Filesystem operations the frontend exposes to the agent.
#[derive(Clone, Debug, Default)]
pub struct FilesystemCapabilities {
    pub read_file: bool,
    pub write_file: bool,
}
