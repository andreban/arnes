// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

mod error;
mod frontend;
mod host;
mod identity;
mod message;
mod metering;
mod tool_context;

pub use error::{CoreError, Result};
pub use frontend::{
    EventKind, Frontend, FrontendCapabilities, Permission, PermissionRequest, SessionEvent,
    StopReason,
};
pub use host::{DirEntry, Host, LocalHost, TerminalHandle, TerminalSnapshot, TerminalSpec};
pub use identity::{AgentId, ModelKey};
pub use message::{ContentBlock, Message};
pub use metering::{CumulativeUsage, ModelUsage, TokenCounts, TurnUsage};
pub use tool_context::{ProgressSink, ProgressUpdate, ToolContext};
