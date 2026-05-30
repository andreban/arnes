// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

//! Frontend trait and the event vocabulary it consumes.

use async_trait::async_trait;

use crate::AgentId;

/// Implemented by every UI adapter.
#[async_trait]
pub trait Frontend: Send + Sync {
    async fn on_event(&self, event: SessionEvent);
    async fn request_permission(&self, req: PermissionRequest) -> Permission;
    fn capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities
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
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub enum StopReason {
    EndTurn,
    Cancelled,
}

/// Permission-prompt payload. Filled in when the permission system lands.
#[derive(Clone, Debug, Default)]
pub struct PermissionRequest;

/// Frontend's verdict on a permission request.
#[derive(Clone, Debug)]
pub enum Permission {
    AllowOnce,
    Deny,
}

/// What the frontend supports. Bool fields added per capability when
/// there's a real caller branching on it.
#[derive(Clone, Debug, Default)]
pub struct FrontendCapabilities;
