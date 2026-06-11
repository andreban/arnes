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
        title: String,
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

/// Semantic category of a tool call, so a frontend can label it and pick an
/// icon. `Other` is the catch-all default for tools that don't declare a more
/// specific kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    #[default]
    Other,
}

/// Describes the gated tool call awaiting the user's approval.
#[derive(Clone, Debug, Default)]
pub struct PermissionRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: Value,
    /// Semantic category of the tool, for labelling the prompt.
    pub kind: ToolKind,
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
